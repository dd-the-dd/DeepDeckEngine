use super::super::{MachineResult, OracleTokenCursor};
use super::source_sacrifice::atomic_action::{DiscardVerbForm, recognize_discard_action};
use super::{PayLifeCostNode, recognize_mandatory_additional_cost_header, recognize_pay_life_cost};

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

fn recognize_additional_cost_option(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AlternativeAdditionalCostOptionNode> {
    cursor.enter("additional-cost.option.dispatch");
    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("discard") => recognize_discard_card_cost(cursor)
            .map(AlternativeAdditionalCostOptionNode::DiscardCard),
        Some("pay") => {
            recognize_pay_life_cost(cursor).map(AlternativeAdditionalCostOptionNode::PayLife)
        }
        _ => Err(cursor.fail("a supported additional-cost atom")),
    }
}

fn recognize_discard_card_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DiscardCardCostNode> {
    recognize_discard_action(cursor, DiscardVerbForm::Imperative, |cursor| {
        cursor.enter("cost.discard-card.quantity");
        let quantity = cursor
            .peek()
            .ok_or_else(|| cursor.fail("`a` or `one`"))?
            .to_string();
        cursor.require(
            quantity.eq_ignore_ascii_case("a") || quantity.eq_ignore_ascii_case("one"),
            "`a` or `one`",
        )?;
        cursor.consume(&quantity)?;
        cursor.enter("cost.discard-card.card-noun");
        cursor.consume("card")?;
        Ok(())
    })?;
    cursor.enter("cost.discard-card.complete");
    Ok(DiscardCardCostNode)
}
