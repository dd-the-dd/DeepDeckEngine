use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CollectionBinding, CollectionOrder, LocalConditionNode, LocalZone, NumericAtom,
    PartitionCollectionNode, TargetListNode, TopCardsEffectNode, TopCardsPartitionSequenceNode,
    recognize_cards_exist_in_controller_graveyard_condition, recognize_numeric_atom,
    recognize_possessive_target_player,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TopCardsLibraryOwnerNode {
    Controller,
    TargetPlayer(TargetListNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ConditionalTopCardsPartitionSequenceNode {
    pub top: TopCardsEffectNode,
    pub default_partition: PartitionCollectionNode,
    pub replacement_condition: LocalConditionNode,
    pub replacement_partition: PartitionCollectionNode,
}

fn recognize_positive_count(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<i64> {
    let NumericAtom::Integer(value) = recognize_numeric_atom(cursor)? else {
        return Err(cursor.fail("a fixed positive card count"));
    };
    cursor.require(value > 0, "a fixed positive card count")?;
    Ok(value)
}

fn recognize_possessive_zone(
    cursor: &mut OracleTokenCursor<'_>,
    expected: LocalZone,
) -> MachineResult<LocalZone> {
    cursor.enter("zone.possessive.controller");
    cursor.consume("your")?;
    cursor.enter("zone.possessive.kind");
    match expected {
        LocalZone::Library => cursor.consume("library")?,
        LocalZone::Hand => cursor.consume("hand")?,
    }
    Ok(expected)
}

pub(in crate::oracle::canonical) fn recognize_top_cards_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TopCardsEffectNode> {
    cursor.enter("effect.top-cards.action");
    cursor.consume_phrase(&["look", "at", "the", "top"])?;
    cursor.enter("effect.top-cards.count");
    let count = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("card"))
    {
        1
    } else {
        recognize_positive_count(cursor)?
    };
    cursor.enter("effect.top-cards.cards");
    if count == 1 {
        cursor.consume("card")?;
    } else {
        cursor.consume("cards")?;
    }
    cursor.enter("effect.top-cards.source");
    cursor.consume("of")?;
    let owner = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
    {
        recognize_possessive_zone(cursor, LocalZone::Library)?;
        TopCardsLibraryOwnerNode::Controller
    } else {
        let target = recognize_possessive_target_player(cursor)?;
        cursor.enter("effect.top-cards.target-library");
        cursor.consume("library")?;
        TopCardsLibraryOwnerNode::TargetPlayer(target)
    };
    Ok(TopCardsEffectNode {
        count,
        zone: LocalZone::Library,
        owner,
        binding: CollectionBinding(0),
    })
}

pub(in crate::oracle::canonical) fn recognize_partition(
    cursor: &mut OracleTokenCursor<'_>,
    binding: CollectionBinding,
) -> MachineResult<PartitionCollectionNode> {
    cursor.enter("effect.partition.action");
    crate::oracle::canonical::automata::consume_lemma(
        cursor,
        crate::oracle::canonical::automata::VerbLemma::Put,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    )?;
    cursor.enter("effect.partition.count");
    let chosen_count = recognize_positive_count(cursor)?;
    cursor.enter("reference.collection.selected-from-binding");
    cursor.consume_phrase(&["of", "them"])?;
    cursor.enter("effect.partition.selected-destination");
    cursor.consume("into")?;
    let chosen_destination = recognize_possessive_zone(cursor, LocalZone::Hand)?;
    cursor.enter("effect.partition.remainder-connector");
    cursor.consume("and")?;
    cursor.enter("reference.collection.remainder");
    cursor.consume_phrase(&["the", "rest"])?;
    cursor.enter("effect.partition.remainder-destination");
    cursor.consume_phrase(&["on", "the", "bottom", "of"])?;
    let remainder_destination = recognize_possessive_zone(cursor, LocalZone::Library)?;
    cursor.enter("effect.partition.order");
    cursor.consume("in")?;
    let order = if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("any"))
    {
        cursor.consume_phrase(&["any", "order"])?;
        CollectionOrder::Chosen
    } else {
        cursor.consume_phrase(&["a", "random", "order"])?;
        CollectionOrder::Random
    };
    Ok(PartitionCollectionNode {
        binding,
        chosen_count,
        chosen_destination,
        remainder_destination,
        order,
    })
}

pub(in crate::oracle::canonical) fn recognize_conditional_top_cards_partition_sequence(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionalTopCardsPartitionSequenceNode> {
    let top = recognize_top_cards_effect(cursor)?;
    cursor.enter("sequence.conditional-top-cards.look-boundary");
    cursor.consume(".")?;
    let default_partition = recognize_partition(cursor, top.binding)?;
    cursor.enter("sequence.conditional-top-cards.default-boundary");
    cursor.consume(".")?;
    let replacement_condition = recognize_cards_exist_in_controller_graveyard_condition(cursor)?;
    cursor.enter("sequence.conditional-top-cards.instead");
    cursor.consume("instead")?;
    let replacement_partition = recognize_partition(cursor, top.binding)?;
    cursor.enter("sequence.conditional-top-cards.complete");
    cursor.consume(".")?;
    cursor.finish()?;
    Ok(ConditionalTopCardsPartitionSequenceNode {
        top,
        default_partition,
        replacement_condition,
        replacement_partition,
    })
}

pub(in crate::oracle::canonical) fn recognize_top_cards_partition_sequence(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TopCardsPartitionSequenceNode> {
    let top = recognize_top_cards_effect(cursor)?;
    cursor.enter("sequence.top-cards.sentence-boundary");
    cursor.consume(".")?;
    let partition = recognize_partition(cursor, top.binding)?;
    cursor.enter("sequence.top-cards.complete");
    cursor.consume(".")?;
    cursor.finish()?;
    Ok(TopCardsPartitionSequenceNode { top, partition })
}
