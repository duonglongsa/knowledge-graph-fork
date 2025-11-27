use crate::typescript::definitions::process_definition_node;
use crate::typescript::imports::detect_import_declaration;
use crate::typescript::references::types::TypeScriptExpression;
use crate::typescript::types::{TypeScriptFqn, TypeScriptNodeFqnMap};
use crate::typescript::types::{TypeScriptImportedSymbolInfoVec, TypeScriptScopeStack};
use crate::utils::Range;

use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_core::{AstGrep, Node};
use ast_grep_language::SupportLang;
use rustc_hash::FxHashMap;
use smallvec::smallvec;

pub const TS_FQN_SEPARATOR: &str = "::";

/// FQN indexing and computation functionality for JS/TS
/// This handles the core logic for building FQN maps from AST traversal
/// Returns a tuple of (js_node_fqn_map, imports) where:
/// - js_node_fqn_map: Maps byte ranges to (Node, FQN parts with metadata)
/// - imports: List of imported symbols detected directly from AST
pub fn parse_ast<'a>(
    ast: &'a AstGrep<StrDoc<SupportLang>>,
) -> (
    TypeScriptNodeFqnMap<'a>,
    TypeScriptImportedSymbolInfoVec,
    Vec<TypeScriptExpression>,
) {
    let mut node_fqn_map = FxHashMap::default();
    let mut current_scope: TypeScriptScopeStack = smallvec![]; // Start with empty scope
    let mut imports: TypeScriptImportedSymbolInfoVec = smallvec![];
    let mut expressions: Vec<TypeScriptExpression> = vec![];

    // Stack of nodes to process
    let mut stack: Vec<Option<Node<StrDoc<SupportLang>>>> = vec![];
    stack.push(Some(ast.root()));

    while let Some(node_option) = stack.pop() {
        if let Some(node) = node_option {
            let children: Vec<_> = node.children().collect();
            stack.reserve(children.len());

            // Check if this node is an expression
            let expression = TypeScriptExpression::from_node(&node);
            if expression.valid() {
                expressions.push(expression);
            }

            // Check if this node is an import declaration
            let detected_imports = detect_import_declaration(&node, &current_scope);
            imports.extend(detected_imports);

            if let Some(new_scope) =
                process_definition_node(node, &mut current_scope, &mut node_fqn_map)
            {
                current_scope.push(new_scope);
                stack.push(None); // Sentinel indicating end of scope
            }

            // Push children in reverse order to maintain left-to-right traversal
            for child in children.into_iter().rev() {
                stack.push(Some(child));
            }
        } else {
            // Sentinel found - pop from scope stack
            current_scope.pop();
        }
    }

    (node_fqn_map, imports, expressions)
}

/// Converts a TypeScript FQN to a string by joining node names with '::'
pub fn typescript_fqn_to_string(fqn: &TypeScriptFqn) -> String {
    if fqn.is_empty() {
        return String::new();
    }
    fqn.as_ref()
        .iter()
        .map(|part| part.node_name().to_string())
        .collect::<Vec<_>>()
        .join(TS_FQN_SEPARATOR)
}

/// Find TypeScript FQN with metadata
pub fn find_typescript_fqn_for_node<'a>(
    range: &Range,
    node_fqn_map: &TypeScriptNodeFqnMap<'a>,
) -> Option<TypeScriptFqn> {
    node_fqn_map
        .get(range)
        .map(|(_, fqn_parts)| fqn_parts.clone())
}
