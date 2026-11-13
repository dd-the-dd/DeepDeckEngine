use super::super::*;
use crate::oracle::canonical::automata::entry_replacement::{
    recognize_exile_instead_replacement, recognize_object_not_cast_condition,
    recognize_would_enter_battlefield_event,
};
use crate::oracle::syntax::AbilityInput;

/// Claims the structural replacement boundary only. Object predicate, event,
/// cast provenance, anaphora, and destination remain committed child-machine
/// responsibilities after this boundary is selected.
pub(in crate::oracle::canonical) fn claims_promoted_uncast_entry_replacement_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("if"))
        && tokens.windows(2).any(|window| {
            window[0].raw.eq_ignore_ascii_case("would")
                && window[1].raw.eq_ignore_ascii_case("enter")
        })
        && tokens.windows(2).any(|window| {
            (window[0].raw.eq_ignore_ascii_case("wasn't")
                || window[0].raw.eq_ignore_ascii_case("wasnâ€™t"))
                && window[1].raw.eq_ignore_ascii_case("cast")
        })
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("exile"))
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("instead"))
        && tokens.last().is_some_and(|token| token.raw == ".")
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UncastEntryReplacementNode {
    event: WouldEnterBattlefieldEventNode,
    condition: ObjectCastConditionNode,
    replacement: ZoneReplacementNode,
}

fn recognize_uncast_entry_replacement(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<UncastEntryReplacementNode> {
    cursor.enter("replacement.uncast-entry.envelope");
    cursor.consume("if")?;
    let event = recognize_would_enter_battlefield_event(cursor)?;
    let condition = recognize_object_not_cast_condition(cursor, event.binding)?;
    let replacement = recognize_exile_instead_replacement(cursor, event.binding)?;
    cursor.enter("replacement.uncast-entry.terminator");
    cursor.consume(".")?;
    cursor.enter("replacement.uncast-entry.full-consumption");
    cursor.finish()?;

    cursor.enter("replacement.uncast-entry.binding-validation");
    cursor.require(
        event.binding == condition.object && event.binding == replacement.object,
        "one event-object binding shared by condition and replacement",
    )?;
    cursor.require(
        condition.status == ObjectCastStatus::NotCast,
        "the entering object not having been cast",
    )?;
    cursor.require(
        replacement.destination == ReplacementDestination::Exile,
        "exile as the replacement destination",
    )?;
    Ok(UncastEntryReplacementNode {
        event,
        condition,
        replacement,
    })
}

pub(in crate::oracle::canonical) fn parse_promoted_uncast_entry_replacement(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("uncast-entry-replacement", input.tokens);
    let node = recognize_uncast_entry_replacement(&mut cursor)?;
    let where_predicate = lower_permanent_object_predicate(&node.event.object);
    let modifier_kind = match node.condition.status {
        ObjectCastStatus::NotCast => "prohibitUncastBattlefieldEntry",
    };
    let destination = match node.replacement.destination {
        ReplacementDestination::Exile => "exile",
    };
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "modifiers": [{
                "kind": modifier_kind,
                "where": where_predicate,
                "destination": destination,
            }],
        }),
        &[
            "Recognize a typed would-enter-the-battlefield event",
            "Retain the entering object as a shared replacement binding",
            "Require that object not to have been cast",
            "Replace its battlefield entry with exile",
            &trace,
        ],
    ))
}
