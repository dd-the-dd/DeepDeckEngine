use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectRecognitionContext, MoveSourceEffectNode, SourceFaceChange, SourceMoveController,
    SourceZone, recognize_source_reference, recognize_source_reference_with_antecedent,
};

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
            cursor.consume("exile")?;
            cursor.enter("effect.move-source.exile.source");
            let source = recognize_source_reference(cursor, name_links, name_failure)?;
            cursor.enter("effect.move-source.complete");
            Ok(MoveSourceEffectNode {
                source,
                from: SourceZone::Battlefield,
                to: SourceZone::Exile,
                face_change: SourceFaceChange::Preserve,
                controller: SourceMoveController::Preserve,
            })
        }
        Some("return") => {
            cursor.consume("return")?;
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
            })
        }
        _ => Err(cursor.fail("`exile` or `return` source-zone action")),
    }
}

#[cfg(test)]
#[path = "source_zone_move_tests.rs"]
mod tests;
