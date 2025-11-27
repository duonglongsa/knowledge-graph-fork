use std::{collections::HashSet, sync::LazyLock};

use crate::typescript::references::types::{
    ExpressionModifiers, ExpressionSymbolInfo, TypeScriptAnnotatedSymbol, TypeScriptExpression,
    TypeScriptReferenceMetadata, TypeScriptSymbolType,
};
use crate::utils::{Range, node_to_range};
use ast_grep_core::Node;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_language::SupportLang;

// CONSTANTS
static ASSIGNMENT_OPERATORS: LazyLock<HashSet<String>> = LazyLock::new(|| {
    HashSet::from_iter(
        [
            "=", "+=", "-=", "*=", "/=", "%=", "**=", "<<=", ">>=", ">>>=", "&=", "^=", "|=",
            "&&=", "||=", "??=",
        ]
        .map(|s| s.to_string()),
    )
});

static DESTRUCTED_DECLARATION_NODES: LazyLock<HashSet<String>> = LazyLock::new(|| {
    HashSet::from_iter(["object_pattern", "array_pattern"].map(|s| s.to_string()))
});

pub struct ExpressionSymbolUtils;

impl ExpressionSymbolUtils {
    #[inline]
    pub fn contains_call_expression(node: &Node<StrDoc<SupportLang>>) -> bool {
        for child in node.children() {
            if child.kind() == "call_expression" {
                return true;
            }
        }
        false
    }

    #[inline]
    pub fn extract_chained_method_info(
        call_node: &Node<StrDoc<SupportLang>>,
    ) -> Option<(String, Range, TypeScriptReferenceMetadata)> {
        // For a chained call, find the property identifier node and extract its name and range
        let func_node = call_node.field("function")?;

        let mut method_name = None;
        let mut method_range = None;

        // The function should be a member_expression, find the property identifier
        for child in func_node.children() {
            if child.kind() == "property_identifier" {
                method_name = Some(child.text().to_string());
                method_range = Some(node_to_range(&child));
                break;
            }
        }

        if let (Some(name), Some(range)) = (method_name, method_range) {
            // Extract arguments from the call
            let args = Self::extract_call_arguments(call_node);
            let metadata = TypeScriptReferenceMetadata::Call {
                is_async: false, // Will be set at higher level
                is_super: false, // Will be set at higher level
                is_this: false,  // Will be set at higher level
                args,
            };

            Some((name, range, metadata))
        } else {
            None
        }
    }

    #[inline]
    pub fn extract_call_arguments(
        call_node: &Node<StrDoc<SupportLang>>,
    ) -> Vec<TypeScriptAnnotatedSymbol> {
        let mut args = Vec::new();
        if let Some(args_node) = call_node.field("arguments") {
            for child in args_node.children() {
                // Skip punctuation like commas and parentheses
                if !matches!(child.kind().as_ref(), "," | "(" | ")") {
                    let (symbol_type, metadata) = Self::analyze_argument_node(&child);
                    args.push(TypeScriptAnnotatedSymbol::new(
                        child.text().to_string(),
                        node_to_range(&child),
                        symbol_type,
                        None,
                        metadata,
                    ));
                }
            }
        }
        args
    }

    #[inline]
    /// Analyze an argument node to determine if it's a callback function and extract metadata
    pub fn analyze_argument_node(
        node: &Node<StrDoc<SupportLang>>,
    ) -> (TypeScriptSymbolType, Option<TypeScriptReferenceMetadata>) {
        match node.kind().as_ref() {
            "arrow_function" => {
                let (parameters, is_async, body) = Self::extract_function_details(node);
                (
                    TypeScriptSymbolType::ArrowFunctionCallback,
                    Some(TypeScriptReferenceMetadata::Callback {
                        parameters,
                        is_async,
                        body,
                    }),
                )
            }
            "function_expression" => {
                let (parameters, is_async, body) = Self::extract_function_details(node);
                let symbol_type = if Self::has_function_name(node) {
                    TypeScriptSymbolType::FunctionExpressionCallback
                } else {
                    TypeScriptSymbolType::AnonymousFunctionCallback
                };
                (
                    symbol_type,
                    Some(TypeScriptReferenceMetadata::Callback {
                        parameters,
                        is_async,
                        body,
                    }),
                )
            }
            "function" => {
                let (parameters, is_async, body) = Self::extract_function_details(node);
                (
                    TypeScriptSymbolType::AnonymousFunctionCallback,
                    Some(TypeScriptReferenceMetadata::Callback {
                        parameters,
                        is_async,
                        body,
                    }),
                )
            }
            _ => {
                // Regular argument (identifier, literal, etc.)
                (TypeScriptSymbolType::Identifier, None)
            }
        }
    }

