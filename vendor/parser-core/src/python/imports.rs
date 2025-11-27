use crate::imports::ImportIdentifier;
use crate::python::fqn::find_python_fqn_for_node;
use crate::python::python_ast::PythonMatchKind;
use crate::python::types::{
    PythonFqn, PythonImportType, PythonImportedSymbolInfo, PythonNodeFqnMap,
};
use crate::rules::MatchWithNodes;
use crate::utils::{Position, Range};
use rustc_hash::FxHashMap;

/// Type-safe constants for capture variable names used in the rule engine
pub mod meta_vars {
    pub const IMPORT_STATEMENT: &str = "IMPORT_STATEMENT";
    pub const IMPORT_PATH: &str = "IMPORT_PATH";
    pub const ALIASED_IMPORT_PATH: &str = "ALIASED_IMPORT_PATH";
    pub const FROM_IMPORT_PATH: &str = "FROM_IMPORT_PATH";
    pub const ALIASED_FROM_IMPORT_PATH: &str = "ALIASED_FROM_IMPORT_PATH";
    pub const WILDCARD_IMPORT_PATH: &str = "WILDCARD_IMPORT_PATH";
    pub const RELATIVE_IMPORT_PATH: &str = "RELATIVE_IMPORT_PATH";
    pub const ALIASED_RELATIVE_IMPORT_PATH: &str = "ALIASED_RELATIVE_IMPORT_PATH";
    pub const RELATIVE_WILDCARD_IMPORT_PATH: &str = "RELATIVE_WILDCARD_IMPORT_PATH";
    pub const FUTURE_IMPORT_PATH: &str = "FUTURE_IMPORT_PATH";
    pub const ALIASED_FUTURE_IMPORT_PATH: &str = "ALIASED_FUTURE_IMPORT_PATH";
    pub const FROM_IMPORT_SYMBOL: &str = "FROM_IMPORT_SYMBOL";
    pub const ALIASED_FROM_IMPORT_SYMBOL: &str = "ALIASED_FROM_IMPORT_SYMBOL";
    pub const RELATIVE_IMPORT_SYMBOL: &str = "RELATIVE_IMPORT_SYMBOL";
    pub const ALIASED_RELATIVE_IMPORT_SYMBOL: &str = "ALIASED_RELATIVE_IMPORT_SYMBOL";
    pub const FUTURE_IMPORT_SYMBOL: &str = "FUTURE_IMPORT_SYMBOL";
    pub const ALIASED_FUTURE_IMPORT_SYMBOL: &str = "ALIASED_FUTURE_IMPORT_SYMBOL";
    pub const ALIASED_IMPORT_ALIAS: &str = "ALIASED_IMPORT_ALIAS";
    pub const ALIASED_FROM_IMPORT_ALIAS: &str = "ALIASED_FROM_IMPORT_ALIAS";
    pub const ALIASED_RELATIVE_IMPORT_ALIAS: &str = "ALIASED_RELATIVE_IMPORT_ALIAS";
    pub const ALIASED_FUTURE_IMPORT_ALIAS: &str = "ALIASED_FUTURE_IMPORT_ALIAS";
}

/// Configuration for extracting different types of imports
#[derive(Debug)]
struct ImportedSymbolExtractor {
    import_type: PythonImportType,
    import_path: fn(&FxHashMap<String, crate::rules::MetaVarNode>) -> Option<String>,
    identifier: fn(&FxHashMap<String, crate::rules::MetaVarNode>) -> Option<ImportIdentifier>,
    node: fn(&FxHashMap<String, crate::rules::MetaVarNode>) -> Option<&crate::rules::MetaVarNode>,
}

/// Unified entry point for finding imported symbols
pub fn find_imports(
    matches: &[MatchWithNodes],
    node_fqn_map: &PythonNodeFqnMap,
) -> Vec<PythonImportedSymbolInfo> {
    let mut imported_symbols = Vec::new();

    for match_item in matches {
        // Only process import matches
        if PythonMatchKind::from_rule_id(&match_item.match_info.rule_id)
            != PythonMatchKind::ImportedSymbol
        {
            continue;
        }

        // Extract imported symbol based on the captured variables
        if let Some(imported_symbol_info) = extract_imported_symbol_info(match_item, node_fqn_map) {
            imported_symbols.push(imported_symbol_info);
        }
    }

    imported_symbols
}

