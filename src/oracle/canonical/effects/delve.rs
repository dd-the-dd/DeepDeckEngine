use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_delve_keyword_family(
    input: &AbilityInput<'_>,
) -> bool {
    input.tokens.first().is_some_and(|token| {
        token.token_type == "keywordWord" && token.raw.eq_ignore_ascii_case("delve")
    })
}

pub(in crate::oracle::canonical) fn parse_promoted_delve_keyword(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("keyword-delve", input.tokens);
    recognize_delve_keyword(&mut cursor)?;
    Ok(draft(
        json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": { "kind": "delve" },
        }),
        &["Recognize Delve and validate its optional canonical reminder token by token"],
    ))
}

pub(in crate::oracle::canonical) fn claims_promoted_decision_entry_counters_family(
    input: &AbilityInput<'_>,
) -> bool {
    source_reference_starts_at(
        0,
        input.tokens.first().map(|token| token.raw.as_str()),
        input.name_links,
        input.name_resolution_failure,
    ) && input.tokens.windows(2).any(|window| {
        window[0].raw.eq_ignore_ascii_case("enters") && window[1].raw.eq_ignore_ascii_case("with")
    }) && input.tokens.windows(2).any(|window| {
        window[0].raw.eq_ignore_ascii_case("for") && window[1].raw.eq_ignore_ascii_case("each")
    }) && input.tokens.windows(2).any(|window| {
        window[0].raw.eq_ignore_ascii_case("exiled") && window[1].raw.eq_ignore_ascii_case("with")
    })
}

pub(in crate::oracle::canonical) fn parse_promoted_decision_entry_counters(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("decision-entry-counters", input.tokens);
    let node = recognize_delve_entry_counters(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("replacement.decision-entry-counters.source-shape");
    cursor.require(
        node.source == LocalObjectReference::SourceSelf,
        "the current source entering",
    )?;
    Ok(draft(
        json!({
            "kind": "replacementEffect",
            "source": self_ref(),
            "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
            "replacement": [{
                "kind": "putEnteringCounters",
                "counter": node.counter.as_str(),
                "count": {
                    "kind": "countDecisionCardsMatching",
                    "decisionId": "delveCards",
                    "where": lower_card_object_predicate(&node.cards),
                },
            }],
        }),
        &[
            "Recognize an entering source and a named counter",
            "Filter cards retained by the casting decision through the shared selector",
            "Apply one primitive entering-counter replacement",
        ],
    ))
}

pub(in crate::oracle::canonical) fn claims_promoted_graveyard_leave_counter_family(
    input: &AbilityInput<'_>,
) -> bool {
    let event_prefix = input.tokens.first().is_some_and(|token| {
        token.token_type == "triggerWord" && token.raw.eq_ignore_ascii_case("whenever")
    }) && input.tokens.iter().any(|token| {
        token.raw.eq_ignore_ascii_case("leave") || token.raw.eq_ignore_ascii_case("leaves")
    }) && input
        .tokens
        .iter()
        .any(|token| token.raw.eq_ignore_ascii_case("graveyard"));
    if !event_prefix {
        return false;
    }

    let Some(effect_start) = input.tokens.iter().position(|token| token.raw == ",") else {
        return false;
    };
    let mut cursor = OracleTokenCursor::new("graveyard-leave-counter-claim", input.tokens);
    cursor.advance_to(effect_start + 1).is_ok() && claims_put_counters_effect(&cursor)
}

pub(in crate::oracle::canonical) fn parse_promoted_graveyard_leave_counter(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("graveyard-leave-counter", input.tokens);
    let node = recognize_graveyard_leave_counter_trigger(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("trigger.graveyard-leave.lower-effect");
    let effect = lower_targetless_local_effect(
        &LocalEffectNode::PutCounters(node.effect),
        TargetlessDeclarationSupport::None,
    )
    .map_err(|error| cursor.fail(error.expectation()))?;
    Ok(draft(
        json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": {
                "kind": "cardsLeftGraveyard",
                "player": controller(),
                "where": lower_card_object_predicate(&node.cards),
                "aggregation": match node.aggregation {
                    GraveyardLeaveAggregationNode::PerCard => "perCard",
                    GraveyardLeaveAggregationNode::OneOrMore => "oneOrMore",
                },
            },
            "effects": [effect],
        }),
        &[
            "Recognize a controller-graveyard leave event and its batch aggregation",
            "Reuse the shared card selector and independent put-counters effect",
        ],
    ))
}
