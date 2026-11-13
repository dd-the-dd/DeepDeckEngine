use super::super::*;
use crate::oracle::syntax::AbilityInput;

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": self_ref() })
}

pub(in crate::oracle::canonical) fn claims_promoted_top_cards_reorder_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("look"))
        && tokens
            .get(7)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("your"))
        && tokens
            .get(8)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("library"))
        && tokens.windows(3).any(|window| {
            window[0].raw == ","
                && window[1].raw.eq_ignore_ascii_case("then")
                && window[2].raw.eq_ignore_ascii_case("put")
        })
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("back"))
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("shuffle"))
        && tokens.last().is_some_and(|token| token.raw == ".")
}

pub(in crate::oracle::canonical) fn parse_promoted_top_cards_reorder(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("top-cards-reorder", input.tokens);
    let sequence = recognize_top_cards_reorder_sequence(&mut cursor)?;
    cursor.enter("top-cards-reorder.binding-validation");
    cursor.require(
        sequence.top.zone == LocalZone::Library
            && sequence.top.owner == TopCardsLibraryOwnerNode::Controller,
        "a controller library source",
    )?;
    cursor.require(
        sequence.reorder.binding == sequence.top.binding,
        "the previously inspected collection",
    )?;
    cursor.require(
        sequence.reorder.destination == sequence.top.zone
            && sequence.reorder.position == LibraryPosition::Top,
        "the inspected zone at its top position",
    )?;
    cursor.require(
        sequence.reorder.order == CollectionOrder::Chosen,
        "a controller-chosen order",
    )?;
    cursor.require(
        sequence.shuffle.player == LocalPlayerReference::Controller
            && sequence.shuffle.zone == sequence.top.zone,
        "an optional shuffle of the inspected library",
    )?;

    let player = controller();
    let looked = "lookedCards";
    let order = "topOrder";
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": [
                {
                    "kind": "lookAtTopCards",
                    "zone": library(player.clone()),
                    "count": integer(sequence.top.count),
                    "bind": looked,
                },
                {
                    "kind": "chooseOrder",
                    "id": order,
                    "player": player.clone(),
                    "objects": bound_objects(looked),
                },
                {
                    "kind": "moveCards",
                    "cards": decision_result(order),
                    "to": {
                        "kind": "library",
                        "player": player.clone(),
                        "position": "top",
                    },
                    "order": {
                        "kind": "decisionOrder",
                        "decisionId": order,
                    },
                },
                {
                    "kind": "optionalEffects",
                    "player": player.clone(),
                    "effects": [{
                        "kind": "shuffleZone",
                        "zone": library(player),
                    }],
                },
            ],
        }),
        &[
            "Recognize and bind one top-card collection",
            "Resolve the collection pronoun through the typed binding",
            "Lower ordering and movement as independent primitives",
            "Link the optional shuffle to the same inspected library",
            &trace,
        ],
    ))
}

pub(in crate::oracle::canonical) fn claims_promoted_plain_draw_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() == 4
        && tokens
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("draw"))
        && tokens.get(2).is_some_and(|token| {
            token.raw.eq_ignore_ascii_case("card") || token.raw.eq_ignore_ascii_case("cards")
        })
        && tokens.last().is_some_and(|token| token.raw == ".")
}

pub(in crate::oracle::canonical) fn parse_promoted_plain_draw(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("plain-draw", input.tokens);
    let draw = recognize_draw_cards_effect(&mut cursor)?;
    cursor.enter("effect.plain-draw.terminator");
    cursor.consume(".")?;
    cursor.enter("effect.plain-draw.complete");
    cursor.finish()?;
    let LocalPlayerReference::Controller = draw.player else {
        return Err(cursor.fail("the source controller as the drawing player"));
    };
    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": [{
                "kind": "drawCards",
                "player": controller(),
                "count": lower_numeric_expression(&draw.count)
                    .expect("plain draw grammar has no target-property binding"),
            }],
        }),
        &[
            "Recognize a reusable draw effect",
            "Lower the numeric count and controller reference independently",
        ],
    ))
}
