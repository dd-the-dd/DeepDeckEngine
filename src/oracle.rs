mod ability_catalog;
mod audit;
mod canonical;
mod characteristic_vocabulary;
mod contextual_binding;
mod counter_name;
mod model;
mod pipeline;
mod regex_cache;
mod syntax;

#[cfg(test)]
mod ability_catalog_tests;
#[cfg(test)]
mod audit_tests;

pub use model::{
    AbilitySource, OracleAuditStage, OracleCardFace, OracleCardParseRequest, OracleCardParseResult,
    ParsedOracleAbility, RecognizerProvenance,
};
pub use pipeline::{parse_oracle_card, parse_oracle_card_native};

pub fn compile_named_token_native_rule(oracle: &str) -> Option<serde_json::Value> {
    canonical::compile_named_token_native_rule(oracle)
}
