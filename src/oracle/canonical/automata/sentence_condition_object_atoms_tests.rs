//! Reader-level contract of the object-state, player-state and turn atoms:
//! a referenced object hands its verb phrase to the shared object-query
//! grammar (relations, history, zones), a player copula reads typed player
//! predicates, a coordinated conjunct restores an elided `you control`, and
//! a missing antecedent or binding stays an explicit failure.
use crate::oracle::syntax::recognize_entities;

use super::super::{
    BooleanConditionNode, ConditionPlayerSubject, ConditionZone, EffectRecognitionContext,
    ObjectPredicate, ObjectStatus, PermanentEffectReference, PermanentSubjectScope,
    PlayerDesignation, PlayerPopulation, PlayerQuantifier, TargetBinding,
};
use super::{
    ConditionAtom, ConditionObjectReference, OracleTokenCursor, SentenceConditionContext,
    recognize_bound_pronoun_predicate, recognize_sentence_condition,
};

const TARGET: TargetBinding = TargetBinding {
    effect_index: 0,
    slot_index: 0,
};

fn read_with(
    text: &str,
    effects: &EffectRecognitionContext,
) -> Result<BooleanConditionNode<ConditionAtom>, (String, String)> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("object-condition-atom-test", &tokens);
    let context = SentenceConditionContext {
        name_links: &[],
        name_failure: None,
        effects,
        card_target: None,
        card_focus: false,
        departed_targets: &[],
    };
    recognize_sentence_condition(&mut cursor, &context)
        .map_err(|failure| (failure.state.to_string(), failure.expected))
}

fn read(text: &str) -> Result<BooleanConditionNode<ConditionAtom>, (String, String)> {
    read_with(text, &EffectRecognitionContext::default())
}

fn target_context() -> EffectRecognitionContext {
    EffectRecognitionContext {
        permanent_subject_scope: PermanentSubjectScope::Local(Some(
            PermanentEffectReference::Target(TARGET),
        )),
        ..EffectRecognitionContext::default()
    }
}

fn atom(node: BooleanConditionNode<ConditionAtom>) -> ConditionAtom {
    let BooleanConditionNode::Atom(atom) = node else {
        panic!("one atom, got {node:?}");
    };
    atom
}

#[test]
fn player_copulas_read_the_monarch_the_starting_player_and_poison() {
    let ConditionAtom::PlayerDesignation(monarch) =
        atom(read("If you're the monarch,").expect("monarch"))
    else {
        panic!("a designation atom");
    };
    assert_eq!(monarch.designation, PlayerDesignation::Monarch);
    assert!(monarch.held);
    assert_eq!(monarch.subject, ConditionPlayerSubject::Controller);

    // Sibling: a quantified holder of the same designation.
    let ConditionAtom::PlayerDesignation(opponent) =
        atom(read("If an opponent is the monarch,").expect("an opponent is the monarch"))
    else {
        panic!("a designation atom");
    };
    assert_eq!(
        opponent.subject,
        ConditionPlayerSubject::Quantified {
            quantifier: PlayerQuantifier::Some,
            population: PlayerPopulation::Opponents,
        }
    );

    // The starting player is a fact of the game in either tense.
    for text in [
        "If you weren't the starting player,",
        "If you aren't the starting player,",
    ] {
        let ConditionAtom::PlayerDesignation(node) = atom(read(text).expect(text)) else {
            panic!("a designation atom for {text}");
        };
        assert_eq!(node.designation, PlayerDesignation::StartingPlayer);
        assert!(!node.held, "{text}");
    }

    // Poisoned is the shared player-counter comparison, not a designation.
    assert!(matches!(
        atom(read("If an opponent is poisoned,").expect("poisoned")),
        ConditionAtom::Compare(_)
    ));
}

#[test]
fn a_past_designation_and_an_unbound_player_fail_explicitly() {
    // Near-miss: a past monarch is not recorded.
    let (state, _) = read("If you were the monarch,").expect_err("a past monarch");
    assert_eq!(state, "condition.player-copula.tense");
    // Near-miss: `that player` without a triggering event's player.
    assert!(read("If that player is the monarch,").is_err());
}

