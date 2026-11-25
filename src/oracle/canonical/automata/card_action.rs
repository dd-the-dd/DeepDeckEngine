//! Atomic actions over a previously observed card collection. No sentence recipes.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardOperandNode {
    Reference(CardCollectionReferenceNode),
    Selection(CardSelectionOperandNode),
    ZoneCollection(CardZoneCollectionNode),
    Targets(CardTargetOperandNode),
    LibraryTop(CardLibraryTopNode),
    /// `the copy` / `the copies` / `any number of the copies`: the card
    /// copies the preceding copy action of this program created (CR
    /// 707.12). `plural` admits every copy, each cast or not (CR 707.12a).
    CardCopies {
        plural: bool,
    },
    /// A choice among cards of a hidden zone or from outside the game.
    HiddenChoice(HiddenCardChoiceNode),
    /// `your commander from the command zone` (CR 408.1, 903.3): the
    /// controller's cards in their own command zone matching the predicate.
    /// The command zone holds only the cards its holder owns there, so the
    /// predicate keeps the commander designation and its ownership.
    CommandZone {
        predicate: ObjectPredicate,
    },
    /// A card-collection pronoun (`it`, `that card`, `them`, `those cards`)
    /// read where the binding environment names a typed object antecedent:
    /// the ability's source, a declared target, the event's object, the
    /// object an earlier action moved, or a set it produced. The lowerer
    /// reads the program's observed card collection when it has one (the
    /// collection is the nearer antecedent of a card pronoun) and the typed
    /// object otherwise, as the card that object is or became (CR 400.7).
    ObjectAntecedent {
        reference: CardCollectionReferenceNode,
        object: PermanentEffectReference,
    },
}

/// The frame facts a card action reads from its enclosing program: whether
/// the ability source can answer numeric reads (a resolution-owned query may
/// then compare with its counters or characteristics, CR 608.2h), and the
/// name links a source-control window resolves a printed name with.
#[derive(Clone, Copy)]
pub(in crate::oracle::canonical) struct CardActionReadContext<'a> {
    pub source_available: bool,
    pub preposed_duration: Option<&'a EffectDurationNode>,
    pub name_links: &'a [crate::oracle::syntax::ResolvedNameLink],
    pub name_failure: Option<&'a crate::oracle::syntax::NameResolutionFailure>,
}

impl<'a> CardActionReadContext<'a> {
    /// The facts of one effect program position.
    pub(in crate::oracle::canonical) fn of(
        context: &'a EffectRecognitionContext,
        name_links: &'a [crate::oracle::syntax::ResolvedNameLink],
        name_failure: Option<&'a crate::oracle::syntax::NameResolutionFailure>,
    ) -> Self {
        Self {
            source_available: context.source.is_some()
                || context.activation_source_availability
                    == ExplicitActivationSourceAvailability::RetainedBattlefieldPermanent,
            preposed_duration: context.preposed_duration.as_ref(),
            name_links,
            name_failure,
        }
    }

    fn without_preposed_duration(self) -> Self {
        Self {
            preposed_duration: None,
            ..self
        }
    }
}

/// Where the cards of a hidden choice are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum HiddenCardSourceNode {
    /// `from outside the game` (CR 400.11a: the sideboard). Only cards the
    /// choosing player owns are there.
    OutsideGame,
    /// `from your|target player's hand|graveyard`.
    Zone {
        zone: CardZoneName,
        owner: CardZoneOwnerNode,
    },
}

/// `QUANTITY PREDICATE card(s) you own from outside the game` or
/// `QUANTITY PREDICATE card(s) at random from ZONE`: a quantity, a card
/// predicate, a source and a selection policy as independent slots. The
/// chosen cards are the observed collection of a later `it`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct HiddenCardChoiceNode {
    pub quantity: CardSelectionQuantityNode,
    pub predicate: ObjectPredicate,
    pub source: HiddenCardSourceNode,
    /// `at random`: the engine selects with its replayable random source.
    pub random: bool,
}

/// The base-cost policy of a cast during resolution or of a casting
/// permission (CR 118.9, 601.2f).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardCastCostNode {
    /// The spell's mana cost and every other cost.
    Printed,
    /// `without paying its|their mana cost(s)` (CR 118.9a).
    WithoutManaCost,
    /// `by paying COST rather than paying its mana cost` (CR 118.9).
    Alternative(ManaCostNode),
    /// `by paying life equal to its mana value | by paying an amount of
    /// {E} equal to its mana value | by discarding a card rather than paying
    /// its mana cost` (CR 118.9): an alternative cost that is not mana.
    AlternativeNonMana(NonManaCastCostNode),
}

/// A non-mana alternative base cost of a cast frame (CR 118.9). The amount
/// equal to the spell's mana value is the mana value of what is actually
/// cast (CR 202.3), evaluated when the cost is determined (CR 601.2f).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum NonManaCastCostNode {
    /// `paying life equal to its|the spell's|that spell's mana value`.
    LifeEqualToManaValue,
    /// `paying an amount of {E} equal to its mana value` (CR 107.14, 122.1).
    PlayerCountersEqualToManaValue(crate::oracle::counter_name::CounterName),
    /// `discarding N card(s)` (CR 701.9).
    Discard(NumericExpression),
}

/// Whose graveyard a cast spell's exile replacement watches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CastGraveyardOwnerNode {
    /// `your graveyard`: only a spell the controller owns (CR 404.1).
    Controller,
    /// `a graveyard`, `their graveyard`, `its owner's graveyard`.
    Any,
}

/// `If that spell|those spells|a [PREDICATE] spell cast this way would be
/// put into GRAVEYARD, exile it|them instead.` (CR 614.1a): a replacement of
/// the graveyard destination of each spell the action casts, from wherever
/// the spell would go there (resolution or a counter).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CastGraveyardExileNode {
    /// The spells cast this way the replacement applies to; `None` is all.
    pub predicate: Option<ObjectPredicate>,
    pub graveyard: CastGraveyardOwnerNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardPermissionVerb {
    Play,
    Cast,
}

/// A permission window that the shared effect duration does not name: it
/// ends on a zone change of the permitted cards or on a later event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardPermissionWindowNode {
    /// `for as long as it|that card|they|those cards remain(s) exiled`: each
    /// permission ends when its card leaves exile (CR 400.7, 611.2b).
    WhileInExile,
    /// `until your next end step`: through the beginning of the controller's
    /// next end step.
    UntilYourNextEndStep,
    /// `until you exile another card with this NOUN`: the next card the source
    /// exiles replaces the permission (CR 607.2a linked exile).
    UntilSourceExilesAnother,
    /// `until [the beginning of] your next upkeep`: it ends as the
    /// controller's next upkeep begins (CR 503.1). No player receives
    /// priority in the untap step before it (CR 502.4), so the window and
    /// `until your next turn` admit the same plays.
    UntilYourNextUpkeep,
}

/// One cast/play frame (CR 601.2, 608.2g) with independent slots:
/// optionality (`you may cast` or an imperative `cast` performed if able),
/// the cards (a bound card, declared card targets, a card query with its
/// origin, or the copies a copy action created), the base-cost policy, the
/// window and the graveyard exile replacement of the spells cast. With a
/// window it is a player permission; without one the cards are cast while
/// the effect resolves. A trailing `if CONDITION` stays with the sequence's
/// postposed condition. The static self-cast permission of a card is a
/// different frame (`ability-frame.permission`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardPermissionNode {
    pub verb: CardPermissionVerb,
    pub cards: CardOperandNode,
    pub duration: Option<EffectDurationNode>,
    /// A window the shared duration does not name, trailing or preposed.
    /// Never present together with `duration`.
    pub window: Option<CardPermissionWindowNode>,
    /// `you may` (true) or an imperative `cast` (false), which the player
    /// performs if able (CR 608.2g).
    pub optional: bool,
    /// The base-cost policy. Without a window, a free cast happens while
    /// the ability resolves (CR 608.2g), not later.
    pub cost: CardCastCostNode,
    /// `, and mana of any type can be spent to cast that spell|it|them`: a
    /// spending policy of the permitted casts (CR 609.4b).
    pub mana_of_any_type: bool,
    /// The following `If that spell would be put into a graveyard, exile it
    /// instead.` sentence, which amends exactly the spells this frame casts.
    pub graveyard_exile: Option<CastGraveyardExileNode>,
}

