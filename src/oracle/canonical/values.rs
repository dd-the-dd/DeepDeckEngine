use super::*;

pub(super) fn parse_color_prefix(value: &str) -> Option<(Vec<String>, Vec<String>)> {
    let normalized = value.replace(',', " ").replace(" and ", " ");
    let words = normalized.split_whitespace().collect::<Vec<_>>();
    let mut colors = Vec::new();
    let mut index = 0;
    while let Some(word) = words.get(index) {
        match word.to_ascii_lowercase().as_str() {
            "white" | "blue" | "black" | "red" | "green" => {
                colors.push(word.to_ascii_lowercase());
                index += 1;
            }
            "colorless" => {
                if !colors.is_empty() {
                    return None;
                }
                index += 1;
                break;
            }
            _ => break,
        }
    }
    if index == 0 || index >= words.len() {
        return None;
    }
    Some((
        colors,
        words[index..]
            .iter()
            .map(|word| (*word).to_string())
            .collect(),
    ))
}

pub(super) fn permanent_criteria_pattern() -> &'static str {
    r"[A-Za-z0-9+/' -]+(?:\s+(?:and/or|or|and)\s+[A-Za-z0-9+/' -]+)*"
}

pub(super) fn variable_clause_pattern() -> String {
    format!(
        concat!(
            r"the greatest (?:power|toughness) among {} you control",
            r"|the number of creatures you control of the chosen type",
            r"|the number of experience counters you have",
            r"|the number of cards in your hand",
            r"|the number of cards in your library",
            r"|the number of {} your opponents control",
            r"|the number of {} you control(?: with power \d+ or greater)?",
            r"|the amount of mana spent to cast her",
            r"|the amount of mana spent to cast that spell",
            r"|that spell's mana value",
            r"|your devotion to that color",
            r"|this creature's power",
        ),
        permanent_criteria_pattern(),
        permanent_criteria_pattern(),
        permanent_criteria_pattern(),
    )
}

pub(super) fn x_variable_expression(clause: &str) -> Option<Value> {
    let clause = clause.trim();
    let normalized = clause.to_ascii_lowercase();
    match normalized.as_str() {
        "the number of creatures you control of the chosen type" => Some(json!({
            "kind": "countChosenCreatureType",
            "decisionId": "chosenCreatureType",
        })),
        "the number of experience counters you have" => Some(json!({
            "kind": "countPlayerCounters",
            "player": controller(),
            "counter": "experience",
        })),
        "the number of cards in your hand" => Some(json!({
            "kind": "countCards",
            "zone": { "kind": "hand", "player": controller() },
            "where": Value::Null,
        })),
        "the number of cards in your library" => Some(json!({
            "kind": "countCards",
            "zone": { "kind": "library", "player": controller() },
            "where": Value::Null,
        })),
        "the amount of mana spent to cast her" => Some(json!({ "kind": "manaSpentToCastSource" })),
        "the amount of mana spent to cast that spell" => {
            Some(json!({ "kind": "triggeringSpellManaSpent" }))
        }
        "that spell's mana value" => Some(json!({ "kind": "triggeringSpellManaValue" })),
        "your devotion to that color" => Some(json!({
            "kind": "devotionToChosenColor",
            "player": controller(),
        })),
        "this creature's power" => Some(json!({
            "kind": "powerOf",
            "object": self_ref(),
        })),
        _ => {
            if let Some(captures) = Regex::new(r"(?i)^the number of (.+?) your opponents control$")
                .expect("generic opponent permanent count regex compiles")
                .captures(clause)
            {
                return Some(json!({
                    "kind": "countPermanentsControlledByOpponents",
                    "player": controller(),
                    "where": parse_permanent_criteria(captures.get(1)?.as_str(), "")?,
                }));
            }
            if let Some(captures) = Regex::new(
                r"(?i)^the number of ([a-z0-9+/' -]+) counters? on this (?:artifact|creature|enchantment|land|permanent)$",
            )
            .expect("source counter-count variable regex compiles")
            .captures(clause)
            {
                return Some(json!({
                    "kind": "countCounters",
                    "object": self_ref(),
                    "counter": captures.get(1)?.as_str(),
                }));
            }
            if let Some(captures) =
                Regex::new(r"(?i)^the greatest (power|toughness) among (.+) you control$")
                    .expect("generic greatest-stat variable regex compiles")
                    .captures(clause)
            {
                return Some(json!({
                    "kind": if captures[1].eq_ignore_ascii_case("power") {
                        "greatestPower"
                    } else {
                        "greatestToughness"
                    },
                    "player": controller(),
                    "where": parse_permanent_criteria(captures.get(2)?.as_str(), "")?,
                }));
            }

            Regex::new(r"(?i)^the number of (.+?) you control(?: (with power \d+ or greater))?$")
                .expect("generic count variable regex compiles")
                .captures(clause)
                .and_then(|captures| {
                    let mut criteria = captures.get(1)?.as_str().to_string();
                    if let Some(suffix) = captures.get(2) {
                        criteria.push(' ');
                        criteria.push_str(suffix.as_str());
                    }
                    Some(json!({
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": parse_permanent_criteria(&criteria, "")?,
                    }))
                })
        }
    }
}

pub(super) fn x_variable_text_expression(value: &str) -> Option<Value> {
    let trimmed = value.trim();
    let clause = trimmed.strip_prefix("X, where X is ").unwrap_or(trimmed);
    x_variable_expression(clause)
}

pub(super) fn avatar_quantity(value: &str) -> Option<Value> {
    if let Some(quantity) = parse_number_word(value.trim()) {
        return Some(integer(quantity));
    }
    x_variable_text_expression(value)
}

pub(super) fn parse_consecutive_number_choices(value: &str) -> Option<Vec<i64>> {
    let normalized = value
        .replace(", or ", ",")
        .replace(" or ", ",")
        .replace(", and ", ",")
        .replace(" and ", ",");
    let choices = normalized
        .split(',')
        .map(|choice| parse_number_word(choice.trim()))
        .collect::<Option<Vec<_>>>()?;
    if choices.len() < 2
        || choices
            .iter()
            .enumerate()
            .any(|(index, choice)| *choice != i64::try_from(index + 1).ok().unwrap_or_default())
    {
        return None;
    }
    Some(choices)
}

pub(super) fn value_references_chosen_target(value: &Value, target_id: &str) -> bool {
    if value["kind"].as_str() == Some("chosenTarget") && value["id"].as_str() == Some(target_id) {
        return true;
    }
    match value {
        Value::Array(values) => values
            .iter()
            .any(|value| value_references_chosen_target(value, target_id)),
        Value::Object(values) => values
            .values()
            .any(|value| value_references_chosen_target(value, target_id)),
        _ => false,
    }
}

pub(super) fn earthbend_effect(target_id: &str, quantity: Value) -> Value {
    json!({
        "kind": "earthbend",
        "land": chosen_target(target_id),
        "candidates": {
            "kind": "permanents",
            "controller": controller(),
            "where": card_type("Land"),
        },
        "quantity": quantity,
    })
}

pub(super) fn firebending_ability(quantity: Value) -> Value {
    json!({
        "kind": "firebending",
        "quantity": quantity,
    })
}

pub(super) fn with_token_entry_state(mut effect: Value, tapped: bool) -> Value {
    if tapped {
        effect["tapped"] = Value::Bool(true);
    }
    effect
}
