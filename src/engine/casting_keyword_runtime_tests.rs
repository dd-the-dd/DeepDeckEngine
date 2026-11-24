//! Runtime proofs of the casting-keyword and keyword-cost lowerings: escape
//! from the graveyard with an exact exile count, a conditional alternative
//! cost from hand, a nonmana kicker payment, disguise's face-down ward and
//! a printed `hexproof from`.
use super::super::*;

struct FirstOption;

impl DecisionProvider for FirstOption {
    fn choose(
        &mut self,
        _state: &GameState,
        _request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(0)
    }
}

fn test_definition(id: &str, type_line: &str) -> CardDefinition {
    serde_json::from_value(json!({
        "id": id, "name": id, "typeLine": type_line,
        "power": (type_line == "Creature").then_some("2"),
        "toughness": (type_line == "Creature").then_some("2"),
    }))
    .expect("standalone test definition is valid")
}

fn test_instance(id: &str, definition: CardDefinition, owner: &str) -> CardInstance {
    let mut instance = card_instance(owner, &definition, 0);
    instance.instance_id = id.into();
    instance
}

fn test_engine() -> GameEngine {
    let filler = test_definition("filler", "Basic Land - Plains");
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        2207,
    )
    .expect("standalone test engine is valid");
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.priority_player = Some(0);
    engine.state.players[0].hand.clear();
    engine.state.players[0].graveyard.clear();
    engine
}

fn pool(symbols: &[&str]) -> Vec<FloatingMana> {
    symbols
        .iter()
        .map(|symbol| FloatingMana {
            symbol: symbol.to_string(),
            spend_restriction: None,
        })
        .collect()
}

fn keyword_rule(ability: Value) -> Value {
    json!({ "kind": "keywordAbility", "source": { "kind": "self" }, "ability": ability })
}

/// `Escape—{R}, Exile two other cards from your graveyard.` (CR 702.138a)
#[test]
fn escape_casts_from_the_graveyard_by_exiling_exactly_its_count_of_other_cards() {
    let mut engine = test_engine();
    engine.state.players[0].mana_pool = pool(&["R"]);
    let mut escaping = test_definition("escaping-card", "Creature");
    escaping.mana_cost = "{4}{R}".to_string();
    escaping.rules.push(keyword_rule(json!({
        "kind": "escape",
        "costs": [
            { "kind": "payMana", "manaCost": "{R}" },
            {
                "kind": "exileGraveyardCards",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "where": null,
                "excludeSource": true,
                "quantity": { "kind": "exactly", "count": { "kind": "integer", "value": 2 } },
                "decisionId": "escapeExileCards",
            },
        ],
    })));
    assert!(escape_ability(&escaping).is_some());
    engine.state.players[0]
        .graveyard
        .push(test_instance("escaping-card", escaping, "player-0"));
    for id in ["fuel-a", "fuel-b", "fuel-c"] {
        engine.state.players[0].graveyard.push(test_instance(
            id,
            test_definition(id, "Instant"),
            "player-0",
        ));
    }
    let escapes = engine
        .legal_priority_actions(0)
        .into_iter()
        .filter(|action| {
            action.card_instance_id.as_deref() == Some("escaping-card")
                && action.decisions.get("useEscape").and_then(Value::as_bool) == Some(true)
        })
        .collect::<Vec<_>>();
    assert_eq!(escapes.len(), 3, "each pair of the three other cards");
    assert!(escapes.iter().all(|action| {
        action.decisions["escapeExileCards"]
            .as_array()
            .is_some_and(|cards| cards.len() == 2)
    }));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .apply_priority_action(&escapes[0], &mut FirstOption)
        .expect("escape casts the card and exiles its two payment cards");
    assert_eq!(engine.state.players[0].exile.len(), 2);
    assert_eq!(engine.state.players[0].graveyard.len(), 1);
    assert_eq!(
        engine
            .state
            .stack
            .last()
            .map(|object| object.card.instance_id.as_str()),
        Some("escaping-card")
    );
}