impl CardPermissionNode {
    /// `without paying its|their mana cost(s)`.
    pub(in crate::oracle::canonical) fn without_paying_mana_cost(&self) -> bool {
        self.cost == CardCastCostNode::WithoutManaCost
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardActionNode {
    Look {
        count: NumericExpression,
        owner: LocalPlayerReference,
    },
    /// `look at OPERAND` over another player's hidden zone: the top cards of
    /// a declared player's library or a declared player's hand. Only the
    /// looking player sees the cards; they become the observed collection.
    LookAt(CardOperandNode),
    Reveal(CardOperandNode),
    /// `reveal|exile cards from the top of your library until you
    /// reveal|exile QUANTITY PREDICATE`: every revealed or exiled card is the
    /// observed collection; the last QUANTITY matching cards are the
    /// antecedent of `that card`. `verb` is the action performed on each
    /// card (CR 701.20a reveal, CR 406.3 exile).
    RevealUntil {
        quantity: NumericExpression,
        predicate: ObjectPredicate,
        owner: LocalPlayerReference,
        verb: CardCollectionVerb,
    },
    Move {
        cards: CardOperandNode,
        destination: CardMoveDestinationNode,
    },
    Permit(CardPermissionNode),
    /// `copy CARD [twice|N times]` (CR 707.12): copies of a bound card in
    /// the zone it is in. The copies are the antecedent of `the copy` /
    /// `the copies`, which the following cast frame consumes.
    Copy {
        cards: CardOperandNode,
        count: NumericExpression,
    },
    /// `[you] choose SELECTION` (CR 608.2d): the controller chooses cards from
    /// an observed collection while the effect resolves. The choice is the
    /// singular or plural antecedent of a later `that card`, `it` or `them`.
    Choose(CardSelectionOperandNode),
    /// `[you] choose QUANTITY QUERY card(s) in|from ZONE` (CR 608.2d): the
    /// controller chooses cards of a named zone while the effect resolves.
    /// The chosen cards are the observed collection (the chosen-card binding)
    /// a later `that card`, `it` or `them` names, in that zone.
    ChooseFromZone(CardZoneCollectionNode),
    /// `choose target QUERY card(s) in ZONE` (CR 115.1, 601.2c): the
    /// instruction is the card target declaration itself; the declared
    /// cards are the collection a later `that card`, `it`, `them` or `the
    /// chosen card(s)` names, in that zone.
    ChooseTargetCards(CardTargetOperandNode),
    /// `CARDS gain(s) flashback until end of turn` (CR 702.34a): a keyword
    /// the cards have where they are, fixed on resolution (CR 611.2c).
    GrantKeyword(CardKeywordGrantNode),
    /// `shuffle [your library]` (CR 701.20a).
    Shuffle {
        owner: LocalPlayerReference,
    },
}

fn operand(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<CardOperandNode> {
    if let Some(copies) = try_recognize_card_copies(cursor)? {
        return Ok(copies);
    }
    match try_recognize_hidden_card_choice(cursor) {
        CollectionOperandAttempt::Recognized(fragment) => {
            return Ok(CardOperandNode::HiddenChoice(fragment.node));
        }
        CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
        CollectionOperandAttempt::NotClaimed | CollectionOperandAttempt::RejectedCandidate(_) => {}
    }
    match try_recognize_card_target_operand(cursor) {
        CollectionOperandAttempt::Recognized(fragment) => {
            return Ok(CardOperandNode::Targets(fragment.node));
        }
        CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
        CollectionOperandAttempt::NotClaimed | CollectionOperandAttempt::RejectedCandidate(_) => {}
    }
    match try_recognize_card_library_position(cursor, true) {
        CollectionOperandAttempt::Recognized(fragment) => {
            return Ok(CardOperandNode::LibraryTop(fragment.node));
        }
        CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
        CollectionOperandAttempt::NotClaimed | CollectionOperandAttempt::RejectedCandidate(_) => {}
    }
    match try_recognize_card_zone_collection(cursor) {
        CollectionOperandAttempt::Recognized(fragment) => {
            return Ok(CardOperandNode::ZoneCollection(fragment.node));
        }
        CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
        CollectionOperandAttempt::NotClaimed | CollectionOperandAttempt::RejectedCandidate(_) => {}
    }
    match try_recognize_card_collection_reference(cursor) {
        CardCollectionReferenceAttempt::Recognized(fragment) => {
            // `the exiled cards not cast this way`: every member of the
            // restated collection minus what a preceding cast cast.
            if matches!(
                fragment.node,
                CardCollectionReferenceNode::Antecedent
                    | CardCollectionReferenceNode::Exiled
                    | CardCollectionReferenceNode::Revealed
            ) && recognize_uncast_relative_clause(cursor)?
            {
                return Ok(CardOperandNode::Selection(CardSelectionOperandNode {
                    quantity: CardSelectionQuantityNode::All {
                        via: None,
                        except_cast: true,
                    },
                    predicate: None,
                    from: fragment.node,
                }));
            }
            if let Some(object) = card_pronoun_object_antecedent(cursor, fragment.node) {
                return Ok(CardOperandNode::ObjectAntecedent {
                    reference: fragment.node,
                    object,
                });
            }
            return Ok(CardOperandNode::Reference(fragment.node));
        }
        CardCollectionReferenceAttempt::FailedCommitted(failure) => return Err(failure),
        CardCollectionReferenceAttempt::NotClaimed
        | CardCollectionReferenceAttempt::RejectedCandidate(_) => {}
    }
    match try_recognize_card_selection_operand(cursor) {
        CardSelectionOperandAttempt::Recognized(fragment) => {
            Ok(CardOperandNode::Selection(fragment.node))
        }
        CardSelectionOperandAttempt::RejectedCandidate(failure)
        | CardSelectionOperandAttempt::FailedCommitted(failure) => Err(failure),
        CardSelectionOperandAttempt::NotClaimed => {
            Err(cursor.fail("a collection reference or quantified card query"))
        }
    }
}

/// The typed object a card-collection pronoun names through the one binding
/// environment. A singular pronoun reads the shared pronoun resolver (the
/// source, a declared target, the event's object, the card a move put onto
/// the battlefield), else the one permanent an earlier sacrifice put into a
/// graveyard (a card action acts on the card it became, CR 400.7). A plural
/// pronoun reads the declared target set or the created tokens. Every
/// card-collection producer (look, reveal, search, mill, draw) clears these
/// antecedents or is read first by the lowerer, so an observed collection
/// stays the pronoun's nearer antecedent. When the event moved its object
/// into a public zone, the frame's event card answers instead. Nothing is
/// guessed: without such an antecedent the operand stays a collection
/// reference whose lowering requires an observation.
fn card_pronoun_object_antecedent(
    cursor: &OracleTokenCursor<'_>,
    reference: CardCollectionReferenceNode,
) -> Option<PermanentEffectReference> {
    let environment = cursor.binding_environment()?;
    if environment.frame.event_card.is_some() {
        return None;
    }
    match reference {
        CardCollectionReferenceNode::Selected => {
            resolve_pronoun_antecedent(environment)
                .ok()
                // A declared target that need not be a permanent (`counter target
                // spell`) is never a card operand: a spell that left the stack is
                // a new object (CR 400.7).
                .filter(|reference| {
                    !environment.declared_target.as_ref().is_some_and(|target| {
                        *reference == PermanentEffectReference::Target(target.binding)
                    })
                })
                .or_else(|| {
                    environment
                        .result_object
                        .as_ref()
                        .filter(|result| {
                            result.number == GrammaticalNumber::Singular
                                && result.verb == Some(ActionResultVerb::Sacrificed)
                        })
                        .map(ActionResultObject::permanent_reference)
                })
        }
        CardCollectionReferenceNode::Antecedent => environment
            .objects
            .as_ref()
            .filter(|set| set.number == GrammaticalNumber::Plural)
            .and_then(|set| match &set.denotes {
                ObjectSetDenotation::Bound(reference) => Some(*reference),
                ObjectSetDenotation::Collection { .. } => None,
            }),
        CardCollectionReferenceNode::Remainder
        | CardCollectionReferenceNode::Other
        | CardCollectionReferenceNode::Zone
        | CardCollectionReferenceNode::Exiled
        | CardCollectionReferenceNode::Linked
        | CardCollectionReferenceNode::Revealed
        | CardCollectionReferenceNode::Restated(_) => None,
    }
}

/// `None` means this child made no claim; candidate failure is transactional at
/// the sequence parent. Ellipsis is admitted only after a preceding move action.
pub(in crate::oracle::canonical) fn recognize_card_action(
    cursor: &mut OracleTokenCursor<'_>,
    move_ellipsis: bool,
    frame: CardActionReadContext<'_>,
) -> MachineResult<Option<CardActionNode>> {
    let supplied_duration = frame.preposed_duration;
    let Some(head) = cursor.peek() else {
        return Ok(None);
    };
    if card_keyword_grant_follows(cursor) {
        // `target instant card in your graveyard gains flashback until end
        // of turn`: the card operand is the clause subject. Committed once
        // the keyword is read; an unread subject leaves the clause to others.
        let mut trial = cursor.clone();
        let start = trial.position();
        match recognize_card_keyword_grant(&mut trial, operand) {
            Ok(node) => {
                *cursor = trial;
                return Ok(Some(CardActionNode::GrantKeyword(node)));
            }
            Err(failure)
                if trial
                    .token_slice(start, trial.position())
                    .iter()
                    .any(|token| token.raw.eq_ignore_ascii_case("flashback")) =>
            {
                return Err(failure);
            }
            Err(_) => {}
        }
    }
    if permission_head_starts(cursor) || imperative_cast_starts(cursor) {
        return recognize_card_permission(cursor, frame, None)
            .map(|node| Some(CardActionNode::Permit(node)));
    }
    if card_copy_starts(cursor) {
        return recognize_card_copy(cursor).map(Some);
    }
    if head.eq_ignore_ascii_case("for") || head.eq_ignore_ascii_case("until") {
        // `For as long as that card remains exiled, you may play it.` /
        // `Until your next end step, you may play those cards.`: a window
        // this child claims only when a complete, comma-closed window is
        // followed by a permission head; every other preposed phrase stays
        // with the sequence's own duration introducer.
        let Some((mut trial, window)) = try_preposed_permission_window(cursor) else {
            return Ok(None);
        };
        cursor.require(
            supplied_duration.is_none(),
            "one preposed window for one permission",
        )?;
        let node =
            recognize_card_permission(&mut trial, frame.without_preposed_duration(), Some(window))?;
        *cursor = trial;
        return Ok(Some(CardActionNode::Permit(node)));
    }
    if head.eq_ignore_ascii_case("choose")
        || (head.eq_ignore_ascii_case("you")
            && cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("choose")))
    {
        // Only a selection from an observed collection belongs here; every
        // other `choose` domain stays with the choose-instruction frame.
        let mut trial = cursor.clone();
        trial.enter("card-action.choose.verb");
        if head.eq_ignore_ascii_case("you") {
            trial.consume("you")?;
        }
        trial.consume("choose")?;
        // `choose target artifact card in your graveyard`: a card target
        // declaration; a permanent target stays with the choose frame.
        trial.enter("card-action.choose.target-cards");
        match try_recognize_card_target_operand(&mut trial) {
            CollectionOperandAttempt::Recognized(fragment) => {
                *cursor = trial;
                return Ok(Some(CardActionNode::ChooseTargetCards(fragment.node)));
            }
            CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
            CollectionOperandAttempt::NotClaimed
            | CollectionOperandAttempt::RejectedCandidate(_) => {}
        }
        trial.enter("card-action.choose.selection");
        match try_recognize_card_selection_operand(&mut trial) {
            CardSelectionOperandAttempt::Recognized(fragment) => {
                *cursor = trial;
                return Ok(Some(CardActionNode::Choose(fragment.node)));
            }
            CardSelectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
            CardSelectionOperandAttempt::NotClaimed
            | CardSelectionOperandAttempt::RejectedCandidate(_) => {}
        }
        // `choose an instant or sorcery card in your graveyard`: a chosen
        // quantity of a named zone's cards, read by the shared zone
        // collection. `all cards in ZONE` chooses nothing and stays unread.
        trial.enter("card-action.choose.zone");
        return match try_recognize_card_zone_collection(&mut trial) {
            CollectionOperandAttempt::Recognized(fragment) if fragment.node.quantity.is_some() => {
                *cursor = trial;
                Ok(Some(CardActionNode::ChooseFromZone(fragment.node)))
            }
            CollectionOperandAttempt::FailedCommitted(failure) => Err(failure),
            CollectionOperandAttempt::Recognized(_)
            | CollectionOperandAttempt::NotClaimed
            | CollectionOperandAttempt::RejectedCandidate(_) => Ok(None),
        };
    }
    if head.eq_ignore_ascii_case("shuffle") {
        cursor.enter("card-action.shuffle");
        cursor.consume("shuffle")?;
        if super::shuffle_object_follows(cursor) {
            // `shuffle OBJECT into LIBRARY` (CR 701.24b): the shared card
            // operand, then the library destination that owns the shuffle.
            cursor.enter("card-action.shuffle.object");
            let cards = operand(cursor)?;
            let destination = super::recognize_shuffle_library_destination(cursor)?;
            return Ok(Some(moved_cards(cards, destination)));
        }
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
        {
            cursor.consume_phrase(&["your", "library"])?;
        }
        return Ok(Some(CardActionNode::Shuffle {
            owner: LocalPlayerReference::Controller,
        }));
    }
    if head.eq_ignore_ascii_case("look") {
        return recognize_look(cursor).map(Some);
    }
    if is_verb_form(&head, VerbLemma::Reveal, VerbForm::Base) {
        cursor.consume(
            crate::oracle::canonical::automata::verb_entry(VerbLemma::Reveal)
                .surface(VerbForm::Base),
        )?;
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("cards"))
            && cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("from"))
        {
            return recognize_reveal_until(cursor, CardCollectionVerb::Revealed).map(Some);
        }
        return operand(cursor).map(|value| Some(CardActionNode::Reveal(value)));
    }
    if is_verb_form(&head, VerbLemma::Exile, VerbForm::Base) {
        cursor.consume(
            crate::oracle::canonical::automata::verb_entry(VerbLemma::Exile)
                .surface(VerbForm::Base),
        )?;
        // `exile cards from the top of your library until you exile ...`:
        // the same until-reader as reveal, with the exile action.
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("cards"))
            && cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("from"))
        {
            return recognize_reveal_until(cursor, CardCollectionVerb::Exiled).map(Some);
        }
        let cards = operand(cursor)?;
        let destination = recognize_card_action_destination(cursor, "exile")?;
        return Ok(Some(moved_cards(cards, destination)));
    }
    let explicit_move = is_verb_form(&head, VerbLemma::Put, VerbForm::Base)
        || is_verb_form(&head, VerbLemma::Return, VerbForm::Base);
    if !explicit_move && !move_ellipsis {
        return Ok(None);
    }
    if explicit_move {
        let verb = head.to_owned();
        cursor.consume(&verb)?;
        // `return to the battlefield all creature cards ...`: a heavy object
        // follows its destination (the same two children, reversed).
        if let Some(destination) = super::try_recognize_preposed_card_move_destination(cursor)? {
            let cards = operand(cursor)?;
            return Ok(Some(moved_cards(cards, destination)));
        }
    }
    let cards = operand(cursor)?;
    let destination = recognize_card_action_destination(cursor, "move")?;
    Ok(Some(moved_cards(cards, destination)))
}

