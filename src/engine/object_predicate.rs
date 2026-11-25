//! Definition-only object-predicate evaluation.
//!
//! Shared definition predicates keep changeling restricted to the versioned
//! creature-subtype domain. Runtime state such as controller, zone, or
//! effective characteristics does not belong in this immutable view. The one
//! piece of object state it reads is the rule-metadata gate (level/station
//! bands, Class levels, Room doors): a permanent's printed abilities inside a
//! closed band are not its abilities (CR 711.2a, 722.2a), and an object off
//! the battlefield reads every gate at rest.

use serde_json::Value;

use super::mana::{color_name_to_symbol, parse_mana_symbols};
use super::{
    CardDefinition, CardInstance, GameEngine, card_color_symbols, has_card_subtype, has_card_type,
    integer_value, keyword_rule_supported, mana_value, rule_contains_kind, type_line_contains,
    value_kind,
};

#[derive(Clone, Copy, Debug)]
pub(in crate::engine) struct DefinitionView<'a> {
    definition: &'a CardDefinition,
    /// The permanent whose printed rules are read; `None` off the battlefield.
    permanent: Option<&'a CardInstance>,
}

impl<'a> DefinitionView<'a> {
    pub(in crate::engine) fn new(definition: &'a CardDefinition) -> Self {
        Self {
            definition,
            permanent: None,
        }
    }

    /// The printed layer of a battlefield permanent, behind its band gates.
    pub(in crate::engine) fn for_permanent(permanent: &'a CardInstance) -> Self {
        Self {
            definition: &permanent.definition,
            permanent: Some(permanent),
        }
    }

    pub(super) fn definition(self) -> &'a CardDefinition {
        self.definition
    }

    /// Whether a printed rule's metadata gate is open for this object.
    pub(super) fn rule_gate_open(self, rule: &Value) -> bool {
        match self.permanent {
            Some(permanent) => GameEngine::rule_metadata_active_for_source(permanent, rule),
            None => GameEngine::rule_metadata_active_off_battlefield(rule),
        }
    }

    /// Supported printed keyword rules whose gate is open.
    pub(super) fn keyword_rules(self) -> impl Iterator<Item = &'a Value> {
        self.definition
            .printed_rules()
            .iter()
            .filter(move |rule| keyword_rule_supported(rule) && self.rule_gate_open(rule))
    }

    pub(super) fn has_keyword(self, keyword: &str) -> bool {
        self.keyword_rules()
            .any(|rule| rule_contains_kind(rule, keyword))
    }
}

impl<'a> From<&'a CardDefinition> for DefinitionView<'a> {
    fn from(definition: &'a CardDefinition) -> Self {
        Self::new(definition)
    }
}

pub(super) fn compare_integers(left: i32, operator: Option<&str>, right: i32) -> bool {
    match operator {
        Some(">=") => left >= right,
        Some(">") => left > right,
        Some("<=") => left <= right,
        Some("<") => left < right,
        Some("==") => left == right,
        _ => false,
    }
}

