use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the structural activated envelope while leaving every cost, target,
/// count, and temporal decision to its typed child machine. Malformed members
/// remain claimed so their committed state is reported instead of falling
/// through to legacy parsing.
pub(in crate::oracle::canonical) fn claims_promoted_activated_top_card_delayed_draw_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let Some(separator) = tokens.iter().position(|token| token.raw == ":") else {
        return false;
    };
    let supported_cost = tokens.first().is_some_and(|token| {
        token.token_type == "manaSymbol"
            || token.token_type == "tapSymbol"
            || token.raw.eq_ignore_ascii_case("pay")
            || token.raw.eq_ignore_ascii_case("sacrifice")
    });
    supported_cost
        && tokens
            .get(separator + 1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("look"))
        && tokens
            .iter()
            .skip(separator + 1)
            .any(|token| token.raw.eq_ignore_ascii_case("beginning"))
        && tokens
            .iter()
            .skip(separator + 1)
            .any(|token| token.raw.eq_ignore_ascii_case("next"))
}

fn lower_delayed_child_effect(
    cursor: &OracleTokenCursor<'_>,
    effect: &LocalEffectNode,
) -> MachineResult<Value> {
    let LocalEffectNode::DrawCards(draw) = effect else {
        return Err(cursor.fail("a drawCards child effect for this promoted envelope"));
    };
    let LocalPlayerReference::Controller = draw.player else {
        return Err(cursor.fail("the source controller as the delayed drawing player"));
    };
    Ok(json!({
        "kind": "drawCards",
        "player": controller(),
        "count": lower_numeric_expression(&draw.count)
            .expect("delayed draw grammar has no target-property numeric binding"),
    }))
}

pub(in crate::oracle::canonical) fn parse_promoted_activated_top_card_delayed_draw(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("activated-top-card-delayed-draw", input.tokens);
    let ability = recognize_activated_top_card_delayed_draw(&mut cursor)?;
    cursor.enter("activation.top-card-delayed-draw.validation");
    cursor.require(
        !ability.costs.costs.is_empty(),
        "at least one independent activation cost",
    )?;
    cursor.require(
        ability.inspection.zone == LocalZone::Library,
        "a library top-card inspection",
    )?;

    let costs = ability
        .costs
        .costs
        .iter()
        .map(lower_local_activation_cost)
        .collect::<Vec<_>>();
    let (library_owner, decisions) = match &ability.inspection.owner {
        TopCardsLibraryOwnerNode::Controller => (controller(), Vec::new()),
        TopCardsLibraryOwnerNode::TargetPlayer(target) => {
            let (target_id, candidates, minimum, maximum) = lower_single_target(target);
            (
                chosen_target(target_id),
                vec![target_decision(target_id, candidates, minimum, maximum)],
            )
        }
    };
    let delayed_step = match ability.delayed.step {
        DelayedTurnStep::Upkeep => "upkeep",
        DelayedTurnStep::EndStep => "endStep",
    };
    cursor.require(
        ability.delayed.turn == DelayedTurnReference::NextTurn,
        "the next turn as the delayed turn reference",
    )?;
    let delayed_child = lower_delayed_child_effect(&cursor, &ability.delayed.effect)?;
    let mut rule = json!({
        "kind": "activatedAbility",
        "source": self_ref(),
        "costs": costs,
        "effects": [
            {
                "kind": "lookAtTopCards",
                "zone": library(library_owner),
                "count": integer(ability.inspection.count),
                "bind": "lookedTopCards",
            },
            {
                "kind": "installDelayedStepTrigger",
                "step": delayed_step,
                "controller": controller(),
                "effects": [delayed_child],
            },
        ],
    });
    if !decisions.is_empty() {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": decisions,
        });
    }

    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        with_activation_object_cost_decisions(rule, &ability.costs),
        &[
            "Compose independent activation-cost children",
            "Inspect the top of the controller or selected player's library",
            "Wrap one primitive child effect in a typed next-turn step schedule",
            &trace,
        ],
    ))
}
