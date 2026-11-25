use super::super::{MachineFailure, OracleTokenCursor};
use super::RecognizedEffectFragment;

/// Unbound grammatical roles. The parent resolves lexical scope and set subtraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardCollectionReferenceNode {
    Antecedent,
    Remainder,
    Other,
    Selected,
    /// `from it` naming an observed collection through a singular zone noun
    /// (`target opponent reveals their hand. You choose a card from it`).
    /// Only the selection operand reads it; the lowerer proves the zone.
    Zone,
    /// `the exiled card(s)`: the cards this program exiled, else the cards
    /// exiled by a linked ability of the same object (CR 607.2a).
    Exiled,
    /// `[each|all] card(s) exiled with this NOUN|it` (CR 607.2a).
    Linked,
    /// `the revealed card(s)`: the cards the most recent reveal action of
    /// this program revealed. The lowerer proves that reveal exists.
    Revealed,
    /// `[the] cards VERBed this way` read as the origin of a cast frame
    /// (`from among cards revealed this way`): the whole observed collection
    /// of the scope, whose producing verb the lowerer proves (CR 608.2c).
    /// Only the cast frame's origin reader produces it; an ordinary operand
    /// keeps the `all cards VERBed this way` selection.
    Restated(super::CardCollectionVerb),
}

/// `card(s) exiled with this NOUN` / `with it` after the head the caller read.
fn recognize_exiled_with_source(trial: &mut OracleTokenCursor<'_>) -> Result<(), MachineFailure> {
    let noun = trial.peek().map(str::to_ascii_lowercase);
    if !matches!(noun.as_deref(), Some("card" | "cards")) {
        return Err(trial.fail("`card` or `cards` before `exiled with`"));
    }
    trial.consume(noun.as_deref().unwrap_or("cards"))?;
    trial.enter("collection.reference.linked-exile");
    trial.consume_phrase(&["exiled", "with"])?;
    if trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("it"))
    {
        return trial.consume("it");
    }
    trial.consume("this")?;
    let noun = trial
        .peek()
        .ok_or_else(|| trial.fail("a source noun after `exiled with this`"))?
        .to_string();
    trial.require(
        crate::oracle::characteristic_vocabulary::lookup_object_selector_lexeme(&noun).is_some()
            || crate::oracle::characteristic_vocabulary::lookup_noncreature_subtype(&noun)
                .is_some(),
        "a registered source noun after `exiled with this`",
    )?;
    trial.consume(&noun)
}

/// `of them`, `of those cards`, `of the [revealed|exiled] cards` after `the
/// rest`: the restated whole collection whose remainder is named. It names
/// no other basis than the observed collection, so the role stays the
/// remainder; the lowerer proves the preceding selection.
fn recognize_restated_remainder_basis(
    trial: &mut OracleTokenCursor<'_>,
) -> Result<(), MachineFailure> {
    trial.enter("collection.reference.remainder-basis");
    trial.consume("of")?;
    match trial.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("them") => trial.consume("them"),
        Some("those") => trial.consume_phrase(&["those", "cards"]),
        Some("the") => {
            trial.consume("the")?;
            if trial.peek().is_some_and(|raw| {
                raw.eq_ignore_ascii_case("revealed") || raw.eq_ignore_ascii_case("exiled")
            }) {
                let verb = trial.peek().unwrap_or("revealed").to_owned();
                trial.consume(&verb)?;
            }
            trial.consume("cards")
        }
        _ => Err(trial.fail("`them`, `those cards` or `the [revealed] cards` after `the rest of`")),
    }
}

pub(in crate::oracle::canonical) enum CollectionOperandAttempt<T> {
    NotClaimed,
    Recognized(RecognizedEffectFragment<T>),
    RejectedCandidate(MachineFailure),
    FailedCommitted(MachineFailure),
}

pub(in crate::oracle::canonical) type CardCollectionReferenceAttempt =
    CollectionOperandAttempt<CardCollectionReferenceNode>;

