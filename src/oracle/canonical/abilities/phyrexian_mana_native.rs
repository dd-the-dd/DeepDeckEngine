use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims a parenthesized single-symbol payment glossary structurally. The
/// child automaton owns color agreement, life quantity and full consumption.
pub(in crate::oracle::canonical) fn claims_promoted_phyrexian_mana_reminder_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.first().is_some_and(|token| token.raw == "(")
        && tokens
            .get(1)
            .is_some_and(|token| token.token_type == "manaSymbol" && token.raw.contains('/'))
        && tokens
            .get(2)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("can"))
        && tokens.last().is_some_and(|token| token.raw == ")")
}

pub(in crate::oracle::canonical) fn parse_promoted_phyrexian_mana_reminder(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let envelope = recognize_parenthetical_ability_envelope(input.tokens)?;
    let mut cursor = OracleTokenCursor::new("phyrexian-mana-reminder", envelope.inner);
    let reminder = recognize_phyrexian_mana_reminder(&mut cursor)?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "rulesMarker",
            "source": self_ref(),
            "text": input.source.text.trim(),
        }),
        &[
            "Resolve a whole-ability parenthetical token envelope",
            "Recognize one colored Phyrexian mana symbol",
            "Validate the matching fixed-mana alternative",
            &format!(
                "Retain the generic {} or {} life payment equivalence",
                reminder.mana_alternative.as_oracle_cost(),
                reminder.life_alternative,
            ),
            &trace,
        ],
    ))
}
