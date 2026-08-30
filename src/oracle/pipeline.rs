use regex::Regex;
use serde_json::{Value, json};

#[path = "canonical/mod.rs"]
mod canonical;

use super::model::{
    OracleAuditStage, OracleCardParseRequest, OracleCardParseResult, ParsedOracleAbility,
    ParserDiagnostic, RecognizedEntity, SimplificationIteration,
};
use super::syntax::{AbilityInput, documents, split_abilities};
use canonical::{CanonicalRuleDraft, parse_canonical_rule};

const PARSER_SCHEMA_VERSION: &str = "oracle-parser/v1";

fn is_class_type_line(type_line: &str) -> bool {
    type_line
        .split(|character: char| !character.is_alphanumeric())
        .any(|part| part.eq_ignore_ascii_case("Class"))
}

fn class_level_header(text: &str) -> Option<i64> {
    let class_level_re = Regex::new(r"^((?:\{[^}]+\})+): Level ([2-9][0-9]*)$")
        .expect("class level header regex compiles");
    class_level_re
        .captures(text.trim())?
        .get(2)?
        .as_str()
        .parse()
        .ok()
}

fn apply_class_level_requirement(rule: &mut Value, minimum_level: i64) {
    if minimum_level <= 1 {
        return;
    }
    let existing = rule["minimumClassLevel"].as_i64().unwrap_or(1);
    rule["minimumClassLevel"] = Value::from(existing.max(minimum_level));
}

fn is_quantity_token(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase().replace('-', " ");
    if normalized == "x" || normalized.parse::<i64>().is_ok() {
        return true;
    }
    let mut words = normalized.split_whitespace();
    let Some(first) = words.next() else {
        return false;
    };
    let first_is_quantity = matches!(
        first,
        "a" | "an"
            | "one"
            | "two"
            | "three"
            | "four"
            | "five"
            | "six"
            | "seven"
            | "eight"
            | "nine"
            | "ten"
            | "eleven"
            | "twelve"
            | "thirteen"
            | "fourteen"
            | "fifteen"
            | "sixteen"
            | "seventeen"
            | "eighteen"
            | "nineteen"
            | "twenty"
            | "thirty"
            | "forty"
            | "fifty"
            | "sixty"
            | "seventy"
            | "eighty"
            | "ninety"
    );
    if !first_is_quantity {
        return false;
    }
    let Some(second) = words.next() else {
        return true;
    };
    words.next().is_none()
        && matches!(
            second,
            "one" | "two" | "three" | "four" | "five" | "six" | "seven" | "eight" | "nine"
        )
}

fn token_type(raw: &str) -> (&'static str, &'static str) {
    let normalized = raw.to_ascii_lowercase();
    if raw.starts_with('{') {
        if normalized == "{t}" || normalized == "{q}" {
            return ("cost", "tapSymbol");
        }
        return ("cost", "manaSymbol");
    }
    if ["when", "whenever", "at"].contains(&normalized.as_str()) {
        return ("trigger", "triggerWord");
    }
    if ["enter", "enters", "entered", "attacks"].contains(&normalized.as_str()) {
        return ("trigger", "eventWord");
    }
    if ["if", "unless", "and", "or", "not", "then", "instead"].contains(&normalized.as_str()) {
        return ("logic", "logicWord");
    }
    if ["this", "that", "it", "its", "you", "your", "they", "their"].contains(&normalized.as_str())
    {
        return ("entityReference", "referenceWord");
    }
    if normalized == "target" {
        return ("entityReference", "targetMarker");
    }
    if [
        "artifact",
        "battle",
        "card",
        "creature",
        "enchantment",
        "instant",
        "land",
        "permanent",
        "planeswalker",
        "player",
        "sorcery",
        "spell",
    ]
    .contains(&normalized.as_str())
    {
        return ("entityReference", "typeWord");
    }
    if [
        "add", "cast", "copy", "counter", "create", "deal", "deals", "discard", "draw", "exile",
        "gain", "look", "lose", "mill", "play", "put", "return", "reveal", "search", "shuffle",
    ]
    .contains(&normalized.as_str())
    {
        return ("magicVocabulary", "actionWord");
    }
    if raw == ":" || raw == "," || raw == "." || raw == "\u{2022}" || raw == "+" {
        return ("boundary", "separator");
    }
    if is_quantity_token(raw) {
        return ("logic", "quantity");
    }
    ("vocabulary", "word")
}

