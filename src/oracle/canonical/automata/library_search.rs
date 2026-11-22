use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CardZoneName, CollectionBinding, LibrarySearchEffectNode, LocalPlayerReference, ObjectKind,
    ObjectPredicate, SearchDestination, SearchShuffleOrder, TargetBinding, TargetCardinality,
    recognize_card_zone_atom, recognize_player_reference, recognize_search_card_selector,
    recognize_target_cardinality,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LibrarySearchState {
    Player,
    Permission,
    SearchAction,
    PossessiveLibrary,
    FilterArticle,
    Filter,
    EffectSeparator,
    PostFilterAction,
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
    Complete,
}

#[derive(Default)]
struct LibrarySearchMemory {
    player: Option<LocalPlayerReference>,
    filter: Option<ObjectPredicate>,
    cardinality: Option<TargetCardinality>,
    destination: Option<SearchDestination>,
    reveal: bool,
    shuffle_order: Option<SearchShuffleOrder>,
}

/// Recognizes an optional search of the referenced player's own library,
/// followed by moving the selected result and shuffling that same library.
/// The result pronoun is resolved locally; lowering assigns the decision id.
pub(in crate::oracle::canonical) fn recognize_optional_library_search(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<LibrarySearchEffectNode> {
    recognize_library_search(cursor, antecedent, false)
}

/// Recognizes a mandatory search of the controller's library. The search
/// body is shared with optional target-controller searches so filters,
/// anaphora, destinations, and shuffle sequencing evolve only once.
pub(in crate::oracle::canonical) fn recognize_controller_library_search(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LibrarySearchEffectNode> {
    recognize_library_search(cursor, None, true)
}

fn recognize_library_search(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
    controller_owned: bool,
) -> MachineResult<LibrarySearchEffectNode> {
    let mut state = LibrarySearchState::Player;
    let mut memory = LibrarySearchMemory::default();
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
                cursor.consume_phrase(&[
                    library_player_possessive(memory.player.expect("recognized searching player")),
                    "library",
                    "for",
                ])?;
                LibrarySearchState::FilterArticle
            }
            LibrarySearchState::FilterArticle => {
                cursor.enter("effect.library-search.filter-article");
                let cardinality = recognize_target_cardinality(cursor)?;
                cursor.enter("effect.library-search.cardinality-bounds");
                cursor.require(
                    cardinality.maximum <= i32::MAX as u32,
                    "a search cardinality representable by the runtime count domain",
                )?;
                memory.cardinality = Some(cardinality);
                LibrarySearchState::Filter
            }
            LibrarySearchState::Filter => {
                cursor.enter("effect.library-search.filter");
                let cardinality = memory.cardinality.expect("recognized search cardinality");
                memory.filter = Some(recognize_search_card_selector(
                    cursor,
                    cardinality.maximum != 1,
                )?);
                LibrarySearchState::EffectSeparator
            }
            LibrarySearchState::EffectSeparator => {
                cursor.enter("effect.library-search.effect-separator");
                cursor.consume(",")?;
                LibrarySearchState::PostFilterAction
            }
            LibrarySearchState::PostFilterAction => {
                cursor.enter("effect.library-search.post-filter-action");
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
                    Some("then") => {
                        cursor.consume("then")?;
                        memory.shuffle_order = Some(SearchShuffleOrder::ShuffleThenMove);
                        LibrarySearchState::ShuffleAction
                    }
                    _ => Err(cursor.fail("`reveal`, `put`, or `then shuffle`"))?,
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
                cursor.consume_phrase(&[",", "then"])?;
                LibrarySearchState::PostMoveShuffleAction
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
                let minimum = if filter == ObjectPredicate::ObjectKind(ObjectKind::Card) {
                    cardinality.minimum
                } else {
                    0
                };
                return Ok(LibrarySearchEffectNode {
                    player: cursor.require_value(memory.player, "a resolved searching player")?,
                    filter,
                    minimum,
                    maximum: cardinality.maximum,
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
