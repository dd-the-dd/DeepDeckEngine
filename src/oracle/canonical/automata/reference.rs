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
                } else if gendered_source_pronoun_at(cursor, name_links) {
                    // `When Josu Vess enters, if he was kicked`: a gendered
                    // personal pronoun names the character the text names,
                    // the source itself (CR 201.5); never another object.
                    cursor.enter("reference.source.gendered-pronoun");
                    let pronoun = cursor.peek().unwrap_or_default().to_owned();
                    cursor.consume(&pronoun)?;
                    SourceReferenceState::Complete
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
                // `this creature's`, `this Aura's`: the genitive of the source
                // names one of its characteristics or players, which the
                // possessive readers own (`recognize_possessive_source_reference`).
                // The object itself is not referenced here.
                if cursor
                    .peek()
                    .is_some_and(|raw| possessive_source_noun(raw) && !is_token_possessive(raw))
                {
                    cursor.enter("reference.source.possessive");
                    return Err(cursor.fail(
                        "the source object itself; `this NOUN's` names a characteristic or a player of the source, which this reader does not take",
                    ));
                }
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
                            // `this token's power` (CR 111.1): the token's
                            // own quoted ability names it by `token`, whose
                            // possessive is not a lexical type possessive.
                            || ["token's", "token\u{2019}s"]
                                .iter()
                                .any(|form| raw.eq_ignore_ascii_case(form))
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
    // Without a source binding from the caller, a pronoun names the source
    // only when the enclosing program's one binding environment resolves it
    // to the source (`This creature gets +1/+1 until end of turn. Sacrifice
    // it at the beginning of the next end step.`). Any other antecedent is
    // not this reader's object and stays a committed diagnostic below.
    let environment_antecedent = cursor
        .binding_environment()
        .and_then(|environment| super::resolve_pronoun_antecedent(environment).ok());
    let environment_source = environment_antecedent.and_then(|reference| match reference {
        super::PermanentEffectReference::Source(source) => Some(source),
        _ => None,
    });
    let antecedent = antecedent.or(environment_source.as_ref());
    if cursor.peek().is_some_and(is_source_anaphor) {
        let token = cursor
            .peek_token()
            .ok_or_else(|| cursor.fail("a source anaphor"))?
            .clone();
        // The program's antecedent is another object (a created token, a
        // declared target, an action result): this reader denotes only the
        // ability's source, so the pronoun's consumer needs a typed object
        // operand instead. A distinct state names that missing contract.
        if antecedent.is_none() && environment_antecedent.is_some() {
            cursor.enter("reference.source-or-anaphor.object-antecedent");
            return Err(cursor.fail_with_span(
                "a source antecedent; the pronoun names another object this consumer cannot denote",
                cursor.position(),
                token.span_start,
                token.span_end,
                token.raw.clone(),
            ));
        }
        // `If this Equipment is a creature and equipped creature is a
        // Warrior, it ...`: the preceding condition named several objects
        // and published none of them; the pronoun has competing antecedents.
        if antecedent.is_none()
            && cursor.binding_environment().is_some_and(|environment| {
                environment.object_producer
                    == Some(super::ObjectProducer::CompetingConditionSubjects)
            })
        {
            cursor.enter("reference.source-or-anaphor.competing-antecedents");
            return Err(cursor.fail_with_span(
                super::COMPETING_CONDITION_ANTECEDENTS,
                cursor.position(),
                token.span_start,
                token.span_end,
                token.raw.clone(),
            ));
        }
        // `Whenever a player casts a spell that targets Speedball, he gets
        // +2/+2`: before any action opened a lexical scope, a gendered
        // personal pronoun names the character the text names (the source,
        // CR 201.5), whatever other object the event records. After an
        // action (`Exile Jace, then return him`) the source may be a new
        // object (CR 400.7) and the pronoun keeps its diagnostic.
        let gendered_source = antecedent.is_none()
            && !cursor
                .binding_environment()
                .is_some_and(|environment| environment.local_object_scope)
            && gendered_source_pronoun_at(cursor, name_links);
        let gendered = gendered_source.then_some(LocalObjectReference::SourceSelf);
        let antecedent = antecedent.or(gendered.as_ref());
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

/// Lexical lookahead from a verb at the cursor: its operand is an
/// attached-object reference (`exile enchanted creature`, `destroy
/// enchanted land`), the one permanent the source is attached to, which the
/// bound-reference zone actions read through the shared contextual operand.
/// It proves no binding.
pub(in crate::oracle::canonical) fn attached_object_operand_follows_verb(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let mut operand = cursor.clone();
    operand.advance_to(operand.position() + 1).is_ok() && attached_object_reference_starts(&operand)
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
        // `enchanted Mountain`: the Aura's enchant ability may name a
        // subtype (`Enchant Mountain`, CR 303.4a); the noun restates it.
        AttachedObjectRole::Enchanted => {
            noun.eq_ignore_ascii_case("permanent")
                || lookup_permanent_card_type(&noun, AcceptedForms::SingularOnly).is_some()
                || lookup_noncreature_subtype(&noun).is_some()
        }
    };
    cursor.require(
        accepted,
        match role {
            AttachedObjectRole::Equipped => "`creature` after `equipped`",
            AttachedObjectRole::Enchanted => {
                "`permanent`, a permanent card type or a noncreature subtype after `enchanted`"
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

    // `its controller` / `that NOUN's owner` without this reader's own
    // target binding: the object the shared binding environment names (an
    // event object, the source, a moved card), never a guessed target.
    if antecedent.is_none() && super::object_related_player_head_length(cursor).is_some() {
        let environment = super::installed_binding_environment(cursor);
        return super::recognize_bound_player_reference(cursor, &environment)
            .map(LocalPlayerReference::Bound);
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

/// The genitive of a noun the source can be named by after `this` (CR
/// 205.3, 109.2): a card type, an object domain, a noncreature subtype
/// (`this Aura's`, `this Vehicle's`) or `token`. Lexical only.
pub(in crate::oracle::canonical) fn possessive_source_noun(raw: &str) -> bool {
    let normalized = raw.to_ascii_lowercase();
    let Some(base) = normalized
        .strip_suffix("'s")
        .or_else(|| normalized.strip_suffix("\u{2019}s"))
        .filter(|base| !base.is_empty())
    else {
        return false;
    };
    // Subtypes are written capitalized; read the written base.
    let written = &raw[..base.len()];
    lookup_card_type(base, AcceptedForms::SingularOnly).is_some()
        || lookup_object_domain(base, AcceptedForms::SingularOnly).is_some()
        || lookup_object_characteristic(base, AcceptedForms::SingularOnly).is_some()
        || lookup_noncreature_subtype(written).is_some()
}

/// `token's` / `token’s`: the token's own quoted abilities name it by this
/// genitive (CR 111.1); the source reader keeps reading it as the source.
fn is_token_possessive(raw: &str) -> bool {
    ["token's", "token\u{2019}s"]
        .iter()
        .any(|form| raw.eq_ignore_ascii_case(form))
}

/// `this NOUN's` (CR 109.2, 205.3): the source named by the genitive of one
/// of its nouns, for a reader of one of its characteristics or players (`this
/// creature's owner`, `this Aura's controller`). It consumes the determiner
/// and the genitive and returns the source; the caller reads what the
/// genitive names and refuses anything it does not take.
pub(in crate::oracle::canonical) fn recognize_possessive_source_reference(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalObjectReference> {
    cursor.enter("reference.source.possessive");
    cursor.consume("this")?;
    let genitive = cursor
        .peek()
        .filter(|raw| possessive_source_noun(raw))
        .ok_or_else(|| cursor.fail("the genitive of a source noun after `this`"))?
        .to_string();
    cursor.consume(&genitive)?;
    cursor.enter("reference.source.possessive.complete");
    Ok(LocalObjectReference::SourceSelf)
}

/// A gendered personal pronoun (`he`, `she`, `him`, the object `her`) that
/// names the source at the cursor: the ability's text names its source by
/// its name (a current-card or current-face link, CR 201.5), and the shared
/// binding environment names no other object with the pronoun. The
/// possessive `her` (`her owner's library`) is a determiner, followed by its
/// noun phrase; only an object `her` (before punctuation, a conjunction or a
/// preposition) is a pronoun here. Lexical and environment lookahead only.
pub(in crate::oracle::canonical) fn gendered_source_pronoun_at(
    cursor: &OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
) -> bool {
    let Some(raw) = cursor.peek().map(str::to_ascii_lowercase) else {
        return false;
    };
    let personal = match raw.as_str() {
        "he" | "she" | "him" => true,
        "her" => cursor.peek_ahead(1).is_none_or(|next| {
            matches!(
                next.to_ascii_lowercase().as_str(),
                "." | ","
                    | ";"
                    | ":"
                    | ")"
                    | "and"
                    | "or"
                    | "then"
                    | "to"
                    | "into"
                    | "onto"
                    | "from"
                    | "with"
                    | "until"
                    | "at"
                    | "if"
                    | "unless"
                    | "as"
                    | "for"
                    | "on"
                    | "in"
                    | "this"
            )
        }),
        _ => false,
    };
    let names_source = name_links.iter().any(|link| {
        matches!(
            link.kind,
            NameLinkKind::CurrentCard | NameLinkKind::CurrentFace
        )
    });
    let other_object = cursor.binding_environment().is_some_and(|environment| {
        super::resolve_pronoun_antecedent(environment)
            .is_ok_and(|reference| !matches!(reference, super::PermanentEffectReference::Source(_)))
    });
    personal && names_source && !other_object
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
