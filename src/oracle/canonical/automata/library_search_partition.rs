use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CollectionBinding, CollectionDestination, LibrarySearchCollectionNode,
    LibrarySearchPartitionSequenceNode, LocalPlayerReference, LocalZone, NumericAtom,
    RevealCollectionNode, SearchCollectionPartitionNode, ShuffleCollectionZoneNode,
    recognize_numeric_atom, recognize_search_card_selector,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchCollectionState {
    Action,
    Player,
    Zone,
    Relation,
    Cardinality,
    Maximum,
    Filter,
}

#[derive(Default)]
struct SearchCollectionMemory {
    player: Option<LocalPlayerReference>,
    maximum: Option<u32>,
}

fn recognize_search_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LibrarySearchCollectionNode> {
    let mut state = SearchCollectionState::Action;
    let mut memory = SearchCollectionMemory::default();
    loop {
        state = match state {
            SearchCollectionState::Action => {
                cursor.enter("effect.library-search-collection.action");
                cursor.consume("search")?;
                SearchCollectionState::Player
            }
            SearchCollectionState::Player => {
                cursor.enter("effect.library-search-collection.player");
                cursor.consume("your")?;
                memory.player = Some(LocalPlayerReference::Controller);
                SearchCollectionState::Zone
            }
            SearchCollectionState::Zone => {
                cursor.enter("effect.library-search-collection.zone");
                cursor.consume("library")?;
                SearchCollectionState::Relation
            }
            SearchCollectionState::Relation => {
                cursor.enter("effect.library-search-collection.relation");
                cursor.consume("for")?;
                SearchCollectionState::Cardinality
            }
            SearchCollectionState::Cardinality => {
                cursor.enter("effect.library-search-collection.cardinality");
                cursor.consume_phrase(&["up", "to"])?;
                SearchCollectionState::Maximum
            }
            SearchCollectionState::Maximum => {
                cursor.enter("effect.library-search-collection.maximum");
                let NumericAtom::Integer(maximum) = recognize_numeric_atom(cursor)? else {
                    return Err(cursor.fail("a fixed positive maximum card count"));
                };
                cursor.require(maximum > 0, "a fixed positive maximum card count")?;
                memory.maximum = Some(u32::try_from(maximum).map_err(|_| {
                    cursor.fail("a maximum card count representable by the grammar")
                })?);
                SearchCollectionState::Filter
            }
            SearchCollectionState::Filter => {
                cursor.enter("effect.library-search-collection.filter");
                let maximum = cursor.require_value(memory.maximum, "a populated maximum")?;
                let filter = recognize_search_card_selector(cursor, maximum != 1)?;
                cursor.enter("effect.library-search-collection.separator");
                cursor.consume(",")?;
                return Ok(LibrarySearchCollectionNode {
                    player: cursor.require_value(memory.player, "the searching player")?,
                    zone: LocalZone::Library,
                    minimum: 0,
                    maximum,
                    filter,
                    binding: CollectionBinding(0),
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RevealCollectionState {
    Action,
    Reference,
    Separator,
    Complete,
}

fn recognize_reveal_collection(
    cursor: &mut OracleTokenCursor<'_>,
    binding: CollectionBinding,
) -> MachineResult<RevealCollectionNode> {
    let mut state = RevealCollectionState::Action;
    loop {
        state = match state {
            RevealCollectionState::Action => {
                cursor.enter("effect.reveal-collection.action");
                crate::oracle::canonical::automata::consume_lemma(
                    cursor,
                    crate::oracle::canonical::automata::VerbLemma::Reveal,
                    crate::oracle::canonical::automata::VerbAgreement::Base,
                )?;
                RevealCollectionState::Reference
            }
            RevealCollectionState::Reference => {
                cursor.enter("effect.reveal-collection.reference");
                if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("them"))
                {
                    cursor.consume("them")?;
                } else {
                    cursor.consume_phrase(&["those", "cards"])?;
                }
                RevealCollectionState::Separator
            }
            RevealCollectionState::Separator => {
                cursor.enter("effect.reveal-collection.separator");
                cursor.consume(",")?;
                RevealCollectionState::Complete
            }
            RevealCollectionState::Complete => {
                cursor.enter("effect.reveal-collection.complete");
                return Ok(RevealCollectionNode { binding });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PartitionCollectionState {
    Action,
    SelectedCount,
    SelectedDestination,
    SelectedModifier,
    Connector,
    RemainderReference,
    RemainderRelation,
    RemainderDestination,
    Complete,
}

fn recognize_collection_partition(
    cursor: &mut OracleTokenCursor<'_>,
    binding: CollectionBinding,
    collection_maximum: u32,
) -> MachineResult<SearchCollectionPartitionNode> {
    let mut state = PartitionCollectionState::Action;
    let mut selected_count = None;
    let mut selected_tapped = false;
    loop {
        state = match state {
            PartitionCollectionState::Action => {
                cursor.enter("effect.collection-partition.action");
                crate::oracle::canonical::automata::consume_lemma(
                    cursor,
                    crate::oracle::canonical::automata::VerbLemma::Put,
                    crate::oracle::canonical::automata::VerbAgreement::Base,
                )?;
                PartitionCollectionState::SelectedCount
            }
            PartitionCollectionState::SelectedCount => {
                cursor.enter("effect.collection-partition.selected-count");
                let NumericAtom::Integer(count) = recognize_numeric_atom(cursor)? else {
                    return Err(cursor.fail("a fixed selected-card count"));
                };
                cursor.require(count > 0, "a positive selected-card count")?;
                selected_count = Some(u32::try_from(count).map_err(|_| {
                    cursor.fail("a selected-card count representable by the grammar")
                })?);
                PartitionCollectionState::SelectedDestination
            }
            PartitionCollectionState::SelectedDestination => {
                cursor.enter("effect.collection-partition.selected-destination");
                cursor.consume_phrase(&["onto", "the", "battlefield"])?;
                PartitionCollectionState::SelectedModifier
            }
            PartitionCollectionState::SelectedModifier => {
                cursor.enter("effect.collection-partition.selected-modifier");
                if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("tapped"))
                {
                    cursor.consume("tapped")?;
                    selected_tapped = true;
                }
                PartitionCollectionState::Connector
            }
            PartitionCollectionState::Connector => {
                cursor.enter("effect.collection-partition.connector");
                cursor.consume("and")?;
                PartitionCollectionState::RemainderReference
            }
            PartitionCollectionState::RemainderReference => {
                cursor.enter("effect.collection-partition.remainder-reference");
                cursor.require(
                    collection_maximum == 2,
                    "a two-card collection for the singular `the other` remainder",
                )?;
                cursor.consume_phrase(&["the", "other"])?;
                PartitionCollectionState::RemainderRelation
            }
            PartitionCollectionState::RemainderRelation => {
                cursor.enter("effect.collection-partition.remainder-relation");
                cursor.consume("into")?;
                PartitionCollectionState::RemainderDestination
            }
            PartitionCollectionState::RemainderDestination => {
                cursor.enter("effect.collection-partition.remainder-destination");
                cursor.consume_phrase(&["your", "hand"])?;
                PartitionCollectionState::Complete
            }
            PartitionCollectionState::Complete => {
                cursor.enter("effect.collection-partition.complete");
                return Ok(SearchCollectionPartitionNode {
                    binding,
                    selected_count: cursor
                        .require_value(selected_count, "a populated selected-card count")?,
                    selected_destination: CollectionDestination::Battlefield {
                        tapped: selected_tapped,
                    },
                    remainder_destination: CollectionDestination::Hand,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchPartitionSequenceState {
    Search,
    Reveal,
    Partition,
    ShuffleConnector,
    Shuffle,
    Terminator,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_library_search_partition_sequence(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LibrarySearchPartitionSequenceNode> {
    let mut state = SearchPartitionSequenceState::Search;
    let mut search = None;
    let mut reveal = None;
    let mut partition = None;
    loop {
        state = match state {
            SearchPartitionSequenceState::Search => {
                cursor.enter("sequence.library-search-partition.search");
                search = Some(recognize_search_collection(cursor)?);
                SearchPartitionSequenceState::Reveal
            }
            SearchPartitionSequenceState::Reveal => {
                cursor.enter("sequence.library-search-partition.reveal");
                let binding = search
                    .as_ref()
                    .map(|node: &LibrarySearchCollectionNode| node.binding)
                    .ok_or_else(|| cursor.fail("a bound searched collection"))?;
                reveal = Some(recognize_reveal_collection(cursor, binding)?);
                SearchPartitionSequenceState::Partition
            }
            SearchPartitionSequenceState::Partition => {
                cursor.enter("sequence.library-search-partition.partition");
                let searched = search
                    .as_ref()
                    .ok_or_else(|| cursor.fail("a completed library search"))?;
                partition = Some(recognize_collection_partition(
                    cursor,
                    searched.binding,
                    searched.maximum,
                )?);
                SearchPartitionSequenceState::ShuffleConnector
            }
            SearchPartitionSequenceState::ShuffleConnector => {
                cursor.enter("sequence.library-search-partition.shuffle-connector");
                cursor.consume_phrase(&[",", "then"])?;
                SearchPartitionSequenceState::Shuffle
            }
            SearchPartitionSequenceState::Shuffle => {
                cursor.enter("sequence.library-search-partition.shuffle");
                cursor.consume("shuffle")?;
                SearchPartitionSequenceState::Terminator
            }
            SearchPartitionSequenceState::Terminator => {
                cursor.enter("sequence.library-search-partition.terminator");
                cursor.consume(".")?;
                SearchPartitionSequenceState::Complete
            }
            SearchPartitionSequenceState::Complete => {
                cursor.enter("sequence.library-search-partition.complete");
                cursor.finish()?;
                let search = cursor.require_value(search, "a completed library search")?;
                return Ok(LibrarySearchPartitionSequenceNode {
                    shuffle: ShuffleCollectionZoneNode {
                        player: search.player.clone(),
                        zone: search.zone,
                    },
                    search,
                    reveal: cursor.require_value(reveal, "a bound reveal effect")?,
                    partition: cursor
                        .require_value(partition, "a completed collection partition")?,
                });
            }
        };
    }
}
