use super::super::{MachineResult, OracleTokenCursor};
use super::card_collection_operand::CollectionOperandAttempt;
use super::collection::{
    LibraryPosition, recognize_collection_order, recognize_library_position_kind,
};
use super::zone_list::{CardZoneName, recognize_card_zone_atom};
use super::{CollectionOrder, LocalPlayerReference, RecognizedEffectFragment};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardMoveOwnerNode {
    Player(LocalPlayerReference),
    EachCardOwner,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardMoveDestinationNode {
    pub zone: CardZoneName,
    pub owner: CardMoveOwnerNode,
    pub position: Option<LibraryPosition>,
    pub order: Option<CollectionOrder>,
    pub tapped: bool,
    /// `with N COUNTER counters on it|them`: the cards enter the battlefield
    /// with these counters (CR 122.6), not receive them afterwards.
    pub entering_counters: Option<EnteringCountersNode>,
    /// `face down`: the cards are exiled face down (CR 406.3) or enter the
    /// battlefield face down (CR 708.2). Never together with `transformed`.
    pub face_down: bool,
    /// `transformed`: the cards enter the battlefield with their back face
    /// up (CR 712.14a). Battlefield only.
    pub transformed: bool,
    /// `shuffle X into a library`: each library that received a card is
    /// shuffled after the move (CR 701.24b). Library only.
    pub shuffle: bool,
    /// `attached to OBJECT`: the permanent an entering Aura, Equipment or
    /// Fortification becomes attached to (CR 303.4f, 301.5c). Battlefield
    /// only.
    pub attached_to: Option<EntryAttachmentNode>,
    /// `put them back [in any order]`, `on the bottom of that library`: the
    /// library is the one the moved cards were observed in. The consuming
    /// lowerer proves the cards are such an observation; the cards' owner's
    /// library is that library (CR 400.3).
    pub origin_library: bool,
    /// `tapped and attacking [DEFENDER]`: the permanents enter attacking
    /// (CR 508.4); they were never declared as attackers, so nothing that
    /// triggers on an attack declaration sees them (CR 508.3a, 508.4).
    /// Battlefield only, always together with `tapped`.
    pub attacking: Option<EnteringAttackNode>,
}

/// Whom permanents put onto the battlefield attacking attack (CR 508.4):
/// the controller chooses among the defenders the effect admits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum EnteringAttackNode {
    /// No defender is named: any player, planeswalker or battle the
    /// controller's creatures could attack (CR 508.4).
    AnyDefender,
    /// `attacking that player|that opponent|defending player`.
    Player(super::BoundPlayerReference),
    /// `attacking defending player or a planeswalker they control`.
    PlayerOrPlaneswalkerTheyControl(super::BoundPlayerReference),
}

/// `that player|that opponent|defending player` after `attacking`. In an
/// attack trigger whose program declared no player of its own, `that
/// player|opponent` is the attacked player the event records (CR 506.2,
/// 603.2), as `defending player` is; every other reading is the shared
/// bound-player resolver's, which fails explicitly without a binding.
fn recognize_attacked_player(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::BoundPlayerReference> {
    cursor.enter("collection.destination.entry.attacking.defender");
    let environment = cursor
        .binding_environment()
        .cloned()
        .ok_or_else(|| cursor.fail("a binding environment naming the attacked player"))?;
    let that = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("that"));
    let opponent = that
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("opponent"));
    let event_attacked = environment.player_target.is_none()
        && environment.player.is_none()
        && (environment.frame.defending_player || environment.frame.event_player);
    if that && event_attacked {
        let noun = if opponent { "opponent" } else { "player" };
        cursor.consume_phrase(&["that", noun])?;
        return Ok(super::BoundPlayerReference::EventPlayer);
    }
    cursor.require(
        !opponent,
        "an attack event that records the attacked opponent",
    )?;
    super::recognize_bound_player_reference(cursor, &environment)
}

