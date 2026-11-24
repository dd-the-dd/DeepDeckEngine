use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CardZoneName, CollectionBinding, EffectRecognitionContext, LibrarySearchEffectNode,
    LocalEffectNode, LocalPlayerReference, ObjectKind, ObjectPredicate, SearchDestination,
    SearchShuffleOrder, TargetBinding, TargetCardinality, TargetListNode,
    expressed_target_count_starts, recognize_card_zone_atom, recognize_card_zone_list,
    recognize_optional_sequence, recognize_player_reference, recognize_possessive_target_player,
    recognize_search_card_alternatives, recognize_target_cardinality, recognize_target_count,
};

/// The zones of the searched player a search looks through (CR 701.23a):
/// the library alone, or the library with other zones of the same player
/// (`your library and/or graveyard`, `your graveyard, hand, and library`).
/// The written list is read by the shared zone-list reader; the searched
/// library is always one of its zones, and a search with it is followed by
/// the library's shuffle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SearchOrigin {
    /// Whose zones are searched (CR 701.23a).
    pub holder: SearchedPlayer,
    pub zones: Vec<CardZoneName>,
}

/// The player whose zones a search looks through: the searching player's own
/// (`your library`), or one target player the possessive declares (`target
/// player's library`, CR 115.1). The possessive is the shared target-list
/// reader's; the declared player is the program's player antecedent, so a
/// later `that player shuffles` names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SearchedPlayer {
    Searcher,
    DeclaredTarget(TargetListNode),
}

impl SearchOrigin {
    pub(in crate::oracle::canonical) fn library() -> Self {
        Self {
            holder: SearchedPlayer::Searcher,
            zones: vec![CardZoneName::Library],
        }
    }

