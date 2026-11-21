use super::super::*;
use super::spell_copy_trigger;

struct NoChoices;
impl DecisionProvider for NoChoices {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Err(EngineError::new(format!(
            "unexpected choice {}",
            request.id
        )))
    }
}

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{0}".to_string(),
        power: None,
        toughness: None,
        rules: Vec::new(),
    }
}

fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|n| PlayerDeck {
                    id: format!("player-{n}"),
                    name: format!("Player {n}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Basic Land ? Island"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        311,
    )
    .expect("copy-program setup")
}

#[test]
fn replicate_payment_schedules_atomic_copies_and_keeps_the_original_snapshot() {
    let mut engine = engine();
    engine.state.step = GameStep::PrecombatMain;
    let mut spell = definition("replicate-draw", "Sorcery");
    spell.mana_cost = "{U}".to_string();
    spell.rules = vec![
        json!({"kind":"keywordAbility","source":{"kind":"self"},"ability":{
            "kind":"replicate","cost":{"kind":"payMana","manaCost":"{1}{U}"},"repeatable":true
        }}),
        json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[{
            "kind":"drawCards","player":{"kind":"abilityController"},"count":{"kind":"integer","value":1}
        }]}),
    ];
    let source = card_instance("player-0", &spell, 71);
    engine.state.players[0].hand = vec![source.clone()];
    engine.state.players[0].mana_pool = ["U", "U", "C", "U", "C"]
        .into_iter()
        .map(|symbol| FloatingMana {
            symbol: symbol.to_string(),
            spend_restriction: None,
        })
        .collect();
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action
                    .decisions
                    .get("replicateCount:0")
                    .and_then(Value::as_u64)
                    == Some(2)
        })
        .expect("two replicate payments available");
    engine
        .apply_priority_action(&action, &mut NoChoices)
        .expect("cast with replicate");
    assert!(engine.state.players[0].mana_pool.is_empty());
    assert_eq!(engine.state.stack.len(), 2);
    let original = engine.state.stack[0].clone();
    let trigger = &engine.state.stack[1];
    assert_eq!(trigger.card.instance_id, source.instance_id);
    assert_eq!(trigger.decisions["triggeringStackId"], original.id);
    assert_eq!(trigger.decisions["copyCount"], 2);
    assert_eq!(
        trigger.ability_rule.as_ref().unwrap()["effects"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        trigger.ability_rule.as_ref().unwrap()["effects"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["kind"] == "copyStackItem")
    );
    engine
        .resolve_top_stack(&mut NoChoices)
        .expect("replicate trigger resolves without choices for a targetless spell");
    assert_eq!(engine.state.stack.len(), 3);
    assert_eq!(engine.state.stack[0].id, original.id);
    assert_eq!(engine.state.stack[0].decisions, original.decisions);
    assert!(
        engine
            .state
            .stack
            .iter()
            .skip(1)
            .all(|copy| copy.controller == original.controller
                && copy.card.definition.rules == original.card.definition.rules)
    );
}

#[test]
fn vanished_original_creates_no_copy_and_zero_count_has_no_effect() {
    let mut engine = engine();
    let source = card_instance("player-0", &definition("copy-source", "Sorcery"), 15);
    engine.put_spell_copy_trigger_on_stack(
        &source,
        "absent-stack",
        2,
        true,
        BTreeMap::new(),
        super::super::trigger_multiplicity::TriggerOrigin::ObjectAbility,
    );
    engine
        .resolve_top_stack(&mut NoChoices)
        .expect("original is absent at resolution");
    assert!(engine.state.stack.is_empty());
    assert_eq!(spell_copy_trigger(0, true)["effects"], json!([]));
}

#[test]
fn removed_opaque_resolvers_and_composites_are_not_executable() {
    for kind in [
        "resolveTriggeredInstruction",
        "resolveSpellInstruction",
        "discardThenDraw",
        "earthbend",
        "exileUntilNextEndStep",
        "exileThenReturnTransformed",
        "sacrificeThenSearchLibrary",
        "scry",
        "surveil",
        "tapOrUntapPermanent",
        "proliferate",
    ] {
        let effect = json!({
            "kind":kind,"operation":if kind=="resolveSpellInstruction" {"sunfall"} else {"punchingBagVitalityClock"},
            "player":{"kind":"abilityController"},"count":{"kind":"integer","value":1},
            "discardCount":{"kind":"integer","value":1},"drawCount":{"kind":"integer","value":1},
            "permanent":{"kind":"chosenTarget","id":"target"},"land":{"kind":"chosenTarget","id":"target"},
            "candidates":{"kind":"permanents","player":{"kind":"abilityController"},"where":{"kind":"cardTypeContains","value":"Land"}},
            "quantity":{"kind":"integer","value":1},"object":{"kind":"self"},"controller":{"kind":"abilityController"},
            "objects":{"kind":"boundObjects","binding":"objects"},"returnUnderOwnerControl":true,
            "sacrificeWhere":{"kind":"cardTypeContains","value":"Creature"},"searchWhere":{"kind":"cardTypeContains","value":"Land"},
            "destination":"battlefield","tapped":true
        });
        assert!(!effect_supported(&effect), "{kind}");
        assert!(
            !rule_is_executable(
                &json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[effect]})
            ),
            "{kind}"
        );
    }

    assert!(effect_supported(&spell_copy_trigger(2, true)["effects"][0]));
}

