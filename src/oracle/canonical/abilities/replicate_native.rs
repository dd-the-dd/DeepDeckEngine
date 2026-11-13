use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the mana-Replicate family at the keyword-and-cost envelope. Once
/// claimed, malformed reminders remain native structured failures rather than
/// falling back to the historical whole-line regular expression.
pub(in crate::oracle::canonical) fn claims_promoted_mana_replicate_family(
    input: &AbilityInput<'_>,
) -> bool {
    input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("replicate"))
        && input
            .tokens
            .get(1)
            .is_some_and(|token| token.token_type == "manaSymbol")
}

pub(in crate::oracle::canonical) fn parse_promoted_mana_replicate(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("keyword-replicate", input.tokens);
    let replicate = recognize_mana_replicate(&mut cursor)?;
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
                "kind": "replicate",
                "cost": {
                    "kind": "payMana",
                    "manaCost": replicate.cost.as_oracle_cost(),
                },
                "repeatable": true,
            },
        }),
        &[
            "Recognize Replicate from the shared token stream",
            "Delegate its repeatable payment to the mana-cost automaton",
            "Validate the optional copy and retarget reminder sentences structurally",
            &trace,
        ],
    ))
}
