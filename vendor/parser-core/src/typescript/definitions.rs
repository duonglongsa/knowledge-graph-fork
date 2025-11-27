use crate::typescript::types::{
    TypeScriptDefinitionInfo, TypeScriptDefinitionType, TypeScriptFqn, TypeScriptFqnPart,
    TypeScriptNodeFqnMap, TypeScriptScopeStack,
};
use crate::utils::node_to_range;
use ast_grep_core::Node;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_language::SupportLang;

pub fn find_definitions(js_node_fqn_map: &TypeScriptNodeFqnMap) -> Vec<TypeScriptDefinitionInfo> {
    let mut definitions = Vec::new();
    for (_range, (node, fqn)) in js_node_fqn_map.iter() {
        let last_fqn_part = fqn.last().unwrap();
        let definition_type = last_fqn_part.node_type;
        definitions.push(TypeScriptDefinitionInfo::new(
            definition_type,
            node.text().to_string(),
            fqn.clone(),
            last_fqn_part.range,
        ));
    }

    definitions
}

/// Process a node and returns the FQN part if the node creates a scope
pub fn process_definition_node<'a>(
    node: Node<'a, StrDoc<SupportLang>>,
    current_scope: &mut TypeScriptScopeStack,
    node_fqn_map: &mut TypeScriptNodeFqnMap<'a>,
) -> Option<TypeScriptFqnPart> {
    let node_kind = node.kind();
    let def_type = TypeScriptDefinitionType::from_node_kind(node_kind.as_ref());
    // Will return None if the node kind is not a definition type
    def_type?;
    let def_type = def_type.unwrap();

    match def_type {
        // Handle anonymous classes with a name assignment E.g with `const foo = class {}`
        TypeScriptDefinitionType::NamedClassExpression => {
            if let Some(parent) = node.parent() {
                let parent_kind = parent.kind();
                let name_node = if parent_kind == "variable_declarator"
                    || parent_kind == "public_field_definition"
                {
                    parent.field("name")
                } else {
                    None
                };

                if let Some(name_node) = name_node {
                    return handle_named_definition_node(
                        &name_node,
                        &node,
                        def_type,
                        current_scope,
                        node_fqn_map,
                    );
                }
            }
        }

        // Handle named declarations E.g with `* {}`
        TypeScriptDefinitionType::Class
        | TypeScriptDefinitionType::Function
        | TypeScriptDefinitionType::NamedGeneratorFunctionDeclaration
        | TypeScriptDefinitionType::Enum => {
            if let Some(name_node) = node.field("name") {
                return handle_named_definition_node(
                    &name_node,
                    &node,
                    def_type,
                    current_scope,
                    node_fqn_map,
                );
            }
        }

        // Only capture methods that are inside a class body, not object literals
        TypeScriptDefinitionType::Method => {
            if let Some(name_node) = node.field("name")
                && let Some(parent) = node.parent()
                && let Some(grandparent) = parent.parent()
            {
                // Check if we're inside a class_body -> class_declaration
                if parent.kind() == "class_body" && grandparent.kind() == "class_declaration" {
                    return handle_named_definition_node(
                        &name_node,
                        &node,
                        def_type,
                        current_scope,
                        node_fqn_map,
                    );
                }
            }
        }

        // Handle anonymous declarations with a name assignment E.g with `() => {}`
        TypeScriptDefinitionType::NamedArrowFunction
        | TypeScriptDefinitionType::NamedFunctionExpression
        | TypeScriptDefinitionType::NamedGeneratorFunctionExpression => {
            if let Some(parent) = node.parent() {
                let parent_kind = parent.kind();
                let name_node = if parent_kind == "variable_declarator"
                    || parent_kind == "public_field_definition"
                {
                    parent.field("name")
                } else {
                    None
                };
                if let Some(name_node) = name_node {
                    return handle_named_definition_node(
                        &name_node,
                        &parent,
                        def_type,
                        current_scope,
                        node_fqn_map,
                    );
                }
            }
        }

        // Capture type declarations
        TypeScriptDefinitionType::Type
        | TypeScriptDefinitionType::Interface
        | TypeScriptDefinitionType::Namespace => {
            if let Some(name_node) = node.field("name") {
                return handle_named_definition_node(
                    &name_node,
                    &node,
                    def_type,
                    current_scope,
                    node_fqn_map,
                );
            }
        }

        _ => {}
    }

    None
}

