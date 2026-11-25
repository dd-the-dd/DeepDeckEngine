use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectRecognitionContext, MoveSourceEffectNode, SourceFaceChange, SourceMoveController,
    SourceZone, recognize_contextual_permanent_operand, recognize_source_reference,
    recognize_source_reference_with_antecedent,
};

/// Lookahead at `put OBJECT from your hand|graveyard`: the moved object names
/// the card zone it leaves before its destination. Pure.
pub(in crate::oracle::canonical) fn source_card_origin_follows(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    for offset in 1..8 {
        match cursor
            .peek_ahead(offset)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("from") => {
                return cursor
                    .peek_ahead(offset + 1)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
                    && cursor.peek_ahead(offset + 2).is_some_and(|raw| {
                        raw.eq_ignore_ascii_case("hand") || raw.eq_ignore_ascii_case("graveyard")
                    });
            }
            Some("on" | "onto" | "into" | "." | "," | ";") | None => return false,
            Some(_) => {}
        }
    }
    false
}

/// Recognizes one source-object zone move. A caller composes exile and return
/// as two effects, so an intervening zone-change event remains observable.
pub(in crate::oracle::canonical) fn recognize_source_zone_move_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<MoveSourceEffectNode> {
    cursor.enter("effect.move-source.action");
    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("exile") => {
            crate::oracle::canonical::automata::consume_lemma(
                cursor,
                crate::oracle::canonical::automata::VerbLemma::Exile,
                crate::oracle::canonical::automata::VerbAgreement::Base,
            )?;
            cursor.enter("effect.move-source.exile.source");
            let source = recognize_source_reference(cursor, name_links, name_failure)?;
            // `Exile NAME with N KIND counters on it` (CR 702.62a): the
            // shared entry-counter reader of every object-to-zone frame;
            // the source is exiled with them.
            let entering_counters = if cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("with"))
            {
                cursor.enter("effect.move-source.exile.counters");
                Some(super::recognize_entering_counters(cursor)?)
            } else {
                None
            };
            cursor.enter("effect.move-source.complete");
            Ok(MoveSourceEffectNode {
                source,
                from: SourceZone::Battlefield,
                to: SourceZone::Exile,
                face_change: SourceFaceChange::Preserve,
                controller: SourceMoveController::Preserve,
                tapped: false,
                attached_to: None,
                entering_counters,
                attacking: None,
            })
        }
        Some("return") => {
            crate::oracle::canonical::automata::consume_lemma(
                cursor,
                crate::oracle::canonical::automata::VerbLemma::Return,
                crate::oracle::canonical::automata::VerbAgreement::Base,
            )?;
            cursor.enter("effect.move-source.return.source");
            let source = recognize_source_reference_with_antecedent(
                cursor,
                name_links,
                name_failure,
                context.source.as_ref(),
            )?;
            // An explicit origin is the only proof of where the source is
            // moved from; the bare form keeps its transformed-return meaning.
            cursor.enter("effect.move-source.return.origin");
            let origin = if cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("from"))
            {
                cursor.consume("from")?;
                cursor.consume("your")?;
                cursor.consume("graveyard")?;
                Some(SourceZone::Graveyard)
            } else {
                None
            };
            cursor.enter("effect.move-source.return.destination");
            cursor.consume("to")?;
            if origin == Some(SourceZone::Graveyard)
                && cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
            {
                // `from your graveyard to the battlefield [tapped]`: the card
                // enters under the control of the player putting it there
                // (CR 110.2), the ability's controller.
                cursor.enter("effect.move-source.return.battlefield");
                cursor.consume_phrase(&["the", "battlefield"])?;
                let tapped = cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("tapped"));
                if tapped {
                    cursor.consume("tapped")?;
                }
                // `tapped and attacking [DEFENDER]` (CR 508.4): the shared
                // attacking-entry reader of every battlefield destination.
                let attacking = if tapped
                    && cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                    && cursor
                        .peek_ahead(1)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("attacking"))
                {
                    cursor.consume("and")?;
                    Some(super::recognize_entering_attack(cursor)?)
                } else {
                    None
                };
                // `with N COUNTER counter(s) on it`: the shared entry-counter
                // reader of card moves; the source enters with them (CR
                // 122.6), it does not receive them after entering.
                let entering_counters = if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("with"))
                {
                    cursor.enter("effect.move-source.return.entering-counters");
                    Some(super::recognize_entering_counters(cursor)?)
                } else {
                    None
                };
                // `attached to OBJECT`: the shared permanent operand names
                // the permanent the source enters attached to (CR 303.4f).
                let attached_to = if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("attached"))
                {
                    cursor.enter("effect.move-source.return.attached-to");
                    cursor.consume_phrase(&["attached", "to"])?;
                    Some(recognize_contextual_permanent_operand(
                        cursor,
                        name_links,
                        name_failure,
                        context,
                    )?)
                } else {
                    None
                };
                cursor.enter("effect.move-source.complete");
                return Ok(MoveSourceEffectNode {
                    source,
                    from: SourceZone::Graveyard,
                    to: SourceZone::Battlefield,
                    face_change: SourceFaceChange::Preserve,
                    controller: SourceMoveController::Preserve,
                    tapped,
                    attached_to,
                    entering_counters,
                    attacking,
                });
            }
            if origin == Some(SourceZone::Graveyard) {
                cursor.consume("your")?;
                cursor.consume("hand")?;
                cursor.enter("effect.move-source.complete");
                return Ok(MoveSourceEffectNode {
                    source,
                    from: SourceZone::Graveyard,
                    to: SourceZone::Hand,
                    face_change: SourceFaceChange::Preserve,
                    controller: SourceMoveController::Preserve,
                    tapped: false,
                    attached_to: None,
                    entering_counters: None,
                    attacking: None,
                });
            }
            if origin.is_none()
                && cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("its"))
            {
                // CR 108.3: a permanent returns to its owner's hand, which
                // may differ from its controller's.
                cursor.enter("effect.move-source.return.owners-hand");
                cursor.consume("its")?;
                let owner = cursor
                    .peek()
                    .filter(|raw| {
                        raw.eq_ignore_ascii_case("owner's")
                            || raw.eq_ignore_ascii_case("owner\u{2019}s")
                    })
                    .map(str::to_owned)
                    .ok_or_else(|| cursor.fail("owner's"))?;
                cursor.consume(&owner)?;
                cursor.consume("hand")?;
                cursor.enter("effect.move-source.complete");
                return Ok(MoveSourceEffectNode {
                    source,
                    from: SourceZone::Battlefield,
                    to: SourceZone::Hand,
                    face_change: SourceFaceChange::Preserve,
                    controller: SourceMoveController::Owner,
                    tapped: false,
                    attached_to: None,
                    entering_counters: None,
                    attacking: None,
                });
            }
            cursor.consume_phrase(&["the", "battlefield"])?;
            cursor.enter("effect.move-source.return.face-change");
            cursor.consume("transformed")?;
            cursor.enter("effect.move-source.return.controller");
            cursor.consume("under")?;
            let possessor = cursor.consume_type("referenceWord")?;
            cursor.require(
                possessor.raw.eq_ignore_ascii_case("her")
                    || possessor.raw.eq_ignore_ascii_case("his")
                    || possessor.raw.eq_ignore_ascii_case("its")
                    || possessor.raw.eq_ignore_ascii_case("their"),
                "a possessive source anaphor",
            )?;
            let owner = cursor.consume_type("possessivePlayerRoleWord")?;
            cursor.require(
                owner.raw.eq_ignore_ascii_case("owner's")
                    || owner.raw.eq_ignore_ascii_case("owner’s"),
                "the source owner's control",
            )?;
            cursor.consume("control")?;
            cursor.enter("effect.move-source.complete");
            Ok(MoveSourceEffectNode {
                source,
                from: SourceZone::Exile,
                to: SourceZone::Battlefield,
                face_change: SourceFaceChange::Transform,
                controller: SourceMoveController::Owner,
                tapped: false,
                attached_to: None,
                entering_counters: None,
                attacking: None,
            })
        }
        // `put this card from your hand|graveyard|exile onto the battlefield
        // [tapped] [transformed]`: the object-to-zone frame of `put` over the
        // source card, whose explicit origin proves where it is moved from.
        Some(_)
            if crate::oracle::canonical::automata::lemma_at(
                cursor,
                crate::oracle::canonical::automata::VerbLemma::Put,
                crate::oracle::canonical::automata::VerbAgreement::Base,
            ) =>
        {
            crate::oracle::canonical::automata::consume_lemma(
                cursor,
                crate::oracle::canonical::automata::VerbLemma::Put,
                crate::oracle::canonical::automata::VerbAgreement::Base,
            )?;
            cursor.enter("effect.move-source.put.source");
            let source = recognize_source_reference_with_antecedent(
                cursor,
                name_links,
                name_failure,
                context.source.as_ref(),
            )?;
            cursor.enter("effect.move-source.put.origin");
            // The ability is activated where the card is (its activation
            // zone follows the origin): the payer's hand or graveyard.
            cursor.consume_phrase(&["from", "your"])?;
            let from = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                Some("hand") => {
                    cursor.consume("hand")?;
                    SourceZone::Hand
                }
                Some("graveyard") => {
                    cursor.consume("graveyard")?;
                    SourceZone::Graveyard
                }
                _ => return Err(cursor.fail("`hand` or `graveyard` after `from your`")),
            };
            cursor.enter("effect.move-source.put.destination");
            cursor.consume_phrase(&["onto", "the", "battlefield"])?;
            // The shared entry modifiers of a battlefield destination; the
            // card enters under the control of the player putting it there
            // (CR 110.2), the ability's controller.
            let mut modifiers = super::EntryModifiers::new(super::CardMoveOwnerNode::Player(
                super::LocalPlayerReference::Controller,
            ));
            super::recognize_entry_modifiers(
                cursor,
                super::CardZoneName::Battlefield,
                &mut modifiers,
            )?;
            cursor.enter("effect.move-source.put.entry-support");
            cursor.require(
                !modifiers.face_down
                    && modifiers.attached_to.is_none()
                    && modifiers.controller
                        == super::CardMoveOwnerNode::Player(super::LocalPlayerReference::Controller),
                "a source entry tapped, transformed or with counters (face-down, attached and other-controller entries have no source-move writer)",
            )?;
            cursor.enter("effect.move-source.complete");
            Ok(MoveSourceEffectNode {
                source,
                from,
                to: SourceZone::Battlefield,
                face_change: if modifiers.transformed {
                    SourceFaceChange::Transform
                } else {
                    SourceFaceChange::Preserve
                },
                controller: SourceMoveController::Preserve,
                tapped: modifiers.tapped,
                attached_to: None,
                entering_counters: modifiers.entering_counters,
                attacking: modifiers.attacking,
            })
        }
        _ => Err(cursor.fail("`exile`, `return` or `put` source-zone action")),
    }
}

#[cfg(test)]
#[path = "source_zone_move_tests.rs"]
mod tests;
