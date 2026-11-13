use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CollectionBinding, LibrarySearchEffectNode, LocalPlayerReference, ObjectKind, ObjectPredicate,
    SearchDestination, SearchShuffleOrder, TargetBinding, recognize_player_reference,
    recognize_search_card_selector,
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
                    if controller_owned { "your" } else { "their" },
                    "library",
                    "for",
                ])?;
                LibrarySearchState::FilterArticle
            }
            LibrarySearchState::FilterArticle => {
                cursor.enter("effect.library-search.filter-article");
                match cursor.peek() {
                    Some(article)
                        if article.eq_ignore_ascii_case("a")
                            || article.eq_ignore_ascii_case("an") =>
                    {
                        let article = article.to_string();
                        cursor.consume(&article)?;
                    }
                    _ => return Err(cursor.fail("`a` or `an` before a card selector")),
                }
                LibrarySearchState::Filter
            }
            LibrarySearchState::Filter => {
                cursor.enter("effect.library-search.filter");
                memory.filter = Some(recognize_search_card_selector(cursor, false)?);
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
                        cursor.consume("reveal")?;
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
                recognize_search_result_reference(cursor)?;
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
                cursor.consume("put")?;
                LibrarySearchState::ResultReference
            }
            LibrarySearchState::ResultReference => {
                cursor.enter("effect.library-search.result-reference");
                recognize_search_result_reference(cursor)?;
                LibrarySearchState::Destination
            }
            LibrarySearchState::Destination => {
                cursor.enter("effect.library-search.destination");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("into"))
                {
                    cursor.consume_phrase(&["into", "your", "hand"])?;
                    memory.destination = Some(SearchDestination::Hand);
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
                        cursor.consume_phrase(&["of", "your", "library"])?;
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
                let minimum = u32::from(filter == ObjectPredicate::ObjectKind(ObjectKind::Card));
                return Ok(LibrarySearchEffectNode {
                    player: cursor.require_value(memory.player, "a resolved searching player")?,
                    filter,
                    minimum,
                    maximum: 1,
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

fn recognize_search_result_reference(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
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
