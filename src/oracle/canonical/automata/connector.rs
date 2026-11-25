use super::super::{MachineResult, OracleTokenCursor};
use super::EffectConnector;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConnectorState {
    OptionalPunctuation,
    Kind,
    Complete,
}

/// Recognizes an effect-sequence connector. The lexical layer only identifies
/// punctuation and logic words; this parent-context child decides that `and`
/// or `then`, or a comma at an already completed effect boundary, orders
/// independent effects. Children retain their internal punctuation.
pub(in crate::oracle::canonical) fn recognize_effect_connector(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EffectConnector> {
    let mut state = ConnectorState::OptionalPunctuation;
    let mut connector = None;
    let mut coordinating_comma = false;
    loop {
        state = match state {
            ConnectorState::OptionalPunctuation => {
                cursor.enter("sequence.connector.optional-punctuation");
                if cursor.peek().is_some_and(|token| token == ",") {
                    cursor.consume(",")?;
                    coordinating_comma = true;
                }
                ConnectorState::Kind
            }
            ConnectorState::Kind => {
                cursor.enter("sequence.connector.kind");
                connector = Some(
                    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                        Some("and") => {
                            cursor.consume("and")?;
                            EffectConnector::And
                        }
                        Some("then") => {
                            cursor.consume("then")?;
                            EffectConnector::Then
                        }
                        // `ACTION or ACTION` (CR 608.2d): the alternatives the
                        // performer chooses between while the effect resolves.
                        // Never sequential conjunction: the sequence frames the
                        // run of `or`-joined actions as one resolution choice.
                        Some("or") => {
                            cursor.consume("or")?;
                            EffectConnector::Or
                        }
                        // Do not consume or guess the next action here. The
                        // sequence parent delegates it transactionally to a child.
                        _ if coordinating_comma => EffectConnector::And,
                        _ => return Err(cursor.fail("`and` or `then`")),
                    },
                );
                ConnectorState::Complete
            }
            ConnectorState::Complete => {
                cursor.enter("sequence.connector.complete");
                return cursor.require_value(connector, "a populated effect connector");
            }
        };
    }
}

/// Gapped coordination (verb ellipsis): `destroy A and all B`, `put N
/// counters on A, M counters on B`. The conjunct after an `and` (or a
/// coordinating comma) is an argument frame of the previous imperative
/// clause's lemma with the verb elided. This view writes the elided verb in
/// the conjunction's slot (`slot`, the token just before the conjunct): the
/// ordinary readers then read the conjunct as a complete clause of the same
/// lemma. Spans and indices stay the original ones, so every diagnostic
/// points at the written text; the conjunction itself was already read as
/// the connector of the two actions.
pub(in crate::oracle::canonical) fn elided_verb_tokens(
    cursor: &OracleTokenCursor<'_>,
    slot: usize,
    lemma: super::VerbLemma,
) -> Vec<crate::oracle::model::RecognizedEntity> {
    elided_verb_tokens_in_form(cursor, slot, lemma, super::VerbForm::Base)
}

/// The same view for a finite antecedent clause (`target player sacrifices
/// an artifact and a land`): the elided verb is written in the form its
/// subject gave the written verb, so the conjunct reads as a subjectless
/// verb of the same performer.
pub(in crate::oracle::canonical) fn elided_verb_tokens_in_form(
    cursor: &OracleTokenCursor<'_>,
    slot: usize,
    lemma: super::VerbLemma,
    form: super::VerbForm,
) -> Vec<crate::oracle::model::RecognizedEntity> {
    let mut tokens = cursor.token_slice(0, usize::MAX).to_vec();
    if let Some(token) = tokens.get_mut(slot) {
        let surface = super::verb_entry(lemma).surface(form);
        if let Some(class) = crate::oracle::syntax::recognize_entities(surface)
            .into_iter()
            .next()
        {
            token.category = class.category;
            token.token_type = class.token_type;
        }
        token.raw = surface.to_string();
    }
    tokens
}

