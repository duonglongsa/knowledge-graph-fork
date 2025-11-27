use crate::imports::ImportIdentifier;
use crate::typescript::types::{
    TypeScriptFqn, TypeScriptImportType, TypeScriptImportedSymbolInfo,
    TypeScriptImportedSymbolInfoVec, TypeScriptScopeStack, ts_import_node_types,
};
use crate::utils::node_to_range;

use ast_grep_core::Node;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_language::SupportLang;

/// Detect if a node represents an import statement and extract the import information
pub fn detect_import_declaration(
    node: &Node<StrDoc<SupportLang>>,
    current_scope: &TypeScriptScopeStack,
) -> TypeScriptImportedSymbolInfoVec {
    match node.kind().as_ref() {
        ts_import_node_types::IMPORT_STATEMENT => extract_import_statement(node, current_scope),
        ts_import_node_types::VARIABLE_DECLARATOR => extract_require_imports(node, current_scope),
        ts_import_node_types::CALL_EXPRESSION => extract_side_effect_imports(node, current_scope),
        _ => TypeScriptImportedSymbolInfoVec::new(),
    }
}

/// Extract imports from import statements (ES6 style)
fn extract_import_statement(
    node: &Node<StrDoc<SupportLang>>,
    current_scope: &TypeScriptScopeStack,
) -> TypeScriptImportedSymbolInfoVec {
    let mut imports = TypeScriptImportedSymbolInfoVec::new();

    // Check for import express = require('express') e.g import_require_clause
    if let Some(import_require) = node
        .children()
        .find(|child| child.kind() == ts_import_node_types::IMPORT_REQUIRE_CLAUSE)
    {
        // Extract source and identifier directly from children
        if let (Some(name_node), Some(source_node)) = (
            import_require
                .children()
                .find(|child| child.kind() == ts_import_node_types::IDENTIFIER),
            import_require
                .children()
                .find(|child| child.kind() == ts_import_node_types::STRING),
        ) {
            let source_path = clean_source_string(&source_node.text());
            let identifier = ImportIdentifier {
                name: name_node.text().to_string(),
                alias: None,
            };
            imports.push(create_import_info(
                TypeScriptImportType::ImportAndRequire,
                source_path,
                Some(identifier),
                &name_node,
                current_scope,
            ));
        }
        return imports;
    }

    let source_path = if let Some(source_node) = node.field("source") {
        clean_source_string(&source_node.text())
    } else {
        return imports;
    };

    let import_clause = node
        .children()
        .find(|child| child.kind() == ts_import_node_types::IMPORT_CLAUSE);

    // Check for side effect imports e.g import 'react'
    if import_clause.is_none() {
        imports.push(create_import_info(
            TypeScriptImportType::SideEffectImport,
            source_path,
            None,
            node,
            current_scope,
        ));
        return imports;
    }

    if let Some(import_clause) = import_clause {
        // Handle import-require syntax: import express = require('express') e.g import_require_clause
        if let Some(import_require) = import_clause.field("import_require_clause") {
            if let Some(name_node) = import_require.field("name") {
                let identifier = ImportIdentifier {
                    name: name_node.text().to_string(),
                    alias: None,
                };
                imports.push(create_import_info(
                    TypeScriptImportType::ImportAndRequire,
                    source_path,
                    Some(identifier),
                    &name_node,
                    current_scope,
                ));
            }
            return imports;
        }

        for child in import_clause.children() {
            match child.kind().as_ref() {
                ts_import_node_types::IDENTIFIER => {
                    // Handle default imports e.g import React from 'react' e.g identifier
                    let identifier = ImportIdentifier {
                        name: child.text().to_string(),
                        alias: None,
                    };
                    imports.push(create_import_info(
                        TypeScriptImportType::DefaultImport,
                        source_path.clone(),
                        Some(identifier),
                        &child,
                        current_scope,
                    ));
                }
                ts_import_node_types::NAMESPACE_IMPORT => {
                    // Namespace import: import * as React from 'react' e.g namespace_import
                    if let Some(alias_node) = child
                        .children()
                        .find(|grandchild| grandchild.kind() == ts_import_node_types::IDENTIFIER)
                    {
                        let identifier = ImportIdentifier {
                            name: "*".to_string(),
                            alias: Some(alias_node.text().to_string()),
                        };
                        imports.push(create_import_info(
                            TypeScriptImportType::NamespaceImport,
                            source_path.clone(),
                            Some(identifier),
                            &alias_node,
                            current_scope,
                        ));
                    }
                }
                ts_import_node_types::NAMED_IMPORTS => {
                    // Named imports: import { useState, useEffect } from 'react'
                    imports.extend(extract_named_imports(&child, &source_path, current_scope));
                }
                _ => {}
            }
        }
    }

    imports
}

