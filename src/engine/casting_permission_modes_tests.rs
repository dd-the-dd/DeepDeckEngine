use super::*;

fn definition(id: &str, ty: &str, mana: &str, rules: Vec<Value>) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":ty,"manaCost":mana,"rules":rules}))
        .unwrap()
}
fn setup() -> GameEngine {
    let filler = definition("filler", "Instant", "{0}", vec![]);
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|n| PlayerDeck {
                    id: format!("p{n}"),
                    name: format!("P{n}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 4],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        771,
    )
    .unwrap();
    for player in &mut engine.state.players {
        player.library.clear();
    }
    let permission = definition(
        "permission",
        "Enchantment",
        "{7}{U}{U}{U}",
        vec![json!({
            "kind":"staticAbility","source":{"kind":"self"},"activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
            "modifiers":[{"kind":"castingPermission","players":{"kind":"controllerOf","object":{"kind":"self"}},"sourceZone":"hand","where":null,"withoutPayingManaCost":true,"asThoughFlash":false}]
        })],
    );
    let spell = definition(
        "spell",
        "Sorcery",
        "{X}{G}",
        vec![json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[]})],
    );
    engine.state.players[0]
        .battlefield
        .push(card_instance("p0", &permission, 1));
    engine.state.players[0]
        .hand
        .push(card_instance("p0", &spell, 2));
    engine.state.players[0].mana_pool = ["G", "C", "C"]
        .into_iter()
        .map(|symbol| FloatingMana {
            symbol: symbol.into(),
            spend_restriction: None,
        })
        .collect();
    engine.state.step = GameStep::PrecombatMain;
    engine.admit_initial_object_identities().unwrap();
    engine
}
#[test]
fn static_permission_keeps_both_modes_and_normal_paid_x() {
    let engine = setup();
    let actions = engine.try_legal_priority_actions(0).unwrap();
    let casts = actions
        .iter()
        .filter(|a| a.kind == ActionKind::CastSpell)
        .collect::<Vec<_>>();
    let ids = casts.iter().map(|a| a.id.as_str()).collect::<BTreeSet<_>>();
    assert_eq!(
        ids.len(),
        casts.len(),
        "each casting route has a unique action id"
    );
    assert!(
        casts
            .iter()
            .any(|a| a.decisions["withoutPayingManaCost"] == false && a.decisions["xValue"] == 2)
    );
    assert!(
        casts
            .iter()
            .any(|a| a.decisions["withoutPayingManaCost"] == true && a.decisions["xValue"] == 0)
    );
    assert!(
        !casts
            .iter()
            .any(|a| a.decisions["withoutPayingManaCost"] == true
                && a.decisions["xValue"].as_i64().unwrap_or(0) > 0)
    );
}
#[test]
fn stale_static_waiver_is_rejected_before_mutation() {
    let mut engine = setup();
    let action = engine
        .try_legal_priority_actions(0)
        .unwrap()
        .into_iter()
        .find(|a| a.kind == ActionKind::CastSpell && a.decisions["withoutPayingManaCost"] == true)
        .unwrap();
    assert!(
        engine
            .validate_static_casting_base_permission(&action)
            .is_ok()
    );
    engine.state.players[0].battlefield.clear();
    let before = serde_json::to_value(&engine.state).unwrap();
    let error = engine
        .apply_checked_priority_action(&action, &mut NoChoices)
        .unwrap_err();
    assert!(error.to_string().contains("permission is no longer active"));
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
    let request = CanonicalSpellCast {
        controller_id: "p0".into(),
        source_definition_id: "spell".into(),
        use_alternative_cost: false,
        alternative_exile_definition_id: None,
        declaration_decisions: BTreeMap::from([("withoutPayingManaCost".into(), json!(true))]),
    };
    assert!(!engine.canonical_spell_cast_available(&request).unwrap());
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
}

struct NoChoices;
impl DecisionProvider for NoChoices {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Err(EngineError::new("unexpected permission proof callback"))
    }
}

