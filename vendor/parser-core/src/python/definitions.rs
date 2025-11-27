use crate::python::fqn::find_python_fqn_for_node;
use crate::python::python_ast::PythonMatchKind;
use crate::python::types::{
    PythonDefinitionInfo, PythonDefinitionType, PythonFqn, PythonNodeFqnMap,
};
use crate::rules::MatchWithNodes;
use crate::utils::{Position, Range};
use rustc_hash::FxHashMap;

/// Type-safe constants for capture variable names used in the rule definitions
mod meta_vars {
    pub const CLASS_DEF_NAME: &str = "CLASS_DEF_NAME";
    pub const DECORATED_CLASS_DEF_NAME: &str = "DECORATED_CLASS_DEF_NAME";
    pub const METHOD_DEF_NAME: &str = "METHOD_DEF_NAME";
    pub const DECORATED_METHOD_DEF_NAME: &str = "DECORATED_METHOD_DEF_NAME";
    pub const ASYNC_METHOD_DEF_NAME: &str = "ASYNC_METHOD_DEF_NAME";
    pub const DECORATED_ASYNC_METHOD_DEF_NAME: &str = "DECORATED_ASYNC_METHOD_DEF_NAME";
    pub const FUNCTION_DEF_NAME: &str = "FUNCTION_DEF_NAME";
    pub const DECORATED_FUNCTION_DEF_NAME: &str = "DECORATED_FUNCTION_DEF_NAME";
    pub const ASYNC_FUNCTION_DEF_NAME: &str = "ASYNC_FUNCTION_DEF_NAME";
    pub const DECORATED_ASYNC_FUNCTION_DEF_NAME: &str = "DECORATED_ASYNC_FUNCTION_DEF_NAME";
    pub const LAMBDA_DEF: &str = "LAMBDA_DEF";
    pub const LAMBDA_VARIABLE_NAME: &str = "LAMBDA_VARIABLE_NAME";
    pub const LAMBDA_ATTRIBUTE_NAME: &str = "LAMBDA_ATTRIBUTE_NAME";
}

/// Configuration for extracting different types of definitions
#[derive(Debug)]
struct DefinitionExtractor {
    definition_type: PythonDefinitionType,
    extractor: fn(&FxHashMap<String, crate::rules::MetaVarNode>) -> Option<String>,
}

/// Unified entry point for finding definitions with FQN computation
/// This function combines DefinitionExtractor functionality with FQN computation
pub fn find_definitions(
    matches: &[MatchWithNodes],
    node_fqn_map: &PythonNodeFqnMap,
) -> Vec<PythonDefinitionInfo> {
    let mut definitions = Vec::new();

    for match_item in matches {
        // Only process definition matches
        if PythonMatchKind::from_rule_id(&match_item.match_info.rule_id)
            != PythonMatchKind::Definition
        {
            continue;
        }

        // Extract definition based on the captured variables
        if let Some(def_info) = extract_definition_info(match_item, node_fqn_map) {
            definitions.push(def_info);
        }
    }

    definitions
}