/// Extract named imports from a named_imports node
fn extract_named_imports(
    named_imports_node: &Node<StrDoc<SupportLang>>,
    source_path: &str,
    current_scope: &TypeScriptScopeStack,
) -> TypeScriptImportedSymbolInfoVec {
    let mut imports = TypeScriptImportedSymbolInfoVec::new();

    for child in named_imports_node.children() {
        if child.kind() == ts_import_node_types::IMPORT_SPECIFIER
            && let Some(name_node) = child.field("name")
        {
            let name = name_node.text().to_string();
            let alias = child
                .field("alias")
                .map(|alias_node| alias_node.text().to_string());

            let (import_type, identifier_node) = if alias.is_some() {
                (
                    TypeScriptImportType::AliasedImport,
                    child.field("alias").unwrap(),
                )
            } else {
                (TypeScriptImportType::NamedImport, name_node)
            };

            let identifier = ImportIdentifier { name, alias };
            imports.push(create_import_info(
                import_type,
                source_path.to_string(),
                Some(identifier),
                &identifier_node,
                current_scope,
            ));
        }
    }

    imports
}

/// Extract require-style imports from variable declarations
fn extract_require_imports(
    node: &Node<StrDoc<SupportLang>>,
    current_scope: &TypeScriptScopeStack,
) -> TypeScriptImportedSymbolInfoVec {
    let mut imports = TypeScriptImportedSymbolInfoVec::new();

    // Check if the value is a require/import call
    if let Some(value_node) = node.field("value") {
        let source_path = match extract_source_from_call(&value_node) {
            Some(source) => source,
            None => return imports,
        };

        // Check the name pattern to determine import type
        if let Some(name_node) = node.field("name") {
            match name_node.kind().as_ref() {
                ts_import_node_types::IDENTIFIER => {
                    // Simple assignment: const fs = require('fs')
                    let identifier = ImportIdentifier {
                        name: name_node.text().to_string(),
                        alias: None,
                    };
                    imports.push(create_import_info(
                        TypeScriptImportType::SvaRequireOrImport,
                        source_path,
                        Some(identifier),
                        &name_node,
                        current_scope,
                    ));
                }
                ts_import_node_types::OBJECT_PATTERN => {
                    // Destructuring: const { readFile } = require('fs')
                    imports.extend(extract_destructured_imports(
                        &name_node,
                        &source_path,
                        current_scope,
                    ));
                }
                _ => {}
            }
        }
    }

    imports
}

/// Extract destructured imports from object patterns
fn extract_destructured_imports(
    object_pattern: &Node<StrDoc<SupportLang>>,
    source_path: &str,
    current_scope: &TypeScriptScopeStack,
) -> TypeScriptImportedSymbolInfoVec {
    let mut imports = TypeScriptImportedSymbolInfoVec::new();

    for child in object_pattern.children() {
        match child.kind().as_ref() {
            ts_import_node_types::SHORTHAND_PROPERTY_IDENTIFIER_PATTERN => {
                // Shorthand: { readFile }
                let identifier = ImportIdentifier {
                    name: child.text().to_string(),
                    alias: None,
                };
                imports.push(create_import_info(
                    TypeScriptImportType::DestructuredImportOrRequire,
                    source_path.to_string(),
                    Some(identifier),
                    &child,
                    current_scope,
                ));
            }
            ts_import_node_types::PAIR_PATTERN => {
                // Aliased: { readFile: fsRead }
                if let (Some(key_node), Some(value_node)) =
                    (child.field("key"), child.field("value"))
                {
                    let identifier = ImportIdentifier {
                        name: key_node.text().to_string(),
                        alias: Some(value_node.text().to_string()),
                    };
                    imports.push(create_import_info(
                        TypeScriptImportType::AliasedImportOrRequire,
                        source_path.to_string(),
                        Some(identifier),
                        &value_node,
                        current_scope,
                    ));
                }
            }
            _ => {}
        }
    }

    imports
}