/// `attacking [that player|that opponent|defending player [or a planeswalker
/// they control]]` after `tapped and` (CR 508.4).
pub(in crate::oracle::canonical) fn recognize_entering_attack(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EnteringAttackNode> {
    cursor.enter("collection.destination.entry.attacking");
    cursor.consume("attacking")?;
    let word = |cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let names_player = (word(cursor, 0, "that")
        && (word(cursor, 1, "player") || word(cursor, 1, "opponent")))
        || (word(cursor, 0, "defending") && word(cursor, 1, "player"));
    if !names_player {
        return Ok(EnteringAttackNode::AnyDefender);
    }
    let player = recognize_attacked_player(cursor)?;
    if word(cursor, 0, "or") && word(cursor, 1, "a") && word(cursor, 2, "planeswalker") {
        cursor.enter("collection.destination.entry.attacking.planeswalker");
        cursor.consume_phrase(&["or", "a", "planeswalker", "they", "control"])?;
        return Ok(EnteringAttackNode::PlayerOrPlaneswalkerTheyControl(player));
    }
    Ok(EnteringAttackNode::Player(player))
}

/// The one permanent `attached to` names, read by the shared readers: the
/// ability's source (`this creature`), a bound reference (`it`, `that
/// creature`), one declared target, or one permanent chosen on resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum EntryAttachmentNode {
    Source,
    Reference(super::PermanentEffectReference),
    Target(super::TargetListNode),
    Chosen(super::PermanentSelectionOperandNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EnteringCountersNode {
    pub counter: crate::oracle::counter_name::CounterName,
    pub count: super::NumericExpression,
}

/// `with N COUNTER counter(s) on it|them` after a battlefield destination.
pub(in crate::oracle::canonical) fn recognize_entering_counters(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EnteringCountersNode> {
    cursor.enter("collection.destination.entering-counters");
    cursor.consume("with")?;
    let count = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
    {
        let article = cursor.peek().unwrap_or("a").to_owned();
        cursor.consume(&article)?;
        // `with an additional COUNTER counter on it`: the same placement as
        // the object enters (CR 122.6); it has no counters it would
        // otherwise enter with from this move.
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("additional"))
        {
            cursor.consume("additional")?;
        }
        super::NumericExpression::from(1)
    } else {
        super::recognize_numeric_expression(cursor, None)?
    };
    cursor.enter("collection.destination.entering-counter-kind");
    let counter = super::recognize_counter_name(cursor)?;
    cursor.enter("collection.destination.entering-counter-noun");
    let singular = count == super::NumericExpression::from(1);
    cursor.consume(if singular { "counter" } else { "counters" })?;
    cursor.enter("collection.destination.entering-counter-holder");
    cursor.consume("on")?;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("them"))
    {
        cursor.consume("them")?;
    } else {
        cursor.consume("it")?;
    }
    Ok(EnteringCountersNode { counter, count })
}

/// The entry modifiers one destination admits, read in any written order
/// (`tapped and transformed under your control with a +1/+1 counter on
/// it`). Each modifier is read once; a repeated or inapplicable one fails.
pub(in crate::oracle::canonical) struct EntryModifiers {
    pub tapped: bool,
    pub face_down: bool,
    pub transformed: bool,
    /// The controller of the entering permanents; the destination owner
    /// until an explicit `under X's control` replaces it (CR 110.2a).
    pub controller: CardMoveOwnerNode,
    pub entering_counters: Option<EnteringCountersNode>,
    pub attached_to: Option<EntryAttachmentNode>,
    /// `tapped and attacking`: read only right after `tapped and`.
    pub attacking: Option<EnteringAttackNode>,
    explicit_controller: bool,
}

impl EntryModifiers {
    pub(in crate::oracle::canonical) fn new(controller: CardMoveOwnerNode) -> Self {
        Self {
            tapped: false,
            face_down: false,
            transformed: false,
            controller,
            entering_counters: None,
            attached_to: None,
            attacking: None,
            explicit_controller: false,
        }
    }
}

/// A modifier word that may follow `and` inside the modifier list. Any other
/// `and` belongs to the parent's connector. `attacking` is read only as the
/// `tapped and attacking` pair (CR 508.4), never on its own.
fn entry_modifier_starts(cursor: &OracleTokenCursor<'_>, offset: usize, battlefield: bool) -> bool {
    match cursor
        .peek_ahead(offset)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("tapped" | "transformed") => battlefield,
        Some("face") => cursor
            .peek_ahead(offset + 1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("down") || raw.eq_ignore_ascii_case("up")),
        _ => false,
    }
}