    #[inline]
    /// Extract function details (parameters, async status, body) from a function node
    fn extract_function_details(
        node: &Node<StrDoc<SupportLang>>,
    ) -> (Vec<String>, bool, Option<String>) {
        let mut parameters = Vec::new();
        let mut is_async = false;
        let mut body = None;

        // Check if function is async
        if let Some(parent) = node.parent() {
            // Check if parent contains async keyword
            for sibling in parent.children() {
                if sibling.kind() == "async" {
                    is_async = true;
                    break;
                }
            }
        }

        // Extract parameters
        if let Some(params_node) = node.field("parameters").or_else(|| node.field("parameter")) {
            Self::extract_parameters_from_node(&params_node, &mut parameters);
        }

        // Extract function body
        if let Some(body_node) = node.field("body") {
            body = Some(body_node.text().to_string());
        }

        (parameters, is_async, body)
    }

    #[inline]
    /// Extract parameter names from a parameters node
    fn extract_parameters_from_node(
        params_node: &Node<StrDoc<SupportLang>>,
        parameters: &mut Vec<String>,
    ) {
        for child in params_node.children() {
            match child.kind().as_ref() {
                "identifier" => {
                    parameters.push(child.text().to_string());
                }
                "required_parameter" | "optional_parameter" => {
                    // For TypeScript parameters with types
                    if let Some(pattern) = child.field("pattern")
                        && pattern.kind() == "identifier"
                    {
                        parameters.push(pattern.text().to_string());
                    }
                }
                "formal_parameters" => {
                    // Nested formal parameters (for cases like (a, b) => ...)
                    Self::extract_parameters_from_node(&child, parameters);
                }
                _ => {
                    // Handle other parameter patterns like destructuring
                    if !matches!(child.kind().as_ref(), "," | "(" | ")") {
                        parameters.push(child.text().to_string());
                    }
                }
            }
        }
    }

    #[inline]
    /// Check if a function expression has a name
    fn has_function_name(node: &Node<StrDoc<SupportLang>>) -> bool {
        node.field("name").is_some()
    }

    #[inline]
    /// Check if this call expression is nested within a larger call chain
    pub fn is_nested_in_chain(node: &Node<StrDoc<SupportLang>>) -> bool {
        // Check if this node is part of a chained call by looking at parent structure
        if let Some(parent) = node.parent() {
            // For chained calls, the structure is: call_expression -> member_expression -> call_expression
            if parent.kind() == "member_expression"
                && let Some(grandparent) = parent.parent()
            {
                return grandparent.kind() == "call_expression";
            }
        }
        false
    }

    #[inline]
    /// Check if this call/new expression is part of an assignment or variable declaration
    pub fn is_part_of_assignment(node: &Node<StrDoc<SupportLang>>) -> bool {
        // Walk up the parent tree to see if we're on the right side of an assignment or variable declaration
        let mut current = node.parent();
        while let Some(parent) = current {
            match parent.kind().as_ref() {
                "assignment_expression" => {
                    // Check if our node is on the right side of the assignment
                    if let Some(right_field) = parent.field("right") {
                        // Check if our node is a descendant of the right field
                        return Self::is_node_descendant_of(&right_field, node);
                    }
                }
                "variable_declarator" => {
                    // Check if our node is in the value (initializer) of the variable declaration
                    if let Some(value_field) = parent.field("value") {
                        // Check if our node is a descendant of the value field
                        return Self::is_node_descendant_of(&value_field, node);
                    }
                }
                _ => {}
            }
            current = parent.parent();
        }
        false
    }

