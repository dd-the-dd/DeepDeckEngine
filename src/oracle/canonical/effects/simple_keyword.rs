use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_simple_keyword_family(
    input: &AbilityInput<'_>,
) -> bool {
    let mut cursor = OracleTokenCursor::new("simple-keyword-list", input.tokens);
    let failure = match recognize_standalone_keyword_ability(&mut cursor) {
        Ok(_) => return true,
        Err(failure) => failure,
    };

    let committed_keyword_reminder = failure
        .trace
        .iter()
        .any(|transition| transition.state == "keyword.reminder.open");

    let first_is_exalted = input.tokens.first().is_some_and(|token| {
        parameterless_keyword_from_oracle_word(&token.raw) == Some(GrantedKeyword::Exalted)
    });
    let ends_with_bare_ward = failure.state == "keyword.ward.cost"
        && input
            .tokens
            .last()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("ward"));
    let ends_with_list_connector = failure.state == "keyword-list.item"
        && input
            .tokens
            .last()
            .is_some_and(|token| token.raw == "," || token.raw.eq_ignore_ascii_case("and"));
    let is_bare_ward_ability = input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("ward"))
        && input.tokens.len() == 1;

    committed_keyword_reminder
        || first_is_exalted
        || ends_with_bare_ward
        || ends_with_list_connector
        || is_bare_ward_ability
}

pub(in crate::oracle::canonical) fn parse_promoted_simple_keyword(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("simple-keyword-list", input.tokens);
    let keywords = recognize_standalone_keyword_ability(&mut cursor)?.keywords;
    let abilities = keywords
        .into_iter()
        .map(|keyword| match keyword {
            KeywordAtomNode::Parameterless(keyword) => {
                json!({ "kind": keyword.canonical_name() })
            }
            KeywordAtomNode::Ward { cost } => json!({
                "kind": "ward",
                "cost": {
                    "kind": "payMana",
                    "manaCost": cost.as_oracle_cost(),
                },
            }),
        })
        .collect::<Vec<_>>();
    let transition_trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    let rule = if let [ability] = abilities.as_slice() {
        json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": ability,
        })
    } else {
        json!({
            "kind": "keywordAbilityGroup",
            "source": self_ref(),
            "abilities": abilities,
        })
    };
    Ok(draft(
        rule,
        &[
            "Recognize a parameterless-keyword list from the shared token stream",
            "Lower one keyword as the existing singular primitive or several as a primitive group",
            &transition_trace,
        ],
    ))
}
