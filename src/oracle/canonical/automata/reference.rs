use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, lookup_card_type, lookup_object_characteristic, lookup_object_domain,
};
use crate::oracle::syntax::{
    NameLinkKind, NameResolutionFailure, ResolvedNameLink, UnresolvedName,
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{LocalObjectReference, LocalPlayerReference, TargetBinding};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceReferenceState {
    Start,
    ContextName,
    ThisObject,
    Complete,
}

/// Recognizes a reference to the object whose Oracle text is being compiled.
///
/// A printed name is accepted only through a link produced by the lexical name
/// resolver. The recognizer does not compare, join, or guess name strings.
pub(in crate::oracle::canonical) fn recognize_source_reference(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<LocalObjectReference> {
    let mut state = SourceReferenceState::Start;
    loop {
        state = match state {
            SourceReferenceState::Start => {
                cursor.enter("reference.source.start");
                if current_source_link(cursor.position(), name_links).is_some() {
                    SourceReferenceState::ContextName
                } else if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("this"))
                {
                    SourceReferenceState::ThisObject
                } else if let Some(unresolved) = unresolved_name_at(cursor.position(), name_failure)
                {
                    cursor.enter("reference.source.unresolved-name");
                    return Err(cursor.fail_with_span(
                        "a name linked to the current card or current face",
                        unresolved.token_start,
                        unresolved.span_start,
                        unresolved.span_end,
                        unresolved.raw.clone(),
                    ));
                } else {
                    return Err(cursor
                        .fail("current-card name, current-face name, or `this` source object"));
                }
            }
            SourceReferenceState::ContextName => {
                cursor.enter("reference.source.context-name");
                let link = current_source_link(cursor.position(), name_links)
                    .ok_or_else(|| cursor.fail("a resolved current-object name link"))?;
                cursor.advance_to(link.token_end)?;
                SourceReferenceState::Complete
            }
            SourceReferenceState::ThisObject => {
                cursor.enter("reference.source.this-object");
                cursor.consume("this")?;
                let object_noun = cursor
                    .peek()
                    .filter(|raw| {
                        lookup_card_type(raw, AcceptedForms::SingularOnly).is_some()
                            || lookup_object_domain(raw, AcceptedForms::SingularOnly).is_some()
                            || lookup_object_characteristic(raw, AcceptedForms::SingularOnly)
                                .is_some()
                    })
                    .ok_or_else(|| cursor.fail("a source-object type after `this`"))?
                    .to_string();
                cursor.consume(&object_noun)?;
                SourceReferenceState::Complete
            }
            SourceReferenceState::Complete => {
                cursor.enter("reference.source.complete");
                return Ok(LocalObjectReference::SourceSelf);
            }
        };
    }
}

/// Resolves a source name/`this` reference or a pronoun whose antecedent is
/// supplied by the surrounding ability envelope. Pronouns are never guessed:
/// without a bound source the same token produces a committed diagnostic.
pub(in crate::oracle::canonical) fn recognize_source_reference_with_antecedent(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    antecedent: Option<&LocalObjectReference>,
) -> MachineResult<LocalObjectReference> {
    cursor.enter("reference.source-or-anaphor.start");
    if cursor.peek().is_some_and(is_source_anaphor) {
        let token = cursor
            .peek_token()
            .ok_or_else(|| cursor.fail("a source anaphor"))?
            .clone();
        let reference = antecedent.cloned().ok_or_else(|| {
            cursor.fail_with_span(
                "a source binding for the anaphor",
                cursor.position(),
                token.span_start,
                token.span_end,
                token.raw.clone(),
            )
        })?;
        cursor.advance_to(cursor.position() + 1)?;
        cursor.enter("reference.source-or-anaphor.complete");
        return Ok(reference);
    }
    recognize_source_reference(cursor, name_links, name_failure)
}

pub(in crate::oracle::canonical) fn recognize_player_reference(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<LocalPlayerReference> {
    cursor.enter("reference.player.start");
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("you"))
    {
        cursor.consume("you")?;
        cursor.enter("reference.player.complete");
        return Ok(LocalPlayerReference::Controller);
    }

    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("that"))
    {
        cursor.enter("reference.player.that-player");
        cursor.consume_phrase(&["that", "player"])?;
        let object =
            antecedent.ok_or_else(|| cursor.fail("a prior target binding for `that player`"))?;
        cursor.enter("reference.player.complete");
        return Ok(LocalPlayerReference::ControllerOf(object));
    }

    let object = recognize_anaphoric_target_reference(cursor, antecedent)?;
    cursor.enter("reference.player.controller-of-object");
    cursor.consume("controller")?;
    cursor.enter("reference.player.complete");
    Ok(LocalPlayerReference::ControllerOf(object))
}

pub(in crate::oracle::canonical) fn recognize_anaphoric_target_reference(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<TargetBinding> {
    cursor.enter("reference.anaphora.start");
    cursor.consume("its")?;
    cursor.enter("reference.anaphora.resolve-target");
    antecedent.ok_or_else(|| cursor.fail("a prior target binding for `its`"))
}

pub(in crate::oracle::canonical) fn source_reference_starts_at(
    position: usize,
    next: Option<&str>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> bool {
    current_source_link(position, name_links).is_some()
        || next.is_some_and(|token| token.eq_ignore_ascii_case("this"))
        || unresolved_name_at(position, name_failure).is_some()
}

pub(in crate::oracle::canonical) fn source_or_anaphor_reference_starts_at(
    position: usize,
    next: Option<&str>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> bool {
    next.is_some_and(is_source_anaphor)
        || source_reference_starts_at(position, next, name_links, name_failure)
}

fn is_source_anaphor(raw: &str) -> bool {
    raw.eq_ignore_ascii_case("her")
        || raw.eq_ignore_ascii_case("him")
        || raw.eq_ignore_ascii_case("it")
}

fn unresolved_name_at<'a>(
    position: usize,
    failure: Option<&'a NameResolutionFailure>,
) -> Option<&'a UnresolvedName> {
    failure?
        .unresolved
        .iter()
        .find(|unresolved| unresolved.token_start == position)
}

fn current_source_link(
    position: usize,
    name_links: &[ResolvedNameLink],
) -> Option<&ResolvedNameLink> {
    name_links.iter().find(|link| {
        link.token_start == position
            && matches!(
                link.kind,
                NameLinkKind::CurrentCard | NameLinkKind::CurrentFace
            )
    })
}
