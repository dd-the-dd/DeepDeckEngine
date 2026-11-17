use serde_json::Value;

use std::sync::OnceLock;

use crate::card_catalog::{named_token_names, ordinary_card_names};

use super::audit::{build_stages, iterations_for_rule, semantic_kind, unsupported_iteration};
use super::canonical::{
    CanonicalParseOutcome, compose_card_rules_native, parse_canonical_rule_native,
};
use super::card_structure::{
    CardPartRole, CardStructureKind, CardStructurePlan, plan_card_structure,
};
use super::contextual_binding::validate_native_contextual_bindings;
use super::model::{
    OracleCardParseRequest, OracleCardParseResult, ParsedOracleAbility, ParserDiagnostic,
    ParserTransitionDiagnostic, RecognizerProvenance,
};
use super::syntax::{
    AbilityInput, ExactNameTokenIndex, NameLinkKind, NameResolutionContext, NamedCardIndex,
    apply_class_level_requirement, class_level_header_from_tokens,
    classify_promoted_ability_from_tokens, documents, is_class_type_line, is_reminder_only_line,
    recognize_oracle_entities, resolve_counter_name_spans, resolve_named_token_spans,
    resolve_unknown_names, split_abilities,
};

const PARSER_SCHEMA_VERSION: &str = "oracle-parser/v1";

struct CatalogNamedCardIndex;

struct CatalogNamedTokenIndex;

impl NamedCardIndex for CatalogNamedTokenIndex {
    fn exact_name_ends(
        &self,
        tokens: &[super::model::RecognizedEntity],
        start: usize,
        maximum_end: usize,
    ) -> Vec<usize> {
        static INDEX: OnceLock<ExactNameTokenIndex> = OnceLock::new();
        INDEX
            .get_or_init(|| {
                ExactNameTokenIndex::from_names(named_token_names().unwrap_or_default())
            })
            .exact_name_ends(tokens, start, maximum_end)
    }
}

impl NamedCardIndex for CatalogNamedCardIndex {
    fn exact_name_ends(
        &self,
        tokens: &[super::model::RecognizedEntity],
        start: usize,
        maximum_end: usize,
    ) -> Vec<usize> {
        static INDEX: OnceLock<ExactNameTokenIndex> = OnceLock::new();
        INDEX
            .get_or_init(|| {
                let names = ordinary_card_names().unwrap_or_default();
                ExactNameTokenIndex::from_names(names)
            })
            .exact_name_ends(tokens, start, maximum_end)
    }
}

pub fn parse_oracle_card(request: OracleCardParseRequest) -> OracleCardParseResult {
    parse_oracle_card_native(request)
}

/// Adds the face-local Room door identity after primitive compilation.
///
/// Ability line numbers restart on each face, so they cannot identify the left
/// or right door. The request face order is the card-level source of truth and
/// applies to both public parsing entry points.
fn apply_room_door_index(
    structure: &CardStructurePlan,
    source_face_id: Option<&str>,
    rule: &mut Value,
) {
    if rule["kind"].as_str() == Some("rulesMarker") || structure.kind != CardStructureKind::Rooms {
        return;
    }

    let Some(source_face_id) = source_face_id else {
        return;
    };
    let Some(door_index) = structure
        .parts
        .iter()
        .position(|part| part.face_id.as_deref() == Some(source_face_id))
    else {
        return;
    };

    rule["roomDoorIndex"] = Value::from(door_index as u64);
}

/// An Adventure, Omen or prepared spell part is cast through its layout's
/// own rules (CR 715): exile on resolution, later casting permission, or a
/// prepared copy. The engine does not implement those layouts yet, so a
/// recognized part is not executable coverage and fails explicitly.
fn reject_parts_without_layout_runtime(
    structure: &CardStructurePlan,
    abilities: &mut [ParsedOracleAbility],
) {
    for ability in abilities
        .iter_mut()
        .filter(|ability| ability.status == "canonical")
    {
        let Some(face_id) = ability.source.face_id.as_deref() else {
            continue;
        };
        let role = structure
            .parts
            .iter()
            .find(|part| part.face_id.as_deref() == Some(face_id))
            .map(|part| part.role);
        if !matches!(
            role,
            Some(
                CardPartRole::AdventureSpell
                    | CardPartRole::OmenSpell
                    | CardPartRole::PreparedSpell
            )
        ) {
            continue;
        }
        ability.status = "unsupported".to_string();
        ability.rule = None;
        ability.diagnostics.push(ParserDiagnostic {
            code: "card_layout_runtime_missing".to_string(),
            message: "This spell part is cast through Adventure, Omen or prepare layout rules                       (CR 715) that the engine does not implement yet."
                .to_string(),
            severity: "unsupported".to_string(),
            automaton: Some("card-face-factory".to_string()),
            state: Some("card-structure.layout-runtime".to_string()),
            token_index: None,
            span_start: None,
            span_end: None,
            expected: Some("a runtime for the card's layout casting rules".to_string()),
            found: None,
            trace: Vec::new(),
        });
    }
}

