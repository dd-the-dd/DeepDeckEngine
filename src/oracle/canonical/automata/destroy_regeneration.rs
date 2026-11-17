use crate::oracle::characteristic_vocabulary::AcceptedForms;

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalEffectNode, ObjectPredicate, ZoneChangeAction, ZoneChangeEffectNode,
    ZoneChangeRegenerationPolicy, ZoneChangeTargetSupport, recognize_object_predicate_atom,
    recognize_zone_change_effect_with_support,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AmendmentState {
    DestroyTerminator,
    ObjectAnaphor,
    DestroyedManner,
    NegatedModal,
    Passive,
    Regeneration,
    Complete,
}

fn amendment_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek() == Some(".")
        && ((cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("it"))
            && cursor.peek_ahead(2).is_some_and(|raw| {
                raw.eq_ignore_ascii_case("can't") || raw.eq_ignore_ascii_case("can’t")
            }))
            || (cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("a"))
                && cursor
                    .peek_ahead(3)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("destroyed"))
                && cursor
                    .peek_ahead(4)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
                && cursor
                    .peek_ahead(5)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("way"))
                && cursor.peek_ahead(6).is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("can't") || raw.eq_ignore_ascii_case("can’t")
                })))
}

pub(in crate::oracle::canonical) fn recognize_zone_change_with_regeneration_amendment(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalEffectNode> {
    recognize_zone_change_with_regeneration_amendment_with_support(
        cursor,
        ZoneChangeTargetSupport::Scalar,
    )
}

pub(in crate::oracle::canonical) fn recognize_zone_change_with_regeneration_amendment_with_support(
    cursor: &mut OracleTokenCursor<'_>,
    support: ZoneChangeTargetSupport,
) -> MachineResult<LocalEffectNode> {
    let parsed = recognize_zone_change_effect_with_support(cursor, support)?;
    let LocalEffectNode::ZoneChange(mut zone_change) = parsed else {
        return Err(cursor.fail("an atomic targeted zone change"));
    };
    if zone_change.action != ZoneChangeAction::Destroy || !amendment_starts(cursor) {
        return Ok(LocalEffectNode::ZoneChange(zone_change));
    }

    let mut state = AmendmentState::DestroyTerminator;
    let mut referenced_type = None;
    loop {
        state = match state {
            AmendmentState::DestroyTerminator => {
                cursor.enter("sequence.destroy-regeneration.destroy-terminator");
                cursor.consume(".")?;
                AmendmentState::ObjectAnaphor
            }
            AmendmentState::ObjectAnaphor => {
                cursor.enter("sequence.destroy-regeneration.object-anaphor");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("it"))
                {
                    cursor.consume("it")?;
                    cursor.require(
                        zone_change.target.slots.len() == 1
                            && zone_change.target.slots[0].cardinality.maximum == 1,
                        "one scalar destroyed-object antecedent",
                    )?;
                    AmendmentState::NegatedModal
                } else {
                    cursor.consume("a")?;
                    let predicate =
                        recognize_object_predicate_atom(cursor, AcceptedForms::SingularOnly)?;
                    let ObjectPredicate::CardType(object_type) = predicate else {
                        return Err(cursor.fail("a singular registered card type"));
                    };
                    cursor.require(
                        zone_change.target.slots.len() == 1
                            && zone_change.target.slots[0].cardinality.maximum == 1
                            && zone_change.target.slots[0]
                                .selector
                                .guarantees_card_type(object_type),
                        "a preceding scalar target selector guaranteed to have the referenced card type",
                    )?;
                    referenced_type = Some(object_type);
                    AmendmentState::DestroyedManner
                }
            }
            AmendmentState::DestroyedManner => {
                cursor.enter("sequence.destroy-regeneration.destroyed-manner");
                cursor.consume_phrase(&["destroyed", "this", "way"])?;
                AmendmentState::NegatedModal
            }
            AmendmentState::NegatedModal => {
                cursor.enter("sequence.destroy-regeneration.negated-modal");
                let modal = cursor
                    .peek()
                    .filter(|raw| {
                        raw.eq_ignore_ascii_case("can't") || raw.eq_ignore_ascii_case("can’t")
                    })
                    .ok_or_else(|| cursor.fail("`can't`"))?
                    .to_string();
                cursor.consume(&modal)?;
                AmendmentState::Passive
            }
            AmendmentState::Passive => {
                cursor.enter("sequence.destroy-regeneration.passive");
                cursor.consume("be")?;
                AmendmentState::Regeneration
            }
            AmendmentState::Regeneration => {
                cursor.enter("sequence.destroy-regeneration.regeneration");
                cursor.consume("regenerated")?;
                AmendmentState::Complete
            }
            AmendmentState::Complete => {
                cursor.enter("sequence.destroy-regeneration.complete");
                zone_change.regeneration =
                    ZoneChangeRegenerationPolicy::Prohibited { referenced_type };
                return Ok(LocalEffectNode::ZoneChange(zone_change));
            }
        };
    }
}

pub(in crate::oracle::canonical) fn recognize_destroy_regeneration_prohibition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ZoneChangeEffectNode> {
    let node = recognize_zone_change_with_regeneration_amendment(cursor)?;
    let LocalEffectNode::ZoneChange(node) = node else {
        return Err(cursor.fail("a targeted destroy with a regeneration prohibition"));
    };
    cursor.require(
        matches!(
            node.regeneration,
            ZoneChangeRegenerationPolicy::Prohibited { .. }
        ),
        "a targeted destroy with a regeneration prohibition",
    )?;
    cursor.enter("sequence.destroy-regeneration.amendment-terminator");
    cursor.consume(".")?;
    cursor.finish()?;
    Ok(node)
}

#[cfg(test)]
#[path = "destroy_regeneration_tests.rs"]
mod tests;
