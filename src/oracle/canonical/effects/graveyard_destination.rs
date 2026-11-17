use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_entry_zone_batch_family(
    input: &AbilityInput<'_>,
) -> bool {
    input.tokens.first().is_some_and(|token| {
        token.token_type == "triggerWord" && token.raw.eq_ignore_ascii_case("when")
    }) && input
        .tokens
        .iter()
        .any(|token| token.token_type == "eventWord" && token.raw.eq_ignore_ascii_case("enters"))
        && input.tokens.windows(3).any(|window| {
            window[0].token_type == "actionWord"
                && window[0].raw.eq_ignore_ascii_case("exile")
                && window[1].token_type == "quantifierWord"
                && window[1].raw.eq_ignore_ascii_case("all")
                && window[2].token_type == "zoneWord"
                && window[2].raw.eq_ignore_ascii_case("graveyards")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_entry_zone_batch(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("entry-zone-batch", input.tokens);
    let _node = recognize_source_entry_exile_all_graveyards(&mut cursor)?;
    cursor.enter("entry-zone-batch.generic-movement-unavailable");
    Err(cursor.fail("a proved generic zone collection and moveCards implementation"))
}

pub(in crate::oracle::canonical) fn claims_promoted_graveyard_destination_replacement_family(
    input: &AbilityInput<'_>,
) -> bool {
    input.tokens.first().is_some_and(|token| {
        token.token_type == "logicWord" && token.raw.eq_ignore_ascii_case("if")
    }) && input
        .tokens
        .iter()
        .any(|token| token.token_type == "replacementModalWord")
        && input.tokens.iter().any(|token| {
            token.token_type == "zoneWord" && token.raw.eq_ignore_ascii_case("graveyard")
        })
        && input.tokens.iter().any(|token| {
            token.token_type == "logicWord" && token.raw.eq_ignore_ascii_case("instead")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_graveyard_destination_replacement(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("graveyard-destination-replacement", input.tokens);
    let node = recognize_graveyard_destination_replacement(&mut cursor)?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    let where_filter = match node.object_kind {
        GraveyardObjectKindNode::Card => not(json!({ "kind": "isToken" })),
        GraveyardObjectKindNode::Token => json!({ "kind": "isToken" }),
        GraveyardObjectKindNode::CardOrToken => Value::Null,
    };
    let graveyard_owners = match node.graveyard_owners {
        GraveyardOwnerScopeNode::EachPlayer => json!({ "kind": "eachPlayer" }),
        GraveyardOwnerScopeNode::Controller => controller(),
        GraveyardOwnerScopeNode::Opponents => json!({
            "kind": "opponentsOf",
            "player": controller(),
        }),
    };
    let from = match node.from {
        ZoneOriginNode::AnyZone => json!({ "kind": "anyZone" }),
    };
    let to = match node.to {
        ZoneDestinationNode::Exile => json!({ "kind": "exile" }),
    };

    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [{
                "kind": "replaceGraveyardDestination",
                "graveyardOwners": graveyard_owners,
                "where": where_filter,
                "from": from,
                "to": to,
            }],
        }),
        &[
            "Recognize card/token applicability independently",
            "Resolve the graveyard owner scope",
            "Resolve origin and replacement destination as typed zones",
            "Install one graveyard-destination replacement modifier",
            &trace,
        ],
    ))
}