fn native_failure_kind(
    input: &AbilityInput<'_>,
    promoted_kind: Option<&'static str>,
) -> &'static str {
    if let Some(kind) = promoted_kind {
        return kind;
    }
    let starts_with_source_name = input.name_links.iter().any(|link| {
        link.token_start == 0
            && matches!(
                link.kind,
                NameLinkKind::CurrentCard | NameLinkKind::CurrentFace
            )
    });
    if !starts_with_source_name
        && input
            .tokens
            .first()
            .is_some_and(|token| token.token_type == "keywordWord")
    {
        return "keywordAbility";
    }
    if input
        .face_type_line
        .split(|character: char| !character.is_alphanumeric())
        .any(|part| part.eq_ignore_ascii_case("instant") || part.eq_ignore_ascii_case("sorcery"))
    {
        return "spellAbility";
    }
    "staticAbility"
}

/// Compiles exclusively through shared token automata and primitive IR.
/// Unsupported grammar remains explicit; there is no legacy fallback.
pub fn parse_oracle_card_native(request: OracleCardParseRequest) -> OracleCardParseResult {
    let card_structure = match plan_card_structure(&request) {
        Ok(structure) => structure,
        Err(failure) => {
            let diagnostic = failure.into_diagnostic();
            return OracleCardParseResult {
                schema_version: PARSER_SCHEMA_VERSION.to_string(),
                status: "unsupported".to_string(),
                card_structure: None,
                context: request,
                abilities: Vec::new(),
                stages: vec![super::model::OracleAuditStage {
                    key: "cardStructure".to_string(),
                    title: "Card-face structure factory".to_string(),
                    status: "unsupported".to_string(),
                    items: Vec::new(),
                    abilities: Vec::new(),
                    terminal: Some(
                        serde_json::to_value(&diagnostic).expect("a parser diagnostic serializes"),
                    ),
                }],
                diagnostics: vec![diagnostic],
            };
        }
    };
    let context = request.clone();
    let legendary = |type_line: &str| {
        type_line
            .split_whitespace()
            .any(|word| word.eq_ignore_ascii_case("legendary"))
    };
    let name_context = NameResolutionContext::with_legendary_names(
        &request.card_name,
        legendary(&request.type_line),
        request
            .faces
            .iter()
            .map(|face| (face.name.as_str(), legendary(&face.type_line))),
    );
    let named_cards = CatalogNamedCardIndex;
    let named_tokens = CatalogNamedTokenIndex;
    let mut abilities = Vec::new();
    for document in documents(&request, &card_structure) {
        let is_class = is_class_type_line(document.face_type_line);
        let mut minimum_class_level = 1_i64;
        for source in split_abilities(&document) {
            let ability_index = abilities.len();
            #[cfg(feature = "automata-trace")]
            let _trace_ability = super::automata_trace::scope_ability(ability_index);
            let mut entities = recognize_oracle_entities(&source.text);
            let announced_class_level = is_class
                .then(|| class_level_header_from_tokens(&entities))
                .flatten();
            resolve_named_token_spans(&mut entities, &named_tokens);
            resolve_counter_name_spans(&mut entities);
            let mut name_resolution_failure = None;
            let name_links =
                match resolve_unknown_names(&source.text, &entities, &name_context, &named_cards) {
                    Ok(resolution) => resolution.resolved,
                    Err(failure) => {
                        let resolved = failure.resolved.clone();
                        name_resolution_failure = Some(failure);
                        resolved
                    }
                };
            let input = AbilityInput {
                face_name: document.face_name,
                face_type_line: document.face_type_line,
                source: &source,
                tokens: &entities,
                name_links: &name_links,
                name_resolution_failure: name_resolution_failure.as_ref(),
            };
            let promoted_ability_kind = classify_promoted_ability_from_tokens(&input);
            let ability_kind = { promoted_ability_kind.unwrap_or("unclassifiedAbility") };
            let parse_outcome = { parse_canonical_rule_native(&input, promoted_ability_kind) };
            // Move candidate evidence out without cloning its message/trace.
            // Normalize the owned primary into the existing failure path.
            let (parse_outcome, candidate_failure) = match parse_outcome {
                CanonicalParseOutcome::FailedWithCandidateEvidence {
                    failure,
                    candidate_failure,
                    recognizer,
                } => (
                    CanonicalParseOutcome::Failed {
                        failure,
                        recognizer,
                    },
                    Some(candidate_failure),
                ),
                other => (other, None),
            };

            match parse_outcome {
                CanonicalParseOutcome::Parsed {
                    mut draft,
                    recognizer,
                } => {
                    if announced_class_level.is_none() {
                        apply_class_level_requirement(&mut draft.rule, minimum_class_level);
                    }
                    apply_room_door_index(
                        &card_structure,
                        source.face_id.as_deref(),
                        &mut draft.rule,
                    );
                    let final_ability_kind = semantic_kind(&draft.rule)
                        .unwrap_or(ability_kind)
                        .to_string();
                    let iteration_kind = &final_ability_kind;
                    let iterations =
                        iterations_for_rule(&input, ability_index, iteration_kind, &draft);
                    abilities.push(ParsedOracleAbility {
                        source,
                        ability_type: final_ability_kind,
                        status: "canonical".to_string(),
                        recognizer,
                        rule: Some(draft.rule),
                        entities,
                        iterations,
                        diagnostics: Vec::new(),
                    });
                }
                CanonicalParseOutcome::Unsupported if is_reminder_only_line(&source.text) => {
                    // A reminder-only line that no family claims has no rules meaning.
                }
                CanonicalParseOutcome::Unsupported => {
                    let iterations = unsupported_iteration(&input, ability_index, ability_kind);
                    abilities.push(ParsedOracleAbility {
                        source,
                        ability_type: ability_kind.to_string(),
                        status: "unsupported".to_string(),
                        recognizer: RecognizerProvenance::Unclaimed,
                        rule: None,
                        entities,
                        iterations,
                        diagnostics: vec![ParserDiagnostic {
                            code: "unsupported_oracle_ability".to_string(),
                            message: "No token-driven native automaton claimed this ability."
                                .to_string(),
                            severity: "unsupported".to_string(),
                            automaton: None,
                            state: None,
                            token_index: None,
                            span_start: None,
                            span_end: None,
                            expected: None,
                            found: None,
                            trace: Vec::new(),
                        }],
                    });
                }
                CanonicalParseOutcome::Failed { .. }
                | CanonicalParseOutcome::FailedWithCandidateEvidence { .. }
                    if is_reminder_only_line(&source.text) =>
                {
                    // CR 207.2: an unclaimed reminder-only line is not an ability.
                }
                CanonicalParseOutcome::Failed {
                    failure,
                    recognizer,
                }
                | CanonicalParseOutcome::FailedWithCandidateEvidence {
                    failure,
                    recognizer,
                    ..
                } => {
                    let failure_kind = { native_failure_kind(&input, promoted_ability_kind) };
                    let iterations = unsupported_iteration(&input, ability_index, failure_kind);
                    abilities.push(ParsedOracleAbility {
                        source,
                        ability_type: failure_kind.to_string(),
                        status: "unsupported".to_string(),
                        recognizer,
                        rule: None,
                        entities,
                        iterations,
                        diagnostics: std::iter::once(("oracle_machine_failure", failure))
                            .chain(
                                candidate_failure.into_iter().map(|candidate| {
                                    ("oracle_candidate_machine_failure", candidate)
                                }),
                            )
                            .map(|(code, failure)| {
                                let trace_start = failure.trace.len().saturating_sub(32);
                                ParserDiagnostic {
                                    code: code.to_string(),
                                    message: failure.to_string(),
                                    severity: "unsupported".to_string(),
                                    automaton: Some(failure.automaton.to_string()),
                                    state: Some(failure.state.to_string()),
                                    token_index: Some(failure.token_index),
                                    span_start: Some(failure.span_start),
                                    span_end: Some(failure.span_end),
                                    expected: Some(failure.expected),
                                    found: failure.actual,
                                    trace: failure.trace[trace_start..]
                                        .iter()
                                        .map(|transition| ParserTransitionDiagnostic {
                                            state: transition.state.to_string(),
                                            token_index: transition.token_index,
                                        })
                                        .collect(),
                                }
                            })
                            .collect(),
                    });
                }
            }
            if let Some(level) = announced_class_level {
                minimum_class_level = level;
            }
        }
    }
    compose_card_rules_native(&mut abilities);
    // Binding validity is checked after composition in the owning face context.
    validate_native_contextual_bindings(&request, &mut abilities);
    reject_parts_without_layout_runtime(&card_structure, &mut abilities);
    let status = if abilities
        .iter()
        .all(|ability| ability.status == "canonical")
    {
        "canonical"
    } else {
        "unsupported"
    }
    .to_string();
    let diagnostics = abilities
        .iter()
        .flat_map(|ability| ability.diagnostics.clone())
        .collect();
    let stages = build_stages(&abilities);

    OracleCardParseResult {
        schema_version: PARSER_SCHEMA_VERSION.to_string(),
        status,
        card_structure: Some(card_structure),
        context,
        abilities,
        stages,
        diagnostics,
    }
}

#[cfg(test)]
#[path = "candidate_failure_evidence_tests.rs"]
mod candidate_failure_evidence_tests;
