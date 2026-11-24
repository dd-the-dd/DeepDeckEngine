//! Keyword definitions compile to ordinary reusable runtime instructions.

use serde_json::{Value, json};

use super::{
    CardDefinition, card_filter_supported, cost_supported, distinct_card_types, integer_value,
    player_reference_supported, value_kind,
};

/// Morph (CR 702.37a) and disguise (CR 702.168a) both cast their card face
/// down as a 2/2 creature for {3} and turn it face up for the keyword cost.
pub(super) fn is_face_down_casting_keyword(value: &Value) -> bool {
    matches!(value_kind(value), Some("morph" | "disguise"))
}

/// The rules of the face-down 2/2 a card is cast as (CR 708.2a): none for
/// morph, ward {2} for disguise (CR 702.168a).
pub(super) fn face_down_rules(card: &CardDefinition) -> Vec<Value> {
    let disguised = card.printed_rules().iter().any(|rule| {
        value_kind(rule) == Some("keywordAbility")
            && value_kind(&rule["ability"]) == Some("disguise")
    });
    if !disguised {
        return Vec::new();
    }
    vec![json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "ward", "cost": { "kind": "payMana", "manaCost": "{2}" } },
    })]
}

/// The graveyard cards an escape cost exiles (CR 702.138a): exactly N other
/// cards (`quantity: exactly`), or any number of them satisfying an
/// aggregate condition over the chosen set (`distinctCardTypesAtLeast`).
fn escape_exile_quantity(exile_cost: &Value) -> Option<EscapeExileQuantity> {
    match value_kind(&exile_cost["quantity"]) {
        Some("exactly") => integer_value(&exile_cost["quantity"]["count"])
            .and_then(|count| usize::try_from(count).ok())
            .filter(|count| *count > 0)
            .map(EscapeExileQuantity::Exactly),
        Some("anyNumber")
            if value_kind(&exile_cost["aggregateCondition"])
                == Some("distinctCardTypesAtLeast") =>
        {
            integer_value(&exile_cost["aggregateCondition"]["count"])
                .filter(|count| *count > 0)
                .map(EscapeExileQuantity::DistinctCardTypesAtLeast)
        }
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum EscapeExileQuantity {
    Exactly(usize),
    DistinctCardTypesAtLeast(i32),
}

/// The escape keyword shape the runtime pays: a mana payment and the
/// graveyard exile of other cards.
pub(super) fn escape_ability_supported(value: &Value) -> bool {
    value_kind(value) == Some("escape")
        && value["costs"].as_array().is_some_and(|costs| {
            costs.len() == 2
                && value_kind(&costs[0]) == Some("payMana")
                && cost_supported(&costs[0])
                && value_kind(&costs[1]) == Some("exileGraveyardCards")
                && player_reference_supported(&costs[1]["player"])
                && card_filter_supported(&costs[1]["where"])
                && costs[1]["excludeSource"].is_boolean()
                && escape_exile_quantity(&costs[1]).is_some()
                && costs[1]["decisionId"].is_string()
        })
}

/// The sizes of the graveyard selections that can pay the escape exile
/// among `candidates` cards.
pub(super) fn escape_selection_sizes(
    exile_cost: &Value,
    candidates: usize,
) -> std::ops::RangeInclusive<usize> {
    match escape_exile_quantity(exile_cost) {
        Some(EscapeExileQuantity::Exactly(count)) => count..=count,
        Some(EscapeExileQuantity::DistinctCardTypesAtLeast(_)) => 1..=candidates,
        // An unsupported shape admits no selection (the range is empty).
        None => 1..=0,
    }
}

/// Whether the chosen graveyard cards pay the escape exile.
pub(super) fn escape_selection_satisfies<'a>(
    exile_cost: &Value,
    selected: impl IntoIterator<Item = &'a CardDefinition>,
) -> bool {
    let selected = selected.into_iter().collect::<Vec<_>>();
    match escape_exile_quantity(exile_cost) {
        Some(EscapeExileQuantity::Exactly(count)) => selected.len() == count,
        Some(EscapeExileQuantity::DistinctCardTypesAtLeast(minimum)) => {
            !selected.is_empty()
                && distinct_card_types(selected.into_iter()).len() as i32 >= minimum
        }
        None => false,
    }
}

/// The source mana value is captured from the resulting spell at cast time,
/// including its chosen X. Each trigger owns its rule and transient bindings;
/// resolving or countering the source cannot change the captured threshold.
pub(super) fn cascade_trigger_rule(source_mana_value: i32) -> Value {
    let controller = json!({ "kind": "controllerOf", "object": { "kind": "self" } });
    let below_source = json!({
        "kind": "compare",
        "left": { "kind": "manaValueOf", "object": { "kind": "candidate" } },
        "operator": "<",
        "right": { "kind": "integer", "value": source_mana_value },
    });
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "effects": [
            {
                "kind": "exileFromTopUntil",
                "zone": { "kind": "library", "player": controller.clone() },
                "bind": "cascadeExiled",
                "stopCardBind": "cascadeHit",
                "faceDown": false,
                "stopWhere": {
                    "kind": "and",
                    "operands": [
                        { "kind": "not", "operand": { "kind": "cardTypeContains", "value": "Land" } },
                        below_source.clone(),
                    ],
                },
                "alsoStopsWhen": { "kind": "sourceZoneEmpty" },
            },
            {
                "kind": "castOneCard",
                "player": controller.clone(),
                "cards": { "kind": "boundObjects", "binding": "cascadeHit" },
                "sourceZone": "exile",
                "normalCostsApply": false,
                "optional": true,
                "spellWhere": below_source,
                "bind": "cascadeCast",
            },
            {
                "kind": "moveCards",
                "cards": { "kind": "boundObjects", "binding": "cascadeExiled" },
                "from": { "kind": "exile", "player": controller.clone() },
                "to": { "kind": "library", "player": controller, "position": "bottom" },
                "order": { "kind": "random" },
            },
        ],
    })
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "keyword_programs_tests.rs"]
mod tests;