    #[inline]
    /// Helper to check if a node is a descendant of another node
    pub fn is_node_descendant_of(
        ancestor: &Node<StrDoc<SupportLang>>,
        descendant: &Node<StrDoc<SupportLang>>,
    ) -> bool {
        // Direct match
        if ancestor.range() == descendant.range() {
            return true;
        }

        for child in ancestor.dfs() {
            if child.range() == descendant.range() {
                return true;
            }
        }

        false
    }

    #[inline]
    /// Check if a call expression is a require/import call that should be skipped
    pub fn is_require_or_import_call(node: &Node<StrDoc<SupportLang>>) -> bool {
        // Get the function being called (first child)
        if let Some(function_node) = node.children().next() {
            matches!(function_node.text().as_ref(), "require" | "import")
        } else {
            false
        }
    }

    #[inline]
    /// Extract assignment operator from assignment expression
    pub fn extract_assignment_operator(node: &Node<StrDoc<SupportLang>>) -> String {
        // Look for the operator in the assignment expression children
        for child in node.children() {
            let text = child.text();
            if ASSIGNMENT_OPERATORS.contains(text.as_ref()) {
                return text.to_string();
            }
        }
        "=".to_string() // Default to simple assignment
    }

    /// Iteratively extract property accesses and indexing from a member/subscript expression chain
    /// For example, `this.processor[key].loc` would return:
    /// - `this` as MethodCallSource  
    /// - `processor` as Property
    /// - `key` as Index
    /// - `loc` as Property
    pub fn extract_member_chain_symbols(
        node: &Node<StrDoc<SupportLang>>,
        results: &mut Vec<ExpressionSymbolInfo>,
        modifiers: ExpressionModifiers,
        is_first_call: bool,
    ) {
        // Collect all the parts of the member chain by traversing from right to left
        let mut chain_parts = Vec::new();
        let mut current_node = Some(node.clone());

        // Walk the member/subscript expression chain from right to left
        while let Some(node) = current_node {
            match node.kind().as_ref() {
                "member_expression" => {
                    let mut object_node = None;
                    let mut property_node = None;

                    // Extract object and property from member expression
                    for child in node.children() {
                        match child.kind().as_ref() {
                            "identifier"
                            | "member_expression"
                            | "subscript_expression"
                            | "this"
                            | "super"
                            | "new_expression" => {
                                object_node = Some(child);
                            }
                            "property_identifier" => {
                                property_node = Some(child);
                            }
                            _ => {}
                        }
                    }

                    // Add the property to the chain (if it exists)
                    if let Some(property) = property_node {
                        chain_parts.push((property, TypeScriptSymbolType::Property));
                    }

                    // Continue with the object (left side of the member expression)
                    current_node = object_node;
                }
                "subscript_expression" => {
                    let mut object_node = None;
                    let mut index_node = None;

                    // Extract object and index from subscript expression
                    for child in node.children() {
                        match child.kind().as_ref() {
                            "member_expression" | "subscript_expression" | "this" | "super" => {
                                object_node = Some(child);
                            }
                            "identifier" => {
                                if object_node.is_none() {
                                    object_node = Some(child);
                                } else {
                                    index_node = Some(child);
                                }
                            }
                            _ => {
                                // For bracket notation, the index might be under different node types
                                if object_node.is_some() && index_node.is_none() {
                                    index_node = Some(child);
                                }
                            }
                        }
                    }

                    // Add the index to the chain (if it exists)
                    if let Some(index) = index_node {
                        chain_parts.push((index, TypeScriptSymbolType::Index));
                    }

                    // Continue with the object (left side of the subscript expression)
                    current_node = object_node;
                }
                "identifier" | "this" | "super" => {
                    // Base case: add the identifier as the root
                    chain_parts.push((node, TypeScriptSymbolType::MethodCallSource));
                    current_node = None;
                }
                _ => {
                    // For other node types, treat as root
                    chain_parts.push((node, TypeScriptSymbolType::MethodCallSource));
                    current_node = None;
                }
            }
        }

        // Reverse the chain parts to get the correct order (left to right)
        chain_parts.reverse();

        // Add all parts to results
        for (i, (part_node, symbol_type)) in chain_parts.into_iter().enumerate() {
            let is_root = i == 0;
            let metadata = match symbol_type {
                TypeScriptSymbolType::Index => {
                    Some(TypeScriptReferenceMetadata::Index {
                        is_computed: true, // Bracket notation is always computed
                    })
                }
                _ => Some(TypeScriptReferenceMetadata::Call {
                    is_async: if is_root {
                        modifiers.is_await && is_first_call
                    } else {
                        false
                    },
                    is_super: modifiers.is_super,
                    is_this: modifiers.is_this,
                    args: vec![],
                }),
            };

            results.push(ExpressionSymbolInfo {
                range: node_to_range(&part_node),
                name: part_node.text().to_string(),
                symbol_type,
                metadata,
            });
        }
    }
}

