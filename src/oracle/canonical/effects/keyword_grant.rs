use super::super::*;
use crate::oracle::syntax::AbilityInput;

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

fn lower_keyword_grant_duration(duration: EffectDurationNode) -> Value {
    match duration {
        EffectDurationNode::UntilEndOfCurrentTurn => {
            json!({ "kind": "untilEndOfCurrentTurn" })
        }
        EffectDurationNode::UntilNextTurn { player } => json!({
            "kind": "untilNextTurn",
            "player": match player {
                LocalPlayerReference::Controller => controller(),
                LocalPlayerReference::ControllerOf(_) => {
                    unreachable!("keyword grant duration cannot require a target binding")
                }
            },
        }),
    }
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
    let duration = lower_keyword_grant_duration(grant.duration);
    let effects = grant
        .keywords
        .into_iter()
        .map(|keyword| {
            json!({
                "kind": "grantKeyword",
                "object": object.clone(),
                "keyword": keyword.canonical_name(),
                "duration": duration.clone(),
            })
        })
        .collect();
    Some(LoweredTemporaryKeywordGrant { decision, effects })
}

fn starts_permanent_set(raw: &str, token_type: &str) -> bool {
    matches!(
        token_type,
        "typeWord" | "negativeTypeWord" | "colorWord" | "supertypeWord"
    ) || raw.chars().next().is_some_and(char::is_uppercase)
}

/// Claims the controlled-object-set keyword-grant envelope without deciding
/// its set predicate, keyword atoms, list punctuation, or duration. Those
/// concepts remain committed child-machine responsibilities.
pub(in crate::oracle::canonical) fn claims_promoted_controlled_keyword_grant_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens
        .first()
        .is_some_and(|token| starts_permanent_set(&token.raw, &token.token_type))
        && (tokens.windows(3).any(|window| {
            window[0].raw.eq_ignore_ascii_case("you")
                && window[1].raw.eq_ignore_ascii_case("control")
                && matches!(
                    window[2].raw.to_ascii_lowercase().as_str(),
                    "gain" | "gains"
                )
        }) || tokens.windows(4).any(|window| {
            window[0].raw.eq_ignore_ascii_case("you")
                && window[1].raw.eq_ignore_ascii_case("control")
                && window[2].raw.eq_ignore_ascii_case("also")
                && matches!(
                    window[3].raw.to_ascii_lowercase().as_str(),
                    "gain" | "gains"
                )
        }))
}

pub(in crate::oracle::canonical) fn parse_promoted_controlled_keyword_grant(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("controlled-keyword-grant", input.tokens);
    let grant = recognize_controlled_keyword_grant(&mut cursor)?;
    let lowered = lower_temporary_keyword_grant(
        TemporaryKeywordGrantNode {
            objects: TemporaryKeywordGrantObjectsNode::ControlledSet(grant.objects),
            keywords: grant.keywords,
            duration: grant.duration,
        },
        "targetPermanent",
    )
    .ok_or_else(|| cursor.fail("a controller-scoped permanent keyword grant"))?;
    cursor.require(
        lowered.decision.is_none(),
        "a targetless controlled-set keyword grant",
    )?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": lowered.effects,
        }),
        &[
            "Recognize a controller-scoped battlefield object set",
            "Recognize each granted keyword as an independent typed atom",
            "Recognize an explicit typed effect duration",
            "Lower each keyword atom to one grantKeyword primitive",
            &trace,
        ],
    ))
}