/// Create the definition extractors configuration with enhanced env_var tracking
fn create_definition_extractors() -> Vec<DefinitionExtractor> {
    use meta_vars::*;

    vec![
        DefinitionExtractor {
            definition_type: PythonDefinitionType::Class,
            extractor: |env| env.get(CLASS_DEF_NAME).map(|node| node.text.clone()),
        },
        DefinitionExtractor {
            definition_type: PythonDefinitionType::DecoratedClass,
            extractor: |env| {
                env.get(DECORATED_CLASS_DEF_NAME)
                    .map(|node| node.text.clone())
            },
        },
        DefinitionExtractor {
            definition_type: PythonDefinitionType::Method,
            extractor: |env| env.get(METHOD_DEF_NAME).map(|node| node.text.clone()),
        },
        DefinitionExtractor {
            definition_type: PythonDefinitionType::DecoratedMethod,
            extractor: |env| {
                env.get(DECORATED_METHOD_DEF_NAME)
                    .map(|node| node.text.clone())
            },
        },
        DefinitionExtractor {
            definition_type: PythonDefinitionType::AsyncMethod,
            extractor: |env| env.get(ASYNC_METHOD_DEF_NAME).map(|node| node.text.clone()),
        },
        DefinitionExtractor {
            definition_type: PythonDefinitionType::DecoratedAsyncMethod,
            extractor: |env| {
                env.get(DECORATED_ASYNC_METHOD_DEF_NAME)
                    .map(|node| node.text.clone())
            },
        },
        DefinitionExtractor {
            definition_type: PythonDefinitionType::AsyncFunction,
            extractor: |env| {
                env.get(ASYNC_FUNCTION_DEF_NAME)
                    .map(|node| node.text.clone())
            },
        },
        DefinitionExtractor {
            definition_type: PythonDefinitionType::DecoratedAsyncFunction,
            extractor: |env| {
                env.get(DECORATED_ASYNC_FUNCTION_DEF_NAME)
                    .map(|node| node.text.clone())
            },
        },
        DefinitionExtractor {
            definition_type: PythonDefinitionType::Function,
            extractor: |env| env.get(FUNCTION_DEF_NAME).map(|node| node.text.clone()),
        },
        DefinitionExtractor {
            definition_type: PythonDefinitionType::DecoratedFunction,
            extractor: |env| {
                env.get(DECORATED_FUNCTION_DEF_NAME)
                    .map(|node| node.text.clone())
            },
        },
        DefinitionExtractor {
            definition_type: PythonDefinitionType::Lambda,
            extractor: |env| {
                if env.get(LAMBDA_DEF).is_some() {
                    // For lambda assignments, extract the variable/attribute name
                    if let Some(var_node) = env.get(LAMBDA_VARIABLE_NAME) {
                        Some(var_node.text.clone())
                    } else {
                        env.get(LAMBDA_ATTRIBUTE_NAME)
                            .map(|attr_node| attr_node.text.clone())
                    }
                } else {
                    None
                }
            },
        },
    ]
}

/// Extract definition information that combines DefinitionExtractor with FQN computation
/// and captures all variables
fn extract_definition_info(
    match_item: &MatchWithNodes,
    node_fqn_map: &PythonNodeFqnMap,
) -> Option<PythonDefinitionInfo> {
    let env = &match_item.match_info.meta_var_map;

    // Try each extractor until we find one that matches
    for extractor in create_definition_extractors() {
        if let Some(name) = (extractor.extractor)(env)
            && let Some(fqn) =
                find_fqn_for_definition(&extractor.definition_type, env, node_fqn_map)
        {
            return Some(PythonDefinitionInfo::new(
                extractor.definition_type,
                name,
                fqn,
                match_item.match_info.range,
            ));
        }
        // If no FQN can be computed, skip this definition (don't index it)
    }

    None
}

/// Find FQN for a definition by looking up its name node in the FQN map
fn find_fqn_for_definition(
    def_type: &PythonDefinitionType,
    env: &FxHashMap<String, crate::rules::MetaVarNode>,
    node_fqn_map: &PythonNodeFqnMap,
) -> Option<PythonFqn> {
    // Try to find the capture variable for this definition type
    let meta_var_name = match def_type {
        PythonDefinitionType::Class => meta_vars::CLASS_DEF_NAME,
        PythonDefinitionType::DecoratedClass => meta_vars::DECORATED_CLASS_DEF_NAME,
        PythonDefinitionType::Function => meta_vars::FUNCTION_DEF_NAME,
        PythonDefinitionType::DecoratedFunction => meta_vars::DECORATED_FUNCTION_DEF_NAME,
        PythonDefinitionType::AsyncFunction => meta_vars::ASYNC_FUNCTION_DEF_NAME,
        PythonDefinitionType::DecoratedAsyncFunction => {
            meta_vars::DECORATED_ASYNC_FUNCTION_DEF_NAME
        }
        PythonDefinitionType::Method => meta_vars::METHOD_DEF_NAME,
        PythonDefinitionType::AsyncMethod => meta_vars::ASYNC_METHOD_DEF_NAME,
        PythonDefinitionType::DecoratedMethod => meta_vars::DECORATED_METHOD_DEF_NAME,
        PythonDefinitionType::DecoratedAsyncMethod => meta_vars::DECORATED_ASYNC_METHOD_DEF_NAME,
        PythonDefinitionType::Lambda => {
            // For lambdas, find the assignment target
            find_lambda_meta_var(env)
        }
    };

    // Get the environment node for this definition
    if let Some(env_node) = env.get(meta_var_name) {
        let range = Range::new(
            Position::new(env_node.range.0, env_node.range.1),
            Position::new(env_node.range.2, env_node.range.3),
            env_node.byte_offset,
        );

        return find_python_fqn_for_node(range, node_fqn_map);
    }

    None
}

