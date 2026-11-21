use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CollectionSelectionMethod, PayLifeCostNode, PaymentActionNode, PaymentObjectsNode,
    ResourcePaymentNode, recognize_mandatory_additional_cost_header, recognize_payment_action,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DiscardCardCostNode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AlternativeAdditionalCostOptionNode {
    DiscardCard(DiscardCardCostNode),
    PayLife(PayLifeCostNode),
}

impl AlternativeAdditionalCostOptionNode {
    pub(in crate::oracle::canonical) fn mode_name(self) -> &'static str {
        match self {
            Self::DiscardCard(_) => "discard",
            Self::PayLife(_) => "payLife",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AlternativeAdditionalCostChoiceNode {
    pub options: Vec<AlternativeAdditionalCostOptionNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EnvelopeState {
    Envelope,
    FirstOption,
    ConnectorOrTerminator,
    NextOption,
    Complete,
}

/// Recognizes a mandatory additional-cost envelope containing a contextual
/// `or` choice. Each branch delegates to an independent typed cost atom.
pub(in crate::oracle::canonical) fn recognize_alternative_additional_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AlternativeAdditionalCostChoiceNode> {
    let mut state = EnvelopeState::Envelope;
    let mut options = Vec::new();
    loop {
        state = match state {
            EnvelopeState::Envelope => {
                cursor.enter("additional-cost.alternative.envelope");
                recognize_mandatory_additional_cost_header(cursor)?;
                EnvelopeState::FirstOption
            }
            EnvelopeState::FirstOption => {
                cursor.enter("additional-cost.alternative.first-option");
                options.push(recognize_additional_cost_option(cursor)?);
                EnvelopeState::ConnectorOrTerminator
            }
            EnvelopeState::ConnectorOrTerminator => {
                cursor.enter("additional-cost.alternative.connector-or-terminator");
                if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("or"))
                {
                    cursor.consume("or")?;
                    EnvelopeState::NextOption
                } else if cursor.peek().is_some_and(|token| token == ".") {
                    cursor.consume(".")?;
                    EnvelopeState::Complete
                } else {
                    return Err(cursor.fail("`or` or the additional-cost terminator"));
                }
            }
            EnvelopeState::NextOption => {
                cursor.enter("additional-cost.alternative.next-option");
                options.push(recognize_additional_cost_option(cursor)?);
                EnvelopeState::ConnectorOrTerminator
            }
            EnvelopeState::Complete => {
                cursor.enter("additional-cost.alternative.complete");
                cursor.finish()?;
                cursor.require(options.len() >= 2, "at least two alternative cost options")?;
                return Ok(AlternativeAdditionalCostChoiceNode { options });
            }
        };
    }
}

/// Each option is one shared payment action, projected onto the casting
/// cost schema this envelope lowers (`discard` of any card or `payLife`).
fn recognize_additional_cost_option(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AlternativeAdditionalCostOptionNode> {
    cursor.enter("additional-cost.option.dispatch");
    let payment = recognize_payment_action(cursor)?;
    cursor.enter("additional-cost.option.schema");
    match payment {
        PaymentActionNode::Pay(ResourcePaymentNode::Life(life)) => {
            Ok(AlternativeAdditionalCostOptionNode::PayLife(life))
        }
        PaymentActionNode::Discard(PaymentObjectsNode::Chosen(chosen))
            if chosen.count == 1
                && !chosen.exclude_source
                && chosen.method == CollectionSelectionMethod::Choice
                && chosen.filter.is_any_card() =>
        {
            Ok(AlternativeAdditionalCostOptionNode::DiscardCard(
                DiscardCardCostNode,
            ))
        }
        _ => Err(cursor.fail(
            "a supported additional-cost option: discard one card of the payer's choice or pay N life",
        )),
    }
}
