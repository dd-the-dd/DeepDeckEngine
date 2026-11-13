use serde_json::Value;

#[derive(Debug)]
pub(crate) struct CanonicalRuleDraft {
    pub operations: Vec<String>,
    pub rule: Value,
}