impl TypeScriptExpression {
    pub fn valid(&self) -> bool {
        self.range.byte_offset.0 != 0 && self.range.byte_offset.1 != 0
    }

    pub fn from_node(node: &Node<StrDoc<SupportLang>>) -> Self {
        let mut expression = Self::new();

        match node.kind().as_ref() {
            "call_expression" | "new_expression" => {
                // Skip calls that are part of assignments/declarations - those are handled by the assignment path
                if !ExpressionSymbolUtils::is_require_or_import_call(node)
                    && !ExpressionSymbolUtils::is_nested_in_chain(node)
                {
                    expression.range = node_to_range(node);
                    expression.string = node.text().to_string();
                    expression.resolve_symbols(node);

                    // Process LHS to find assignment/declaration targets
                    expression.process_lhs(node);
                }
            }
            "assignment_expression" | "augmented_assignment_expression" => {
                // Only capture assignments that DON'T contain calls/new expressions
                if !Self::assignment_contains_call_or_new(node) {
                    expression.range = node_to_range(node);
                    expression.string = node.text().to_string();
                    Self::process_assignment_expression(node, &mut expression);
                }
            }
            "variable_declarator" => {
                // Only process variable declarations that DON'T contain calls/new expressions
                if let Some(value_node) = node.field("value")
                    && !Self::node_contains_call_or_new(&value_node)
                {
                    expression.range = node_to_range(node);
                    expression.string = node.text().to_string();
                    Self::process_variable_declaration(node, &value_node, &mut expression);
                }
            }
            _ => {}
        }
        expression
    }

    pub fn process_lhs(&mut self, node: &Node<StrDoc<SupportLang>>) {
        // Walk back up the tree to find the assignment/declaration
        // and extract only the target symbols (LHS)
        let mut current = node.parent();
        let mut target_symbols = vec![];

        while let Some(parent) = current {
            match parent.kind().as_ref() {
                "assignment_expression" => {
                    // Check if our node is on the right side of the assignment
                    if let Some(right_field) = parent.field("right") {
                        // Check if our node is a descendant of the right field
                        if ExpressionSymbolUtils::is_node_descendant_of(&right_field, node) {
                            // Extract only the left side (assignment targets)
                            if let Some(left_node) = parent.field("left") {
                                let operator =
                                    ExpressionSymbolUtils::extract_assignment_operator(&parent);
                                Self::extract_targets(&left_node, &mut target_symbols, &operator);
                            }
                            break;
                        }
                    }
                }
                "variable_declarator" => {
                    // Check if our node is in the value (initializer) of the variable declaration
                    if let Some(value_field) = parent.field("value") {
                        // Check if our node is a descendant of the value field
                        if ExpressionSymbolUtils::is_node_descendant_of(&value_field, node) {
                            // Extract only the variable name/pattern (declaration targets)
                            if let Some(name_node) = parent.field("name") {
                                Self::extract_targets(&name_node, &mut target_symbols, "=");
                            }
                            break;
                        }
                    }
                }
                // Stop at certain node types that define scope boundaries
                "function_declaration"
                | "method_definition"
                | "arrow_function"
                | "function_expression" => {
                    break;
                }
                _ => {}
            }
            current = parent.parent();
        }

        self.assigment_target_symbols = target_symbols;
    }

