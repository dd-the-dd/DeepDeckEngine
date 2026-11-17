use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the characteristic-definition family by reusable lexical anchors.
/// The committed automaton still validates every token and the current-source
/// name link; these anchors do not encode a card name or a whole sentence.
pub(in crate::oracle::canonical) fn claims_promoted_graveyard_characteristic_family(
    input: &AbilityInput<'_>,
) -> bool {
    source_reference_starts_at(
        0,
        input.tokens.first().map(|token| token.raw.as_str()),
        input.name_links,
        input.name_resolution_failure,
    ) && input.tokens.windows(2).any(|window| {
        window[0].raw.eq_ignore_ascii_case("card") && window[1].raw.eq_ignore_ascii_case("types")
    }) && input
        .tokens
        .iter()
        .any(|token| token.raw.eq_ignore_ascii_case("power"))
        && input
            .tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("toughness"))
        && input.tokens.iter().any(|token| {
            token.raw.eq_ignore_ascii_case("graveyard")
                || token.raw.eq_ignore_ascii_case("graveyards")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_graveyard_characteristic(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("graveyard-card-type-characteristic", input.tokens);
    let characteristic = recognize_graveyard_card_type_characteristic(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("graveyard-card-type-characteristic.validation");
    cursor.require(
        characteristic.object == LocalObjectReference::SourceSelf,
        "the current source as the characteristic object",
    )?;
    let power = lower_numeric_expression(&characteristic.power)
        .ok_or_else(|| cursor.fail("a target-independent power expression"))?;
    let toughness = lower_numeric_expression(&characteristic.toughness)
        .ok_or_else(|| cursor.fail("a target-independent toughness expression"))?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [{
                "kind": "setBasePowerToughness",
                "objects": self_ref(),
                "power": power,
                "toughness": toughness,
            }],
        }),
        &[
            "Resolve the characteristic source through the shared name-link automaton",
            "Count distinct card types in a typed graveyard scope",
            "Resolve `that number` against transient numeric memory",
            "Compose the reused count and literal through numeric addition",
            &trace,
        ],
    ))
}
