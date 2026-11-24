//! The shared verb operand: one reader, typed alternatives, transactional
//! failures. Each case is a distinct contract of the reader, not a card.
use super::*;
use crate::oracle::syntax::recognize_entities;

fn operand(text: &str) -> MachineResult<VerbObjectOperandNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("verb-operand-test", &tokens);
    let node =
        recognize_verb_object_operand(&mut cursor, VerbObjectOperandExpectation::OBJECT_TO_ZONE)?;
    cursor.finish()?;
    Ok(node)
}

#[test]
fn a_universal_query_with_a_controller_population_and_variable_bound_is_a_collection() {
    let Ok(VerbObjectOperandNode::Collection {
        objects,
        exclude_source,
    }) = operand("each creature your opponents control with toughness X or less")
    else {
        panic!("a universal battlefield collection");
    };
    assert!(!exclude_source);
    assert_eq!(
        objects.controller,
        Some(ObjectControllerScope::OpponentOfAbilityController)
    );
    assert!(object_predicate_uses_x(&objects.predicate));
}

#[test]
fn another_without_target_is_a_resolution_choice_excluding_the_source() {
    let Ok(VerbObjectOperandNode::Selection(selection)) = operand("another creature you control")
    else {
        panic!("a resolution choice, not a declaration");
    };
    assert_eq!(
        selection.bounds,
        Some(ExpressedTargetCardinality::EXACTLY_ONE)
    );
    assert!(selection.query.exclude_source);
    assert_eq!(
        selection.query.controller,
        Some(ObjectControllerScope::AbilityController)
    );
}

#[test]
fn variable_and_unbounded_target_counts_are_one_typed_count() {
    let Ok(VerbObjectOperandNode::Targets(targets)) = operand("X target Mountains") else {
        panic!("X declared targets");
    };
    let slot = &targets.slots[0];
    assert_eq!(slot.cardinality, TargetCardinality::EXPRESSED);
    assert_eq!(
        slot.count,
        Some(TargetCount {
            minimum: NumericExpression::Atom(NumericAtom::VariableX),
            maximum: TargetCountBound::Amount(NumericExpression::Atom(NumericAtom::VariableX)),
        })
    );
    let Ok(VerbObjectOperandNode::Targets(targets)) =
        operand("any number of target nonland permanents you control")
    else {
        panic!("unbounded declared targets");
    };
    assert!(
        targets.slots[0]
            .count
            .as_ref()
            .is_some_and(TargetCount::is_unbounded)
    );
    // A literal count keeps the historical literal slot exactly.
    let Ok(VerbObjectOperandNode::Targets(targets)) = operand("up to two target creatures") else {
        panic!("literal declared targets");
    };
    assert_eq!(targets.slots[0].count, None);
    assert_eq!(
        targets.slots[0].cardinality,
        TargetCardinality {
            minimum: 0,
            maximum: 2
        }
    );
}

#[test]
fn a_literal_only_target_list_still_refuses_a_variable_count() {
    let tokens = recognize_entities("X target creatures");
    let mut cursor = OracleTokenCursor::new("verb-operand-literal", &tokens);
    assert!(recognize_target_list(&mut cursor).is_err());
}

#[test]
fn a_graveyard_origin_routes_qualified_card_targets_to_the_card_operand() {
    let Ok(VerbObjectOperandNode::CardTargets(target)) =
        operand("another target Elf card from your graveyard")
    else {
        panic!("a declared card target in the named zone");
    };
    assert!(target.exclude_source);
    assert_eq!(target.zone, CardZoneName::Graveyard);
    let Ok(VerbObjectOperandNode::CardTargets(target)) =
        operand("any number of target creature cards with mana value 2 or less from graveyards")
    else {
        panic!("unbounded card targets across graveyards");
    };
    assert!(target.unbounded);
    assert_eq!(target.owner, CardTargetZoneOwner::AnyPlayer);
}

#[test]
fn a_qualified_card_target_with_unsupported_trailing_text_fails_committed() {
    // Near miss: the origin commits the card reading; trailing words are not
    // reinterpreted as a battlefield target.
    let tokens = recognize_entities("target creature card with flying from a graveyard quickly");
    let mut cursor = OracleTokenCursor::new("verb-operand-near-miss", &tokens);
    let node =
        recognize_verb_object_operand(&mut cursor, VerbObjectOperandExpectation::OBJECT_TO_ZONE)
            .expect("the operand itself is complete");
    assert!(matches!(node, VerbObjectOperandNode::CardTargets(_)));
    assert!(cursor.finish().is_err());
}

#[test]
fn a_library_position_reads_a_declared_owner_through_the_possessive_target() {
    let Ok(VerbObjectOperandNode::LibraryTop(top)) =
        operand("the top three cards of target opponent's library")
    else {
        panic!("a library position");
    };
    assert_eq!(top.count, NumericExpression::from(3));
    let CardLibraryOwnerNode::TargetPlayer(owner) = top.owner else {
        panic!("a declared library owner");
    };
    assert_eq!(
        owner.slots[0].selector,
        TargetSelector::Player(PlayerPredicate::Opponent)
    );
    // The controller-only contract of manifest and look permissions refuses
    // a declared owner instead of reading the controller's library.
    let tokens = recognize_entities("the top card of target player's library");
    let mut cursor = OracleTokenCursor::new("library-top-controller-only", &tokens);
    assert!(matches!(
        try_recognize_card_library_top(&mut cursor),
        CollectionOperandAttempt::FailedCommitted(_)
    ));
}