#[test]
fn another_players_turn_needs_the_event_player_it_names() {
    // Near-miss: no event records a player for `their`.
    let (state, _) = read("If it's not their turn,").expect_err("no event player");
    assert_eq!(state, "condition.player-turn.possessor");
    let ConditionAtom::PlayerTurn(node) =
        atom(read("If it's an opponent's turn,").expect("an opponent's turn"))
    else {
        panic!("a player-turn atom");
    };
    assert!(node.is_turn);
}

#[test]
fn an_elided_control_conjunct_is_its_own_presence_comparison() {
    let node = read("If you control a blue permanent and a black permanent,")
        .expect("coordinated presence");
    let BooleanConditionNode::And(operands) = node else {
        panic!("a conjunction, got {node:?}");
    };
    assert_eq!(operands.len(), 2);
    assert!(operands.iter().all(|operand| matches!(
        operand,
        BooleanConditionNode::Atom(ConditionAtom::Compare(_))
    )));
    // The lookbehind finds the stated `you control` of the same clause; a
    // conjunct after another subject's verb is not elided (near-miss).
    let elided_at = |text: &str, position: usize| {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("elision-test", &tokens);
        cursor.advance_to(position).expect("the second conjunct");
        super::super::elided_controlled_presence_starts(&cursor)
    };
    assert!(elided_at("you control a Forest and a Plains", 5));
    assert!(!elided_at("an opponent controls a Forest and a Plains", 6));
}

#[test]
fn a_referenced_object_reads_zones_relations_and_history() {
    let ConditionAtom::ObjectZone(zone) =
        atom(read_with("If it's still on the battlefield,", &target_context()).expect("zone"))
    else {
        panic!("a zone atom");
    };
    assert_eq!(
        zone.object,
        ConditionObjectReference::Permanent(PermanentEffectReference::Target(TARGET))
    );
    assert_eq!(zone.zones, vec![ConditionZone::Battlefield]);
    assert!(!zone.negated);

    let ConditionAtom::ObjectSatisfies(history) =
        atom(read_with("If it was dealt damage this turn,", &target_context()).expect("history"))
    else {
        panic!("an object atom");
    };
    assert_eq!(
        history.predicate,
        ObjectPredicate::Status(ObjectStatus::WasDealtDamageThisTurn)
    );

    let ConditionAtom::ObjectSatisfies(entered) =
        atom(read_with("If it entered this turn,", &target_context()).expect("entered"))
    else {
        panic!("an object atom");
    };
    assert_eq!(
        entered.predicate,
        ObjectPredicate::Status(ObjectStatus::EnteredThisTurn)
    );

    // Near-miss: a pronoun without an antecedent stays a binding failure.
    let (state, _) = read("If it's a land,").expect_err("no antecedent");
    assert_eq!(state, "condition.sentence.object.antecedent");
}

#[test]
fn the_cast_record_is_read_for_the_source_only() {
    // Near-miss: a declared target's cast is not recorded.
    let (state, _) =
        read_with("If it was cast,", &target_context()).expect_err("a target's cast record");
    assert_eq!(state, "condition.sentence.object.cast-record");
}

#[test]
fn a_bound_target_reads_an_expression_comparison_and_rejects_its_zone() {
    let tokens = recognize_entities(
        "its power is less than or equal to the number of cards in your graveyard",
    );
    let mut cursor = OracleTokenCursor::new("bound-pronoun-test", &tokens);
    let predicate = recognize_bound_pronoun_predicate(&mut cursor).expect("expression comparison");
    assert!(matches!(
        predicate,
        ObjectPredicate::NumericComparison { .. }
    ));
    cursor
        .finish()
        .expect("the comparison consumes its operand");

    let tokens = recognize_entities("it's on the battlefield");
    let mut cursor = OracleTokenCursor::new("bound-pronoun-test", &tokens);
    let failure =
        recognize_bound_pronoun_predicate(&mut cursor).expect_err("a zone is no characteristic");
    assert_eq!(failure.state, "condition.bound-object.characteristic");
}
