use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) const OPTIONAL_CARD_DEPLOYMENT_DECISION_ID: &str =
    "optionalCardDeployment";

/// Claims a draw followed by the structural optional-deployment prefix. Child
/// automata remain responsible for the count, card filter, zones, tapped state,
/// punctuation, and full consumption so claimed near misses fail visibly.
pub(in crate::oracle::canonical) fn claims_promoted_draw_optional_card_deployment_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("draw"))
        && tokens.windows(4).any(|window| {
            window[0].raw == "."
                && window[1].raw.eq_ignore_ascii_case("you")
                && window[2].raw.eq_ignore_ascii_case("may")
                && window[3].raw.eq_ignore_ascii_case("put")
        })
        && tokens.last().is_some_and(|token| token.raw == ".")
}

fn controller_card_zone(zone: CardDeploymentSourceZone, player: Value) -> Value {
    let kind = match zone {
        CardDeploymentSourceZone::Hand => "hand",
        CardDeploymentSourceZone::Graveyard => "graveyard",
    };
    json!({ "kind": kind, "player": player })
}

pub(in crate::oracle::canonical) fn parse_promoted_draw_optional_card_deployment(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("draw-optional-card-deployment", input.tokens);
    let sequence =
        recognize_effect_sequence(&mut cursor, input.name_links, input.name_resolution_failure)?;
    cursor.enter("sequence.draw-optional-card-deployment.validation");
    cursor.require(
        sequence.connectors == [EffectConnector::Then],
        "one ordered sentence-boundary connector",
    )?;
    let [
        LocalEffectNode::DrawCards(draw),
        LocalEffectNode::OptionalCardDeployment(deployment),
    ] = sequence.effects.as_slice()
    else {
        return Err(cursor.fail("a draw effect followed by an optional card deployment"));
    };
    cursor.require(
        draw.player == LocalPlayerReference::Controller
            && deployment.player == LocalPlayerReference::Controller,
        "the source controller as the drawing and deploying player",
    )?;
    cursor.require(
        deployment.destination == CardDeploymentDestination::Battlefield,
        "the battlefield as the deployment destination",
    )?;

    let player = controller();
    let selected = decision_result(OPTIONAL_CARD_DEPLOYMENT_DECISION_ID);
    let source = controller_card_zone(deployment.source, player.clone());
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
                    "kind": "drawCards",
                    "player": player.clone(),
                    "count": lower_numeric_expression(&draw.count)
                        .expect("draw/deployment grammar has no target-property binding"),
                },
                {
                    "kind": "chooseCards",
                    "id": OPTIONAL_CARD_DEPLOYMENT_DECISION_ID,
                    "player": player.clone(),
                    "minimum": integer(0),
                    "maximum": integer(1),
                    "candidates": {
                        "kind": "cards",
                        "zone": source,
                        "where": lower_card_object_predicate(&deployment.filter),
                    },
                },
                {
                    "kind": "moveCards",
                    "cards": selected,
                    "to": {
                        "kind": "battlefield",
                        "player": player,
                        "tapped": deployment.tapped,
                    },
                },
            ],
        }),
        &[
            "Recognize draw and optional deployment from one immutable token stream",
            "Reuse the shared card-selector grammar for the deployment filter",
            "Keep the optional choice at resolution with zero-to-one chooseCards cardinality",
            "Lower movement independently through the generic moveCards primitive",
            &trace,
        ],
    ))
}