/// Create the imported symbol extractors configuration with enhanced env_var tracking
fn create_imported_symbol_extractors() -> Vec<ImportedSymbolExtractor> {
    use meta_vars::*;

    vec![
        ImportedSymbolExtractor {
            import_type: PythonImportType::Import,
            import_path: |env| env.get(IMPORT_PATH).map(|node| node.text.clone()),
            identifier: |env| {
                env.get(IMPORT_PATH)
                    .map(|node| node.text.clone())
                    .map(|name| ImportIdentifier { name, alias: None })
            },
            node: |env| env.get(IMPORT_PATH),
        },
        ImportedSymbolExtractor {
            import_type: PythonImportType::AliasedImport,
            import_path: |env| env.get(ALIASED_IMPORT_PATH).map(|node| node.text.clone()),
            identifier: |env| {
                env.get(ALIASED_IMPORT_PATH)
                    .map(|node| node.text.clone())
                    .map(|name| ImportIdentifier {
                        name,
                        alias: env.get(ALIASED_IMPORT_ALIAS).map(|node| node.text.clone()),
                    })
            },
            node: |env| env.get(ALIASED_IMPORT_PATH),
        },
        ImportedSymbolExtractor {
            import_type: PythonImportType::FromImport,
            import_path: |env| env.get(FROM_IMPORT_PATH).map(|node| node.text.clone()),
            identifier: |env| {
                env.get(FROM_IMPORT_SYMBOL)
                    .map(|node| node.text.clone())
                    .map(|name| ImportIdentifier { name, alias: None })
            },
            node: |env| env.get(FROM_IMPORT_SYMBOL),
        },
        ImportedSymbolExtractor {
            import_type: PythonImportType::AliasedFromImport,
            import_path: |env| {
                env.get(ALIASED_FROM_IMPORT_PATH)
                    .map(|node| node.text.clone())
            },
            identifier: |env| {
                env.get(ALIASED_FROM_IMPORT_SYMBOL)
                    .map(|node| node.text.clone())
                    .map(|name| ImportIdentifier {
                        name,
                        alias: env
                            .get(ALIASED_FROM_IMPORT_ALIAS)
                            .map(|node| node.text.clone()),
                    })
            },
            node: |env| env.get(ALIASED_FROM_IMPORT_SYMBOL),
        },
        ImportedSymbolExtractor {
            import_type: PythonImportType::WildcardImport,
            import_path: |env| env.get(WILDCARD_IMPORT_PATH).map(|node| node.text.clone()),
            identifier: |env| {
                env.get(WILDCARD_IMPORT_PATH)
                    .map(|node| node.text.clone())
                    .map(|_import_path| ImportIdentifier {
                        name: "*".to_string(),
                        alias: None,
                    })
            },
            node: |env| env.get(WILDCARD_IMPORT_PATH),
        },
        ImportedSymbolExtractor {
            import_type: PythonImportType::RelativeWildcardImport,
            import_path: |env| {
                env.get(RELATIVE_WILDCARD_IMPORT_PATH)
                    .map(|node| node.text.clone())
            },
            identifier: |env| {
                env.get(RELATIVE_WILDCARD_IMPORT_PATH)
                    .map(|node| node.text.clone())
                    .map(|_import_path| ImportIdentifier {
                        name: "*".to_string(),
                        alias: None,
                    })
            },
            node: |env| env.get(RELATIVE_WILDCARD_IMPORT_PATH),
        },
        ImportedSymbolExtractor {
            import_type: PythonImportType::RelativeImport,
            import_path: |env| env.get(RELATIVE_IMPORT_PATH).map(|node| node.text.clone()),
            identifier: |env| {
                env.get(RELATIVE_IMPORT_SYMBOL)
                    .map(|node| node.text.clone())
                    .map(|name| ImportIdentifier { name, alias: None })
            },
            node: |env| env.get(RELATIVE_IMPORT_SYMBOL),
        },
        ImportedSymbolExtractor {
            import_type: PythonImportType::AliasedRelativeImport,
            import_path: |env| {
                env.get(ALIASED_RELATIVE_IMPORT_PATH)
                    .map(|node| node.text.clone())
            },
            identifier: |env| {
                env.get(ALIASED_RELATIVE_IMPORT_SYMBOL)
                    .map(|node| node.text.clone())
                    .map(|name| ImportIdentifier {
                        name,
                        alias: env
                            .get(ALIASED_RELATIVE_IMPORT_ALIAS)
                            .map(|node| node.text.clone()),
                    })
            },
            node: |env| env.get(ALIASED_RELATIVE_IMPORT_SYMBOL),
        },
        ImportedSymbolExtractor {
            import_type: PythonImportType::FutureImport,
            import_path: |env| {
                if env.get(FUTURE_IMPORT_SYMBOL).is_some() {
                    Some("__future__".to_string())
                } else {
                    None
                }
            },
            identifier: |env| {
                env.get(FUTURE_IMPORT_SYMBOL)
                    .map(|node| node.text.clone())
                    .map(|name| ImportIdentifier { name, alias: None })
            },
            node: |env| env.get(FUTURE_IMPORT_SYMBOL),
        },
        ImportedSymbolExtractor {
            import_type: PythonImportType::AliasedFutureImport,
            import_path: |env| {
                if env.get(ALIASED_FUTURE_IMPORT_SYMBOL).is_some() {
                    Some("__future__".to_string())
                } else {
                    None
                }
            },
            identifier: |env| {
                env.get(ALIASED_FUTURE_IMPORT_SYMBOL)
                    .map(|node| node.text.clone())
                    .map(|name| ImportIdentifier {
                        name,
                        alias: env
                            .get(ALIASED_FUTURE_IMPORT_ALIAS)
                            .map(|node| node.text.clone()),
                    })
            },
            node: |env| env.get(ALIASED_FUTURE_IMPORT_SYMBOL),
        },
    ]
}

