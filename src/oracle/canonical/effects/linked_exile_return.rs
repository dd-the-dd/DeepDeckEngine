use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the structural family from token boundaries. Malformed members stay
/// committed once a supported cost head and linked-return action head exist,
/// allowing the responsible local state to surface a diagnostic.
pub(in crate::oracle::canonical) fn claims_promoted_linked_exile_return_activation_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let supported_cost = tokens.first().is_some_and(|token| {
        token.token_type == "manaSymbol"
            || token.token_type == "tapSymbol"
            || token.raw.eq_ignore_ascii_case("sacrifice")
            || token.raw.eq_ignore_ascii_case("pay")
    });
    let linked_return_head = tokens.windows(2).any(|window| {
        window[0].raw.eq_ignore_ascii_case("put") && window[1].raw.eq_ignore_ascii_case("each")
    });
    let linked_origin = tokens.windows(3).any(|window| {
        window[0].raw.eq_ignore_ascii_case("exiled")
            && window[1].raw.eq_ignore_ascii_case("with")
            && window[2].raw.eq_ignore_ascii_case("this")
    });
    supported_cost && linked_return_head && linked_origin
}

pub(in crate::oracle::canonical) fn parse_promoted_linked_exile_return_activation(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("linked-exile-return-activation", input.tokens);
    let ability = recognize_linked_exile_return_activation(&mut cursor)?;

    cursor.enter("activation.linked-return.validation");
    cursor.require(
        !ability.costs.costs.is_empty(),
        "at least one independent activation cost",
    )?;
    cursor.require(
        ability.characteristics.objects == LinkedSetBinding::MovedCards,
        "the moved linked set as characteristic subject",
    )?;
    cursor.require(
        ability.characteristics.retain_existing_types,
        "retention of the moved cards' existing types",
    )?;
    let ActivationTimingNode::Sorcery = ability.timing;

    let costs = ability
        .costs
        .costs
        .iter()
        .map(lower_local_activation_cost)
        .collect::<Vec<_>>();
    let effect = json!({
        "kind": "moveCards",
        "cards": {
            "kind": "filterObjects",
            "objects": { "kind": "cardsExiledWithSource" },
            "where": lower_card_object_predicate(&ability.movement.cards.predicate),
        },
        "to": {
            "kind": "battlefield",
            "player": controller(),
            "tapped": false,
            "enterWithCounters": [{
                "counter": ability.movement.entering_counter.kind,
                "count": integer(ability.movement.entering_counter.count),
            }],
            "addTypes": [ObjectCardType::Creature.ir_name()],
            "addSubtypes": [ability.characteristics.subtype.ir_name()],
            "basePower": integer(ability.characteristics.power_toughness.power),
            "baseToughness": integer(ability.characteristics.power_toughness.toughness),
            "retainExistingTypes": true,
        },
    });
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        with_activation_object_cost_decisions(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "activationCondition": { "kind": "sorceryTiming" },
                "effects": [effect],
            }),
            &ability.costs,
        ),
        &[
            "Compose independent activation-cost, linked-set, zone-move, characteristic, and timing children",
            "Filter the source-linked exile set with the shared card predicate",
            "Lower the result to one primitive moveCards operation with structured entering modifications",
            &trace,
        ],
    ))
}
