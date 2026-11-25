//! Zone actions on one bound permanent reference (`destroy|exile|sacrifice
//! it|that NOUN|the NOUN`, `put it on top of|shuffle it into a library`):
//! one reader, the verb's action as data.
use super::super::{MachineResult, OracleTokenCursor};

/// `destroy REFERENCE`: the destruction of one bound permanent reference
/// (the shared contextual operand), with an optional regeneration
/// prohibition (CR 701.19c). Target declarations stay with the targeted
/// zone change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ReferencedPermanentActionNode {
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
    /// `put REFERENCE on top of|on the bottom of|on their choice of the top
    /// or bottom of their library`: the permanent goes to its owner's
    /// library (CR 400.3) at the written place.
    PutOnOwnerLibrary(OwnerLibraryPlacement),
    /// `shuffle REFERENCE into their library` (CR 701.24a): the permanent
    /// goes to its owner's library (CR 400.3), then that library is shuffled.
    ShuffleIntoOwnerLibrary,
}

/// Where a permanent put into its owner's library goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum OwnerLibraryPlacement {
    Top,
    Bottom,
    /// `their choice of the top or bottom`: the acting player, the owner
    /// whose library it is, chooses on resolution.
    OwnerChoiceTopOrBottom,
    /// `into LIBRARY ORDINAL from the top` (CR 401.2): this many cards stay
    /// above it (the shared library-position child).
    BeneathTopCards(super::NumericAtom),
}

/// `exile|sacrifice REFERENCE` on a permanent the program bound other than
/// the ability's source (`exile it` after a card was returned to the
/// battlefield, `sacrifice the creature`): the shared reader below, claimed
/// only when it reads such a reference. Source references keep their own
/// readers; anything else is not claimed.
pub(in crate::oracle::canonical) fn recognize_referenced_permanent_zone_action(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
    context: &super::EffectRecognitionContext,
) -> ReferencedPermanentActionAttempt {
    let action = if super::lemma_at(cursor, super::VerbLemma::Exile, super::VerbAgreement::Base) {
        ReferencedPermanentAction::Exile
    } else if super::lemma_at(
        cursor,
        super::VerbLemma::Sacrifice,
        super::VerbAgreement::Base,
    ) {
        ReferencedPermanentAction::Sacrifice
    } else if super::lemma_at(cursor, super::VerbLemma::Put, super::VerbAgreement::Base) {
        // The placement is read after the reference.
        ReferencedPermanentAction::PutOnOwnerLibrary(OwnerLibraryPlacement::Top)
    } else if super::lemma_at(
        cursor,
        super::VerbLemma::Shuffle,
        super::VerbAgreement::Base,
    ) {
        ReferencedPermanentAction::ShuffleIntoOwnerLibrary
    } else {
        return ReferencedPermanentActionAttempt::NotClaimed;
    };
    // One read into a trial: the verb, then the bound reference. Exile and
    // sacrifice have no clause after their reference, so the claim is the
    // whole action: a non-source bound reference is parsed; anything else is
    // not claimed and leaves the other readers in charge.
    let mut trial = cursor.clone();
    match recognize_referenced_permanent_action(
        &mut trial,
        action,
        name_links,
        name_failure,
        context,
    ) {
        Ok(node) if !matches!(node.object, super::PermanentEffectReference::Source(_)) => {
            *cursor = trial;
            ReferencedPermanentActionAttempt::Parsed(node)
        }
        Ok(_) | Err(_) => ReferencedPermanentActionAttempt::NotClaimed,
    }
}

/// The transactional outcome of the zone action on a bound permanent.
pub(in crate::oracle::canonical) enum ReferencedPermanentActionAttempt {
    /// No verb of the family, or no non-source bound reference after it.
    NotClaimed,
    Parsed(ReferencedPermanentActionNode),
}

/// `destroy REFERENCE` (the established destroy reader).
pub(in crate::oracle::canonical) fn recognize_destroy_reference_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
    context: &super::EffectRecognitionContext,
) -> MachineResult<ReferencedPermanentActionNode> {
    recognize_referenced_permanent_action(
        cursor,
        ReferencedPermanentAction::Destroy,
        name_links,
        name_failure,
        context,
    )
}

