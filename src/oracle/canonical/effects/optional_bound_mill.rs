use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims a combat-damage trigger whose instruction begins with the optional
/// event-sized mill shape. The promoted machine owns the entire ability and
/// therefore exposes malformed anaphora and trailing tokens as failures.
pub(in crate::oracle::canonical) fn claims_promoted_optional_bound_mill_family(
    input: &AbilityInput<'_>,
) -> bool {
    input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("whenever"))
        && source_reference_starts_at(
            1,
            input.tokens.get(1).map(|token| token.raw.as_str()),
            input.name_links,
            input.name_resolution_failure,
        )
        && input.tokens.windows(3).any(|window| {
            window[0].raw.eq_ignore_ascii_case("deals")
                && window[1].raw.eq_ignore_ascii_case("combat")
                && window[2].raw.eq_ignore_ascii_case("damage")
        })
        && input.tokens.windows(4).any(|window| {
            window[0].raw.eq_ignore_ascii_case("you")
                && window[1].raw.eq_ignore_ascii_case("may")
                && window[2].raw.eq_ignore_ascii_case("mill")
                && window[3].raw.eq_ignore_ascii_case("that")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_optional_bound_mill(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("combat-damage-optional-bound-mill", input.tokens);
    let ability = recognize_combat_damage_optional_mill_ability(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("combat-damage-optional-bound-mill.validation");
    cursor.require(
        ability.source == LocalObjectReference::SourceSelf,
        "the current source as combat-damage dealer",
    )?;
    cursor.require(
        ability.recovery.count == NumericExpression::EventValue(ability.event_amount),
        "the triggering combat-damage amount as mill count",
    )?;
    cursor.require(
        ability.recovery.binding == CollectionBinding(0),
        "the collection produced by the immediately preceding mill",
    )?;
    let count = lower_numeric_expression(&ability.recovery.count)
        .ok_or_else(|| cursor.fail("an event-bound mill count"))?;
    let filter = lower_card_object_predicate(&ability.recovery.filter);
    let player = controller();
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
            "event": {
                "kind": "combatDamageToPlayer",
                "source": self_ref(),
            },
            "effects": [{
                "kind": "optionalAction",
                "player": player.clone(),
                "action": {
                    "kind": "mill",
                    "player": player.clone(),
                    "count": count,
                    "bind": "milledCards",
                },
                "onPerformed": [
                    {
                        "kind": "chooseCards",
                        "id": "milledCardForHand",
                        "player": player.clone(),
                        "from": {
                            "kind": "boundObjects",
                            "binding": "milledCards",
                        },
                        "where": filter,
                        "minimum": integer(0),
                        "maximum": integer(1),
                    },
                    {
                        "kind": "moveCards",
                        "cards": decision_result("milledCardForHand"),
                        "to": hand(player),
                    },
                ],
            }],
        }),
        &[
            "Recognize combat damage to a player and bind its amount",
            "Resolve `that many` through the event numeric binding",
            "Bind the cards produced by an optional mill action",
            "Resolve the reflexive `If you do` branch",
            "Filter a choice from the bound collection through the shared object selector",
            "Lower choice and movement as independent primitive effects",
            &trace,
        ],
    ))
}
