//! A choice made as a permanent enters, bound into the permanent's own rules
//! (CR 614.12).
//!
//! `As ... enters, choose a color` stores the value on the permanent
//! (`storedDecisionValue`). Its other abilities read it symbolically
//! (`chosenColor`, `chosenCreatureType`, `chosenCardType` with the stored
//! decision id). When the value is stored, each such reference of the
//! permanent's rules becomes the concrete predicate it denotes, so every
//! consumer (static selectors, trigger filters, costs, effects) evaluates
//! the ordinary characteristic predicate. The rules live on the current
//! object's definition: leaving the battlefield restores the printed
//! definition, so a new object makes a new choice (CR 400.7).
//!
//! The producing replacement and its stored value are left untouched, as is
//! a `choice` slot that names the choice itself rather than filtering by it
//! (`addChosenSubtype`).
use super::*;

/// The characteristic predicate a stored chosen value denotes.
fn bound_predicate(kind: &str, chosen: &str) -> Option<Value> {
    match kind {
        "chosenColor" => {
            let name = match chosen.to_ascii_uppercase().as_str() {
                "W" => "white",
                "U" => "blue",
                "B" => "black",
                "R" => "red",
                "G" => "green",
                _ => color_name_to_symbol(chosen).map(|_| chosen)?,
            };
            Some(json!({ "kind": "colorContains", "value": name.to_ascii_lowercase() }))
        }
        "chosenCreatureType" => Some(json!({ "kind": "subtypeContains", "value": chosen })),
        "chosenCardType" => Some(json!({ "kind": "cardTypeContains", "value": chosen })),
        // CR 201.2: the object's name is the name stored as it entered.
        "chosenCardName" => Some(json!({ "kind": "nameEquals", "value": chosen })),
        _ => None,
    }
}

fn bind(value: &mut Value, decision_id: &str, chosen: &str) {
    if value["decisionId"].as_str() == Some(decision_id)
        && let Some(bound) = value_kind(value).and_then(|kind| bound_predicate(kind, chosen))
    {
        *value = bound;
        return;
    }
    match value {
        Value::Object(values) => values
            .iter_mut()
            .filter(|(key, _)| key.as_str() != "choice")
            .for_each(|(_, value)| bind(value, decision_id, chosen)),
        Value::Array(values) => values
            .iter_mut()
            .for_each(|value| bind(value, decision_id, chosen)),
        _ => {}
    }
}

/// Binds every reference to the stored `decision_id` in the permanent's
/// rules to the chosen value.
pub(super) fn bind_stored_choice(definition: &mut CardDefinition, decision_id: &str, chosen: &str) {
    for rule in definition.printed_rules_mut().iter_mut() {
        if matches!(
            value_kind(rule),
            Some("replacementEffect" | "storedDecisionValue")
        ) {
            continue;
        }
        bind(rule, decision_id, chosen);
    }
}

/// `N mana of the chosen color` (`storedColor`): the color its source
/// stored as it entered. A source without that stored color produces no
/// option (CR 106.5: it can't produce mana of an undefined type).
pub(super) fn stored_color_mana_options(
    value: &Value,
    source: Option<&CardInstance>,
) -> Option<Vec<Vec<String>>> {
    if value_kind(value) != Some("storedColor") {
        return None;
    }
    let amount = integer_value(&value["amount"])
        .and_then(|amount| usize::try_from(amount).ok())
        .unwrap_or(1);
    let symbol = value["decisionId"]
        .as_str()
        .zip(source)
        .and_then(|(decision_id, source)| stored_decision_value(&source.definition, decision_id))
        .map(str::to_ascii_uppercase)
        .filter(|symbol| matches!(symbol.as_str(), "W" | "U" | "B" | "R" | "G"));
    Some(
        symbol
            .map(|symbol| vec![vec![symbol; amount]])
            .unwrap_or_default(),
    )
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "stored_choice_binding_tests.rs"]
mod tests;
