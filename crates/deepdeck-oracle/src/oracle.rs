#[path = "../../../src/oracle/ability_catalog.rs"]
mod ability_catalog;
#[path = "../../../src/oracle/audit.rs"]
mod audit;
#[cfg(feature = "automata-trace")]
#[path = "../../../src/oracle/automata_trace.rs"]
mod automata_trace;
#[path = "../../../src/oracle/band_gate.rs"]
mod band_gate;
#[path = "../../../src/oracle/canonical/mod.rs"]
mod canonical;
#[path = "../../../src/oracle/card_structure.rs"]
mod card_structure;
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

#[cfg(feature = "automata-trace")]
pub use automata_trace::{
    AutomataTrace, AutomataTraceEvent, AutomataTraceEventKind, TracedOracleCardParseResult,
    parse_oracle_card_native_traced,
};
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

/// The playable rules of one parsed ability rule. A template keyword (CR 702)
/// is parsed as one `keywordExpansion` holding the rules of its definition;
/// they are returned with the keyword's provenance. Any other rule is
/// returned unchanged.
pub fn keyword_expansion_rules(rule: &serde_json::Value) -> Vec<serde_json::Value> {
    canonical::keyword_expansion_rules(rule)
}
