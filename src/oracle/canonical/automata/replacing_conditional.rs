//! A replacing conditional sentence (CR 608.2c): `If COND, BODY instead.` or
//! `If COND, instead BODY.` replaces the instruction written in the sentence
//! just before it. The condition is checked once, where the replaced
//! sentence begins; when it holds only BODY is performed, otherwise only the
//! replaced sentence is.
//!
//! ```text
//! Replacing := Replaced "." "If" COND "," ["instead"] BODY ["instead"] "."
//!            | Replaced "." BODY "instead" "if" COND "."
//! ```
//!
//! The second form writes the same condition after its body (`That creature
//! gets -13/-13 until end of turn instead if a creature died this turn.`);
//! it is located only after the ordinary reading of the program failed
//! (`postposed_replacing_sentence`).
//!
//! The shared effect sequence reads the replaced sentence and the `If`
//! sentence together, in one binding environment: BODY names the replaced
//! sentence's targets and results (`that creature gets -6/-6 instead`). This
//! parent owns only where the two sentences are and the replacement form of
//! the conditional the sequence produced: its other branch is the written
//! range before its `then` branch (`SentenceConditionalNode::replaced_first`).
//! A sentence it cannot prove (no replaced sentence, another conditional over
//! it, a condition that reads the replaced instruction's result) fails
//! explicitly; it is never read as an ordinary, additive conditional.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectRecognitionContext, EffectSequenceNode, condition_any, library_look_reminder_starts_at,
    recognize_effect_sequence_with_context,
};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

/// Where the written `instead` of a replacing sentence is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum InsteadPosition {
    /// `If COND, instead BODY.`: read by the sentence condition child when
    /// its parent enables replacing conditionals.
    Medial,
    /// `If COND, BODY instead.`: the absolute index of the `instead` token,
    /// which directly precedes the sentence's period.
    Postposed(usize),
    /// `BODY instead if COND.`: the absolute indices of the `instead` token
    /// directly before the postposed `if`, and of the sentence's period.
    PostposedCondition { instead: usize, period: usize },
}

/// The lexical location of a replacing sentence and of the sentence it
/// replaces (absolute token indices).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ReplacingSentence {
    /// The first token of the replaced sentence.
    pub replaced_start: usize,
    /// The `if` that opens the replacing sentence.
    pub start: usize,
    /// End (exclusive) of the replacing sentence, its period and directly
    /// following reminders included.
    pub end: usize,
    pub instead: InsteadPosition,
}

/// One top-level sentence: its first token, its period and its end
/// (exclusive) including directly following parenthesized reminders.
#[derive(Clone, Copy, Debug)]
struct WrittenSentence {
    start: usize,
    period: usize,
    end: usize,
}

fn word_at(cursor: &OracleTokenCursor<'_>, index: usize, expected: &str) -> bool {
    cursor
        .token_slice(index, index + 1)
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
}