#[test]
fn miracle_window_does_not_authorize_static_waiver_or_normal_paid_timing() {
    let mut engine = setup();
    engine.state.active_player = 1;
    engine.state.step = GameStep::Upkeep;
    let spell_id = engine.state.players[0].hand[0].instance_id.clone();
    engine.state.permissions.push(json!({"kind":"miraclePermission","playerId":"p0","cardInstanceId":spell_id,"whileInZone":"hand","alternativeManaCost":"{G}"}));
    let casts = engine
        .try_legal_priority_actions(0)
        .unwrap()
        .into_iter()
        .filter(|a| a.kind == ActionKind::CastSpell)
        .collect::<Vec<_>>();
    assert!(!casts.is_empty());
    assert!(casts.iter().all(|a| {
        a.decisions.get("miracleCast").and_then(Value::as_bool) == Some(true)
            && a.decisions
                .get("withoutPayingManaCost")
                .and_then(Value::as_bool)
                != Some(true)
    }));
    engine.state.players[0].mana_pool.clear();
    assert!(
        !engine
            .try_legal_priority_actions(0)
            .unwrap()
            .iter()
            .any(|a| a.kind == ActionKind::CastSpell)
    );
    engine.state.active_player = 0;
    engine.state.step = GameStep::PrecombatMain;
    assert!(
        engine
            .try_legal_priority_actions(0)
            .unwrap()
            .iter()
            .any(|a| a.kind == ActionKind::CastSpell
                && a.decisions
                    .get("withoutPayingManaCost")
                    .and_then(Value::as_bool)
                    == Some(true)
                && a.decisions.get("miracleCast").and_then(Value::as_bool) != Some(true))
    );
}
#[test]
fn untagged_forged_waiver_is_not_an_action_and_resolution_context_stays_separate() {
    let mut engine = setup();
    let mut action = engine
        .try_legal_priority_actions(0)
        .unwrap()
        .into_iter()
        .find(|a| a.kind == ActionKind::CastSpell && a.decisions["withoutPayingManaCost"] == true)
        .unwrap();
    let mut wrong_policy = action.clone();
    wrong_policy
        .decisions
        .get_mut("staticCastingPermissionAuthorization")
        .unwrap()["policy"]["withoutPayingManaCost"] = json!(false);
    let before_policy = serde_json::to_value(&engine.state).unwrap();
    assert!(
        engine
            .apply_checked_priority_action(&wrong_policy, &mut NoChoices)
            .unwrap_err()
            .to_string()
            .contains("permission is no longer active")
    );
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before_policy);
    action
        .decisions
        .remove("usesStaticCastingPermissionBaseCost");
    action
        .decisions
        .remove("staticCastingPermissionAuthorization");
    let before = serde_json::to_value(&engine.state).unwrap();
    assert!(
        engine
            .apply_checked_priority_action(&action, &mut NoChoices)
            .unwrap_err()
            .to_string()
            .contains("untagged hand casting waiver")
    );
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
    // This is the unchanged private resolution constructor, not a fabricated
    // public flag: its actions already carry the trusted resolution context.
    let cards = engine.state.players[0].hand.clone();
    let resolution = engine
        .cast_one_card_actions(0, &cards, "hand", false, None)
        .expect("checked resolution casting dependencies");
    assert!(!resolution.is_empty());
    assert!(
        resolution
            .iter()
            .all(|a| engine.validate_static_casting_base_permission(a).is_ok())
    );
}

#[test]
fn permission_sacrifice_choice_never_enters_stack_target_designations() {
    let mut engine = setup();
    engine.state.players[0].battlefield[0].definition.rules[0]["modifiers"][0]["additionalCost"] =
        json!({"kind":"sacrificePermanent","where":{"kind":"cardTypeContains","value":"Creature"}});
    let creature = definition(
        "cost-creature",
        "Creature",
        "{0}",
        vec![
            json!({"kind":"keywordAbility","source":{"kind":"self"},"ability":{"kind":"ward","cost":{"kind":"payMana","manaCost":"{2}"}}}),
        ],
    );
    let mut creature = creature;
    creature.power = Some("1".into());
    creature.toughness = Some("1".into());
    engine.state.players[0]
        .battlefield
        .push(card_instance("p0", &creature, 3));
    engine.admit_explicit_test_fixture_objects().unwrap();
    let action = engine
        .try_legal_priority_actions(0)
        .unwrap()
        .into_iter()
        .find(|a| a.kind == ActionKind::CastSpell && a.decisions["withoutPayingManaCost"] == true)
        .unwrap();
    assert!(
        action
            .decisions
            .get("staticCastingSacrifice")
            .and_then(Value::as_str)
            .is_some()
    );
    assert!(action.targets.is_empty());
    assert!(action.target_order.is_empty());
    engine
        .apply_checked_priority_action(&action, &mut NoChoices)
        .unwrap();
    assert_eq!(
        engine.state.stack.len(),
        1,
        "cost payment must not enqueue a Ward trigger"
    );
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|c| c.definition.id == "cost-creature")
    );
    assert_eq!(
        engine.state.players[0].mana_pool.len(),
        3,
        "waived base and cost choice do not pay Ward"
    );
}
