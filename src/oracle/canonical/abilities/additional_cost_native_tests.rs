use super::*;
use crate::oracle::model::{AbilitySource, RecognizedEntity};
use crate::oracle::syntax::{AbilityInput, recognize_entities};

fn input<'a>(source: &'a AbilitySource, tokens: &'a [RecognizedEntity]) -> AbilityInput<'a> {
    AbilityInput {
        face_name: "Generic Additional Cost",
        face_type_line: "Instant",
        source,
        tokens,
        name_links: &[],
        name_resolution_failure: None,
    }
}

fn lower(text: &str) -> MachineResult<CanonicalRuleDraft> {
    let source = AbilitySource {
        face_id: None,
        text: text.into(),
        line_start: 0,
        line_end: text.len(),
    };
    let tokens = recognize_entities(text);
    let input = input(&source, &tokens);
    assert!(claims_additional_cost_frame(&input), "{text}");
    parse_additional_cost_frame(&input)
}

fn declaration(text: &str) -> Value {
    match lower(text) {
        Ok(draft) => draft.rule["declaration"].clone(),
        Err(failure) => panic!("{text}: {failure:?}"),
    }
}

fn hand_cards(filter: Value) -> Value {
    json!({
        "kind": "cards",
        "zone": { "kind": "hand", "player": { "kind": "controllerOf", "object": { "kind": "self" } } },
        "where": filter,
    })
}

fn mode(value: &str) -> Value {
    json!({
        "kind": "selectionContains",
        "selection": { "kind": "decisionResult", "decisionId": "additionalCostMode" },
        "value": value,
    })
}

/// The mandatory single-action shapes keep the declaration the retired
/// recipe readers produced (CR 601.2h: a choice while casting, not a target).
#[test]
fn mandatory_single_actions_keep_their_declared_choice() {
    assert_eq!(
        declaration("As an additional cost to cast this spell, discard a card."),
        json!({
            "kind": "castingDeclaration",
            "decisions": [{
                "kind": "chooseObjects", "id": "additionalCostCard",
                "candidates": hand_cards(Value::Null),
                "quantity": { "kind": "exactly", "value": { "kind": "integer", "value": 1 } },
            }],
            "additionalCosts": [{ "kind": "discardCard", "card": { "kind": "chosenObject", "id": "additionalCostCard" } }],
        })
    );
    let sacrifice =
        declaration("As an additional cost to cast this spell, sacrifice two creatures.");
    assert_eq!(sacrifice["decisions"][0]["id"], "additionalCostPermanent");
    assert_eq!(sacrifice["decisions"][0]["quantity"]["value"]["value"], 2);
    assert_eq!(
        sacrifice["additionalCosts"],
        json!([
            { "kind": "sacrificePermanent", "permanent": { "kind": "chosenObject", "id": "additionalCostPermanent:0" } },
            { "kind": "sacrificePermanent", "permanent": { "kind": "chosenObject", "id": "additionalCostPermanent:1" } },
        ])
    );
}

/// `A or B` is one announced mode (CR 601.2b); each branch's choice is
/// declared only with its mode and its costs are conditional on it.
#[test]
fn alternatives_are_one_announced_mode_with_conditional_branches() {
    let bone_shards = declaration(
        "As an additional cost to cast this spell, sacrifice a creature or discard a card.",
    );
    assert_eq!(
        bone_shards["decisions"][0]["options"],
        json!(["sacrifice", "discard"])
    );
    assert_eq!(bone_shards["decisions"][1]["condition"], mode("sacrifice"));
    assert_eq!(bone_shards["decisions"][2]["condition"], mode("discard"));
    assert_eq!(
        bone_shards["additionalCosts"][0]["condition"],
        mode("sacrifice")
    );
    assert_eq!(
        bone_shards["additionalCosts"][1]["then"][0]["kind"],
        "discardCard"
    );

    let lightning_axe =
        declaration("As an additional cost to cast this spell, discard a card or pay {5}.");
    assert_eq!(
        lightning_axe["decisions"][0]["options"],
        json!(["discard", "payMana"])
    );
    assert_eq!(
        lightning_axe["additionalCosts"][1]["then"],
        json!([{ "kind": "payMana", "manaCost": "{5}" }])
    );

    // `A, B, or C` (Dusk Mangler).
    let three = declaration(
        "As an additional cost to cast this spell, sacrifice a creature, discard a card, or pay 4 life.",
    );
    assert_eq!(
        three["decisions"][0]["options"],
        json!(["sacrifice", "discard", "payLife"])
    );
}

