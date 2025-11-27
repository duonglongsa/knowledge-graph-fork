//! Rule loading and management system for ast-grep
//!
//! This module provides a high-level interface for loading and executing ast-grep rules
//! against source code. It bridges the gap between raw ast-grep functionality and
//! application-specific needs by:
//!
//! - Managing rule collections for different programming languages
//! - Providing structured access to match results and captured variables
//! - Offering both high-level (serializable) and low-level (raw AST node) APIs
//! - Supporting advanced pattern matching with metavariables ($A, $$$B, etc.)

use crate::{
    Result,
    parser::SupportedLanguage,
    utils::{Position, Range},
};
use ast_grep_config::{CombinedScan, RuleConfig};
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_core::{
    AstGrep, Node,
    meta_var::{MetaVarEnv, MetaVariable},
};
use ast_grep_language::SupportLang;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use serde::{Deserialize, Serialize};
use tracing::debug;

/// Type alias for variables mapping
type MetaVarMap = FxHashMap<String, MetaVarNode>;

/// Type alias for single captured AST nodes mapping  
type SingleNodeMap<'t> = FxHashMap<String, Node<'t, StrDoc<SupportLang>>>;

/// Type alias for multi-captured AST nodes mapping
type MultiNodeMap<'t> = FxHashMap<String, Vec<Node<'t, StrDoc<SupportLang>>>>;

/// Small vector for variables
type SmallMetaVarVec = SmallVec<[String; 4]>;

/// Type of metavariable capture used in ast-grep patterns
///
/// Metavariables allow patterns to capture and reuse parts of the matched code:
/// - `$A` captures a single AST node (like a variable name or expression)
/// - `$$$A` captures multiple consecutive nodes (like function parameters)
/// - Transformed variables result from applying transformations to captured content
///
/// See:
/// https://ast-grep.github.io/guide/pattern-syntax.html#meta-variable-capturing
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureType {
    /// Single capture like `$A` - captures one AST node
    /// Example: In pattern `function $NAME() {}`, $NAME captures the function name
    Single,

    /// Multi capture like `$$$A` - captures sequences of nodes
    /// Example: In pattern `foo($$$ARGS)`, $ARGS captures all function arguments
    Multi,

    /// Transformed variable - result of applying transformations to other captures
    /// Example: Converting captured text to uppercase or applying other modifications
    Transformed,
}

/// Information about a single AST node captured during pattern matching
///
/// This provides both the textual content and precise location information
/// for each captured variable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaVarNode {
    /// The actual text content of the captured node
    /// Example: For `$A` matching `userName`, text would be "userName"
    pub text: String,

    /// Position range (start_line, start_col, end_line, end_col) - 0-indexed
    /// Useful for generating precise error messages or IDE integrations
    pub range: (usize, usize, usize, usize),

    /// Byte offset range within the source file
    /// More efficient for programmatic text manipulation than line/column
    pub byte_offset: (usize, usize),

    /// How this variable was captured (single node, multiple nodes, or transformed)
    pub capture_type: CaptureType,
}

/// Complete information about a pattern match found by a rule
///
/// This is the data structure returned when rules successfully match code.
/// It contains everything needed to understand what was found and where, plus
/// all the captured variables that can be used for analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchInfo {
    /// ID of the rule that produced this match (for filtering/grouping results)
    pub rule_id: Box<str>,

    /// Location of the entire match in the source code
    pub range: Range,

    /// The complete text that was matched by the pattern
    /// Example: For pattern `class $NAME {}`, might be "class MyClass {}"
    pub matched: String,

    /// Source file path where this match was found (if available)
    pub file: Option<Box<str>>,

    /// Programming language that was parsed to find this match
    pub language: Option<Box<str>>,

    /// Dictionary of all variables captured by the pattern
    /// Key: variable name (e.g., "NAME"), Value: details about what was captured
    pub meta_var_map: MetaVarMap,
}

/// Raw AST node information alongside MatchInfo for advanced use cases
///
/// While MatchInfo provides serializable, language-agnostic information,
/// MatchWithNodes gives direct access to the underlying tree-sitter AST nodes.
/// This is useful for:
/// - Advanced analysis requiring tree-sitter's full API
/// - Performance-critical code that needs to avoid text conversion
/// - Complex transformations that need to understand AST structure
pub struct MatchWithNodes<'t> {
    /// The high-level, serializable match information
    pub match_info: MatchInfo,

    /// Direct access to the tree-sitter AST node for the main match
    /// Provides full tree-sitter functionality (traversal, syntax info, etc.)
    pub raw_node: Node<'t, StrDoc<SupportLang>>,

    /// Raw AST nodes for single-captured variables ($A, $B, etc.)
    /// Key: variable name, Value: the actual AST node that was captured
    pub env_nodes: SingleNodeMap<'t>,

    /// Raw AST nodes for multi-captured variables ($$$A, $$$B, etc.)
    /// Key: variable name, Value: sequence of AST nodes that were captured
    pub multi_env_nodes: MultiNodeMap<'t>,
}