/// The transactional outcome of the card-action child (S0, `ChildOutcome`
/// contract): a cast/play frame that proved it owns the clause (its
/// permitted query read `from`, or a tail only it reads follows its cards)
/// returns `FailedCommitted` with its own state, so the sequence parent
/// publishes that failure instead of `effect.start`. Every other failure is
/// a `RejectedCandidate` the parent may replace with another child's
/// reading; `NotClaimed` means no card action starts here.
///
/// The sequence parent (`automata/sequence.rs`) calls it in place of a
/// speculative `recognize_card_action` and commits its attempt frame on
/// `FailedCommitted`.
pub(in crate::oracle::canonical) fn try_recognize_card_action_outcome(
    cursor: &mut OracleTokenCursor<'_>,
    move_ellipsis: bool,
    frame: CardActionReadContext<'_>,
) -> CollectionOperandAttempt<CardActionNode> {
    let mut trial = cursor.clone();
    let result = if permission_head_starts(&trial) || imperative_cast_starts(&trial) {
        let mut committed = false;
        match recognize_card_permission_frame(&mut trial, frame, None, &mut committed) {
            Ok(node) => Ok(Some(CardActionNode::Permit(node))),
            Err(failure) if committed => {
                return CollectionOperandAttempt::FailedCommitted(failure);
            }
            Err(failure) => Err(failure),
        }
    } else {
        recognize_card_action(&mut trial, move_ellipsis, frame)
    };
    match result {
        Ok(Some(node)) => {
            let end_position = trial.position();
            *cursor = trial;
            CollectionOperandAttempt::Recognized(RecognizedEffectFragment { node, end_position })
        }
        Ok(None) => CollectionOperandAttempt::NotClaimed,
        Err(failure) => CollectionOperandAttempt::RejectedCandidate(failure),
    }
}

/// One card move. A declared target a pronoun puts onto the battlefield
/// (`exile target creature, then return that card to the battlefield`) is
/// the card an earlier action of the program moved out of play: a declared
/// permanent never re-enters the battlefield itself (CR 400.7). The pronoun
/// then names the program's observed card, as a collection reference.
fn moved_cards(cards: CardOperandNode, destination: CardMoveDestinationNode) -> CardActionNode {
    let cards = match cards {
        CardOperandNode::ObjectAntecedent {
            reference,
            object: PermanentEffectReference::Target(_) | PermanentEffectReference::TargetSet(_),
        } if destination.zone == CardZoneName::Battlefield => CardOperandNode::Reference(reference),
        cards => cards,
    };
    CardActionNode::Move { cards, destination }
}

/// `look at OPERAND`: the shared library-position operand (the controller's
/// or a declared player's library) or the shared zone collection naming a
/// declared player's whole hand. The controller's own library keeps the
/// historical `Look` node; every other owner is a private `LookAt`.
fn recognize_look(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<CardActionNode> {
    cursor.enter("card-action.look.source");
    cursor.consume_phrase(&["look", "at"])?;
    // `the top N cards of LIBRARY` or its counted form `N|that many cards
    // from the top of LIBRARY`: one shared library-position operand.
    cursor.enter("card-action.look.library");
    let top = match try_recognize_card_library_position(cursor, true) {
        CollectionOperandAttempt::Recognized(fragment) => Some(fragment.node),
        CollectionOperandAttempt::NotClaimed => None,
        CollectionOperandAttempt::RejectedCandidate(failure)
        | CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
    };
    if let Some(top) = top {
        return Ok(match top.owner {
            CardLibraryOwnerNode::Player(LocalPlayerReference::Controller) => {
                CardActionNode::Look {
                    count: top.count,
                    owner: LocalPlayerReference::Controller,
                }
            }
            _ => CardActionNode::LookAt(CardOperandNode::LibraryTop(top)),
        });
    }
    cursor.enter("card-action.look.zone");
    if let Some(player) = try_recognize_bound_possessive_player(cursor)? {
        // `look at that player's|defending player's hand`: the whole hand of
        // the player the binding environment names (CR 401.2, 402.3).
        cursor.enter("card-action.look.bound-hand");
        let zone = recognize_card_zone_atom(cursor)?;
        cursor.require(
            zone == CardZoneName::Hand,
            "a bound player's hand to look at",
        )?;
        return Ok(CardActionNode::LookAt(CardOperandNode::ZoneCollection(
            CardZoneCollectionNode {
                zone,
                owner: CardZoneOwnerNode::Player(LocalPlayerReference::Bound(player)),
                predicate: None,
                quantity: None,
                additional_zones: Vec::new(),
            },
        )));
    }
    let zone = match try_recognize_card_zone_collection(cursor) {
        CollectionOperandAttempt::Recognized(fragment) => fragment.node,
        CollectionOperandAttempt::NotClaimed => {
            return Err(cursor.fail("the top cards of a library or a targeted player's hand"));
        }
        CollectionOperandAttempt::RejectedCandidate(failure)
        | CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
    };
    cursor.require(
        zone.zone == CardZoneName::Hand
            && zone.additional_zones.is_empty()
            && zone.predicate.is_none()
            && zone.quantity.is_none()
            && matches!(zone.owner, CardZoneOwnerNode::TargetPlayer(_)),
        "a targeted player's whole hand (a filtered look reveals only part of it)",
    )?;
    Ok(CardActionNode::LookAt(CardOperandNode::ZoneCollection(
        zone,
    )))
}

/// `that player's` / `defending player's`: the shared bound-player
/// resolver reads the possessive as its player head over a view of the same
/// tokens whose possessive role word is the plain noun (equal length,
/// indices and spans). `None` consumes nothing.
pub(in crate::oracle::canonical) fn try_recognize_bound_possessive_player(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<BoundPlayerReference>> {
    let possessive = cursor.peek_ahead(1).is_some_and(|raw| {
        raw.eq_ignore_ascii_case("player's") || raw.eq_ignore_ascii_case("player\u{2019}s")
    });
    if !(possessive && (peek_word(cursor, 0, "that") || peek_word(cursor, 0, "defending"))) {
        return Ok(None);
    }
    let environment = cursor
        .binding_environment()
        .cloned()
        .ok_or_else(|| cursor.fail("a binding environment naming the possessive player"))?;
    let start = cursor.position();
    let mut view = cursor.token_slice(0, usize::MAX).to_vec();
    view[start + 1].raw = "player".to_string();
    let mut trial = cursor.with_tokens(&view);
    let player = recognize_bound_player_reference(&mut trial, &environment)?;
    cursor.require(
        trial.position() == start + 2,
        "a possessive player head of two words",
    )?;
    cursor.advance_to(start + 2)?;
    Ok(Some(player))
}

fn peek_word(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// `you may play|cast`: the head of a card permission. Pure lookahead.
fn permission_head_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    peek_word(cursor, 0, "you")
        && peek_word(cursor, 1, "may")
        && (peek_word(cursor, 2, "play") || peek_word(cursor, 2, "cast"))
}

/// An imperative `cast` at the clause head (CR 608.2g: the player casts
/// the cards while the effect resolves, if able). Pure lookahead; the
/// static `cast this spell only ...` restriction never reaches an effect
/// program, and `cast spells ... as though they had flash` is a player rule
/// that fails at its operand.
fn imperative_cast_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    peek_word(cursor, 0, "cast")
}

/// `copy CARD` over a card reference (`it`, `that card`, `the exiled card`,
/// `those cards`). A stack object (`copy target spell`, `copy that spell`,
/// `copy it` naming the cast spell of the event) stays with the stack-copy
/// frame. Pure lookahead.
fn card_copy_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    if !peek_word(cursor, 0, "copy") || super::copy_stack_object_starts(cursor) {
        return false;
    }
    let mut trial = cursor.clone();
    if trial.consume("copy").is_err() {
        return false;
    }
    matches!(
        try_recognize_card_collection_reference(&mut trial),
        CollectionOperandAttempt::Recognized(_)
    )
}

/// `copy CARD [twice|thrice|N times]` (CR 707.12). The copy exists only to
/// be cast: the next instruction must cast `the copy` / `the copies`
/// (CR 707.10a makes an uncast card copy cease to exist), so a copy that
/// nothing casts is an explicit failure here, never a dangling object.
fn recognize_card_copy(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<CardActionNode> {
    cursor.enter("card-action.copy.verb");
    cursor.consume("copy")?;
    cursor.enter("card-action.copy.card");
    let cards = match try_recognize_card_collection_reference(cursor) {
        CollectionOperandAttempt::Recognized(fragment) => CardOperandNode::Reference(fragment.node),
        CollectionOperandAttempt::NotClaimed => {
            return Err(cursor.fail("a bound card to copy (it, that card, the exiled card)"));
        }
        CollectionOperandAttempt::RejectedCandidate(failure)
        | CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
    };
    cursor.enter("card-action.copy.count");
    let count = recognize_repetition_count(cursor)?;
    cursor.enter("card-action.copy.cast-follows");
    cursor.require(
        copy_cast_follows(cursor),
        "an instruction casting the copy right after the copy (CR 707.12)",
    )?;
    Ok(CardActionNode::Copy { cards, count })
}

/// `once` or the shared repetition adjunct (`twice`, `thrice`, `N times`)
/// after an action; absent means one performance. The adjunct reads its
/// amount in a view that ends before `times`, which the shared arithmetic
/// would otherwise read as multiplication.
fn recognize_repetition_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    if peek_word(cursor, 0, "once") {
        cursor.consume("once")?;
        return Ok(NumericExpression::from(1));
    }
    if !super::repetition_adjunct_starts(cursor) {
        return Ok(NumericExpression::from(1));
    }
    let count = super::recognize_repetition_adjunct(cursor)?;
    if let NumericExpression::Atom(NumericAtom::Integer(value)) = &count {
        cursor.enter("card-action.repetition.count");
        cursor.require(*value > 0, "a positive repetition count")?;
    }
    Ok(count)
}

/// Whether the next instruction casts `the copy` / `the copies`: after the
/// sentence end or a coordinator, an optional `you may`, then `cast the
/// copy|copies` or `cast any number of the copies`. Pure lookahead.
fn copy_cast_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    let mut offset = 0;
    if peek_word(cursor, offset, ",") {
        offset += 1;
    }
    if peek_word(cursor, offset, ".")
        || peek_word(cursor, offset, "then")
        || peek_word(cursor, offset, "and")
    {
        offset += 1;
    } else if offset == 0 {
        return false;
    }
    if peek_word(cursor, offset, "then") {
        offset += 1;
    }
    if peek_word(cursor, offset, "you") && peek_word(cursor, offset + 1, "may") {
        offset += 2;
    }
    if !peek_word(cursor, offset, "cast") {
        return false;
    }
    offset += 1;
    if peek_word(cursor, offset, "any")
        && peek_word(cursor, offset + 1, "number")
        && peek_word(cursor, offset + 2, "of")
    {
        offset += 3;
    }
    peek_word(cursor, offset, "the")
        && (peek_word(cursor, offset + 1, "copy") || peek_word(cursor, offset + 1, "copies"))
}

