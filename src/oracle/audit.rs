use regex::Regex;
use serde_json::{Value, json};

use super::canonical::CanonicalRuleDraft;
use super::model::{OracleAuditStage, ParsedOracleAbility, SimplificationIteration};
use super::syntax::{AbilityInput, activation_parts, recognize_entities};

fn clean_phrase(value: &str) -> String {
    value
        .trim()
        .trim_matches(|character: char| {
            character.is_whitespace()
                || matches!(character, '.' | ',' | ';' | '(' | ')' | '\u{2022}')
        })
        .trim()
        .to_string()
}

fn find_ascii_case_insensitive(value: &str, expected: &str) -> Option<usize> {
    value
        .to_ascii_lowercase()
        .find(&expected.to_ascii_lowercase())
}

fn source_phrase(input: &AbilityInput<'_>) -> String {
    let text = input.source.text.trim();
    for phrase in ["this land", "this creature", "this artifact", "this spell"] {
        if let Some(index) = find_ascii_case_insensitive(text, phrase) {
            return text[index..index + phrase.len()].to_string();
        }
    }
    if text
        .to_ascii_lowercase()
        .starts_with(&input.face_name.to_ascii_lowercase())
    {
        return input.face_name.to_string();
    }
    "implicit source".to_string()
}

fn condition_phrase(text: &str) -> Option<String> {
    if let Some(index) = find_ascii_case_insensitive(text, "unless ") {
        return Some(clean_phrase(&text[index..]));
    }
    let marker = ", if ";
    let index = find_ascii_case_insensitive(text, marker)?;
    let condition_start = index + 2;
    let remainder = &text[condition_start..];
    let condition_end = remainder.find(',').unwrap_or(remainder.len());
    Some(clean_phrase(&remainder[..condition_end]))
}

fn event_phrase(text: &str, ability_kind: &str) -> Option<String> {
    if ability_kind == "triggeredAbility" {
        return Some(clean_phrase(
            text.split_once(',').map_or(text, |(event, _)| event),
        ));
    }
    if ability_kind != "replacementEffect" {
        return None;
    }
    if let Some(index) = find_ascii_case_insensitive(text, " unless ") {
        let action = &text[..index];
        let event = action
            .strip_suffix(" tapped")
            .or_else(|| action.strip_suffix(" prepared"))
            .unwrap_or(action);
        return Some(clean_phrase(event));
    }
    if Regex::new(r"(?i)^as .+ enters,")
        .expect("as-enters event regex compiles")
        .is_match(text.trim_start())
    {
        return Some(clean_phrase(
            text.split_once(',').map_or(text, |(event, _)| event),
        ));
    }
    let lower = text.to_ascii_lowercase();
    let enters_index = lower.find(" enters")?;
    Some(clean_phrase(&text[..enters_index + " enters".len()]))
}

fn replacement_phrase(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    if lower.contains(" enters tapped unless ") {
        return Some("enters tapped".to_string());
    }
    if let Some(index) = lower.rfind("it enters tapped") {
        return Some(clean_phrase(&text[index..index + "it enters tapped".len()]));
    }
    if let Some(index) = lower.find(" enters prepared") {
        return Some(clean_phrase(&text[index..index + " enters prepared".len()]));
    }
    None
}

fn triggered_effect_phrase(text: &str) -> String {
    let Some((_, remainder)) = text.split_once(',') else {
        return clean_phrase(text);
    };
    let remainder = remainder.trim();
    if remainder.to_ascii_lowercase().starts_with("if ")
        && let Some((_, effect)) = remainder.split_once(',')
    {
        return clean_phrase(effect);
    }
    clean_phrase(remainder)
}

