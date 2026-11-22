use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, lookup_card_type, lookup_noncreature_subtype, lookup_object_characteristic,
    lookup_object_domain, lookup_permanent_card_type,
};
use crate::oracle::syntax::{
    NameLinkKind, NameResolutionFailure, ResolvedNameLink, UnresolvedName,
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    AttachedObjectRole, LocalObjectReference, LocalPlayerReference, TargetBinding,
    classify_unsupported_word,
};

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
                    // An unlinked word that names a mechanic or a verb is not
                    // a card name: report the concept that is actually missing.
                    if let Some(word) = classify_unsupported_word(
                        cursor.token_slice(0, usize::MAX),
                        unresolved.token_start,
                    ) {
                        let tokens = cursor.token_slice(unresolved.token_start, word.end_token());
                        let found = tokens
                            .iter()
                            .map(|token| token.raw.as_str())
                            .collect::<Vec<_>>()
                            .join(" ");
                        cursor.enter(word.state());
                        return Err(cursor.fail_with_span(
                            word.expected(),
                            unresolved.token_start,
                            tokens
                                .first()
                                .map_or(unresolved.span_start, |token| token.span_start),
                            tokens
                                .last()
                                .map_or(unresolved.span_end, |token| token.span_end),
                            found,
                        ));
                    }
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
                            // CR 205.3: `this Equipment`, `this Aura`, `this
                            // Saga`... name the source by one of its subtypes.
                            || lookup_noncreature_subtype(raw).is_some()
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
    // A resolved name is an explicit reference even when its first word is a
    // pronoun. Consume its complete lexical link before considering anaphora.
    if current_source_link(cursor.position(), name_links).is_some() {
        return recognize_source_reference(cursor, name_links, name_failure);
    }
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
    if definite_object_anaphor_starts(cursor) {
        // `the permanent` (CR 702.32a): a definite description names the
        // same antecedent a pronoun would, never a new object.
        cursor.enter("reference.source-or-anaphor.definite");
        let reference = antecedent
            .cloned()
            .ok_or_else(|| cursor.fail("a source binding for the definite description"))?;
        cursor.advance_to(cursor.position() + 2)?;
        cursor.enter("reference.source-or-anaphor.complete");
        return Ok(reference);
    }
    recognize_source_reference(cursor, name_links, name_failure)
}

/// `the NOUN` where the noun names an object domain or card type
/// (`the permanent`, `the creature`): a definite anaphor.
pub(in crate::oracle::canonical) fn definite_object_anaphor_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        && cursor.peek_ahead(1).is_some_and(|raw| {
            lookup_card_type(&raw, AcceptedForms::SingularOnly).is_some()
                || lookup_object_domain(&raw, AcceptedForms::SingularOnly).is_some()
        })
}

/// Bridge a typed lexical subject into the existing source-only reference grammar.
/// Targets/observed objects need a different damage-source IR contract; never
/// substitute the ability owner. Explicit source names do not use this antecedent.
pub(in crate::oracle::canonical) fn recognize_source_reference_with_typed_antecedent(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    antecedent: Option<super::PermanentEffectReference>,
) -> MachineResult<LocalObjectReference> {
    if current_source_link(cursor.position(), name_links).is_some() {
        return recognize_source_reference(cursor, name_links, name_failure);
    }
    let source = match antecedent {
        Some(super::PermanentEffectReference::Source(source)) => Some(source),
        Some(_) if cursor.peek().is_some_and(is_source_anaphor) => {
            cursor.enter("reference.source.antecedent-kind");
            return Err(cursor.fail("a source-identity antecedent; targeted or observed damage sources remain unsupported"));
        }
        _ => None,
    };
    recognize_source_reference_with_antecedent(cursor, name_links, name_failure, source.as_ref())
}

/// The attachment relation a source face has by its subtype: an Aura
/// enchants, an Equipment equips (CR 301.5, 303.4). Other faces have none.
pub(in crate::oracle::canonical) fn source_attachment_role(
    face_type_line: &str,
) -> Option<AttachedObjectRole> {
    let subtypes = face_type_line.split('\u{2014}').nth(1)?;
    let has = |subtype: &str| {
        subtypes
            .split(|character: char| !character.is_alphanumeric())
            .any(|word| word == subtype)
    };
    if has("Aura") {
        Some(AttachedObjectRole::Enchanted)
    } else if has("Equipment") {
        Some(AttachedObjectRole::Equipped)
    } else {
        None
    }
}

