use super::super::*;
use crate::oracle::characteristic_vocabulary::{AcceptedForms, CardType, lookup_card_type};
use crate::oracle::syntax::AbilityInput;

/// Claims the keyword `Suspend`. The ability word `Suspended Animation` is a
/// different token and stays with its own family.
pub(in crate::oracle::canonical) fn claims_promoted_suspend_family(
    input: &AbilityInput<'_>,
) -> bool {
    input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("suspend"))
}

/// CR 702.62a: the hand-zone alternative to casting is retained as one typed
/// keyword. Its two exile-zone triggered abilities are not separate printed
/// text; the runtime expands this keyword into ordinary triggered rules.
pub(in crate::oracle::canonical) fn parse_promoted_suspend(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("keyword-suspend", input.tokens);
    cursor.enter("keyword.suspend.source-context");
    cursor.require(
        castable_face(input.face_type_line),
        "a nonland face that can begin to be cast from hand for Suspend",
    )?;
    let suspend = recognize_suspend_keyword(&mut cursor)?;
    Ok(draft(
        json!({"kind":"keywordAbility", "source":self_ref(),
            "ability":{
                "kind":"suspend",
                "timeCounters":suspend.time_counters,
                "cost":{"kind":"payMana","manaCost":suspend.cost.as_oracle_cost()},
            }
        }),
        &[
            "Recognize the registered Suspend keyword with a fixed positive time-counter count",
            "Retain the shared typed fixed mana cost and validate an optional reminder against both parameters",
            "Reuse the hand-zone suspend action and the runtime expansion into exile-zone upkeep and last-counter triggers",
        ],
    ))
}

fn castable_face(type_line: &str) -> bool {
    !type_line
        .split('\u{2014}')
        .next()
        .unwrap_or(type_line)
        .split(|character: char| !character.is_alphabetic())
        .filter(|word| !word.is_empty())
        .any(|word| lookup_card_type(word, AcceptedForms::SingularOnly) == Some(CardType::Land))
}

#[cfg(test)]
#[path = "suspend_native_tests.rs"]
mod tests;
