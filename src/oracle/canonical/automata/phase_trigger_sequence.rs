use super::super::{MachineResult, OracleTokenCursor};
use super::{EffectSequenceNode, StepEventNode, recognize_step_event};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PhaseTriggerEnvelopeNode {
    pub event: StepEventNode,
    pub optional: bool,
    pub body_start: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PhaseTriggerSequenceNode {
    pub envelope: PhaseTriggerEnvelopeNode,
    pub effects: EffectSequenceNode,
}

pub(in crate::oracle::canonical) fn recognize_phase_trigger_envelope(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PhaseTriggerEnvelopeNode> {
    cursor.enter("trigger.phase.at");
    cursor.consume("at")?;
    cursor.enter("trigger.phase.article");
    cursor.consume("the")?;
    cursor.enter("trigger.phase.beginning");
    let marker = cursor.consume_type("temporalMarkerWord")?;
    cursor.require(
        marker.raw.eq_ignore_ascii_case("beginning"),
        "the `beginning` temporal marker",
    )?;
    cursor.enter("trigger.phase.relation");
    cursor.consume("of")?;
    cursor.enter("trigger.phase.event");
    let event = recognize_step_event(cursor)?;
    cursor.enter("trigger.phase.separator");
    cursor.consume(",")?;
    cursor.enter("trigger.phase.body");
    cursor.enter("trigger.phase.optionality");
    let optional = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("may"));
    if optional {
        cursor.consume_phrase(&["you", "may"])?;
    }
    let body_start = cursor.position();
    Ok(PhaseTriggerEnvelopeNode {
        event,
        optional,
        body_start,
    })
}
