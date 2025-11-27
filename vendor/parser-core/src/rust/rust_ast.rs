use ast_grep_config::RuleConfig;
use ast_grep_language::SupportLang;
use once_cell::sync::Lazy;
use rustc_hash::FxHashMap;

// Rust extracts definitions and imports directly during FQN traversal, no YAML rules needed
pub static RULES_CONFIG: Lazy<Vec<RuleConfig<SupportLang>>> = Lazy::new(Vec::new);

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum RustMatchKind {
    Other(String),
}

pub static RULE_ID_KIND_MAP: Lazy<FxHashMap<&'static str, RustMatchKind>> =
    Lazy::new(FxHashMap::default);

impl RustMatchKind {
    pub fn from_rule_id(rule_id: &str) -> Self {
        RULE_ID_KIND_MAP
            .get(rule_id)
            .cloned()
            .unwrap_or_else(|| RustMatchKind::Other(rule_id.to_string()))
    }
}
