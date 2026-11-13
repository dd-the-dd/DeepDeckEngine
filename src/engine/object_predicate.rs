//! Definition-only object-predicate evaluation.
//!
//! This is a semantic extraction boundary, not a behavior change. The JSON
//! predicate vocabulary and all of its historical edge cases intentionally
//! match the former `matches_card_filter` implementation. Runtime state such
//! as controller, zone, counters, or effective characteristics does not belong
//! in this view.

use serde_json::Value;

use super::mana::{color_name_to_symbol, parse_mana_symbols};
use super::{
    CardDefinition, card_color_symbols, has_card_subtype, has_card_type, has_keyword,
    integer_value, mana_value, type_line_contains, value_kind,
};

#[derive(Clone, Copy, Debug)]
pub(in crate::engine) struct DefinitionView<'a> {
    definition: &'a CardDefinition,
}

impl<'a> DefinitionView<'a> {
    pub(in crate::engine) fn new(definition: &'a CardDefinition) -> Self {
        Self { definition }
    }

    fn definition(self) -> &'a CardDefinition {
        self.definition
    }
}

fn compare_integers(left: i32, operator: Option<&str>, right: i32) -> bool {
    match operator {
        Some(">=") => left >= right,
        Some(">") => left > right,
        Some("<=") => left <= right,
        Some("<") => left < right,
        Some("==") => left == right,
        _ => false,
    }
}

fn is_permanent_card(card: &CardDefinition) -> bool {
    [
        "Artifact",
        "Battle",
        "Creature",
        "Enchantment",
        "Land",
        "Planeswalker",
    ]
    .into_iter()
    .any(|card_type| has_card_type(card, card_type))
}

/// Evaluates the existing definition-level predicate schema against an
/// immutable card definition. This function deliberately preserves legacy
/// semantics; later ObjectView contexts may compose it without changing it.
pub(in crate::engine) fn matches_object_predicate(
    view: DefinitionView<'_>,
    predicate: &Value,
) -> bool {
    let card = view.definition();
    match value_kind(predicate) {
        Some("cardTypeContains") => predicate["value"]
            .as_str()
            .is_some_and(|value| has_card_type(card, value)),
        Some("subtypeContains") => predicate["value"]
            .as_str()
            .is_some_and(|value| has_keyword(card, "changeling") || has_card_subtype(card, value)),
        Some("typeLineContains") => predicate["value"].as_str().is_some_and(|value| {
            card.type_line
                .to_ascii_lowercase()
                .contains(&value.to_ascii_lowercase())
        }),
        Some("nameEquals") => predicate["value"]
            .as_str()
            .is_some_and(|value| card.name.eq_ignore_ascii_case(value)),
        Some("nameStartsWith") => predicate["value"].as_str().is_some_and(|value| {
            card.name
                .to_ascii_lowercase()
                .starts_with(&value.to_ascii_lowercase())
        }),
        Some("isToken") => card.is_token,
        Some("isCommander") => card.is_commander,
        Some("isLegendary") => card.type_line.starts_with("Legendary "),
        Some("isBasic") => card
            .type_line
            .split_whitespace()
            .any(|word| word.eq_ignore_ascii_case("Basic")),
        Some("isPermanentCard") => is_permanent_card(card),
        Some("totalPowerAndToughnessAtMost") => {
            let maximum = integer_value(&predicate["value"]).unwrap_or(i32::MAX);
            let power = printed_stat(card.power.as_deref());
            let toughness = printed_stat(card.toughness.as_deref());
            power.saturating_add(toughness) <= maximum
        }
        Some("hasActivatedAbility") => card
            .rules
            .iter()
            .any(|rule| value_kind(rule) == Some("activatedAbility")),
        Some("hasKeyword") => predicate["value"]
            .as_str()
            .is_some_and(|keyword| has_keyword(card, keyword)),
        Some("manaCostContainsX") => parse_mana_symbols(&card.mana_cost)
            .iter()
            .any(|symbol| symbol == "X"),
        Some("manaCostEquals") => predicate["value"]
            .as_str()
            .is_some_and(|value| card.mana_cost.eq_ignore_ascii_case(value)),
        Some("historic") => {
            has_card_type(card, "Artifact")
                || card.type_line.starts_with("Legendary ")
                || type_line_contains(&card.type_line, "Saga")
        }
        Some("colorDoesNotContain") => predicate["value"].as_str().is_some_and(|color| {
            let Some(symbol) = color_name_to_symbol(color) else {
                return false;
            };
            has_keyword(card, "devoid") || !card_color_symbols(card).contains(symbol)
        }),
        Some("colorContains") => predicate["value"].as_str().is_some_and(|color| {
            color_name_to_symbol(color)
                .is_some_and(|symbol| card_color_symbols(card).contains(symbol))
        }),
        Some("compare")
            if value_kind(&predicate["left"]) == Some("manaValueOf")
                && value_kind(&predicate["left"]["object"]) == Some("candidate") =>
        {
            let Some(right) =
                integer_value(&predicate["right"]).and_then(|value| i32::try_from(value).ok())
            else {
                return false;
            };
            compare_integers(mana_value(card), predicate["operator"].as_str(), right)
        }
        Some("compare")
            if value_kind(&predicate["left"]) == Some("colorCountOf")
                && value_kind(&predicate["left"]["object"]) == Some("candidate") =>
        {
            let Some(right) =
                integer_value(&predicate["right"]).and_then(|value| i32::try_from(value).ok())
            else {
                return false;
            };
            compare_integers(
                card_color_symbols(card).len() as i32,
                predicate["operator"].as_str(),
                right,
            )
        }
        Some("compare")
            if value_kind(&predicate["left"]) == Some("powerOf")
                && value_kind(&predicate["left"]["object"]) == Some("candidate") =>
        {
            let left = printed_stat(card.power.as_deref());
            let Some(right) = integer_value(&predicate["right"])
                .and_then(|value| i32::try_from(value).ok())
                .or_else(|| {
                    (value_kind(&predicate["right"]) == Some("toughnessOf")
                        && value_kind(&predicate["right"]["object"]) == Some("candidate"))
                    .then(|| printed_stat(card.toughness.as_deref()))
                })
            else {
                return false;
            };
            compare_integers(left, predicate["operator"].as_str(), right)
        }
        Some("compare")
            if value_kind(&predicate["left"]) == Some("toughnessOf")
                && value_kind(&predicate["left"]["object"]) == Some("candidate") =>
        {
            let left = printed_stat(card.toughness.as_deref());
            let Some(right) = integer_value(&predicate["right"])
                .and_then(|value| i32::try_from(value).ok())
                .or_else(|| {
                    (value_kind(&predicate["right"]) == Some("powerOf")
                        && value_kind(&predicate["right"]["object"]) == Some("candidate"))
                    .then(|| printed_stat(card.power.as_deref()))
                })
            else {
                return false;
            };
            compare_integers(left, predicate["operator"].as_str(), right)
        }
        Some("or") => predicate["operands"].as_array().is_some_and(|operands| {
            operands
                .iter()
                .any(|operand| matches_object_predicate(view, operand))
        }),
        Some("and") => predicate["operands"].as_array().is_some_and(|operands| {
            operands
                .iter()
                .all(|operand| matches_object_predicate(view, operand))
        }),
        Some("not") => !matches_object_predicate(view, &predicate["operand"]),
        None if predicate.is_null() => true,
        _ => false,
    }
}

fn printed_stat(value: Option<&str>) -> i32 {
    value
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(0)
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "object_predicate_tests.rs"]
mod tests;
