//! Lowering of the characteristic-defining power/toughness frame (CR 604.3,
//! 613.4a). The ability is a static ability of its source whose modifiers
//! carry `"layer": "7a"`: the runtime evaluates them before every layer-7b
//! setting effect and in every zone, from the card's own rules, instead of
//! treating them as battlefield setting effects.
use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the frame by its own anchors: a source reference first, then a
/// stat word with its `is equal to` / `are each equal to` copula. The
/// committed automaton still validates every token and the name link.
pub(in crate::oracle::canonical) fn claims_characteristic_defining_power_toughness(
    input: &AbilityInput<'_>,
) -> bool {
    source_reference_starts_at(
        0,
        input.tokens.first().map(|token| token.raw.as_str()),
        input.name_links,
        input.name_resolution_failure,
    ) && characteristic_defining_power_toughness_starts(input.tokens)
}

/// The layer-7a modifiers of one characteristic-defining ability of its
/// own source. Also the lowering of the same frame inside a quoted token
/// ability (`This token's power and toughness are each equal to ...`).
pub(in crate::oracle::canonical) fn lower_characteristic_defining_modifiers(
    node: &PowerToughnessCharacteristicNode,
) -> Result<Value, &'static str> {
    if node.object != LocalObjectReference::SourceSelf {
        return Err("the current source as the characteristic object");
    }
    let lower = |expression: &NumericExpression| {
        lower_numeric_expression(expression).ok_or("a target-independent characteristic amount")
    };
    let modifier = match (&node.power, &node.toughness) {
        (Some(power), Some(toughness)) => json!({
            "kind": "setBasePowerToughness",
            "objects": self_ref(),
            "layer": "7a",
            "power": lower(power)?,
            "toughness": lower(toughness)?,
        }),
        (Some(power), None) => json!({
            "kind": "setPower",
            "objects": self_ref(),
            "layer": "7a",
            "power": lower(power)?,
        }),
        (None, Some(toughness)) => json!({
            "kind": "setToughness",
            "objects": self_ref(),
            "layer": "7a",
            "toughness": lower(toughness)?,
        }),
        (None, None) => return Err("a defined power or toughness"),
    };
    Ok(json!([modifier]))
}

/// The complete static rule of a characteristic-defining ability.
pub(in crate::oracle::canonical) fn lower_characteristic_defining_rule(
    node: &PowerToughnessCharacteristicNode,
) -> Result<Value, &'static str> {
    Ok(json!({
        "kind": "staticAbility",
        "source": self_ref(),
        "activeWhile": active_while_battlefield(),
        "characteristicDefining": true,
        "modifiers": lower_characteristic_defining_modifiers(node)?,
    }))
}

pub(in crate::oracle::canonical) fn parse_characteristic_defining_power_toughness(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor =
        OracleTokenCursor::new("characteristic-defining-power-toughness", input.tokens);
    let characteristic = recognize_characteristic_defining_power_toughness(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("characteristic-defining-power-toughness.lowering");
    let rule = lower_characteristic_defining_rule(&characteristic)
        .map_err(|expected| cursor.fail(expected))?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        rule,
        &[
            "Resolve the characteristic source through the shared name-link automaton",
            "Read the defined stats and their copula",
            "Read each amount with the shared `equal to` numeric grammar",
            "Resolve `that number` against transient numeric memory",
            &trace,
        ],
    ))
}