fn section_phrase(input: &AbilityInput<'_>, ability_kind: &str, section: &str) -> String {
    let text = input.source.text.trim();
    match section {
        "source" => source_phrase(input),
        "costs" => activation_parts(text)
            .map(|(costs, _)| clean_phrase(costs))
            .unwrap_or_else(|| clean_phrase(text)),
        "activationCondition" => find_ascii_case_insensitive(text, "Activate only if")
            .map(|index| clean_phrase(&text[index..]))
            .unwrap_or_else(|| clean_phrase(text)),
        "event" => event_phrase(text, ability_kind).unwrap_or_else(|| clean_phrase(text)),
        "condition" => condition_phrase(text).unwrap_or_else(|| clean_phrase(text)),
        "decisions" => find_ascii_case_insensitive(text, "you may pay")
            .map(|index| {
                let decision = &text[index..];
                clean_phrase(
                    decision
                        .split_once('.')
                        .map_or(decision, |(sentence, _)| sentence),
                )
            })
            .unwrap_or_else(|| clean_phrase(text)),
        "replacement" => replacement_phrase(text).unwrap_or_else(|| clean_phrase(text)),
        "effects" => {
            if let Some((_, effects)) = activation_parts(text) {
                let condition_index = find_ascii_case_insensitive(effects, "Activate only if")
                    .unwrap_or(effects.len());
                clean_phrase(&effects[..condition_index])
            } else if ability_kind == "triggeredAbility" {
                triggered_effect_phrase(text)
            } else if ability_kind == "replacementEffect" {
                replacement_phrase(text).unwrap_or_else(|| clean_phrase(text))
            } else {
                clean_phrase(text)
            }
        }
        "activeWhile" => source_phrase(input),
        "declaration" | "modifiers" | "ability" | "abilities" => clean_phrase(text),
        _ => clean_phrase(text),
    }
}

fn unresolved_entities(
    input: &AbilityInput<'_>,
    ability_kind: &str,
    section: &str,
    path: &str,
) -> Value {
    let text = section_phrase(input, ability_kind, section);
    let entities = recognize_entities(&text)
        .into_iter()
        .map(|entity| entity.raw)
        .collect::<Vec<_>>();
    json!({
        "kind": "unresolvedEntities",
        "section": section,
        "path": path,
        "text": text,
        "entities": entities,
    })
}

fn project_array(
    values: &[Value],
    depth: usize,
    maximum_depth: usize,
    input: &AbilityInput<'_>,
    ability_kind: &str,
    section: &str,
    path: &str,
) -> Value {
    if values.is_empty() {
        return Value::Array(Vec::new());
    }
    if depth > maximum_depth {
        return Value::Array(vec![unresolved_entities(
            input,
            ability_kind,
            section,
            path,
        )]);
    }
    Value::Array(
        values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                project_value(
                    value,
                    depth,
                    maximum_depth,
                    input,
                    ability_kind,
                    section,
                    &format!("{path}[{index}]"),
                )
            })
            .collect(),
    )
}

fn project_value(
    value: &Value,
    depth: usize,
    maximum_depth: usize,
    input: &AbilityInput<'_>,
    ability_kind: &str,
    section: &str,
    path: &str,
) -> Value {
    match value {
        Value::Object(object) => {
            if depth > maximum_depth {
                return unresolved_entities(input, ability_kind, section, path);
            }
            let projected = object
                .iter()
                .map(|(key, child)| {
                    let child_path = format!("{path}.{key}");
                    let child = match child {
                        Value::Object(_) => project_value(
                            child,
                            depth + 1,
                            maximum_depth,
                            input,
                            ability_kind,
                            section,
                            &child_path,
                        ),
                        Value::Array(values) => project_array(
                            values,
                            depth + 1,
                            maximum_depth,
                            input,
                            ability_kind,
                            section,
                            &child_path,
                        ),
                        _ => child.clone(),
                    };
                    (key.clone(), child)
                })
                .collect();
            Value::Object(projected)
        }
        Value::Array(values) => project_array(
            values,
            depth,
            maximum_depth,
            input,
            ability_kind,
            section,
            path,
        ),
        _ => value.clone(),
    }
}