    pub(in crate::oracle::canonical) fn is_library_only(&self) -> bool {
        self.zones == [CardZoneName::Library]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LibrarySearchState {
    Player,
    Permission,
    SearchAction,
    PossessiveLibrary,
    OriginZones,
    FilterArticle,
    Filter,
    /// The selection is read: the written tail of this frame, or the end of
    /// the action when later ordinary actions read the bound result.
    SelectionComplete,
    EffectSeparator,
    PostFilterAction,
    ExileReference,
    ExileCompletion,
    RevealReference,
    RevealSeparator,
    ShuffleAction,
    ShuffleConjunction,
    MoveAction,
    ResultReference,
    Destination,
    DestinationModifier,
    PostMoveShuffleSeparator,
    PostMoveShuffleAction,
    ConditionalShuffle,
    Complete,
}

#[derive(Clone, Default)]
struct LibrarySearchMemory {
    player: Option<LocalPlayerReference>,
    holder: Option<SearchedPlayer>,
    origin: Option<SearchOrigin>,
    filter: Option<ObjectPredicate>,
    slots: Vec<ObjectPredicate>,
    cardinality: Option<TargetCardinality>,
    /// The typed count when it is not two positive literals.
    count: Option<super::TargetCount>,
    destination: Option<SearchDestination>,
    reveal: bool,
    shuffle_order: Option<SearchShuffleOrder>,
}

/// How a search frame may end after its selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchTailPolicy {
    /// The frame reads its own written move and shuffle (historical
    /// contract of reminder and activated parents).
    Complete,
    /// The frame reads its written tail when one completes; otherwise the
    /// action ends after its selection and binds the found cards for the
    /// ordinary actions that follow (CR 701.23a, 608.2c).
    CompleteOrBound,
}

/// Recognizes an optional search of the referenced player's own library,
/// followed by moving the selected result and shuffling that same library.
/// The result pronoun is resolved locally; lowering assigns the decision id.
pub(in crate::oracle::canonical) fn recognize_optional_library_search(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<LibrarySearchEffectNode> {
    recognize_library_search(cursor, antecedent, false, SearchTailPolicy::Complete)
}

/// Recognizes a mandatory search of the controller's library. The search
/// body is shared with optional target-controller searches so filters,
/// anaphora, destinations, and shuffle sequencing evolve only once.
pub(in crate::oracle::canonical) fn recognize_controller_library_search(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LibrarySearchEffectNode> {
    recognize_library_search(cursor, None, true, SearchTailPolicy::Complete)
}

/// `search ZONES for SELECTION` as one ordinary action of an effect
/// sequence: the written move/shuffle tail when it completes, otherwise a
/// selection whose found cards are bound ([`SearchDestination::Bound`]).
/// The actions after it (`reveal it`, `put it onto the battlefield under
/// your control`, `exile them`, `then shuffle`, `Then that player
/// shuffles.`) are the sequence's own actions over that binding.
pub(in crate::oracle::canonical) fn recognize_library_search_action(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LibrarySearchEffectNode> {
    recognize_library_search(cursor, None, true, SearchTailPolicy::CompleteOrBound)
}

/// `you may search ...` read by the clause layer: the historical optional
/// search with its written tail, or, when that tail does not complete, the
/// permission over its whole sentence (CR 608.2d): the search binding its
/// found cards, then the sentence's ordinary actions over them, all inside
/// the one optional block. A possessed player's permission (`its controller
/// may search`) keeps the written-tail contract only.
pub(in crate::oracle::canonical) fn recognize_optional_library_search_program(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<LocalEffectNode> {
    let mut strict = cursor.clone();
    let strict_failure = match recognize_optional_library_search(&mut strict, antecedent) {
        Ok(search) => {
            *cursor = strict;
            return Ok(LocalEffectNode::OptionalLibrarySearch(search));
        }
        Err(failure) => failure,
    };
    let controller_permission = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("may"));
    if !controller_permission {
        return Err(strict_failure);
    }
    // CR 118.12: an `unless` payment inside a permission body has no
    // represented order; the body keeps refusing it (as every `you may`
    // sentence does).
    let mut permission_context = context.clone();
    permission_context.unless_frames = super::UnlessFrameSupport::Unsupported;
    let mut body = cursor.clone();
    let sequence =
        recognize_optional_sequence(&mut body, name_links, name_failure, &permission_context)?;
    body.enter("effect.library-search.optional-program");
    body.require(
        matches!(
            sequence.effects.first(),
            Some(LocalEffectNode::ControllerLibrarySearch(_))
        ),
        "the permitted search as the first action of its sentence",
    )?;
    *cursor = body;
    Ok(LocalEffectNode::OptionalSequence(sequence))
}

/// Whether a search node ends at its selection, its found cards bound for
/// the ordinary actions that follow it.
pub(in crate::oracle::canonical) fn search_binds_found_cards(
    search: &LibrarySearchEffectNode,
) -> bool {
    search.destination == SearchDestination::Bound
}

fn recognize_library_search(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
    controller_owned: bool,
    policy: SearchTailPolicy,
) -> MachineResult<LibrarySearchEffectNode> {
    run_library_search(
        cursor,
        LibrarySearchState::Player,
        LibrarySearchMemory::default(),
        antecedent,
        controller_owned,
        policy,
    )
}

fn run_library_search(
    cursor: &mut OracleTokenCursor<'_>,
    start: LibrarySearchState,
    memory: LibrarySearchMemory,
    antecedent: Option<TargetBinding>,
    controller_owned: bool,
    policy: SearchTailPolicy,
) -> MachineResult<LibrarySearchEffectNode> {
    let mut memory = memory;
    let mut state = start;
    loop {
        state = match state {
            LibrarySearchState::Player => {
                cursor.enter("effect.library-search.player");
                if controller_owned {
                    memory.player = Some(LocalPlayerReference::Controller);
                    LibrarySearchState::SearchAction
                } else {
                    memory.player = Some(recognize_player_reference(cursor, antecedent)?);
                    LibrarySearchState::Permission
                }
            }
            LibrarySearchState::Permission => {
                cursor.enter("effect.library-search.permission");
                cursor.consume("may")?;
                LibrarySearchState::SearchAction
            }
            LibrarySearchState::SearchAction => {
                cursor.enter("effect.library-search.action");
                cursor.consume("search")?;
                LibrarySearchState::PossessiveLibrary
            }
            LibrarySearchState::PossessiveLibrary => {
                cursor.enter("effect.library-search.possessive-library");
                // The controller's own search may look through a declared
                // player's zones; a referenced player searches their own.
                if controller_owned
                    && cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("target"))
                {
                    let target = recognize_possessive_target_player(cursor)?;
                    memory.holder = Some(SearchedPlayer::DeclaredTarget(target));
                } else {
                    cursor.consume(library_player_possessive(
                        memory.player.expect("recognized searching player"),
                    ))?;
                    memory.holder = Some(SearchedPlayer::Searcher);
                }
                LibrarySearchState::OriginZones
            }
            LibrarySearchState::OriginZones => {
                // One possessive, then the shared zone list: every listed
                // zone is the searched player's own.
                cursor.enter("effect.library-search.origin-zones");
                let zones = recognize_card_zone_list(cursor)?;
                cursor.enter("effect.library-search.origin-library");
                cursor.require(
                    zones.contains(&CardZoneName::Library),
                    "the searched player's library among the searched zones",
                )?;
                cursor.require(
                    zones.iter().all(|zone| {
                        matches!(
                            zone,
                            CardZoneName::Library | CardZoneName::Graveyard | CardZoneName::Hand
                        )
                    }),
                    "the searched player's own library, graveyard or hand",
                )?;
                memory.origin = Some(SearchOrigin {
                    holder: memory.holder.take().expect("recognized searched player"),
                    zones,
                });
                cursor.enter("effect.library-search.origin-complete");
                cursor.consume("for")?;
                LibrarySearchState::FilterArticle
            }
            LibrarySearchState::FilterArticle => {
                cursor.enter("effect.library-search.filter-article");
                // `up to X`, `X`, `that many`: the shared typed count; the
                // literal envelope keeps every literal reader plural.
                if expressed_target_count_starts(cursor) {
                    // `any number of` (CR 701.23a): no maximum below the
                    // searched zones' matching cards; the lowering counts them.
                    let count = recognize_target_count(cursor)?;
                    cursor.enter("effect.library-search.expressed-count");
                    if let Some(literal) = count.literal() {
                        memory.cardinality = Some(literal);
                    } else {
                        memory.cardinality = Some(TargetCardinality::EXPRESSED);
                        memory.count = Some(count);
                    }
                } else {
                    let cardinality = recognize_target_cardinality(cursor)?;
                    cursor.enter("effect.library-search.cardinality-bounds");
                    cursor.require(
                        cardinality.maximum <= i32::MAX as u32,
                        "a search cardinality representable by the runtime count domain",
                    )?;
                    memory.cardinality = Some(cardinality);
                }
                LibrarySearchState::Filter
            }
            LibrarySearchState::Filter => {
                cursor.enter("effect.library-search.filter");
                let cardinality = memory.cardinality.expect("recognized search cardinality");
                // One card phrase of the shared card selector, or separately
                // headed alternatives sharing this cardinality.
                let first = recognize_search_card_alternatives(cursor, cardinality.maximum != 1)?;
                // `a Forest card and a Plains card`, `an Island card, a
                // Swamp card, and a Mountain card`: one card for each
                // separately determined phrase (CR 701.23a).
                let mut slots = Vec::new();
                if cardinality.minimum == 1 && cardinality.maximum == 1 {
                    while let Some(slot) = recognize_additional_search_slot(cursor) {
                        slots.push(slot);
                    }
                }
                if slots.is_empty() {
                    memory.filter = Some(first);
                } else {
                    cursor.enter("effect.library-search.slots");
                    slots.insert(0, first);
                    let count = u32::try_from(slots.len()).map_err(|_| {
                        cursor.fail("a search slot count representable by the runtime count domain")
                    })?;
                    memory.cardinality = Some(TargetCardinality {
                        minimum: count,
                        maximum: count,
                    });
                    memory.filter = Some(ObjectPredicate::Union(slots.clone()));
                    memory.slots = slots;
                }
                LibrarySearchState::SelectionComplete
            }
            LibrarySearchState::SelectionComplete => {
                cursor.enter("effect.library-search.selection-complete");
                match policy {
                    SearchTailPolicy::Complete => LibrarySearchState::EffectSeparator,
                    SearchTailPolicy::CompleteOrBound => {
                        // The written tail is a transactional child: when it
                        // completes it is this frame's; otherwise the action
                        // ends here and nothing after the selection is read.
                        let mut trial = cursor.clone();
                        if let Ok(node) = run_library_search(
                            &mut trial,
                            LibrarySearchState::EffectSeparator,
                            memory.clone(),
                            antecedent,
                            controller_owned,
                            SearchTailPolicy::Complete,
                        ) {
                            *cursor = trial;
                            return Ok(node);
                        }
                        cursor.enter("effect.library-search.bound-result");
                        memory.destination = Some(SearchDestination::Bound);
                        memory.shuffle_order = Some(SearchShuffleOrder::ByHolderInstruction);
                        LibrarySearchState::Complete
                    }
                }
            }
            LibrarySearchState::EffectSeparator => {
                cursor.enter("effect.library-search.effect-separator");
                // `... and exile them`, `... and put it onto the battlefield`,
                // `... and reveal it`: the coordinated action on the found
                // cards; every other action follows its comma.
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                    && cursor.peek_ahead(1).is_some_and(|raw| {
                        raw.eq_ignore_ascii_case("exile")
                            || raw.eq_ignore_ascii_case("put")
                            || raw.eq_ignore_ascii_case("reveal")
                    })
                {
                    cursor.consume("and")?;
                } else {
                    cursor.consume(",")?;
                }
                LibrarySearchState::PostFilterAction
            }
            LibrarySearchState::PostFilterAction => {
                cursor.enter("effect.library-search.post-filter-action");
                // `reveal it, and put it ...` / `reveal it, then put it ...`:
                // the move follows the reveal in the same sequence.
                if memory.reveal
                    && cursor.peek().is_some_and(|raw| {
                        raw.eq_ignore_ascii_case("and") || raw.eq_ignore_ascii_case("then")
                    })
                    && cursor
                        .peek_ahead(1)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("put"))
                {
                    let connector = cursor.peek().expect("checked connector").to_string();
                    cursor.consume(&connector)?;
                }
                match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                    Some("reveal") => {
                        crate::oracle::canonical::automata::consume_lemma(
                            cursor,
                            crate::oracle::canonical::automata::VerbLemma::Reveal,
                            crate::oracle::canonical::automata::VerbAgreement::Base,
                        )?;
                        memory.reveal = true;
                        LibrarySearchState::RevealReference
                    }
                    Some("put") => {
                        memory.shuffle_order = Some(SearchShuffleOrder::MoveThenShuffle);
                        LibrarySearchState::MoveAction
                    }
                    // `exile it|them, then shuffle` (CR 406.1): the move of
                    // the found cards to exile, the same move as `put`.
                    Some("exile") => {
                        crate::oracle::canonical::automata::consume_lemma(
                            cursor,
                            crate::oracle::canonical::automata::VerbLemma::Exile,
                            crate::oracle::canonical::automata::VerbAgreement::Base,
                        )?;
                        memory.shuffle_order = Some(SearchShuffleOrder::MoveThenShuffle);
                        memory.destination = Some(SearchDestination::Zone(CardZoneName::Exile));
                        LibrarySearchState::ExileReference
                    }
                    Some("then") => {
                        cursor.consume("then")?;
                        memory.shuffle_order = Some(SearchShuffleOrder::ShuffleThenMove);
                        LibrarySearchState::ShuffleAction
                    }
                    _ => Err(cursor.fail("`reveal`, `put`, `exile`, or `then shuffle`"))?,
                }
            }
            LibrarySearchState::ExileReference => {
                cursor.enter("effect.library-search.exile-reference");
                recognize_search_result_reference(
                    cursor,
                    memory
                        .cardinality
                        .expect("recognized search cardinality")
                        .maximum
                        != 1,
                )?;
                LibrarySearchState::ExileCompletion
            }
            LibrarySearchState::ExileCompletion => {
                cursor.enter("effect.library-search.exile-completion");
                if cursor.peek() == Some(",") {
                    LibrarySearchState::PostMoveShuffleSeparator
                } else {
                    // `Search target player's library for X and exile them.
                    // Then that player shuffles.`: the searched player's own
                    // later instruction shuffles (CR 701.20a); only a
                    // declared player's library leaves the shuffle to it.
                    cursor.require(
                        matches!(
                            memory.origin.as_ref().map(|origin| &origin.holder),
                            Some(SearchedPlayer::DeclaredTarget(_))
                        ),
                        "`, then shuffle` after exiling from the searcher's own library",
                    )?;
                    memory.shuffle_order = Some(SearchShuffleOrder::ByHolderInstruction);
                    LibrarySearchState::Complete
                }
            }
            LibrarySearchState::RevealReference => {
                cursor.enter("effect.library-search.reveal-reference");
                recognize_search_result_reference(
                    cursor,
                    memory
                        .cardinality
                        .expect("recognized search cardinality")
                        .maximum
                        != 1,
                )?;
                LibrarySearchState::RevealSeparator
            }
            LibrarySearchState::RevealSeparator => {
                cursor.enter("effect.library-search.reveal-separator");
                cursor.consume(",")?;
                LibrarySearchState::PostFilterAction
            }
            LibrarySearchState::ShuffleAction => {
                cursor.enter("effect.library-search.shuffle-action");
                cursor.consume("shuffle")?;
                LibrarySearchState::ShuffleConjunction
            }
            LibrarySearchState::ShuffleConjunction => {
                cursor.enter("effect.library-search.shuffle-conjunction");
                cursor.consume("and")?;
                LibrarySearchState::MoveAction
            }
            LibrarySearchState::MoveAction => {
                cursor.enter("effect.library-search.move-action");
                crate::oracle::canonical::automata::consume_lemma(
                    cursor,
                    crate::oracle::canonical::automata::VerbLemma::Put,
                    crate::oracle::canonical::automata::VerbAgreement::Base,
                )?;
                LibrarySearchState::ResultReference
            }
            LibrarySearchState::ResultReference => {
                cursor.enter("effect.library-search.result-reference");
                recognize_search_result_reference(
                    cursor,
                    memory
                        .cardinality
                        .expect("recognized search cardinality")
                        .maximum
                        != 1,
                )?;
                LibrarySearchState::Destination
            }
            LibrarySearchState::Destination => {
                cursor.enter("effect.library-search.destination");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("into"))
                {
                    cursor.consume("into")?;
                    cursor.consume(library_player_possessive(
                        memory.player.expect("recognized searching player"),
                    ))?;
                    let surface = cursor.peek().map(str::to_owned);
                    let zone = recognize_card_zone_atom(cursor)?;
                    cursor.require(
                        matches!(zone, CardZoneName::Hand | CardZoneName::Graveyard)
                            && surface
                                .as_deref()
                                .is_some_and(|word| word.eq_ignore_ascii_case(zone.ir_name())),
                        "a singular searched-player hand or graveyard destination",
                    )?;
                    memory.destination = Some(SearchDestination::Zone(zone));
                    LibrarySearchState::PostMoveShuffleSeparator
                } else if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("onto"))
                {
                    cursor.consume_phrase(&["onto", "the", "battlefield"])?;
                    memory.destination = Some(SearchDestination::Battlefield { tapped: false });
                    LibrarySearchState::DestinationModifier
                } else {
                    cursor.consume_phrase(&["on", "top"])?;
                    if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
                    {
                        cursor.consume_phrase(&[
                            "of",
                            library_player_possessive(
                                memory.player.expect("recognized searching player"),
                            ),
                            "library",
                        ])?;
                    }
                    cursor.require(
                        memory.shuffle_order == Some(SearchShuffleOrder::ShuffleThenMove),
                        "a library-top move after shuffling the searched library",
                    )?;
                    if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("in"))
                    {
                        // `put those cards on top in any order`: the searcher
                        // orders the cards put on its own library (CR 401.4);
                        // the lowering orders every plural placement.
                        cursor.enter("effect.library-search.top-order");
                        cursor.require(
                            memory
                                .cardinality
                                .expect("recognized search cardinality")
                                .maximum
                                != 1,
                            "an order only for several cards put on top",
                        )?;
                        cursor.consume_phrase(&["in", "any", "order"])?;
                    }
                    memory.destination = Some(SearchDestination::LibraryTop);
                    LibrarySearchState::Complete
                }
            }
            LibrarySearchState::DestinationModifier => {
                cursor.enter("effect.library-search.destination-modifier");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("tapped"))
                {
                    cursor.consume("tapped")?;
                    memory.destination = Some(SearchDestination::Battlefield { tapped: true });
                }
                LibrarySearchState::PostMoveShuffleSeparator
            }
            LibrarySearchState::PostMoveShuffleSeparator => {
                cursor.enter("effect.library-search.post-move-shuffle-separator");
                cursor.require(
                    memory.shuffle_order == Some(SearchShuffleOrder::MoveThenShuffle),
                    "a move-before-shuffle search sequence",
                )?;
                if conditional_shuffle_starts(cursor) {
                    LibrarySearchState::ConditionalShuffle
                } else {
                    cursor.consume_phrase(&[",", "then"])?;
                    LibrarySearchState::PostMoveShuffleAction
                }
            }
            LibrarySearchState::ConditionalShuffle => {
                // `Search your library and/or graveyard ... . If you search
                // your library this way, shuffle.` (CR 701.23a, 701.20a): the
                // searcher chooses whether the library is among the searched
                // zones, and only a searched library is shuffled.
                cursor.enter("effect.library-search.conditional-shuffle");
                cursor.require(
                    memory
                        .origin
                        .as_ref()
                        .is_some_and(|origin| !origin.is_library_only()),
                    "a search of the library and another zone before `if you search your library this way`",
                )?;
                cursor.require(
                    matches!(
                        memory.origin.as_ref().map(|origin| &origin.holder),
                        Some(SearchedPlayer::Searcher)
                    ) && memory.player == Some(LocalPlayerReference::Controller),
                    "the searching controller's own zones before `if you search your library this way`",
                )?;
                cursor.consume_phrase(&[
                    ".", "if", "you", "search", "your", "library", "this", "way", ",", "shuffle",
                ])?;
                cursor.enter("effect.library-search.conditional-shuffle.terminator");
                cursor.require(
                    cursor.peek().is_none_or(|raw| raw == "."),
                    "the end of the conditional shuffle sentence",
                )?;
                memory.shuffle_order = Some(SearchShuffleOrder::IfLibrarySearched);
                LibrarySearchState::Complete
            }
            LibrarySearchState::PostMoveShuffleAction => {
                cursor.enter("effect.library-search.post-move-shuffle-action");
                cursor.consume("shuffle")?;
                LibrarySearchState::Complete
            }
            LibrarySearchState::Complete => {
                cursor.enter("effect.library-search.complete");
                let filter = cursor.require_value(memory.filter, "a resolved card filter")?;
                let cardinality = memory.cardinality.expect("recognized search cardinality");
                // Hidden-zone searches with a stated quality may fail to find.
                let finds_any_card = filter == ObjectPredicate::ObjectKind(ObjectKind::Card);
                let minimum = if finds_any_card {
                    cardinality.minimum
                } else {
                    0
                };
                let count = memory.count.map(|count| {
                    if finds_any_card {
                        count
                    } else {
                        super::TargetCount {
                            minimum: super::NumericExpression::from(0),
                            maximum: count.maximum,
                        }
                    }
                });
                return Ok(LibrarySearchEffectNode {
                    player: cursor.require_value(memory.player, "a resolved searching player")?,
                    origin: cursor.require_value(memory.origin, "the resolved searched zones")?,
                    filter,
                    slots: memory.slots,
                    minimum,
                    maximum: cardinality.maximum,
                    count,
                    destination: cursor
                        .require_value(memory.destination, "a resolved search destination")?,
                    reveal: memory.reveal,
                    shuffle_order: cursor
                        .require_value(memory.shuffle_order, "a resolved search shuffle order")?,
                    binding: CollectionBinding(0),
                });
            }
        };
    }
}