/// `the copy` / `the copies` / `any number of the copies` (CR 707.12a: each
/// copy is cast or not individually). `None` consumes nothing.
fn try_recognize_card_copies(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<CardOperandNode>> {
    let (width, plural) = if peek_word(cursor, 0, "the") && peek_word(cursor, 1, "copy") {
        (2, false)
    } else if peek_word(cursor, 0, "the") && peek_word(cursor, 1, "copies") {
        (2, true)
    } else if peek_word(cursor, 0, "any")
        && peek_word(cursor, 1, "number")
        && peek_word(cursor, 2, "of")
        && peek_word(cursor, 3, "the")
        && peek_word(cursor, 4, "copies")
    {
        (5, true)
    } else {
        return Ok(None);
    };
    cursor.enter("card-action.copies");
    cursor.advance_to(cursor.position() + width)?;
    Ok(Some(CardOperandNode::CardCopies { plural }))
}

/// Lexical signal of a hidden-zone choice in the current clause: `outside
/// the game` or `at random` before the sentence end. Pure lookahead.
fn hidden_choice_signal(cursor: &OracleTokenCursor<'_>) -> bool {
    (0..16)
        .take_while(|offset| {
            cursor
                .peek_ahead(*offset)
                .is_some_and(|raw| raw != "." && raw != ";")
        })
        .any(|offset| {
            (peek_word(cursor, offset, "outside")
                && peek_word(cursor, offset + 1, "the")
                && peek_word(cursor, offset + 2, "game"))
                || (peek_word(cursor, offset, "at") && peek_word(cursor, offset + 1, "random"))
        })
}

/// `QUANTITY PREDICATE card(s) you own from outside the game` (CR 400.11a)
/// or `QUANTITY PREDICATE card(s) at random from ZONE`: the shared object
/// noun phrase in the card domain, then the source. The phrase is committed
/// at its source (`from outside the game`, `at random from`).
fn try_recognize_hidden_card_choice(
    cursor: &mut OracleTokenCursor<'_>,
) -> CollectionOperandAttempt<HiddenCardChoiceNode> {
    if !hidden_choice_signal(cursor) {
        return CollectionOperandAttempt::NotClaimed;
    }
    let mut trial = cursor.clone();
    let mut committed = false;
    let result = (|| -> MachineResult<HiddenCardChoiceNode> {
        trial.enter("card-action.hidden-choice.cards");
        let phrase = recognize_object_noun_phrase(
            &mut trial,
            ObjectNounPhraseExpectation::in_domains(&[ObjectKind::Card]),
        )?;
        let quantity = match &phrase.determiner {
            Some(ObjectDeterminer::Chosen {
                quantity,
                another: false,
            }) => match quantity {
                ChosenObjectQuantity::Exactly(count) => {
                    CardSelectionQuantityNode::Exactly(count.clone())
                }
                ChosenObjectQuantity::UpTo(count) => CardSelectionQuantityNode::UpTo(count.clone()),
                ChosenObjectQuantity::AnyNumber => CardSelectionQuantityNode::AnyNumber,
            },
            _ => {
                return Err(trial
                    .fail("a chosen quantity of cards (`a`, `N`, `up to N` or `any number of`)"));
            }
        };
        trial.require(
            !phrase.exclude_source && phrase.domain == Some(ObjectKind::Card),
            "a card-domain query without a source exclusion",
        )?;
        if peek_word(&trial, 0, "at") {
            trial.enter("card-action.hidden-choice.random");
            trial.consume_phrase(&["at", "random", "from"])?;
            committed = true;
            trial.require(
                phrase.query.relations().is_empty(),
                "no player relation on cards chosen at random from a zone",
            )?;
            let predicate = phrase.query.predicate().clone();
            let (zone, owner) = match recognize_permitted_card_origin(&mut trial)? {
                PermittedCardOrigin::Zone {
                    zone,
                    owner,
                    additional,
                } if additional.is_empty() => (zone, owner),
                _ => {
                    return Err(trial.fail("one hand or graveyard to choose from at random"));
                }
            };
            trial.require(
                matches!(zone, CardZoneName::Hand | CardZoneName::Graveyard),
                "a hand or graveyard to choose from at random",
            )?;
            return Ok(HiddenCardChoiceNode {
                quantity,
                predicate,
                source: HiddenCardSourceNode::Zone { zone, owner },
                random: true,
            });
        }
        trial.enter("card-action.hidden-choice.outside-game");
        trial.consume_phrase(&["from", "outside", "the", "game"])?;
        committed = true;
        // CR 400.11a: outside the game is the choosing player's sideboard,
        // which holds only cards they own; the phrase states that owner.
        trial.enter("card-action.hidden-choice.outside-game.owner");
        let related = phrase.query.into_related_query(&mut trial)?;
        trial.require(
            related.objects.controller.is_none()
                && related.ownership == Some(ObjectOwnershipScope::AbilityController),
            "cards `you own` from outside the game",
        )?;
        Ok(HiddenCardChoiceNode {
            quantity,
            predicate: related.objects.predicate,
            source: HiddenCardSourceNode::OutsideGame,
            random: false,
        })
    })();
    match result {
        Ok(node) => {
            trial.enter("card-action.hidden-choice.complete");
            let end_position = trial.position();
            *cursor = trial;
            CollectionOperandAttempt::Recognized(RecognizedEffectFragment { node, end_position })
        }
        Err(failure) if committed => CollectionOperandAttempt::FailedCommitted(failure),
        Err(failure) => CollectionOperandAttempt::RejectedCandidate(failure),
    }
}

/// A preposed `WINDOW,` followed by a permission head, on a trial cursor.
fn try_preposed_permission_window<'a>(
    cursor: &OracleTokenCursor<'a>,
) -> Option<(OracleTokenCursor<'a>, CardPermissionWindowNode)> {
    let mut trial = cursor.clone();
    let window = if peek_word(&trial, 0, "for") {
        recognize_exiled_window(&mut trial).ok()?
    } else if next_upkeep_window_starts(&trial) {
        recognize_next_upkeep_window(&mut trial).ok()?
    } else if peek_word(&trial, 1, "your") {
        recognize_next_end_step_window(&mut trial).ok()?
    } else {
        return None;
    };
    trial.consume(",").ok()?;
    permission_head_starts(&trial).then_some((trial, window))
}

/// `for as long as it|they|that card|those cards|the exiled card(s)
/// remain(s) exiled`. The subject restates the permitted cards; the window
/// applies to each permitted card separately.
fn recognize_exiled_window(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardPermissionWindowNode> {
    cursor.enter("card-action.permit.window.while-exiled");
    cursor.consume_phrase(&["for", "as", "long", "as"])?;
    cursor.enter("card-action.permit.window.while-exiled.subject");
    let plural = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("it") => {
            cursor.consume("it")?;
            false
        }
        Some("they") => {
            cursor.consume("they")?;
            true
        }
        Some("that") => {
            cursor.consume_phrase(&["that", "card"])?;
            false
        }
        Some("those") => {
            cursor.consume_phrase(&["those", "cards"])?;
            true
        }
        Some("the") => {
            cursor.consume_phrase(&["the", "exiled"])?;
            let plural = peek_word(cursor, 0, "cards");
            cursor.consume(if plural { "cards" } else { "card" })?;
            plural
        }
        _ => {
            return Err(cursor.fail("the permitted cards: it, they, that card or those cards"));
        }
    };
    cursor.enter("card-action.permit.window.while-exiled.verb");
    cursor.consume(if plural { "remain" } else { "remains" })?;
    cursor.consume("exiled")?;
    Ok(CardPermissionWindowNode::WhileInExile)
}

/// `until your next end step`.
fn recognize_next_end_step_window(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardPermissionWindowNode> {
    cursor.enter("card-action.permit.window.next-end-step");
    cursor.consume_phrase(&["until", "your", "next", "end", "step"])?;
    Ok(CardPermissionWindowNode::UntilYourNextEndStep)
}

/// `until [the beginning of] your next upkeep`. Pure lookahead.
fn next_upkeep_window_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let offset = if peek_word(cursor, 1, "the") && peek_word(cursor, 2, "beginning") {
        4
    } else {
        1
    };
    peek_word(cursor, 0, "until")
        && peek_word(cursor, offset, "your")
        && peek_word(cursor, offset + 1, "next")
        && peek_word(cursor, offset + 2, "upkeep")
}

/// `until [the beginning of] your next upkeep` (CR 503.1).
fn recognize_next_upkeep_window(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardPermissionWindowNode> {
    cursor.enter("card-action.permit.window.next-upkeep");
    cursor.consume("until")?;
    if peek_word(cursor, 0, "the") {
        cursor.consume_phrase(&["the", "beginning", "of"])?;
    }
    cursor.consume_phrase(&["your", "next", "upkeep"])?;
    Ok(CardPermissionWindowNode::UntilYourNextUpkeep)
}

/// `until you exile another card with this NOUN`: the source noun is a
/// registered object or subtype word, as in `cards exiled with this NOUN`.
fn recognize_source_exile_window(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardPermissionWindowNode> {
    cursor.enter("card-action.permit.window.source-exiles-another");
    cursor.consume_phrase(&["until", "you", "exile", "another", "card", "with", "this"])?;
    let noun = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a source noun after `with this`"))?
        .to_string();
    cursor.require(
        crate::oracle::characteristic_vocabulary::lookup_object_selector_lexeme(&noun).is_some()
            || crate::oracle::characteristic_vocabulary::lookup_noncreature_subtype(&noun)
                .is_some(),
        "a registered source noun after `with this`",
    )?;
    cursor.consume(&noun)?;
    Ok(CardPermissionWindowNode::UntilSourceExilesAnother)
}

/// Whether a trailing permission window starts at the cursor.
fn permission_window_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    peek_word(cursor, 0, "until")
        || (peek_word(cursor, 0, "this") && peek_word(cursor, 1, "turn"))
        || (peek_word(cursor, 0, "for") && peek_word(cursor, 1, "as"))
}

/// The trailing window of a permission: the shared play-window duration, the
/// source-control duration, or a window of this family.
fn recognize_trailing_permission_window(
    cursor: &mut OracleTokenCursor<'_>,
    frame: CardActionReadContext<'_>,
) -> MachineResult<(Option<EffectDurationNode>, Option<CardPermissionWindowNode>)> {
    if peek_word(cursor, 0, "for") {
        if peek_word(cursor, 4, "you") && peek_word(cursor, 5, "control") {
            // `for as long as you control this creature|NAME` (CR 611.2b):
            // the shared source reference, with the program's name links.
            return Ok((
                Some(recognize_source_control_duration(
                    cursor,
                    frame.name_links,
                    frame.name_failure,
                )?),
                None,
            ));
        }
        return Ok((None, Some(recognize_exiled_window(cursor)?)));
    }
    if next_upkeep_window_starts(cursor) {
        return Ok((None, Some(recognize_next_upkeep_window(cursor)?)));
    }
    if peek_word(cursor, 0, "until") && peek_word(cursor, 1, "your") && peek_word(cursor, 3, "end")
    {
        return Ok((None, Some(recognize_next_end_step_window(cursor)?)));
    }
    if peek_word(cursor, 0, "until") && peek_word(cursor, 1, "you") {
        return Ok((None, Some(recognize_source_exile_window(cursor)?)));
    }
    Ok((Some(recognize_play_window_duration(cursor)?), None))
}

/// `without paying its|their mana cost(s)`.
fn recognize_without_paying(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<bool> {
    if !peek_word(cursor, 0, "without") {
        return Ok(false);
    }
    cursor.enter("card-action.permit.base-cost");
    cursor.consume_phrase(&["without", "paying"])?;
    let (possessive, noun) = if peek_word(cursor, 0, "their") {
        ("their", "costs")
    } else {
        ("its", "cost")
    };
    cursor.consume_phrase(&[possessive, "mana", noun])?;
    Ok(true)
}

/// `[,] and mana of any type can be spent to cast that spell|it|them|those
/// spells`: the permitted casts' spending policy, coordinated with the
/// permission it qualifies.
fn any_type_spending_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let offset = usize::from(peek_word(cursor, 0, ","));
    peek_word(cursor, offset, "and") && peek_word(cursor, offset + 1, "mana")
}

fn recognize_any_type_spending(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<bool> {
    if !any_type_spending_starts(cursor) {
        return Ok(false);
    }
    let comma = peek_word(cursor, 0, ",");
    cursor.enter("card-action.permit.spending.any-type");
    if comma {
        cursor.consume(",")?;
    }
    cursor.consume_phrase(&[
        "and", "mana", "of", "any", "type", "can", "be", "spent", "to", "cast",
    ])?;
    cursor.enter("card-action.permit.spending.spells");
    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("it") => cursor.consume("it")?,
        Some("them") => cursor.consume("them")?,
        Some("that") => cursor.consume_phrase(&["that", "spell"])?,
        Some("those") => cursor.consume_phrase(&["those", "spells"])?,
        _ => return Err(cursor.fail("the permitted spells: that spell, it, them or those spells")),
    }
    Ok(true)
}

/// Where the permitted cards of a quantified query are.
enum PermittedCardOrigin {
    /// `from among them|those cards|the exiled cards|cards exiled with this`,
    /// `from among [the] [QUALIFIER] cards VERBed this way` and `from among
    /// those QUALIFIER cards`: the bound collection, with the qualifier the
    /// restatement adds to the permitted query (`those nonland cards`).
    Collection {
        from: CardCollectionReferenceNode,
        qualifier: Option<ObjectPredicate>,
    },
    /// `from your|target player's|that player's|each opponent's ZONE [or
    /// ZONE]`: one owner's zones, read as one union (`from your hand or
    /// graveyard`).
    Zone {
        zone: CardZoneName,
        owner: CardZoneOwnerNode,
        additional: Vec<CardZoneName>,
    },
    /// `from the command zone` (CR 408.1): the controller's command zone.
    CommandZone,
}

/// Whether a registered zone noun of a permitted origin stands at `offset`.
fn origin_zone_word_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    ["graveyard", "hand", "exile", "library"]
        .iter()
        .any(|zone| peek_word(cursor, offset, zone))
}