fn project_rule(
    rule: &Value,
    maximum_depth: usize,
    input: &AbilityInput<'_>,
    ability_kind: &str,
) -> Value {
    let Some(object) = rule.as_object() else {
        return rule.clone();
    };
    let projected = object
        .iter()
        .map(|(key, value)| {
            if key == "kind" {
                let projected_kind = if maximum_depth >= maximum_object_depth(rule, 0) {
                    value.clone()
                } else {
                    Value::String(ability_kind.to_string())
                };
                return (key.clone(), projected_kind);
            }
            let value = match value {
                Value::Object(_) => {
                    project_value(value, 1, maximum_depth, input, ability_kind, key, key)
                }
                Value::Array(values) => {
                    project_array(values, 1, maximum_depth, input, ability_kind, key, key)
                }
                _ => value.clone(),
            };
            (key.clone(), value)
        })
        .collect();
    Value::Object(projected)
}

fn maximum_object_depth(value: &Value, depth: usize) -> usize {
    match value {
        Value::Object(object) => object.values().fold(depth, |maximum, child| {
            let child_depth = match child {
                Value::Object(_) => maximum_object_depth(child, depth + 1),
                Value::Array(values) => values
                    .iter()
                    .map(|value| maximum_object_depth(value, depth + 1))
                    .max()
                    .unwrap_or(depth + 1),
                _ => depth,
            };
            maximum.max(child_depth)
        }),
        Value::Array(values) => values
            .iter()
            .map(|value| maximum_object_depth(value, depth))
            .max()
            .unwrap_or(depth),
        _ => depth,
    }
}

fn classified_sections(input: &AbilityInput<'_>, ability_kind: &str) -> Vec<&'static str> {
    let lower = input.source.text.to_ascii_lowercase();
    match ability_kind {
        "activatedAbility" | "manaAbility" => {
            let mut sections = vec!["source", "costs"];
            if lower.contains("activate only if") {
                sections.push("activationCondition");
            }
            sections.push("effects");
            sections
        }
        "triggeredAbility" => {
            let mut sections = vec!["source", "event"];
            if lower.contains(", if ") {
                sections.push("condition");
            }
            sections.push("effects");
            sections
        }
        "replacementEffect" => {
            let mut sections = vec!["source", "event"];
            if lower.contains("you may pay") {
                sections.push("decisions");
            }
            if lower.contains(" unless ") {
                sections.push("condition");
            }
            sections.push("replacement");
            sections
        }
        "staticAbility" => vec!["source", "activeWhile", "modifiers"],
        "keywordAbility" => vec!["source", "ability"],
        "keywordAbilityGroup" => vec!["source", "abilities"],
        _ => {
            let mut sections = vec!["source"];
            if lower.contains("target")
                || lower.starts_with("choose ")
                || lower.starts_with("tiered ")
                || lower.starts_with("spree ")
            {
                sections.push("declaration");
            }
            sections.push("effects");
            sections
        }
    }
}

fn section_is_array(section: &str) -> bool {
    matches!(
        section,
        "abilities" | "costs" | "decisions" | "effects" | "modifiers" | "replacement"
    )
}

fn partitioned_rule_frame(input: &AbilityInput<'_>, ability_kind: &str) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("kind".to_string(), Value::String(ability_kind.to_string()));
    for section in classified_sections(input, ability_kind) {
        let unresolved = unresolved_entities(input, ability_kind, section, section);
        object.insert(
            section.to_string(),
            if section_is_array(section) {
                Value::Array(vec![unresolved])
            } else {
                unresolved
            },
        );
    }
    Value::Object(object)
}

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "<invalid Rule IR>".to_string())
}

pub(crate) fn semantic_kind(value: &Value) -> Option<&str> {
    value.as_object()?.get("kind")?.as_str()
}