fn extract_imported_symbol_info(
    match_item: &MatchWithNodes,
    node_fqn_map: &PythonNodeFqnMap,
) -> Option<PythonImportedSymbolInfo> {
    let env = &match_item.match_info.meta_var_map;

    // Try each extractor until we find one that matches
    for extractor in create_imported_symbol_extractors() {
        if let Some(import_path) = (extractor.import_path)(env)
            && let Some(env_node) = (extractor.node)(env)
        {
            let range = Range::new(
                Position::new(env_node.range.0, env_node.range.1),
                Position::new(env_node.range.2, env_node.range.3),
                env_node.byte_offset,
            );
            let scope = get_import_scope(env, node_fqn_map);

            return Some(PythonImportedSymbolInfo {
                import_type: extractor.import_type,
                import_path,
                identifier: (extractor.identifier)(env),
                range,
                scope,
            });
        }
    }

    None
}

fn get_import_scope(
    env: &FxHashMap<String, crate::rules::MetaVarNode>,
    node_fqn_map: &PythonNodeFqnMap,
) -> Option<PythonFqn> {
    if let Some(env_node) = env.get(meta_vars::IMPORT_STATEMENT) {
        let range = Range::new(
            Position::new(env_node.range.0, env_node.range.1),
            Position::new(env_node.range.2, env_node.range.3),
            env_node.byte_offset,
        );
        return find_python_fqn_for_node(range, node_fqn_map);
    }

    None
}

#[cfg(test)]
mod import_tests {
    use super::*;
    use crate::parser::{GenericParser, LanguageParser, SupportedLanguage};
    use crate::python::fqn::build_fqn_index;
    use crate::rules::{RuleManager, run_rules};

    fn test_import_extraction(
        code: &str,
        expected_imported_symbols: Vec<(PythonImportType, &str, ImportIdentifier)>, // (import_type, import_path, identifier)
        description: &str,
    ) {
        println!("\n=== Testing: {description} ===");
        println!("Code snippet:\n{code}");

        let parser = GenericParser::default_for_language(SupportedLanguage::Python);
        let parse_result = parser.parse(code, Some("test.py")).unwrap();
        let rule_manager = RuleManager::new(SupportedLanguage::Python);
        let matches = run_rules(&parse_result.ast, Some("test.py"), &rule_manager);

        let node_fqn_map = build_fqn_index(&parse_result.ast);
        let imported_symbols = find_imports(&matches, &node_fqn_map);

        assert_eq!(
            imported_symbols.len(),
            expected_imported_symbols.len(),
            "Expected {} imported symbols, found {}",
            expected_imported_symbols.len(),
            imported_symbols.len()
        );

        println!("Found {} imported symbols:", imported_symbols.len());
        for (expected_type, expected_path, expected_identifier) in expected_imported_symbols {
            let _matching_symbol = imported_symbols
                .iter()
                .find(|i| {
                    i.import_type == expected_type
                        && i.import_path == expected_path
                        && i.identifier == Some(expected_identifier.clone())
                })
                .unwrap_or_else(|| {
                    panic!(
                        "Could not find: type={:?}, path={}, name={:?}, alias={:?}",
                        expected_type,
                        expected_path,
                        expected_identifier.name,
                        expected_identifier.alias
                    )
                });

            println!(
                "Found: type={:?}, path={}, name={:?}, alias={:?}",
                expected_type, expected_path, expected_identifier.name, expected_identifier.alias
            );
        }
        println!("✅ All assertions passed for: {description}\n");
    }