/// Extract side effect imports from call expressions
fn extract_side_effect_imports(
    node: &Node<StrDoc<SupportLang>>,
    current_scope: &TypeScriptScopeStack,
) -> TypeScriptImportedSymbolInfoVec {
    let mut imports = TypeScriptImportedSymbolInfoVec::new();

    // Check if it's a standalone require/import call (not assigned to a variable)
    if let Some(function_node) = node.field("function") {
        let function_name = function_node.text();
        if matches!(function_name.as_ref(), "require" | "import") {
            // Check if it's not inside a variable_declarator or assignment
            if !is_inside_assignment(node)
                && let Some(args) = node.field("arguments")
            {
                for arg in args.children() {
                    if arg.kind() == ts_import_node_types::STRING {
                        let source_path = clean_source_string(&arg.text());
                        imports.push(create_import_info(
                            TypeScriptImportType::SideEffectImportOrRequire,
                            source_path,
                            None,
                            node,
                            current_scope,
                        ));
                        break;
                    }
                }
            }
        }
    }

    imports
}

/// Helper to extract source path from call expressions (handles await expressions)
fn extract_source_from_call(node: &Node<StrDoc<SupportLang>>) -> Option<String> {
    // Handle await expression by looking at its argument
    let target_node = if node.kind() == ts_import_node_types::AWAIT_EXPRESSION {
        // Find the call_expression child (should be the second child after 'await')
        node.children()
            .find(|child| child.kind() == ts_import_node_types::CALL_EXPRESSION)?
    } else {
        node.clone()
    };

    // Check if it's a call expression
    if target_node.kind() == ts_import_node_types::CALL_EXPRESSION
        && let Some(function_node) = target_node.field("function")
    {
        let function_name = function_node.text();
        if matches!(function_name.as_ref(), "require" | "import")
            && let Some(args) = target_node.field("arguments")
        {
            for arg in args.children() {
                if arg.kind() == ts_import_node_types::STRING {
                    let source_path = clean_source_string(&arg.text());
                    return Some(source_path);
                }
            }
        }
    }

    None
}

/// Check if a node is inside an assignment context
fn is_inside_assignment(node: &Node<StrDoc<SupportLang>>) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        match parent.kind().as_ref() {
            ts_import_node_types::VARIABLE_DECLARATOR => return true,
            ts_import_node_types::ASSIGNMENT_EXPRESSION => return true,
            _ => current = parent.parent(),
        }
    }
    false
}

/// Create a TypeScriptImportedSymbolInfo from the given parameters
fn create_import_info(
    import_type: TypeScriptImportType,
    import_path: String,
    identifier: Option<ImportIdentifier>,
    node: &Node<StrDoc<SupportLang>>,
    current_scope: &TypeScriptScopeStack,
) -> TypeScriptImportedSymbolInfo {
    let range = node_to_range(node);
    let scope = if current_scope.is_empty() {
        None
    } else {
        Some(TypeScriptFqn::new(current_scope.clone()))
    };

    TypeScriptImportedSymbolInfo {
        import_type,
        import_path,
        identifier,
        range,
        scope,
    }
}

/// Helper function to clean source strings by removing quotes
fn clean_source_string(source: &str) -> String {
    source
        .trim_start_matches(['\'', '"'])
        .trim_end_matches(['\'', '"'])
        .to_string()
}

#[cfg(test)]
mod import_tests {
    use super::*;
    use crate::parser::{GenericParser, LanguageParser, SupportedLanguage};
    use crate::typescript::ast::parse_ast;
    use rustc_hash::FxHashSet;

