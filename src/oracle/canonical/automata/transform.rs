use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectRecognitionContext, PermanentOperandNode, TransformPermanentEffectNode,
    chosen_permanent_operand_starts, recognize_chosen_permanent_operand,
    recognize_contextual_permanent_operand, recognize_source_reference_with_antecedent,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransformState {
    Action,
    Object,
    Complete,
}

/// Recognizes the atomic transform action for the current source object.
///
/// Double-faced eligibility and the CR 701.27g once-per-stack guard belong to
/// runtime validation. This machine only resolves the grammatical source
/// reference and never infers eligibility from Oracle prose. A pronoun
/// (`untap Cecil and transform it`) names the source only when the one
/// binding environment proves the source is its antecedent; a pronoun whose
/// antecedent is another object keeps the shared reader's distinct
/// diagnostic (`reference.source-or-anaphor.object-antecedent`).
pub(in crate::oracle::canonical) fn recognize_transform_source_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<TransformPermanentEffectNode> {
    let mut state = TransformState::Action;
    let mut permanent = None;
    loop {
        state = match state {
            TransformState::Action => {
                cursor.enter("effect.transform.action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    action.raw.eq_ignore_ascii_case("transform"),
                    "the `transform` action",
                )?;
                TransformState::Object
            }
            TransformState::Object => {
                cursor.enter("effect.transform.object");
                permanent = Some(recognize_source_reference_with_antecedent(
                    cursor,
                    name_links,
                    name_failure,
                    None,
                )?);
                TransformState::Complete
            }
            TransformState::Complete => {
                cursor.enter("effect.transform.complete");
                return Ok(TransformPermanentEffectNode {
                    permanent: PermanentOperandNode::Reference(
                        cursor.require_value(permanent, "a populated transform source")?,
                    ),
                });
            }
        };
    }
}

/// The determiner after `transform` heads a permanent operand other than
/// the source or a pronoun: a target declaration, a universal collection,
/// a resolution choice or a demonstrative (`transform target Incubator
/// token you control`, `transform all Humans`, `transform an Incubator token
/// you control`). Lexical lookahead only.
fn operand_determiner_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    let after = |offset: usize, word: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    };
    let mut operand = cursor.clone();
    if operand.advance_to(operand.position() + 1).is_err() {
        return false;
    }
    ["target", "up", "all", "each", "another", "that", "those"]
        .iter()
        .any(|word| after(1, word))
        || operand
            .peek_token()
            .is_some_and(|token| token.token_type == "quantity")
        || chosen_permanent_operand_starts(&operand)
}

/// `transform OBJECT` (CR 701.28a): the source reference keeps its own
/// reader and diagnostics; any other determiner heads the shared permanent
/// operand, or a permanent chosen while the action resolves. Eligibility
/// (a double-faced permanent, CR 701.28c) stays a runtime check.
pub(in crate::oracle::canonical) fn recognize_transform_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<TransformPermanentEffectNode> {
    if !operand_determiner_follows(cursor) {
        return recognize_transform_source_effect(cursor, name_links, name_failure);
    }
    cursor.enter("effect.transform.action");
    let action = cursor.consume_type("actionWord")?;
    cursor.require(
        action.raw.eq_ignore_ascii_case("transform"),
        "the `transform` action",
    )?;
    cursor.enter("effect.transform.operand");
    let permanent = if chosen_permanent_operand_starts(cursor) {
        PermanentOperandNode::Chosen(recognize_chosen_permanent_operand(cursor)?)
    } else {
        recognize_contextual_permanent_operand(cursor, name_links, name_failure, context)?
    };
    cursor.enter("effect.transform.complete");
    Ok(TransformPermanentEffectNode { permanent })
}

#[cfg(test)]
#[path = "transform_tests.rs"]
mod tests;