fn compute_definition_metadata<'a>(
    name_node: &Node<'a, StrDoc<SupportLang>>,
    definition_type: TypeScriptDefinitionType,
) -> Option<&'a str> {
    if definition_type == TypeScriptDefinitionType::Method {
        let parent = name_node.parent().unwrap();
        if parent.text().starts_with("get ") {
            // println!("Parent text: {:?}", parent.text());
            return Some("METHOD_GETTER");
        }
        if parent.text().starts_with("set ") {
            // println!("Parent text: {:?}", parent.text());
            return Some("METHOD_SETTER");
        }
    }
    None
}

/// Helper function to create and insert FQN parts for a named node
fn handle_named_definition_node<'a>(
    name_node: &Node<'a, StrDoc<SupportLang>>,
    definition_node: &Node<'a, StrDoc<SupportLang>>,
    definition_type: TypeScriptDefinitionType,
    current_scope: &mut TypeScriptScopeStack,
    node_fqn_map: &mut TypeScriptNodeFqnMap<'a>,
) -> Option<TypeScriptFqnPart> {
    let name = name_node.text();
    let mut fqn_parts = current_scope.to_owned();

    // TODO: Add metadata to FQNPart
    let _metadata = compute_definition_metadata(name_node, definition_type);
    let new_part = TypeScriptFqnPart::new(
        definition_type,
        name.to_string(),
        node_to_range(definition_node),
    );

    fqn_parts.push(new_part.clone());

    let name_range = node_to_range(name_node);
    let fqn = TypeScriptFqn::new(fqn_parts);
    node_fqn_map.insert(name_range, (name_node.clone(), fqn));

    Some(new_part)
}

#[cfg(test)]
mod definition_tests {
    use super::*;
    use crate::parser::{GenericParser, LanguageParser, SupportedLanguage};
    use crate::typescript::ast::{parse_ast, typescript_fqn_to_string};
    use crate::typescript::types::TypeScriptDefinitionType;

    /// Helper function to test a definition type with a code snippet
    fn test_definition_extraction(
        code: &str,
        expected_definitions: Vec<(&str, TypeScriptDefinitionType, &str)>, // (name, type, expected_fqn)
        description: &str,
    ) {
        println!("\n=== Testing: {description} ===");
        println!("Code snippet:\n{code}");

        let parser = GenericParser::default_for_language(SupportedLanguage::TypeScript);
        let parse_result = parser.parse(code, Some("test.ts")).unwrap();
        let (typescript_node_fqn_map, _, _) = parse_ast(&parse_result.ast);
        let definitions = find_definitions(&typescript_node_fqn_map);

        println!("Found {} definitions:", definitions.len());
        for def in &definitions {
            let fqn_str = typescript_fqn_to_string(&def.fqn);
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
            let actual_fqn_str = typescript_fqn_to_string(actual_fqn);
            assert_eq!(
                actual_fqn_str, expected_fqn,
                "FQN mismatch for {expected_name}: expected '{expected_fqn}', got '{actual_fqn_str}'"
            );
        }
        println!("✅ All assertions passed for: {description}\n");
    }

    #[test]
    #[ignore]
    fn test_named_call_expression_extraction() {
        test_definition_extraction(
            "
              export const POST = withSession(async ({ session }) => {
                const [user, _] = await Promise.all([
                  prisma.user.update({
                    where: {
                      id: session.user.id,
                    },
                    data: {
                      subscribed: true,
                    },
                    select: {
                      id: true,
                      name: true,
                      email: true,
                      subscribed: true,
                    },
                  }),
                  subscribe({ email: session.user.email, name: session.user.name }),
                ]);

                return NextResponse.json(user);
              });",
            vec![(
                "POST",
                TypeScriptDefinitionType::NamedCallExpression,
                "POST",
            )],
            "Named call expression extraction",
        );
    }
}