    fn test_import_extraction(
        code: &str,
        expected_imported_symbols: Vec<(TypeScriptImportType, &str, Option<ImportIdentifier>)>,
        description: &str,
    ) {
        println!("\n=== Testing: {description} ===");
        println!("Code snippet:\n{code}");

        let parser = GenericParser::default_for_language(SupportedLanguage::TypeScript);
        let parse_result = parser.parse(code, Some("test.ts")).unwrap();

        let (_, imports, _) = parse_ast(&parse_result.ast);

        for symbol in &imports {
            println!("symbol: {symbol:?}");
        }

        assert_eq!(
            imports.len(),
            expected_imported_symbols.len(),
            "Expected {} imported symbols, found {}",
            expected_imported_symbols.len(),
            imports.len()
        );

        let ranges = FxHashSet::from_iter(imports.iter().map(|i| i.range.byte_offset));
        assert_eq!(
            ranges.len(),
            imports.len(),
            "Imported symbols have duplicate ranges"
        );

        println!("Found {} imported symbols:", imports.len());
        for (expected_type, expected_path, expected_identifier) in expected_imported_symbols {
            let _matching_symbol = imports
                .iter()
                .find(|i| {
                    i.import_type == expected_type
                        && i.import_path == expected_path
                        && i.identifier == expected_identifier.clone()
                })
                .unwrap_or_else(|| {
                    panic!(
                        "Could not find: type={:?}, path={}, name={:?}, alias={:?}",
                        expected_type,
                        expected_path,
                        expected_identifier.as_ref().unwrap().name,
                        expected_identifier.as_ref().unwrap().alias
                    )
                });

            if expected_identifier.is_some() {
                println!(
                    "Found: type={:?}, path={}, name={:?}, alias={:?}",
                    expected_type,
                    expected_path,
                    expected_identifier.as_ref().unwrap().name,
                    expected_identifier.as_ref().unwrap().alias
                );
            } else {
                println!(
                    "Found: type={:?}, path={}, name={:?}, alias={:?}",
                    expected_type, expected_path, "None", "None"
                );
            }
        }
        println!("✅ All assertions passed for: {description}\n");
    }

    #[test]
    fn test_import_extraction_typescript_code() -> crate::Result<()> {
        let path = "src/typescript/fixtures/typescript/imports.ts";
        let code = std::fs::read_to_string(path)?;
        let parser = GenericParser::default_for_language(SupportedLanguage::TypeScript);
        let parse_result = parser.parse(&code, Some("test.ts")).unwrap();

        let (_, imports, _) = parse_ast(&parse_result.ast);
        for symbol in &imports {
            println!(
                "Found: type={:?}, path={}, name={:?}, alias={:?}",
                symbol.import_type, symbol.import_path, "None", "None"
            );
            let range = symbol.range;
            println!("Imported symbol range: {range:?}");
        }

        Ok(())
    }