/// One reader for the zone actions on one bound permanent reference
/// (`destroy|exile|sacrifice it|that NOUN|the NOUN`): the verb, the shared
/// reference operand (a definite description through the shared resolver),
/// and destruction's regeneration prohibition (CR 701.19c).
pub(in crate::oracle::canonical) fn recognize_referenced_permanent_action(
    cursor: &mut OracleTokenCursor<'_>,
    action: ReferencedPermanentAction,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
    context: &super::EffectRecognitionContext,
) -> MachineResult<ReferencedPermanentActionNode> {
    let (verb_state, object_state) = match action {
        ReferencedPermanentAction::Destroy => (
            "effect.destroy-reference.action",
            "effect.destroy-reference.object",
        ),
        ReferencedPermanentAction::Exile
        | ReferencedPermanentAction::Sacrifice
        | ReferencedPermanentAction::PutOnOwnerLibrary(_)
        | ReferencedPermanentAction::ShuffleIntoOwnerLibrary => (
            "effect.referenced-permanent.action",
            "effect.referenced-permanent.object",
        ),
    };
    cursor.enter(verb_state);
    match action {
        ReferencedPermanentAction::Destroy => cursor.consume("destroy")?,
        ReferencedPermanentAction::Exile => {
            super::consume_lemma(cursor, super::VerbLemma::Exile, super::VerbAgreement::Base)?
        }
        ReferencedPermanentAction::Sacrifice => super::consume_lemma(
            cursor,
            super::VerbLemma::Sacrifice,
            super::VerbAgreement::Base,
        )?,
        ReferencedPermanentAction::PutOnOwnerLibrary(_) => {
            super::consume_lemma(cursor, super::VerbLemma::Put, super::VerbAgreement::Base)?
        }
        ReferencedPermanentAction::ShuffleIntoOwnerLibrary => super::consume_lemma(
            cursor,
            super::VerbLemma::Shuffle,
            super::VerbAgreement::Base,
        )?,
    }
    cursor.enter(object_state);
    let object = if super::definite_object_anaphor_starts(cursor) {
        super::recognize_definite_object_reference(
            cursor,
            &super::BindingEnvironment::for_permanent_context(context),
        )?
    } else {
        let operand = super::recognize_contextual_permanent_operand(
            cursor,
            name_links,
            name_failure,
            context,
        )?;
        match (operand, &action) {
            (super::PermanentOperandNode::BoundReference(object), _) => object,
            // `put this creature on top of its owner's library`: the source
            // is the one permanent the placement moves (CR 400.3). The zone
            // action claim above still leaves source exile/sacrifice to
            // their own readers.
            (
                super::PermanentOperandNode::Reference(source),
                ReferencedPermanentAction::PutOnOwnerLibrary(_),
            ) => super::PermanentEffectReference::Source(source),
            _ => return Err(cursor.fail("a bound permanent reference")),
        }
    };
    let action = match action {
        ReferencedPermanentAction::PutOnOwnerLibrary(_) => {
            ReferencedPermanentAction::PutOnOwnerLibrary(recognize_owner_library_placement(cursor)?)
        }
        ReferencedPermanentAction::ShuffleIntoOwnerLibrary => {
            cursor.enter("effect.referenced-permanent.shuffle-destination");
            cursor.consume("into")?;
            recognize_owner_library(cursor)?;
            action
        }
        other => other,
    };
    let cannot_regenerate = match action {
        ReferencedPermanentAction::Destroy => {
            super::recognize_scalar_regeneration_amendment(cursor)?
        }
        ReferencedPermanentAction::Exile
        | ReferencedPermanentAction::Sacrifice
        | ReferencedPermanentAction::PutOnOwnerLibrary(_)
        | ReferencedPermanentAction::ShuffleIntoOwnerLibrary => false,
    };
    Ok(ReferencedPermanentActionNode {
        object,
        cannot_regenerate,
        action,
    })
}

/// `on top of LIBRARY` | `on the bottom of LIBRARY` | `on your|its owner's
/// choice of the top or bottom of LIBRARY`: the place in the owner's library.
fn recognize_owner_library_placement(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<OwnerLibraryPlacement> {
    cursor.enter("effect.referenced-permanent.library-placement");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("into"))
    {
        // `into its owner's library third from the top`: the library, then
        // the shared position beneath its top cards.
        cursor.consume("into")?;
        recognize_owner_library(cursor)?;
        cursor.enter("effect.referenced-permanent.library-placement.beneath-top");
        let position = super::try_recognize_beneath_top_position(cursor)?
            .ok_or_else(|| cursor.fail("`ORDINAL from the top` after `into` a library"))?;
        let super::LibraryPosition::BeneathTopCards(above) = position else {
            return Err(cursor.fail("a position beneath the top cards of the library"));
        };
        return Ok(OwnerLibraryPlacement::BeneathTopCards(above));
    }
    cursor.consume("on")?;
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let top = word(0, "top");
    let bottom = word(0, "the") && word(1, "bottom");
    let placement = if top {
        cursor.consume_phrase(&["top", "of"])?;
        OwnerLibraryPlacement::Top
    } else if bottom {
        cursor.consume_phrase(&["the", "bottom", "of"])?;
        OwnerLibraryPlacement::Bottom
    } else {
        cursor.enter("effect.referenced-permanent.library-placement.choice");
        recognize_library_holder(cursor, "choice")?;
        cursor.consume_phrase(&["of", "the", "top", "or", "bottom", "of"])?;
        OwnerLibraryPlacement::OwnerChoiceTopOrBottom
    };
    recognize_owner_library(cursor)?;
    Ok(placement)
}

/// `your library` (the acting player, the owner, after the performer's
/// person shift) | `its owner's library`.
fn recognize_owner_library(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("effect.referenced-permanent.owner-library");
    recognize_library_holder(cursor, "library")
}

/// `your NOUN` | `its owner's NOUN`: the owner of the referenced permanent.
fn recognize_library_holder(cursor: &mut OracleTokenCursor<'_>, noun: &str) -> MachineResult<()> {
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
    {
        return cursor.consume_phrase(&["your", noun]);
    }
    cursor.consume("its")?;
    let owner = cursor.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case("owner's") || raw.eq_ignore_ascii_case("owner\u{2019}s")
    });
    cursor.require(owner, "`your` or `its owner's`")?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.consume(noun)
}