/// Find the appropriate capture variable for lambda definitions
fn find_lambda_meta_var(env: &FxHashMap<String, crate::rules::MetaVarNode>) -> &'static str {
    if env.contains_key(meta_vars::LAMBDA_VARIABLE_NAME) {
        meta_vars::LAMBDA_VARIABLE_NAME
    } else if env.contains_key(meta_vars::LAMBDA_ATTRIBUTE_NAME) {
        meta_vars::LAMBDA_ATTRIBUTE_NAME
    } else {
        meta_vars::LAMBDA_DEF
    }
}

#[cfg(test)]
mod definition_tests {
    use super::*;
    use crate::parser::{GenericParser, LanguageParser, SupportedLanguage};
    use crate::python::fqn::{build_fqn_index, python_fqn_to_string};
    use crate::rules::{RuleManager, run_rules};

    fn test_definition_extraction(
        code: &str,
        expected_definitions: Vec<(&str, PythonDefinitionType, &str)>, // (name, type, expected_fqn)
        description: &str,
    ) {
        println!("\n=== Testing: {description} ===");
        println!("Code snippet:\n{code}");

        let parser = GenericParser::default_for_language(SupportedLanguage::Python);
        let parse_result = parser.parse(code, Some("test.py")).unwrap();
        let rule_manager = RuleManager::new(SupportedLanguage::Python);
        let matches = run_rules(&parse_result.ast, Some("test.py"), &rule_manager);

        let node_fqn_map = build_fqn_index(&parse_result.ast);
        let definitions = find_definitions(&matches, &node_fqn_map);

        println!("Found {} definitions:", definitions.len());
        for def in &definitions {
            let fqn_str = python_fqn_to_string(&def.fqn);
            println!("  {:?}: {} -> {}", def.definition_type, def.name, fqn_str);
        }

        assert_eq!(
            definitions.len(),
            expected_definitions.len(),
            "Expected {} definitions, found {}",
            expected_definitions.len(),
            definitions.len()
        );

        for (expected_name, expected_type, expected_fqn) in expected_definitions {
            let matching_def = definitions
                .iter()
                .find(|d| d.name == expected_name && d.definition_type == expected_type)
                .unwrap_or_else(|| {
                    panic!("Could not find definition: {expected_name} of type {expected_type:?}")
                });

            let actual_fqn = &matching_def.fqn;
            let actual_fqn_str = python_fqn_to_string(actual_fqn);
            assert_eq!(
                actual_fqn_str, expected_fqn,
                "FQN mismatch for {expected_name}: expected '{expected_fqn}', got '{actual_fqn_str}'"
            );
        }
        println!("✅ All assertions passed for: {description}\n");
    }

    #[test]
    fn test_simple_function_definition() {
        let code = r#"
def simple_function(x: int, y: str = "default") -> bool:
    return len(y) > x
        "#;
        let expected_definitions = vec![(
            "simple_function",
            PythonDefinitionType::Function,
            "simple_function",
        )];
        test_definition_extraction(code, expected_definitions, "Simple function definition");
    }

    #[test]
    fn test_generator_function_definition() {
        let code = r#"
def generator_function():
    yield 1
    yield 2
        "#;
        let expected_definitions = vec![(
            "generator_function",
            PythonDefinitionType::Function,
            "generator_function",
        )];
        test_definition_extraction(code, expected_definitions, "Generator definition");
    }

    #[test]
    fn test_decorated_function_definition() {
        let code = r#"
@staticmethod
@property
def decorated_function():
    pass
        "#;
        let expected_definitions = vec![(
            "decorated_function",
            PythonDefinitionType::DecoratedFunction,
            "decorated_function",
        )];
        test_definition_extraction(code, expected_definitions, "Decorated function definition");
    }

    #[test]
    fn test_async_function_definition() {
        let code = r#"
async def async_function():
    pass
        "#;
        let expected_definitions = vec![(
            "async_function",
            PythonDefinitionType::AsyncFunction,
            "async_function",
        )];
        test_definition_extraction(
            code,
            expected_definitions,
            "Asynchronous function definition",
        );
    }