    pub fn resolve_modifiers(&self, node: &Node<StrDoc<SupportLang>>) -> ExpressionModifiers {
        let mut modifiers = ExpressionModifiers::default();

        // Check ancestors for await
        let mut current_node = Some(node.clone());
        while let Some(node) = current_node {
            if let Some(parent) = node.parent() {
                if parent.kind() == "await_expression" {
                    modifiers.is_await = true;
                    break;
                }
                current_node = Some(parent);
            } else {
                break;
            }
        }

        // Traverse down to find "this"
        let mut stack = vec![node.clone()];
        while let Some(current) = stack.pop() {
            if current.kind() == "this" {
                modifiers.is_this = true;
                break;
            }

            for child in current.children() {
                stack.push(child);
            }
        }

        // Traverse down to find "super"
        let mut stack = vec![node.clone()];
        while let Some(current) = stack.pop() {
            if current.kind() == "super" {
                modifiers.is_super = true;
                break;
            }

            for child in current.children() {
                stack.push(child);
            }
        }

        modifiers
    }

    pub fn resolve_symbols(&mut self, node: &Node<StrDoc<SupportLang>>) {
        let mut results = vec![];
        // Resolve modifiers once at the top level
        let modifiers = self.resolve_modifiers(node);
        Self::collect_symbols_recursive(node, &mut results, modifiers, true);
        self.symbols = results;
    }

