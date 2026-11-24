use super::super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_permanent_died_effect_sequence_family(
    input: &AbilityInput<'_>,
) -> bool {
    let mut cursor = OracleTokenCursor::new("permanent-died-effect-sequence-claim", input.tokens);
    if cursor.consume("whenever").is_err()
        || recognize_permanent_died_event(
            &mut cursor,
            input.name_links,
            input.name_resolution_failure,
        )
        .is_err()
    {
        return false;
    }
    if cursor.consume(",").is_err() {
        return true;
    }
    let body_position = cursor.position();
    let first = input
        .tokens
        .get(body_position)
        .map(|token| token.raw.as_str());
    let second = input
        .tokens
        .get(body_position + 1)
        .map(|token| token.raw.as_str());
    first.is_some_and(|raw| is_verb_form(&raw, VerbLemma::Create, VerbForm::Base))
        || first.is_some_and(|raw| raw.eq_ignore_ascii_case("target"))
            && second.is_some_and(|raw| raw.eq_ignore_ascii_case("player"))
        || first.is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
            && second.is_some_and(|raw| {
                is_verb_form(&raw, VerbLemma::Create, VerbForm::Base)
                    || LIFE_LEMMAS
                        .iter()
                        .any(|lemma| is_verb_form(raw, *lemma, VerbForm::Base))
            })
}

fn lower_event_branch(branch: &PermanentDiedEventBranchNode) -> Value {
    match branch {
        PermanentDiedEventBranchNode::Source(LocalObjectReference::SourceSelf) => json!({
            "kind": "permanentDied",
            "object": self_ref(),
        }),
        PermanentDiedEventBranchNode::Filtered {
            predicate,
            controller: controller_scope,
            exclude_source,
        } => {
            let mut event = json!({
                "kind": "permanentDied",
                "where": lower_permanent_object_predicate(predicate),
            });
            if *exclude_source {
                event["excludeSource"] = Value::Bool(true);
            }
            if let Some(controller_scope) = controller_scope {
                event["player"] = match controller_scope {
                    ObjectControllerScope::AbilityController => controller(),
                    ObjectControllerScope::OpponentOfAbilityController => json!({
                        "kind": "opponentsOf",
                        "player": controller(),
                    }),
                    ObjectControllerScope::NotAbilityController
                    | ObjectControllerScope::EventPlayer
                    | ObjectControllerScope::Player(_) => unreachable!(
                        "the permanent-died automaton rejects negative and event-bound controller scopes"
                    ),
                };
            }
            event
        }
    }
}

fn lower_event(event: &PermanentDiedEventNode) -> Value {
    let mut events = event
        .alternatives
        .iter()
        .map(lower_event_branch)
        .collect::<Vec<_>>();
    match events.len() {
        1 => events.pop().expect("one death event"),
        _ => json!({ "kind": "oneOf", "events": events }),
    }
}

pub(in crate::oracle::canonical) fn parse_promoted_permanent_died_effect_sequence(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("permanent-died-effect-sequence", input.tokens);
    let node = recognize_permanent_died_effect_sequence(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    let lowered = lower_local_effect_sequence(&mut cursor, &node.effects)?;
    cursor.require(
        !lowered.effects.is_empty(),
        "at least one executable atomic effect",
    )?;
    let mut rule = json!({
        "kind": "triggeredAbility",
        "source": self_ref(),
        "event": lower_event(&node.event),
        "effects": lowered.effects,
    });
    if !lowered.decisions.is_empty() {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": lowered.decisions,
        });
    }
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        rule,
        &[
            "Recognize permanent-death alternatives through the shared event automaton",
            "Delegate the complete body to the shared EffectSequence",
            "Preserve player-target declarations and anaphoric bindings from sequence lowering",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "permanent_died_effect_sequence_tests.rs"]
mod tests;
