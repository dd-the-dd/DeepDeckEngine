use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the count-conditioned entering-state family through reusable
/// lexical anchors: a source reference, `enters tapped`, `unless you
/// control|have`, and an `N or more/fewer` comparison. The committed
/// automaton validates every token.
pub(in crate::oracle::canonical) fn claims_promoted_controlled_count_entry_family(
    input: &AbilityInput<'_>,
) -> bool {
    source_reference_starts_at(
        0,
        input.tokens.first().map(|token| token.raw.as_str()),
        input.name_links,
        input.name_resolution_failure,
    ) && input.tokens.windows(2).any(|window| {
        window[0].raw.eq_ignore_ascii_case("enters") && window[1].raw.eq_ignore_ascii_case("tapped")
    }) && input.tokens.windows(3).any(|window| {
        window[0].raw.eq_ignore_ascii_case("unless")
            && window[1].raw.eq_ignore_ascii_case("you")
            && (window[2].raw.eq_ignore_ascii_case("control")
                || window[2].raw.eq_ignore_ascii_case("have"))
    }) && input.tokens.windows(2).any(|window| {
        window[0].token_type == "quantity" && window[1].raw.eq_ignore_ascii_case("or")
    })
}

pub(in crate::oracle::canonical) fn parse_promoted_controlled_count_entry(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("controlled-count-entry", input.tokens);
    let entry = recognize_controlled_count_entry(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("replacement.controlled-count-entry.validation");
    cursor.require(
        entry.source == LocalObjectReference::SourceSelf,
        "the current source as the entering object",
    )?;
    let operator = |direction: ControlledCountDirection| match direction {
        ControlledCountDirection::OrMore => ">=",
        ControlledCountDirection::OrFewer => "<=",
    };
    let exemption = match &entry.condition {
        EntryCountConditionNode::ControlledPermanents {
            threshold,
            direction,
            predicate,
        } => compare(
            operator(*direction),
            json!({
                "kind": "countPermanents",
                "player": controller(),
                "where": lower_permanent_object_predicate(predicate),
            }),
            integer(*threshold),
        ),
        EntryCountConditionNode::Opponents(condition) => {
            cursor.require(
                condition.player == LocalPlayerReference::Controller,
                "the source controller as the player whose opponents are counted",
            )?;
            compare(
                operator(condition.direction),
                json!({ "kind": "countOpponents", "player": controller() }),
                integer(condition.threshold),
            )
        }
    };
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "replacementEffect",
            "source": self_ref(),
            "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
            "condition": not(exemption),
            "replacement": [{
                "kind": "setEnteringState",
                "object": self_ref(),
                "tapped": true,
            }],
        }),
        &[
            "Resolve the entering source through the shared reference automaton",
            "Parse a numeric threshold and `or more`/`or fewer` comparison",
            "Count controlled permanents or opponents through shared condition children",
            "Lower count, comparison, negation, and entering state as independent primitives",
            &trace,
        ],
    ))
}
