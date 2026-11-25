use super::*;
use crate::oracle::canonical::effects::lower_local_effect_sequence;
use crate::oracle::syntax::recognize_entities;

fn action(text: &str) -> CardActionNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("target-card-operand-test", &tokens);
    let CollectionOperandAttempt::Recognized(fragment) =
        try_recognize_target_card_action(&mut cursor)
    else {
        panic!("a complete typed target-card movement is required: {text}");
    };
    cursor.finish().unwrap();
    fragment.node
}

#[test]
fn target_query_keeps_zone_owner_separate_from_predicate_and_destination() {
    let node = action(
        "Put target artifact or enchantment card from your graveyard on top of your library",
    );
    let CardActionNode::Move {
        cards: CardOperandNode::Targets(target),
        destination,
    } = node
    else {
        panic!()
    };
    assert_eq!(
        target.owner,
        CardTargetZoneOwner::Player(LocalPlayerReference::Controller)
    );
    assert_eq!(target.zone, CardZoneName::Graveyard);
    assert_eq!(target.cardinality, ExpressedTargetCardinality::EXACTLY_ONE);
    assert!(target.predicate.has_domain(ObjectKind::Card));
    assert_eq!(
        destination.owner,
        CardMoveOwnerNode::Player(LocalPlayerReference::Controller)
    );
    assert_eq!(destination.zone, CardZoneName::Library);
    assert_eq!(destination.position, Some(LibraryPosition::Top));
    action(
        "Put target artifact card or an enchantment card from your graveyard on top of your library",
    );
    let tokens = recognize_entities(
        "Put two target artifact cards and an enchantment card from your graveyard on top of your library",
    );
    let mut cursor = OracleTokenCursor::new("target-card-quota", &tokens);
    assert!(!matches!(
        try_recognize_target_card_action(&mut cursor),
        CollectionOperandAttempt::Recognized(_)
    ));
    assert_eq!(
        cursor.position(),
        0,
        "separately quantified heads cannot collapse to one quota"
    );
}

#[test]
fn unsupported_owner_is_a_committed_error_without_cursor_commit() {
    let tokens =
        recognize_entities("Put target card from neighboring graveyards on top of your library");
    let mut cursor = OracleTokenCursor::new("target-card-operand-test", &tokens);
    let CollectionOperandAttempt::FailedCommitted(failure) =
        try_recognize_target_card_action(&mut cursor)
    else {
        panic!("typed card/from envelope owns its unsupported zone owner")
    };
    assert_eq!(failure.state, "card-target.source-owner");
    assert_eq!(failure.actual.as_deref(), Some("neighboring"));
    assert_eq!(cursor.position(), 0);
}

#[test]
fn two_target_operands_allocate_distinct_declarations_and_use_their_own_bindings() {
    let tokens = recognize_entities(
        "Put target enchantment card from your graveyard on top of your library. Put target artifact card from your graveyard on the bottom of your library.",
    );
    let mut cursor = OracleTokenCursor::new("target-card-operand-test", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None).unwrap();
    let lowered = lower_local_effect_sequence(&mut cursor, &sequence).unwrap();
    assert_eq!(lowered.decisions.len(), 2);
    assert_eq!(lowered.decisions[0]["id"], "targetCardOperand");
    assert_eq!(lowered.decisions[1]["id"], "targetCardOperand2");
    assert_eq!(
        lowered.effects[0]["cards"]["id"],
        lowered.decisions[0]["id"]
    );
    assert_eq!(
        lowered.effects[1]["cards"]["id"],
        lowered.decisions[1]["id"]
    );
    assert_eq!(lowered.effects[0]["to"]["position"], "top");
    assert_eq!(lowered.effects[1]["to"]["position"], "bottom");
}

#[test]
fn compatibility_projection_cannot_erase_an_explicit_source_zone_owner() {
    let owned =
        action("Put target artifact card from your graveyard on top of its owner's library");
    assert!(legacy_target_card_move(&owned).is_none());
    let CardActionNode::Move {
        cards: CardOperandNode::Targets(target),
        ..
    } = owned
    else {
        panic!()
    };
    assert_eq!(
        target.owner,
        CardTargetZoneOwner::Player(LocalPlayerReference::Controller)
    );
    let old = action("Put target artifact card from a graveyard on top of its owner's library");
    let compat = legacy_target_card_move(&old).unwrap();
    assert_eq!(compat.source_zone, TargetCardSourceZone::AnyGraveyard);
    assert_eq!(
        compat.action,
        CardZoneChangeAction::Move(CardZoneDestination::TargetOwnerLibraryTop)
    );
}

#[test]
fn printed_characteristics_of_a_card_in_a_graveyard_are_declarable() {
    // CR 400.7, 109.3: a card in a graveyard is read through its printed
    // colors and power; the source's power is bound into the filter when
    // the target is chosen (CR 601.2c) and rechecked (CR 608.2b).
    for text in [
        "Put target red enchantment card from your graveyard on top of your library",
        "Put target creature card with power 2 or less from your graveyard on top of your library",
        "Put target multicolored creature card with toughness 3 or less from your graveyard on top of your library",
    ] {
        let CardActionNode::Move {
            cards: CardOperandNode::Targets(target),
            ..
        } = action(text)
        else {
            panic!("a declared card target: {text}")
        };
        assert!(target.predicate.has_domain(ObjectKind::Card), "{text}");
    }
}

#[test]
fn a_resolution_choice_stays_undeclarable_on_a_card_target() {
    let tokens = recognize_entities(
        "Put target creature card of the chosen type from your graveyard on top of your library",
    );
    let mut cursor = OracleTokenCursor::new("target-card-operand-test", &tokens);
    // An explicit `from ZONE` commits the card-target reading, so the
    // unproved predicate is a committed, located refusal; the caller's
    // cursor is untouched either way.
    let CollectionOperandAttempt::FailedCommitted(failure) =
        try_recognize_target_card_action(&mut cursor)
    else {
        panic!("a choice made on resolution must not be admitted")
    };
    assert_eq!(failure.state, "card-target.query-capability");
    assert_eq!(cursor.position(), 0);
}

#[test]
fn the_zone_history_relative_qualifies_the_cards_of_the_named_zone() {
    let tokens = recognize_entities(
        "target creature card in your graveyard that was put there from the battlefield this turn",
    );
    let mut cursor = OracleTokenCursor::new("target-card-zone-history", &tokens);
    let CollectionOperandAttempt::Recognized(fragment) =
        try_recognize_card_target_operand(&mut cursor)
    else {
        panic!("the zone-history tail belongs to the card target")
    };
    cursor.finish().unwrap();
    let ObjectPredicate::Intersection(operands) = &fragment.node.predicate else {
        panic!(
            "the head and the history compose: {:?}",
            fragment.node.predicate
        )
    };
    assert!(operands.contains(&ObjectPredicate::Status(
        ObjectStatus::PutThereFromBattlefieldThisTurn
    )));
    // Near miss: another departure zone is not this atom and stays an
    // explicit failure of the operand, not the end of the operand.
    let tokens = recognize_entities(
        "target creature card in your graveyard that was put there from your library this turn",
    );
    let mut cursor = OracleTokenCursor::new("target-card-zone-history-near-miss", &tokens);
    let CollectionOperandAttempt::FailedCommitted(failure) =
        try_recognize_card_target_operand(&mut cursor)
    else {
        panic!("an unread relative after the zone is the operand's failure")
    };
    assert_eq!(failure.state, "selector.relative-clause.unsupported");
}