    fn collect_symbols_recursive(
        node: &Node<StrDoc<SupportLang>>,
        results: &mut Vec<ExpressionSymbolInfo>,
        modifiers: ExpressionModifiers,
        is_first_call: bool,
    ) {
        // For call_expression the callee is under the "function" field
        // For new_expression the callee is under the "constructor" field
        let func_node = node.field("function").or_else(|| node.field("constructor"));

        if let Some(func_node) = func_node {
            // Check if the function contains a call_expression (chained call)
            let has_chained_call = ExpressionSymbolUtils::contains_call_expression(&func_node);

            if has_chained_call {
                // Find the call_expression inside the member_expression and recurse
                for child in func_node.children() {
                    if child.kind() == "call_expression" {
                        // Pass is_first_call=true for the deeper call, false for this level
                        Self::collect_symbols_recursive(&child, results, modifiers, true);
                        break;
                    }
                }

                // After processing the deeper call, extract the method name and range for this level
                if let Some((method_name, method_range, mut metadata)) =
                    ExpressionSymbolUtils::extract_chained_method_info(node)
                {
                    if let TypeScriptReferenceMetadata::Call {
                        ref mut is_async,
                        ref mut is_super,
                        ref mut is_this,
                        ..
                    } = metadata
                    {
                        *is_async = false;
                        *is_super = modifiers.is_super;
                        *is_this = modifiers.is_this;
                    }

                    // NOTE: Chained calls are never async
                    results.push(ExpressionSymbolInfo {
                        range: method_range,
                        name: method_name,
                        symbol_type: TypeScriptSymbolType::MethodCall,
                        metadata: Some(metadata),
                    });
                }
            } else {
                // Member expression without chained calls
                if func_node.kind() == "member_expression" {
                    // Extract object and property from member expression
                    let mut object_node = None;
                    let mut property_node = None;

                    for child in func_node.children() {
                        match child.kind().as_ref() {
                            // Allow new_expression as a valid object for cases like: new MyClass().foo()
                            // Allow subscript_expression for cases like: myObj[key].foo()
                            "identifier"
                            | "member_expression"
                            | "subscript_expression"
                            | "this"
                            | "super"
                            | "new_expression" => {
                                object_node = Some(child);
                            }
                            "property_identifier" => {
                                property_node = Some(child);
                            }
                            _ => {}
                        }
                    }

                    // If this node is a call_expression, treat as a regular method call with a source
                    if node.kind() == "call_expression" {
                        // Use helper function to recursively extract member chain symbols
                        if let Some(object) = object_node {
                            ExpressionSymbolUtils::extract_member_chain_symbols(
                                &object,
                                results,
                                modifiers,
                                is_first_call,
                            );
                        }

                        // Add the property as MethodCall
                        if let Some(property) = property_node {
                            let args = ExpressionSymbolUtils::extract_call_arguments(node);
                            results.push(ExpressionSymbolInfo {
                                range: node_to_range(&property),
                                name: property.text().to_string(),
                                symbol_type: TypeScriptSymbolType::MethodCall,
                                metadata: Some(TypeScriptReferenceMetadata::Call {
                                    is_async: modifiers.is_await && is_first_call,
                                    is_super: modifiers.is_super,
                                    is_this: modifiers.is_this,
                                    args,
                                }),
                            });
                        }
                    } else if node.kind() == "new_expression" {
                        // For constructor calls like: new Namespace.MyClass()
                        if let Some(property) = property_node {
                            let args = ExpressionSymbolUtils::extract_call_arguments(node);
                            results.push(ExpressionSymbolInfo {
                                range: node_to_range(&property),
                                name: property.text().to_string(),
                                symbol_type: TypeScriptSymbolType::ConstructorCall,
                                metadata: Some(TypeScriptReferenceMetadata::Constructor { args }),
                            });
                        } else {
                            // Fallback to entire callee text if property not found
                            let args = ExpressionSymbolUtils::extract_call_arguments(node);
                            results.push(ExpressionSymbolInfo {
                                range: node_to_range(&func_node),
                                name: func_node.text().to_string(),
                                symbol_type: TypeScriptSymbolType::ConstructorCall,
                                metadata: Some(TypeScriptReferenceMetadata::Constructor { args }),
                            });
                        }
                    }
                } else if func_node.kind() == "subscript_expression" {
                    // Handle subscript expressions like myObj[key]()
                    if node.kind() == "call_expression" {
                        // Use helper function to extract all member chain symbols including indexing
                        ExpressionSymbolUtils::extract_member_chain_symbols(
                            &func_node,
                            results,
                            modifiers,
                            is_first_call,
                        );

                        // For subscript calls like myObj[key](), add the call with the entire expression
                        // This represents calling a function that is accessed via bracket notation
                        let args = ExpressionSymbolUtils::extract_call_arguments(node);
                        results.push(ExpressionSymbolInfo {
                            range: node_to_range(&func_node),
                            name: func_node.text().to_string(),
                            symbol_type: TypeScriptSymbolType::Call,
                            metadata: Some(TypeScriptReferenceMetadata::Call {
                                is_async: modifiers.is_await && is_first_call,
                                is_super: modifiers.is_super,
                                is_this: modifiers.is_this,
                                args,
                            }),
                        });
                    } else if node.kind() == "new_expression" {
                        // Constructor call with subscript: new MyClass[key]()
                        let args = ExpressionSymbolUtils::extract_call_arguments(node);
                        results.push(ExpressionSymbolInfo {
                            range: node_to_range(&func_node),
                            name: func_node.text().to_string(),
                            symbol_type: TypeScriptSymbolType::ConstructorCall,
                            metadata: Some(TypeScriptReferenceMetadata::Constructor { args }),
                        });
                    }
                } else {
                    // Non-member callee
                    if node.kind() == "call_expression" {
                        // Simple function call (identifier)
                        let args = ExpressionSymbolUtils::extract_call_arguments(node);
                        results.push(ExpressionSymbolInfo {
                            range: node_to_range(&func_node),
                            name: func_node.text().to_string(),
                            symbol_type: TypeScriptSymbolType::Call,
                            metadata: Some(TypeScriptReferenceMetadata::Call {
                                is_async: modifiers.is_await && is_first_call,
                                is_super: modifiers.is_super,
                                is_this: modifiers.is_this,
                                args,
                            }),
                        });
                    } else if node.kind() == "new_expression" {
                        // Simple constructor call: new Identifier()
                        let args = ExpressionSymbolUtils::extract_call_arguments(node);
                        results.push(ExpressionSymbolInfo {
                            range: node_to_range(&func_node),
                            name: func_node.text().to_string(),
                            symbol_type: TypeScriptSymbolType::ConstructorCall,
                            metadata: Some(TypeScriptReferenceMetadata::Constructor { args }),
                        });
                    }
                }
            }
        }
    }

    // ASSIGNMENT PROCESSING

    /// Check if an assignment contains call or new expressions on the RHS
    fn assignment_contains_call_or_new(node: &Node<StrDoc<SupportLang>>) -> bool {
        if let Some(right_node) = node.field("right") {
            Self::node_contains_call_or_new(&right_node)
        } else {
            false
        }
    }

    /// Check if a node contains call or new expressions
    fn node_contains_call_or_new(node: &Node<StrDoc<SupportLang>>) -> bool {
        if matches!(node.kind().as_ref(), "call_expression" | "new_expression") {
            return true;
        }

        // Recursively check all children
        for child in node.children() {
            if Self::node_contains_call_or_new(&child) {
                return true;
            }
        }

        false
    }