fn recognize_entities(text: &str) -> Vec<RecognizedEntity> {
    let token_re =
        Regex::new(r"\{[^}]+\}|\u{2022}|\+|[.,:;]|[+-]?\d+/[+-]?\d+|[A-Za-z0-9][A-Za-z0-9'/-]*")
            .expect("entity token regex compiles");
    token_re
        .find_iter(text)
        .enumerate()
        .map(|(index, matched)| {
            let raw = matched.as_str();
            let (category, entity_type) = token_type(raw);
            RecognizedEntity {
                category: category.to_string(),
                index,
                raw: raw.to_string(),
                token_type: entity_type.to_string(),
            }
        })
        .collect()
}

fn activation_parts(text: &str) -> Option<(&str, &str)> {
    let text = text
        .split_once(" — ")
        .map(|(_, activation)| activation)
        .unwrap_or(text);
    let activation_re = Regex::new(
        r"(?i)^\s*(?:\(\s*)?(?:(?:\{[^}]+\})+|Pay \d+ life)(?:\s*,\s*(?:(?:\{[^}]+\})+|Pay \d+ life))*\s*:",
    )
    .expect("activation prefix regex compiles");
    if let Some(matched) = activation_re.find(text) {
        return Some((&text[..matched.end() - 1], &text[matched.end()..]));
    }
    let mut parenthesis_depth = 0_u32;
    let mut quoted = false;
    let mut separator = None;
    for (index, character) in text.char_indices() {
        match character {
            '"' => quoted = !quoted,
            '(' if !quoted => parenthesis_depth += 1,
            ')' if !quoted => parenthesis_depth = parenthesis_depth.saturating_sub(1),
            ':' if !quoted && parenthesis_depth == 0 => {
                separator = Some(index);
                break;
            }
            _ => {}
        }
    }
    let separator = separator?;
    let cost = &text[..separator];
    let effect = &text[separator + 1..];
    let lower = cost.trim().to_ascii_lowercase();
    (lower.starts_with("sacrifice ")
        || lower.starts_with("remove ")
        || lower.starts_with("equip—")
        || lower.contains(", sacrifice "))
    .then_some((cost, effect))
}

