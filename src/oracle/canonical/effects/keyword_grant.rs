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
                LocalPlayerReference::ControllerOfQualified(_)
            | LocalPlayerReference::Bound(_) => return None,
                LocalPlayerReference::ControllerOf(_) => {
                    unreachable!("keyword grant duration cannot require a target binding")
                }
            },
        }),
        // The engine's permanent grant belongs to the object it was granted
        // to; a new object does not keep it (CR 400.7).
        EffectDurationNode::Indefinite => json!({ "kind": "permanent" }),
        EffectDurationNode::UntilEndOfNextTurn { .. }
        | EffectDurationNode::WhileSourceControlled
        | EffectDurationNode::WhileSourceTapped
        | EffectDurationNode::WhileSourceOnBattlefield => return None,
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
        // A declared permanent target is on the battlefield.
        lower_keyword_grant_to_object(
            &grant.keywords,
            &grant.abilities,
            &grant.duration,
            &object,
            true,
        )?;
    Some(LoweredTemporaryKeywordGrant { decision, effects })
}

/// A quoted ability or a parameterized keyword is granted as one complete
/// rule of the affected object (CR 113.1). The engine appends it to the
/// object's rules for the duration, so only a temporary grant of a
/// triggered, activated, mana or keyword rule is
/// representable; a static rule is accepted only on a single object. An
/// indefinite grant (`This Saga gains "..."`, CR 611.2a) is recorded on one
/// object and lost when it changes zones (CR 400.7).
fn quoted_ability_grantable(rule: &Value, object: &Value, duration: &Value) -> bool {
    let single = matches!(
        object["kind"].as_str(),
        Some("chosenTarget" | "self" | "abilitySource")
    );
    let temporary = duration["kind"].as_str() == Some("untilEndOfCurrentTurn");
    let indefinite_single = duration["kind"].as_str() == Some("permanent") && single;
    (temporary || indefinite_single)
        && (single || object["kind"].as_str() == Some("eachPermanent"))
        && match rule["kind"].as_str() {
            // A granted parameterized keyword (Ward, protection, toxic N) is
            // one complete keyword rule of the affected object (CR 702.1).
            Some(
                "triggeredAbility"
                | "activatedAbility"
                | "manaAbility"
                | "keywordAbility"
                | "keywordAbilityGroup",
            ) => true,
            Some("staticAbility") => single,
            _ => false,
        }
}

pub(in crate::oracle::canonical) fn lower_keyword_grant_to_object(
    keywords: &[GrantedKeyword],
    abilities: &[QuotedAbilityNode],
    duration: &EffectDurationNode,
    object: &Value,
    on_battlefield: bool,
) -> Option<Vec<Value>> {
    // An indefinite grant writes to a battlefield permanent (CR 611.2a,
    // 400.7). Only object expressions that denote battlefield permanents
    // take it; a card in exile, a graveyard or on the stack (`it gains
    // suspend`) stays explicitly unsupported instead of writing nowhere. A
    // spell's own source is refused by the spell program, which knows it is
    // one.
    if *duration == EffectDurationNode::Indefinite && !on_battlefield {
        return None;
    }
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
        if ability.rule["kind"] == KEYWORD_CHOICE_RULE {
            effects.extend(lower_keyword_choice_grant(
                &ability.rule,
                object,
                &duration,
            )?);
            continue;
        }
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

/// `gains your choice of K, K, or K` (CR 608.2d): the controller chooses
/// one option as the grant resolves, then the object gains the chosen
/// keyword. The choice is the generic `chooseOption` decision; the grant is
/// the ordinary `grantKeyword` whose keyword reads that decision among the
/// same options. The decision is named after the granted object so two
/// grants of one program never share it.
fn lower_keyword_choice_grant(
    rule: &Value,
    object: &Value,
    duration: &Value,
) -> Option<Vec<Value>> {
    let options = rule["options"].as_array()?;
    if options.len() < 2 || !options.iter().all(Value::is_string) {
        return None;
    }
    let owner = object["id"]
        .as_str()
        .or_else(|| object["binding"].as_str())
        .or_else(|| object["kind"].as_str())?;
    let id = format!("{owner}:keywordChoice");
    Some(vec![
        json!({
            "kind": "chooseOption",
            "id": id,
            "player": controller(),
            "options": options,
        }),
        json!({
            "kind": "grantKeyword",
            "object": object,
            "keyword": { "kind": "chosenOption", "decisionId": id, "options": options },
            "duration": duration,
        }),
    ])
}
