#[path = "../../../src/oracle/ability_catalog.rs"]
mod ability_catalog;
#[path = "../../../src/oracle/audit.rs"]
mod audit;
#[path = "../../../src/oracle/canonical/mod.rs"]
mod canonical;
#[path = "../../../src/oracle/characteristic_vocabulary.rs"]
mod characteristic_vocabulary;
#[path = "../../../src/oracle/contextual_binding.rs"]
mod contextual_binding;
#[path = "../../../src/oracle/counter_name.rs"]
mod counter_name;
#[path = "../../../src/oracle/model.rs"]
mod model;
#[path = "../../../src/oracle/pipeline.rs"]
mod pipeline;
#[path = "../../../src/oracle/regex_cache.rs"]
mod regex_cache;
#[path = "../../../src/oracle/syntax/mod.rs"]
mod syntax;

#[cfg(test)]
#[path = "../../../src/oracle/ability_catalog_tests.rs"]
mod ability_catalog_tests;
#[cfg(test)]
#[path = "../../../src/oracle/audit_tests.rs"]
mod audit_tests;
#[cfg(test)]
#[path = "../../../src/oracle/contextual_binding_tests.rs"]
mod contextual_binding_tests;

pub use model::{
    AbilitySource, OracleAuditStage, OracleCardFace, OracleCardParseRequest, OracleCardParseResult,
    ParsedOracleAbility, RecognizerProvenance,
};
pub use pipeline::{parse_oracle_card, parse_oracle_card_native};

pub fn compile_named_token_native_rule(oracle: &str) -> Option<serde_json::Value> {
    canonical::compile_named_token_native_rule(oracle)
}