impl MatchInfo {
    /// Get details about a specific captured variable
    /// Returns None if the variable wasn't captured in this match
    pub fn get_env(&self, name: &str) -> Option<&MetaVarNode> {
        self.meta_var_map.get(name)
    }

    /// Check if a specific variable was captured in this match
    /// Useful for conditional logic based on what the pattern captured
    pub fn has_env(&self, name: &str) -> bool {
        self.meta_var_map.contains_key(name)
    }

    /// Get names of all variables captured in this match
    /// Useful for debugging or dynamic processing of captured variables
    pub fn env_names(&self) -> SmallMetaVarVec {
        self.meta_var_map.keys().cloned().collect()
    }

    /// Calculate how many lines this match spans
    /// Useful for metrics or filtering matches by size
    pub fn line_span(&self) -> usize {
        self.range.line_span()
    }

    /// Calculate the byte length of the matched text
    /// Useful for performance analysis or memory allocation
    pub fn byte_length(&self) -> usize {
        self.range.byte_length()
    }
}

/// Combined rule manager that handles rule loading and execution for a specific language
///
/// This is the main entry point for using ast-grep rules. It:
/// - Loads all available rules for a programming language
/// - Pre-compiles them into an combined scanner for reuse
/// - Provides methods to run rules against source code and get structured results
///
/// The manager is designed to be created once per language and reused across
/// multiple files/code snippets.
pub struct RuleManager {
    /// The programming language this manager handles
    pub language: SupportedLanguage,

    /// Pre-compiled rule scanner for pattern matching
    /// This avoids re-compiling rules on every execution
    combined_scan: CombinedScan<'static, SupportLang>,

    /// Reference to the loaded rules (useful for debugging and introspection)
    rules: Vec<&'static RuleConfig<SupportLang>>,
}

impl RuleManager {
    /// Create a new rule manager for a specific programming language
    ///
    /// This loads all available rules for the language and compiles them into
    /// an efficient scanner. If no rules are available for the language,
    /// the manager will return empty results when executing rules.
    ///
    /// ## Arguments
    /// * `language` - The programming language to load rules for
    ///
    /// ## Behavior
    /// When no rules are configured for a language, a warning is logged and
    /// the manager gracefully handles empty rule sets by returning empty results.
    pub fn new(language: SupportedLanguage) -> Self {
        let rules = Self::load_rules_for_language(language);
        if rules.is_empty() {
            debug!(
                "No rules configured for language: {language:?}. Rule execution will return empty results."
            );
        }

        // Pre-compile rules into a combined scanner for efficiency
        // CombinedScan can handle empty rule sets gracefully
        let combined_scan = CombinedScan::new(rules.clone());

        Self {
            language,
            combined_scan,
            rules,
        }
    }

    /// Load all rules defined for a specific programming language
    ///
    /// Rules are loaded from language-specific modules that define patterns
    /// for common code structures (classes, methods, etc.).
    fn load_rules_for_language(
        language: SupportedLanguage,
    ) -> Vec<&'static RuleConfig<SupportLang>> {
        match language {
            SupportedLanguage::Ruby => {
                // Ruby definitions are now extracted directly during FQN traversal, no YAML rules needed
                vec![]
            }
            SupportedLanguage::CSharp => {
                vec![]
            }
            SupportedLanguage::Python => {
                // Load Python-specific rules (class definitions, function definitions, etc.)
                let python_rules = &*crate::python::python_ast::RULES_CONFIG;
                python_rules.iter().collect()
            }
            SupportedLanguage::TypeScript => {
                vec![]
            }
            SupportedLanguage::Kotlin => {
                vec![]
            }
            SupportedLanguage::Java => {
                vec![]
            }
            SupportedLanguage::Rust => {
                let rust_rules = &*crate::rust::rust_ast::RULES_CONFIG;
                rust_rules.iter().collect()
            }
        }
    }

    /// Get read-only access to all loaded rules
    /// Useful for debugging, rule introspection
    pub fn rules(&self) -> &[&'static RuleConfig<SupportLang>] {
        &self.rules
    }

    /// Execute rules and return both high-level info and raw AST nodes
    ///
    /// This provides the same functionality as execute_rules() but also returns
    /// direct access to the underlying tree-sitter AST nodes. Use this when you need:
    /// - Fine-grained control over AST traversal
    /// - Access to tree-sitter's advanced features
    /// - Maximum performance for complex analysis
    ///
    /// # Arguments
    /// * `ast` - The parsed AST to search within  
    /// * `file_path` - Optional file path for context in results
    ///
    /// # Returns
    /// Vector of MatchWithNodes structs with both high-level and low-level data
    pub fn execute_rules<'t>(
        &self,
        ast: &'t AstGrep<StrDoc<SupportLang>>,
        file_path: Option<&str>,
    ) -> Result<Vec<MatchWithNodes<'t>>> {
        // Execute the pre-compiled scanner against the AST
        let scan_result = self.combined_scan.scan(ast, false);

        // Pre-allocate with reasonable capacity based on rules
        let estimated_capacity = self.rules.len();
        let mut results = Vec::with_capacity(estimated_capacity);

        // Process each rule's matches
        for (rule, matches) in scan_result.matches {
            for m in matches {
                // Extract position and range information
                let range = m.range();
                let start_pos = m.start_pos();
                let end_pos = m.end_pos();
                let node = m.get_node();

                // Convert ast-grep's variables to our structured format
                let (env, env_nodes, multi_env_nodes) = env_to_map_with_nodes(m.get_env());

                // Build high-level match information
                let match_info = MatchInfo {
                    rule_id: rule.id.clone().into_boxed_str(),
                    range: Range::new(
                        Position::new(start_pos.line(), start_pos.column(node)),
                        Position::new(end_pos.line(), end_pos.column(node)),
                        (range.start, range.end),
                    ),
                    matched: m.text().to_string(),
                    file: file_path.map(|s| s.to_string().into_boxed_str()),
                    language: Some(format!("{:?}", m.lang()).into_boxed_str()),
                    meta_var_map: env,
                };

                // Combine high-level and low-level data
                results.push(MatchWithNodes {
                    match_info,
                    raw_node: node.clone(),
                    env_nodes,
                    multi_env_nodes,
                });
            }
        }

        Ok(results)
    }
}