/// `each opponent's` / `an opponent's`: the possessive of the opponents of
/// the ability's controller, before a graveyard.
fn opponent_possessive_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    cursor.peek_ahead(offset).is_some_and(|raw| {
        raw.eq_ignore_ascii_case("opponent's") || raw.eq_ignore_ascii_case("opponent\u{2019}s")
    })
}

/// The longest qualifier a restated collection writes before its `cards`.
const RESTATED_QUALIFIER_WIDTH: usize = 8;

/// `[the|those] [QUALIFIER] cards [VERBed this way]` after `from among`: the
/// scope's observed collection restated by its producing verb (`cards
/// revealed this way`, `the nonland cards exiled this way`), or a qualified
/// plural anaphor (`those nonland cards`). The qualifier is the shared card
/// query read up to `cards`; it narrows the permitted query. `None` claims
/// nothing (`them`, `those cards`, `the exiled cards` keep the shared
/// collection reference); once its `this way` or qualifier is proved, a
/// failure is this origin's own.
fn recognize_restated_collection_origin(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<PermittedCardOrigin>> {
    let determiner = ["the", "those"]
        .into_iter()
        .find(|word| peek_word(cursor, 0, word));
    let start = usize::from(determiner.is_some());
    let Some(noun) = (start..start + RESTATED_QUALIFIER_WIDTH)
        .take_while(|offset| {
            cursor
                .peek_ahead(*offset)
                .is_some_and(|raw| !matches!(raw, "." | "," | ";"))
        })
        .find(|offset| peek_word(cursor, *offset, "cards"))
    else {
        return Ok(None);
    };
    let verb = match cursor.peek_ahead(noun + 1).map(str::to_ascii_lowercase) {
        Some(word)
            if matches!(word.as_str(), "revealed" | "exiled")
                && peek_word(cursor, noun + 2, "this")
                && peek_word(cursor, noun + 3, "way") =>
        {
            Some(if word == "revealed" {
                CardCollectionVerb::Revealed
            } else {
                CardCollectionVerb::Exiled
            })
        }
        _ => None,
    };
    let qualified = noun > start;
    if verb.is_none() && !(determiner == Some("those") && qualified) {
        return Ok(None);
    }
    // `those exiled cards` is the shared reference itself; only a qualifier
    // that reference does not read is this origin's.
    if verb.is_none()
        && matches!(
            try_recognize_card_collection_reference(&mut cursor.clone()),
            CollectionOperandAttempt::Recognized(_)
        )
    {
        return Ok(None);
    }
    cursor.enter("card-action.permit.origin.restated");
    if let Some(determiner) = determiner {
        cursor.consume(determiner)?;
    }
    let qualifier = if qualified {
        cursor.enter("card-action.permit.origin.restated.qualifier");
        let end = cursor.position() + (noun - start) + 1;
        let query = cursor.bounded(end, |inner| {
            recognize_object_noun_query(inner, ObjectQueryContext::ORDINARY)
        })?;
        cursor.require(
            query.relations().is_empty() && query.predicate().is_in_domain(ObjectKind::Card),
            "an unscoped card-domain qualifier of the restated collection",
        )?;
        Some(query.predicate().clone())
            .filter(|predicate| *predicate != ObjectPredicate::ObjectKind(ObjectKind::Card))
    } else {
        cursor.consume("cards")?;
        None
    };
    let from = match verb {
        Some(verb) => {
            cursor.enter("card-action.permit.origin.restated.verb");
            let word = match verb {
                CardCollectionVerb::Revealed => "revealed",
                CardCollectionVerb::Exiled => "exiled",
            };
            cursor.consume_phrase(&[word, "this", "way"])?;
            CardCollectionReferenceNode::Restated(verb)
        }
        None => CardCollectionReferenceNode::Antecedent,
    };
    Ok(Some(PermittedCardOrigin::Collection { from, qualifier }))
}

fn recognize_permitted_card_origin(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PermittedCardOrigin> {
    if peek_word(cursor, 0, "among") {
        cursor.enter("card-action.permit.origin.collection");
        cursor.consume("among")?;
        if let Some(origin) = recognize_restated_collection_origin(cursor)? {
            return Ok(origin);
        }
        let reference = match try_recognize_card_collection_reference(cursor) {
            CollectionOperandAttempt::Recognized(fragment) => fragment.node,
            CollectionOperandAttempt::NotClaimed => {
                return Err(cursor.fail("a bound card collection after `from among`"));
            }
            CollectionOperandAttempt::RejectedCandidate(failure)
            | CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
        };
        cursor.require(
            matches!(
                reference,
                CardCollectionReferenceNode::Antecedent
                    | CardCollectionReferenceNode::Exiled
                    | CardCollectionReferenceNode::Linked
            ),
            "them, those cards, the exiled cards or cards exiled with the source",
        )?;
        return Ok(PermittedCardOrigin::Collection {
            from: reference,
            qualifier: None,
        });
    }
    if peek_word(cursor, 0, "the") && peek_word(cursor, 1, "command") {
        // `from the command zone` (CR 408.1): only the controller's own
        // command zone holds cards they can cast from it.
        cursor.enter("card-action.permit.origin.command-zone");
        cursor.consume_phrase(&["the", "command", "zone"])?;
        return Ok(PermittedCardOrigin::CommandZone);
    }
    cursor.enter("card-action.permit.origin.zone");
    let owner = if peek_word(cursor, 0, "your") {
        cursor.consume("your")?;
        CardZoneOwnerNode::Player(LocalPlayerReference::Controller)
    } else if (peek_word(cursor, 0, "a") || peek_word(cursor, 0, "any"))
        && peek_word(cursor, 1, "graveyard")
    {
        // `from a|any graveyard`: any player's graveyard, one public zone
        // union.
        let determiner = cursor.peek().unwrap_or("a").to_owned();
        cursor.consume(&determiner)?;
        CardZoneOwnerNode::Players(PlayerSetReferenceNode::EachPlayer)
    } else if (peek_word(cursor, 0, "an") || peek_word(cursor, 0, "each"))
        && opponent_possessive_at(cursor, 1)
        && peek_word(cursor, 2, "graveyard")
    {
        // `from an|each opponent's graveyard`: the union of the opponents'
        // graveyards, from which the cards are chosen.
        let determiner = cursor.peek().unwrap_or("an").to_owned();
        let role = cursor.peek_ahead(1).unwrap_or("opponent's").to_owned();
        cursor.consume(&determiner)?;
        cursor.consume(&role)?;
        CardZoneOwnerNode::Players(PlayerSetReferenceNode::OpponentsOfAbilityController)
    } else if starts_possessive_target_player(cursor) {
        CardZoneOwnerNode::TargetPlayer(recognize_possessive_target_player(cursor)?)
    } else if let Some(player) = try_recognize_bound_possessive_player(cursor)? {
        // `from that player's|defending player's graveyard`: the player the
        // binding environment names; the zone is that player's own.
        CardZoneOwnerNode::Player(LocalPlayerReference::Bound(player))
    } else {
        return Err(cursor.fail(
            "`among` a bound collection, the command zone, `your` zone, or a bound or targeted player's zone",
        ));
    };
    let zone = recognize_card_zone_atom(cursor)?;
    // `from your hand or graveyard`, `from your graveyard and/or hand`: one
    // owner, several of their zones. A connector not followed by a zone
    // noun is the parent's.
    let mut additional = Vec::new();
    while (peek_word(cursor, 0, "or") || peek_word(cursor, 0, "and/or"))
        && origin_zone_word_at(cursor, 1)
    {
        cursor.enter("card-action.permit.origin.zone-union");
        let connector = cursor.peek().unwrap_or("or").to_owned();
        cursor.consume(&connector)?;
        let other = recognize_card_zone_atom(cursor)?;
        cursor.require(
            other != zone && !additional.contains(&other),
            "distinct zones in one origin union",
        )?;
        additional.push(other);
    }
    cursor.enter("card-action.permit.origin.zone-kind");
    cursor.require(
        std::iter::once(&zone).chain(additional.iter()).all(|zone| {
            matches!(
                zone,
                CardZoneName::Hand | CardZoneName::Graveyard | CardZoneName::Exile
            )
        }),
        "a hand, graveyard or exile origin of the permitted cards",
    )?;
    cursor.require(
        !matches!(owner, CardZoneOwnerNode::Players(_))
            || (zone == CardZoneName::Graveyard && additional.is_empty()),
        "a population of owners only for the public graveyard (CR 400.2)",
    )?;
    Ok(PermittedCardOrigin::Zone {
        zone,
        owner,
        additional,
    })
}

/// CR 601.2a: a card becomes a spell with the same characteristics, so a
/// spell query over the cards to cast reads as the same query in the card
/// domain. Only the domain noun changes; every characteristic is kept.
fn spell_query_as_card_query(predicate: &ObjectPredicate) -> ObjectPredicate {
    match predicate {
        ObjectPredicate::ObjectKind(ObjectKind::Spell) => {
            ObjectPredicate::ObjectKind(ObjectKind::Card)
        }
        ObjectPredicate::Not(operand) => {
            ObjectPredicate::Not(Box::new(spell_query_as_card_query(operand)))
        }
        ObjectPredicate::Intersection(operands) => {
            ObjectPredicate::Intersection(operands.iter().map(spell_query_as_card_query).collect())
        }
        ObjectPredicate::Union(operands) => {
            ObjectPredicate::Union(operands.iter().map(spell_query_as_card_query).collect())
        }
        other => other.clone(),
    }
}

/// Intersects one more qualifier into a query predicate.
fn intersect_predicate(predicate: ObjectPredicate, qualifier: ObjectPredicate) -> ObjectPredicate {
    match predicate {
        ObjectPredicate::Intersection(mut operands) => {
            operands.push(qualifier);
            ObjectPredicate::Intersection(operands)
        }
        other => ObjectPredicate::Intersection(vec![other, qualifier]),
    }
}

/// Lexical lookahead for a mana-value comparison this frame owns: `with
/// mana value ...` or a relative `with [equal or] lesser|greater mana
/// value`. Pure lookahead.
fn spell_comparison_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    if !peek_word(cursor, 0, "with") {
        return false;
    }
    let relative_at = |offset: usize| {
        (peek_word(cursor, offset, "lesser") || peek_word(cursor, offset, "greater"))
            && peek_word(cursor, offset + 1, "mana")
            && peek_word(cursor, offset + 2, "value")
    };
    (peek_word(cursor, 1, "mana") && peek_word(cursor, 2, "value"))
        || relative_at(1)
        || (peek_word(cursor, 1, "equal") && peek_word(cursor, 2, "or") && relative_at(3))
}

/// The position of the first comparison `with` this frame owns before the
/// end of the clause, for a phrase that the shared reader cannot finish
/// with it. Pure lookahead.
fn owned_comparison_offset(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let mut trial = cursor.clone();
    for offset in 0..24 {
        match trial.peek() {
            None | Some("." | ";" | ",") => return None,
            _ => {}
        }
        if spell_comparison_starts(&trial) {
            return Some(offset);
        }
        if peek_word(&trial, 0, "from") {
            return None;
        }
        trial.advance_to(trial.position() + 1).ok()?;
    }
    None
}

/// The antecedent spell a relative comparison (`with equal or lesser mana
/// value`) is measured against: the spell the triggering cast event records
/// (CR 603.2). Any other antecedent stays an explicit failure.
fn antecedent_spell_mana_value(cursor: &OracleTokenCursor<'_>) -> MachineResult<NumericExpression> {
    cursor.require(
        cursor
            .binding_environment()
            .is_some_and(|environment| environment.frame.event_spell),
        "a triggering cast event whose spell the relative mana-value comparison names",
    )?;
    Ok(NumericExpression::EventValue(
        EventNumericBinding::CastSpellManaValue,
    ))
}

/// The right operand of a mana-value comparison: `that damage` (the amount
/// the triggering damage event records, CR 120.3), `that spell's mana value`
/// (the triggering cast's spell), or the shared query numeric expression
/// (`X`, `the number of Spirits you control`, `this creature's power`). A
/// demonstrative without its event is an explicit failure, never zero.
fn recognize_comparison_operand(
    cursor: &mut OracleTokenCursor<'_>,
    frame: CardActionReadContext<'_>,
) -> MachineResult<NumericExpression> {
    if peek_word(cursor, 0, "that") && peek_word(cursor, 1, "damage") {
        cursor.enter("card-action.permit.query.comparison.event-damage");
        cursor.consume_phrase(&["that", "damage"])?;
        cursor.require(
            cursor.event_numeric_binding() == Some(EventNumericBinding::DamageAmount),
            "a triggering damage event whose amount `that damage` names",
        )?;
        return Ok(NumericExpression::EventValue(
            EventNumericBinding::DamageAmount,
        ));
    }
    if peek_word(cursor, 0, "that")
        && cursor.peek_ahead(1).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("spell's") || raw.eq_ignore_ascii_case("spell\u{2019}s")
        })
        && peek_word(cursor, 2, "mana")
    {
        cursor.enter("card-action.permit.query.comparison.event-spell");
        let possessive = cursor.peek_ahead(1).unwrap_or("spell's").to_owned();
        cursor.consume("that")?;
        cursor.consume(&possessive)?;
        cursor.consume_phrase(&["mana", "value"])?;
        return antecedent_spell_mana_value(cursor);
    }
    cursor.enter("card-action.permit.query.comparison.expression");
    // The query grammar first (source properties, `X`); a right operand it
    // does not read (`the number of creatures you control`, `the greatest
    // power among ...`) is the shared comparison right-hand side, the same
    // grammar a selector comparison reads before `from`.
    let mut query = cursor.clone();
    let expression = match recognize_query_numeric_expression(&mut query) {
        Ok(expression) => {
            *cursor = query;
            expression
        }
        Err(first) => {
            let mut comparison = cursor.clone();
            match super::arithmetic_expression::recognize_comparison_rhs_expression(&mut comparison)
            {
                Ok(expression) => {
                    *cursor = comparison;
                    expression
                }
                Err(_) => return Err(first),
            }
        }
    };
    // The source's counters or characteristics are read only where the
    // frame proves its source (CR 608.2h), as in the shared selector.
    cursor.enter("card-action.permit.query.comparison.source-context");
    cursor.require(
        frame.source_available || !numeric_expression_reads_source(&expression),
        "a source whose counters or characteristics the comparison reads",
    )?;
    Ok(expression)
}

