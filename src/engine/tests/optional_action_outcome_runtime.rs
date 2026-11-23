use super::*;

// Optional instructions (not costs) whose outcome follows actual performance
// (CR 608.2c, 603.12). Real Oracle texts.
const LOOT_TEXT: &str = "Whenever this creature deals combat damage to a player, you may draw a card. If you do, discard a card.";
const DECLINED_TEXT: &str = "At the beginning of your upkeep, you may sacrifice an artifact, enchantment, or token. If you don't, tap this creature.";
const REFLEXIVE_TEXT: &str = "When The Fire Nation Drill enters, you may tap it. When you do, destroy target creature with power 4 or less.";

/// Answers the optional-action choice, or an optional resolution payment
/// (pay or decline), with `self.0`; any other decision (discard, sacrifice
/// or reflexive target) takes its first option.
struct Perform(bool);
impl DecisionProvider for Perform {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if let Some(decline) = request
            .options
            .iter()
            .position(|option| option.kind == ActionKind::DeclinePayment)
        {
            return Ok(if self.0 {
                request
                    .options
                    .iter()
                    .position(|option| option.kind != ActionKind::DeclinePayment)
                    .unwrap_or(decline)
            } else {
                decline
            });
        }
        Ok(request
            .options
            .iter()
            .position(|option| {
                option.decisions.get("performOptionalAction") == Some(&Value::Bool(self.0))
            })
            .unwrap_or(0))
    }
}

fn parsed_effect(name: &str, type_line: &str, text: &str) -> (Value, Value) {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0].rule.clone().expect("canonical rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    let effect = rule["effects"][0].clone();
    // `you may sacrifice X. If you don't, ...` is an optional resolution cost
    // whether X is one object or a closed object list (roadmap #12).
    assert!(
        matches!(
            effect["kind"].as_str(),
            Some("optionalAction" | "optionalPayCostPerformEffects")
        ),
        "{effect:#}"
    );
    (rule, effect)
}

fn admitted(engine: &mut GameEngine, mut card: CardInstance) -> CardInstance {
    engine
        .assign_new_object_incarnation(&mut card)
        .expect("admit test object");
    card
}

fn card(engine: &mut GameEngine, id: &str, type_line: &str) -> CardInstance {
    admitted(
        engine,
        test_instance(id, test_definition(id, type_line), "player-0"),
    )
}

