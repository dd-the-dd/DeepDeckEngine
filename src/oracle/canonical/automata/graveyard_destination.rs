use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalObjectReference, ObjectCardType, SelectorPredicate, positive_object_selector_atom,
};
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};

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