/// Whether an expression reads the ability source's counters or
/// characteristics, directly or inside arithmetic.
fn numeric_expression_reads_source(expression: &NumericExpression) -> bool {
    match expression {
        NumericExpression::CountCounters {
            object: LocalObjectReference::SourceSelf,
            ..
        }
        | NumericExpression::ObjectProperty {
            object: LocalObjectReference::SourceSelf,
            ..
        } => true,
        NumericExpression::Add { left, right }
        | NumericExpression::Subtract { left, right }
        | NumericExpression::Multiply { left, right }
        | NumericExpression::Divide { left, right, .. } => {
            numeric_expression_reads_source(left) || numeric_expression_reads_source(right)
        }
        _ => false,
    }
}

/// `with mana value [equal to|less than|greater than [or equal to]] OPERAND
/// [or less|or greater]` and the relative `with [equal or] lesser|greater
/// mana value` (CR 202.3): one mana-value comparison of the permitted cards
/// with a numeric expression or with the antecedent spell's mana value.
/// The parent intersects the comparison into the whole query.
fn recognize_spell_comparison_qualifier(
    cursor: &mut OracleTokenCursor<'_>,
    frame: CardActionReadContext<'_>,
) -> MachineResult<ObjectPredicate> {
    cursor.enter("card-action.permit.query.comparison");
    cursor.consume("with")?;
    let comparison = |operator, right| ObjectPredicate::NumericComparison {
        left: SelectorNumericProperty::ManaValue,
        operator,
        right: Box::new(right),
    };
    if !peek_word(cursor, 0, "mana") {
        // `with [equal or] lesser|greater mana value`.
        cursor.enter("card-action.permit.query.comparison.relative");
        let inclusive = peek_word(cursor, 0, "equal");
        if inclusive {
            cursor.consume_phrase(&["equal", "or"])?;
        }
        let lesser = peek_word(cursor, 0, "lesser");
        cursor.consume(if lesser { "lesser" } else { "greater" })?;
        cursor.consume_phrase(&["mana", "value"])?;
        let operator = match (lesser, inclusive) {
            (true, true) => NumericComparisonOperator::LessThanOrEqual,
            (true, false) => NumericComparisonOperator::LessThan,
            (false, true) => NumericComparisonOperator::GreaterThanOrEqual,
            (false, false) => NumericComparisonOperator::GreaterThan,
        };
        return Ok(comparison(operator, antecedent_spell_mana_value(cursor)?));
    }
    cursor.consume_phrase(&["mana", "value"])?;
    cursor.enter("card-action.permit.query.comparison.relation");
    let prefix = |less: bool, inclusive: bool| match (less, inclusive) {
        (true, true) => NumericComparisonOperator::LessThanOrEqual,
        (true, false) => NumericComparisonOperator::LessThan,
        (false, true) => NumericComparisonOperator::GreaterThanOrEqual,
        (false, false) => NumericComparisonOperator::GreaterThan,
    };
    let mut operator = if peek_word(cursor, 0, "equal") && peek_word(cursor, 1, "to") {
        cursor.consume_phrase(&["equal", "to"])?;
        Some(NumericComparisonOperator::Equal)
    } else if (peek_word(cursor, 0, "less") || peek_word(cursor, 0, "greater"))
        && peek_word(cursor, 1, "than")
    {
        let less = peek_word(cursor, 0, "less");
        cursor.consume_phrase(&[if less { "less" } else { "greater" }, "than"])?;
        let inclusive = peek_word(cursor, 0, "or")
            && peek_word(cursor, 1, "equal")
            && peek_word(cursor, 2, "to");
        if inclusive {
            cursor.consume_phrase(&["or", "equal", "to"])?;
        }
        Some(prefix(less, inclusive))
    } else {
        None
    };
    let right = recognize_comparison_operand(cursor, frame)?;
    if operator.is_none() {
        // `X or less`, `4 or greater`: the postfix direction of a bare
        // operand; a bare operand alone is an equality.
        operator = Some(
            if peek_word(cursor, 0, "or")
                && (peek_word(cursor, 1, "less") || peek_word(cursor, 1, "greater"))
            {
                cursor.enter("card-action.permit.query.comparison.direction");
                let less = peek_word(cursor, 1, "less");
                cursor.consume_phrase(&["or", if less { "less" } else { "greater" }])?;
                prefix(less, true)
            } else {
                NumericComparisonOperator::Equal
            },
        );
    }
    let operator = cursor.require_value(operator, "a mana-value comparison operator")?;
    Ok(comparison(operator, right))
}

/// `... from your graveyard with mana value 4 or less`: a qualifier placed
/// after the origin still qualifies the whole query. Mana value is this
/// frame's comparison; power and toughness keep the shared selector tail.
fn recognize_postposed_numeric_qualifier(
    cursor: &mut OracleTokenCursor<'_>,
    predicate: ObjectPredicate,
    frame: CardActionReadContext<'_>,
) -> MachineResult<ObjectPredicate> {
    if spell_comparison_starts(cursor) {
        let comparison = recognize_spell_comparison_qualifier(cursor, frame)?;
        return Ok(intersect_predicate(predicate, comparison));
    }
    let property_follows = peek_word(cursor, 1, "power") || peek_word(cursor, 1, "toughness");
    if !(peek_word(cursor, 0, "with") && property_follows) {
        return Ok(predicate);
    }
    cursor.enter("card-action.permit.query.postposed-qualifier");
    cursor.consume("with")?;
    let property = recognize_selector_numeric_property(cursor)?;
    let comparison = recognize_numeric_property_comparison_tail(cursor, property)?;
    Ok(intersect_predicate(predicate, comparison))
}

/// The permitted card noun phrase in the spell and card domains. The
/// ordinary query is read first; a comparison it cannot admit is read again
/// as a resolution-owned query (CR 608.2c), whose comparisons may read the
/// shared numeric expressions the source context proves. When neither can
/// finish the phrase with a comparison this frame owns (`with equal or
/// lesser mana value`, `with mana value less than or equal to that
/// damage`), the phrase is read up to that `with` and this frame reads the
/// comparison.
fn recognize_permitted_noun_phrase(
    cursor: &mut OracleTokenCursor<'_>,
    frame: CardActionReadContext<'_>,
) -> MachineResult<(ObjectNounPhrase, Option<ObjectPredicate>)> {
    const DOMAINS: &[ObjectKind] = &[ObjectKind::Spell, ObjectKind::Card];
    let mut first = None;
    for context in [
        ObjectQueryContext::ORDINARY,
        ObjectQueryContext::resolution(frame.source_available),
    ] {
        let expectation = ObjectNounPhraseExpectation {
            domains: DOMAINS,
            context,
        };
        let mut trial = cursor.clone();
        match recognize_object_noun_phrase(&mut trial, expectation) {
            Ok(phrase) => {
                *cursor = trial;
                // A comparison the shared reader left unread right after
                // the noun belongs to the same query.
                let comparison = if spell_comparison_starts(cursor) {
                    Some(recognize_spell_comparison_qualifier(cursor, frame)?)
                } else {
                    None
                };
                return Ok((phrase, comparison));
            }
            Err(failure) => {
                first.get_or_insert(failure);
            }
        }
    }
    let first = cursor.require_value(first, "a permitted card noun phrase")?;
    let Some(offset) = owned_comparison_offset(cursor) else {
        return Err(first);
    };
    let end = cursor.position() + offset;
    let phrase = cursor.bounded(end, |inner| {
        recognize_object_noun_phrase(inner, ObjectNounPhraseExpectation::in_domains(DOMAINS))
    })?;
    let comparison = recognize_spell_comparison_qualifier(cursor, frame)?;
    Ok((phrase, Some(comparison)))
}