    #[test]
    fn test_decorated_async_function_definition() {
        let code = r#"
@decorator
async def decorated_async_function():
    pass
        "#;
        let expected_definitions = vec![(
            "decorated_async_function",
            PythonDefinitionType::DecoratedAsyncFunction,
            "decorated_async_function",
        )];
        test_definition_extraction(
            code,
            expected_definitions,
            "Decorated asynchronous function definition",
        );
    }

    #[test]
    fn test_lambda_function_definition() {
        let code = r#"
module_lambda = lambda x: x * 2
        "#;
        let expected_definitions = vec![(
            "module_lambda",
            PythonDefinitionType::Lambda,
            "module_lambda",
        )];
        test_definition_extraction(code, expected_definitions, "Module-level lambda definition");
    }

    #[test]
    fn test_nested_function_definitions() {
        let code = r#"
def outer_function():
    def inner_function():
        pass
    
    inner_lambda = lambda x: x
    return inner_function
        "#;
        let expected_definitions = vec![
            (
                "outer_function",
                PythonDefinitionType::Function,
                "outer_function",
            ),
            (
                "inner_function",
                PythonDefinitionType::Function,
                "outer_function.inner_function",
            ),
            (
                "inner_lambda",
                PythonDefinitionType::Lambda,
                "outer_function.inner_lambda",
            ),
        ];
        test_definition_extraction(code, expected_definitions, "Nested function definitions");
    }

    #[test]
    fn test_simple_class_definition() {
        let code = r#"
class SimpleClass:
    class_var = 42
        "#;
        let expected_definitions =
            vec![("SimpleClass", PythonDefinitionType::Class, "SimpleClass")];
        test_definition_extraction(code, expected_definitions, "Simple class definition");
    }

    #[test]
    fn test_decorated_class_definition() {
        let code = r#"
from dataclasses import dataclass
@dataclass
class DecoratedClass:
    field: int
        "#;
        let expected_definitions = vec![(
            "DecoratedClass",
            PythonDefinitionType::DecoratedClass,
            "DecoratedClass",
        )];
        test_definition_extraction(code, expected_definitions, "Decorated class definition");
    }

    #[test]
    fn test_method_definitions() {
        let code = r#"
class MyClass:
    def method(self):
        self.attr_lambda = lambda x: x * 2
    
    async def async_method(self):
        pass
    
    def nested_method(self):
        def inner_function():
            pass
    
    @classmethod
    def decorated_method(cls):
        pass
    
    @classmethod
    async def decorated_async_method(cls):
        pass
    
    lambda_method = lambda self: self.class_var * 2
        "#;
        let expected_definitions = vec![
            ("MyClass", PythonDefinitionType::Class, "MyClass"),
            ("method", PythonDefinitionType::Method, "MyClass.method"),
            (
                "self.attr_lambda",
                PythonDefinitionType::Lambda,
                "MyClass.method.self#attr_lambda",
            ),
            (
                "async_method",
                PythonDefinitionType::AsyncMethod,
                "MyClass.async_method",
            ),
            (
                "nested_method",
                PythonDefinitionType::Method,
                "MyClass.nested_method",
            ),
            (
                "inner_function",
                PythonDefinitionType::Function,
                "MyClass.nested_method.inner_function",
            ),
            (
                "decorated_method",
                PythonDefinitionType::DecoratedMethod,
                "MyClass.decorated_method",
            ),
            (
                "decorated_async_method",
                PythonDefinitionType::DecoratedAsyncMethod,
                "MyClass.decorated_async_method",
            ),
            (
                "lambda_method",
                PythonDefinitionType::Lambda,
                "MyClass.lambda_method",
            ),
        ];
        test_definition_extraction(code, expected_definitions, "Class method definitions");
    }

    #[test]
    fn test_nested_class_definitions() {
        let code = r#"
class NestedClass:
    class InnerClass:
        class_var = 42
        "#;
        let expected_definitions = vec![
            ("NestedClass", PythonDefinitionType::Class, "NestedClass"),
            (
                "InnerClass",
                PythonDefinitionType::Class,
                "NestedClass.InnerClass",
            ),
        ];
        test_definition_extraction(code, expected_definitions, "Class method definitions");
    }
}
