//! GitLab Code Parser Core
//!
//! A foundational library for parsing and analyzing code across multiple programming languages
//! using `ast-grep` and `tree-sitter` for pattern matching and AST analysis.

pub mod analyzer;
pub mod csharp;
pub mod definitions;
pub mod fqn;
pub mod imports;
pub mod java;
pub mod kotlin;
pub mod parser;
pub mod python;
pub mod references;
pub mod ruby;
pub mod rules;
pub mod rust;
pub mod typescript;
pub mod utils;

// Re-export commonly used types
pub use analyzer::{AnalysisResult, Analyzer};
pub use ast_grep_language::{LanguageExt, SupportLang};
pub use definitions::DefinitionLookup;
pub use parser::{LanguageParser, ParseResult, SupportedLanguage};
pub use rules::{MatchInfo, RuleManager};
pub use utils::{Position, Range};

/// The main result type for parsing operations
pub type Result<T> = std::result::Result<T, Error>;

/// Core error types for the parser
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Rule loading error: {0}")]
    RuleLoading(String),

    #[error("Language not supported: {0}")]
    UnsupportedLanguage(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("TOML deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        parser::{GenericParser, detect_language_from_path},
        rules::{RuleManager, run_rules},
    };
    use rustc_hash::FxHashMap;

    #[test]
    fn test_library_exports() {
        // Test that main exports are available
        let _: SupportedLanguage = SupportedLanguage::Ruby;
        let _: Result<()> = Ok(());
    }

    #[test]
    fn test_complete_ruby_parsing_workflow() -> Result<()> {
        // 1. Create parser
        let parser = GenericParser::default_for_language(SupportedLanguage::Ruby);

        // 2. Sample Ruby code
        let ruby_code = r#"
class Calculator
  def initialize
    @value = 0
  end

  def add(number)
    @value += number
    self
  end

  def subtract(number)
    @value -= number
    self
  end

  def result
    @value
  end
end

module MathUtils
  def self.square(n)
    n * n
  end
end

calc = Calculator.new
result = calc.add(5).subtract(2).result
squared = MathUtils.square(result)
"#;

        // 4. Parse the code
        let parse_result = parser.parse(ruby_code, Some("calculator.rb"))?;

        // 5. Verify results
        assert_eq!(parse_result.language, SupportedLanguage::Ruby);
        assert_eq!(parse_result.file_path.as_deref(), Some("calculator.rb"));
        assert!(!parse_result.ast.root().text().is_empty());

        println!("Parse result:");
        println!("  Language: {}", parse_result.language);
        println!("  File: {:?}", parse_result.file_path);
        println!("  AST root node: {}", parse_result.ast.root().kind());

        Ok(())
    }

    #[test]
    fn test_rule_loading_and_execution() -> Result<()> {
        // 1. Test language detection
        let language = detect_language_from_path("test.rb")?;
        assert_eq!(language, SupportedLanguage::Ruby);

        // 2. Create rule manager (automatically loads rules)
        let rule_manager = RuleManager::new(language);
        println!("Successfully loaded embedded rules for {language}");

        // 3. Test rule execution
        if !rule_manager.rules().is_empty() {
            let ruby_code = "class Test\n  def hello\n    puts 'world'\n  end\nend";
            let ast = language.to_support_lang().ast_grep(ruby_code);
            let matches = run_rules(&ast, Some("test.rb"), &rule_manager);

            println!("  Found {} matches", matches.len());
            for (i, m) in matches.iter().take(5).enumerate() {
                println!(
                    "    Match {}: {} (rule: {})",
                    i + 1,
                    m.match_info.matched.trim(),
                    m.match_info.rule_id
                );
            }
        }

        Ok(())
    }

    #[test]
    fn test_cross_language_support() -> Result<()> {
        let languages = [("test.rb", SupportedLanguage::Ruby)];

        for (file_path, expected_lang) in languages {
            let detected = detect_language_from_path(file_path)?;
            assert_eq!(detected, expected_lang);

            // Test parser creation for each language
            let parser = GenericParser::default_for_language(expected_lang);
            assert_eq!(parser.language(), expected_lang);
        }

        Ok(())
    }

    #[test]
    fn test_error_handling() {
        // Test unsupported language
        assert!(detect_language_from_path("unknown.xyz").is_err());
    }

    #[test]
    fn test_utility_functions() {
        use crate::rules::{MatchWithNodes, filter_matches_by_rule_id};

        // Create actual parsed nodes for testing
        let test_code = "class Test1\nend\nclass Test2\nend";
        let ast = SupportedLanguage::Ruby
            .to_support_lang()
            .ast_grep(test_code);
        let test_node1 = ast.root().child(0).unwrap();
        let test_node2 = ast.root().child(1).unwrap();

        // Create some sample matches
        let matches = vec![
            MatchWithNodes {
                match_info: MatchInfo {
                    rule_id: "test-rule-1".to_string().into_boxed_str(),
                    range: Range::new(Position::new(1, 0), Position::new(1, 5), (0, 5)),
                    matched: "test1".to_string(),
                    file: Some("test.rb".to_string().into_boxed_str()),
                    language: Some("Ruby".to_string().into_boxed_str()),
                    meta_var_map: FxHashMap::default(),
                },
                raw_node: test_node1,
                env_nodes: FxHashMap::default(),
                multi_env_nodes: FxHashMap::default(),
            },
            MatchWithNodes {
                match_info: MatchInfo {
                    rule_id: "test-rule-2".to_string().into_boxed_str(),
                    range: Range::new(Position::new(2, 0), Position::new(2, 5), (6, 11)),
                    matched: "test2".to_string(),
                    file: Some("test.rb".to_string().into_boxed_str()),
                    language: Some("Ruby".to_string().into_boxed_str()),
                    meta_var_map: FxHashMap::default(),
                },
                raw_node: test_node2,
                env_nodes: FxHashMap::default(),
                multi_env_nodes: FxHashMap::default(),
            },
        ];

        // Test filtering
        let filtered = filter_matches_by_rule_id(&matches, "test-rule-1");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].match_info.rule_id.as_ref(), "test-rule-1");
    }
}
