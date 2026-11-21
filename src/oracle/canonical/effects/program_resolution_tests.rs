use super::*;
use crate::oracle::syntax::recognize_entities;

fn parse(text: &str) -> EffectSequenceNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("resolution-unit", &tokens);
    recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
    .unwrap()
}

fn body(sequence: EffectSequenceNode) -> AbilityBodyNode {
    AbilityBodyNode {
        context: EffectProgramContext::Spell,
        costs: None,
        program: EffectProgramNode::Sequence(sequence),
        annotation: None,
    }
}

fn resolve(sequence: EffectSequenceNode) -> Result<AbilityBodyResolution, ProgramResolutionError> {
    resolve_ability_body_scopes(body(sequence))
}

fn with_effects(effects: Vec<LocalEffectNode>) -> EffectSequenceNode {
    EffectSequenceNode {
        connectors: vec![EffectConnector::Then; effects.len().saturating_sub(1)],
        effects,
        where_x: Vec::new(),
        conditionals: Vec::new(),
        unless: Vec::new(),
    }
}

#[test]
fn ordered_player_producer_is_required_before_lowering() {
    let sequence = parse("Target player draws two cards and loses 2 life.");
    assert!(matches!(
        resolve(sequence.clone()),
        Ok(AbilityBodyResolution::ScopeChecked(_))
    ));
    let error = resolve(with_effects(vec![sequence.effects[1].clone()]))
        .err()
        .unwrap();
    assert_eq!(error.state, "lower.player-action.subject");
    assert_eq!(
        error.expectation,
        "an earlier declared player subject target slot"
    );
    assert!(!error.support_failure);
}

#[test]
fn player_and_collection_producers_cannot_launder_each_other() {
    let players = parse("Target player draws a card and loses 1 life.");
    let cards = parse("Look at the top two cards of your library. Reveal them.");
    let error = resolve(with_effects(vec![
        cards.effects[0].clone(),
        players.effects[1].clone(),
    ]))
    .err()
    .unwrap();
    assert_eq!(error.state, "lower.player-action.subject");
    let error = resolve(with_effects(vec![
        players.effects[0].clone(),
        cards.effects[1].clone(),
    ]))
    .err()
    .unwrap();
    assert_eq!(error.state, "lower.card-action.bindings");
    assert!(error.support_failure);
}

#[test]
fn nested_optional_player_producers_neither_borrow_nor_escape() {
    let players = parse("Target player draws a card and loses 1 life.");
    let declaration = players.effects[0].clone();
    let bound_use = players.effects[1].clone();
    for sequence in [
        with_effects(vec![
            declaration.clone(),
            LocalEffectNode::OptionalSequence(with_effects(vec![bound_use.clone()])),
        ]),
        with_effects(vec![
            LocalEffectNode::OptionalSequence(with_effects(vec![declaration])),
            bound_use,
        ]),
    ] {
        let error = resolve(sequence).err().unwrap();
        assert_eq!(error.state, "lower.player-action.subject");
    }
}

#[test]
fn collection_focus_is_produced_and_invalidated_before_json_lowering() {
    let cards = parse("Look at the top two cards of your library. Reveal them.");
    assert!(matches!(
        resolve(cards.clone()),
        Ok(AbilityBodyResolution::ScopeChecked(_))
    ));
    let draw = parse("Draw a card.").effects.remove(0);
    // Optional observation/export rejection retains its existing native owner.
    assert!(matches!(
        resolve(with_effects(vec![
            cards.effects[0].clone(),
            LocalEffectNode::OptionalSequence(with_effects(vec![draw.clone()])),
            cards.effects[1].clone(),
        ])),
        Ok(AbilityBodyResolution::OutsideSlice(_))
    ));
    let error = resolve(with_effects(vec![
        cards.effects[0].clone(),
        draw,
        cards.effects[1].clone(),
    ]))
    .err()
    .unwrap();
    assert_eq!(
        error.expectation,
        "an observed card collection in the current lexical scope"
    );
    let error = resolve(with_effects(vec![
        cards.effects[1].clone(),
        cards.effects[0].clone(),
    ]))
    .err()
    .unwrap();
    assert_eq!(
        error.expectation,
        "an observed card collection in the current lexical scope"
    );
}

#[test]
fn outside_slice_is_structural_and_checked_program_keeps_native_ir_exact() {
    let players = parse("Target player draws two cards and loses 2 life.");
    let outside = parse("Add {G}.").effects.remove(0);
    // The invalid reference is not inspected under a falsely checked marker.
    assert!(matches!(
        resolve(with_effects(vec![players.effects[1].clone(), outside])),
        Ok(AbilityBodyResolution::OutsideSlice(_))
    ));

    let mut activation = body(players.clone());
    activation.context = EffectProgramContext::Activated;
    activation.costs = Some(ActivationCostSequenceNode {
        costs: vec![],
        connectors: vec![],
    });
    assert!(matches!(
        resolve_ability_body_scopes(activation),
        Ok(AbilityBodyResolution::OutsideSlice(_))
    ));
    let filtered = LocalEffectNode::CardAction(CardActionNode::Reveal(CardOperandNode::Selection(
        CardSelectionOperandNode {
            from: CardCollectionReferenceNode::Antecedent,
            quantity: CardSelectionQuantityNode::Exactly(NumericExpression::from(1)),
            predicate: Some(ObjectPredicate::ObjectKind(ObjectKind::Card)),
        },
    )));
    // Query reference resolution has not migrated, even with a literal count.
    assert!(matches!(
        resolve(with_effects(vec![filtered])),
        Ok(AbilityBodyResolution::OutsideSlice(_))
    ));

    match resolve(players.clone()).unwrap() {
        AbilityBodyResolution::ScopeChecked(_) => {}
        AbilityBodyResolution::OutsideSlice(_) => panic!("expected checked player scope"),
    }
    let tokens = recognize_entities("");
    let mut cursor = OracleTokenCursor::new("resolution-unit", &tokens);
    let EffectProgramParseAttempt::Parsed(after) =
        lower_contextual_effect_program(&mut cursor, EffectProgramContext::Spell, None, players)
    else {
        panic!("checked contextual lowering")
    };
    assert_eq!(
        after.rule,
        json!({
            "kind":"spellAbility", "source":{"kind":"self"},
            "declaration":{"kind":"castingDeclaration","decisions":[{"kind":"chooseTargets","id":"targetPlayer","minimum":1,"maximum":1,"candidates":{"kind":"players"}}]},
            "effects":[
                {"kind":"drawCards","player":{"kind":"chosenTarget","id":"targetPlayer"},"count":{"kind":"integer","value":2}},
                {"kind":"loseLife","player":{"kind":"chosenTarget","id":"targetPlayer"},"amount":{"kind":"integer","value":2}}
            ]
        })
    );
}