    /// Process assignment expressions like: x = 5, y = obj.prop, z = { destructure }, etc.
    fn process_assignment_expression(
        node: &Node<StrDoc<SupportLang>>,
        expression: &mut TypeScriptExpression,
    ) {
        let mut symbols = vec![];

        // Extract assignment target (LHS)
        if let Some(left_node) = node.field("left") {
            let operator = ExpressionSymbolUtils::extract_assignment_operator(node);
            let mut target_symbols = vec![];
            Self::extract_targets(&left_node, &mut target_symbols, &operator);
            expression.assigment_target_symbols.extend(target_symbols);
        }

        // Extract RHS symbols (identifiers, member access, but not calls - those are handled separately)
        if let Some(right_node) = node.field("right") {
            Self::extract_assignment_rhs_symbols(&right_node, &mut symbols);
        }

        expression.symbols = symbols;
    }

    /// Process variable declarations like: const x = value, let { id } = obj, etc.
    fn process_variable_declaration(
        declarator_node: &Node<StrDoc<SupportLang>>,
        value_node: &Node<StrDoc<SupportLang>>,
        expression: &mut TypeScriptExpression,
    ) {
        let mut symbols = vec![];

        // Extract variable name/pattern (LHS)
        if let Some(name_node) = declarator_node.field("name") {
            let mut target_symbols = vec![];
            Self::extract_targets(&name_node, &mut target_symbols, "=");
            expression.assigment_target_symbols.extend(target_symbols);
        }

        // Extract RHS symbols (identifiers, member access, but not calls - those are handled separately)
        Self::extract_assignment_rhs_symbols(value_node, &mut symbols);

        expression.symbols = symbols;
    }

    /// Extract symbols from RHS of assignments (identifiers, member access, etc.)
    fn extract_assignment_rhs_symbols(
        node: &Node<StrDoc<SupportLang>>,
        results: &mut Vec<ExpressionSymbolInfo>,
    ) {
        match node.kind().as_ref() {
            "identifier" => {
                // Simple identifier reference like: z = x
                results.push(ExpressionSymbolInfo {
                    range: node_to_range(node),
                    name: node.text().to_string(),
                    symbol_type: TypeScriptSymbolType::Identifier,
                    metadata: None,
                });
            }
            "member_expression" => {
                // Member access like: z = obj.property
                ExpressionSymbolUtils::extract_member_chain_symbols(
                    node,
                    results,
                    ExpressionModifiers::default(),
                    true,
                );
            }
            "subscript_expression" => {
                // Subscript access like: z = obj[key]
                ExpressionSymbolUtils::extract_member_chain_symbols(
                    node,
                    results,
                    ExpressionModifiers::default(),
                    true,
                );
            }
            "object" | "array" => {
                // Object/array literals like: z = { a: 1 } or z = [1, 2, 3]
                // We don't extract symbols from literals
            }
            _ => {
                // For other node types (literals, calls, etc.), we don't extract symbols
                // Calls and new expressions are handled by their own code path
            }
        }
    }

    // LHS PROCESSING

    /// Extract assignment or declaration targets from a node (both simple and destructured)
    fn extract_targets(
        node: &Node<StrDoc<SupportLang>>,
        target_symbols: &mut Vec<ExpressionSymbolInfo>,
        operator: &str,
    ) {
        if DESTRUCTED_DECLARATION_NODES.contains(node.kind().as_ref()) {
            Self::extract_destructured_targets(node, target_symbols, operator);
        } else {
            // Simple assignment or declaration target
            target_symbols.push(ExpressionSymbolInfo {
                range: node_to_range(node),
                name: node.text().to_string(),
                symbol_type: TypeScriptSymbolType::AssignmentTarget,
                metadata: Some(TypeScriptReferenceMetadata::Assignment {
                    operator: operator.to_string(),
                    is_destructured: false,
                    aliased_from: None,
                }),
            });
        }
    }

