//! Basic usage example for the GitLab Code Parser Core library

use parser_core::{
    Result,
    parser::{GenericParser, LanguageParser, SupportedLanguage, detect_language_from_path},
    rules::{RuleManager, run_rules},
};

fn main() -> Result<()> {
    println!("GitLab Code Parser Core - Basic Usage Example");
    println!("==============================================");

    // 1. Language Detection
    println!("\n1. Language Detection:");
    let files = ["app.rb", "script.py", "main.js", "index.ts"];
    for file in &files {
        match detect_language_from_path(file) {
            Ok(lang) => println!("  {file} -> {lang}"),
            Err(e) => println!("  {file} -> Error: {e}"),
        }
    }

    // 2. Create Parser
    println!("\n2. Create Parser:");
    let file_path = "calculator.rb";
    let language = detect_language_from_path(file_path).unwrap();
    let parser = GenericParser::default_for_language(language);
    println!("  Created parser for {}", parser.language());

    // 3. Parse Ruby Code
    println!("\n3. Parse Ruby Code:");
    let ruby_code = r#"
class Calculator
  def initialize
    @value = 0
  end

  def add(number)
    @value += number
    self
  end

  def result
    @value
  end
end

calc = Calculator.new
result = calc.add(10).result
"#;

    let parse_result = parser.parse(ruby_code, Some("calculator.rb"))?;
    println!("  Parsed {} successfully", parse_result.language);
    println!("  File: {:?}", parse_result.file_path);
    println!("  AST root: {}", parse_result.ast.root().kind());

    // 5. Rule Loading and Execution
    println!("\n5. Rule Loading and Execution:");
    let ruby_rule_manager = RuleManager::new(SupportedLanguage::Ruby);

    let matches = run_rules(&parse_result.ast, Some("calculator.rb"), &ruby_rule_manager);

    println!("  Found {} matches:", matches.len());
    for (i, m) in matches.iter().take(3).enumerate() {
        println!(
            "    {}: {} (rule: {})",
            i + 1,
            m.match_info.matched.trim(),
            m.match_info.rule_id
        );
    }

    // 6. Multi-language Support
    println!("\n6. Multi-language Support:");
    let languages = [SupportedLanguage::Ruby];

    for lang in &languages {
        let _parser = GenericParser::default_for_language(*lang);
        let extensions = lang.file_extensions();
        println!("  {lang} supports: {extensions:?}");
    }

    println!("\n✅ Example completed successfully!");
    Ok(())
}