/// Convenience function to execute rules against an AST
pub fn run_rules<'t>(
    ast: &'t AstGrep<StrDoc<SupportLang>>,
    file_path: Option<&str>,
    manager: &RuleManager,
) -> Vec<MatchWithNodes<'t>> {
    manager.execute_rules(ast, file_path).unwrap_or_else(|e| {
        tracing::error!("Failed to execute rules: {}", e);
        Vec::new()
    })
}

/// Convert ast-grep's internal MetaVarEnv to our structured format
///
/// This bridges the gap between ast-grep's internal representation and our
/// application-specific data structures. It extracts both high-level information
/// (text, positions) and provides access to raw AST nodes for advanced use cases.
///
/// Returns a tuple of:
/// 1. MetaVarMap - High-level variable information
/// 2. SingleNodeMap - Raw nodes for single captures  
/// 3. MultiNodeMap - Raw nodes for multi-captures
///
/// Note that this logic has been largely borrowed from
/// https://github.com/ast-grep/ast-grep.github.io/blob/main/src/utils.rs
fn env_to_map_with_nodes<'t>(
    env: &MetaVarEnv<'t, StrDoc<SupportLang>>,
) -> (MetaVarMap, SingleNodeMap<'t>, MultiNodeMap<'t>) {
    // Pre-allocate HashMaps with reasonable capacity
    let mut map = FxHashMap::with_capacity_and_hasher(8, Default::default());
    let mut node_map = FxHashMap::with_capacity_and_hasher(8, Default::default());
    let mut multi_node_map = FxHashMap::with_capacity_and_hasher(4, Default::default());

    // Process each variable captured by the pattern
    for id in env.get_matched_variables() {
        match id {
            // Handle single captures like $A, $B
            MetaVariable::Capture(name, _) => {
                if let Some(node) = env.get_match(&name) {
                    // Extract position and range information
                    let node_range = node.range();
                    let range = (
                        node.start_pos().line(),
                        node.start_pos().column(node),
                        node.end_pos().line(),
                        node.end_pos().column(node),
                    );

                    // Store high-level information
                    map.insert(
                        name.clone(),
                        MetaVarNode {
                            text: node.text().to_string(),
                            range,
                            byte_offset: (node_range.start, node_range.end),
                            capture_type: CaptureType::Single,
                        },
                    );

                    // Store raw AST node for advanced use cases
                    node_map.insert(name.clone(), node.clone());
                } else if let Some(bytes) = env.get_transformed(&name) {
                    // Handle transformed variables (result of applying transformations)
                    // Note: we currently don't use transformed variables
                    map.insert(
                        name.clone(),
                        MetaVarNode {
                            text: bytes.iter().map(|b| *b as char).collect(),
                            range: (0, 0, 0, 0), // Transformed variables don't have source positions
                            byte_offset: (0, 0),
                            capture_type: CaptureType::Transformed,
                        },
                    );
                    // No raw node available for transformed variables
                }
            }
            // Handle multi-captures like $$$A
            MetaVariable::MultiCapture(name) => {
                let nodes = env.get_multiple_matches(&name);
                let (Some(first), Some(last)) = (nodes.first(), nodes.last()) else {
                    continue;
                };

                // Calculate range spanning all captured nodes
                let start = first.start_pos();
                let end = last.end_pos();
                let start_byte = first.range().start;
                let end_byte = last.range().end;

                // Optimize string concatenation by pre-calculating total length
                let total_len: usize = nodes.iter().map(|n| n.text().len()).sum();
                let mut text = String::with_capacity(total_len);
                for node in &nodes {
                    text.push_str(&node.text());
                }

                map.insert(
                    name.clone(),
                    MetaVarNode {
                        text,
                        range: (
                            start.line(),
                            start.column(first),
                            end.line(),
                            end.column(last),
                        ),
                        byte_offset: (start_byte, end_byte),
                        capture_type: CaptureType::Multi,
                    },
                );

                // Store all raw nodes in the sequence
                multi_node_map.insert(name.clone(), nodes);
            }
            // Ignore anonymous variables (ones that don't capture anything)
            _ => continue,
        }
    }

    (map, node_map, multi_node_map)
}

