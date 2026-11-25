use super::super::super::OracleTokenCursor;
use super::super::{
    EffectConnector, EffectRecognitionContext, EffectSequenceParseAttempt, LocalEffectNode,
    PlayerBinding, PlayerEffectSubject, recognize_effect_sequence_with_context,
    try_recognize_effect_sequence_with_context,
};
use crate::oracle::syntax::recognize_entities;

#[test]
fn punctuation_coordinates_without_consuming_the_following_action() {
    for (text, expected) in [
        (", draw a card.", EffectConnector::And),
        (", and draw a card.", EffectConnector::And),
        (", then draw a card.", EffectConnector::Then),
        ("and draw a card.", EffectConnector::And),
        ("then draw a card.", EffectConnector::Then),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("comma-connector-unit", &tokens);
        assert_eq!(
            super::recognize_effect_connector(&mut cursor).unwrap(),
            expected
        );
        assert_eq!(cursor.peek(), Some("draw"));
    }
    // `or` is the alternative connector (CR 608.2d), never a conjunction.
    for text in [", or draw a card.", "or draw a card."] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("comma-alternative-unit", &tokens);
        assert_eq!(
            super::recognize_effect_connector(&mut cursor).unwrap(),
            EffectConnector::Or
        );
        assert_eq!(cursor.peek(), Some("draw"));
    }
}

#[test]
fn comma_lists_compose_repeated_actions_with_player_and_controller_scopes() {
    let tokens = recognize_entities(
        "Target player gains 3 life, draws a card, loses 2 life, draws two cards, and gains 1 life.",
    );
    let mut cursor = OracleTokenCursor::new("comma-player-sequence-unit", &tokens);
    let sequence = recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
    .unwrap();
    assert_eq!(sequence.effects.len(), 5);
    assert_eq!(sequence.connectors, [EffectConnector::And; 4]);
    let LocalEffectNode::TargetPlayerLife(first) = &sequence.effects[0] else {
        panic!("existing explicit life subject")
    };
    assert!(first.declares_target);
    assert_eq!(first.player.effect_index, 0);
    for effect in &sequence.effects[1..] {
        let LocalEffectNode::PlayerAction(action) = effect else {
            panic!("one independently bound player action")
        };
        assert_eq!(action.player, PlayerEffectSubject::Bound(PlayerBinding(0)));
    }
    let tokens = recognize_entities("Draw a card, gain 2 life, then draw two cards.");
    let mut cursor = OracleTokenCursor::new("comma-controller-sequence-unit", &tokens);
    let sequence = recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
    .unwrap();
    assert_eq!(sequence.effects.len(), 3);
    assert_eq!(
        sequence.connectors,
        [EffectConnector::And, EffectConnector::Then]
    );
}

#[test]
fn comma_never_hides_missing_effect_alternatives_or_an_unproved_subject() {
    for text in [
        "Target player gains 3 life,.",
        "Target player gains 3 life,, draws a card.",
        "Target player gains 3 life, teleport a card.",
        "Target player gains 3 life, or draws a card.",
        "Target player draws a card, discard a card.",
        "Target player gains 3 life. Draws a card.",
        "You may draw a card, gain 1 life.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("comma-scope-negative-unit", &tokens);
        assert!(
            recognize_effect_sequence_with_context(
                &mut cursor,
                &[],
                None,
                &EffectRecognitionContext::default()
            )
            .is_err(),
            "{text}"
        );
    }
    let tokens = recognize_entities("Draw a card, seek a creature card.");
    let mut cursor = OracleTokenCursor::new("comma-transactional-child-unit", &tokens);
    let result = try_recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    );
    assert!(matches!(
        result,
        EffectSequenceParseAttempt::RejectedCandidate(_)
    ));
    assert_eq!(cursor.position(), 0);
}

fn parse_sequence(
    text: &str,
) -> Result<super::super::EffectSequenceNode, super::super::super::MachineFailure> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("gapped-conjunct-unit", &tokens);
    recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
}

#[test]
fn a_gapped_conjunct_reads_the_previous_imperative_lemma() {
    // Stand Together: two counter frames of one `put`; the second slot is
    // `another target creature` from the shared target list.
    let sequence = parse_sequence(
        "Put two +1/+1 counters on target creature and two +1/+1 counters on another target creature.",
    )
    .unwrap();
    assert!(matches!(
        sequence.effects.as_slice(),
        [LocalEffectNode::PutCounters(_), LocalEffectNode::PutCounters(second)]
            if matches!(&second.permanent, super::super::PermanentOperandNode::Targets(targets)
                if targets.slots[0].exclude_source)
    ));
    assert_eq!(sequence.connectors, [EffectConnector::And]);
    // A comma list of three frames (Incremental Growth's shape).
    let sequence = parse_sequence(
        "Put a +1/+1 counter on target creature, two +1/+1 counters on another target creature, and three +1/+1 counters on another target creature.",
    )
    .unwrap();
    assert_eq!(sequence.effects.len(), 3);
    // In Garruk's Wake: two universal collections of one `destroy`.
    let sequence = parse_sequence(
        "Destroy all creatures you don't control and all planeswalkers you don't control.",
    )
    .unwrap();
    assert!(matches!(
        sequence.effects.as_slice(),
        [
            LocalEffectNode::DestroyCollection(_),
            LocalEffectNode::DestroyCollection(_)
        ]
    ));
}

#[test]
fn a_conjunct_with_its_own_verb_or_another_sentence_is_not_gapped() {
    // `and` joining two different lemmas stays an ordinary sequence.
    let sequence = parse_sequence("Destroy all artifacts and draw a card.").unwrap();
    assert!(matches!(
        sequence.effects.as_slice(),
        [
            LocalEffectNode::DestroyCollection(_),
            LocalEffectNode::DrawCards(_)
        ] | [
            LocalEffectNode::DestroyCollection(_),
            LocalEffectNode::PlayerAction(_)
        ]
    ));
    // A sentence boundary ends the ellipsis scope.
    assert!(parse_sequence("Destroy all artifacts. All enchantments.").is_err());
    // A conjunct that is not a complete frame of the lemma keeps failing.
    assert!(parse_sequence("Put a +1/+1 counter on target creature and two counters.").is_err());
}
