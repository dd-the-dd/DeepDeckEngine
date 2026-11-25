use super::super::{ChildOutcome, CommitPoint, MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ZoneBatchActionNode {
    Exile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ZoneCollectionNode {
    AllGraveyards,
    OpponentGraveyards,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ZoneBatchEffectNode {
    pub action: ZoneBatchActionNode,
    pub collection: ZoneCollectionNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceEntryZoneBatchNode {
    pub source: LocalObjectReference,
    pub source_type: ObjectCardType,
    pub effect: ZoneBatchEffectNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum GraveyardObjectKindNode {
    Card,
    Token,
    CardOrToken,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum GraveyardOwnerScopeNode {
    EachPlayer,
    Controller,
    Opponents,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ZoneOriginNode {
    AnyZone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ZoneDestinationNode {
    Exile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct GraveyardDestinationReplacementNode {
    pub object_kind: GraveyardObjectKindNode,
    pub graveyard_owners: GraveyardOwnerScopeNode,
    pub from: ZoneOriginNode,
    pub to: ZoneDestinationNode,
}

fn permanent_source_type(raw: &str) -> Option<ObjectCardType> {
    match positive_object_selector_atom(raw) {
        Some(SelectorPredicate::CardType(card_type)) if card_type.is_permanent_type() => {
            Some(card_type)
        }
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntryZoneBatchState {
    Trigger,
    Source,
    SourceType,
    Event,
    Separator,
    Action,
    Terminator,
    Complete,
}

#[derive(Default)]
struct EntryZoneBatchFrame {
    source_type: Option<ObjectCardType>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ZoneBatchState {
    Action,
    Quantifier,
    PossessiveOwner,
    Collection,
    Complete,
}

/// Recognizes an exile action over a graveyard collection independently from
/// the ability envelope. `all graveyards` and `each opponent's graveyard`
/// differ only by their typed owner scope.
pub(in crate::oracle::canonical) fn recognize_exile_graveyard_collection_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ZoneBatchEffectNode> {
    let mut state = ZoneBatchState::Action;
    let mut collection = None;
    // `all [your] opponents' graveyards` names the same opponent scope as
    // `each opponent's graveyard`, with the plural zone noun.
    let mut plural_zone = false;
    loop {
        state = match state {
            ZoneBatchState::Action => {
                cursor.enter("effect.zone-batch.action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    is_verb_form(&action.raw, VerbLemma::Exile, VerbForm::Base),
                    "the `exile` action",
                )?;
                ZoneBatchState::Quantifier
            }
            ZoneBatchState::Quantifier => {
                cursor.enter("effect.zone-batch.quantifier");
                let quantifier = cursor.consume_type("quantifierWord")?;
                if quantifier.raw.eq_ignore_ascii_case("all") {
                    plural_zone = true;
                    if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
                        && cursor.peek_ahead(1).is_some_and(|raw| {
                            raw.eq_ignore_ascii_case("opponents'")
                                || raw.eq_ignore_ascii_case("opponents\u{2019}")
                        })
                    {
                        cursor.consume("your")?;
                    }
                    let opponents = cursor
                        .peek()
                        .filter(|raw| {
                            raw.eq_ignore_ascii_case("opponents'")
                                || raw.eq_ignore_ascii_case("opponents\u{2019}")
                        })
                        .map(str::to_owned);
                    if let Some(opponents) = opponents {
                        cursor.enter("effect.zone-batch.possessive-owner");
                        cursor.consume(&opponents)?;
                        collection = Some(ZoneCollectionNode::OpponentGraveyards);
                    } else {
                        collection = Some(ZoneCollectionNode::AllGraveyards);
                    }
                    ZoneBatchState::Collection
                } else {
                    cursor.require(
                        quantifier.raw.eq_ignore_ascii_case("each"),
                        "the `all` or `each` collection quantifier",
                    )?;
                    collection = Some(ZoneCollectionNode::OpponentGraveyards);
                    ZoneBatchState::PossessiveOwner
                }
            }
            ZoneBatchState::PossessiveOwner => {
                cursor.enter("effect.zone-batch.possessive-owner");
                let owner = cursor.consume_type("possessivePlayerRoleWord")?;
                cursor.require(
                    owner.raw.eq_ignore_ascii_case("opponent's")
                        || owner.raw.eq_ignore_ascii_case("opponent’s"),
                    "the opponent possessive",
                )?;
                ZoneBatchState::Collection
            }
            ZoneBatchState::Collection => {
                cursor.enter("effect.zone-batch.collection");
                let expected = match collection {
                    Some(_) if plural_zone => "graveyards",
                    Some(ZoneCollectionNode::AllGraveyards) => "graveyards",
                    Some(ZoneCollectionNode::OpponentGraveyards) => "graveyard",
                    None => return Err(cursor.fail("a populated graveyard collection scope")),
                };
                let token = cursor
                    .peek_token()
                    .cloned()
                    .ok_or_else(|| cursor.fail(expected))?;
                if token.token_type != "zoneWord" || !token.raw.eq_ignore_ascii_case(expected) {
                    return Err(cursor.fail_with_span(
                        expected,
                        token.index,
                        token.span_start,
                        token.span_end,
                        token.raw,
                    ));
                }
                cursor.advance_to(cursor.position() + 1)?;
                ZoneBatchState::Complete
            }
            ZoneBatchState::Complete => {
                cursor.enter("effect.zone-batch.complete");
                return Ok(ZoneBatchEffectNode {
                    action: ZoneBatchActionNode::Exile,
                    collection: cursor.require_value(collection, "a graveyard collection")?,
                });
            }
        };
    }
}

/// Recognizes a self-entry event followed by one atomic action over a zone
/// collection. Event recognition and collection recognition remain distinct
/// states so the same zone-batch child can be reused by other ability forms.
pub(in crate::oracle::canonical) fn recognize_source_entry_exile_all_graveyards(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SourceEntryZoneBatchNode> {
    let mut state = EntryZoneBatchState::Trigger;
    let mut frame = EntryZoneBatchFrame::default();
    loop {
        state = match state {
            EntryZoneBatchState::Trigger => {
                cursor.enter("trigger.source-entry.trigger");
                let trigger = cursor.consume_type("triggerWord")?;
                cursor.require(
                    trigger.raw.eq_ignore_ascii_case("when"),
                    "the `when` trigger",
                )?;
                EntryZoneBatchState::Source
            }
            EntryZoneBatchState::Source => {
                cursor.enter("trigger.source-entry.source");
                cursor.consume("this")?;
                EntryZoneBatchState::SourceType
            }
            EntryZoneBatchState::SourceType => {
                cursor.enter("trigger.source-entry.source-type");
                let raw = cursor.consume_type("typeWord")?;
                frame.source_type = Some(
                    permanent_source_type(&raw.raw)
                        .ok_or_else(|| cursor.fail("a permanent card type"))?,
                );
                EntryZoneBatchState::Event
            }
            EntryZoneBatchState::Event => {
                cursor.enter("trigger.source-entry.event");
                let event = cursor.consume_type("eventWord")?;
                cursor.require(
                    event.raw.eq_ignore_ascii_case("enters"),
                    "the `enters` event",
                )?;
                EntryZoneBatchState::Separator
            }
            EntryZoneBatchState::Separator => {
                cursor.enter("trigger.source-entry.separator");
                cursor.consume(",")?;
                EntryZoneBatchState::Action
            }
            EntryZoneBatchState::Action => {
                cursor.enter("trigger.source-entry.zone-batch-effect");
                let effect = recognize_exile_graveyard_collection_effect(cursor)?;
                cursor.require(
                    effect.collection == ZoneCollectionNode::AllGraveyards,
                    "the all-graveyards collection for this entry grammar",
                )?;
                EntryZoneBatchState::Terminator
            }
            EntryZoneBatchState::Terminator => {
                cursor.enter("effect.zone-batch.terminator");
                cursor.consume(".")?;
                EntryZoneBatchState::Complete
            }
            EntryZoneBatchState::Complete => {
                cursor.enter("trigger.source-entry.zone-batch.complete");
                cursor.finish()?;
                return Ok(SourceEntryZoneBatchNode {
                    source: LocalObjectReference::SourceSelf,
                    source_type: cursor.require_value(frame.source_type, "a source card type")?,
                    effect: ZoneBatchEffectNode {
                        action: ZoneBatchActionNode::Exile,
                        collection: ZoneCollectionNode::AllGraveyards,
                    },
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GraveyardReplacementState {
    Conditional,
    Article,
    ObjectKind,
    ObjectConnectorOrEvent,
    TokenBranch,
    EventModality,
    Passive,
    MoveAction,
    DestinationRelation,
    GraveyardOwner,
    GraveyardZone,
    OriginRelation,
    Origin,
    Separator,
    ReplacementAction,
    ObjectReference,
    Instead,
    Terminator,
    Complete,
}

#[derive(Default)]
struct GraveyardReplacementFrame {
    object_kind: Option<GraveyardObjectKindNode>,
    graveyard_owners: Option<GraveyardOwnerScopeNode>,
}

/// Recognizes a replacement of the destination of a card/token zone move.
/// Object kind, graveyard owner, origin and replacement destination are stored
/// independently instead of being inferred from one matched sentence.
pub(in crate::oracle::canonical) fn recognize_graveyard_destination_replacement(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<GraveyardDestinationReplacementNode> {
    let mut state = GraveyardReplacementState::Conditional;
    let mut frame = GraveyardReplacementFrame::default();
    loop {
        state = match state {
            GraveyardReplacementState::Conditional => {
                cursor.enter("replacement.graveyard-destination.conditional");
                cursor.consume("if")?;
                GraveyardReplacementState::Article
            }
            GraveyardReplacementState::Article => {
                cursor.enter("replacement.graveyard-destination.object-article");
                cursor.consume("a")?;
                GraveyardReplacementState::ObjectKind
            }
            GraveyardReplacementState::ObjectKind => {
                cursor.enter("replacement.graveyard-destination.object-kind");
                match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                    Some("card") => {
                        cursor.consume("card")?;
                        frame.object_kind = Some(GraveyardObjectKindNode::Card);
                        GraveyardReplacementState::ObjectConnectorOrEvent
                    }
                    Some("token") => {
                        cursor.consume("token")?;
                        frame.object_kind = Some(GraveyardObjectKindNode::Token);
                        GraveyardReplacementState::EventModality
                    }
                    _ => return Err(cursor.fail("`card` or `token`")),
                }
            }
            GraveyardReplacementState::ObjectConnectorOrEvent => {
                cursor.enter("replacement.graveyard-destination.object-connector-or-event");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
                {
                    cursor.consume("or")?;
                    GraveyardReplacementState::TokenBranch
                } else {
                    GraveyardReplacementState::EventModality
                }
            }
            GraveyardReplacementState::TokenBranch => {
                cursor.enter("replacement.graveyard-destination.object-token-branch");
                cursor.consume("token")?;
                frame.object_kind = Some(GraveyardObjectKindNode::CardOrToken);
                GraveyardReplacementState::EventModality
            }
            GraveyardReplacementState::EventModality => {
                cursor.enter("replacement.graveyard-destination.event-modality");
                cursor.consume_type("replacementModalWord")?;
                GraveyardReplacementState::Passive
            }
            GraveyardReplacementState::Passive => {
                cursor.enter("replacement.graveyard-destination.passive");
                cursor.consume("be")?;
                GraveyardReplacementState::MoveAction
            }
            GraveyardReplacementState::MoveAction => {
                cursor.enter("replacement.graveyard-destination.move-action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    is_verb_form(&action.raw, VerbLemma::Put, VerbForm::Base),
                    "the `put` zone move",
                )?;
                GraveyardReplacementState::DestinationRelation
            }
            GraveyardReplacementState::DestinationRelation => {
                cursor.enter("replacement.graveyard-destination.destination-relation");
                cursor.consume("into")?;
                GraveyardReplacementState::GraveyardOwner
            }
            GraveyardReplacementState::GraveyardOwner => {
                cursor.enter("replacement.graveyard-destination.graveyard-owner");
                match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                    Some("a") => {
                        cursor.consume("a")?;
                        frame.graveyard_owners = Some(GraveyardOwnerScopeNode::EachPlayer);
                    }
                    Some("your") => {
                        cursor.consume("your")?;
                        frame.graveyard_owners = Some(GraveyardOwnerScopeNode::Controller);
                    }
                    Some("an") => {
                        cursor.consume("an")?;
                        let owner = cursor.consume_type("possessivePlayerRoleWord")?;
                        cursor.require(
                            owner.raw.eq_ignore_ascii_case("opponent's")
                                || owner.raw.eq_ignore_ascii_case("opponent’s"),
                            "the opponent possessive",
                        )?;
                        frame.graveyard_owners = Some(GraveyardOwnerScopeNode::Opponents);
                    }
                    _ => return Err(cursor.fail("`a`, `your`, or `an opponent's`")),
                }
                GraveyardReplacementState::GraveyardZone
            }
            GraveyardReplacementState::GraveyardZone => {
                cursor.enter("replacement.graveyard-destination.graveyard-zone");
                let zone = cursor.consume_type("zoneWord")?;
                cursor.require(
                    zone.raw.eq_ignore_ascii_case("graveyard"),
                    "the graveyard zone",
                )?;
                GraveyardReplacementState::OriginRelation
            }
            GraveyardReplacementState::OriginRelation => {
                cursor.enter("replacement.graveyard-destination.origin-relation");
                cursor.consume("from")?;
                GraveyardReplacementState::Origin
            }
            GraveyardReplacementState::Origin => {
                cursor.enter("replacement.graveyard-destination.origin");
                let origin = cursor.consume_type("zoneScopeWord")?;
                cursor.require(
                    origin.raw.eq_ignore_ascii_case("anywhere"),
                    "the `anywhere` zone origin",
                )?;
                GraveyardReplacementState::Separator
            }
            GraveyardReplacementState::Separator => {
                cursor.enter("replacement.graveyard-destination.separator");
                cursor.consume(",")?;
                GraveyardReplacementState::ReplacementAction
            }
            GraveyardReplacementState::ReplacementAction => {
                cursor.enter("replacement.graveyard-destination.replacement-action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    is_verb_form(&action.raw, VerbLemma::Exile, VerbForm::Base),
                    "the `exile` action",
                )?;
                GraveyardReplacementState::ObjectReference
            }
            GraveyardReplacementState::ObjectReference => {
                cursor.enter("replacement.graveyard-destination.object-reference");
                cursor.consume("it")?;
                GraveyardReplacementState::Instead
            }
            GraveyardReplacementState::Instead => {
                cursor.enter("replacement.graveyard-destination.instead");
                cursor.consume("instead")?;
                GraveyardReplacementState::Terminator
            }
            GraveyardReplacementState::Terminator => {
                cursor.enter("replacement.graveyard-destination.terminator");
                cursor.consume(".")?;
                GraveyardReplacementState::Complete
            }
            GraveyardReplacementState::Complete => {
                cursor.enter("replacement.graveyard-destination.complete");
                cursor.finish()?;
                return Ok(GraveyardDestinationReplacementNode {
                    object_kind: cursor.require_value(frame.object_kind, "an object kind")?,
                    graveyard_owners: cursor
                        .require_value(frame.graveyard_owners, "a graveyard owner scope")?,
                    from: ZoneOriginNode::AnyZone,
                    to: ZoneDestinationNode::Exile,
                });
            }
        };
    }
}

// ---------------------------------------------------------------------------
// The shared departure replacement frame (CR 614.1a, 700.4, 603.12).
//
// ```text
// Frame       := ["until" "end" "of" "turn" ","] "if" Subject Event [Lifetime]
//                "," Program "." [Reflexive]
// Subject     := SourceReference                      -- `this creature`, its name
//              | ("a" | "an" | "another") ObjectQuery ["dealt" "damage" "this" "way"]
// Event       := "would" "die"
//              | "would" "be" "put" "into" Graveyard [Origin]
// Graveyard   := "a" "graveyard" | "your" "graveyard" | "an" "opponent's" "graveyard"
//              | ("its" | "their") "owner's" "graveyard"
// Origin      := "from" "anywhere" | "from" "the" "battlefield"
// Lifetime    := "this" "turn"
// Program     := ["instead"] Destination ["instead"]  -- exactly one `instead`
// Destination := "exile" Object
//              | "return" Object "to" Owner "hand"
//              | "put" Object ("on" "top" "of" | "on" "the" "bottom" "of") Owner "library"
//              | "put" Object "into" Owner "hand"
//              | ["reveal" Object "and"] "shuffle" Object "into" Owner "library"
// Object      := "it" | "that" Noun | SourceReference
// Owner       := ("its" | "their") "owner's" | "your"
// Reflexive   := "when" "you" "do" "," EffectProgram    -- CR 603.12
// ```
//
// The subject is the shared object query with its player relations; the
// event commits the frame once `would` and the departure verb are read, so an
// `if` clause naming any other event stays with its own reader. The replaced
// object is the event's object: the program names it with an anaphor only.
// ---------------------------------------------------------------------------

/// The objects whose move a departure replacement replaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DepartureSubjectNode {
    /// `this creature`, the card's name: the source itself.
    Source,
    /// `a|an|another QUERY`: each object the shared query admits.
    Objects(DepartureObjectsNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DepartureObjectsNode {
    pub predicate: SelectorPredicate,
    /// The domain the query names (CR 109.2a): permanents are matched by
    /// their effective characteristics, cards by their printed ones.
    pub domain: ObjectKind,
    pub controller: Option<ObjectControllerScope>,
    pub ownership: Option<ObjectOwnershipScope>,
    /// `another`: other than the source.
    pub exclude_source: bool,
    /// `dealt damage this way` (CR 608.2c): only the objects the creating
    /// effect dealt damage to.
    pub damaged_this_way: bool,
}

/// Where the departing object comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DepartureOriginNode {
    /// No origin written: the subject's domain decides (a permanent leaves
    /// the battlefield; a card can move from any zone).
    Unstated,
    /// `from anywhere`.
    Anywhere,
    /// `from the battlefield`.
    Battlefield,
}

/// The replaced departure event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DepartureEventNode {
    /// `would die` (CR 700.4): put into a graveyard from the battlefield.
    Dies,
    /// `would be put into GRAVEYARD [from ORIGIN]`.
    PutIntoGraveyard {
        graveyard: GraveyardOwnerScopeNode,
        origin: DepartureOriginNode,
    },
}

/// Where the object goes instead (CR 614.1a), always its owner's zone
/// (CR 400.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ReplacementDestinationNode {
    Exile,
    OwnerHand,
    OwnerLibraryTop,
    OwnerLibraryBottom,
    /// `[reveal it and] shuffle it into its owner's library` (CR 701.24a).
    OwnerLibraryShuffle {
        revealed: bool,
    },
}

/// One complete departure replacement.
#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) struct DepartureReplacementNode {
    pub subject: DepartureSubjectNode,
    pub event: DepartureEventNode,
    /// `this turn` or a preposed `Until end of turn,`: an effect-created
    /// replacement (CR 611.2a); `None` for a static ability.
    pub lifetime: Option<CreatedRuleLifetime>,
    pub destination: ReplacementDestinationNode,
    /// `When you do, ...`: the reflexive ability the applied replacement
    /// creates (CR 603.12).
    pub reflexive: Option<EffectProgramNode>,
}

/// Which enclosing frame reads the replacement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DepartureFrame {
    /// A whole static ability, through its last token, with an optional
    /// reflexive sentence.
    Ability,
    /// One sentence of a resolving effect's program, through its period.
    Sentence,
}

fn departure_word_at(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

fn possessive_at(cursor: &OracleTokenCursor<'_>, offset: usize, noun: &str) -> bool {
    cursor.peek_ahead(offset).is_some_and(|raw| {
        let lower = raw.to_ascii_lowercase();
        lower == format!("{noun}'s") || lower == format!("{noun}\u{2019}s")
    })
}

fn consume_possessive(cursor: &mut OracleTokenCursor<'_>, noun: &str) -> MachineResult<()> {
    let raw = cursor
        .peek()
        .filter(|_| possessive_at(cursor, 0, noun))
        .map(str::to_owned)
        .ok_or_else(|| cursor.fail("a possessive owner"))?;
    cursor.consume(&raw)
}

/// Whether `if ... would die|be put into` starts at the cursor, before the
/// sentence's first comma. Pure lookahead for program parents.
pub(in crate::oracle::canonical) fn departure_replacement_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let mut offset = 0;
    if departure_word_at(cursor, 0, "until") {
        if !(departure_word_at(cursor, 1, "end")
            && departure_word_at(cursor, 2, "of")
            && departure_word_at(cursor, 3, "turn")
            && departure_word_at(cursor, 4, ","))
        {
            return false;
        }
        offset = 5;
    }
    if !departure_word_at(cursor, offset, "if") {
        return false;
    }
    (offset + 1..)
        .take_while(|index| {
            cursor
                .peek_ahead(*index)
                .is_some_and(|raw| raw != "," && raw != ".")
        })
        .any(|index| {
            departure_word_at(cursor, index, "would")
                && (departure_word_at(cursor, index + 1, "die")
                    || (departure_word_at(cursor, index + 1, "be")
                        && departure_word_at(cursor, index + 2, "put")
                        && departure_word_at(cursor, index + 3, "into")))
        })
}

/// Reads one departure replacement; `NoMatch` until its subject, `would`
/// and departure verb are read.
pub(in crate::oracle::canonical) fn try_recognize_departure_replacement(
    cursor: &mut OracleTokenCursor<'_>,
    frame: DepartureFrame,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> ChildOutcome<DepartureReplacementNode> {
    cursor.transaction(|cursor, commit| {
        recognize_departure_replacement(cursor, commit, frame, name_links, name_failure)
    })
}

fn recognize_departure_replacement(
    cursor: &mut OracleTokenCursor<'_>,
    commit: &mut CommitPoint,
    frame: DepartureFrame,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<DepartureReplacementNode> {
    cursor.enter("replacement.departure.lifetime.preposed");
    let preposed = if departure_word_at(cursor, 0, "until") {
        let lifetime = super::embedded_delayed_trigger::recognize_created_rule_lifetime(cursor)?;
        cursor.consume(",")?;
        Some(lifetime)
    } else {
        None
    };
    cursor.enter("replacement.departure.introducer");
    cursor.consume("if")?;
    let subject = recognize_departure_subject(cursor, name_links, name_failure)?;
    let event = recognize_departure_event(cursor)?;
    commit.commit();
    cursor.enter("replacement.departure.lifetime");
    let postposed = if departure_word_at(cursor, 0, "this") {
        Some(super::embedded_delayed_trigger::recognize_created_rule_lifetime(cursor)?)
    } else {
        None
    };
    cursor.require(
        !(preposed.is_some() && postposed.is_some()),
        "one lifetime for the created replacement",
    )?;
    let lifetime = preposed.or(postposed);
    cursor.enter("replacement.departure.separator");
    cursor.consume(",")?;
    let destination = recognize_departure_program(cursor, &subject, name_links, name_failure)?;
    let reflexive = match frame {
        DepartureFrame::Sentence => None,
        DepartureFrame::Ability => {
            let reflexive = if cursor.peek().is_some() {
                cursor.enter("replacement.departure.reflexive");
                cursor.consume_phrase(&["when", "you", "do", ","])?;
                let context = EffectRecognitionContext::default();
                Some(recognize_effect_program_with_context(
                    cursor,
                    name_links,
                    name_failure,
                    &context,
                )?)
            } else {
                None
            };
            cursor.enter("replacement.departure.complete");
            cursor.finish()?;
            reflexive
        }
    };
    Ok(DepartureReplacementNode {
        subject,
        event,
        lifetime,
        destination,
        reflexive,
    })
}

fn recognize_departure_subject(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<DepartureSubjectNode> {
    cursor.enter("replacement.departure.subject");
    if source_reference_starts_at(cursor.position(), cursor.peek(), name_links, name_failure) {
        recognize_source_reference(cursor, name_links, name_failure)?;
        return Ok(DepartureSubjectNode::Source);
    }
    let phrase = recognize_object_noun_phrase(
        cursor,
        ObjectNounPhraseExpectation::in_domains(&[ObjectKind::Permanent, ObjectKind::Card]),
    )?;
    cursor.enter("replacement.departure.subject.determiner");
    cursor.require(
        matches!(
            &phrase.determiner,
            Some(ObjectDeterminer::Chosen {
                quantity: ChosenObjectQuantity::Exactly(count),
                ..
            }) if *count == NumericExpression::from(1)
        ),
        "`a`, `an` or `another` before the object that would move",
    )?;
    cursor.enter("replacement.departure.subject.domain");
    let domain = cursor.require_value(
        phrase.domain,
        "a permanent or card noun naming the object that would move",
    )?;
    let exclude_source = phrase.exclude_source;
    let related = phrase.query.into_related_query(cursor)?;
    cursor.enter("replacement.departure.subject.result");
    let damaged_this_way = if departure_word_at(cursor, 0, "dealt")
        && departure_word_at(cursor, 1, "damage")
        && departure_word_at(cursor, 2, "this")
        && departure_word_at(cursor, 3, "way")
    {
        cursor.consume_phrase(&["dealt", "damage", "this", "way"])?;
        true
    } else {
        false
    };
    Ok(DepartureSubjectNode::Objects(DepartureObjectsNode {
        predicate: related.objects.predicate,
        domain,
        controller: related.objects.controller,
        ownership: related.ownership,
        exclude_source,
        damaged_this_way,
    }))
}

fn recognize_departure_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DepartureEventNode> {
    cursor.enter("replacement.departure.event");
    cursor.consume("would")?;
    if departure_word_at(cursor, 0, "die") {
        cursor.consume("die")?;
        return Ok(DepartureEventNode::Dies);
    }
    cursor.consume_phrase(&["be", "put", "into"])?;
    cursor.enter("replacement.departure.event.graveyard");
    let graveyard = if departure_word_at(cursor, 0, "a") {
        cursor.consume("a")?;
        GraveyardOwnerScopeNode::EachPlayer
    } else if departure_word_at(cursor, 0, "your") {
        cursor.consume("your")?;
        GraveyardOwnerScopeNode::Controller
    } else if departure_word_at(cursor, 0, "an") && possessive_at(cursor, 1, "opponent") {
        cursor.consume("an")?;
        consume_possessive(cursor, "opponent")?;
        GraveyardOwnerScopeNode::Opponents
    } else if (departure_word_at(cursor, 0, "its") || departure_word_at(cursor, 0, "their"))
        && possessive_at(cursor, 1, "owner")
    {
        // Every card goes to its owner's graveyard (CR 400.3).
        let raw = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&raw)?;
        consume_possessive(cursor, "owner")?;
        GraveyardOwnerScopeNode::EachPlayer
    } else {
        return Err(cursor.fail("`a`, `your`, `an opponent's` or `its owner's` graveyard"));
    };
    cursor.consume("graveyard")?;
    cursor.enter("replacement.departure.event.origin");
    let origin = if departure_word_at(cursor, 0, "from") {
        cursor.consume("from")?;
        if departure_word_at(cursor, 0, "anywhere") {
            cursor.consume("anywhere")?;
            DepartureOriginNode::Anywhere
        } else {
            cursor.consume_phrase(&["the", "battlefield"])?;
            DepartureOriginNode::Battlefield
        }
    } else {
        DepartureOriginNode::Unstated
    };
    Ok(DepartureEventNode::PutIntoGraveyard { graveyard, origin })
}

/// `it`, `that NOUN`, or (for the source's own move) its name or `this
/// NOUN`: the event's object, never another one.
fn recognize_replaced_object(
    cursor: &mut OracleTokenCursor<'_>,
    subject: &DepartureSubjectNode,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<()> {
    cursor.enter("replacement.departure.object");
    if departure_word_at(cursor, 0, "it") {
        return cursor.consume("it");
    }
    if departure_word_at(cursor, 0, "that")
        && ["card", "creature", "permanent", "token"]
            .iter()
            .any(|noun| departure_word_at(cursor, 1, noun))
    {
        let noun = cursor.peek_ahead(1).unwrap_or_default().to_owned();
        return cursor.consume_phrase(&["that", noun.as_str()]);
    }
    if matches!(subject, DepartureSubjectNode::Source)
        && source_reference_starts_at(cursor.position(), cursor.peek(), name_links, name_failure)
    {
        recognize_source_reference(cursor, name_links, name_failure)?;
        return Ok(());
    }
    Err(cursor.fail("`it`, `that NOUN` or the source naming the object that would move"))
}

/// `its owner's` / `their owner's` / `your` before a hand or library: the
/// object always goes to its owner's zone (CR 400.3).
fn recognize_owner_zone(cursor: &mut OracleTokenCursor<'_>, zone: &str) -> MachineResult<()> {
    cursor.enter("replacement.departure.owner-zone");
    if departure_word_at(cursor, 0, "your") {
        cursor.consume("your")?;
    } else if (departure_word_at(cursor, 0, "its") || departure_word_at(cursor, 0, "their"))
        && possessive_at(cursor, 1, "owner")
    {
        let raw = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&raw)?;
        consume_possessive(cursor, "owner")?;
    } else {
        return Err(cursor.fail("`its owner's` zone of the object"));
    }
    cursor.consume(zone)
}

/// `[instead] DESTINATION [instead] .` with exactly one `instead`.
fn recognize_departure_program(
    cursor: &mut OracleTokenCursor<'_>,
    subject: &DepartureSubjectNode,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<ReplacementDestinationNode> {
    cursor.enter("replacement.departure.instead.leading");
    let leading = departure_word_at(cursor, 0, "instead");
    if leading {
        cursor.consume("instead")?;
    }
    cursor.enter("replacement.departure.action");
    let destination = if departure_word_at(cursor, 0, "exile") {
        cursor.consume("exile")?;
        recognize_replaced_object(cursor, subject, name_links, name_failure)?;
        ReplacementDestinationNode::Exile
    } else if departure_word_at(cursor, 0, "return") {
        cursor.consume("return")?;
        recognize_replaced_object(cursor, subject, name_links, name_failure)?;
        cursor.enter("replacement.departure.action.hand");
        cursor.consume("to")?;
        recognize_owner_zone(cursor, "hand")?;
        ReplacementDestinationNode::OwnerHand
    } else if departure_word_at(cursor, 0, "put") {
        cursor.consume("put")?;
        recognize_replaced_object(cursor, subject, name_links, name_failure)?;
        cursor.enter("replacement.departure.action.position");
        if departure_word_at(cursor, 0, "into") {
            cursor.consume("into")?;
            recognize_owner_zone(cursor, "hand")?;
            ReplacementDestinationNode::OwnerHand
        } else if departure_word_at(cursor, 1, "top") {
            cursor.consume_phrase(&["on", "top", "of"])?;
            recognize_owner_zone(cursor, "library")?;
            ReplacementDestinationNode::OwnerLibraryTop
        } else {
            cursor.consume_phrase(&["on", "the", "bottom", "of"])?;
            recognize_owner_zone(cursor, "library")?;
            ReplacementDestinationNode::OwnerLibraryBottom
        }
    } else if departure_word_at(cursor, 0, "reveal") || departure_word_at(cursor, 0, "shuffle") {
        let revealed = departure_word_at(cursor, 0, "reveal");
        if revealed {
            cursor.consume("reveal")?;
            recognize_replaced_object(cursor, subject, name_links, name_failure)?;
            cursor.enter("replacement.departure.action.reveal-and");
            cursor.consume("and")?;
        }
        cursor.enter("replacement.departure.action.shuffle");
        cursor.consume("shuffle")?;
        recognize_replaced_object(cursor, subject, name_links, name_failure)?;
        cursor.consume("into")?;
        recognize_owner_zone(cursor, "library")?;
        ReplacementDestinationNode::OwnerLibraryShuffle { revealed }
    } else {
        return Err(cursor.fail(
            "a replacing move of the object: exile it, return it to its owner's hand, put it on top or the bottom of its owner's library, or shuffle it into its owner's library",
        ));
    };
    cursor.enter("replacement.departure.instead");
    if !leading {
        cursor.consume("instead")?;
    }
    cursor.enter("replacement.departure.additional-action");
    // `instead exile it and create ...`: an instruction performed with the
    // replacing move is not part of this frame and fails here explicitly.
    cursor.consume(".")?;
    Ok(destination)
}