/// Spectacle: an alternative mana cost from hand available only after an
/// opponent lost life this turn (CR 702.137a).
#[test]
fn spectacle_alternative_cost_waits_for_an_opponent_life_loss() {
    let mut engine = test_engine();
    engine.state.players[0].mana_pool = pool(&["R"]);
    let mut card = test_definition("spectacle-creature", "Creature");
    card.mana_cost = "{3}{R}{R}".to_string();
    card.rules.push(keyword_rule(json!({
        "kind": "alternativeCost",
        "mode": "spectacle",
        "costs": [{ "kind": "payMana", "manaCost": "{R}" }],
        "condition": {
            "kind": "compare",
            "operator": ">=",
            "left": {
                "kind": "countEventsThisTurn",
                "event": "lifeLost",
                "player": {
                    "kind": "opponentsOf",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                },
                "where": null,
            },
            "right": { "kind": "integer", "value": 1 },
        },
    })));
    assert!(keyword_rule_supported(&card.rules[0]));
    engine.state.players[0]
        .hand
        .push(test_instance("spectacle-creature", card, "player-0"));
    let alternative = |engine: &GameEngine| {
        engine.legal_priority_actions(0).into_iter().find(|action| {
            action.card_instance_id.as_deref() == Some("spectacle-creature")
                && action
                    .decisions
                    .get("useAlternativeCost")
                    .and_then(Value::as_bool)
                    == Some(true)
        })
    };
    assert!(alternative(&engine).is_none(), "no opponent lost life yet");
    engine.record_event(
        "lifeLost",
        Some("player-1".to_string()),
        None,
        json!({ "amount": 2 }),
    );
    let action = alternative(&engine).expect("spectacle is available after the life loss");
    assert_eq!(action.decisions["alternativeCostMode"], "spectacle");
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .apply_priority_action(&action, &mut FirstOption)
        .expect("the spectacle cost pays for the spell");
    assert!(engine.state.players[0].mana_pool.is_empty());
}

/// `Kicker—Sacrifice an artifact.`: a kicked cast declares and sacrifices
/// the artifact as it is cast (CR 702.33a, 601.2h).
#[test]
fn nonmana_kicker_declares_and_pays_its_sacrifice() {
    let mut engine = test_engine();
    engine.state.players[0].mana_pool = pool(&["C"]);
    let mut card = test_definition("kicker-creature", "Creature");
    card.mana_cost = "{1}".to_string();
    card.rules.push(keyword_rule(json!({
        "kind": "kicker",
        "cost": {
            "kind": "sacrificePermanent",
            "where": { "kind": "cardTypeContains", "value": "Artifact" },
        },
        "repeatable": false,
    })));
    assert!(keyword_rule_supported(&card.rules[0]));
    engine.state.players[0]
        .hand
        .push(test_instance("kicker-creature", card, "player-0"));
    engine.state.players[0].battlefield.push(test_instance(
        "fodder",
        test_definition("fodder", "Artifact"),
        "player-0",
    ));
    let kicked = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.card_instance_id.as_deref() == Some("kicker-creature")
                && action.decisions.get("kicked").and_then(Value::as_bool) == Some(true)
        })
        .expect("the artifact can pay the kicker cost");
    assert_eq!(
        kicked.targets.get("kickerCost"),
        Some(&TargetRef::Permanent {
            instance_id: "fodder".to_string()
        })
    );
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .apply_priority_action(&kicked, &mut FirstOption)
        .expect("the kicked spell is cast");
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "fodder")
    );
    let spell = engine.state.stack.last().expect("the kicked spell");
    assert!(!spell.targets.contains_key("kickerCost"));
}

