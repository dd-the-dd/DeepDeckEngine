//! Lowering of temporary rules (CR 611.2a). An allowance keeps its existing
//! primitive; every other rule is the static modifier vocabulary of a player
//! predicate or game rule, installed as one temporary continuous effect
//! whose lifetime is explicit. No rule is approximated: a predicate without
//! a static modifier fails.
use super::super::*;

/// The engine lifetime of a temporary rule.
fn lower_temporary_rule_duration(
    duration: &TemporaryRuleDuration,
) -> Result<Value, TargetlessLoweringError> {
    match duration {
        TemporaryRuleDuration::Effect(EffectDurationNode::UntilEndOfCurrentTurn) => {
            Ok(json!({"kind": "untilEndOfCurrentTurn"}))
        }
        TemporaryRuleDuration::Effect(EffectDurationNode::UntilNextTurn {
            player: LocalPlayerReference::Controller,
        }) => Ok(json!({"kind": "untilNextTurn", "player": controller()})),
        TemporaryRuleDuration::Effect(EffectDurationNode::UntilEndOfNextTurn {
            player: LocalPlayerReference::Controller,
        }) => Ok(json!({"kind": "untilEndOfNextTurn", "player": controller()})),
        TemporaryRuleDuration::RestOfGame => Ok(json!({"kind": "restOfGame"})),
        TemporaryRuleDuration::Effect(_) => {
            Err(TargetlessLoweringError::UnsupportedPermissionLifetime)
        }
    }
}

/// The static modifiers a temporary rule applies while it lasts.
fn lower_temporary_rule_modifiers(
    rule: &TemporaryRuleNode,
) -> Result<Vec<Value>, TargetlessLoweringError> {
    match rule {
        TemporaryRuleNode::Allowance(_) | TemporaryRuleNode::NextSpell { .. } => {
            Err(TargetlessLoweringError::UnsupportedEffect)
        }
        // CR 402.2: the ability controller has no maximum hand size.
        TemporaryRuleNode::NoMaximumHandSize => Ok(vec![
            json!({"kind": "noMaximumHandSize", "player": controller()}),
        ]),
        // CR 615.12: prevention effects don't apply to any damage.
        TemporaryRuleNode::DamageCantBePrevented => Ok(vec![json!({
            "kind": "eccStaticAbility",
            "ability": "damageCannotBePrevented",
        })]),
        // CR 702.8a, 601.3: the same timing grant a static permission
        // installs, held by the ability's controller while the rule lasts.
        TemporaryRuleNode::FlashCasting { spells } => {
            if !domain_filter_is_representable(spells) {
                return Err(TargetlessLoweringError::UnsupportedEffect);
            }
            Ok(vec![json!({
                "kind": "grantFlashCasting",
                "player": controller(),
                "where": lower_spell_object_predicate(spells).unwrap_or(Value::Null),
                "firstEachTurn": false,
            })])
        }
        // CR 704.5j: the legend rule ignores the waived permanents while the
        // effect lasts, including permanents that enter later (CR 611.2c).
        TemporaryRuleNode::LegendRuleWaiver(waiver) => {
            let cursor = OracleTokenCursor::new("temporary-rule.lowering", &[]);
            Ok(vec![
                lower_legend_rule_waiver(&cursor, waiver)
                    .map_err(|_| TargetlessLoweringError::UnsupportedEffect)?,
            ])
        }
        // CR 614.1c: each matching permanent enters tapped while it lasts.
        TemporaryRuleNode::EntersTapped { objects } => {
            let cursor = OracleTokenCursor::new("temporary-rule.lowering", &[]);
            let objects = lower_static_permanent_query(&cursor, objects)
                .map_err(|_| TargetlessLoweringError::UnsupportedEffect)?;
            // The entry replacement reads the filter and controller only.
            if objects.get("ownership").is_some() || objects.get("excludeSource").is_some() {
                return Err(TargetlessLoweringError::UnsupportedEffect);
            }
            Ok(vec![
                json!({"kind": "permanentsEnterTapped", "objects": objects}),
            ])
        }
        // CR 601.3: the allowance's actor holds it while the rule lasts.
        TemporaryRuleNode::Casting(node) => lower_casting_permission_modifiers(node)
            .map_err(|_| TargetlessLoweringError::UnsupportedEffect),
        TemporaryRuleNode::Players { subject, predicate } => {
            let players = match subject {
                StaticPlayerSubjectNode::You => controller(),
                StaticPlayerSubjectNode::Players(players) => lower_player_set_reference(*players),
                StaticPlayerSubjectNode::AttachedPermanentController => {
                    return Err(TargetlessLoweringError::UnsupportedEffect);
                }
            };
            // The static lowering reports its failures through a cursor; a
            // temporary rule only needs to know that the predicate has no
            // static modifier.
            let mut cursor = OracleTokenCursor::new("temporary-rule.lowering", &[]);
            lower_static_player_predicate(&mut cursor, &players, predicate)
                .map_err(|_| TargetlessLoweringError::UnsupportedEffect)
        }
    }
}

