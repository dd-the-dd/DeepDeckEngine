use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims a choose-target/search envelope from token topology only. Exact
/// target predicates, zones, name references and sequence punctuation remain
/// responsibilities of the committed child automaton.
pub(in crate::oracle::canonical) fn claims_promoted_targeted_same_name_search_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let first_sentence_end = tokens.iter().position(|token| token.raw == ".");
    let same_name_relation = tokens.windows(3).any(|window| {
        window[0].raw.eq_ignore_ascii_case("same")
            && window[1].raw.eq_ignore_ascii_case("name")
            && window[2].raw.eq_ignore_ascii_case("as")
    });
    let owner_reference = tokens.windows(2).any(|window| {
        window[0].raw.eq_ignore_ascii_case("its")
            && (window[1].raw.eq_ignore_ascii_case("owner's")
                || window[1].raw.eq_ignore_ascii_case("owner’s"))
    });

    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("choose"))
        && tokens
            .get(1)
            .is_some_and(|token| token.token_type == "targetMarker")
        && first_sentence_end
            .and_then(|boundary| tokens.get(boundary + 1))
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("search"))
        && same_name_relation
        && owner_reference
}

pub(in crate::oracle::canonical) fn parse_promoted_targeted_same_name_search(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("targeted-same-name-search", input.tokens);
    let _search = recognize_targeted_same_name_search(&mut cursor)?;
    cursor.enter("targeted-same-name-search.generic-movement-unavailable");
    Err(cursor
        .fail("proved private zone inspection, captured name predicate and moveCards primitives"))
}
