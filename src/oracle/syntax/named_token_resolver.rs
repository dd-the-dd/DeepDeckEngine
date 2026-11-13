use super::super::model::RecognizedEntity;
use super::NamedCardIndex;

const MAX_NAMED_TOKEN_TOKENS: usize = 20;

/// Resolves only exact catalog names immediately before `token(s)`.
///
/// The bounded context prevents ordinary unknown words from being promoted to
/// named tokens. When names overlap, the longest exact span wins.
pub(crate) fn resolve_named_token_spans(
    tokens: &mut [RecognizedEntity],
    index: &impl NamedCardIndex,
) {
    for noun_index in 0..tokens.len() {
        if !matches_token_noun(&tokens[noun_index].raw) {
            continue;
        }
        let first = noun_index.saturating_sub(MAX_NAMED_TOKEN_TOKENS);
        let best = (first..noun_index)
            .filter(|start| {
                index
                    .exact_name_ends(tokens, *start, noun_index)
                    .contains(&noun_index)
            })
            .min();
        let Some(start) = best else {
            continue;
        };
        for token in &mut tokens[start..noun_index] {
            token.category = "magicVocabulary".to_string();
            token.token_type = "namedTokenWord".to_string();
        }
    }
}

fn matches_token_noun(raw: &str) -> bool {
    raw.eq_ignore_ascii_case("token") || raw.eq_ignore_ascii_case("tokens")
}
