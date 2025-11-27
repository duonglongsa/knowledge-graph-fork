use crate::parser::{GenericParser, LanguageParser, SupportedLanguage};
use crate::typescript::ast::parse_ast;
use crate::typescript::definitions::find_definitions;
use crate::typescript::references::types::TypeScriptExpression;
use crate::typescript::references::types::TypeScriptSymbol;
use ast_grep_core::AstGrep;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_language::SupportLang;

pub fn traverse_ast(ast: &AstGrep<StrDoc<SupportLang>>) -> Vec<TypeScriptExpression> {
    ast.root()
        .dfs()
        .map(|node| TypeScriptExpression::from_node(&node))
        .filter(|expression| expression.valid())
        .collect()
}

pub fn get_expressions(code: &str) -> Vec<TypeScriptExpression> {
    let parser = GenericParser::default_for_language(SupportedLanguage::TypeScript);
    let parse_result = parser.parse(code, Some("test.ts")).unwrap();
    traverse_ast(&parse_result.ast)
}

pub fn get_symbols(code: &str) -> Vec<TypeScriptSymbol> {
    let parser = GenericParser::default_for_language(SupportedLanguage::TypeScript);
    let parse_result = parser.parse(code, Some("test.ts")).unwrap();
    let (node_fqn_map, imports, _expressions) = parse_ast(&parse_result.ast);
    let definitions = find_definitions(&node_fqn_map);

    let mut symbols = Vec::new();
    for definition in definitions {
        symbols.push(TypeScriptSymbol::Definition(definition));
    }
    for import in imports {
        symbols.push(TypeScriptSymbol::Import(import));
    }

    symbols
}

pub fn print_expressions(expressions: &Vec<TypeScriptExpression>) {
    for expression in expressions {
        println!("Expression: {:?}", expression.string);
        println!("Assignments: {:?}", expression.assigment_target_symbols);
        let assignment_target_names = expression
            .assigment_target_symbols
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>();
        println!("Expression assigned_to: [{assignment_target_names:?}]");
        println!("Expression range: {}", expression.range);
        for symbol in &expression.symbols {
            println!(
                "Symbol type: {:?}, name: {:?}, range[{:?}], metadata: {:?}",
                symbol.symbol_type, symbol.name, symbol.range, symbol.metadata
            );
        }
        println!("--------------------------------");
    }
}