/// `a|N|up to N|any number of|[bare plural] SPELL-OR-CARD-QUERY from ORIGIN`:
/// the permitted cards of a permission as one shared noun phrase (its
/// quantity and query) and an origin. A spell query (`cast`) is read in the
/// card domain. Targets, references and library positions stay with the
/// shared card operand; the phrase is committed at its `from`. `your
/// commander from the command zone` reads the shared commander predicate.
fn try_recognize_permitted_card_query(
    cursor: &mut OracleTokenCursor<'_>,
    verb: CardPermissionVerb,
    frame: CardActionReadContext<'_>,
) -> CollectionOperandAttempt<CardOperandNode> {
    let mut trial = cursor.clone();
    let mut committed = false;
    let result = (|| -> MachineResult<CardOperandNode> {
        trial.enter("card-action.permit.query");
        if your_commander_starts(&trial) {
            // `your commander from the command zone` (CR 903.3): the
            // commander designation and the controller's ownership.
            let predicate = recognize_your_commander(&mut trial)?;
            committed = true;
            trial.enter("card-action.permit.query.commander-origin");
            trial.consume("from")?;
            let origin = recognize_permitted_card_origin(&mut trial)?;
            trial.require(
                matches!(origin, PermittedCardOrigin::CommandZone),
                "the command zone as the origin of your commander",
            )?;
            return Ok(CardOperandNode::CommandZone { predicate });
        }
        let (phrase, comparison) = recognize_permitted_noun_phrase(&mut trial, frame)?;
        let quantity = match phrase.determiner {
            None | Some(ObjectDeterminer::Universal { each: false }) => {
                CardSelectionQuantityNode::All {
                    via: None,
                    except_cast: false,
                }
            }
            Some(ObjectDeterminer::Chosen {
                quantity,
                another: false,
            }) => match quantity {
                ChosenObjectQuantity::Exactly(count) => CardSelectionQuantityNode::Exactly(count),
                ChosenObjectQuantity::UpTo(count) => CardSelectionQuantityNode::UpTo(count),
                ChosenObjectQuantity::AnyNumber => CardSelectionQuantityNode::AnyNumber,
            },
            _ => return Err(trial.fail("a selection quantity or a bare plural of permitted cards")),
        };
        trial.require(
            !phrase.exclude_source && phrase.query.relations().is_empty(),
            "an unscoped spell or card query without a source exclusion",
        )?;
        let predicate = match phrase.domain {
            Some(ObjectKind::Spell) => {
                trial.require(
                    verb == CardPermissionVerb::Cast,
                    "`cast` for a permitted spell query",
                )?;
                spell_query_as_card_query(phrase.query.predicate())
            }
            Some(ObjectKind::Card) => phrase.query.predicate().clone(),
            _ => return Err(trial.fail("a spell or card query")),
        };
        let predicate = match comparison {
            Some(comparison) => intersect_predicate(predicate, comparison),
            None => predicate,
        };
        trial.enter("card-action.permit.query.origin");
        trial.consume("from")?;
        committed = true;
        let origin = recognize_permitted_card_origin(&mut trial)?;
        // `from among those nonland cards`: the restatement's qualifier
        // narrows the same query.
        let predicate = match &origin {
            PermittedCardOrigin::Collection {
                qualifier: Some(qualifier),
                ..
            } => intersect_predicate(predicate, qualifier.clone()),
            _ => predicate,
        };
        let predicate = recognize_postposed_numeric_qualifier(&mut trial, predicate, frame)?;
        trial.enter("card-action.permit.query.domain");
        trial.require(
            predicate.is_in_domain(ObjectKind::Card),
            "a card-domain query over the permitted cards",
        )?;
        let predicate =
            (predicate != ObjectPredicate::ObjectKind(ObjectKind::Card)).then_some(predicate);
        Ok(match origin {
            PermittedCardOrigin::Collection { from, .. } => {
                CardOperandNode::Selection(CardSelectionOperandNode {
                    quantity,
                    predicate,
                    from,
                })
            }
            PermittedCardOrigin::Zone {
                zone,
                owner,
                additional,
            } => CardOperandNode::ZoneCollection(CardZoneCollectionNode {
                zone,
                owner,
                predicate,
                quantity: match quantity {
                    CardSelectionQuantityNode::All { .. } => None,
                    other => Some(other),
                },
                additional_zones: additional,
            }),
            PermittedCardOrigin::CommandZone => {
                return Err(trial.fail("`your commander` as the card cast from the command zone"));
            }
        })
    })();
    match result {
        Ok(node) => {
            trial.enter("card-action.permit.query.complete");
            let end_position = trial.position();
            *cursor = trial;
            CollectionOperandAttempt::Recognized(RecognizedEffectFragment { node, end_position })
        }
        Err(failure) if committed => CollectionOperandAttempt::FailedCommitted(failure),
        Err(failure) => CollectionOperandAttempt::RejectedCandidate(failure),
    }
}

fn recognize_reveal_until(
    cursor: &mut OracleTokenCursor<'_>,
    verb: CardCollectionVerb,
) -> MachineResult<CardActionNode> {
    cursor.enter("card-action.reveal-until.source");
    cursor.consume_phrase(&["cards", "from", "the", "top", "of", "your", "library"])?;
    cursor.enter("card-action.reveal-until.condition");
    cursor.consume("until")?;
    // The acting player restated: `you`, or `they` after a player
    // performer's person shift.
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("they"))
    {
        cursor.consume("they")?;
    } else {
        cursor.consume("you")?;
    }
    cursor.consume(match verb {
        CardCollectionVerb::Revealed => "reveal",
        CardCollectionVerb::Exiled => "exile",
    })?;
    cursor.enter("card-action.reveal-until.quantity");
    // One object noun phrase: its count and card query; this parent
    // type-checks a chosen exact count of unscoped cards.
    let phrase = recognize_object_noun_phrase(
        cursor,
        ObjectNounPhraseExpectation::in_domains(&[ObjectKind::Card]),
    )?;
    let quantity = match phrase.determiner {
        Some(ObjectDeterminer::Chosen {
            quantity: ChosenObjectQuantity::Exactly(quantity),
            another: false,
        }) if !phrase.exclude_source => quantity,
        _ => return Err(cursor.fail("`a` or a count of revealed cards")),
    };
    if let NumericExpression::Atom(NumericAtom::Integer(value)) = &quantity {
        cursor.require(*value > 0, "a positive reveal-until quantity")?;
    }
    // Exiling stops at the first matching card (one exile-until writer).
    cursor.require(
        verb == CardCollectionVerb::Revealed || quantity == NumericExpression::from(1),
        "one matching card ends an exile-until",
    )?;
    cursor.enter("card-action.reveal-until.predicate");
    cursor.require(
        phrase.query.relations().is_empty() && phrase.domain == Some(ObjectKind::Card),
        "an unscoped card-domain predicate for the revealed cards",
    )?;
    Ok(CardActionNode::RevealUntil {
        quantity,
        predicate: phrase.query.predicate().clone(),
        owner: LocalPlayerReference::Controller,
        verb,
    })
}

/// `its|the spell's|that spell's mana value`: the mana value of what the
/// frame casts (CR 202.3), the measure of a non-mana alternative cost.
fn recognize_cast_spell_mana_value(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("card-action.permit.alternative-cost.mana-value");
    if peek_word(cursor, 0, "its") {
        cursor.consume("its")?;
    } else {
        let determiner = if peek_word(cursor, 0, "that") {
            "that"
        } else {
            "the"
        };
        cursor.consume(determiner)?;
        let possessive = cursor
            .peek()
            .filter(|raw| {
                raw.eq_ignore_ascii_case("spell's") || raw.eq_ignore_ascii_case("spell\u{2019}s")
            })
            .map(str::to_owned)
            .ok_or_else(|| cursor.fail("`its`, `the spell's` or `that spell's` mana value"))?;
        cursor.consume(&possessive)?;
    }
    cursor.consume_phrase(&["mana", "value"])
}

