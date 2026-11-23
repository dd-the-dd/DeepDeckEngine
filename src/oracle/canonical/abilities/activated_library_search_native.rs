use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims a token-structured activated library search. The activation parent
/// owns the colon while the shared cost and search children own their local
/// grammar. Once this envelope is visible, malformed costs or search clauses
/// remain native machine failures rather than falling through to legacy text
/// parsing.
pub(in crate::oracle::canonical) fn claims_promoted_activated_library_search_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    if !tokens
        .first()
        .is_some_and(|token| token.token_type == "tapSymbol")
    {
        return false;
    }

    let Some(separator) = tokens.iter().position(|token| token.raw == ":") else {
        return false;
    };
    tokens
        .get(separator + 1..separator + 5)
        .is_some_and(|effect_head| {
            ["search", "your", "library", "for"]
                .iter()
                .zip(effect_head)
                .all(|(expected, token)| token.raw.eq_ignore_ascii_case(expected))
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_activated_library_search(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("activated-library-search", input.tokens);
    let ability = recognize_activated_library_search(&mut cursor)?;
    cursor.enter("activation.library-search.validation");
    cursor.require(
        ability.search.player == LocalPlayerReference::Controller,
        "the ability controller as the searching player",
    )?;
    let SearchDestination::Battlefield { .. } = ability.search.destination else {
        return Err(cursor.fail("a battlefield destination for an activated library search"));
    };
    let costs = ability
        .costs
        .costs
        .iter()
        .flat_map(lower_local_activation_costs)
        .collect::<Vec<_>>();
    let mut effects = lower_library_search_effects(&mut cursor, &ability.search, controller())?;
    if let Some(followup) = &ability.followup {
        cursor.require(
            followup.effect.binding == ability.search.binding,
            "the searched collection as the conditional untap subject",
        )?;
        effects.push(json!({
            "kind": "conditionalEffect",
            "condition": lower_condition(&followup.condition),
            "then": [{
                "kind": "untapPermanents",
                "objects": decision_result(&lower_collection_binding(followup.effect.binding)),
            }],
            "else": [],
        }));
    }
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        with_activation_object_cost_decisions(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "effects": effects,
            }),
            &ability.costs,
        ),
        &[
            "Recognize one activated-ability envelope from the shared token stream",
            "Compose tap, fixed life payment, and source sacrifice as independent cost children",
            "Delegate subtype alternatives and result anaphora to the shared library-search automaton",
            "Lower choose, move, shuffle, and search record as independent primitive effects",
            "Compose an optional controlled-count condition with a bound-result untap",
            &trace,
        ],
    ))
}