/// Filter matches to only those produced by a specific rule
///
/// Useful when you're only interested in results from one particular rule,
/// or when conducting rule-specific analysis.
pub fn filter_matches_by_rule_id<'a, 't>(
    matches: &'a [MatchWithNodes<'t>],
    rule_id: &str,
) -> Vec<&'a MatchWithNodes<'t>> {
    matches
        .iter()
        .filter(|m| m.match_info.rule_id.as_ref() == rule_id)
        .collect()
}

/// Group matches by the rule that produced them
pub fn group_matches_by_rule_id<'a, 't>(
    matches: &'a [MatchWithNodes<'t>],
) -> FxHashMap<String, Vec<&'a MatchWithNodes<'t>>> {
    let mut grouped = FxHashMap::with_capacity_and_hasher(matches.len() / 2, Default::default());
    for m in matches {
        grouped
            .entry(m.match_info.rule_id.to_string())
            .or_insert_with(Vec::new)
            .push(m);
    }
    grouped
}

/// Extract all captured variables from a set of matches
pub fn extract_all_meta_vars(matches: &[MatchInfo]) -> Vec<(String, String)> {
    // Pre-allocate based on estimated variable count
    let estimated_vars = matches.len() * 2; // Estimate 2 variables per match
    let mut vars = Vec::with_capacity(estimated_vars);

    for m in matches {
        for (var_name, env_node) in &m.meta_var_map {
            vars.push((var_name.clone(), env_node.text.clone()));
        }
    }
    vars
}

#[cfg(test)]
mod tests {
    use super::*;
    use ast_grep_language::LanguageExt;

    #[test]
    fn test_env_node_creation() {
        let env_node = MetaVarNode {
            text: "test".to_string(),
            range: (1, 0, 1, 4),
            byte_offset: (0, 4),
            capture_type: CaptureType::Single,
        };
        assert_eq!(env_node.text, "test");
    }

    #[test]
    fn test_match_info_creation() {
        let match_info = MatchInfo {
            rule_id: "test-rule".to_string().into_boxed_str(),
            range: Range::new(Position::new(1, 0), Position::new(1, 4), (0, 4)),
            matched: "test".to_string(),
            file: None,
            language: None,
            meta_var_map: FxHashMap::default(),
        };

        assert_eq!(match_info.rule_id.as_ref(), "test-rule");
        assert_eq!(match_info.matched, "test");
        assert_eq!(match_info.line_span(), 1);
        assert_eq!(match_info.byte_length(), 4);
    }