fn collect_semantic_nodes(value: &Value, output: &mut Vec<Value>) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_semantic_nodes(value, output);
            }
        }
        Value::Object(object) => {
            for child in object.values() {
                collect_semantic_nodes(child, output);
            }
            if let Some(kind) = semantic_kind(value)
                && kind != "integer"
                && !output.contains(value)
            {
                output.push(value.clone());
            }
        }
        _ => {}
    }
}

pub(crate) fn iterations_for_rule(
    input: &AbilityInput<'_>,
    ability_index: usize,
    ability_kind: &str,
    draft: &CanonicalRuleDraft,
) -> Vec<SimplificationIteration> {
    let maximum_depth = maximum_object_depth(&draft.rule, 0);
    let mut iterations = Vec::with_capacity(maximum_depth + 1);
    for depth in 0..=maximum_depth {
        let result = if depth == 0 {
            partitioned_rule_frame(input, ability_kind)
        } else {
            project_rule(&draft.rule, depth, input, ability_kind)
        };
        let canonical = result == draft.rule;
        let iteration_number = iterations.len() + 1;
        let operation = if depth == 0 {
            format!("Classify {ability_kind} and partition entities")
        } else if canonical {
            "Emit canonical Rule IR".to_string()
        } else if let Some(detail) = draft.operations.get(depth - 1) {
            format!("Resolve semantic depth {depth}: {detail}")
        } else {
            format!("Resolve semantic depth {depth}")
        };
        iterations.push(SimplificationIteration {
            depth,
            id: format!("ability:{ability_index}:iteration:{iteration_number}"),
            operation,
            result_text: pretty_json(&result),
            result,
            status: if canonical {
                "canonical".to_string()
            } else {
                "state".to_string()
            },
            title: format!("Iteration {iteration_number}"),
        });
    }
    iterations
}

pub(crate) fn unsupported_iteration(
    input: &AbilityInput<'_>,
    ability_index: usize,
    ability_kind: &str,
) -> Vec<SimplificationIteration> {
    let result = partitioned_rule_frame(input, ability_kind);
    vec![SimplificationIteration {
        depth: 0,
        id: format!("ability:{ability_index}:iteration:1"),
        operation: format!("Classify {ability_kind} and partition entities"),
        result_text: pretty_json(&result),
        result,
        status: "unsupported".to_string(),
        title: "Iteration 1".to_string(),
    }]
}

fn vocabulary_items(abilities: &[ParsedOracleAbility]) -> Vec<Value> {
    let mut kinds = Vec::new();
    for ability in abilities {
        if let Some(rule) = &ability.rule {
            collect_semantic_nodes(rule, &mut kinds);
        }
    }
    let mut names = kinds
        .iter()
        .filter_map(semantic_kind)
        .filter(|kind| {
            [
                "addMana",
                "counterSpell",
                "createTokens",
                "dealDamage",
                "destroyPermanent",
                "drawCards",
                "exilePermanent",
                "flashback",
                "gainLife",
                "mill",
                "paradigm",
                "setPrepared",
            ]
            .contains(kind)
        })
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    names
        .into_iter()
        .map(|kind| json!({ "category": "magicVocabulary", "label": kind, "type": kind }))
        .collect()
}

fn primitive_items(vocabulary: &[Value]) -> Vec<Value> {
    vocabulary
        .iter()
        .filter_map(|item| item["type"].as_str())
        .map(|kind| {
            let primitive = match kind {
                "addMana" => "changeManaPool",
                "createTokens" => "createPermanentObjects",
                "dealDamage" => "applyDamageEvent",
                "drawCards" | "mill" => "moveCardsAndEmitEvent",
                "counterSpell" | "exilePermanent" => "moveObjectBetweenZones",
                "destroyPermanent" => "destroyWithReplacementCheck",
                "gainLife" => "changeLifeAndEmitEvent",
                "flashback" | "paradigm" | "setPrepared" => "installNamedRuleComposition",
                _ => "executeVocabulary",
            };
            json!({
                "category": "actionPrimitive",
                "label": primitive,
                "sourceVocabulary": kind,
                "type": primitive,
            })
        })
        .collect()
}

