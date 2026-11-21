use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_top_cards_partition_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let Some(boundary) = tokens.iter().position(|token| token.raw == ".") else {
        return false;
    };
    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("look"))
        && tokens
            .get(boundary + 1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("put"))
        && tokens.last().is_some_and(|token| token.raw == ".")
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("rest"))
        && !tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("instead"))
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": self_ref() })
}

pub(in crate::oracle::canonical) fn lower_top_cards_partition_effects(
    partition: &PartitionCollectionNode,
    selection_count: Value,
) -> Vec<Value> {
    let remainder = json!({
        "kind": "setDifference",
        "left": { "kind": "boundObjects", "binding": "lookedCards" },
        "right": decision_result("cardsForHand"),
    });
    let mut effects = vec![
        json!({
            "kind": "chooseCards",
            "id": "cardsForHand",
            "player": controller(),
            "from": { "kind": "boundObjects", "binding": "lookedCards" },
            "count": selection_count,
        }),
        json!({
            "kind": "moveCards",
            "cards": decision_result("cardsForHand"),
            "to": { "kind": "hand", "player": controller() },
        }),
    ];
    match partition.order {
        CollectionOrder::Chosen => {
            effects.push(json!({
                "kind": "chooseOrder",
                "id": "remainderOrder",
                "player": controller(),
                "objects": remainder,
            }));
            effects.push(json!({
                "kind": "moveCards",
                "cards": decision_result("remainderOrder"),
                "to": { "kind": "library", "player": controller(), "position": "bottom" },
                "order": { "kind": "decisionOrder", "decisionId": "remainderOrder" },
            }));
        }
        CollectionOrder::Random => effects.push(json!({
            "kind": "moveCards",
            "cards": remainder,
            "to": { "kind": "library", "player": controller(), "position": "bottom" },
            "order": { "kind": "random" },
        })),
    }
    effects
}

pub(in crate::oracle::canonical) fn parse_promoted_top_cards_partition(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("top-cards-partition", input.tokens);
    let sequence = recognize_top_cards_partition_sequence(&mut cursor)?;
    cursor.enter("sequence.top-cards.validation");
    cursor.require(
        sequence.top.zone == LocalZone::Library
            && sequence.top.owner == TopCardsLibraryOwnerNode::Controller,
        "a controller library source",
    )?;
    cursor.require(
        sequence.partition.binding == sequence.top.binding,
        "a partition of the previously bound collection",
    )?;
    cursor.require(
        sequence.partition.chosen_destination == LocalZone::Hand
            && sequence.partition.remainder_destination == LocalZone::Library,
        "hand selection and library-bottom remainder destinations",
    )?;
    let mut effects = vec![json!({
        "kind": "lookAtTopCards",
        "zone": { "kind": "library", "player": controller() },
        "count": integer(sequence.top.count),
        "bind": "lookedCards",
    })];
    effects.extend(lower_top_cards_partition_effects(
        &sequence.partition,
        json!({
            "kind": "minimum",
            "operands": [
                integer(sequence.partition.chosen_count),
                { "kind": "countObjects", "objects": { "kind": "boundObjects", "binding": "lookedCards" } },
            ],
        }),
    ));
    Ok(draft(
        json!({ "kind": "spellAbility", "source": self_ref(), "effects": effects }),
        &[
            "Recognize a bound top-card collection",
            "Partition the collection through typed references",
            "Lower selection, ordering, and moves to primitive effects",
        ],
    ))
}
