use super::super::*;

#[derive(Clone, Debug, PartialEq)]
pub(in crate::oracle::canonical) struct LoweredTemporaryKeywordGrant {
    pub decision: Option<Value>,
    pub effects: Vec<Value>,
}

fn keyword_grant_target_supported(selector: &TargetSelector) -> bool {
    match selector {
        TargetSelector::Object(_) | TargetSelector::ControlledObject { .. } => true,
        TargetSelector::Union(branches) => {
            !branches.is_empty() && branches.iter().all(keyword_grant_target_supported)
        }
        TargetSelector::Any | TargetSelector::Player(_) | TargetSelector::StackItem(_) => false,
    }
}

fn lower_keyword_grant_duration(duration: EffectDurationNode) -> Option<Value> {
    Some(match duration {
        EffectDurationNode::UntilEndOfCurrentTurn => {
            json!({ "kind": "untilEndOfCurrentTurn" })
        }
        EffectDurationNode::UntilNextTurn { player } => json!({
            "kind": "untilNextTurn",
            "player": match player {
                LocalPlayerReference::Controller => controller(),
                LocalPlayerReference::ControllerOfQualified(_) => return None,
                LocalPlayerReference::ControllerOf(_) => {
                    unreachable!("keyword grant duration cannot require a target binding")
                }
            },
        }),
        EffectDurationNode::UntilEndOfNextTurn { .. }
        | EffectDurationNode::WhileSourceControlled => return None,
    })
}

/// Lowers both targeted and controller-scoped grants through one primitive
/// path. A modal parent may attach its selection condition to the optional
/// target decision and wrap the independent grantKeyword effects.
pub(in crate::oracle::canonical) fn lower_temporary_keyword_grant(
    grant: TemporaryKeywordGrantNode,
    target_id: &str,
) -> Option<LoweredTemporaryKeywordGrant> {
    let (decision, object) = match grant.objects {
        TemporaryKeywordGrantObjectsNode::Targets(targets) => {
            if targets.slots.len() != 1
                || targets.slots[0].cardinality.maximum != 1
                || !targets
                    .slots
                    .first()
                    .is_some_and(|slot| keyword_grant_target_supported(&slot.selector))
            {
                return None;
            }
            let slot = &targets.slots[0];
            let (_, candidates) = lower_target_slot_candidates(slot)?;
            (
                Some(target_decision(
                    target_id,
                    candidates,
                    i64::from(slot.cardinality.minimum),
                    i64::from(slot.cardinality.maximum),
                )),
                chosen_target(target_id),
            )
        }
        TemporaryKeywordGrantObjectsNode::ControlledSet(objects) => {
            if objects.controller != ObjectControllerScope::AbilityController {
                return None;
            }
            (
                None,
                json!({
                    "kind": "eachPermanent",
                    "player": controller(),
                    "where": lower_permanent_object_predicate(&objects.predicate),
                }),
            )
        }
    };
    let effects =
        lower_keyword_grant_to_object(&grant.keywords, &grant.abilities, &grant.duration, &object)?;
    Some(LoweredTemporaryKeywordGrant { decision, effects })
}

/// A quoted ability is granted as one complete rule of the affected object
/// (CR 113.1). The engine appends it to the object's rules for the duration,
/// so only a temporary grant of a triggered, activated or mana rule is
/// representable; a static rule is accepted only on a single object.
fn quoted_ability_grantable(rule: &Value, object: &Value, duration: &Value) -> bool {
    let single = matches!(
        object["kind"].as_str(),
        Some("chosenTarget" | "self" | "abilitySource")
    );
    duration["kind"].as_str() == Some("untilEndOfCurrentTurn")
        && (single || object["kind"].as_str() == Some("eachPermanent"))
        && match rule["kind"].as_str() {
            Some("triggeredAbility" | "activatedAbility" | "manaAbility") => true,
            Some("staticAbility") => single,
            _ => false,
        }
}

pub(in crate::oracle::canonical) fn lower_keyword_grant_to_object(
    keywords: &[GrantedKeyword],
    abilities: &[QuotedAbilityNode],
    duration: &EffectDurationNode,
    object: &Value,
) -> Option<Vec<Value>> {
    let duration = lower_keyword_grant_duration(duration.clone())?;
    let mut effects = keywords
        .iter()
        .map(|keyword| {
            json!({
                "kind":"grantKeyword", "object":object,
                "keyword":keyword.canonical_name(), "duration":duration,
            })
        })
        .collect::<Vec<_>>();
    for ability in abilities {
        if !quoted_ability_grantable(&ability.rule, object, &duration) {
            return None;
        }
        effects.push(json!({
            "kind":"grantAbility", "object":object,
            "ability":ability.rule, "duration":duration,
        }));
    }
    Some(effects)
}