pub(crate) fn build_stages(abilities: &[ParsedOracleAbility]) -> Vec<OracleAuditStage> {
    let all_canonical = abilities
        .iter()
        .all(|ability| ability.status == "canonical");
    let recognition_items = abilities
        .iter()
        .enumerate()
        .map(|(index, ability)| {
            let recognized_text = ability
                .entities
                .iter()
                .map(|entity| entity.raw.as_str())
                .collect::<Vec<_>>()
                .join(" | ");
            json!({
                "abilityIndex": index,
                "category": "abilitySection",
                "entities": ability.entities,
                "faceId": ability.source.face_id,
                "label": format!("Ability {}: {recognized_text}", index + 1),
                "sourceText": ability.source.text,
                "status": ability.status,
                "type": "abilitySection",
            })
        })
        .collect();
    let simplifier_abilities = abilities
        .iter()
        .enumerate()
        .map(|(index, ability)| {
            json!({
                "abilityType": ability.ability_type,
                "id": format!("ability:{index}"),
                "index": index,
                "iterations": ability.iterations,
                "sourceText": ability.source.text,
                "status": ability.status,
                "terminal": {
                    "detail": if ability.status == "canonical" {
                        "This ability reduced to canonical Rule IR."
                    } else {
                        "No semantics-preserving simplification rule matched this ability."
                    },
                    "label": if ability.status == "canonical" {
                        "Canonical Rule IR reached"
                    } else {
                        "No simplification available"
                    },
                    "status": ability.status,
                },
                "title": format!("Ability {}", index + 1),
            })
        })
        .collect();
    let vocabulary = vocabulary_items(abilities);
    let primitives = primitive_items(&vocabulary);
    let registrations = abilities
        .iter()
        .filter_map(|ability| ability.rule.as_ref())
        .filter_map(semantic_kind)
        .map(|kind| {
            json!({
                "category": "effectBlock",
                "label": kind,
                "type": "engineRegistration",
            })
        })
        .collect::<Vec<_>>();

    vec![
        OracleAuditStage {
            key: "entityRecognition".to_string(),
            title: "Entity recognition".to_string(),
            status: "ready".to_string(),
            items: recognition_items,
            abilities: Vec::new(),
            terminal: None,
        },
        OracleAuditStage {
            key: "entitySimplifier".to_string(),
            title: "Entity simplifier".to_string(),
            status: if all_canonical {
                "canonical".to_string()
            } else {
                "unsupported".to_string()
            },
            items: Vec::new(),
            abilities: simplifier_abilities,
            terminal: Some(json!({
                "detail": if all_canonical {
                    "Every ability reached canonical Rule IR."
                } else {
                    "At least one ability has no applicable simplification rule."
                },
                "label": if all_canonical {
                    "Canonical Rule IR reached"
                } else {
                    "No simplification available"
                },
                "status": if all_canonical { "canonical" } else { "unsupported" },
            })),
        },
        OracleAuditStage {
            key: "vocabularyExpansion".to_string(),
            title: "Vocabulary expansion".to_string(),
            status: if vocabulary.is_empty() {
                "empty".to_string()
            } else {
                "ready".to_string()
            },
            items: vocabulary,
            abilities: Vec::new(),
            terminal: None,
        },
        OracleAuditStage {
            key: "primitiveExpansion".to_string(),
            title: "Primitive expansion".to_string(),
            status: if primitives.is_empty() {
                "empty".to_string()
            } else {
                "ready".to_string()
            },
            items: primitives,
            abilities: Vec::new(),
            terminal: None,
        },
        OracleAuditStage {
            key: "engineRegistration".to_string(),
            title: "Engine registration".to_string(),
            status: if registrations.is_empty() {
                "empty".to_string()
            } else {
                "ready".to_string()
            },
            items: registrations,
            abilities: Vec::new(),
            terminal: None,
        },
    ]
}
