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

/// `. that|the NOUN can't` after a destruction: a nominal anaphor of the
/// destroyed object (the selector check follows once it is read).
fn nominal_amendment_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("that") || raw.eq_ignore_ascii_case("the"))
        && cursor.peek_ahead(2).is_some_and(|raw| {
            crate::oracle::characteristic_vocabulary::lookup_card_type(
                raw,
                AcceptedForms::SingularOnly,
            )
            .is_some()
        })
        && cursor.peek_ahead(3).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("can't") || raw.eq_ignore_ascii_case("can’t")
        })
}

fn amendment_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek() == Some(".")
        && (nominal_amendment_starts(cursor)
            || (cursor.peek_ahead(1).is_some_and(|raw| {
                raw.eq_ignore_ascii_case("it") || raw.eq_ignore_ascii_case("they")
            }) && cursor.peek_ahead(2).is_some_and(|raw| {
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

/// The singular card type a destroyed scalar target is guaranteed to have,
/// named by a following nominal anaphor (`That creature`, `The land`).
fn recognize_destroyed_nominal(
    cursor: &mut OracleTokenCursor<'_>,
    zone_change: &ZoneChangeEffectNode,
) -> MachineResult<super::ObjectCardType> {
    let determiner = cursor.peek().unwrap_or_default().to_string();
    cursor.consume(&determiner)?;
    let predicate = recognize_object_predicate_atom(cursor, AcceptedForms::SingularOnly)?;
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
    Ok(object_type)
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

/// `. Creatures destroyed this way can't be regenerated` after a destroyed
/// collection: the restated collection names the destroyed objects (CR
/// 608.2c, 701.19c). Its noun must be the collection's own query, or
/// `permanents`, so the amendment never reaches objects the collection did
/// not destroy. Returns false without consuming when it does not start here.
pub(in crate::oracle::canonical) fn recognize_restated_collection_regeneration_amendment(
    cursor: &mut OracleTokenCursor<'_>,
    objects: &super::ObjectQueryNode,
) -> MachineResult<bool> {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if !(cursor.peek() == Some(".")
        && word(2, "destroyed")
        && word(3, "this")
        && word(4, "way")
        && cursor.peek_ahead(5).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("can't") || raw.eq_ignore_ascii_case("can’t")
        }))
    {
        return Ok(false);
    }
    let mut trial = cursor.clone();
    trial.enter("sequence.collection-regeneration.restated");
    trial.consume(".")?;
    let Ok(noun) = recognize_object_predicate_atom(&mut trial, AcceptedForms::SingularOrPlural)
    else {
        return Ok(false);
    };
    if trial
        .peek()
        .is_none_or(|raw| !raw.eq_ignore_ascii_case("destroyed"))
    {
        return Ok(false);
    }
    *cursor = trial;
    cursor.require(
        noun == objects.predicate
            || noun == ObjectPredicate::ObjectKind(super::ObjectKind::Permanent),
        "the destroyed collection's own noun or `permanents`",
    )?;
    cursor.consume_phrase(&["destroyed", "this", "way"])?;
    let modal = cursor.peek().unwrap_or_default().to_string();
    cursor.consume(&modal)?;
    cursor.enter("sequence.collection-regeneration.regenerated");
    cursor.consume_phrase(&["be", "regenerated"])?;
    Ok(true)
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
    // The shared verb operand may yield a reference or collection action;
    // its reader already consumed that action's own regeneration amendment.
    let LocalEffectNode::ZoneChange(mut zone_change) = parsed else {
        cursor.enter("sequence.destroy-regeneration.untargeted-operand");
        cursor.require(
            support == ZoneChangeTargetSupport::TargetCollection,
            "an atomic targeted zone change",
        )?;
        return Ok(parsed);
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
                } else if cursor.peek().is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("that") || raw.eq_ignore_ascii_case("the")
                }) {
                    // `That creature can't be regenerated.` (CR 701.19c):
                    // the destroyed target named by its guaranteed type.
                    referenced_type = Some(recognize_destroyed_nominal(cursor, &zone_change)?);
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

/// The number a sentence-level regeneration rider's subject states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RiderNumber {
    Singular,
    Plural,
}

/// The subject of `SUBJECT can't be regenerated.` as a rider of an earlier
/// destruction, with the width of its words:
///
/// ```text
/// RiderSubject := "it" | "they"
///               | ("that" | "the") NOUN                     scalar anaphor
///               | "a" NOUN "destroyed" "this" "way"          restated
///               | NOUNS "destroyed" "this" "way"             restated set
/// ```
fn rider_subject_at(cursor: &OracleTokenCursor<'_>) -> Option<(RiderNumber, usize)> {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let singular_type = |offset: usize| {
        cursor.peek_ahead(offset).is_some_and(|raw| {
            crate::oracle::characteristic_vocabulary::lookup_card_type(
                raw,
                AcceptedForms::SingularOnly,
            )
            .is_some()
        })
    };
    let restated = |offset: usize| {
        word(offset, "destroyed") && word(offset + 1, "this") && word(offset + 2, "way")
    };
    if word(0, "it") {
        return Some((RiderNumber::Singular, 1));
    }
    if word(0, "they") {
        return Some((RiderNumber::Plural, 1));
    }
    if (word(0, "that") || word(0, "the")) && singular_type(1) {
        return Some((RiderNumber::Singular, 2));
    }
    if (word(0, "a") || word(0, "an")) && singular_type(1) && restated(2) {
        return Some((RiderNumber::Singular, 5));
    }
    let plural_type = cursor.peek().is_some_and(|raw| {
        crate::oracle::characteristic_vocabulary::lookup_card_type(
            raw,
            AcceptedForms::SingularOrPlural,
        )
        .is_some()
            && crate::oracle::characteristic_vocabulary::lookup_card_type(
                raw,
                AcceptedForms::SingularOnly,
            )
            .is_none()
    }) || word(0, "permanents");
    (plural_type && restated(1)).then_some((RiderNumber::Plural, 4))
}

/// `SUBJECT can't be regenerated.` at the start of a sentence, with no
/// duration: a rider of the destruction the previous sentence performed
/// (CR 701.19c) that the destroy reader could not read as its own
/// amendment (`Destroy up to X target creatures, where X is ... They can't
/// be regenerated.`, `destroy that creature. The creature can't be
/// regenerated.`). With a duration it is a lasting rule
/// (`object_rule_clause.rs`), never a rider.
pub(in crate::oracle::canonical) fn regeneration_rider_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let Some((_, width)) = rider_subject_at(cursor) else {
        return false;
    };
    cursor
        .peek_ahead(width)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("can't") || raw.eq_ignore_ascii_case("can’t"))
        && cursor
            .peek_ahead(width + 1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("be"))
        && cursor
            .peek_ahead(width + 2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("regenerated"))
        && cursor.peek_ahead(width + 3).is_none_or(|raw| raw == ".")
}