/// `[tapped] [transformed] [face down|face up] [under X's control] [with N
/// KIND counters on it|them]` after a battlefield destination, and `[face
/// down]` after exile. The list is a child of the destination for any moved
/// operand; the consuming lowerer proves each modifier for its object.
pub(in crate::oracle::canonical) fn recognize_entry_modifiers(
    cursor: &mut OracleTokenCursor<'_>,
    zone: CardZoneName,
    modifiers: &mut EntryModifiers,
) -> MachineResult<()> {
    let battlefield = zone == CardZoneName::Battlefield;
    loop {
        // `tapped and transformed`: the conjunction joins two modifiers.
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
            && entry_modifier_starts(cursor, 1, battlefield)
        {
            cursor.consume("and")?;
        }
        match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
            Some("tapped") if battlefield => {
                cursor.enter("collection.destination.entry.tapped");
                cursor.require(!modifiers.tapped, "one `tapped` entry modifier")?;
                cursor.consume("tapped")?;
                modifiers.tapped = true;
                // `tapped and attacking` (CR 508.4): the pair enters combat.
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                    && cursor
                        .peek_ahead(1)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("attacking"))
                {
                    cursor.consume("and")?;
                    modifiers.attacking = Some(recognize_entering_attack(cursor)?);
                }
            }
            // `tapped and attacking` after another destination: only a
            // permanent can attack (CR 506.3a).
            Some("tapped")
                if cursor
                    .peek_ahead(1)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                    && cursor
                        .peek_ahead(2)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("attacking")) =>
            {
                cursor.enter("collection.destination.entry.attacking.zone");
                return Err(cursor.fail("the battlefield as the destination of an attacking entry"));
            }
            Some("transformed") if battlefield => {
                cursor.enter("collection.destination.entry.transformed");
                cursor.require(
                    !modifiers.transformed && !modifiers.face_down,
                    "one `transformed` entry modifier, never with a face-down entry",
                )?;
                cursor.consume("transformed")?;
                modifiers.transformed = true;
            }
            Some("face")
                if cursor
                    .peek_ahead(1)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("down")) =>
            {
                cursor.enter("collection.destination.entry.face-down");
                cursor.require(
                    !modifiers.face_down && !modifiers.transformed,
                    "one `face down` entry modifier, never with a transformed entry",
                )?;
                cursor.consume_phrase(&["face", "down"])?;
                modifiers.face_down = true;
            }
            // A card is put onto the battlefield face up unless an
            // instruction says otherwise (CR 708.1): `face up` restates it.
            Some("face")
                if battlefield
                    && cursor
                        .peek_ahead(1)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("up")) =>
            {
                cursor.enter("collection.destination.entry.face-up");
                cursor.require(!modifiers.face_down, "one face orientation")?;
                cursor.consume_phrase(&["face", "up"])?;
            }
            Some("under") if battlefield => {
                cursor.enter("collection.destination.controller");
                cursor.require(
                    !modifiers.explicit_controller,
                    "one controller of the entering permanents",
                )?;
                cursor.consume("under")?;
                modifiers.controller = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                    Some("your") => {
                        cursor.consume("your")?;
                        CardMoveOwnerNode::Player(LocalPlayerReference::Controller)
                    }
                    Some(pronoun @ ("its" | "their")) => {
                        let pronoun = pronoun.to_owned();
                        cursor.consume(&pronoun)?;
                        let possessive = cursor
                            .peek()
                            .filter(|raw| {
                                ["owner's", "owner\u{2019}s", "owners'", "owners\u{2019}"]
                                    .iter()
                                    .any(|form| raw.eq_ignore_ascii_case(form))
                            })
                            .map(str::to_owned)
                            .ok_or_else(|| cursor.fail("owner's or owners'"))?;
                        cursor.consume(&possessive)?;
                        CardMoveOwnerNode::EachCardOwner
                    }
                    _ => {
                        return Err(cursor.fail("`your`, `its owner's` or `their owners'` control"));
                    }
                };
                cursor.consume("control")?;
                modifiers.explicit_controller = true;
            }
            Some("with") if battlefield => {
                cursor.require(
                    modifiers.entering_counters.is_none(),
                    "one entering-counter placement",
                )?;
                modifiers.entering_counters = Some(recognize_entering_counters(cursor)?);
            }
            Some("attached")
                if battlefield
                    && cursor
                        .peek_ahead(1)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("to")) =>
            {
                cursor.require(modifiers.attached_to.is_none(), "one attachment")?;
                modifiers.attached_to = Some(recognize_entry_attachment(cursor)?);
            }
            _ => return Ok(()),
        }
    }
}