    /// Extract targets from destructured variable declarations like const {a, b} = obj
    /// or destructured assignments like {a, b} = obj or [x, y] = arr
    #[inline]
    fn extract_destructured_targets(
        pattern_node: &Node<StrDoc<SupportLang>>,
        results: &mut Vec<ExpressionSymbolInfo>,
        operator: &str,
    ) {
        let mut stack = vec![pattern_node.clone()];
        while let Some(current_node) = stack.pop() {
            for child in current_node.children() {
                match child.kind().as_ref() {
                    "identifier" | "shorthand_property_identifier_pattern" => {
                        results.push(ExpressionSymbolInfo {
                            range: node_to_range(&child),
                            name: child.text().to_string(),
                            symbol_type: TypeScriptSymbolType::AssignmentTarget,
                            metadata: Some(TypeScriptReferenceMetadata::Assignment {
                                operator: operator.to_string(),
                                is_destructured: true,
                                aliased_from: None,
                            }),
                        });
                    }
                    "pair_pattern" => {
                        // For patterns like {key: value} - capture both original property name and alias
                        if let Some(key_node) = child.field("key")
                            && let Some(value_node) = child.field("value")
                            && value_node.kind() == "identifier"
                        {
                            results.push(ExpressionSymbolInfo {
                                range: node_to_range(&value_node),
                                name: value_node.text().to_string(), // Aliased value, e.g what is being used (e.g. userId)
                                symbol_type: TypeScriptSymbolType::AssignmentTarget,
                                metadata: Some(TypeScriptReferenceMetadata::Assignment {
                                    operator: operator.to_string(),
                                    is_destructured: true,
                                    aliased_from: Some(key_node.text().to_string()), // Alias (e.g., "id")
                                }),
                            });
                        }
                    }
                    // Handle nested patterns iteratively by adding to stack
                    "object_pattern" | "array_pattern" => {
                        stack.push(child);
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::typescript::references::tests::{get_expressions, print_expressions};

    #[test]
    fn test_expression_modifiers() {
        let code: &'static str = r#"
        await this.emailService.sendEmail(to, sanitizedSubject, sanitizedBody).foo(bars).apples(baz);
        let hi = await this.processor.loc.sendEmail(to, sanitizedSubject, sanitizedBody);
        await super.superclass.postProcess(notification);
        foo();
        foo().bar();
        mycallsource.baz();
        mycallsource.source2.bee();
        let newClass = new MyClass();
        let newClass2 = new MyClass().cafe(bar);
          this.processNotification(notification)
                    .then(() => this.emit('notification:success', notification))
                    .catch(error => this.emit('notification:failed', notification, error.message));

        let result = myObj[key](arg);
        let result = myObj[key].foo(arg);
        "#;
        let expressions = get_expressions(code);
        print_expressions(&expressions);
        assert_eq!(expressions.len(), 14);
    }

    #[test]
    fn test_assignment_expressions() {
        let code: &'static str = r#"
        // Simple assignments
        x = 5;
        y = foo();
        z = obj.method();
        
        // Complex assignments
        result = this.service.processData(input);
        config += additionalConfig;
        
        // Nested assignments
        user.profile = await api.fetchProfile(userId);
        "#;
        let expressions = get_expressions(code);
        print_expressions(&expressions);
        assert_eq!(expressions.len(), 6);
    }

    #[test]
    fn test_lexical_declarations() {
        let code: &'static str = r#"
        // Variable declarations
        let x = 5;
        const result = foo();
        var config = obj.method();
        
        // Complex declarations
        const data = this.service.fetchData(id);
        let user = await api.getUser(userId);
        
        // Destructured declarations
        const {name, age} = person;
        let [first, second] = array;
        const {id: userId} = user;
        
        // Standalone calls (should still be parsed separately)
        bar();
        standalone.call();
        "#;
        let expressions = get_expressions(code);
        print_expressions(&expressions);
        assert_eq!(expressions.len(), 10);
    }

    #[test]
    fn test_multiple_assignments() {
        let code: &'static str = r#"
        // Variable declarations
        let x = new DatabaseConnection();
        y = x.connect();
        x.disconnect();
        z = x;
        z.connect();
        "#;
        let expressions = get_expressions(code);
        print_expressions(&expressions);
        assert_eq!(expressions.len(), 5);
    }

    #[test]
    fn test_complex_expressions_unfolded() {
        let code = include_str!("../fixtures/typescript/references/complex.ts");
        let expressions = get_expressions(code);
        print_expressions(&expressions);
        assert_eq!(expressions.len(), 121);
    }
}
