use super::super::*;
use crate::oracle::syntax::AbilityInput;

fn starts_targeted_temporary_modifier(input: &AbilityInput<'_>) -> bool {
    match input.tokens {
        [first, ..]
            if first.raw.eq_ignore_ascii_case("target")
                || first.raw.eq_ignore_ascii_case("up")
                || first.token_type == "quantity" =>
        {
            true
        }
        [first, second, ..]
            if first.raw.eq_ignore_ascii_case("another")
                && second.raw.eq_ignore_ascii_case("target") =>
        {
            true
        }
        _ => false,
    }
}

/// Claims only the literal signed P/T family. Variable deltas retain their
/// existing parser until the numeric-expression child can represent them.
pub(in crate::oracle::canonical) fn claims_promoted_temporary_power_toughness_family(
    input: &AbilityInput<'_>,
) -> bool {
    let first_sentence_end = input
        .tokens
        .iter()
        .position(|token| token.raw == ".")
        .unwrap_or(input.tokens.len());
    starts_targeted_temporary_modifier(input)
        && input.tokens[..first_sentence_end].windows(2).any(|window| {
            matches!(window[0].raw.to_ascii_lowercase().as_str(), "get" | "gets")
                && signed_power_toughness_from_raw(&window[1].raw).is_some()
        })
}