/// The top-level sentences from the cursor to the end of the input. Quoted
/// abilities and parenthesized text are nested scopes: their periods do not
/// end a sentence of this program (CR 113.1, 207.2).
fn written_sentences(cursor: &OracleTokenCursor<'_>) -> Vec<WrittenSentence> {
    let tokens = cursor.token_slice(cursor.position(), usize::MAX);
    let base = cursor.position();
    let mut sentences = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut quoted = false;
    let mut index = 0;
    while index < tokens.len() {
        match tokens[index].raw.as_str() {
            "\"" => quoted = !quoted,
            "(" if !quoted => depth += 1,
            ")" if !quoted => depth = depth.saturating_sub(1),
            "." if !quoted && depth == 0 => {
                let period = index;
                let mut end = index + 1;
                // A reminder directly after the period belongs to it.
                while tokens.get(end).is_some_and(|token| token.raw == "(") {
                    let mut nested = 0usize;
                    let mut cursor_index = end;
                    while let Some(token) = tokens.get(cursor_index) {
                        match token.raw.as_str() {
                            "(" => nested += 1,
                            ")" => {
                                nested = nested.saturating_sub(1);
                                if nested == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        cursor_index += 1;
                    }
                    if cursor_index >= tokens.len() {
                        break;
                    }
                    end = cursor_index + 1;
                }
                sentences.push(WrittenSentence {
                    start: base + start,
                    period: base + period,
                    end: base + end,
                });
                start = end;
                index = end;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    sentences
}

/// The first top-level comma of the sentence after its `if`.
fn condition_separator(cursor: &OracleTokenCursor<'_>, sentence: WrittenSentence) -> Option<usize> {
    let mut depth = 0usize;
    let mut quoted = false;
    (sentence.start + 1..sentence.period).find(|index| {
        let raw = cursor
            .token_slice(*index, index + 1)
            .first()
            .map_or("", |token| token.raw.as_str());
        match raw {
            "\"" => quoted = !quoted,
            "(" if !quoted => depth += 1,
            ")" if !quoted => depth = depth.saturating_sub(1),
            "," if !quoted && depth == 0 => return true,
            _ => {}
        }
        false
    })
}

/// The first later top-level sentence that replaces the sentence before it
/// with `instead` (lexical lookahead only; the sequence proves it). A
/// sentence that creates a replacement effect (`If OBJECT would ..., ...
/// instead`, CR 614.1a) and `instead of` (an amended destination) are other
/// constructs and are not claimed here.
pub(in crate::oracle::canonical) fn replacing_sentence(
    cursor: &OracleTokenCursor<'_>,
) -> Option<ReplacingSentence> {
    let sentences = written_sentences(cursor);
    sentences.windows(2).find_map(|pair| {
        let (replaced, sentence) = (pair[0], pair[1]);
        if !word_at(cursor, sentence.start, "if") {
            return None;
        }
        let separator = condition_separator(cursor, sentence)?;
        if (sentence.start..separator).any(|index| word_at(cursor, index, "would")) {
            return None;
        }
        let medial =
            word_at(cursor, separator + 1, "instead") && !word_at(cursor, separator + 2, "of");
        let postposed =
            sentence.period > separator + 1 && word_at(cursor, sentence.period - 1, "instead");
        let instead = match (medial, postposed) {
            (true, false) => InsteadPosition::Medial,
            (false, true) => InsteadPosition::Postposed(sentence.period - 1),
            _ => return None,
        };
        Some(ReplacingSentence {
            replaced_start: replaced.start,
            start: sentence.start,
            end: sentence.end,
            instead,
        })
    })
}

/// `BODY instead if COND.` (CR 608.2c): the first later top-level sentence
/// that ends with a postposed condition introduced by `instead if`, which
/// replaces the sentence before it. A condition that creates a replacement
/// effect (`instead if it would ...`, CR 614.1a) is another construct. The
/// program parent locates it only after its ordinary reading failed.
pub(in crate::oracle::canonical) fn postposed_replacing_sentence(
    cursor: &OracleTokenCursor<'_>,
) -> Option<ReplacingSentence> {
    let sentences = written_sentences(cursor);
    sentences.windows(2).find_map(|pair| {
        let (replaced, sentence) = (pair[0], pair[1]);
        if word_at(cursor, sentence.start, "if") {
            return None;
        }
        let mut depth = 0usize;
        let mut quoted = false;
        let instead = (sentence.start + 1..sentence.period).find(|index| {
            let raw = cursor
                .token_slice(*index, index + 1)
                .first()
                .map_or("", |token| token.raw.as_str());
            match raw {
                "\"" => quoted = !quoted,
                "(" if !quoted => depth += 1,
                ")" if !quoted => depth = depth.saturating_sub(1),
                _ => {}
            }
            !quoted
                && depth == 0
                && word_at(cursor, *index, "instead")
                && word_at(cursor, index + 1, "if")
        })?;
        if (instead + 2..sentence.period).any(|index| word_at(cursor, index, "would")) {
            return None;
        }
        Some(ReplacingSentence {
            replaced_start: replaced.start,
            start: sentence.start,
            end: sentence.end,
            instead: InsteadPosition::PostposedCondition {
                instead,
                period: sentence.period,
            },
        })
    })
}

/// The actions the shared sequence reads before `end` from the cursor: a
/// prefix of the whole reading, which must agree with it.
fn prefix_actions(
    cursor: &OracleTokenCursor<'_>,
    end: usize,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<Vec<super::LocalEffectNode>> {
    if end == cursor.position() {
        return Ok(Vec::new());
    }
    let mut trial = cursor.clone();
    trial
        .bounded(end, |trial| {
            recognize_effect_sequence_with_context(trial, name_links, name_failure, context)
        })
        .map(|sequence| sequence.effects)
}

/// Reads the program from the cursor through the replacing sentence as one
/// shared sequence, then gives its last conditional the replacement form.
pub(in crate::oracle::canonical) fn recognize_replacing_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    sentence: ReplacingSentence,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectSequenceNode> {
    cursor.enter("program.replacing.sentence");
    let mut replacing = context.clone();
    replacing.replacing_conditionals = true;
    let before_replaced = prefix_actions(
        cursor,
        sentence.replaced_start,
        name_links,
        name_failure,
        &replacing,
    )?;
    let before_condition =
        prefix_actions(cursor, sentence.start, name_links, name_failure, &replacing)?;
    let mut sequence = match sentence.instead {
        InsteadPosition::Medial => cursor.bounded(sentence.end, |cursor| {
            recognize_effect_sequence_with_context(cursor, name_links, name_failure, &replacing)
        })?,
        InsteadPosition::Postposed(instead) => {
            // The written `instead` closes the replacing sentence: the
            // sequence reads a view whose `instead` slot holds the period,
            // so every earlier token keeps its index, span and name link.
            let mut tokens = cursor.token_slice(0, usize::MAX).to_vec();
            let period = tokens
                .get(instead + 1)
                .filter(|token| token.raw == ".")
                .cloned()
                .ok_or_else(|| cursor.fail("a period after the postposed `instead`"))?;
            tokens[instead] = period;
            let mut view = cursor.with_tokens(&tokens);
            let sequence = view.bounded(instead + 1, |view| {
                recognize_effect_sequence_with_context(view, name_links, name_failure, &replacing)
            })?;
            cursor.advance_to(instead + 2)?;
            if cursor.position() < sentence.end {
                // CR 207.2: a reminder of the replacing sentence. A
                // library-look reminder is validated by its action's
                // sentence only, which this reading does not reach.
                cursor.enter("program.replacing.reminder");
                cursor.require(
                    !library_look_reminder_starts_at(cursor),
                    "a reminder the replacing sentence's own reader validates",
                )?;
                cursor.advance_to(sentence.end)?;
            }
            sequence
        }
        InsteadPosition::PostposedCondition { instead, period } => {
            // The body is read up to `instead`, whose slot holds the
            // sentence's period in the view, so every earlier token keeps
            // its index, span and name link.
            let mut tokens = cursor.token_slice(0, usize::MAX).to_vec();
            let period_token = tokens
                .get(period)
                .filter(|token| token.raw == ".")
                .cloned()
                .ok_or_else(|| cursor.fail("a period after the postposed condition"))?;
            tokens[instead] = period_token;
            let mut view = cursor.with_tokens(&tokens);
            let mut sequence = view.bounded(instead + 1, |view| {
                recognize_effect_sequence_with_context(view, name_links, name_failure, &replacing)
            })?;
            cursor.enter("program.replacing.postposed-body");
            cursor.require(
                before_condition.len() < sequence.effects.len()
                    && sequence
                        .conditionals
                        .iter()
                        .all(|conditional| conditional.end() <= before_condition.len()),
                "an unconditional instruction before the postposed `instead if`",
            )?;
            // The condition reads the antecedents of the body it gates: the
            // permanent subject after the body's actions and the program's
            // published antecedents; departures are those before the
            // replaced sentence, where the condition is checked.
            cursor.enter("program.replacing.postposed-condition");
            let mut condition_effects = replacing.clone();
            condition_effects.permanent_subject_scope = sequence.effects.iter().enumerate().fold(
                replacing.permanent_subject_scope,
                |scope, (index, effect)| {
                    super::permanent_subject::permanent_subject_after_effect(effect, index, scope)
                },
            );
            let departed = before_replaced
                .iter()
                .enumerate()
                .filter_map(|(effect_index, effect)| {
                    matches!(effect, super::LocalEffectNode::ZoneChange(_)).then_some(
                        super::TargetBinding {
                            effect_index,
                            slot_index: 0,
                        },
                    )
                })
                .collect::<Vec<_>>();
            let environment = super::publish_program_antecedents(
                super::installed_binding_environment(cursor),
                &sequence.effects,
            );
            let enclosing =
                cursor.replace_binding_environment(Some(std::sync::Arc::new(environment)));
            let condition = cursor.advance_to(instead + 1).and_then(|()| {
                cursor.bounded(period, |cursor| {
                    super::recognize_postposed_sentence_condition(
                        cursor,
                        &super::SentenceConditionContext {
                            name_links,
                            name_failure,
                            effects: &condition_effects,
                            card_target: None,
                            card_focus: false,
                            departed_targets: &departed,
                        },
                    )
                })
            });
            cursor.replace_binding_environment(enclosing);
            let condition = condition?;
            cursor.consume(".")?;
            if cursor.position() < sentence.end {
                // CR 207.2: a reminder of the replacing sentence.
                cursor.enter("program.replacing.reminder");
                cursor.require(
                    !library_look_reminder_starts_at(cursor),
                    "a reminder the replacing sentence's own reader validates",
                )?;
                cursor.advance_to(sentence.end)?;
            }
            sequence.conditionals.push(super::SentenceConditionalNode {
                condition,
                then_first: before_condition.len(),
                then_end: sequence.effects.len(),
                else_end: None,
            });
            sequence
        }
    };
    cursor.enter("program.replacing.prefix");
    let replaced_first = before_replaced.len();
    let then_first = before_condition.len();
    cursor.require(
        sequence.effects.get(..replaced_first) == Some(before_replaced.as_slice())
            && sequence.effects.get(..then_first) == Some(before_condition.as_slice()),
        "the same reading of the sentences before the replacing conditional",
    )?;
    cursor.enter("program.replacing.replaced");
    cursor.require(
        replaced_first < then_first,
        "an instruction written in the sentence before `instead`",
    )?;
    cursor.enter("program.replacing.conditional");
    let index = cursor.require_value(
        sequence.conditionals.len().checked_sub(1),
        "the conditional the `If` sentence opens",
    )?;
    let conditional = &sequence.conditionals[index];
    cursor.require(
        conditional.then_first == then_first
            && conditional.then_end == sequence.effects.len()
            && conditional.else_end.is_none(),
        "the conditional the `If` sentence opens, which `instead` amends",
    )?;
    cursor.enter("program.replacing.scope");
    cursor.require(
        sequence.conditionals[..index]
            .iter()
            .all(|earlier| earlier.end() <= replaced_first),
        "a replaced sentence outside every other conditional sentence",
    )?;
    cursor.require(
        sequence
            .unless
            .iter()
            .all(|frame| frame.end_effect <= replaced_first)
            && sequence
                .delayed
                .iter()
                .all(|frame| frame.end_effect <= replaced_first)
            && sequence.where_x.is_empty(),
        "replaced and replacing instructions without an `unless`, delayed or `where X` frame",
    )?;
    cursor.enter("program.replacing.condition");
    cursor.require(
        !condition_any(&conditional.condition, &|atom: &super::ConditionAtom| {
            atom.reads_previous_action() || atom.reads_action_result()
        }),
        "a condition checked before the replaced instruction, not one that reads its result",
    )?;
    sequence.conditionals[index].else_end = Some(replaced_first);
    Ok(sequence)
}

#[cfg(test)]
#[path = "replacing_conditional_tests.rs"]
mod tests;
