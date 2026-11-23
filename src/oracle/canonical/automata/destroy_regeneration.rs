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
            .is_some_and(|raw| raw.eq_ignore_ascii_case("it") || raw.eq_ignore_ascii_case("they"))
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

/// `. They can't be regenerated` after a destroyed collection: the later
/// sentence amends the earlier destruction (CR 608.2c, 701.19c). Returns
/// false without consuming when the amendment does not start here; once its
/// plural anaphor is read, a malformed remainder is a committed failure. The
/// enclosing sequence keeps the final period.
pub(in crate::oracle::canonical) fn recognize_plural_regeneration_amendment(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<bool> {
    recognize_pronoun_regeneration_amendment(cursor, "they")
}

/// `. It can't be regenerated` after the destruction of one bound object.
pub(in crate::oracle::canonical) fn recognize_scalar_regeneration_amendment(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<bool> {
    recognize_pronoun_regeneration_amendment(cursor, "it")
}

fn recognize_pronoun_regeneration_amendment(
    cursor: &mut OracleTokenCursor<'_>,
    pronoun: &str,
) -> MachineResult<bool> {
    if !(cursor.peek() == Some(".")
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(pronoun))
        && cursor.peek_ahead(2).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("can't") || raw.eq_ignore_ascii_case("can’t")
        }))
    {
        return Ok(false);
    }
    cursor.enter("sequence.collection-regeneration.anaphor");
    cursor.consume(".")?;
    cursor.consume(pronoun)?;
    let modal = cursor.peek().unwrap_or_default().to_string();
    cursor.consume(&modal)?;
    cursor.enter("sequence.collection-regeneration.regenerated");
    cursor.consume_phrase(&["be", "regenerated"])?;
    Ok(true)
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
                } else if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("they"))
                {
                    // CR 701.19c: the prohibition covers every destroyed
                    // member of the declared target collection.
                    cursor.consume("they")?;
                    cursor.require(
                        zone_change.target.slots.len() > 1
                            || zone_change
                                .target
                                .slots
                                .iter()
                                .any(|slot| slot.cardinality.maximum > 1),
                        "a plural destroyed-object antecedent",
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

/// `destroy REFERENCE`: the destruction of one bound permanent reference
/// (the shared contextual operand), with an optional regeneration
/// prohibition (CR 701.19c). Target declarations stay with the targeted
/// zone change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DestroyReferenceEffectNode {
    pub object: super::PermanentEffectReference,
    pub cannot_regenerate: bool,
    /// The zone action on the bound permanent.
    pub action: ReferencedPermanentAction,
}

/// What happens to one bound permanent reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ReferencedPermanentAction {
    /// CR 701.8.
    Destroy,
    /// CR 701.13.
    Exile,
    /// CR 701.21: only its controller sacrifices it.
    Sacrifice,
}

/// `exile|sacrifice REFERENCE` on a permanent the program bound other than
/// the ability's source (`exile it` after a card was returned to the
/// battlefield, `sacrifice that creature`). Source references keep their
/// own readers; anything else is not claimed.
pub(in crate::oracle::canonical) fn recognize_referenced_permanent_zone_action(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
    context: &super::EffectRecognitionContext,
) -> Option<MachineResult<DestroyReferenceEffectNode>> {
    let action = if super::lemma_at(cursor, super::VerbLemma::Exile, super::VerbAgreement::Base) {
        ReferencedPermanentAction::Exile
    } else if super::lemma_at(
        cursor,
        super::VerbLemma::Sacrifice,
        super::VerbAgreement::Base,
    ) {
        ReferencedPermanentAction::Sacrifice
    } else {
        return None;
    };
    let mut trial = cursor.clone();
    trial.enter("effect.referenced-permanent.action");
    trial.advance_to(trial.position() + 1).ok()?;
    trial.enter("effect.referenced-permanent.object");
    let Ok(super::PermanentOperandNode::BoundReference(object)) =
        super::recognize_contextual_permanent_operand(
            &mut trial,
            name_links,
            name_failure,
            context,
        )
    else {
        return None;
    };
    if matches!(object, super::PermanentEffectReference::Source(_)) {
        return None;
    }
    *cursor = trial;
    Some(Ok(DestroyReferenceEffectNode {
        object,
        cannot_regenerate: false,
        action,
    }))
}

pub(in crate::oracle::canonical) fn recognize_destroy_reference_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
    context: &super::EffectRecognitionContext,
) -> MachineResult<DestroyReferenceEffectNode> {
    cursor.enter("effect.destroy-reference.action");
    cursor.consume("destroy")?;
    cursor.enter("effect.destroy-reference.object");
    let operand =
        super::recognize_contextual_permanent_operand(cursor, name_links, name_failure, context)?;
    let super::PermanentOperandNode::BoundReference(object) = operand else {
        return Err(cursor.fail("a bound permanent reference"));
    };
    let cannot_regenerate = recognize_scalar_regeneration_amendment(cursor)?;
    Ok(DestroyReferenceEffectNode {
        object,
        cannot_regenerate,
        action: ReferencedPermanentAction::Destroy,
    })
}

#[cfg(test)]
#[path = "destroy_regeneration_tests.rs"]
mod tests;