fn classify_ability(input: &AbilityInput<'_>) -> &'static str {
    let text = input.source.text.trim();
    let normalized = text.trim_start_matches('(').trim_start();
    let lower = normalized.to_ascii_lowercase();

    if activation_parts(text).is_some() {
        return "activatedAbility";
    }
    if lower.starts_with("when ") || lower.starts_with("whenever ") || lower.starts_with("at ") {
        return "triggeredAbility";
    }
    let standalone_enters_with = !lower.starts_with("graft ")
        && lower
            .split_once(" enters with ")
            .is_some_and(|(subject, _)| !subject.contains('.'));
    let standalone_enters_tapped = lower
        .strip_suffix(" enters tapped.")
        .is_some_and(|subject| !subject.contains('.'));
    if Regex::new(r"(?i)^as .+ enters,")
        .expect("as-enters classifier regex compiles")
        .is_match(normalized)
        || lower.contains(" enters tapped unless ")
        || standalone_enters_with
        || standalone_enters_tapped
        || lower.contains(" enters prepared")
    {
        return "replacementEffect";
    }
    let keyword_list_candidate = (lower.contains(",") || lower.contains(" and "))
        && (Regex::new(
            r"(?i)^(changeling|deathtouch|defender|devoid|double strike|first strike|flash|flying|haste|hexproof|indestructible|lifelink|menace|myriad|prowess|reach|trample|vigilance)\b|^firebending\s+(\d+|x\b)|^mobilize\s+\d+|^ward(\s+\{|—|â€”|Ã¢â‚¬â€)",
        )
        .expect("keyword list classifier regex compiles")
        .is_match(normalized)
            || Regex::new(r"(?i)^blitz\s+\{")
                .expect("blitz keyword list classifier regex compiles")
                .is_match(normalized));
    if keyword_list_candidate {
        return "keywordAbilityGroup";
    }
    let firebending_keyword_candidate = Regex::new(r"(?i)^firebending\s+(\d+|x\b)")
        .expect("firebending keyword classifier regex compiles")
        .is_match(normalized);
    if lower == "flying"
        || lower.starts_with("graft ")
        || lower.starts_with("kicker ")
        || lower.starts_with("blitz ")
        || lower.starts_with("ward")
        || firebending_keyword_candidate
        || lower.starts_with("offspring ")
        || lower.starts_with("paradigm ")
    {
        return "keywordAbility";
    }
    if lower.starts_with("this spell can't be countered")
        || lower.starts_with("activated abilities of sources")
        || lower.starts_with("lands with the chosen name")
        || lower.starts_with("creatures entering don't cause")
    {
        return "staticAbility";
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

fn semantic_kind(value: &Value) -> Option<&str> {
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

fn iterations_for_rule(
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

fn unsupported_iteration(
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

fn build_stages(abilities: &[ParsedOracleAbility]) -> Vec<OracleAuditStage> {
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

pub fn parse_oracle_card(request: OracleCardParseRequest) -> OracleCardParseResult {
    let context = request.clone();
    let mut abilities = Vec::new();
    for document in documents(&request) {
        let is_class = is_class_type_line(document.face_type_line);
        let mut minimum_class_level = 1_i64;
        for source in split_abilities(&document) {
            let announced_class_level =
                is_class.then(|| class_level_header(&source.text)).flatten();
            let ability_index = abilities.len();
            let entities = recognize_entities(&source.text);
            let input = AbilityInput {
                face_name: document.face_name,
                face_type_line: document.face_type_line,
                source: &source,
            };
            let ability_kind = classify_ability(&input);
            match parse_canonical_rule(&input, ability_kind) {
                Some(mut draft) => {
                    if announced_class_level.is_none() {
                        apply_class_level_requirement(&mut draft.rule, minimum_class_level);
                    }
                    let iterations =
                        iterations_for_rule(&input, ability_index, ability_kind, &draft);
                    let final_ability_kind = semantic_kind(&draft.rule)
                        .unwrap_or(ability_kind)
                        .to_string();
                    abilities.push(ParsedOracleAbility {
                        source,
                        ability_type: final_ability_kind,
                        status: "canonical".to_string(),
                        rule: Some(draft.rule),
                        entities,
                        iterations,
                        diagnostics: Vec::new(),
                    });
                }
                None => {
                    let iterations = unsupported_iteration(&input, ability_index, ability_kind);
                    abilities.push(ParsedOracleAbility {
                        source,
                        ability_type: ability_kind.to_string(),
                        status: "unsupported".to_string(),
                        rule: None,
                        entities,
                        iterations,
                        diagnostics: vec![ParserDiagnostic {
                            code: "unsupported_oracle_ability".to_string(),
                            message:
                                "No semantics-preserving simplification rule matched this ability."
                                    .to_string(),
                            severity: "unsupported".to_string(),
                        }],
                    });
                }
            }
            if let Some(level) = announced_class_level {
                minimum_class_level = level;
            }
        }
    }
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
        context,
        abilities,
        stages,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed_rule_for_text<'a>(result: &'a OracleCardParseResult, text: &str) -> &'a Value {
        result
            .abilities
            .iter()
            .find(|ability| ability.source.text == text)
            .and_then(|ability| ability.rule.as_ref())
            .expect("expected Oracle line to have a canonical rule")
    }

    #[test]
    fn class_sections_apply_the_announced_minimum_level_to_following_abilities() {
        let result = parse_oracle_card(OracleCardParseRequest {
            card_name: "Scavenger's Talent".to_string(),
            type_line: "Enchantment - Class".to_string(),
            mana_cost: Some("{B}".to_string()),
            oracle_text: Some(
                "(Gain the next level as a sorcery to add its ability.)\n\
                 Whenever one or more creatures you control die, create a Food token. This ability triggers only once each turn.\n\
                 {1}{B}: Level 2\n\
                 Whenever you sacrifice a permanent, target player mills two cards.\n\
                 {2}{B}: Level 3\n\
                 At the beginning of your end step, you may sacrifice three other nonland permanents. If you do, return a creature card from your graveyard to the battlefield with a finality counter on it."
                    .to_string(),
            ),
            layout: Some("class".to_string()),
            faces: Vec::new(),
        });

        let level_one = parsed_rule_for_text(
            &result,
            "Whenever one or more creatures you control die, create a Food token. This ability triggers only once each turn.",
        );
        assert!(level_one.get("minimumClassLevel").is_none());

        let level_two_activation = parsed_rule_for_text(&result, "{1}{B}: Level 2");
        assert!(level_two_activation.get("minimumClassLevel").is_none());
        assert_eq!(
            level_two_activation["activationCondition"]["value"]["value"],
            1
        );

        let level_two = parsed_rule_for_text(
            &result,
            "Whenever you sacrifice a permanent, target player mills two cards.",
        );
        assert_eq!(level_two["minimumClassLevel"], 2);

        let level_three = parsed_rule_for_text(
            &result,
            "At the beginning of your end step, you may sacrifice three other nonland permanents. If you do, return a creature card from your graveyard to the battlefield with a finality counter on it.",
        );
        assert_eq!(level_three["minimumClassLevel"], 3);
    }

    #[test]
    fn class_level_annotation_is_not_inferred_for_non_class_cards() {
        let result = parse_oracle_card(OracleCardParseRequest {
            card_name: "Not a Class".to_string(),
            type_line: "Enchantment".to_string(),
            mana_cost: Some("{B}".to_string()),
            oracle_text: Some(
                "{1}{B}: Level 2\nWhenever you sacrifice a permanent, target player mills two cards."
                    .to_string(),
            ),
            layout: None,
            faces: Vec::new(),
        });

        let trigger = parsed_rule_for_text(
            &result,
            "Whenever you sacrifice a permanent, target player mills two cards.",
        );
        assert!(trigger.get("minimumClassLevel").is_none());
    }

    #[test]
    fn no_lands_first_unsupported_batch_parses_to_executable_rules() {
        let cards = [
            (
                "Aftermath Analyst",
                "Creature - Elf Detective",
                Some("{1}{G}"),
                "When this creature enters, mill three cards. (Put the top three cards of your library into your graveyard.)\n{3}{G}, Sacrifice this creature: Return all land cards from your graveyard to the battlefield tapped.",
            ),
            (
                "Ashaya, Soul of the Wild",
                "Legendary Creature - Elemental",
                Some("{3}{G}{G}"),
                "Ashaya's power and toughness are each equal to the number of lands you control.\nNontoken creatures you control are Forest lands in addition to their other types. (They're still affected by summoning sickness.)",
            ),
            (
                "Braids, Arisen Nightmare",
                "Legendary Creature - Nightmare",
                Some("{1}{B}{B}"),
                "At the beginning of your end step, you may sacrifice an artifact, creature, enchantment, land, or planeswalker. If you do, each opponent may sacrifice a permanent of their choice that shares a card type with it. For each opponent who doesn't, that player loses 2 life and you draw a card.",
            ),
            (
                "Constant Mists",
                "Instant",
                Some("{1}{G}"),
                "Buyback\u{2014}Sacrifice a land. (You may sacrifice a land in addition to any other costs as you cast this spell. If you do, put this card into your hand as it resolves.)\nPrevent all combat damage that would be dealt this turn.",
            ),
            (
                "Deflecting Swat",
                "Instant",
                Some("{2}{R}"),
                "If you control a commander, you may cast this spell without paying its mana cost.\nYou may choose new targets for target spell or ability.",
            ),
            (
                "Dosan the Falling Leaf",
                "Legendary Creature - Human Monk",
                Some("{1}{G}{G}"),
                "Players can cast spells only during their own turns.",
            ),
            (
                "Exploration Broodship",
                "Artifact - Spacecraft",
                Some("{4}{G}"),
                "Station (Tap another creature you control: Put charge counters equal to its power on this Spacecraft. Station only as a sorcery. It's an artifact creature at 8+.)\n3+ | You may play an additional land on each of your turns.\n8+ | Flying\nOnce during each of your turns, you may cast a permanent spell from your graveyard by sacrificing a land in addition to paying its other costs.",
            ),
            (
                "Field of the Dead",
                "Land",
                None,
                "This land enters tapped.\n{T}: Add {C}.\nWhenever this land or another land you control enters, if you control seven or more lands with different names, create a 2/2 black Zombie creature token.",
            ),
            (
                "Glacial Chasm",
                "Land",
                None,
                "Cumulative upkeep\u{2014}Pay 2 life. (At the beginning of your upkeep, put an age counter on this permanent, then sacrifice it unless you pay its upkeep cost for each age counter on it.)\nWhen this land enters, sacrifice a land.\nCreatures you control can't attack.\nPrevent all damage that would be dealt to you.",
            ),
            (
                "Green Sun's Zenith",
                "Sorcery",
                Some("{X}{G}"),
                "Search your library for a green creature card with mana value X or less, put it onto the battlefield, then shuffle. Shuffle Green Sun's Zenith into its owner's library.",
            ),
        ];

        for (card_name, type_line, mana_cost, oracle_text) in cards {
            let result = parse_oracle_card(OracleCardParseRequest {
                card_name: card_name.to_string(),
                type_line: type_line.to_string(),
                mana_cost: mana_cost.map(str::to_string),
                oracle_text: Some(oracle_text.to_string()),
                layout: None,
                faces: Vec::new(),
            });
            assert_eq!(result.status, "canonical", "{card_name} should parse");
            assert!(
                result.abilities.iter().all(|ability| {
                    ability.status == "canonical"
                        && ability
                            .rule
                            .as_ref()
                            .is_some_and(crate::engine::rule_is_executable)
                }),
                "every {card_name} ability should compile for the engine"
            );
        }
    }

    #[test]
    fn no_lands_linked_mana_and_draw_choice_compile_to_executable_rules() {
        let cards = [
            (
                "Squandered Resources",
                "Enchantment",
                Some("{B}{G}"),
                "Sacrifice a land: Add one mana of any type the sacrificed land could produce.",
            ),
            (
                "Sylvan Library",
                "Enchantment",
                Some("{1}{G}"),
                "At the beginning of your draw step, you may draw two additional cards. If you do, choose two cards in your hand drawn this turn. For each of those cards, pay 4 life or put the card on top of your library.",
            ),
        ];

        for (card_name, type_line, mana_cost, oracle_text) in cards {
            let result = parse_oracle_card(OracleCardParseRequest {
                card_name: card_name.to_string(),
                type_line: type_line.to_string(),
                mana_cost: mana_cost.map(str::to_string),
                oracle_text: Some(oracle_text.to_string()),
                layout: None,
                faces: Vec::new(),
            });
            assert_eq!(result.status, "canonical", "{card_name} should parse");
            assert!(
                result.abilities.iter().all(|ability| {
                    ability.status == "canonical"
                        && ability
                            .rule
                            .as_ref()
                            .is_some_and(crate::engine::rule_is_executable)
                }),
                "every {card_name} ability should compile for the engine"
            );
        }

        let unsupported = parse_oracle_card(OracleCardParseRequest {
            card_name: "Broken Linked Mana".to_string(),
            type_line: "Enchantment".to_string(),
            mana_cost: None,
            oracle_text: Some(
                "{T}: Add one mana of any type the sacrificed land could produce.".to_string(),
            ),
            layout: None,
            faces: Vec::new(),
        });
        assert_eq!(unsupported.status, "unsupported");
    }
}
