use super::super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::AbilityInput;

/// Claims the structural conditional free-cast envelope without constraining
/// the child controlled-permanent predicate. This lets commander, legendary
/// creature, and future typed predicates reuse the same machine.
pub(in crate::oracle::canonical) fn claims_promoted_conditional_free_cast_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("if"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("you"))
        && tokens
            .get(2)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("control"))
        && tokens.windows(6).any(|window| {
            ["you", "may", "cast", "this", "spell", "without"]
                .iter()
                .zip(window)
                .all(|(expected, token)| token.raw.eq_ignore_ascii_case(expected))
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_conditional_free_cast(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("conditional-free-cast", input.tokens);
    let envelope = recognize_conditional_free_cast_envelope(&mut cursor)?;
    let CurrentSpellManaCostReferenceNode = envelope.replaces;
    cursor.require(
        envelope.costs.costs.is_empty() && envelope.costs.connectors.is_empty(),
        "the zero-cost alternative represented by an empty cost sequence",
    )?;
    let condition = cursor.require_value(envelope.condition, "a free-cast condition")?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": {
                "kind": "alternativeCost",
                "costs": [],
                "condition": lower_condition(&condition),
            },
        }),
        &[
            "Recognize a conditional permission to cast the current spell",
            "Resolve the controlled-permanent predicate through the shared selector",
            "Represent the omitted mana payment as a zero-cost alternative",
            "Keep additional spell costs outside the alternative mana-cost primitive",
            &trace,
        ],
    ))
}

/// Claims hand-exile alternative costs, optionally preceded by a turn
/// condition and optionally composed with a fixed life payment.
/// The narrow envelope keeps unrelated alternative-cost grammars at their
/// explicit migration boundaries while committed child failures remain
/// visible for malformed members of this family.
pub(in crate::oracle::canonical) fn claims_promoted_compound_alternative_cost_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let Some(subject) = tokens
        .iter()
        .position(|token| token.raw.eq_ignore_ascii_case("you"))
    else {
        return false;
    };
    let conditioned = tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("if"));
    let supported_condition = !conditioned
        || ["if", "it's", "not", "your", "turn"]
            .iter()
            .enumerate()
            .all(|(index, expected)| {
                tokens
                    .get(index)
                    .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
            });
    supported_condition
        && (conditioned || subject == 0)
        && tokens
            .get(subject + 1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("may"))
        && (tokens
            .get(subject + 2)
            .is_some_and(|token| is_verb_form(&token.raw, VerbLemma::Exile, VerbForm::Base))
            || (tokens
                .get(subject + 2)
                .is_some_and(|token| is_verb_form(&token.raw, VerbLemma::Pay, VerbForm::Base))
                && tokens
                    .get(subject + 3)
                    .is_some_and(|token| token.token_type == "quantity")
                && tokens
                    .get(subject + 4)
                    .is_some_and(|token| token.raw.eq_ignore_ascii_case("life"))))
        && tokens.windows(2).any(|window| {
            window[0].raw.eq_ignore_ascii_case("rather")
                && window[1].raw.eq_ignore_ascii_case("than")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_compound_alternative_cost(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("alternative-mana-cost", input.tokens);
    let envelope = recognize_alternative_mana_cost_envelope(&mut cursor)?;
    let AlternativeManaCostEnvelopeNode {
        condition,
        costs,
        replaces,
    } = envelope;
    let CurrentSpellManaCostReferenceNode = replaces;
    cursor.require(
        costs.connectors.len() + 1 == costs.costs.len(),
        "one connector between each alternative cost",
    )?;
    cursor.require(
        costs
            .connectors
            .iter()
            .all(|connector| *connector == CostConnector::And),
        "`and` between composed alternative costs",
    )?;
    let lowered_costs = costs
        .costs
        .iter()
        .map(|cost| match cost {
            CompoundCostNode::PayLife(pay_life) => json!({
                "kind": "payLife",
                "player": controller(),
                "amount": integer(pay_life.amount),
            }),
            CompoundCostNode::ExileQualifiedCardFromHand(exile) => json!({
                "kind": "exileCard",
                "zone": {
                    "kind": "hand",
                    "player": controller(),
                },
                "where": {
                    "kind": "colorContains",
                    "value": exile.color.oracle_name(),
                },
            }),
        })
        .collect::<Vec<_>>();
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    let mut ability = json!({
        "kind": "alternativeCost",
        "costs": lowered_costs,
    });
    if let Some(condition) = condition.as_ref() {
        ability["condition"] = lower_condition(condition);
    }

    Ok(draft(
        json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": ability,
        }),
        &[
            "Recognize the alternative-mana-cost envelope from the shared token stream",
            "Compose independent pay-life and qualified hand-exile cost nodes",
            "Resolve the replaced cost as the current spell's mana cost",
            "Lower both costs to existing primitive IR",
            &trace,
        ],
    ))
}
