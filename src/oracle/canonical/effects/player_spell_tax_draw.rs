use super::super::*;
use crate::oracle::characteristic_vocabulary::lookup_player_role;
use crate::oracle::syntax::AbilityInput;

/// Claims the event and effect envelope without matching the whole sentence.
/// Once the typed `whenever PLAYER casts a spell, you may draw` prefix is
/// present, malformed `unless`/anaphor/payment suffixes stay committed here.
pub(in crate::oracle::canonical) fn claims_promoted_player_spell_tax_draw_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() >= 10
        && tokens[0].raw.eq_ignore_ascii_case("whenever")
        && tokens[0].token_type == "triggerWord"
        && matches!(tokens[1].raw.to_ascii_lowercase().as_str(), "a" | "an")
        && lookup_player_role(&tokens[2].raw).is_some()
        && tokens[3].raw.eq_ignore_ascii_case("casts")
        && tokens[3].token_type == "actionWord"
        && tokens[4].raw.eq_ignore_ascii_case("a")
        && tokens[5].raw.eq_ignore_ascii_case("spell")
        && tokens[6].raw == ","
        && tokens[7].raw.eq_ignore_ascii_case("you")
        && tokens[8].raw.eq_ignore_ascii_case("may")
        && tokens[9].raw.eq_ignore_ascii_case("draw")
}

pub(in crate::oracle::canonical) fn parse_promoted_player_spell_tax_draw(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("player-spell-tax-draw", input.tokens);
    let ability = recognize_player_spell_tax_draw_triggered_ability(&mut cursor)?;

    cursor.enter("trigger.player-spell-tax-draw.event-shape");
    let event = match ability.event.caster {
        PlayerPredicate::Opponent => json!({
            "kind": "spellCast",
            "opponentOfSourceController": true,
            "where": Value::Null,
        }),
        PlayerPredicate::Any => json!({
            "kind": "spellCast",
            "anyPlayer": true,
            "where": Value::Null,
        }),
    };
    cursor.enter("trigger.player-spell-tax-draw.draw-shape");
    cursor.require(
        ability.effect.draw.player == LocalPlayerReference::Controller,
        "the source controller as the optional drawing player",
    )?;
    let draw_count = lower_numeric_expression(&ability.effect.draw.count)
        .ok_or_else(|| cursor.fail("a target-independent draw count"))?;

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
                    "manaCost": ability.effect.cost.as_oracle_cost(),
                },
                "effects": [],
                "declineEffects": [{
                    "kind": "optionalEffects",
                    "player": controller(),
                    "effects": [{
                        "kind": "drawCards",
                        "player": controller(),
                        "count": draw_count,
                    }],
                }],
            }],
        }),
        &[
            "Recognize a player-casts-spell event and retain its triggering-player binding",
            "Resolve `that player` against the event binding before recognizing payment",
            "Compose optional payment, optional draw, and drawCards with existing control primitives",
            &trace,
        ],
    ))
}
