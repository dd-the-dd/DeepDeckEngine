//! Lowers activation and cast restrictions (CR 601.3, 602.5) into ability
//! metadata: one condition evaluated when the player would activate or cast,
//! one count limit, the activating players and the labeling keyword's class.
//! Nothing here is an effect; the effect program is never touched.
use super::super::*;

/// The legality condition of a restriction node, if it has one. `your`
/// names the would-be controller (CR 109.5), which the engine evaluates
/// with the acting player.
pub(in crate::oracle::canonical) fn lower_restriction_condition(
    restrictions: &ActivationRestrictionsNode,
) -> Option<Value> {
    let mut operands = Vec::new();
    let sorcery = restrictions.timing == Some(ActivationTiming::Sorcery);
    if sorcery {
        operands.push(json!({ "kind": "sorceryTiming" }));
    }
    if let Some(window) = restrictions.window {
        match window.owner {
            // Sorcery timing already implies the acting player's turn (CR 307.1).
            Some(WindowTurnOwner::AbilityPlayer) if sorcery => {}
            Some(WindowTurnOwner::AbilityPlayer) => {
                operands.push(json!({ "kind": "duringControllerTurn", "player": controller() }));
            }
            Some(WindowTurnOwner::Opponent) => operands.push(json!({ "kind": "notYourTurn" })),
            None => {}
        }
        if !window.steps.is_every() {
            operands.push(json!({
                "kind": "duringSteps",
                "steps": window.steps.steps().map(WindowStep::engine_name).collect::<Vec<_>>(),
            }));
        }
    }
    if let Some(condition) = &restrictions.condition {
        operands.push(match condition {
            RestrictionCondition::Shared(condition) => lower_condition(condition),
            RestrictionCondition::SourceAttackedThisTurn => json!({
                "kind": "objectMatchesFilter",
                "object": self_ref(),
                "where": { "kind": "attackedThisTurn" },
            }),
            RestrictionCondition::AbilityPlayerAttacked => {
                json!({ "kind": "controllerIsBeingAttacked" })
            }
        });
    }
    match operands.len() {
        0 => None,
        1 => operands.pop(),
        _ => Some(and(operands)),
    }
}

/// Restrictions belong to the activated ability itself, so they are applied
/// to the lowered rule rather than to any effect (CR 602.5b, 602.5d).
pub(in crate::oracle::canonical) fn lower_activation_restrictions(
    cursor: &mut OracleTokenCursor<'_>,
    rule: &mut Value,
    restrictions: &ActivationRestrictionsNode,
) -> MachineResult<()> {
    let activated = rule["kind"] == "activatedAbility";
    cursor.enter("activation.restriction.lowering");
    if let Some(condition) = lower_restriction_condition(restrictions) {
        rule["activationCondition"] = match rule.get("activationCondition").cloned() {
            Some(existing) if !existing.is_null() => and(vec![existing, condition]),
            _ => condition,
        };
    }
    let id = restrictions
        .keyword
        .map_or("activationRestriction", ActivationKeyword::ability_class);
    if let Some(limit) = restrictions.limit {
        rule["activationLimit"] = match limit {
            ActivationCountLimit::PerTurn(1) => json!({ "kind": "oncePerTurn", "id": id }),
            ActivationCountLimit::PerTurn(maximum) => {
                json!({ "kind": "timesPerTurn", "maximum": maximum, "id": id })
            }
            ActivationCountLimit::PerObject => {
                json!({ "kind": "oncePerGameObjectAbility", "id": id })
            }
        };
    }
    if restrictions.any_player {
        // The mana-ability executors only offer the controller's sources.
        cursor.require(
            activated,
            "a non-mana activated ability for an any-player activation",
        )?;
        rule["anyPlayerMayActivate"] = json!(true);
    }
    if let Some(keyword) = restrictions.keyword {
        cursor.require(
            activated,
            "a non-mana activated ability for its labeling keyword",
        )?;
        rule["abilityClass"] = json!(keyword.ability_class());
        match keyword {
            ActivationKeyword::PowerUp => {
                // CR 702.193a-b: reduced by this permanent's mana cost if it
                // entered this turn.
                let mut modifiers = rule
                    .get("activationCostModifiers")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                modifiers.push(json!({
                    "kind": "reduceManaCost",
                    "amount": { "kind": "manaCostOf", "object": self_ref() },
                    "condition": { "kind": "sourceFlagSet", "flag": "enteredThisTurn" },
                }));
                rule["activationCostModifiers"] = json!(modifiers);
            }
            ActivationKeyword::Forecast => {
                // CR 702.57a: activated only from its owner's hand.
                cursor.require(
                    rule.get("activationZone")
                        .is_none_or(|zone| zone.is_null() || zone.as_str() == Some("hand")),
                    "a forecast ability whose costs do not require another zone",
                )?;
                rule["activationZone"] = json!("hand");
            }
            ActivationKeyword::Boast | ActivationKeyword::Exhaust => {}
        }
    }
    Ok(())
}

/// Claims a whole ability that is a spell's cast restriction. A rules-free
/// label (`Tragic Backstory —`) was already removed by the ability boundary.
pub(in crate::oracle::canonical) fn claims_spell_cast_restriction_family(
    input: &crate::oracle::syntax::AbilityInput<'_>,
) -> bool {
    spell_cast_restriction_starts(&OracleTokenCursor::new(
        "spell-cast-restriction",
        input.tokens,
    ))
}

/// `Cast this spell only CLAUSES.` lowers to a static ability of the card
/// that functions while it is being cast (CR 601.3, 604.1): the engine offers
/// the cast only when its condition holds for the casting player.
pub(in crate::oracle::canonical) fn parse_spell_cast_restriction(
    input: &crate::oracle::syntax::AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("spell-cast-restriction", input.tokens);
    let restrictions = recognize_spell_cast_restriction(&mut cursor)?;
    cursor.enter("cast-restriction.lowering");
    let condition = cursor.require_value(
        lower_restriction_condition(&restrictions),
        "a cast restriction with a legality condition",
    )?;
    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": {
                "kind": "inZone",
                "object": self_ref(),
                "zone": { "kind": "stackOrCast" },
            },
            "modifiers": [{
                "kind": "castOnlyIf",
                "object": self_ref(),
                "condition": condition,
            }],
        }),
        &[
            "Read `Cast this spell only` with the shared restriction clause grammar",
            "Read turn windows with the shared turn-window grammar",
            "Lower one cast-legality condition of the spell itself (CR 601.3)",
        ],
    ))
}
