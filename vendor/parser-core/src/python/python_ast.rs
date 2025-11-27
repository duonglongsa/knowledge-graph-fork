use crate::parser::{SupportedLanguage, load_rules_from_yaml};
use ast_grep_config::RuleConfig;
use ast_grep_language::SupportLang;
use once_cell::sync::Lazy;
use rustc_hash::FxHashMap;

pub const DEFINITIONS_YAML: &str = include_str!("./rules/definitions.yaml");
pub const IMPORTS_YAML: &str = include_str!("./rules/imports.yaml");

pub static PYTHON_RULES: Lazy<String> =
    Lazy::new(|| format!("{DEFINITIONS_YAML}\n---\n{IMPORTS_YAML}"));

pub static RULES_CONFIG: Lazy<Vec<RuleConfig<SupportLang>>> =
    Lazy::new(|| load_rules_from_yaml(&PYTHON_RULES, SupportedLanguage::Python));

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum PythonMatchKind {
    Definition,
    ImportedSymbol,
    Other(String),
}

pub static RULE_ID_KIND_MAP: Lazy<FxHashMap<&'static str, PythonMatchKind>> = Lazy::new(|| {
    let mut m = FxHashMap::default();
    m.insert("python-definitions", PythonMatchKind::Definition);
    m.insert("python-imports", PythonMatchKind::ImportedSymbol);
    m
});

impl PythonMatchKind {
    pub fn from_rule_id(rule_id: &str) -> Self {
        RULE_ID_KIND_MAP
            .get(rule_id)
            .cloned()
            .unwrap_or_else(|| PythonMatchKind::Other(rule_id.to_string()))
    }
}