    #[test]
    fn test_mixed_imports() -> crate::Result<()> {
        let code = r#"
        import React, { useState, useEffect, banana as apple } from 'react';
        "#;
        let expected_imported_symbols = vec![
            (
                TypeScriptImportType::DefaultImport,
                "react",
                Some(ImportIdentifier {
                    name: "React".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::NamedImport,
                "react",
                Some(ImportIdentifier {
                    name: "useState".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::NamedImport,
                "react",
                Some(ImportIdentifier {
                    name: "useEffect".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::AliasedImport,
                "react",
                Some(ImportIdentifier {
                    name: "banana".to_string(),
                    alias: Some("apple".to_string()),
                }),
            ),
        ];
        test_import_extraction(code, expected_imported_symbols, "Mixed imports");
        Ok(())
    }

    #[test]
    fn test_sva_require_or_import() -> crate::Result<()> {
        let code = r#"
        const VAR_NAME_1 = await require("hello1");
        const VAR_NAME_2 = require("hello2");
        const VAR_NAME_3 = import("hello3");
        const VAR_NAME_4 = await import("hello4");
        "#;
        let expected_imported_symbols = vec![
            (
                TypeScriptImportType::SvaRequireOrImport,
                "hello1",
                Some(ImportIdentifier {
                    name: "VAR_NAME_1".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::SvaRequireOrImport,
                "hello2",
                Some(ImportIdentifier {
                    name: "VAR_NAME_2".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::SvaRequireOrImport,
                "hello3",
                Some(ImportIdentifier {
                    name: "VAR_NAME_3".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::SvaRequireOrImport,
                "hello4",
                Some(ImportIdentifier {
                    name: "VAR_NAME_4".to_string(),
                    alias: None,
                }),
            ),
        ];
        test_import_extraction(code, expected_imported_symbols, "SvaRequireOrImport");
        Ok(())
    }

    #[test]
    fn test_import_with_assignment() -> crate::Result<()> {
        let code = r#"
        import express = require('express');
        "#;
        let expected_imported_symbols = vec![(
            TypeScriptImportType::ImportAndRequire,
            "express",
            Some(ImportIdentifier {
                name: "express".to_string(),
                alias: None,
            }),
        )];
        test_import_extraction(code, expected_imported_symbols, "Import with assignment");
        Ok(())
    }

    #[test]
    fn test_advanced_import_or_require() -> crate::Result<()> {
        // const { NAME: ALIAS } = opt<await> either(require('SOURCE'), import('SOURCE'))
        // const { NAME } = opt<await> either(require('SOURCE'), import('SOURCE'))
        let code = r#"
        const { VAR_NAME_1: ALIAS_1 } = await require("hello1");
        const { VAR_NAME_2: ALIAS_2 } = require("hello2");
        const { VAR_NAME_3: ALIAS_3 } = import("hello3");
        const { VAR_NAME_4: ALIAS_4 } = await import("hello4");
        const { VAR_NAME_5 } = await require("hello5");
        const { VAR_NAME_6 } = require("hello6");
        const { VAR_NAME_7 } = import("hello7");
        const { VAR_NAME_8 } = await import("hello8");
        const { VAR_NAME_9: ALIAS_9, VAR_NAME_10: ALIAS_10 } = await require("hello9");
        "#;
        let expected_imported_symbols = vec![
            (
                TypeScriptImportType::AliasedImportOrRequire,
                "hello1",
                Some(ImportIdentifier {
                    name: "VAR_NAME_1".to_string(),
                    alias: Some("ALIAS_1".to_string()),
                }),
            ),
            (
                TypeScriptImportType::AliasedImportOrRequire,
                "hello2",
                Some(ImportIdentifier {
                    name: "VAR_NAME_2".to_string(),
                    alias: Some("ALIAS_2".to_string()),
                }),
            ),
            (
                TypeScriptImportType::AliasedImportOrRequire,
                "hello3",
                Some(ImportIdentifier {
                    name: "VAR_NAME_3".to_string(),
                    alias: Some("ALIAS_3".to_string()),
                }),
            ),
            (
                TypeScriptImportType::AliasedImportOrRequire,
                "hello4",
                Some(ImportIdentifier {
                    name: "VAR_NAME_4".to_string(),
                    alias: Some("ALIAS_4".to_string()),
                }),
            ),
            (
                TypeScriptImportType::DestructuredImportOrRequire,
                "hello5",
                Some(ImportIdentifier {
                    name: "VAR_NAME_5".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::DestructuredImportOrRequire,
                "hello6",
                Some(ImportIdentifier {
                    name: "VAR_NAME_6".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::DestructuredImportOrRequire,
                "hello7",
                Some(ImportIdentifier {
                    name: "VAR_NAME_7".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::DestructuredImportOrRequire,
                "hello8",
                Some(ImportIdentifier {
                    name: "VAR_NAME_8".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::AliasedImportOrRequire,
                "hello9",
                Some(ImportIdentifier {
                    name: "VAR_NAME_9".to_string(),
                    alias: Some("ALIAS_9".to_string()),
                }),
            ),
            (
                TypeScriptImportType::AliasedImportOrRequire,
                "hello9",
                Some(ImportIdentifier {
                    name: "VAR_NAME_10".to_string(),
                    alias: Some("ALIAS_10".to_string()),
                }),
            ),
        ];
        test_import_extraction(
            code,
            expected_imported_symbols,
            "Advanced import or require",
        );
        Ok(())
    }

    #[test]
    fn test_dynamic_imports() -> crate::Result<()> {
        let code = r#"
        await import('reflect-metadata-3');
        import('reflect-metadata-4');
        await require('reflect-metadata-5');
        require('reflect-metadata-6');
        "#;
        let expected_imported_symbols = vec![
            (
                TypeScriptImportType::SideEffectImportOrRequire,
                "reflect-metadata-3",
                None,
            ),
            (
                TypeScriptImportType::SideEffectImportOrRequire,
                "reflect-metadata-4",
                None,
            ),
            (
                TypeScriptImportType::SideEffectImportOrRequire,
                "reflect-metadata-5",
                None,
            ),
            (
                TypeScriptImportType::SideEffectImportOrRequire,
                "reflect-metadata-6",
                None,
            ),
        ];
        test_import_extraction(code, expected_imported_symbols, "Dynamic imports");
        Ok(())
    }

    #[test]
    fn test_require_imports_advanced() -> crate::Result<()> {
        let code = r#"
        const APP = require('SOURCE_1');
        const { BigApple } = require('SOURCE_2');
        const { MyApp : MyAppAlias } = require('SOURCE_3');
        const { MyApp : MyAppAlias, BigApple } = require('SOURCE_4');
        "#;
        let expected_imported_symbols = vec![
            (
                TypeScriptImportType::SvaRequireOrImport,
                "SOURCE_1",
                Some(ImportIdentifier {
                    name: "APP".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::DestructuredImportOrRequire,
                "SOURCE_2",
                Some(ImportIdentifier {
                    name: "BigApple".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::AliasedImportOrRequire,
                "SOURCE_3",
                Some(ImportIdentifier {
                    name: "MyApp".to_string(),
                    alias: Some("MyAppAlias".to_string()),
                }),
            ),
            (
                TypeScriptImportType::AliasedImportOrRequire,
                "SOURCE_4",
                Some(ImportIdentifier {
                    name: "MyApp".to_string(),
                    alias: Some("MyAppAlias".to_string()),
                }),
            ),
            (
                TypeScriptImportType::DestructuredImportOrRequire,
                "SOURCE_4",
                Some(ImportIdentifier {
                    name: "BigApple".to_string(),
                    alias: None,
                }),
            ),
        ];
        test_import_extraction(code, expected_imported_symbols, "Require imports");
        Ok(())
    }

    #[test]
    fn test_side_effect_imports_all() -> crate::Result<()> {
        let code = r#"
        import 'reflect-metadata';
        require('reflect-metadata-2');
        "#;
        let expected_imported_symbols = vec![
            (
                TypeScriptImportType::SideEffectImport,
                "reflect-metadata",
                None,
            ),
            (
                TypeScriptImportType::SideEffectImportOrRequire,
                "reflect-metadata-2",
                None,
            ),
        ];
        test_import_extraction(code, expected_imported_symbols, "Side effect imports");
        Ok(())
    }

    #[test]
    fn test_namespace_imports() {
        let code = r#"
        import * as React from 'react';
        import * as fs from 'fs';
        "#;
        let expected_imported_symbols = vec![
            (
                TypeScriptImportType::NamespaceImport,
                "react",
                Some(ImportIdentifier {
                    name: "*".to_string(),
                    alias: Some("React".to_string()),
                }),
            ),
            (
                TypeScriptImportType::NamespaceImport,
                "fs",
                Some(ImportIdentifier {
                    name: "*".to_string(),
                    alias: Some("fs".to_string()),
                }),
            ),
        ];
        test_import_extraction(code, expected_imported_symbols, "Namespace imports");
    }

    #[test]
    fn test_type_only_imports() {
        let code = r#"
        import type { FC } from 'react';
        import type React from 'react';
        import { type FC2 } from 'react';
        import { type FC3, type FC4 } from 'react';
        import type { FC5, FC6 } from 'react';
        import type { FC7 as FC8 } from 'react';
        import { type FC9 as FC10 } from 'react';
        "#;
        let expected_imported_symbols = vec![
            (
                TypeScriptImportType::NamedImport,
                "react",
                Some(ImportIdentifier {
                    name: "FC".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::DefaultImport,
                "react",
                Some(ImportIdentifier {
                    name: "React".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::NamedImport,
                "react",
                Some(ImportIdentifier {
                    name: "FC2".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::NamedImport,
                "react",
                Some(ImportIdentifier {
                    name: "FC3".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::NamedImport,
                "react",
                Some(ImportIdentifier {
                    name: "FC4".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::NamedImport,
                "react",
                Some(ImportIdentifier {
                    name: "FC5".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::NamedImport,
                "react",
                Some(ImportIdentifier {
                    name: "FC6".to_string(),
                    alias: None,
                }),
            ),
            (
                TypeScriptImportType::AliasedImport,
                "react",
                Some(ImportIdentifier {
                    name: "FC7".to_string(),
                    alias: Some("FC8".to_string()),
                }),
            ),
            (
                TypeScriptImportType::AliasedImport,
                "react",
                Some(ImportIdentifier {
                    name: "FC9".to_string(),
                    alias: Some("FC10".to_string()),
                }),
            ),
        ];
        test_import_extraction(code, expected_imported_symbols, "Type only imports");
    }
}