/// The tap, return, reveal and graveyard-exile branches reuse the same
/// shared payment actions (Disruption Protocol, Wren's Run Vanquisher,
/// Stitched Drake, Deprive).
#[test]
fn chosen_object_actions_lower_to_one_primitive_per_object() {
    let tap = declaration(
        "As an additional cost to cast this spell, tap an untapped artifact you control or pay {1}.",
    );
    assert_eq!(tap["decisions"][1]["id"], "additionalCostTap");
    assert_eq!(
        tap["decisions"][1]["candidates"]["where"]["operands"][1],
        json!({ "kind": "not", "operand": { "kind": "isTapped" } }),
        "only an untapped permanent can pay a tap cost (CR 701.26a)"
    );
    assert_eq!(
        tap["additionalCosts"][0]["then"],
        json!([{ "kind": "tap", "object": { "kind": "chosenObject", "id": "additionalCostTap" } }])
    );

    let reveal = declaration(
        "As an additional cost to cast this spell, reveal an Elf card from your hand or pay {3}.",
    );
    assert_eq!(reveal["decisions"][1]["candidates"]["zone"]["kind"], "hand");
    assert_eq!(
        reveal["additionalCosts"][0]["then"][0]["kind"],
        "revealCard"
    );

    let exile = declaration(
        "As an additional cost to cast this spell, exile a creature card from your graveyard.",
    );
    assert_eq!(
        exile["decisions"][0]["candidates"]["zone"]["kind"],
        "graveyard"
    );
    assert_eq!(exile["additionalCosts"][0]["kind"], "exileObject");

    let deprive = declaration(
        "As an additional cost to cast this spell, return a land you control to its owner's hand.",
    );
    assert_eq!(
        deprive["additionalCosts"][0]["kind"],
        "returnPermanentToOwnersHand"
    );
}

/// An optional cost is a `decline`/`pay` mode the spell may read later.
#[test]
fn optional_cost_is_a_decline_or_pay_mode() {
    let optional = declaration(
        "As an additional cost to cast this spell, you may blight 1. (You may put a -1/-1 counter on a creature you control.)",
    );
    assert_eq!(
        optional["decisions"][0]["options"],
        json!(["decline", "pay"])
    );
    assert_eq!(optional["decisions"][1]["id"], "blightCost1");
    assert_eq!(optional["additionalCosts"][0]["condition"], mode("pay"));
}

/// A previously unused combination: graveyard exile or life, two shared
/// actions never written together by the retired recipes.
#[test]
fn unseen_combination_needs_no_new_production() {
    let combination = declaration(
        "As an additional cost to cast this spell, exile a creature card from your graveyard or pay 3 life.",
    );
    assert_eq!(
        combination["decisions"][0]["options"],
        json!(["exile", "payLife"])
    );
    assert_eq!(
        combination["additionalCosts"][1]["then"][0]["kind"],
        "payLife"
    );
}

/// An unsupported alternative fails explicitly: it never degrades to the
/// other branch.
#[test]
fn unsupported_branch_fails_the_whole_cost() {
    for (text, state) in [
        (
            "As an additional cost to cast this spell, discard a card or lose 3 life.",
            None,
        ),
        (
            "As an additional cost to cast this spell, sacrifice a creature, discard a card.",
            Some("additional-cost.alternatives.list"),
        ),
        (
            "As an additional cost to cast this spell, you may sacrifice a creature or pay {2}.",
            Some("additional-cost.optional.alternatives"),
        ),
        (
            "As an additional cost to cast this spell, discard a card at random.",
            Some("engine.gap.additional-cost-action"),
        ),
    ] {
        let failure = lower(text).expect_err(text);
        if let Some(state) = state {
            assert_eq!(failure.state, state, "{text}");
        }
    }
}