/// The imperative lemma of the clause whose first token is `start`: the
/// lemma a gapped conjunct of the same sentence elides.
pub(in crate::oracle::canonical) fn imperative_lemma_at(
    cursor: &OracleTokenCursor<'_>,
    start: usize,
) -> Option<super::VerbLemma> {
    let token = cursor.token_slice(start, start + 1).first()?;
    super::lookup_effect_verb(&token.raw)
        .filter(|(_, forms)| forms.contains(&super::VerbForm::Base))
        .map(|(entry, _)| entry.lemma)
}

/// A gapped conjunct whose library position elides its library (`put that
/// card on top of your library and the rest on the bottom in any order`):
/// the bottom or top of the conjunct is the library its antecedent clause
/// names. Returns the view with the antecedent's `of POSSESSOR library`
/// written after the elided position, the index the phrase starts at and
/// its length. Spans stay the written ones: the inserted words point at the
/// antecedent phrase they restate. `None` when either half is absent.
pub(in crate::oracle::canonical) fn elided_library_tokens(
    tokens: &[crate::oracle::model::RecognizedEntity],
    conjunct_start: usize,
    antecedent: &[crate::oracle::model::RecognizedEntity],
) -> Option<(Vec<crate::oracle::model::RecognizedEntity>, usize, usize)> {
    let is = |token: Option<&crate::oracle::model::RecognizedEntity>, word: &str| {
        token.is_some_and(|token| token.raw.eq_ignore_ascii_case(word))
    };
    let position_word = |token: &crate::oracle::model::RecognizedEntity| {
        token.raw.eq_ignore_ascii_case("top") || token.raw.eq_ignore_ascii_case("bottom")
    };
    // The antecedent's `top|bottom of POSSESSOR... library`.
    let phrase = antecedent.iter().enumerate().find_map(|(index, token)| {
        if !position_word(token) || !is(antecedent.get(index + 1), "of") {
            return None;
        }
        let end = (index + 2..antecedent.len().min(index + 6))
            .find(|end| is(antecedent.get(*end), "library"))?;
        Some(antecedent[index + 1..=end].to_vec())
    })?;
    // The conjunct's `on the bottom|top` / `on top` with no library after it.
    let mut index = conjunct_start;
    while let Some(token) = tokens.get(index) {
        if token.raw == "." || token.raw == ";" {
            return None;
        }
        let on_position = index >= 1
            && (is(tokens.get(index - 1), "on")
                || (index >= 2
                    && is(tokens.get(index - 1), "the")
                    && is(tokens.get(index - 2), "on")));
        if position_word(token) && on_position && !is(tokens.get(index + 1), "of") {
            let insert_at = index + 1;
            let mut view = tokens[..insert_at].to_vec();
            view.extend(phrase.iter().cloned());
            view.extend(tokens[insert_at..].iter().cloned());
            return Some((view, insert_at, phrase.len()));
        }
        index += 1;
    }
    None
}

/// The finite verb of a clause whose explicit subject starts at `start`
/// and whose predicate ends before `end`: the first token in the subject's
/// agreement form that the verb lexicon knows. A gapped conjunct of that
/// clause elides this verb (`deals 1 damage to A and an additional 1 damage
/// to B`, `sacrifices an artifact and a land`). `None` when the clause has
/// no such verb (an imperative clause, or a verb the lexicon lacks).
pub(in crate::oracle::canonical) fn finite_clause_verb(
    cursor: &OracleTokenCursor<'_>,
    start: usize,
    end: usize,
    agreement: super::VerbAgreement,
) -> Option<(super::VerbLemma, super::VerbForm)> {
    let form = agreement.form();
    cursor
        .token_slice(start, end)
        .iter()
        .skip(1)
        .find_map(|token| {
            super::lookup_verb(&token.raw, &super::ALL_LEMMAS)
                .filter(|(_, forms)| forms.contains(&form))
                .map(|(entry, _)| (entry.lemma, form))
        })
}

#[cfg(test)]
#[path = "connector_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "coordination_tests.rs"]
mod coordination_tests;