/// `attached to this NOUN|it|that NOUN|target NOUN|a NOUN you control`.
fn recognize_entry_attachment(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EntryAttachmentNode> {
    cursor.enter("collection.destination.entry.attached-to");
    cursor.consume_phrase(&["attached", "to"])?;
    // `this creature`, `this Equipment`: the ability's own object.
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
        && cursor.peek_ahead(1).is_some_and(|noun| {
            crate::oracle::characteristic_vocabulary::lookup_object_selector_lexeme(noun).is_some()
                || crate::oracle::characteristic_vocabulary::lookup_noncreature_subtype(noun)
                    .is_some()
        })
    {
        let noun = cursor.peek_ahead(1).unwrap_or("permanent").to_owned();
        cursor.consume("this")?;
        cursor.consume(&noun)?;
        return Ok(EntryAttachmentNode::Source);
    }
    let operand = super::recognize_verb_object_operand(
        cursor,
        super::VerbObjectOperandExpectation {
            counts: super::TargetCountSupport::Expressed,
            card_zones: false,
        },
    )?;
    cursor.enter("collection.destination.entry.attached-to.operand");
    match operand {
        super::VerbObjectOperandNode::Reference(reference) => {
            Ok(EntryAttachmentNode::Reference(reference))
        }
        super::VerbObjectOperandNode::Targets(targets)
            if targets.slots.len() == 1
                && targets.slots[0].cardinality.maximum == 1
                && targets.slots[0].count.is_none()
                && targets.slots[0].selector.is_permanent_target() =>
        {
            Ok(EntryAttachmentNode::Target(targets))
        }
        super::VerbObjectOperandNode::Selection(selection)
            if selection.bounds == Some(super::ExpressedTargetCardinality::EXACTLY_ONE) =>
        {
            Ok(EntryAttachmentNode::Chosen(selection))
        }
        _ => Err(cursor.fail("one permanent the entering object is attached to")),
    }
}

pub(in crate::oracle::canonical) type CardMoveDestinationAttempt =
    CollectionOperandAttempt<CardMoveDestinationNode>;

/// `back [on top of LIBRARY] [in any order|in a random order]`: the moved
/// cards return to the top of the library they were observed in (`look at
/// the top five cards of your library, then put them back in any order`).
fn recognize_back_destination(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardMoveDestinationNode> {
    cursor.enter("collection.destination.back");
    cursor.consume("back")?;
    let mut destination = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("on"))
    {
        // `back on top of your library`: the restated library destination.
        cursor.enter("collection.destination.back.restated");
        match try_recognize_card_move_destination(cursor) {
            CollectionOperandAttempt::Recognized(fragment) => fragment.node,
            CollectionOperandAttempt::NotClaimed => {
                return Err(cursor.fail("`on top of` the library the cards came from"));
            }
            CollectionOperandAttempt::RejectedCandidate(failure)
            | CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
        }
    } else {
        let order = if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("in"))
        {
            cursor.enter("collection.destination.order");
            Some(recognize_collection_order(cursor)?)
        } else {
            None
        };
        CardMoveDestinationNode {
            zone: CardZoneName::Library,
            owner: CardMoveOwnerNode::EachCardOwner,
            position: Some(LibraryPosition::Top),
            order,
            tapped: false,
            entering_counters: None,
            face_down: false,
            transformed: false,
            shuffle: false,
            attached_to: None,
            origin_library: false,
            attacking: None,
        }
    };
    cursor.enter("collection.destination.back.library-top");
    cursor.require(
        destination.zone == CardZoneName::Library
            && destination.position == Some(LibraryPosition::Top),
        "the top of the library the cards were put back on",
    )?;
    destination.origin_library = true;
    Ok(destination)
}

