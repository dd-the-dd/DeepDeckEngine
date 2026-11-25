use super::super::counter_name::CounterName;
use super::super::model::RecognizedEntity;

const MAX_COUNTER_NAME_TOKENS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CounterNameResolverState {
    ScanCounterNoun,
    CollectNameBackward,
    Commit,
}

/// Contextually classifies the opaque span immediately before `counter(s)`.
///
/// This is a lexical enrichment pass over the one tokenization performed by
/// the pipeline. It never changes token boundaries or source spans. Unknown
/// words are resolved only inside this tightly bounded grammatical context;
/// every other unknown remains visible to the card-name resolver and audits.
pub(crate) fn resolve_counter_name_spans(tokens: &mut [RecognizedEntity]) -> Vec<CounterName> {
    let mut resolved = Vec::new();
    let mut state = CounterNameResolverState::ScanCounterNoun;
    let mut noun_index = 0;

    while noun_index < tokens.len() {
        state = match state {
            CounterNameResolverState::ScanCounterNoun => {
                if is_counter_noun(&tokens[noun_index].raw) {
                    CounterNameResolverState::CollectNameBackward
                } else {
                    noun_index += 1;
                    CounterNameResolverState::ScanCounterNoun
                }
            }
            CounterNameResolverState::CollectNameBackward => {
                let end = noun_index;
                let mut start = end;
                while start > 0
                    && end.saturating_sub(start) < MAX_COUNTER_NAME_TOKENS
                    && (is_counter_name_part_candidate(&tokens[start - 1])
                        || is_keyword_counter_name_head(
                            &tokens[start - 1].raw,
                            tokens.get(start).map(|token| token.raw.as_str()),
                        ))
                {
                    start -= 1;
                }

                if start == end {
                    noun_index += 1;
                    CounterNameResolverState::ScanCounterNoun
                } else {
                    for token in &mut tokens[start..end] {
                        // Power/toughness counter values are already a typed
                        // lexical atom used by specialized automata (Persist,
                        // Undying, and numeric counter effects). They still
                        // participate in the opaque CounterName assembled
                        // below, but must not lose their more precise type.
                        if token.token_type != "counterValue" {
                            token.category = "magicVocabulary".to_string();
                            token.token_type = "counterNamePart".to_string();
                        }
                    }
                    if let Some(name) = CounterName::from_parts(
                        tokens[start..end].iter().map(|token| token.raw.as_str()),
                    ) {
                        resolved.push(name);
                    }
                    CounterNameResolverState::Commit
                }
            }
            CounterNameResolverState::Commit => {
                noun_index += 1;
                CounterNameResolverState::ScanCounterNoun
            }
        };
    }

    resolved
}

/// `first` / `double` before `strike`: the head of a keyword counter
/// name (CR 122.1b: first strike and double strike counters). The lexer
/// classifies the head as an ordinal or multiplier elsewhere; inside a
/// counter name it is the keyword's first word.
pub(crate) fn is_keyword_counter_name_head(raw: &str, next: Option<&str>) -> bool {
    (raw.eq_ignore_ascii_case("first") || raw.eq_ignore_ascii_case("double"))
        && next.is_some_and(|next| next.eq_ignore_ascii_case("strike"))
}

fn is_counter_noun(raw: &str) -> bool {
    raw.eq_ignore_ascii_case("counter") || raw.eq_ignore_ascii_case("counters")
}

/// Whether a token can be part of a counter name: the lexer's own token-class
/// check, shared with the grammar that reads a counter name whose
/// `counter(s)` noun is shared with a later option.
pub(crate) fn is_counter_name_part_candidate(token: &RecognizedEntity) -> bool {
    if is_counter_noun(&token.raw) || !has_counter_name_shape(&token.raw) {
        return false;
    }

    !matches!(
        token.token_type.as_str(),
        "actionWord"
            | "cardinalityWord"
            | "comparisonWord"
            | "counterTypeWord"
            | "costActionWord"
            | "costWord"
            | "determinerWord"
            | "directionWord"
            | "eventWord"
            | "logicWord"
            | "modalityWord"
            | "negativePredicateWord"
            | "numericOperatorWord"
            | "ordinalWord"
            | "possessivePlayerRoleWord"
            | "possessiveTypeWord"
            | "predicateWord"
            | "quantifierWord"
            | "quantity"
            | "referenceWord"
            | "relationWord"
            | "replacementModalWord"
            | "separator"
            | "substitutionWord"
            | "tapSymbol"
            | "targetMarker"
            | "targetNoun"
            | "triggerWord"
    )
}

fn has_counter_name_shape(raw: &str) -> bool {
    !raw.is_empty()
        && raw.chars().all(|character| {
            character.is_alphanumeric() || matches!(character, '+' | '-' | '/' | '\'' | '\u{2019}')
        })
}