fn resolve(
    engine: &mut GameEngine,
    source: CardInstance,
    rule: &Value,
    effect: &Value,
    provider: &mut impl DecisionProvider,
) {
    let stack_object = StackObject {
        id: "stack:trigger".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .execute_effect(
            effect,
            None,
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            provider,
        )
        .expect("the optional action program resolves");
}

fn last_outcome(engine: &GameEngine) -> (Value, Value) {
    let event = engine
        .state
        .events
        .iter()
        .rev()
        .find(|event| event.kind == "optionalActionChosen")
        .expect("optional action outcome event");
    (
        event.detail["chosen"].clone(),
        event.detail["performed"].clone(),
    )
}

fn loot_engine() -> (GameEngine, CardInstance) {
    let mut engine = test_engine(2);
    let source = card(&mut engine, "source", "Creature — Human Monk");
    engine.state.players[0].battlefield = vec![source.clone()];
    let in_hand = card(&mut engine, "in-hand", "Instant");
    let in_library = card(&mut engine, "in-library", "Sorcery");
    engine.state.players[0].hand = vec![in_hand];
    engine.state.players[0].library = vec![in_library];
    engine.state.players[0].graveyard.clear();
    (engine, source)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn if_you_do_follows_a_performed_draw() {
    let (rule, effect) = parsed_effect("Jeskai Elder", "Creature — Human Monk", LOOT_TEXT);
    let (mut engine, source) = loot_engine();
    resolve(&mut engine, source, &rule, &effect, &mut Perform(true));
    assert!(
        engine.state.players[0].library.is_empty(),
        "the card was drawn"
    );
    assert_eq!(
        engine.state.players[0].hand.len(),
        1,
        "drew one, discarded one"
    );
    assert_eq!(engine.state.players[0].graveyard.len(), 1);
    assert_eq!(last_outcome(&engine), (json!(true), json!(true)));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_chosen_draw_from_an_empty_library_is_not_performed() {
    // CR 121.3 allows the choice; nothing is drawn, so `If you do` is false.
    let (rule, effect) = parsed_effect("Jeskai Elder", "Creature — Human Monk", LOOT_TEXT);
    let (mut engine, source) = loot_engine();
    engine.state.players[0].library.clear();
    resolve(&mut engine, source, &rule, &effect, &mut Perform(true));
    assert_eq!(
        engine.state.players[0].hand.len(),
        1,
        "nothing was discarded"
    );
    assert!(engine.state.players[0].graveyard.is_empty());
    assert_eq!(last_outcome(&engine), (json!(true), json!(false)));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_declined_draw_performs_nothing() {
    let (rule, effect) = parsed_effect("Jeskai Elder", "Creature — Human Monk", LOOT_TEXT);
    let (mut engine, source) = loot_engine();
    resolve(&mut engine, source, &rule, &effect, &mut Perform(false));
    assert_eq!(engine.state.players[0].library.len(), 1);
    assert_eq!(engine.state.players[0].hand.len(), 1);
    assert!(engine.state.players[0].graveyard.is_empty());
    assert_eq!(last_outcome(&engine), (json!(false), json!(false)));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn if_you_dont_follows_a_declined_or_impossible_instruction() {
    let (rule, effect) = parsed_effect("Devouring Sugarmaw", "Creature — Horror", DECLINED_TEXT);
    let tapped = |engine: &GameEngine| engine.state.players[0].battlefield[0].tapped;

    // Nothing to sacrifice: the option can't be chosen (CR 608.2d).
    let mut engine = test_engine(2);
    let source = card(&mut engine, "source", "Creature — Horror");
    engine.state.players[0].battlefield = vec![source.clone()];
    resolve(
        &mut engine,
        source.clone(),
        &rule,
        &effect,
        &mut Perform(true),
    );
    assert!(tapped(&engine), "an impossible sacrifice is not performed");

    // Performed: the artifact is sacrificed and the source stays untapped.
    let mut engine = test_engine(2);
    let artifact = card(&mut engine, "artifact", "Artifact");
    engine.state.players[0].battlefield = vec![source.clone(), artifact.clone()];
    resolve(
        &mut engine,
        source.clone(),
        &rule,
        &effect,
        &mut Perform(true),
    );
    assert_eq!(engine.state.players[0].battlefield.len(), 1);
    assert!(!tapped(&engine));
    if effect["kind"] == "optionalAction" {
        assert_eq!(last_outcome(&engine), (json!(true), json!(true)));
    }

    // Declined: nothing is sacrificed and the source is tapped.
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![source.clone(), artifact];
    resolve(&mut engine, source, &rule, &effect, &mut Perform(false));
    assert_eq!(engine.state.players[0].battlefield.len(), 2);
    assert!(tapped(&engine));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn when_you_do_triggers_only_after_the_instruction_happened() {
    let (rule, effect) = parsed_effect(
        "The Fire Nation Drill",
        "Legendary Artifact — Vehicle",
        REFLEXIVE_TEXT,
    );
    assert!(
        rule["declaration"].is_null(),
        "the outer trigger declares no target"
    );
    let mut engine = test_engine(2);
    let source = card(&mut engine, "source", "Legendary Artifact — Vehicle");
    let victim = admitted(
        &mut engine,
        test_instance("victim", test_definition("victim", "Creature"), "player-1"),
    );
    engine.state.players[0].battlefield = vec![source.clone()];
    engine.state.players[1].battlefield = vec![victim];
    let stack = engine.state.stack.len();

    // Already tapped: choosing to tap it changes nothing, so no reflexive trigger.
    engine.state.players[0].battlefield[0].tapped = true;
    resolve(
        &mut engine,
        source.clone(),
        &rule,
        &effect,
        &mut Perform(true),
    );
    assert_eq!(engine.state.stack.len(), stack);
    assert_eq!(last_outcome(&engine), (json!(true), json!(false)));

    // Untapped: tapping it creates the reflexive trigger with its own target.
    engine.state.players[0].battlefield[0].tapped = false;
    resolve(&mut engine, source, &rule, &effect, &mut Perform(true));
    assert!(engine.state.players[0].battlefield[0].tapped);
    assert_eq!(engine.state.stack.len(), stack + 1);
    let reflexive = engine.state.stack.last().expect("reflexive trigger");
    assert_eq!(reflexive.targets.len(), 1);
    assert_eq!(
        reflexive.ability_rule.as_ref().expect("reflexive rule")["effects"][0]["kind"],
        "destroyPermanent"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn zone_change_witness_requires_the_object_to_leave_its_zone() {
    // New combination: a returned bound card, measured by its zone change.
    let effect = json!({
        "kind": "optionalAction",
        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        "action": {
            "kind": "moveCards",
            "cards": { "kind": "boundObjects", "binding": "returnedCards" },
            "from": { "kind": "graveyard", "player": { "kind": "controllerOf", "object": { "kind": "self" } } },
            "to": { "kind": "hand", "player": { "kind": "controllerOf", "object": { "kind": "self" } } },
        },
        "onPerformed": [{
            "kind": "gainLife",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "amount": { "kind": "integer", "value": 3 },
        }],
    });
    assert!(effect_supported(&effect));
    let mut engine = test_engine(2);
    let source = card(&mut engine, "source", "Creature");
    let returned = card(&mut engine, "returned", "Creature");
    engine.state.players[0].battlefield = vec![source.clone()];
    engine.state.players[0].graveyard = vec![returned];
    let stack_object = StackObject {
        id: "stack:trigger".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    let mut bindings = BTreeMap::from([(
        "returnedCards".to_string(),
        RuntimeBinding::Objects(vec!["returned".to_string()]),
    )]);
    let life = engine.state.players[0].life;
    engine
        .execute_effect(
            &effect,
            None,
            &stack_object,
            &mut bindings,
            &mut BTreeMap::new(),
            &mut Perform(true),
        )
        .expect("resolves");
    assert!(
        engine.state.players[0]
            .hand
            .iter()
            .any(|card| card.instance_id == "returned")
    );
    assert_eq!(engine.state.players[0].life, life + 3);

    // An unmeasured instruction is unsupported rather than `chosen = performed`.
    let mut unmeasured = effect.clone();
    unmeasured["action"] = json!({ "kind": "shuffleZone", "zone": { "kind": "library", "player": { "kind": "controllerOf", "object": { "kind": "self" } } } });
    assert!(!effect_supported(&unmeasured));
}