/// Disguise (CR 702.168a): the face-down 2/2 has ward {2}; morph's does not.
#[test]
fn disguise_face_down_creature_has_ward_two() {
    let mut disguised = test_definition("disguised", "Creature");
    disguised.rules.push(keyword_rule(json!({
        "kind": "disguise",
        "cost": { "kind": "payMana", "manaCost": "{1}{U}" },
        "putCounterWhenTurnedFaceUp": false,
    })));
    assert!(morph_ability(&disguised).is_some());
    assert_eq!(
        ward_costs(face_down_creature_definition(&disguised).printed_rules()),
        vec![json!({ "kind": "payMana", "manaCost": "{2}" })]
    );
    let mut morph = disguised.clone();
    morph.rules[0]["ability"]["kind"] = json!("morph");
    assert!(morph_ability(&morph).is_some());
    assert!(ward_costs(face_down_creature_definition(&morph).printed_rules()).is_empty());
}

/// `Choose one — … Entwine {1}`: a declaration choosing every mode is marked
/// entwined and adds the entwine cost (CR 702.42a).
#[test]
fn entwine_declares_every_mode_and_adds_its_cost() {
    let mut engine = test_engine();
    engine.state.players[0].mana_pool = pool(&["C", "C"]);
    let mut card = test_definition("entwine-spell", "Instant");
    card.mana_cost = "{1}".to_string();
    let gain = |amount: i64| {
        json!([{
            "kind": "gainLife",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "amount": { "kind": "integer", "value": amount },
        }])
    };
    card.rules.push(json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": { "kind": "castingDeclaration", "decisions": [{
            "id": "chosenModes", "kind": "chooseModes", "minimum": 1,
            "maximum": { "kind": "integer", "value": 1 },
            "options": ["mode-1", "mode-2"], "allowRepeated": false,
        }] },
        "effects": [
            {
                "kind": "conditional",
                "condition": {
                    "kind": "selectionContains",
                    "selection": { "kind": "decisionResult", "decisionId": "chosenModes" },
                    "value": "mode-1",
                },
                "then": gain(1),
            },
            {
                "kind": "conditional",
                "condition": {
                    "kind": "selectionContains",
                    "selection": { "kind": "decisionResult", "decisionId": "chosenModes" },
                    "value": "mode-2",
                },
                "then": gain(2),
            },
        ],
    }));
    card.rules.push(keyword_rule(json!({
        "kind": "entwine",
        "cost": { "kind": "payMana", "manaCost": "{1}" },
    })));
    assert!(keyword_rule_supported(&card.rules[1]));
    engine.state.players[0]
        .hand
        .push(test_instance("entwine-spell", card.clone(), "player-0"));
    let entwined = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.card_instance_id.as_deref() == Some("entwine-spell")
                && action.decisions.get("entwined").and_then(Value::as_bool) == Some(true)
        })
        .expect("two mana pay for the spell and its entwine cost");
    assert_eq!(
        entwined.decisions["chosenModes"],
        json!(["mode-1", "mode-2"])
    );
    assert_eq!(
        engine.casting_additional_mana_costs(&card, "player-0", &entwined.decisions),
        vec!["{1}".to_string()]
    );
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .apply_priority_action(&entwined, &mut FirstOption)
        .expect("the entwined spell is cast");
    engine
        .resolve_top_stack(&mut FirstOption)
        .expect("the entwined spell resolves");
    assert_eq!(engine.state.players[0].life, 23, "both modes resolved");
}

/// `Hexproof from black` stops black sources of opponents only (CR 702.11d).
#[test]
fn printed_hexproof_from_a_quality_stops_only_opponents_matching_sources() {
    let engine = test_engine();
    let mut knight = test_definition("knight", "Creature");
    knight.rules.push(keyword_rule(json!({
        "kind": "hexproofFrom",
        "from": ["black"],
    })));
    assert!(keyword_rule_supported(&knight.rules[0]));
    let permanent = test_instance("knight", knight, "player-0");
    let mut black = test_definition("black-spell", "Instant");
    black.mana_cost = "{B}".to_string();
    let mut white = test_definition("white-spell", "Instant");
    white.mana_cost = "{W}".to_string();
    assert!(!engine.permanent_is_legal_target_for_source(&permanent, "player-1", &black));
    assert!(engine.permanent_is_legal_target_for_source(&permanent, "player-1", &white));
    assert!(engine.permanent_is_legal_target_for_source(&permanent, "player-0", &black));
}