#[test]
fn copy_trigger_uses_last_known_spell_after_countering_without_casting_it_again() {
    let mut engine = engine();
    let mut spell = definition("x-mode-spell", "Instant");
    spell.mana_cost = "{X}{U}".to_string();
    spell.rules = vec![
        json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[{
            "kind":"drawCards","player":{"kind":"abilityController"},"count":{"kind":"decisionResult","decisionId":"xValue"}
        }]}),
    ];
    assert!(rule_is_executable(&spell.rules[0]));
    let mut original = StackObject {
        id: "stack:original-x".to_string(),
        controller: "player-0".to_string(),
        card: card_instance("player-0", &spell, 81),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::from([
            ("xValue".to_string(), json!(7)),
            ("mode".to_string(), json!("draw")),
        ]),
        targets: BTreeMap::new(),
    };
    // Explicit construction of an already-cast physical fixture carrier.
    engine.admit_new_card_carrier(&mut original.card).unwrap();
    engine.state.stack.push(original.clone());
    engine.put_spell_copy_trigger_on_stack(
        &original.card,
        &original.id,
        2,
        true,
        BTreeMap::new(),
        super::super::trigger_multiplicity::TriggerOrigin::ObjectAbility,
    );
    engine
        .counter_stack_object(&original.id, "counter-source", "player-1", false)
        .unwrap();
    assert_eq!(engine.state.stack.len(), 1);
    let cast_events = engine
        .state
        .events
        .iter()
        .filter(|event| event.kind == "spellCast")
        .count();
    engine.resolve_top_stack(&mut NoChoices).unwrap();
    assert_eq!(engine.state.stack.len(), 2);
    for copy in &engine.state.stack {
        assert_eq!(copy.decisions, original.decisions);
        assert_eq!(copy.card.definition.rules, original.card.definition.rules);
        assert!(is_spell_copy(&copy.card));
        assert!(!copy.card.flags.get("wasCast").copied().unwrap_or(false));
    }
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|event| event.kind == "spellCast")
            .count(),
        cast_events
    );
}

struct ChooseOpponent;
impl DecisionProvider for ChooseOpponent {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        match request.choice.as_ref() {
            Some(DecisionChoice::OptionSelection { options, .. }) => options
                .iter()
                .position(|option| option == "player:player-1")
                .ok_or_else(|| EngineError::new("opponent choice absent")),
            _ => Err(EngineError::new("unexpected rechoice shape")),
        }
    }
}

#[test]
fn departure_refreshes_copy_snapshot_after_a_real_target_change() {
    let mut engine = engine();
    let mut spell = definition("retargeted-x-spell", "Instant");
    spell.mana_cost = "{X}{R}".to_string();
    spell.rules = vec![json!({"kind":"spellAbility","source":{"kind":"self"},
        "declaration":{"kind":"castingDeclaration","decisions":[{
            "kind":"chooseTargets","id":"target","candidates":{"kind":"anyTarget"},
            "quantity":{"kind":"integer","value":1},"minimum":{"kind":"integer","value":1},"maximum":{"kind":"integer","value":1}
        }]},
        "effects":[{"kind":"dealDamage","source":{"kind":"self"},"recipient":{"kind":"chosenTarget","id":"target"},"amount":{"kind":"decisionResult","decisionId":"xValue"}}]
    })];
    assert!(rule_is_executable(&spell.rules[0]));
    let mut original = StackObject {
        id: "stack:retarget-original".to_string(),
        controller: "player-0".to_string(),
        card: card_instance("player-0", &spell, 92),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::from([("xValue".to_string(), json!(4))]),
        targets: BTreeMap::from([(
            "target".to_string(),
            TargetRef::Player {
                player_id: "player-0".to_string(),
            },
        )]),
    };
    // Explicit construction of an already-cast physical fixture carrier.
    engine.admit_new_card_carrier(&mut original.card).unwrap();
    engine.state.stack.push(original.clone());
    engine.put_spell_copy_trigger_on_stack(
        &original.card,
        &original.id,
        2,
        true,
        BTreeMap::new(),
        super::super::trigger_multiplicity::TriggerOrigin::ObjectAbility,
    );
    engine
        .choose_new_targets_for_copy(&original.id, "player-0", &mut ChooseOpponent)
        .expect("change original target");
    assert_eq!(
        engine.state.stack[0].targets["target"],
        TargetRef::Player {
            player_id: "player-1".to_string()
        }
    );
    engine
        .counter_stack_object(&original.id, "counter", "player-1", false)
        .unwrap();
    let trigger = engine.state.stack.last().unwrap().clone();
    let snapshot: StackObject =
        serde_json::from_value(trigger.decisions["triggeringStackSnapshot"].clone()).unwrap();
    assert_eq!(
        snapshot.targets["target"],
        TargetRef::Player {
            player_id: "player-1".to_string()
        }
    );
    // Captured triggering-object information never rescues an ordinary missing target.
    assert!(
        engine
            .copy_stack_item_from_effect(
                &original.id,
                &json!({"kind":"copyStackItem","object":{"kind":"chosenTarget","id":"target"}}),
                &trigger
            )
            .is_none()
    );
    engine.resolve_top_stack(&mut ChooseOpponent).unwrap();
    assert_eq!(engine.state.stack.len(), 2);
    assert!(
        engine
            .state
            .stack
            .iter()
            .all(|copy| copy.decisions["xValue"] == 4
                && copy.targets["target"]
                    == TargetRef::Player {
                        player_id: "player-1".to_string()
                    })
    );
}
