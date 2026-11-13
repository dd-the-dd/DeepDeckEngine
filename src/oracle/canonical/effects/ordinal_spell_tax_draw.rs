use super::super::*;
use crate::oracle::characteristic_vocabulary::lookup_player_role;
use crate::oracle::syntax::AbilityInput;

/// Claims the ordinal filtered spell-cast envelope from typed tokens. Once the
/// event and mandatory draw prefix are present, malformed payment or variable
/// definition suffixes remain committed to this promoted family.
pub(in crate::oracle::canonical) fn claims_promoted_ordinal_spell_tax_draw_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    if tokens.len() < 12
        || tokens[0].token_type != "triggerWord"
        || !tokens[0].raw.eq_ignore_ascii_case("whenever")
        || !matches!(tokens[1].raw.to_ascii_lowercase().as_str(), "a" | "an")
        || lookup_player_role(&tokens[2].raw)
            != Some(crate::oracle::characteristic_vocabulary::PlayerRole::Opponent)
        || tokens[3].token_type != "actionWord"
        || !tokens[3].raw.eq_ignore_ascii_case("casts")
        || !tokens[4].raw.eq_ignore_ascii_case("their")
        || tokens[5].token_type != "ordinalWord"
    {
        return false;
    }

    // The predicate child owns every token between the ordinal and the spell
    // domain. The parent only claims the surrounding event/draw envelope.
    (7..tokens.len().saturating_sub(4)).any(|spell_index| {
        tokens[spell_index].raw.eq_ignore_ascii_case("spell")
            && tokens[spell_index + 1].raw.eq_ignore_ascii_case("each")
            && tokens[spell_index + 3].raw == ","
            && tokens[spell_index + 4].raw.eq_ignore_ascii_case("draw")
    })
}

pub(in crate::oracle::canonical) fn parse_promoted_ordinal_spell_tax_draw(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("ordinal-spell-tax-draw", input.tokens);
    let ability = recognize_ordinal_spell_tax_draw_triggered_ability(&mut cursor)?;

    cursor.enter("trigger.ordinal-spell-tax-draw.event-shape");
    let where_filter = lower_spell_object_predicate(&ability.event.filter)
        .ok_or_else(|| cursor.fail("a nontrivial spell predicate"))?;
    let event = match ability.event.caster {
        PlayerPredicate::Opponent => json!({
            "kind": "spellCast",
            "opponentOfSourceController": true,
            "where": where_filter,
            "spellCastOrdinal": integer(ability.event.ordinal),
        }),
        PlayerPredicate::Any => json!({
            "kind": "spellCast",
            "anyPlayer": true,
            "where": where_filter,
            "spellCastOrdinal": integer(ability.event.ordinal),
        }),
    };

    cursor.enter("trigger.ordinal-spell-tax-draw.draw-shape");
    cursor.require(
        ability.draw.player == LocalPlayerReference::Controller,
        "the source controller as the drawing player",
    )?;
    let draw_count = lower_numeric_expression(&ability.draw.count)
        .ok_or_else(|| cursor.fail("a target-independent draw count"))?;
    let payment_amount = lower_numeric_expression(&ability.payment.amount)
        .ok_or_else(|| cursor.fail("a source-property mana amount"))?;

    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": event,
            "effects": [{
                "kind": "optionalPayCostPerformEffects",
                "player": { "kind": "triggeringPlayer" },
                "cost": {
                    "kind": "payMana",
                    "genericAmount": payment_amount,
                },
                "effects": [],
                "declineEffects": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": draw_count,
                }],
            }],
        }),
        &[
            "Recognize an ordinal filtered spell-cast event scoped per triggering player",
            "Resolve `that player` against the event binding before recognizing payment",
            "Resolve the variable generic-mana amount from a typed source property",
            "Compose optional payment and mandatory decline-path draw primitives",
            &trace,
        ],
    ))
}