pub(in crate::oracle::canonical) fn attached_object_reference_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case("enchanted") || raw.eq_ignore_ascii_case("equipped")
    })
}

/// `enchanted PERMANENT` / `equipped creature`: a reference to the permanent
/// the ability source is attached to (CR 301.5a, 303.4a). The role must match
/// the source face supplied by the parent; an Aura may enchant any permanent
/// kind, Equipment only a creature. The runtime binds the object.
pub(in crate::oracle::canonical) fn recognize_attached_object_reference(
    cursor: &mut OracleTokenCursor<'_>,
    source_attachment: Option<AttachedObjectRole>,
) -> MachineResult<AttachedObjectRole> {
    cursor.enter("reference.attached-object.role");
    let role = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("enchanted") => AttachedObjectRole::Enchanted,
        Some("equipped") => AttachedObjectRole::Equipped,
        _ => return Err(cursor.fail("`enchanted` or `equipped`")),
    };
    cursor.require(
        source_attachment == Some(role),
        match role {
            AttachedObjectRole::Enchanted => "an Aura source for `enchanted`",
            AttachedObjectRole::Equipped => "an Equipment source for `equipped`",
        },
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("reference.attached-object.nominal");
    let noun = cursor
        .peek()
        .ok_or_else(|| cursor.fail("an attached permanent nominal"))?
        .to_string();
    let accepted = match role {
        AttachedObjectRole::Equipped => noun.eq_ignore_ascii_case("creature"),
        AttachedObjectRole::Enchanted => {
            noun.eq_ignore_ascii_case("permanent")
                || lookup_permanent_card_type(&noun, AcceptedForms::SingularOnly).is_some()
        }
    };
    cursor.require(
        accepted,
        match role {
            AttachedObjectRole::Equipped => "`creature` after `equipped`",
            AttachedObjectRole::Enchanted => {
                "`permanent` or a permanent card type after `enchanted`"
            }
        },
    )?;
    cursor.consume(&noun)?;
    cursor.enter("reference.attached-object.complete");
    Ok(role)
}
pub(in crate::oracle::canonical) fn recognize_player_reference(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<LocalPlayerReference> {
    recognize_player_reference_with_support(
        cursor,
        antecedent,
        super::TargetReferenceSupport::Unqualified,
    )
}

pub(in crate::oracle::canonical) fn recognize_player_reference_with_support(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
    support: super::TargetReferenceSupport,
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
        && (support == super::TargetReferenceSupport::Unqualified
            || cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("player")))
    {
        cursor.enter("reference.player.that-player");
        // Without this reader's own target binding, `that player` is the
        // player the shared binding environment names.
        let Some(object) = antecedent else {
            let environment = super::installed_binding_environment(cursor);
            return super::recognize_that_player(cursor, &environment)
                .map(LocalPlayerReference::Bound);
        };
        cursor.consume_phrase(&["that", "player"])?;
        cursor.enter("reference.player.complete");
        return Ok(LocalPlayerReference::ControllerOf(object));
    }

    let object = if support == super::TargetReferenceSupport::Unqualified {
        super::TypedTargetReference {
            binding: recognize_anaphoric_target_reference(cursor, antecedent)?,
            required_nominal: None,
        }
    } else {
        super::recognize_typed_target_reference(
            cursor,
            antecedent,
            "reference.player.object-reference.nominal",
        )?
    };
    cursor.enter("reference.player.controller-of-object");
    cursor.consume("controller")?;
    cursor.enter("reference.player.complete");
    Ok(if object.required_nominal.is_some() {
        LocalPlayerReference::ControllerOfQualified(object)
    } else {
        LocalPlayerReference::ControllerOf(object.binding)
    })
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

/// Object pronouns, including the gendered subject and object forms Oracle
/// uses for a named legendary character (`he deals`, `put a counter on her`).
pub(in crate::oracle::canonical) fn is_source_anaphor(raw: &str) -> bool {
    raw.eq_ignore_ascii_case("her")
        || raw.eq_ignore_ascii_case("him")
        || raw.eq_ignore_ascii_case("he")
        || raw.eq_ignore_ascii_case("she")
        || raw.eq_ignore_ascii_case("it")
}

#[cfg(test)]
#[path = "reference_anaphora_tests.rs"]
mod anaphora_tests;

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

#[cfg(test)]
#[path = "unsupported_word_tests.rs"]
mod unsupported_word_tests;