/// Changeling grants creature subtypes only. Membership comes from the same
/// versioned canonical registry as Oracle, without depending on its compiler.
pub(super) fn is_creature_subtype(value: &str) -> bool {
    static TYPES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let types = TYPES.get_or_init(|| {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Registry {
            schema_version: String,
            canonical: Vec<String>,
        }
        let registry: Registry = serde_json::from_str(include_str!(
            "../../data/oracle-vocabulary/creature-subtypes-cr-205.3m-2026-11-06.json"
        ))
        .expect("embedded creature-subtype registry must deserialize");
        assert_eq!(registry.schema_version, "deepdeck-creature-subtypes/v1");
        let mut names: Vec<_> = registry
            .canonical
            .into_iter()
            .map(|name| name.to_ascii_lowercase())
            .collect();
        assert!(
            !names.is_empty(),
            "creature-subtype registry must be nonempty"
        );
        names.sort_unstable();
        assert!(
            names.windows(2).all(|pair| pair[0] != pair[1]),
            "duplicate creature subtype"
        );
        names
    });
    // Compare folded bytes without allocating a new normalized String per query.
    types
        .binary_search_by(|candidate| {
            candidate
                .bytes()
                .cmp(value.bytes().map(|b| b.to_ascii_lowercase()))
        })
        .is_ok()
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
/// immutable card definition. Creature-subtype membership is shared with
/// effective permanent predicates; other subtype families remain explicit.
pub(in crate::engine) fn matches_object_predicate(
    view: DefinitionView<'_>,
    predicate: &Value,
) -> bool {
    match try_evaluate_predicate(predicate, &mut |leaf| {
        Ok::<_, std::convert::Infallible>(matches_object_atom(view, leaf))
    }) {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

/// The filter of a card-types measure over graveyard cards: absent, or a
/// predicate every leaf of which the definition view evaluates.
pub(super) fn distinct_card_type_filter_supported(filter: &Value) -> bool {
    filter.is_null()
        || try_map_predicate_leaves(filter, 0, &mut |leaf| {
            if definition_atom_supported(leaf) {
                Ok(leaf.clone())
            } else {
                Err(super::EngineError::new(
                    "unsupported card-types measure filter",
                ))
            }
        })
        .is_ok()
}

/// Property applicability of this definition view, independent of a caller's
/// broad IR schema. State-only predicates cannot become false under NOT.
pub(super) fn definition_atom_supported(predicate: &Value) -> bool {
    match super::value_kind(predicate) {
        Some(
            "cardTypeContains"
            | "subtypeContains"
            | "typeLineContains"
            | "nameEquals"
            | "nameStartsWith"
            | "isToken"
            | "isCommander"
            | "isLegendary"
            | "isBasic"
            | "isPermanentCard"
            | "hasActivatedAbility"
            | "hasKeyword"
            | "manaCostContainsX"
            | "manaCostEquals"
            | "historic"
            | "colorDoesNotContain"
            | "colorContains",
        ) => super::card_filter_supported(predicate),
        Some("compare") => {
            matches!(
                super::value_kind(&predicate["left"]),
                Some("manaValueOf" | "colorCountOf")
            ) && super::card_filter_supported(predicate)
        }
        _ => false,
    }
}

pub(super) fn matches_object_atom(view: DefinitionView<'_>, predicate: &Value) -> bool {
    let card = view.definition();
    match value_kind(predicate) {
        Some("cardTypeContains") => predicate["value"]
            .as_str()
            .is_some_and(|value| has_card_type(card, value)),
        Some("subtypeContains") => predicate["value"].as_str().is_some_and(|value| {
            (is_creature_subtype(value) && view.has_keyword("changeling"))
                || has_card_subtype(card, value)
        }),
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
            .printed_rules()
            .iter()
            .any(|rule| value_kind(rule) == Some("activatedAbility")),
        Some("hasKeyword") => predicate["value"]
            .as_str()
            .is_some_and(|keyword| view.has_keyword(keyword)),
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
            view.has_keyword("devoid") || !card_color_symbols(card).contains(symbol)
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
        None if predicate.is_null() => true,
        _ => false,
    }
}

/// One lazy Boolean evaluator. Legacy leaves are genuinely Infallible;
/// checked roles keep their own error type and never convert it to false.
pub(super) fn try_evaluate_predicate<E>(
    predicate: &Value,
    leaf: &mut impl FnMut(&Value) -> Result<bool, E>,
) -> Result<bool, E> {
    match super::value_kind(predicate) {
        Some("and" | "or") => {
            let Some(values) = predicate["operands"].as_array() else {
                return Ok(false);
            };
            let conjunction = super::value_kind(predicate) == Some("and");
            for value in values {
                let result = try_evaluate_predicate(value, leaf)?;
                if result != conjunction {
                    return Ok(result);
                }
            }
            Ok(conjunction)
        }
        Some("not") => Ok(!try_evaluate_predicate(&predicate["operand"], leaf)?),
        None if predicate.is_null() => Ok(true),
        _ => leaf(predicate),
    }
}

/// Eager parameter preparation over exactly the same structural algebra.
pub(super) fn try_map_predicate_leaves(
    predicate: &Value,
    depth: usize,
    leaf: &mut impl FnMut(&Value) -> Result<Value, super::EngineError>,
) -> Result<Value, super::EngineError> {
    if depth >= 64 {
        return Err(super::EngineError::new(
            "object query nesting exceeds supported depth",
        ));
    }
    let mut result = predicate.clone();
    match super::value_kind(predicate) {
        Some("and" | "or") => {
            let values = predicate["operands"]
                .as_array()
                .ok_or_else(|| super::EngineError::new("invalid object query operands"))?;
            result["operands"] = Value::Array(
                values
                    .iter()
                    .map(|v| try_map_predicate_leaves(v, depth + 1, leaf))
                    .collect::<Result<Vec<_>, _>>()?,
            );
            Ok(result)
        }
        Some("not") => {
            let operand = predicate
                .get("operand")
                .ok_or_else(|| super::EngineError::new("missing object query NOT operand"))?;
            result["operand"] = try_map_predicate_leaves(operand, depth + 1, leaf)?;
            Ok(result)
        }
        None if predicate.is_null() => Ok(result),
        _ => leaf(predicate),
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

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "object_subtype_domain_tests.rs"]
mod subtype_domain_tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "object_query_relations_runtime_tests.rs"]
mod object_query_relations_runtime_tests;