    #[test]
    fn test_regular_imports() {
        let code = r#"
import this.is.deeply.nested
import numpy, torch.nn
`       "#;
        let expected_imported_symbols = vec![
            (
                PythonImportType::Import,
                "this.is.deeply.nested",
                ImportIdentifier {
                    name: "this.is.deeply.nested".to_string(),
                    alias: None,
                },
            ),
            (
                PythonImportType::Import,
                "numpy",
                ImportIdentifier {
                    name: "numpy".to_string(),
                    alias: None,
                },
            ),
            (
                PythonImportType::Import,
                "torch.nn",
                ImportIdentifier {
                    name: "torch.nn".to_string(),
                    alias: None,
                },
            ),
        ];
        test_import_extraction(code, expected_imported_symbols, "Regular imports");
    }

    #[test]
    fn test_aliased_regular_imports() {
        let code = r#"
import xml.etree.ElementTree as ET
import numpy as np, torch as pytorch
`       "#;
        let expected_imported_symbols = vec![
            (
                PythonImportType::AliasedImport,
                "xml.etree.ElementTree",
                ImportIdentifier {
                    name: "xml.etree.ElementTree".to_string(),
                    alias: Some("ET".to_string()),
                },
            ),
            (
                PythonImportType::AliasedImport,
                "numpy",
                ImportIdentifier {
                    name: "numpy".to_string(),
                    alias: Some("np".to_string()),
                },
            ),
            (
                PythonImportType::AliasedImport,
                "torch",
                ImportIdentifier {
                    name: "torch".to_string(),
                    alias: Some("pytorch".to_string()),
                },
            ),
        ];
        test_import_extraction(code, expected_imported_symbols, "Aliased imports");
    }

    #[test]
    fn test_from_imports() {
        let code = r#"
from os.path import join
from numpy import array, matrix
`       "#;
        let expected_imported_symbols = vec![
            (
                PythonImportType::FromImport,
                "os.path",
                ImportIdentifier {
                    name: "join".to_string(),
                    alias: None,
                },
            ),
            (
                PythonImportType::FromImport,
                "numpy",
                ImportIdentifier {
                    name: "array".to_string(),
                    alias: None,
                },
            ),
            (
                PythonImportType::FromImport,
                "numpy",
                ImportIdentifier {
                    name: "matrix".to_string(),
                    alias: None,
                },
            ),
        ];
        test_import_extraction(code, expected_imported_symbols, "From imports");
    }

    #[test]
    fn test_aliased_from_imports() {
        let code = r#"
from urllib.parse import quote as url_quote
from typing import List as ListType, Dict as DictType
`       "#;
        let expected_imported_symbols = vec![
            (
                PythonImportType::AliasedFromImport,
                "urllib.parse",
                ImportIdentifier {
                    name: "quote".to_string(),
                    alias: Some("url_quote".to_string()),
                },
            ),
            (
                PythonImportType::AliasedFromImport,
                "typing",
                ImportIdentifier {
                    name: "List".to_string(),
                    alias: Some("ListType".to_string()),
                },
            ),
            (
                PythonImportType::AliasedFromImport,
                "typing",
                ImportIdentifier {
                    name: "Dict".to_string(),
                    alias: Some("DictType".to_string()),
                },
            ),
        ];
        test_import_extraction(code, expected_imported_symbols, "Aliased from imports");
    }

