use super::super::*;
use crate::oracle::syntax::AbilityInput;

fn token_is(input: &AbilityInput<'_>, index: usize, expected: &str) -> bool {
    input
        .tokens
        .get(index)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
}

/// Claims only the bounded `When this SOURCE enters, exile TARGET until this
/// SOURCE leaves the battlefield.` envelope. Selector vocabulary remains
/// deliberately unconstrained here so a committed child failure is surfaced.
pub(in crate::oracle::canonical) fn claims_promoted_linked_entry_exile_family(
    input: &AbilityInput<'_>,
) -> bool {
    if !token_is(input, 0, "when")
        || !token_is(input, 1, "this")
        || !token_is(input, 3, "enters")
        || !token_is(input, 4, ",")
        || !token_is(input, 5, "exile")
    {
        return false;
    }

    let Some(duration_index) =
        input
            .tokens
            .iter()
            .enumerate()
            .skip(6)
            .find_map(|(index, token)| {
                (token.token_type == "durationBoundaryWord"
                    && token.raw.eq_ignore_ascii_case("until"))
                .then_some(index)
            })
    else {
        return false;
    };
    let target_is_present = input.tokens[6..duration_index]
        .iter()
        .any(|token| token.token_type == "targetMarker");

    target_is_present
        && duration_index + 7 == input.tokens.len()
        && token_is(input, duration_index + 1, "this")
        && token_is(input, duration_index + 3, "leaves")
        && token_is(input, duration_index + 4, "the")
        && token_is(input, duration_index + 5, "battlefield")
        && token_is(input, duration_index + 6, ".")
}

fn lower_linked_exile_target(targets: &TargetListNode) -> (&'static str, Value, i64, i64) {
    let slot = targets
        .slots
        .first()
        .expect("linked-exile automaton validates one target slot");
    let candidates = match &slot.selector {
        TargetSelector::Object(predicate) => json!({
            "kind": "permanents",
            "where": lower_permanent_object_predicate(predicate),
        }),
        TargetSelector::ControlledObject {
            predicate,
            controller: scope,
        } => json!({
            "kind": "permanents",
            "controller": lower_object_controller_scope(*scope),
            "where": lower_permanent_object_predicate(predicate),
        }),
        TargetSelector::Any
        | TargetSelector::Union(_)
        | TargetSelector::Player(_)
        | TargetSelector::StackItem(_) => {
            unreachable!("linked-exile automaton rejects non-permanent target domains")
        }
    };
    (
        "exileTarget",
        candidates,
        i64::from(slot.cardinality.minimum),
        i64::from(slot.cardinality.maximum),
    )
}

pub(in crate::oracle::canonical) fn parse_promoted_linked_entry_exile(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("linked-entry-exile", input.tokens);
    let node = recognize_source_entry_linked_exile(&mut cursor)?;
    let (target_id, candidates, minimum, maximum) = lower_linked_exile_target(&node.effect.target);
    let target_reference = if maximum == 1 {
        chosen_target(target_id)
    } else {
        json!({ "kind": "chosenTargets", "id": target_id })
    };
    let return_when_source_leaves = matches!(
        node.effect.duration,
        LinkedExileDurationNode::UntilSourceLeavesBattlefield {
            source: LocalObjectReference::SourceSelf,
            source_kind,
        } if source_kind == node.source_kind
    );
    cursor.require(
        return_when_source_leaves,
        "a linked-exile duration owned by the entering source",
    )?;
    let event_object = match node.source {
        LocalObjectReference::SourceSelf => self_ref(),
    };
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
                "kind": "enterBattlefield",
                "object": event_object,
            },
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "kind": "chooseTargets",
                    "id": target_id,
                    "minimum": minimum,
                    "maximum": maximum,
                    "candidates": candidates.clone(),
                }],
            },
            "effects": [{
                "kind": "exilePermanentWithSource",
                "permanent": target_reference,
                "candidates": candidates,
                "minimum": minimum,
                "maximum": maximum,
                "returnWhenSourceLeaves": true,
            }],
        }),
        &[
            "Recognize one source-entry event",
            "Delegate cardinality and permanent predicates to the shared target automata",
            "Bind the duration to the same source identity",
            "Emit one primitive linked-exile effect",
            &trace,
        ],
    ))
}
