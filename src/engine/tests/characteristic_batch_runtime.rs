use super::*;

const ORACLE: &str = "Destroy target non-outlaw creature. (Assassins, Mercenaries, Pirates, Rogues, and Warlocks are outlaws. Everyone else is fair game.)";

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn negated_subtype_batch_filters_targets_and_executes_atomic_destruction() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Shoot the Sheriff".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{B}".to_string()),
        oracle_text: Some(ORACLE.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical");
    assert_eq!(
        parsed.abilities[0].recognizer,
        crate::oracle::RecognizerProvenance::Native
    );
    let rule = parsed.abilities[0]
        .rule
        .clone()
        .expect("the native batch selector emits a rule");
    assert!(rule_is_executable(&rule));

    let mut spell = test_definition("batch-destruction", "Instant");
    spell.mana_cost = "{1}{B}".to_string();
    spell.rules = vec![rule];
    let bear = test_instance(
        "eligible-bear",
        test_definition("eligible-bear", "Creature - Bear"),
        "player-1",
    );
    let rogue = test_instance(
        "ineligible-rogue",
        test_definition("ineligible-rogue", "Creature - Human Rogue"),
        "player-1",
    );
    let assassin = test_instance(
        "ineligible-assassin",
        test_definition("ineligible-assassin", "Creature - Assassin"),
        "player-1",
    );
    let mut changeling_definition =
        test_definition("ineligible-changeling", "Creature - Shapeshifter");
    changeling_definition.rules = vec![json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "changeling" },
    })];
    let changeling = test_instance("ineligible-changeling", changeling_definition, "player-1");
    let artifact = test_instance(
        "ineligible-noncreature",
        test_definition("ineligible-noncreature", "Artifact"),
        "player-1",
    );

    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.players[0].mana_pool = vec![
        FloatingMana {
            symbol: "C".to_string(),
            spend_restriction: None,
        },
        FloatingMana {
            symbol: "B".to_string(),
            spend_restriction: None,
        },
    ];
    engine.state.players[0].hand = vec![test_instance("batch-destruction", spell, "player-0")];
    engine.state.players[1].battlefield = vec![bear, rogue, assassin, changeling, artifact];

    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let casts = engine
        .legal_priority_actions(0)
        .into_iter()
        .filter(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("batch-destruction")
        })
        .collect::<Vec<_>>();
    assert_eq!(casts.len(), 1, "only the Bear is a legal target");
    assert_eq!(
        casts[0].targets.get("targetPermanent"),
        Some(&TargetRef::Permanent {
            instance_id: "eligible-bear".to_string(),
        })
    );

    engine
        .apply_priority_action(&casts[0], &mut RandomAi::seeded(31))
        .expect("cast the primitive destruction spell");
    engine
        .resolve_top_stack(&mut RandomAi::seeded(32))
        .expect("resolve the primitive destroyPermanent operation");

    assert!(
        engine.state.players[1]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "eligible-bear")
    );
    for protected in [
        "ineligible-rogue",
        "ineligible-assassin",
        "ineligible-changeling",
        "ineligible-noncreature",
    ] {
        assert!(
            engine.state.players[1]
                .battlefield
                .iter()
                .any(|card| card.instance_id == protected)
        );
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn subtype_filter_does_not_confuse_a_card_type_with_a_subtype() {
    // Runtime boundary: a negated *subtype* atom naming a card-type word must
    // not exclude objects that merely have that card type. The compiler
    // normalizes the synthetic text "non-Creature" to the card-type atom, so
    // the subtype atom is supplied directly in the shared target IR shape.
    let rule = json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "kind": "chooseTargets",
                "id": "targetPermanent",
                "minimum": 1,
                "maximum": 1,
                "candidates": {
                    "kind": "permanents",
                    "where": {
                        "kind": "and",
                        "operands": [
                            {
                                "kind": "not",
                                "operand": { "kind": "subtypeContains", "value": "Creature" },
                            },
                            { "kind": "cardTypeContains", "value": "Creature" },
                        ],
                    },
                },
            }],
        },
        "effects": [{
            "kind": "destroyPermanent",
            "permanent": { "kind": "chosenTarget", "id": "targetPermanent" },
        }],
    });
    assert!(rule_is_executable(&rule));
    let mut spell = test_definition("subtype-boundary", "Instant");
    spell.rules = vec![rule];

    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.players[0].hand = vec![test_instance("subtype-boundary", spell, "player-0")];
    engine.state.players[1].battlefield = vec![test_instance(
        "human-creature",
        test_definition("human-creature", "Creature - Human"),
        "player-1",
    )];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");

    assert!(engine.legal_priority_actions(0).iter().any(|action| {
        action.kind == ActionKind::CastSpell
            && action.targets.get("targetPermanent")
                == Some(&TargetRef::Permanent {
                    instance_id: "human-creature".to_string(),
                })
    }));
}