    #[test]
    fn test_mixed_from_imports() {
        let code = r#"
from collections import namedtuple, defaultdict as dd, other_stuff
`       "#;
        let expected_imported_symbols = vec![
            (
                PythonImportType::FromImport,
                "collections",
                ImportIdentifier {
                    name: "namedtuple".to_string(),
                    alias: None,
                },
            ),
            (
                PythonImportType::AliasedFromImport,
                "collections",
                ImportIdentifier {
                    name: "defaultdict".to_string(),
                    alias: Some("dd".to_string()),
                },
            ),
            (
                PythonImportType::FromImport,
                "collections",
                ImportIdentifier {
                    name: "other_stuff".to_string(),
                    alias: None,
                },
            ),
        ];
        test_import_extraction(
            code,
            expected_imported_symbols,
            "Mixed from imports (aliased and non-aliased)",
        );
    }

    #[test]
    fn test_wildcard_imports() {
        let code = r#"
from tkinter import *
from this.is.nested import *
        "#;
        let expected_imported_symbols = vec![
            (
                PythonImportType::WildcardImport,
                "tkinter",
                ImportIdentifier {
                    name: "*".to_string(),
                    alias: None,
                },
            ),
            (
                PythonImportType::WildcardImport,
                "this.is.nested",
                ImportIdentifier {
                    name: "*".to_string(),
                    alias: None,
                },
            ),
        ];
        test_import_extraction(code, expected_imported_symbols, "Wildcard imports");
    }

    #[test]
    fn test_relative_imports() {
        let code = r#"
from .. import rel_symbol
from .. import rel_symbol1, rel_symbol2
`       "#;
        let expected_imported_symbols = vec![
            (
                PythonImportType::RelativeImport,
                "..",
                ImportIdentifier {
                    name: "rel_symbol".to_string(),
                    alias: None,
                },
            ),
            (
                PythonImportType::RelativeImport,
                "..",
                ImportIdentifier {
                    name: "rel_symbol1".to_string(),
                    alias: None,
                },
            ),
            (
                PythonImportType::RelativeImport,
                "..",
                ImportIdentifier {
                    name: "rel_symbol2".to_string(),
                    alias: None,
                },
            ),
        ];
        test_import_extraction(code, expected_imported_symbols, "Relative imports");
    }

    #[test]
    fn test_aliased_relative_imports() {
        let code = r#"
from .subpackage import something as sth
from .subpackage import one_thing as oth, another_thing as ath
`       "#;
        let expected_imported_symbols = vec![
            (
                PythonImportType::AliasedRelativeImport,
                ".subpackage",
                ImportIdentifier {
                    name: "something".to_string(),
                    alias: Some("sth".to_string()),
                },
            ),
            (
                PythonImportType::AliasedRelativeImport,
                ".subpackage",
                ImportIdentifier {
                    name: "one_thing".to_string(),
                    alias: Some("oth".to_string()),
                },
            ),
            (
                PythonImportType::AliasedRelativeImport,
                ".subpackage",
                ImportIdentifier {
                    name: "another_thing".to_string(),
                    alias: Some("ath".to_string()),
                },
            ),
        ];
        test_import_extraction(code, expected_imported_symbols, "Aliased relative imports");
    }

    #[test]
    fn test_relative_wildcard_imports() {
        let code = r#"
from ..parent_module import *
`       "#;
        let expected_imported_symbols = vec![(
            PythonImportType::RelativeWildcardImport,
            "..parent_module",
            ImportIdentifier {
                name: "*".to_string(),
                alias: None,
            },
        )];
        test_import_extraction(code, expected_imported_symbols, "Relative wildcard imports");
    }

    #[test]
    fn test_future_imports() {
        let code = r#"
from __future__ import annotations
from __future__ import print_function, division
`       "#;
        let expected_imported_symbols = vec![
            (
                PythonImportType::FutureImport,
                "__future__",
                ImportIdentifier {
                    name: "annotations".to_string(),
                    alias: None,
                },
            ),
            (
                PythonImportType::FutureImport,
                "__future__",
                ImportIdentifier {
                    name: "print_function".to_string(),
                    alias: None,
                },
            ),
            (
                PythonImportType::FutureImport,
                "__future__",
                ImportIdentifier {
                    name: "division".to_string(),
                    alias: None,
                },
            ),
        ];
        test_import_extraction(code, expected_imported_symbols, "Future imports");
    }

    #[test]
    fn test_aliased_future_imports() {
        let code = r#"
from __future__ import annotations as annot
from __future__ import print_function as pf, division as div
`       "#;
        let expected_imported_symbols = vec![
            (
                PythonImportType::AliasedFutureImport,
                "__future__",
                ImportIdentifier {
                    name: "annotations".to_string(),
                    alias: Some("annot".to_string()),
                },
            ),
            (
                PythonImportType::AliasedFutureImport,
                "__future__",
                ImportIdentifier {
                    name: "print_function".to_string(),
                    alias: Some("pf".to_string()),
                },
            ),
            (
                PythonImportType::AliasedFutureImport,
                "__future__",
                ImportIdentifier {
                    name: "division".to_string(),
                    alias: Some("div".to_string()),
                },
            ),
        ];
        test_import_extraction(code, expected_imported_symbols, "Aliased future imports");
    }
}