/// A singular complement is a role, not a runtime demand to find one card.
pub(in crate::oracle::canonical) fn try_recognize_card_collection_reference(
    cursor: &mut OracleTokenCursor<'_>,
) -> CardCollectionReferenceAttempt {
    let mut trial = cursor.clone();
    trial.enter("collection.reference.head");
    let node = match trial.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("them") => {
            if let Err(f) = trial.consume("them") {
                return CollectionOperandAttempt::FailedCommitted(f);
            }
            CardCollectionReferenceNode::Antecedent
        }
        Some("it") => {
            if let Err(f) = trial.consume("it") {
                return CollectionOperandAttempt::FailedCommitted(f);
            }
            CardCollectionReferenceNode::Selected
        }
        // `those exiled cards`: the plural restatement of `the exiled cards`.
        Some("those")
            if trial
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("exiled")) =>
        {
            if let Err(f) = trial.consume_phrase(&["those", "exiled", "cards"]) {
                return CollectionOperandAttempt::RejectedCandidate(f);
            }
            CardCollectionReferenceNode::Exiled
        }
        Some("those") => {
            if let Err(f) = trial.consume_phrase(&["those", "cards"]) {
                return CollectionOperandAttempt::RejectedCandidate(f);
            }
            CardCollectionReferenceNode::Antecedent
        }
        Some("that") => {
            if let Err(f) = trial.consume_phrase(&["that", "card"]) {
                return CollectionOperandAttempt::RejectedCandidate(f);
            }
            CardCollectionReferenceNode::Selected
        }
        Some("each") => {
            if let Err(f) = trial
                .consume("each")
                .and_then(|()| recognize_exiled_with_source(&mut trial))
            {
                return CollectionOperandAttempt::RejectedCandidate(f);
            }
            CardCollectionReferenceNode::Linked
        }
        Some("cards") => {
            if let Err(f) = recognize_exiled_with_source(&mut trial) {
                return CollectionOperandAttempt::RejectedCandidate(f);
            }
            CardCollectionReferenceNode::Linked
        }
        Some("all")
            if trial
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("cards")) =>
        {
            if let Err(f) = trial
                .consume("all")
                .and_then(|()| recognize_exiled_with_source(&mut trial))
            {
                return CollectionOperandAttempt::RejectedCandidate(f);
            }
            CardCollectionReferenceNode::Linked
        }
        // `all revealed cards [not cast this way]`, `all exiled cards`: the
        // universal restatement of `the revealed|exiled cards`.
        Some("all")
            if trial.peek_ahead(1).is_some_and(|raw| {
                raw.eq_ignore_ascii_case("revealed") || raw.eq_ignore_ascii_case("exiled")
            }) && trial
                .peek_ahead(2)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("cards")) =>
        {
            let verb = trial
                .peek_ahead(1)
                .unwrap_or("revealed")
                .to_ascii_lowercase();
            if let Err(f) = trial.consume_phrase(&["all", verb.as_str(), "cards"]) {
                return CollectionOperandAttempt::FailedCommitted(f);
            }
            if verb == "revealed" {
                CardCollectionReferenceNode::Revealed
            } else {
                CardCollectionReferenceNode::Exiled
            }
        }
        Some("all") => {
            // `all other cards revealed this way` is the remainder role again.
            if let Err(f) = trial.consume_phrase(&["all", "other", "cards"]) {
                return CollectionOperandAttempt::RejectedCandidate(f);
            }
            let verb = trial.peek().map(str::to_ascii_lowercase);
            if !matches!(verb.as_deref(), Some("revealed" | "exiled")) {
                return CollectionOperandAttempt::RejectedCandidate(
                    trial.fail("`revealed` or `exiled` after `all other cards`"),
                );
            }
            if let Err(f) = trial
                .consume(verb.as_deref().unwrap_or("revealed"))
                .and_then(|()| trial.consume_phrase(&["this", "way"]))
            {
                return CollectionOperandAttempt::FailedCommitted(f);
            }
            CardCollectionReferenceNode::Remainder
        }
        Some("the") => {
            if let Err(f) = trial.consume("the") {
                return CollectionOperandAttempt::RejectedCandidate(f);
            }
            match trial.peek().map(str::to_ascii_lowercase).as_deref() {
                Some("rest") => {
                    if let Err(f) = trial.consume("rest") {
                        return CollectionOperandAttempt::FailedCommitted(f);
                    }
                    // `the rest of the revealed cards|of the cards|of them`:
                    // the remainder with its whole collection restated.
                    if trial
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
                        && let Err(f) = recognize_restated_remainder_basis(&mut trial)
                    {
                        return CollectionOperandAttempt::FailedCommitted(f);
                    }
                    CardCollectionReferenceNode::Remainder
                }
                Some("other") => {
                    if let Err(f) = trial.consume("other") {
                        return CollectionOperandAttempt::FailedCommitted(f);
                    }
                    CardCollectionReferenceNode::Other
                }
                Some("cards")
                    if trial
                        .peek_ahead(1)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("exiled")) =>
                {
                    if let Err(f) = recognize_exiled_with_source(&mut trial) {
                        return CollectionOperandAttempt::RejectedCandidate(f);
                    }
                    CardCollectionReferenceNode::Linked
                }
                Some("cards") => {
                    if let Err(f) = trial.consume("cards") {
                        return CollectionOperandAttempt::RejectedCandidate(f);
                    }
                    CardCollectionReferenceNode::Antecedent
                }
                Some("revealed")
                    if trial.peek_ahead(1).is_some_and(|raw| {
                        raw.eq_ignore_ascii_case("card") || raw.eq_ignore_ascii_case("cards")
                    }) =>
                {
                    let noun = trial.peek_ahead(1).unwrap_or("cards").to_owned();
                    if let Err(f) = trial
                        .consume("revealed")
                        .and_then(|()| trial.consume(&noun))
                    {
                        return CollectionOperandAttempt::FailedCommitted(f);
                    }
                    CardCollectionReferenceNode::Revealed
                }
                Some("exiled") => {
                    let noun = trial.peek_ahead(1).map(str::to_ascii_lowercase);
                    if !matches!(noun.as_deref(), Some("card" | "cards")) {
                        return CollectionOperandAttempt::RejectedCandidate(
                            trial.fail("`card` or `cards` after `the exiled`"),
                        );
                    }
                    if let Err(f) = trial
                        .consume("exiled")
                        .and_then(|()| trial.consume(noun.as_deref().unwrap_or("card")))
                    {
                        return CollectionOperandAttempt::FailedCommitted(f);
                    }
                    CardCollectionReferenceNode::Exiled
                }
                // `the chosen card(s)`: the cards a choice or a target
                // declaration of this program bound, as `that card` / `those
                // cards` name them; the lowerer proves the binding.
                Some("chosen") => {
                    let noun = trial.peek_ahead(1).map(str::to_ascii_lowercase);
                    let node = match noun.as_deref() {
                        Some("card") => CardCollectionReferenceNode::Selected,
                        Some("cards") => CardCollectionReferenceNode::Antecedent,
                        _ => {
                            return CollectionOperandAttempt::RejectedCandidate(
                                trial.fail("`card` or `cards` after `the chosen`"),
                            );
                        }
                    };
                    if let Err(f) = trial
                        .consume("chosen")
                        .and_then(|()| trial.consume(noun.as_deref().unwrap_or("card")))
                    {
                        return CollectionOperandAttempt::FailedCommitted(f);
                    }
                    node
                }
                _ => {
                    return CollectionOperandAttempt::RejectedCandidate(
                        trial.fail("rest, other or cards after the collection article"),
                    );
                }
            }
        }
        _ => return CollectionOperandAttempt::NotClaimed,
    };
    trial.enter("collection.reference.complete");
    let end_position = trial.position();
    *cursor = trial;
    CollectionOperandAttempt::Recognized(RecognizedEffectFragment { node, end_position })
}

#[cfg(test)]
#[path = "card_collection_operand_tests.rs"]
mod tests;
