use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OracleCardFace {
    pub id: String,
    pub name: String,
    pub type_line: String,
    pub mana_cost: Option<String>,
    pub oracle_text: String,
    #[serde(default)]
    pub power: Option<String>,
    #[serde(default)]
    pub toughness: Option<String>,
    #[serde(default)]
    pub loyalty: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OracleCardParseRequest {
    pub card_name: String,
    pub type_line: String,
    pub mana_cost: Option<String>,
    pub oracle_text: Option<String>,
    #[serde(default)]
    pub layout: Option<String>,
    #[serde(default)]
    pub faces: Vec<OracleCardFace>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AbilitySource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face_id: Option<String>,
    pub text: String,
    pub line_start: usize,
    pub line_end: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognizedEntity {
    pub category: String,
    pub index: usize,
    pub raw: String,
    /// Half-open byte span in the original Oracle ability text.
    pub span_start: usize,
    pub span_end: usize,
    pub token_type: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimplificationIteration {
    pub depth: usize,
    pub id: String,
    pub operation: String,
    pub result: Value,
    pub result_text: String,
    pub status: String,
    pub title: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserTransitionDiagnostic {
    pub state: String,
    pub token_index: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserDiagnostic {
    pub code: String,
    pub message: String,
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automaton: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_start: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_end: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub found: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<ParserTransitionDiagnostic>,
}

/// Identifies which recognition boundary produced (or committed to) an
/// Oracle ability result. This is deliberately independent from semantic IR
/// and runtime executability: a correct primitive rule can still be legacy
/// recognition debt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecognizerProvenance {
    /// A promoted token-driven local automaton with no legacy fallback.
    Native,
    /// The pre-migration canonical parser path.
    Legacy,
    /// No recognizer claimed the ability.
    Unclaimed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedOracleAbility {
    pub source: AbilitySource,
    pub ability_type: String,
    pub status: String,
    pub recognizer: RecognizerProvenance,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<Value>,
    pub entities: Vec<RecognizedEntity>,
    pub iterations: Vec<SimplificationIteration>,
    pub diagnostics: Vec<ParserDiagnostic>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OracleAuditStage {
    pub key: String,
    pub title: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub abilities: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<Value>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OracleCardParseResult {
    pub schema_version: String,
    pub status: String,
    pub card_structure: Option<super::card_structure::CardStructurePlan>,
    pub context: OracleCardParseRequest,
    pub abilities: Vec<ParsedOracleAbility>,
    pub stages: Vec<OracleAuditStage>,
    pub diagnostics: Vec<ParserDiagnostic>,
}