/// One more separately determined card phrase of a search (`and a Plains
/// card`, `, a Swamp card`, `, and a Mountain card`, `and/or a card named
/// NAME`). Transactional: a following action (`, put it ...`, `and put
/// ...`) leaves the cursor unchanged.
fn recognize_additional_search_slot(cursor: &mut OracleTokenCursor<'_>) -> Option<ObjectPredicate> {
    let mut trial = cursor.clone();
    if trial.peek() == Some(",") {
        trial.consume(",").ok()?;
    }
    if let Some(connector) = trial
        .peek()
        .filter(|raw| raw.eq_ignore_ascii_case("and") || raw.eq_ignore_ascii_case("and/or"))
        .map(str::to_string)
    {
        trial.consume(&connector).ok()?;
    }
    let article = trial
        .peek()
        .filter(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))?
        .to_string();
    if trial.position() == cursor.position() {
        // Two phrases need a list separator between them.
        return None;
    }
    trial.consume(&article).ok()?;
    let slot = recognize_search_card_alternatives(&mut trial, false).ok()?;
    *cursor = trial;
    Some(slot)
}

/// `. If you search your library this way, shuffle` at the cursor: the
/// conditional shuffle sentence of a search through several zones. Pure
/// lookahead; the sentence's own period stays with the parent.
fn conditional_shuffle_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    [
        ".", "if", "you", "search", "your", "library", "this", "way", ",", "shuffle",
    ]
    .iter()
    .enumerate()
    .all(|(offset, expected)| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    })
}

fn library_player_possessive(player: LocalPlayerReference) -> &'static str {
    match player {
        LocalPlayerReference::Controller => "your",
        LocalPlayerReference::ControllerOf(_)
        | LocalPlayerReference::ControllerOfQualified(_)
        | LocalPlayerReference::Bound(_) => "their",
    }
}

fn recognize_search_result_reference(
    cursor: &mut OracleTokenCursor<'_>,
    plural: bool,
) -> MachineResult<()> {
    if plural {
        return if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("them"))
        {
            cursor.consume("them")
        } else if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("those"))
        {
            cursor.consume_phrase(&["those", "cards"])
        } else {
            cursor.consume_phrase(&["the", "cards"])
        };
    }
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("it"))
    {
        cursor.consume("it")
    } else if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
    {
        cursor.consume_phrase(&["that", "card"])
    } else {
        cursor.consume_phrase(&["the", "card"])
    }
}

#[cfg(test)]
#[path = "library_search_tests.rs"]
mod tests;
