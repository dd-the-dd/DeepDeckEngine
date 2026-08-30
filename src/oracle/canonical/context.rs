use serde_json::Value;

pub(crate) struct CanonicalRuleDraft {
    pub operations: Vec<String>,
    pub rule: Value,
}
