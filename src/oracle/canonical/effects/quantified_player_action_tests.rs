use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn quantified_loss_does_not_capture_following_controller_gain() {
    let tokens = recognize_entities("Each opponent loses 2 life and you gain 2 life.");
    let mut cursor = OracleTokenCursor::new("quantified-lower-scope", &tokens);
    let sequence = recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
    .unwrap();
    let lowered = lower_local_effect_sequence(&mut cursor, &sequence).unwrap();
    assert!(lowered.decisions.is_empty());
    assert_eq!(lowered.effects.len(), 2);
    assert_eq!(
        lowered.effects[0],
        json!({"kind":"loseLife","player":{"kind":"opponentsOf","player":controller()},"amount":{"kind":"integer","value":2}})
    );
    assert_eq!(
        lowered.effects[1],
        json!({"kind":"gainLife", "player":controller(), "amount":{"kind":"integer","value":2}})
    );
}

#[test]
fn quantified_discard_reuses_choice_and_random_collection_children_without_targets() {
    for (text, random) in [
        ("Each opponent discards a card.", false),
        ("Each opponent discards two cards at random.", true),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("quantified-lower-discard", &tokens);
        let sequence = recognize_effect_sequence_with_context(
            &mut cursor,
            &[],
            None,
            &EffectRecognitionContext::default(),
        )
        .unwrap();
        let lowered = lower_local_effect_sequence(&mut cursor, &sequence).unwrap();
        assert!(lowered.decisions.is_empty());
        let effects = lowered.effects[0]["effects"].as_array().unwrap();
        assert_eq!(effects.len(), if random { 2 } else { 1 });
        assert_eq!(effects.last().unwrap()["kind"], "discardCards");
        assert_eq!(effects.last().unwrap()["player"]["kind"], "boundValue");
        if random {
            assert_eq!(effects[0]["kind"], "chooseCards");
            assert_eq!(effects[0]["selection"]["kind"], "random");
        }
    }
}

#[test]
fn quantified_gain_retains_the_existing_iteration_ir() {
    let tokens = recognize_entities("Each opponent gains 3 life.");
    let mut cursor = OracleTokenCursor::new("quantified-gain-compatibility", &tokens);
    let sequence = recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
    .unwrap();
    let lowered = lower_local_effect_sequence(&mut cursor, &sequence).unwrap();
    assert_eq!(
        lowered.effects,
        vec![
            json!({"kind":"forEachPlayer","players":{"kind":"opponentsOf","player":controller()},"bind":"quantifiedPlayer","effects":[{"kind":"gainLife","player":{"kind":"boundValue","id":"quantifiedPlayer"},"amount":{"kind":"integer","value":3}}]})
        ]
    );
}

#[test]
fn quantified_atomic_loss_does_not_hoist_target_or_recipient_dependent_amounts() {
    let binding = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };
    let dependent = NumericExpression::CardsInZone {
        player: LocalPlayerReference::ControllerOf(binding),
        zone: LocalZone::Library,
    };
    for amount in [
        dependent.clone(),
        NumericExpression::Add {
            left: Box::new(NumericExpression::Atom(NumericAtom::Integer(1))),
            right: Box::new(dependent),
        },
        NumericExpression::PowerOf(binding),
    ] {
        let node = QuantifiedPlayerActionNode {
            players: PlayerSetReferenceNode::OpponentsOfAbilityController,
            excluding_controller: false,
            actions: vec![QuantifiedPlayerAction::Life(LifeActionNode {
                action: TargetPlayerLifeAction::Lose,
                amount,
            })],
        };
        assert!(lower_quantified_player_action(&node, TargetlessDeclarationSupport::None).is_err());
    }
}
