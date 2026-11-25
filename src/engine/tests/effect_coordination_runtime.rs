//! Runtime proofs of effect coordination: a finite gapped damage conjunct
//! (Kaervek's Hex), a resolution choice between `or`-joined actions
//! (Catastrophe, CR 608.2d) and a zone-move tail that exiles the resolving
//! spell with counters (Suspended Sentence, CR 702.62a). Each case starts
//! from Oracle text, so the parser-to-runtime path is the one proved.
use super::*;

fn parse(name: &str, type_line: &str, mana_cost: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("effect-coordination-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:?}",
        parsed.diagnostics
    );
    let rules = parsed
        .abilities
        .iter()
        .map(|ability| ability.rule.clone().expect("canonical ability has a rule"))
        .collect::<Vec<_>>();
    assert!(rules.iter().all(rule_is_executable), "{oracle}: {rules:?}");
    rules
}

fn spell_of(source: CardInstance) -> StackObject {
    StackObject {
        id: format!("stack:{}", source.instance_id),
        controller: source.controller.clone(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn resolve<P: DecisionProvider>(
    engine: &mut GameEngine,
    effects: &Value,
    stack_object: &StackObject,
    provider: &mut P,
) {
    let mut decisions = stack_object.decisions.clone();
    execute_effects_at_resolution_start(
        engine,
        effects,
        stack_object,
        &mut BTreeMap::new(),
        &mut decisions,
        provider,
    )
    .expect("the effects resolve");
}

fn creature(id: &str, mana_cost: &str, owner: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature - Beast");
    definition.mana_cost = mana_cost.to_string();
    definition.toughness = Some("5".to_string());
    test_instance(id, definition, owner)
}

fn damage_on(engine: &GameEngine, id: &str) -> i32 {
    engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|card| card.instance_id == id)
        .map(|card| card.damage_marked)
        .expect("the creature is on the battlefield")
}

/// Picks the resolution option whose position is `index`.
struct ChooseOption(usize);

impl DecisionProvider for ChooseOption {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(match request.kind {
            DecisionKind::ResolutionChoice => self.0.min(request.options.len().saturating_sub(1)),
            _ => 0,
        })
    }
}

/// `Kaervek's Hex deals 1 damage to each nonblack creature and an additional
/// 1 damage to each green creature.`: the gapped conjunct is a second damage
/// action of the same source, so a green creature is dealt 2 in all and a
/// nongreen nonblack creature 1.
#[test]
fn kaerveks_hex_deals_two_to_a_green_creature_and_one_to_a_red_one() {
    let rules = parse(
        "Kaervek's Hex",
        "Sorcery",
        "{3}{B}",
        "Kaervek's Hex deals 1 damage to each nonblack creature and an additional 1 damage to each green creature.",
    );
    let mut engine = test_engine(2);
    engine.state.players[1]
        .battlefield
        .push(creature("green", "{G}", "player-1"));
    engine.state.players[1]
        .battlefield
        .push(creature("red", "{R}", "player-1"));
    engine.state.players[1]
        .battlefield
        .push(creature("black", "{B}", "player-1"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let hex = test_instance("hex", test_definition("hex", "Sorcery"), "player-0");
    resolve(
        &mut engine,
        &rules[0]["effects"],
        &spell_of(hex),
        &mut ChooseOption(0),
    );
    assert_eq!(damage_on(&engine, "green"), 2);
    assert_eq!(damage_on(&engine, "red"), 1);
    assert_eq!(damage_on(&engine, "black"), 0);
}

/// `Destroy all lands or all creatures.` (Catastrophe): the controller
/// chooses one alternative while the spell resolves; only that set is
/// destroyed, whichever is chosen.
#[test]
fn catastrophe_offers_the_choice_and_destroys_only_one_set() {
    let rules = parse(
        "Catastrophe",
        "Sorcery",
        "{4}{W}{W}",
        "Destroy all lands or all creatures. Creatures destroyed this way can't be regenerated.",
    );
    assert_eq!(rules[0]["effects"][0]["kind"], "chooseOneEffect");
    assert_eq!(
        rules[0]["effects"][0]["options"].as_array().map(Vec::len),
        Some(2)
    );
    for (choice, land_survives, creature_survives) in [(0, false, true), (1, true, false)] {
        let mut engine = test_engine(2);
        engine.state.players[1].battlefield.push(test_instance(
            "forest",
            test_definition("forest", "Basic Land - Forest"),
            "player-1",
        ));
        engine.state.players[1]
            .battlefield
            .push(creature("bear", "{1}{G}", "player-1"));
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        let spell = test_instance(
            "catastrophe",
            test_definition("catastrophe", "Sorcery"),
            "player-0",
        );
        resolve(
            &mut engine,
            &rules[0]["effects"],
            &spell_of(spell),
            &mut ChooseOption(choice),
        );
        let ids = engine.state.players[1]
            .battlefield
            .iter()
            .map(|card| card.instance_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids.contains(&"forest"), land_survives, "choice {choice}");
        assert_eq!(ids.contains(&"bear"), creature_survives, "choice {choice}");
    }
}

/// `Exile Suspended Sentence with three time counters on it.`: the
/// resolving spell's own move to exile carries the counters, which stay on
/// the card in exile (CR 608.2n, 702.62a).
#[test]
fn suspended_sentence_is_exiled_with_three_time_counters() {
    let rules = parse(
        "Suspended Sentence",
        "Sorcery",
        "{3}{B}",
        "Destroy target creature an opponent controls. That player loses 3 life. Exile Suspended Sentence with three time counters on it.",
    );
    let exile = rules[0]["effects"]
        .as_array()
        .and_then(|effects| effects.last())
        .expect("the source exile is the last instruction")
        .clone();
    assert_eq!(exile["kind"], "moveCards");
    assert_eq!(exile["to"]["enterWithCounters"][0]["counter"], "time");
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    let mut definition = test_definition("sentence", "Sorcery");
    definition.rules = vec![json!({
        "kind": "spellAbility",
        "source": {"kind": "self"},
        "effects": [exile],
    })];
    engine.state.players[0]
        .hand
        .push(test_instance("sentence", definition, "player-0"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let cast = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("sentence")
        })
        .expect("the spell can be cast from hand");
    engine
        .apply_priority_action(&cast, &mut EmeritusDecisionProvider)
        .expect("the spell is cast");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the spell resolves");
    let exiled = engine.state.players[0]
        .exile
        .iter()
        .find(|card| card.instance_id == "sentence")
        .expect("the spell ends in exile");
    assert_eq!(exiled.counters.get("time"), Some(&3));
    assert!(
        !engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "sentence")
    );
}

/// A card exiled from a graveyard `with two time counters on it` keeps them
/// in exile: the same placement on an ordinary card move.
#[test]
fn a_card_moved_to_exile_receives_the_destination_counters() {
    let mut engine = test_engine(2);
    engine.state.players[1].graveyard.push(test_instance(
        "relic",
        test_definition("relic", "Artifact"),
        "player-1",
    ));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let spell = test_instance(
        "platform",
        test_definition("platform", "Sorcery"),
        "player-0",
    );
    resolve(
        &mut engine,
        &json!([{
            "kind": "moveCards",
            "cards": {
                "kind": "cardsInZone",
                "zone": {
                    "kind": "graveyard",
                    "players": {
                        "kind": "opponentsOf",
                        "player": {"kind": "controllerOf", "object": {"kind": "self"}},
                    },
                },
            },
            "to": {
                "kind": "exile",
                "player": {"kind": "ownerOf", "object": {"kind": "candidate"}},
                "faceDown": false,
                "enterWithCounters": [{"counter": "time", "count": {"kind": "integer", "value": 2}}],
            },
        }]),
        &spell_of(spell),
        &mut ChooseOption(0),
    );
    let exiled = engine.state.players[1]
        .exile
        .iter()
        .find(|card| card.instance_id == "relic")
        .expect("the card is exiled");
    assert_eq!(exiled.counters.get("time"), Some(&2));
}