/// `the next SPELLS you cast this turn RULE` (CR 601.2i): a one-shot
/// modifier matched by the controller's next cast of a matching spell,
/// consumed by it, expiring with the turn.
fn lower_next_spell_rule(spells: &ObjectPredicate, modification: NextSpellModification) -> Value {
    let mut matcher = json!({"kind": "spellCast", "player": controller()});
    if let Some(filter) = lower_spell_object_predicate(spells) {
        matcher["where"] = filter;
    }
    let apply = match modification {
        NextSpellModification::CantBeCountered => json!({
            "kind": "modifyStackObject",
            "object": {"kind": "eventSpell"},
            "modifier": {"kind": "cantBeCountered"},
        }),
        NextSpellModification::CastAsThoughFlash => json!({"kind": "castAsThoughItHadFlash"}),
        // CR 601.2f: evaluated when the rule is made; X is the sentence's
        // binding (or fails the card's X validation without a producer).
        NextSpellModification::ReduceGenericCost(delta) => {
            let amount = match delta {
                ReductionDelta::Generic(amount) => json!({"kind": "integer", "value": amount}),
                ReductionDelta::Variable => {
                    json!({"kind": "decisionResult", "decisionId": "xValue"})
                }
            };
            json!({"kind": "reduceCastingCost", "amount": amount})
        }
    };
    json!({
        "kind": "installOneShotModifier",
        "match": matcher,
        "apply": apply,
        "consume": {"kind": "firstMatchingEvent"},
        "expires": {"kind": "endOfCurrentTurn"},
    })
}

pub(in crate::oracle::canonical) fn lower_temporary_player_permission(
    node: &TemporaryPlayerPermissionNode,
) -> Result<Value, TargetlessLoweringError> {
    if let TemporaryRuleNode::NextSpell {
        spells,
        modification,
    } = &node.rule
    {
        return Ok(lower_next_spell_rule(spells, *modification));
    }
    let TemporaryRuleNode::Allowance(modifier) = &node.rule else {
        return Ok(json!({
            "kind": "installTemporaryStaticModifiers",
            "modifiers": lower_temporary_rule_modifiers(&node.rule)?,
            "duration": lower_temporary_rule_duration(&node.duration)?,
        }));
    };
    if node.duration != TemporaryRuleDuration::Effect(EffectDurationNode::UntilEndOfCurrentTurn) {
        return Err(TargetlessLoweringError::UnsupportedPermissionLifetime);
    }
    match modifier {
        PlayerModifierNode::AdditionalLandPlay { player, count } => {
            if *player != LocalPlayerReference::Controller {
                return Err(TargetlessLoweringError::RequiresTarget);
            }
            let NumericExpression::Atom(NumericAtom::Integer(count)) = count else {
                // Runtime currently reads an i32 literal, not a numeric program.
                return Err(TargetlessLoweringError::UnsupportedPermissionAmount);
            };
            if *count <= 0 || i32::try_from(*count).is_err() {
                return Err(TargetlessLoweringError::UnsupportedPermissionAmount);
            }
            Ok(json!({
                "kind":"grantAdditionalLandPlays", "player":controller(),
                "count":integer(*count), "duration":{"kind":"untilEndOfCurrentTurn"},
            }))
        }
        PlayerModifierNode::NoMaximumHandSize { .. } => {
            Err(TargetlessLoweringError::UnsupportedEffect)
        }
    }
}

#[cfg(test)]
#[path = "player_permission_tests.rs"]
mod tests;
