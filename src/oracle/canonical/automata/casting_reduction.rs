use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CastingCostReductionNode, SelectorPredicate, TargetSelector, recognize_target_slot_selector,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CastingReductionState {
    Subject,
    Action,
    GenericAmount,
    Reduction,
    Quantifier,
    Selector,
    Scope,
    Terminator,
    Complete,
}

#[derive(Default)]
struct CastingReductionFrame {
    generic_reduction_per_permanent: Option<i64>,
    selector: Option<SelectorPredicate>,
}

/// Recognizes a generic casting-cost reduction scaled by permanents on all
/// battlefields. The selector child owns the permanent predicate grammar;
/// this parent owns only scaling, quantification, and battlefield scope.
pub(in crate::oracle::canonical) fn recognize_permanent_scaled_casting_reduction(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CastingCostReductionNode> {
    let mut state = CastingReductionState::Subject;
    let mut frame = CastingReductionFrame::default();
    loop {
        state = match state {
            CastingReductionState::Subject => {
                cursor.enter("casting-reduction.subject");
                cursor.consume_phrase(&["this", "spell"])?;
                CastingReductionState::Action
            }
            CastingReductionState::Action => {
                cursor.enter("casting-reduction.action");
                cursor.consume("costs")?;
                CastingReductionState::GenericAmount
            }
            CastingReductionState::GenericAmount => {
                cursor.enter("casting-reduction.generic-amount");
                let symbol = cursor
                    .peek_token()
                    .filter(|token| token.token_type == "manaSymbol")
                    .map(|token| token.raw.as_str())
                    .ok_or_else(|| cursor.fail("a positive generic mana symbol"))?;
                let amount = symbol
                    .strip_prefix('{')
                    .and_then(|raw| raw.strip_suffix('}'))
                    .and_then(|raw| raw.parse::<i64>().ok())
                    .filter(|amount| *amount > 0)
                    .ok_or_else(|| cursor.fail("a positive generic mana symbol"))?;
                cursor.advance_to(cursor.position() + 1)?;
                frame.generic_reduction_per_permanent = Some(amount);
                CastingReductionState::Reduction
            }
            CastingReductionState::Reduction => {
                cursor.enter("casting-reduction.reduction");
                cursor.consume_phrase(&["less", "to", "cast"])?;
                CastingReductionState::Quantifier
            }
            CastingReductionState::Quantifier => {
                cursor.enter("casting-reduction.quantifier");
                cursor.consume_phrase(&["for", "each"])?;
                CastingReductionState::Selector
            }
            CastingReductionState::Selector => {
                cursor.enter("casting-reduction.selector");
                let selector = recognize_target_slot_selector(cursor)?;
                let TargetSelector::Object(predicate) = selector else {
                    return Err(cursor.fail("a permanent selector"));
                };
                cursor.enter("casting-reduction.selector.validation");
                cursor.require(
                    permanent_predicate_supported(&predicate),
                    "a selector whose domain is battlefield permanents",
                )?;
                frame.selector = Some(predicate);
                CastingReductionState::Scope
            }
            CastingReductionState::Scope => {
                cursor.enter("casting-reduction.scope");
                cursor.consume_phrase(&["on", "the", "battlefield"])?;
                CastingReductionState::Terminator
            }
            CastingReductionState::Terminator => {
                cursor.enter("casting-reduction.terminator");
                cursor.consume(".")?;
                CastingReductionState::Complete
            }
            CastingReductionState::Complete => {
                cursor.enter("casting-reduction.complete");
                cursor.finish()?;
                return Ok(CastingCostReductionNode {
                    generic_reduction_per_permanent: cursor.require_value(
                        frame.generic_reduction_per_permanent,
                        "a generic reduction amount",
                    )?,
                    selector: cursor.require_value(frame.selector, "a permanent selector")?,
                });
            }
        };
    }
}

fn permanent_predicate_supported(predicate: &SelectorPredicate) -> bool {
    match predicate {
        SelectorPredicate::CardType(card_type) if card_type.is_permanent_type() => true,
        SelectorPredicate::Intersection(operands) => {
            operands.iter().any(permanent_predicate_supported)
                && operands.iter().all(permanent_filter_component_supported)
        }
        SelectorPredicate::Union(operands) => {
            !operands.is_empty() && operands.iter().all(permanent_predicate_supported)
        }
        SelectorPredicate::IsCommander
        | SelectorPredicate::IsToken
        | SelectorPredicate::CardType(_)
        | SelectorPredicate::Color(_)
        | SelectorPredicate::Subtype(_)
        | SelectorPredicate::CharacteristicBatch(_)
        | SelectorPredicate::Supertype(_)
        | SelectorPredicate::ExactManaCost(_)
        | SelectorPredicate::HasKeyword(_)
        | SelectorPredicate::ObjectKind(_)
        | SelectorPredicate::NumericComparison { .. }
        | SelectorPredicate::Not(_) => false,
    }
}

fn permanent_filter_component_supported(predicate: &SelectorPredicate) -> bool {
    match predicate {
        SelectorPredicate::CardType(card_type) => card_type.is_permanent_type(),
        SelectorPredicate::ObjectKind(_) => false,
        SelectorPredicate::IsCommander
        | SelectorPredicate::IsToken
        | SelectorPredicate::Color(_)
        | SelectorPredicate::Subtype(_)
        | SelectorPredicate::CharacteristicBatch(_)
        | SelectorPredicate::Supertype(_)
        | SelectorPredicate::ExactManaCost(_)
        | SelectorPredicate::HasKeyword(_)
        | SelectorPredicate::NumericComparison { .. } => true,
        SelectorPredicate::Not(operand) => permanent_filter_component_supported(operand),
        SelectorPredicate::Intersection(operands) | SelectorPredicate::Union(operands) => {
            !operands.is_empty() && operands.iter().all(permanent_filter_component_supported)
        }
    }
}