/// The cost an alternative base cost pays instead of the mana cost: a mana
/// cost (the shared reader), `life equal to MANA-VALUE`, `an amount of {E}
/// equal to MANA-VALUE` (CR 107.14) or `discarding N card(s)`.
fn recognize_alternative_cost_payment(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardCastCostNode> {
    if peek_word(cursor, 0, "by") && peek_word(cursor, 1, "discarding") {
        cursor.enter("card-action.permit.alternative-cost.discard");
        cursor.consume_phrase(&["by", "discarding"])?;
        let count = if peek_word(cursor, 0, "a") {
            cursor.consume("a")?;
            NumericExpression::from(1)
        } else {
            recognize_numeric_expression(cursor, None)?
        };
        let singular = count == NumericExpression::from(1);
        cursor.consume(if singular { "card" } else { "cards" })?;
        return Ok(CardCastCostNode::AlternativeNonMana(
            NonManaCastCostNode::Discard(count),
        ));
    }
    cursor.enter("card-action.permit.alternative-cost");
    cursor.consume_phrase(&["by", "paying"])?;
    if peek_word(cursor, 0, "life") {
        cursor.enter("card-action.permit.alternative-cost.life");
        cursor.consume_phrase(&["life", "equal", "to"])?;
        recognize_cast_spell_mana_value(cursor)?;
        return Ok(CardCastCostNode::AlternativeNonMana(
            NonManaCastCostNode::LifeEqualToManaValue,
        ));
    }
    if peek_word(cursor, 0, "an") && peek_word(cursor, 1, "amount") {
        cursor.enter("card-action.permit.alternative-cost.counters");
        cursor.consume_phrase(&["an", "amount", "of"])?;
        let symbol = cursor.consume_type("counterSymbol")?;
        cursor.require(
            symbol.raw.eq_ignore_ascii_case("{E}"),
            "the registered energy-counter symbol `{E}`",
        )?;
        cursor.consume_phrase(&["equal", "to"])?;
        recognize_cast_spell_mana_value(cursor)?;
        let counter = crate::oracle::counter_name::CounterName::from_parts(["energy"])
            .expect("a non-empty counter name");
        return Ok(CardCastCostNode::AlternativeNonMana(
            NonManaCastCostNode::PlayerCountersEqualToManaValue(counter),
        ));
    }
    Ok(CardCastCostNode::Alternative(recognize_mana_cost(cursor)?))
}

/// `by paying COST | by discarding N cards rather than paying its|their mana
/// cost(s)` (CR 118.9): the substituted cost, then the replaced mana cost.
fn recognize_alternative_base_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<CardCastCostNode>> {
    if !(peek_word(cursor, 0, "by")
        && (peek_word(cursor, 1, "paying") || peek_word(cursor, 1, "discarding")))
    {
        return Ok(None);
    }
    let cost = recognize_alternative_cost_payment(cursor)?;
    cursor.enter("card-action.permit.alternative-cost.replaced");
    cursor.consume_phrase(&["rather", "than", "paying"])?;
    let (possessive, noun) = if peek_word(cursor, 0, "their") {
        ("their", "costs")
    } else {
        ("its", "cost")
    };
    cursor.consume_phrase(&[possessive, "mana", noun])?;
    Ok(Some(cost))
}

/// The base-cost policy of the frame, where Oracle states it.
fn recognize_cast_cost_policy(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardCastCostNode> {
    if recognize_without_paying(cursor)? {
        return Ok(CardCastCostNode::WithoutManaCost);
    }
    Ok(recognize_alternative_base_cost(cursor)?.unwrap_or(CardCastCostNode::Printed))
}

/// `if able` after an imperative cast (CR 608.2g): the player must cast
/// the cards when a legal cast exists. Consumed only when complete.
fn recognize_if_able(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<bool> {
    if !(peek_word(cursor, 0, "if") && peek_word(cursor, 1, "able")) {
        return Ok(false);
    }
    cursor.enter("card-action.cast.if-able");
    cursor.consume_phrase(&["if", "able"])?;
    Ok(true)
}

/// The end of a frame without a stated window: the sentence end, a
/// spending-policy coordinator, or a postposed `if CONDITION` that the
/// enclosing sequence owns (CR 608.2c).
fn permission_ends_without_window(cursor: &OracleTokenCursor<'_>) -> bool {
    matches!(cursor.peek(), None | Some("."))
        || any_type_spending_starts(cursor)
        || peek_word(cursor, 0, "if")
        // `, where X is EXPR` (CR 107.3): the sentence owns the definition.
        || where_x_definition_starts(cursor)
}

/// Lexical lookahead for the graveyard-exile sentence that amends the
/// spells this frame casts: `. If ... would be put into ...`.
fn cast_graveyard_exile_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    peek_word(cursor, 0, ".")
        && peek_word(cursor, 1, "if")
        && (2..12).any(|offset| {
            peek_word(cursor, offset, "would")
                && peek_word(cursor, offset + 1, "be")
                && peek_word(cursor, offset + 2, "put")
                && peek_word(cursor, offset + 3, "into")
        })
}

/// `. If SPELLS would be put into GRAVEYARD, exile it|them instead`: the
/// replacement the cast spells carry (CR 614.1a). SPELLS restates them:
/// `that spell`, `those spells`, or `a [PREDICATE] spell cast this way`,
/// whose predicate is read by the shared noun phrase in the spell domain
/// and checked on the card (CR 601.2a: same characteristics). The final
/// sentence terminator stays with the enclosing sequence.
fn recognize_cast_graveyard_exile(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CastGraveyardExileNode> {
    cursor.enter("card-action.cast.graveyard-exile");
    cursor.consume(".")?;
    cursor.consume("if")?;
    cursor.enter("card-action.cast.graveyard-exile.spells");
    let (predicate, plural) = if peek_word(cursor, 0, "that") {
        cursor.consume_phrase(&["that", "spell"])?;
        (None, false)
    } else if peek_word(cursor, 0, "those") {
        cursor.consume_phrase(&["those", "spells"])?;
        (None, true)
    } else {
        // `a|an [PREDICATE] spell cast this way`: the noun phrase is read on
        // a bounded view that ends before its `cast this way` participle.
        let length = (1..10)
            .find(|offset| {
                peek_word(cursor, *offset, "cast")
                    && peek_word(cursor, offset + 1, "this")
                    && peek_word(cursor, offset + 2, "way")
            })
            .ok_or_else(|| {
                cursor.fail("`that spell`, `those spells` or `a spell cast this way`")
            })?;
        let end = cursor.position() + length;
        let phrase = cursor.bounded(end, |inner| {
            recognize_object_noun_phrase(
                inner,
                ObjectNounPhraseExpectation::in_domains(&[ObjectKind::Spell]),
            )
        })?;
        cursor.require(
            matches!(
                phrase.determiner,
                Some(ObjectDeterminer::Chosen {
                    quantity: ChosenObjectQuantity::Exactly(_),
                    another: false,
                })
            ) && !phrase.exclude_source
                && phrase.query.relations().is_empty()
                && phrase.domain == Some(ObjectKind::Spell),
            "an unscoped spell query restating the spells cast this way",
        )?;
        let predicate = spell_query_as_card_query(phrase.query.predicate());
        cursor.consume_phrase(&["cast", "this", "way"])?;
        (
            (predicate != ObjectPredicate::ObjectKind(ObjectKind::Card)).then_some(predicate),
            false,
        )
    };
    cursor.enter("card-action.cast.graveyard-exile.event");
    cursor.consume_phrase(&["would", "be", "put", "into"])?;
    let graveyard = if peek_word(cursor, 0, "your") {
        cursor.consume("your")?;
        CastGraveyardOwnerNode::Controller
    } else if peek_word(cursor, 0, "a") || peek_word(cursor, 0, "their") {
        let word = cursor.peek().unwrap_or("a").to_string();
        cursor.consume(&word)?;
        CastGraveyardOwnerNode::Any
    } else {
        cursor.consume_phrase(&["its", "owner's"])?;
        CastGraveyardOwnerNode::Any
    };
    cursor.consume("graveyard")?;
    cursor.enter("card-action.cast.graveyard-exile.replacement");
    cursor.consume_phrase(&[",", "exile"])?;
    // A plural restatement takes `them`; `a spell cast this way` / `that
    // spell` takes `it`.
    let pronoun = if plural || peek_word(cursor, 0, "them") {
        "them"
    } else {
        "it"
    };
    cursor.require(
        plural == (pronoun == "them"),
        "a pronoun agreeing with the restated spells",
    )?;
    cursor.consume(pronoun)?;
    cursor.consume("instead")?;
    cursor.enter("card-action.cast.graveyard-exile.terminator");
    cursor.require(
        matches!(cursor.peek(), None | Some(".")),
        "the end of the replacement sentence",
    )?;
    Ok(CastGraveyardExileNode {
        predicate,
        graveyard,
    })
}

fn recognize_card_permission(
    cursor: &mut OracleTokenCursor<'_>,
    frame: CardActionReadContext<'_>,
    preposed_window: Option<CardPermissionWindowNode>,
) -> MachineResult<CardPermissionNode> {
    recognize_card_permission_frame(cursor, frame, preposed_window, &mut false)
}

/// Whether the cast frame's own tail starts after its cards: a base-cost
/// policy, a window, a spending policy or the graveyard exile sentence.
/// Pure lookahead; it is the frame's commit point when its cards were read
/// by a shared operand that another frame could also have read.
fn permission_tail_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    peek_word(cursor, 0, "without")
        || (peek_word(cursor, 0, "by")
            && (peek_word(cursor, 1, "paying") || peek_word(cursor, 1, "discarding")))
        || permission_window_starts(cursor)
        || any_type_spending_starts(cursor)
        || cast_graveyard_exile_starts(cursor)
}

/// The cast/play frame with its commit point reported: `committed` is set
/// once the frame proved it owns the clause (the permitted query read its
/// `from`, or a tail only this frame reads follows its cards). A failure
/// before that point leaves the clause to other frames.
fn recognize_card_permission_frame(
    cursor: &mut OracleTokenCursor<'_>,
    frame: CardActionReadContext<'_>,
    preposed_window: Option<CardPermissionWindowNode>,
    committed: &mut bool,
) -> MachineResult<CardPermissionNode> {
    let supplied_duration = frame.preposed_duration;
    cursor.enter("card-action.permit.head");
    let optional = peek_word(cursor, 0, "you");
    if optional {
        cursor.consume_phrase(&["you", "may"])?;
    }
    let verb = if optional
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("play"))
    {
        cursor.consume("play")?;
        CardPermissionVerb::Play
    } else {
        cursor.consume("cast")?;
        CardPermissionVerb::Cast
    };
    cursor.enter("card-action.permit.cards");
    let cards = match try_recognize_permitted_card_query(cursor, verb, frame) {
        CollectionOperandAttempt::Recognized(fragment) => {
            *committed = true;
            fragment.node
        }
        CollectionOperandAttempt::FailedCommitted(failure) => {
            *committed = true;
            return Err(failure);
        }
        CollectionOperandAttempt::NotClaimed | CollectionOperandAttempt::RejectedCandidate(_) => {
            let cards = operand(cursor)?;
            *committed |= permission_tail_starts(cursor);
            cards
        }
    };
    cursor.enter("card-action.permit.object");
    cursor.require(
        matches!(
            cards,
            CardOperandNode::Reference(_)
                | CardOperandNode::ObjectAntecedent { .. }
                | CardOperandNode::Selection(_)
                | CardOperandNode::ZoneCollection(_)
                | CardOperandNode::Targets(_)
                | CardOperandNode::CardCopies { .. }
                | CardOperandNode::CommandZone { .. }
        ),
        "a bound card collection, declared card targets, card copies, your commander or a quantified card query as the cast object",
    )?;
    let mut if_able = recognize_if_able(cursor)?;
    let mut cost = recognize_cast_cost_policy(cursor)?;
    cursor.enter("card-action.permit.window");
    let (duration, window) = if supplied_duration.is_some() || preposed_window.is_some() {
        cursor.require(
            !permission_window_starts(cursor),
            "no second duration after a sentence-level preposed duration",
        )?;
        (supplied_duration.cloned(), preposed_window)
    } else if permission_window_starts(cursor) {
        recognize_trailing_permission_window(cursor, frame)?
    } else if permission_ends_without_window(cursor) {
        (None, None)
    } else {
        (Some(recognize_play_window_duration(cursor)?), None)
    };
    if cost == CardCastCostNode::Printed {
        // `this turn without paying their mana costs`: the base-cost policy
        // may follow the window.
        cost = recognize_cast_cost_policy(cursor)?;
    }
    if !if_able {
        if_able = recognize_if_able(cursor)?;
    }
    cursor.enter("card-action.permit.spending");
    let mana_of_any_type = recognize_any_type_spending(cursor)?;
    cursor.enter("card-action.cast.optionality");
    cursor.require(
        optional || (duration.is_none() && window.is_none()),
        "an imperative cast performed while the effect resolves, without a window",
    )?;
    cursor.require(
        !(optional && if_able),
        "`if able` only after an imperative cast",
    )?;
    let graveyard_exile = if cast_graveyard_exile_starts(cursor) {
        Some(recognize_cast_graveyard_exile(cursor)?)
    } else {
        None
    };
    Ok(CardPermissionNode {
        verb,
        cards,
        duration,
        window,
        optional,
        cost,
        mana_of_any_type,
        graveyard_exile,
    })
}

pub(in crate::oracle::canonical) fn recognize_card_action_destination(
    cursor: &mut OracleTokenCursor<'_>,
    verb: &str,
) -> MachineResult<CardMoveDestinationNode> {
    if is_verb_form(&verb, VerbLemma::Exile, VerbForm::Base) {
        // `exile OBJECT face down`: the shared entry-modifier list of the
        // exile destination (CR 406.3).
        let mut modifiers = EntryModifiers::new(CardMoveOwnerNode::EachCardOwner);
        recognize_entry_modifiers(cursor, CardZoneName::Exile, &mut modifiers)?;
        return Ok(CardMoveDestinationNode {
            zone: CardZoneName::Exile,
            owner: CardMoveOwnerNode::EachCardOwner,
            position: None,
            order: None,
            tapped: false,
            entering_counters: modifiers.entering_counters,
            face_down: modifiers.face_down,
            transformed: false,
            shuffle: false,
            attached_to: None,
            origin_library: false,
            attacking: None,
        });
    }
    cursor.enter("card-action.move.destination");
    match try_recognize_card_move_destination(cursor) {
        CardMoveDestinationAttempt::Recognized(fragment) => Ok(fragment.node),
        CardMoveDestinationAttempt::RejectedCandidate(failure)
        | CardMoveDestinationAttempt::FailedCommitted(failure) => return Err(failure),
        CardMoveDestinationAttempt::NotClaimed => {
            return Err(cursor.fail("a typed card destination"));
        }
    }
}

/// Permission owns one complete sentence, including all its coordinated actions.
/// The bounded cursor borrows original tokens and spans; it never retokenizes.
pub(in crate::oracle::canonical) fn recognize_optional_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectSequenceNode> {
    cursor.enter("program.optional.sentence");
    cursor.consume_phrase(&["you", "may"])?;
    let start = cursor.position();
    let mut end = start;
    let mut parentheses = 0usize;
    let mut quoted = false;
    loop {
        match cursor.peek_ahead(end - start) {
            Some("\"") => quoted = !quoted,
            Some("(") if !quoted => parentheses += 1,
            Some(")") if !quoted => parentheses = parentheses.saturating_sub(1),
            Some(".") if !quoted && parentheses == 0 => break,
            None => return Err(cursor.fail("a complete optional sentence")),
            _ => {}
        }
        end += 1;
    }
    let mut body_cursor =
        OracleTokenCursor::new("optional-sentence-body", cursor.token_slice(start, end + 1));
    let body = recognize_effect_sequence_with_context(
        &mut body_cursor,
        name_links,
        name_failure,
        context,
    )?;
    cursor.advance_to(end)?;
    Ok(body)
}

#[cfg(test)]
#[path = "card_action_outcome_tests.rs"]
mod outcome_tests;
