use super::super::*;
use crate::oracle::syntax::AbilityInput;

fn starts_permanent_set(raw: &str, token_type: &str) -> bool {
    matches!(
        token_type,
        "typeWord" | "negativeTypeWord" | "colorWord" | "supertypeWord"
    ) || raw.chars().next().is_some_and(char::is_uppercase)
}

/// Claims a persistent Riot-grant envelope while leaving the object set,
/// controller relation, keyword and reminder to their typed child automata.
pub(in crate::oracle::canonical) fn claims_promoted_static_riot_grant_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens
        .first()
        .is_some_and(|token| starts_permanent_set(&token.raw, &token.token_type))
        && tokens.windows(4).any(|window| {
            window[0].raw.eq_ignore_ascii_case("you")
                && window[1].raw.eq_ignore_ascii_case("control")
                && (window[2].raw.eq_ignore_ascii_case("have")
                    || window[2].raw.eq_ignore_ascii_case("has"))
                && window[3].raw.eq_ignore_ascii_case("riot")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_static_riot_grant(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("static-riot-grant", input.tokens);
    let grant = recognize_static_riot_grant(&mut cursor)?;
    cursor.enter("static-keyword-grant.validation");
    cursor.require(
        grant.objects.controller == ObjectControllerScope::AbilityController,
        "a controller-scoped permanent set",
    )?;
    if let Some(reminder) = &grant.keyword.reminder {
        cursor.require(
            reminder.counter == "+1/+1" && reminder.alternative_keyword == GrantedKeyword::Haste,
            "the typed riot counter-or-haste reminder",
        )?;
    }
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
            "activeWhile": active_while_battlefield(),
            "modifiers": [{
                "kind": "grantKeyword",
                "objects": {
                    "kind": "permanents",
                    "controller": controller(),
                    "where": lower_permanent_object_predicate(&grant.objects.predicate),
                },
                "keyword": "riot",
            }],
        }),
        &[
            "Recognize the controlled permanent set through the shared selector automaton",
            "Recognize Riot as an independent keyword child",
            "Validate the optional reminder as typed +1/+1-counter or haste choices",
            "Lower Riot through the generic atomic grantKeyword modifier",
            &trace,
        ],
    ))
}