/// The number of objects a destruction destroys, when it is a destroy
/// action this rider may amend.
fn destroyed_number(effect: &LocalEffectNode) -> Option<RiderNumber> {
    match effect {
        LocalEffectNode::ZoneChange(change) if change.action == ZoneChangeAction::Destroy => {
            // `up to X target creatures`: a typed count is plural whatever
            // its literal envelope.
            let scalar = change.target.slots.len() == 1
                && change.target.slots[0].cardinality.maximum == 1
                && change.target.slots[0]
                    .count
                    .as_ref()
                    .is_none_or(|count| !count.is_plural());
            Some(if scalar {
                RiderNumber::Singular
            } else {
                RiderNumber::Plural
            })
        }
        LocalEffectNode::DestroyCollection(_) => Some(RiderNumber::Plural),
        LocalEffectNode::ReferencedPermanentAction(action)
            if action.action == super::ReferencedPermanentAction::Destroy =>
        {
            Some(RiderNumber::Singular)
        }
        _ => None,
    }
}

/// Reads the rider and prohibits regeneration on the destruction it
/// amends. The rider's number must agree with the destroyed objects; a
/// rider after anything but a destroy action fails here. The enclosing
/// sequence consumes the final period.
pub(in crate::oracle::canonical) fn recognize_regeneration_rider(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<&mut LocalEffectNode>,
) -> MachineResult<()> {
    cursor.enter("sequence.regeneration-rider.antecedent");
    let (number, width) = rider_subject_at(cursor)
        .ok_or_else(|| cursor.fail("a pronoun or a restated noun of the destroyed objects"))?;
    let destroyed =
        antecedent.ok_or_else(|| cursor.fail("a destroy action in the preceding sentence"))?;
    let destroyed_count = destroyed_number(destroyed)
        .ok_or_else(|| cursor.fail("a destroy action in the preceding sentence"))?;
    cursor.enter("sequence.regeneration-rider.agreement");
    cursor.require(
        number == destroyed_count,
        "a rider subject agreeing in number with the destroyed objects",
    )?;
    // `That creature` / `A creature destroyed this way`: a declared scalar
    // target must guarantee the referenced card type.
    let noun_offset = match width {
        2 | 5 => Some(1),
        _ => None,
    };
    let mut referenced_type = None;
    if let Some(offset) = noun_offset {
        let mut noun = cursor.clone();
        noun.advance_to(noun.position() + offset)?;
        let predicate = recognize_object_predicate_atom(&mut noun, AcceptedForms::SingularOnly)?;
        let ObjectPredicate::CardType(object_type) = predicate else {
            return Err(noun.fail("a singular registered card type"));
        };
        if let LocalEffectNode::ZoneChange(change) = &*destroyed {
            noun.require(
                change.target.slots[0]
                    .selector
                    .guarantees_card_type(object_type),
                "a preceding scalar target selector guaranteed to have the referenced card type",
            )?;
        }
        referenced_type = Some(object_type);
    }
    cursor.advance_to(cursor.position() + width)?;
    cursor.enter("sequence.regeneration-rider.negated-modal");
    let modal = cursor.peek().unwrap_or_default().to_string();
    cursor.consume(&modal)?;
    cursor.enter("sequence.regeneration-rider.regenerated");
    cursor.consume_phrase(&["be", "regenerated"])?;
    match destroyed {
        LocalEffectNode::ZoneChange(change) => {
            change.regeneration = ZoneChangeRegenerationPolicy::Prohibited { referenced_type };
        }
        LocalEffectNode::DestroyCollection(collection) => collection.cannot_regenerate = true,
        LocalEffectNode::ReferencedPermanentAction(action) => action.cannot_regenerate = true,
        _ => return Err(cursor.fail("a destroy action in the preceding sentence")),
    }
    Ok(())
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

#[cfg(test)]
#[path = "regeneration_rider_tests.rs"]
mod rider_tests;