/// Reads a destination phrase, never the move action or its object binding.
pub(in crate::oracle::canonical) fn try_recognize_card_move_destination(
    cursor: &mut OracleTokenCursor<'_>,
) -> CardMoveDestinationAttempt {
    let mut trial = cursor.clone();
    if trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("back"))
    {
        return match recognize_back_destination(&mut trial) {
            Ok(node) => {
                trial.enter("collection.destination.complete");
                let end_position = trial.position();
                *cursor = trial;
                CollectionOperandAttempt::Recognized(RecognizedEffectFragment {
                    node,
                    end_position,
                })
            }
            Err(failure) => CollectionOperandAttempt::FailedCommitted(failure),
        };
    }
    let relation = match trial.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("into") => "into",
        Some("to") => "to",
        Some("on") => "on",
        Some("onto") => "onto",
        _ => return CollectionOperandAttempt::NotClaimed,
    };
    let result = (|| -> MachineResult<CardMoveDestinationNode> {
        trial.enter("collection.destination.relation");
        trial.consume(relation)?;
        let surface = relation == "on" || relation == "onto";
        if trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        {
            trial.consume("the")?;
        }
        let mut position = if surface
            && trial.peek().is_some_and(|raw| {
                raw.eq_ignore_ascii_case("top") || raw.eq_ignore_ascii_case("bottom")
            }) {
            let position = recognize_library_position_kind(&mut trial)?;
            trial.consume("of")?;
            Some(position)
        } else {
            None
        };
        trial.enter("collection.destination.owner");
        // Battlefield control comes from the instruction's actor (CR 110.2a).
        // Other zones require an explicit possessive; bare exile is not guessed.
        let battlefield = trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("battlefield"));
        let mut owned_zone_only = false;
        let mut origin_library = false;
        // The grammatical number of the possessor: `your graveyard`, `its
        // owner's hand` name one zone; `their owners' hands` several. A bare
        // `their` agrees with either.
        let mut plural_possessor = Some(false);
        let owner = if !battlefield {
            if trial
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
                && trial
                    .peek_ahead(1)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("library"))
            {
                // `on the bottom of that library`: the library the moved
                // cards were observed in, which is their owner's (CR 400.3).
                trial.consume("that")?;
                origin_library = true;
                CardMoveOwnerNode::EachCardOwner
            } else if trial
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("its"))
            {
                trial.consume("its")?;
                let owner = trial
                    .peek()
                    .filter(|raw| {
                        raw.eq_ignore_ascii_case("owner's")
                            || raw.eq_ignore_ascii_case("owner\u{2019}s")
                    })
                    .map(str::to_owned)
                    .ok_or_else(|| trial.fail("owner's"))?;
                trial.consume(&owner)?;
                CardMoveOwnerNode::EachCardOwner
            } else if trial
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("their"))
            {
                // `their owner's hand` / `their owners' hands`: the plural
                // object pronoun keeps the per-object owner relation.
                trial.consume("their")?;
                let owner = trial
                    .peek()
                    .filter(|raw| {
                        ["owner's", "owner\u{2019}s", "owners'", "owners\u{2019}"]
                            .iter()
                            .any(|form| raw.eq_ignore_ascii_case(form))
                    })
                    .map(str::to_owned);
                plural_possessor = match owner.as_deref() {
                    Some(form) => Some(form.to_ascii_lowercase().starts_with("owners")),
                    None => None,
                };
                match owner {
                    Some(owner) => trial.consume(&owner)?,
                    // `to their hand`, `on top of their library`: a player's
                    // possessive naming the cards' owner. A hand, library or
                    // graveyard is always its owner's (CR 400.3), checked
                    // with the zone below.
                    None => owned_zone_only = true,
                }
                CardMoveOwnerNode::EachCardOwner
            } else {
                trial.consume("your")?;
                CardMoveOwnerNode::Player(LocalPlayerReference::Controller)
            }
        } else {
            CardMoveOwnerNode::Player(LocalPlayerReference::Controller)
        };
        trial.enter("collection.destination.zone");
        let plural_zone = trial.peek().is_some_and(|raw| {
            ["graveyards", "libraries", "hands"]
                .iter()
                .any(|noun| raw.eq_ignore_ascii_case(noun))
        });
        let zone = recognize_card_zone_atom(&mut trial)?;
        trial.require(
            battlefield || plural_possessor.is_none_or(|plural| plural == plural_zone),
            "a zone noun agreeing in number with its possessor",
        )?;
        // `return ... to the battlefield` names the same battlefield entry as
        // `put ... onto the battlefield`; `into the battlefield` is not Oracle.
        trial.require(if surface { zone == CardZoneName::Battlefield || (zone == CardZoneName::Library && position.is_some()) } else { (zone != CardZoneName::Battlefield || relation == "to") && position.is_none() }, "on/onto/to the battlefield or a positioned library; into/to an explicitly owned card zone")?;
        trial.require(
            position.is_none() || zone == CardZoneName::Library,
            "a top/bottom position only for a library",
        )?;
        trial.require(
            !owned_zone_only
                || matches!(
                    zone,
                    CardZoneName::Hand | CardZoneName::Library | CardZoneName::Graveyard
                ),
            "a hand, library or graveyard after a bare player possessive",
        )?;
        trial.require(
            !origin_library || zone == CardZoneName::Library,
            "`that` naming a library",
        )?;
        if zone == CardZoneName::Library && position.is_none() {
            // `into its owner's library second from the top`: the shared
            // library-position child reads a place beneath the top cards.
            position = super::collection::try_recognize_beneath_top_position(&mut trial)?;
        }
        let mut modifiers = EntryModifiers::new(owner);
        if matches!(zone, CardZoneName::Battlefield | CardZoneName::Exile) {
            recognize_entry_modifiers(&mut trial, zone, &mut modifiers)?;
        }
        let EntryModifiers {
            tapped,
            face_down,
            transformed,
            controller: owner,
            entering_counters,
            attached_to,
            attacking,
            ..
        } = modifiers;
        let order = if trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("in"))
        {
            trial.enter("collection.destination.order");
            trial.require(
                zone == CardZoneName::Library,
                "library ordering, not ordering a hand, graveyard, exile or battlefield",
            )?;
            Some(recognize_collection_order(&mut trial)?)
        } else {
            None
        };
        Ok(CardMoveDestinationNode {
            zone,
            owner,
            position,
            order,
            tapped,
            entering_counters,
            face_down,
            transformed,
            shuffle: false,
            attached_to,
            origin_library,
            attacking,
        })
    })();
    match result {
        Ok(node) => {
            trial.enter("collection.destination.complete");
            let end_position = trial.position();
            *cursor = trial;
            CollectionOperandAttempt::Recognized(RecognizedEffectFragment { node, end_position })
        }
        Err(f) => CollectionOperandAttempt::FailedCommitted(f),
    }
}

/// A destination written before the moved object (`return to the
/// battlefield all creature cards ...`, `put onto the battlefield ...`).
/// An object operand never begins with `to`/`onto`, so this claims only a
/// preposed destination; once claimed, a malformed destination fails.
pub(in crate::oracle::canonical) fn try_recognize_preposed_card_move_destination(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<CardMoveDestinationNode>> {
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("to") || raw.eq_ignore_ascii_case("onto"))
    {
        return Ok(None);
    }
    cursor.enter("collection.destination.preposed");
    match try_recognize_card_move_destination(cursor) {
        CollectionOperandAttempt::Recognized(fragment) => Ok(Some(fragment.node)),
        CollectionOperandAttempt::NotClaimed => Ok(None),
        CollectionOperandAttempt::RejectedCandidate(failure)
        | CollectionOperandAttempt::FailedCommitted(failure) => Err(failure),
    }
}

#[cfg(test)]
#[path = "card_move_destination_tests.rs"]
mod tests;
