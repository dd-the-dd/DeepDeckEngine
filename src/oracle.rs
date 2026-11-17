mod ability_catalog;
mod audit;
mod canonical;
mod card_structure;
mod characteristic_vocabulary;
mod contextual_binding;
mod counter_name;
mod model;
mod pipeline;
mod syntax;

#[cfg(test)]
mod ability_catalog_tests;
#[cfg(test)]
mod audit_tests;
#[cfg(test)]
mod reminder_only_line_tests;

pub use card_structure::{
    CardPartRole, CardStructureFailure, CardStructureKind, CardStructurePart, CardStructurePlan,
    plan_card_structure,
};
pub use model::{
    AbilitySource, OracleAuditStage, OracleCardFace, OracleCardParseRequest, OracleCardParseResult,
    ParsedOracleAbility, RecognizerProvenance,
};
pub use pipeline::{parse_oracle_card, parse_oracle_card_native};

pub fn compile_named_token_catalog_rules(
    token_name: &str,
    type_line: &str,
    oracle: &str,
) -> Option<Vec<serde_json::Value>> {
    canonical::compile_named_token_catalog_rules(token_name, type_line, oracle)
}

pub fn compile_named_token_native_rule(oracle: &str) -> Option<serde_json::Value> {
    canonical::compile_named_token_native_rule(oracle)
}
