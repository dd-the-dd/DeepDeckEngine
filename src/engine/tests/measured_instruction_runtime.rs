//! Real Oracle text -> primitive IR -> engine behavior for measured action
//! outcomes (CR 608.2c, 603.12): an instruction group is performed only when
//! its choices chose what it asks for and its actions happened; a reflexive
//! `this way` ability triggers only on the objects its instruction affected;
//! a card move to exile is the result an `exiled this way` phrase restates.
use super::*;

fn parsed_rules(card_name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{3}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    assert_eq!(parsed.status, "canonical", "{oracle}");
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn triggered_rule(card_name: &str, type_line: &str, oracle: &str) -> Value {
    parsed_rules(card_name, type_line, oracle)
        .into_iter()
        .find(|rule| rule["kind"] == "triggeredAbility")
        .expect("a triggered ability")
}

fn cards(prefix: &str, type_lines: &[&str]) -> Vec<CardInstance> {
    type_lines
        .iter()
        .enumerate()
        .map(|(index, type_line)| {
            let id = format!("{prefix}-{index}");
            let mut definition = test_definition(&id, type_line);
            definition.mana_cost = "{2}".to_string();
            test_instance(&id, definition, "player-0")
        })
        .collect()
}

/// An engine whose first player holds exactly the given zones, with the
/// ability's source on the battlefield.
fn engine_with(
    source_type_line: &str,
    hand: &[&str],
    library: &[&str],
    graveyard: &[&str],
) -> (GameEngine, CardInstance) {
    let mut engine = test_engine(2);
    let source = test_instance(
        "outcome-source",
        test_definition("outcome-source", source_type_line),
        "player-0",
    );
    let player = &mut engine.state.players[0];
    player.hand = cards("hand", hand);
    player.library = cards("library", library);
    player.graveyard = cards("graveyard", graveyard);
    player.battlefield = vec![source.clone()];
    engine.state.players[1].hand.clear();
    (engine, source)
}

fn ability_stack(source: &CardInstance, rule: &Value) -> StackObject {
    StackObject {
        id: "stack:outcome-ability".to_string(),
        controller: "player-0".to_string(),
        card: source.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn resolve(engine: &mut GameEngine, source: &CardInstance, rule: &Value, effects: &Value) {
    execute_effects_at_resolution_start(
        engine,
        effects,
        &ability_stack(source, rule),
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the measured outcome resolves");
}

fn controller_ref() -> Value {
    json!({"kind": "controllerOf", "object": {"kind": "self"}})
}

fn life(engine: &GameEngine) -> i32 {
    engine.state.players[0].life
}

const PRIEST: &str = "At the beginning of your upkeep, you may reveal a Dinosaur card from your hand. If you do, you gain 2 life.";

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_reveal_group_is_performed_only_when_a_card_was_revealed() {
    // Priest of the Wakening Sun: choosing to reveal without a Dinosaur in
    // hand chooses no card, so the instruction was not performed.
    for (hand, expected) in [
        (&["Creature - Dinosaur"][..], 22),
        (&["Creature - Elf"][..], 20),
    ] {
        let rule = triggered_rule(
            "Priest of the Wakening Sun",
            "Creature - Human Cleric",
            PRIEST,
        );
        assert!(rule["effects"][0].get("actions").is_some(), "{rule:#}");
        let (mut engine, source) = engine_with("Creature - Human Cleric", hand, &[], &[]);
        resolve(&mut engine, &source, &rule, &rule["effects"]);
        assert_eq!(life(&engine), expected, "hand {hand:?}");
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_clamped_choice_asks_for_its_written_count() {
    // `exile two cards from your graveyard. If you can't` (CR 609.3): a
    // single card in the graveyard is exiled, but the instruction asked for
    // two, so the declined outcome runs.
    let effects = json!([{
        "kind": "performAction",
        "actions": [
            {
                "kind": "chooseCards",
                "id": "chosenZoneCards",
                "player": controller_ref(),
                "from": {"kind": "cardsInZone", "where": null,
                    "zone": {"kind": "graveyard", "player": controller_ref()}},
                "count": {"kind": "minimum", "operands": [
                    {"kind": "integer", "value": 2},
                    {"kind": "countObjects", "objects": {"kind": "cardsInZone", "where": null,
                        "zone": {"kind": "graveyard", "player": controller_ref()}}},
                ]},
            },
            {
                "kind": "moveCards",
                "cards": {"kind": "decisionResult", "decisionId": "chosenZoneCards"},
                "to": {"kind": "exile", "faceDown": false,
                    "player": {"kind": "ownerOf", "object": {"kind": "candidate"}}},
            },
        ],
        "onPerformed": [],
        "onDeclined": [{"kind": "loseLife", "player": controller_ref(),
            "amount": {"kind": "integer", "value": 3}}],
    }]);
    let rule = json!({"kind": "triggeredAbility", "source": {"kind": "self"}, "effects": effects});
    assert!(effects.as_array().unwrap().iter().all(effect_supported));
    for (graveyard, expected_life, expected_exiled) in [
        (&["Sorcery"][..], 17, 1),
        (&["Sorcery", "Instant"][..], 20, 2),
    ] {
        let (mut engine, source) = engine_with("Creature - Wizard", &[], &[], graveyard);
        resolve(&mut engine, &source, &rule, &effects);
        assert_eq!(life(&engine), expected_life, "graveyard {graveyard:?}");
        assert_eq!(engine.state.players[0].exile.len(), expected_exiled);
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_bound_amount_is_measured_after_the_group_binds_it() {
    // `draw X cards, where X is ...`: the draw reads the X its own group
    // bound; drawing fewer than X is not performing it (CR 121.1).
    let effects = json!([{
        "kind": "performAction",
        "actions": [
            {"kind": "bind", "id": "whereX", "value": {"kind": "integer", "value": 2}},
            {"kind": "drawCards", "player": controller_ref(),
                "count": {"kind": "boundValue", "id": "whereX"}},
        ],
        "onPerformed": [{"kind": "gainLife", "player": controller_ref(),
            "amount": {"kind": "integer", "value": 5}}],
    }]);
    let rule = json!({"kind": "triggeredAbility", "source": {"kind": "self"}, "effects": effects});
    for (library, expected) in [
        (&["Sorcery"][..], 20),
        (&["Sorcery", "Instant", "Land"][..], 25),
    ] {
        let (mut engine, source) = engine_with("Creature - Wizard", &[], library, &[]);
        resolve(&mut engine, &source, &rule, &effects);
        assert_eq!(life(&engine), expected, "library {library:?}");
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_disjunction_is_performed_by_the_chosen_option() {
    // `you may sacrifice an artifact or discard a card. If you do`: the
    // performer picks the discard; an empty hand discards nothing.
    let effects = json!([{
        "kind": "optionalAction",
        "player": controller_ref(),
        "action": {
            "kind": "chooseOneEffect",
            "player": controller_ref(),
            "options": [
                {"label": "alternative 1", "effects": [{"kind": "sacrificePermanents",
                    "player": controller_ref(), "count": {"kind": "integer", "value": 1},
                    "where": {"kind": "cardTypeContains", "value": "Artifact"}}]},
                {"label": "alternative 2", "effects": [{"kind": "discardCards",
                    "player": controller_ref(), "count": {"kind": "integer", "value": 1}}]},
            ],
        },
        "onPerformed": [{"kind": "gainLife", "player": controller_ref(),
            "amount": {"kind": "integer", "value": 3}}],
    }]);
    let rule = json!({"kind": "triggeredAbility", "source": {"kind": "self"}, "effects": effects});
    assert!(effects.as_array().unwrap().iter().all(effect_supported));
    for (hand, expected) in [(&["Instant"][..], 23), (&[][..], 20)] {
        let (mut engine, source) = engine_with("Creature - Devil", hand, &[], &[]);
        resolve(&mut engine, &source, &rule, &effects);
        assert_eq!(life(&engine), expected, "hand {hand:?}");
    }
}

const TEO: &str = "Whenever one or more creatures you control with flying attack, draw a card, then discard a card. When you discard a nonland card this way, put a +1/+1 counter on target creature you control.";

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_this_way_reflexive_ability_triggers_only_on_a_matching_result() {
    // CR 603.12: the reflexive ability exists only when the discarded card
    // is a nonland card; a discarded land creates nothing.
    for (drawn, triggers) in [("Sorcery", true), ("Basic Land - Island", false)] {
        let rule = triggered_rule(
            "Teo, Spirited Glider",
            "Legendary Creature - Human Pilot Ally",
            TEO,
        );
        let (mut engine, source) =
            engine_with("Legendary Creature - Human Pilot Ally", &[], &[drawn], &[]);
        resolve(&mut engine, &source, &rule, &rule["effects"]);
        assert!(
            engine.state.players[0].hand.is_empty(),
            "the drawn card was discarded"
        );
        assert_eq!(
            engine
                .state
                .events
                .iter()
                .any(|event| event.kind == "reflexiveTriggeredAbilityPutOnStack"),
            triggers,
            "discarded {drawn}"
        );
    }
}

const BONEHOARD: &str = "Flying, first strike\nAt the beginning of your upkeep, exile the top two cards of your library. You may play them this turn. If you exiled a land card this way, create a 3/1 red Dinosaur creature token. If you exiled a nonland card this way, create a Treasure token.";

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn exiled_this_way_restates_the_cards_moved_to_exile() {
    // Bonehoard Dracosaur: a Dinosaur only when a land was exiled, a
    // Treasure only when a nonland card was.
    for (library, dinosaurs, treasures) in [
        (&["Basic Land - Forest", "Basic Land - Swamp"][..], 1, 0),
        (&["Basic Land - Forest", "Sorcery"][..], 1, 1),
        (&["Instant", "Sorcery"][..], 0, 1),
    ] {
        let rule = triggered_rule(
            "Bonehoard Dracosaur",
            "Creature - Dinosaur Dragon",
            BONEHOARD,
        );
        let (mut engine, source) = engine_with("Creature - Dinosaur Dragon", &[], library, &[]);
        resolve(&mut engine, &source, &rule, &rule["effects"]);
        assert_eq!(engine.state.players[0].exile.len(), 2);
        let created = |subtype: &str| {
            engine.state.players[0]
                .battlefield
                .iter()
                .filter(|card| card.instance_id != source.instance_id)
                .filter(|card| card.definition.type_line.contains(subtype))
                .count()
        };
        assert_eq!(created("Dinosaur"), dinosaurs, "library {library:?}");
        assert_eq!(created("Treasure"), treasures, "library {library:?}");
    }
}