    #[test]
    fn test_match_info_methods() {
        let mut env = FxHashMap::default();
        env.insert(
            "NAME".to_string(),
            MetaVarNode {
                text: "TestClass".to_string(),
                range: (0, 6, 0, 15),
                byte_offset: (6, 15),
                capture_type: CaptureType::Single,
            },
        );
        env.insert(
            "BODY".to_string(),
            MetaVarNode {
                text: "method body".to_string(),
                range: (1, 2, 3, 5),
                byte_offset: (20, 31),
                capture_type: CaptureType::Multi,
            },
        );

        let match_info = MatchInfo {
            rule_id: "test-rule".to_string().into_boxed_str(),
            range: Range::new(Position::new(0, 0), Position::new(4, 3), (0, 50)),
            matched: "class TestClass\n  method body\nend".to_string(),
            file: Some("test.rb".to_string().into_boxed_str()),
            language: Some("Ruby".to_string().into_boxed_str()),
            meta_var_map: env,
        };

        // Test get_env method
        assert!(match_info.get_env("NAME").is_some());
        assert_eq!(match_info.get_env("NAME").unwrap().text, "TestClass");
        assert!(match_info.get_env("NONEXISTENT").is_none());

        // Test has_env method
        assert!(match_info.has_env("NAME"));
        assert!(match_info.has_env("BODY"));
        assert!(!match_info.has_env("NONEXISTENT"));

        // Test env_names method
        let names = match_info.env_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"NAME".to_string()));
        assert!(names.contains(&"BODY".to_string()));

        // Test line_span and byte_length
        assert_eq!(match_info.line_span(), 5); // 0 to 4 inclusive
        assert_eq!(match_info.byte_length(), 50);
    }

    #[test]
    fn test_rule_manager_creation() {
        let manager = RuleManager::new(SupportedLanguage::Ruby);
        assert_eq!(manager.language, SupportedLanguage::Ruby);
        assert!(manager.rules().is_empty()); // Ruby no longer uses YAML rules
    }

    #[test]
    fn test_run_rules_function() {
        let manager = RuleManager::new(SupportedLanguage::Ruby);
        let ruby_code = "class TestClass\n  def test_method\n    puts 'hello'\n  end\nend";
        let ast = SupportedLanguage::Ruby
            .to_support_lang()
            .ast_grep(ruby_code);

        let matches = run_rules(&ast, Some("test.rb"), &manager);
        assert!(matches.is_empty()); // Ruby no longer uses YAML rules

        let matches_no_path = run_rules(&ast, None, &manager);
        assert!(matches_no_path.is_empty()); // Ruby no longer uses YAML rules

        // No matches to check since Ruby no longer uses YAML rules
        println!("Ruby definitions are extracted during FQN traversal, not through YAML rules");
    }

    #[test]
    fn test_filter_matches_by_rule_id() {
        let test_code = "class Test1\nend\nclass Test2\nend";
        let ast = SupportedLanguage::Ruby
            .to_support_lang()
            .ast_grep(test_code);
        let test_node1 = ast.root().child(0).unwrap();
        let test_node2 = ast.root().child(1).unwrap();

        let matches = vec![
            MatchWithNodes {
                match_info: MatchInfo {
                    rule_id: "rule1".to_string().into_boxed_str(),
                    range: Range::new(Position::new(1, 0), Position::new(1, 4), (0, 4)),
                    matched: "test1".to_string(),
                    file: None,
                    language: None,
                    meta_var_map: FxHashMap::default(),
                },
                raw_node: test_node1.clone(),
                env_nodes: FxHashMap::default(),
                multi_env_nodes: FxHashMap::default(),
            },
            MatchWithNodes {
                match_info: MatchInfo {
                    rule_id: "rule2".to_string().into_boxed_str(),
                    range: Range::new(Position::new(2, 0), Position::new(2, 4), (5, 9)),
                    matched: "test2".to_string(),
                    file: None,
                    language: None,
                    meta_var_map: FxHashMap::default(),
                },
                raw_node: test_node2.clone(),
                env_nodes: FxHashMap::default(),
                multi_env_nodes: FxHashMap::default(),
            },
        ];

        let filtered = filter_matches_by_rule_id(&matches, "rule1");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].match_info.rule_id.as_ref(), "rule1");

        let empty_filtered = filter_matches_by_rule_id(&matches, "non-existent");
        assert_eq!(empty_filtered.len(), 0);
    }

    #[test]
    fn test_group_matches_by_rule_id() {
        let test_code = "class Test1\nend\nclass Test2\nend";
        let ast = SupportedLanguage::Ruby
            .to_support_lang()
            .ast_grep(test_code);
        let test_node1 = ast.root().child(0).unwrap();
        let test_node2 = ast.root().child(1).unwrap();

        let matches = vec![
            MatchWithNodes {
                match_info: MatchInfo {
                    rule_id: "rule1".to_string().into_boxed_str(),
                    range: Range::new(Position::new(1, 0), Position::new(1, 4), (0, 4)),
                    matched: "test1".to_string(),
                    file: None,
                    language: None,
                    meta_var_map: FxHashMap::default(),
                },
                raw_node: test_node1.clone(),
                env_nodes: FxHashMap::default(),
                multi_env_nodes: FxHashMap::default(),
            },
            MatchWithNodes {
                match_info: MatchInfo {
                    rule_id: "rule1".to_string().into_boxed_str(), // Same rule ID
                    range: Range::new(Position::new(2, 0), Position::new(2, 4), (5, 9)),
                    matched: "test2".to_string(),
                    file: None,
                    language: None,
                    meta_var_map: FxHashMap::default(),
                },
                raw_node: test_node2.clone(),
                env_nodes: FxHashMap::default(),
                multi_env_nodes: FxHashMap::default(),
            },
            MatchWithNodes {
                match_info: MatchInfo {
                    rule_id: "rule2".to_string().into_boxed_str(), // Different rule ID
                    range: Range::new(Position::new(3, 0), Position::new(3, 4), (10, 14)),
                    matched: "test3".to_string(),
                    file: None,
                    language: None,
                    meta_var_map: FxHashMap::default(),
                },
                raw_node: test_node1.clone(),
                env_nodes: FxHashMap::default(),
                multi_env_nodes: FxHashMap::default(),
            },
        ];

        let grouped = group_matches_by_rule_id(&matches);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped.get("rule1").unwrap().len(), 2);
        assert_eq!(grouped.get("rule2").unwrap().len(), 1);

        let empty_grouped = group_matches_by_rule_id(&[]);
        assert!(empty_grouped.is_empty());
    }

    #[test]
    fn test_extract_all_meta_vars() {
        let mut env1 = FxHashMap::default();
        env1.insert(
            "NAME".to_string(),
            MetaVarNode {
                text: "TestClass".to_string(),
                range: (0, 6, 0, 15),
                byte_offset: (6, 15),
                capture_type: CaptureType::Single,
            },
        );

        let mut env2 = FxHashMap::default();
        env2.insert(
            "METHOD".to_string(),
            MetaVarNode {
                text: "test_method".to_string(),
                range: (1, 6, 1, 17),
                byte_offset: (20, 31),
                capture_type: CaptureType::Single,
            },
        );
        env2.insert(
            "BODY".to_string(),
            MetaVarNode {
                text: "puts 'hello'".to_string(),
                range: (2, 4, 2, 16),
                byte_offset: (35, 47),
                capture_type: CaptureType::Multi,
            },
        );

        let matches = vec![
            MatchInfo {
                rule_id: "rule1".to_string().into_boxed_str(),
                range: Range::new(Position::new(0, 0), Position::new(1, 3), (0, 20)),
                matched: "class TestClass".to_string(),
                file: None,
                language: None,
                meta_var_map: env1,
            },
            MatchInfo {
                rule_id: "rule2".to_string().into_boxed_str(),
                range: Range::new(Position::new(1, 0), Position::new(3, 5), (20, 50)),
                matched: "def test_method\n  puts 'hello'\nend".to_string(),
                file: None,
                language: None,
                meta_var_map: env2,
            },
        ];

        let vars = extract_all_meta_vars(&matches);
        assert_eq!(vars.len(), 3);

        // Check that all variables are present
        let var_names: Vec<_> = vars.iter().map(|(name, _)| name.as_str()).collect();
        assert!(var_names.contains(&"NAME"));
        assert!(var_names.contains(&"METHOD"));
        assert!(var_names.contains(&"BODY"));

        // Check specific values
        for (name, value) in &vars {
            match name.as_str() {
                "NAME" => assert_eq!(value, "TestClass"),
                "METHOD" => assert_eq!(value, "test_method"),
                "BODY" => assert_eq!(value, "puts 'hello'"),
                _ => panic!("Unexpected variable name: {name}"),
            }
        }

        // Test with empty matches
        let empty_vars = extract_all_meta_vars(&[]);
        assert!(empty_vars.is_empty());
    }

    #[test]
    fn test_ruby_rule_manager() {
        let manager = RuleManager::new(SupportedLanguage::Ruby);

        assert_eq!(manager.language, SupportedLanguage::Ruby);
        assert_eq!(manager.rules().len(), 0); // Ruby no longer uses YAML rules - definitions extracted during FQN traversal

        let ruby_code = "class Test\n  def hello\n    puts 'world'\n  end\nend";
        let ast = SupportedLanguage::Ruby
            .to_support_lang()
            .ast_grep(ruby_code);
        let matches = manager.execute_rules(&ast, Some("test.rb")).unwrap();

        assert!(matches.is_empty()); // No YAML rules means no matches from rule manager
        println!("Ruby uses direct AST traversal, no rule matches expected");

        // Test rule execution with raw nodes
        let matches_with_nodes = manager.execute_rules(&ast, Some("test.rb")).unwrap();
        assert!(matches_with_nodes.is_empty()); // No rules means no matches

        for match_with_nodes in &matches_with_nodes {
            let raw_node = &match_with_nodes.raw_node;
            println!("Raw node kind: {}", raw_node.kind());
            println!("Raw node text: {}", raw_node.text());

            // Test access to variables' raw nodes (single captures)
            for (var_name, env_node) in &match_with_nodes.env_nodes {
                println!(
                    "Single capture '{}' raw node kind: {}",
                    var_name,
                    env_node.kind()
                );
            }

            // Test access to multi-captured variables' raw nodes (sequences)
            for (var_name, env_nodes) in &match_with_nodes.multi_env_nodes {
                println!(
                    "Multi capture '{}' has {} nodes:",
                    var_name,
                    env_nodes.len()
                );
                for (i, node) in env_nodes.iter().enumerate() {
                    println!(
                        "  [{}] kind: {}, text: {}",
                        i,
                        node.kind(),
                        node.text().trim()
                    );
                }
            }

            // Test access to capture type information from match_info
            for (var_name, env_node) in &match_with_nodes.match_info.meta_var_map {
                println!(
                    "Variable '{}' capture type: {:?}",
                    var_name, env_node.capture_type
                );
            }
        }
    }

    #[test]
    fn test_ruby_rules_functionality() {
        let manager = RuleManager::new(SupportedLanguage::Ruby);

        let rule_ids: Vec<_> = manager.rules().iter().map(|r| &r.id).collect();
        assert!(rule_ids.is_empty()); // Ruby no longer uses YAML rules
    }

    #[test]
    fn test_capture_types() {
        let manager = RuleManager::new(SupportedLanguage::Ruby);

        let ruby_code = "class TestClass\n  def test_method\n    puts 'hello'\n  end\nend";
        let ast = SupportedLanguage::Ruby
            .to_support_lang()
            .ast_grep(ruby_code);

        let matches_with_nodes = manager.execute_rules(&ast, Some("test.rb")).unwrap();
        assert!(matches_with_nodes.is_empty()); // Ruby no longer uses YAML rules

        // The test would only be meaningful if we had rules with captures
        println!("Ruby uses direct AST traversal, no rule captures to test");
    }

    #[test]
    fn test_capture_type_variants() {
        let single = CaptureType::Single;
        let multi = CaptureType::Multi;
        let transformed = CaptureType::Transformed;

        assert_eq!(format!("{single:?}"), "Single");
        assert_eq!(format!("{multi:?}"), "Multi");
        assert_eq!(format!("{transformed:?}"), "Transformed");

        let single_clone = single.clone();
        assert_eq!(single, single_clone);
    }

    #[test]
    fn test_match_with_nodes_structure() {
        let test_code = "class Test\nend";
        let ast = SupportedLanguage::Ruby
            .to_support_lang()
            .ast_grep(test_code);
        let test_node = ast.root().child(0).unwrap(); // This will be the class node, not program

        let mut env = FxHashMap::default();
        env.insert(
            "NAME".to_string(),
            MetaVarNode {
                text: "Test".to_string(),
                range: (0, 6, 0, 10),
                byte_offset: (6, 10),
                capture_type: CaptureType::Single,
            },
        );

        let mut env_nodes = FxHashMap::default();
        env_nodes.insert("NAME".to_string(), test_node.clone());

        let match_with_nodes = MatchWithNodes {
            match_info: MatchInfo {
                rule_id: "test-rule".to_string().into_boxed_str(),
                range: Range::new(Position::new(0, 0), Position::new(1, 3), (0, 13)),
                matched: test_code.to_string(),
                file: Some("test.rb".to_string().into_boxed_str()),
                language: Some("Ruby".to_string().into_boxed_str()),
                meta_var_map: env,
            },
            raw_node: test_node.clone(),
            env_nodes,
            multi_env_nodes: FxHashMap::default(),
        };

        assert_eq!(match_with_nodes.match_info.rule_id.as_ref(), "test-rule");
        assert_eq!(match_with_nodes.raw_node.kind(), "class");
        assert!(match_with_nodes.env_nodes.contains_key("NAME"));
        assert!(match_with_nodes.multi_env_nodes.is_empty());
    }

    #[test]
    fn test_rule_execution_error_handling() {
        let manager = RuleManager::new(SupportedLanguage::Ruby);

        let empty_ast = SupportedLanguage::Ruby.to_support_lang().ast_grep("");
        let matches = manager.execute_rules(&empty_ast, None);
        assert!(matches.is_ok());

        let simple_ast = SupportedLanguage::Ruby
            .to_support_lang()
            .ast_grep("puts 'hello'");
        let matches = manager.execute_rules(&simple_ast, Some("simple.rb"));
        assert!(matches.is_ok());
    }

    #[test]
    fn test_env_node_all_fields() {
        let env_node = MetaVarNode {
            text: "example_text".to_string(),
            range: (1, 5, 2, 10),
            byte_offset: (15, 27),
            capture_type: CaptureType::Multi,
        };

        assert_eq!(env_node.text, "example_text");
        assert_eq!(env_node.range, (1, 5, 2, 10));
        assert_eq!(env_node.byte_offset, (15, 27));
        assert_eq!(env_node.capture_type, CaptureType::Multi);
    }

    #[test]
    fn test_match_info_with_captures() {
        let mut env = FxHashMap::default();
        env.insert(
            "CLASS_NAME".to_string(),
            MetaVarNode {
                text: "MyClass".to_string(),
                range: (0, 6, 0, 13),
                byte_offset: (6, 13),
                capture_type: CaptureType::Single,
            },
        );
        env.insert(
            "METHODS".to_string(),
            MetaVarNode {
                text: "def foo; end\ndef bar; end".to_string(),
                range: (1, 2, 3, 5),
                byte_offset: (16, 41),
                capture_type: CaptureType::Multi,
            },
        );

        let match_info = MatchInfo {
            rule_id: "class-with-methods".to_string().into_boxed_str(),
            range: Range::new(Position::new(0, 0), Position::new(4, 3), (0, 44)),
            matched: "class MyClass\n  def foo; end\n  def bar; end\nend".to_string(),
            file: Some("my_class.rb".to_string().into_boxed_str()),
            language: Some("Ruby".to_string().into_boxed_str()),
            meta_var_map: env,
        };

        assert!(match_info.has_env("CLASS_NAME"));
        assert!(match_info.has_env("METHODS"));
        assert!(!match_info.has_env("NONEXISTENT"));

        let class_name = match_info.get_env("CLASS_NAME").unwrap();
        assert_eq!(class_name.text, "MyClass");
        assert_eq!(class_name.capture_type, CaptureType::Single);

        let methods = match_info.get_env("METHODS").unwrap();
        assert_eq!(methods.capture_type, CaptureType::Multi);

        let env_names = match_info.env_names();
        assert_eq!(env_names.len(), 2);
        assert!(env_names.contains(&"CLASS_NAME".to_string()));
        assert!(env_names.contains(&"METHODS".to_string()));

        assert_eq!(match_info.line_span(), 5);
        assert_eq!(match_info.byte_length(), 44);
    }

    #[test]
    fn test_rule_manager_edge_cases() {
        let manager = RuleManager::new(SupportedLanguage::Ruby);

        // Test with different types of Ruby code
        let test_cases = vec![
            ("", "empty.rb"),
            ("puts 'hello'", "simple.rb"),
            ("# just a comment", "comment.rb"),
            ("1 + 2", "expression.rb"),
        ];

        for (code, filename) in test_cases {
            let ast = SupportedLanguage::Ruby.to_support_lang().ast_grep(code);
            let result = manager.execute_rules(&ast, Some(filename));
            assert!(result.is_ok(), "Should handle code: {code}");

            let matches = result.unwrap();
            for m in &matches {
                assert_eq!(
                    m.match_info.file,
                    Some(filename.to_string().into_boxed_str())
                );
            }
        }
    }

    #[test]
    fn test_match_info_empty_env() {
        let match_info = MatchInfo {
            rule_id: "empty-env-rule".to_string().into_boxed_str(),
            range: Range::new(Position::new(0, 0), Position::new(0, 10), (0, 10)),
            matched: "simple code".to_string(),
            file: None,
            language: Some("Ruby".to_string().into_boxed_str()),
            meta_var_map: FxHashMap::default(),
        };

        assert!(match_info.env_names().is_empty());
        assert!(!match_info.has_env("anything"));
        assert!(match_info.get_env("anything").is_none());
        assert_eq!(match_info.line_span(), 1);
        assert_eq!(match_info.byte_length(), 10);
    }

    #[test]
    fn test_range_and_position_integration() {
        use crate::utils::{Position, Range};

        let start = Position::new(1, 5);
        let end = Position::new(3, 10);
        let range = Range::new(start, end, (15, 45));

        let match_info = MatchInfo {
            rule_id: "integration-test".to_string().into_boxed_str(),
            range,
            matched: "test\ncode\nhere".to_string(),
            file: Some("integration.rb".to_string().into_boxed_str()),
            language: Some("Ruby".to_string().into_boxed_str()),
            meta_var_map: FxHashMap::default(),
        };

        assert_eq!(match_info.line_span(), 3);
        assert_eq!(match_info.byte_length(), 30);
    }

    #[test]
    fn test_empty_rules_yaml_handling() {
        // Test that load_rules_from_yaml gracefully handles empty/invalid YAML
        use crate::parser::load_rules_from_yaml;

        // Test with YAML that only has metadata (no rule field)
        let empty_yaml = r#"
id: test-empty
language: ruby
"#;
        let rules = load_rules_from_yaml(empty_yaml, SupportedLanguage::Ruby);
        assert!(
            rules.is_empty(),
            "Should return empty rules for YAML without rule field"
        );

        // Test with completely invalid YAML
        let invalid_yaml = "not: yaml: content: [unclosed";
        let rules = load_rules_from_yaml(invalid_yaml, SupportedLanguage::Ruby);
        assert!(
            rules.is_empty(),
            "Should return empty rules for invalid YAML"
        );

        // Test with empty string
        let empty_string = "";
        let rules = load_rules_from_yaml(empty_string, SupportedLanguage::Ruby);
        assert!(rules.is_empty(), "Should return empty rules for empty YAML");
    }
}
