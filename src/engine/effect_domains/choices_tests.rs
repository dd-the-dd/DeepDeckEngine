//! Runtime proofs of the resolution choices of characteristic values:
//! `the color or colors of your choice` (CR 105.1, 608.2d) feeding
//! `setColors`, the single-color form (Rainbow Crow), and `the basic land
//! type of your choice` (CR 205.3i) feeding a land subtype set.
use super::super::super::*;

/// Chooses the option whose recorded value for `decision` is `wanted`.
struct Choose {
    decision: &'static str,
    wanted: Value,
    offered: Vec<usize>,
}

impl DecisionProvider for Choose {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        self.offered.push(request.options.len());
        request
            .options
            .iter()
            .position(|option| option.decisions.get(self.decision) == Some(&self.wanted))
            .ok_or_else(|| EngineError::new("the wanted choice is not offered"))
    }
}

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: "{1}".to_string(),
        name: id.to_string(),
        power: Some("2".to_string()),
        rules: Vec::new(),
        toughness: Some("2".to_string()),
        type_line: type_line.to_string(),
    }
}

fn engine() -> GameEngine {
    let mut filler = definition("choice-filler", "Basic Land - Forest");
    filler.power = None;
    filler.toughness = None;
    GameEngine::new(
        GameSetup {
            opening_hand_size: 7,
            players: (0..2)
                .map(|index| PlayerDeck {
                    cards: vec![filler.clone(); 12],
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                })
                .collect(),
            starting_player: 0,
        },
        512,
    )
    .expect("choice test setup is valid")
}

fn add(engine: &mut GameEngine, id: &str, type_line: &str) -> String {
    let mut card = card_instance("player-0", &definition(id, type_line), 0);
    card.instance_id = id.to_string();
    engine.state.players[0].battlefield.push(card);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    id.to_string()
}

fn stack_for(engine: &GameEngine, source: &str, rule: &Value, target: Option<&str>) -> StackObject {
    let (player, index) = engine.permanent_position(source).expect("source exists");
    StackObject {
        id: format!("stack:{source}"),
        controller: "player-0".to_string(),
        card: engine.state.players[player].battlefield[index].clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: value_kind(rule).map(ToOwned::to_owned),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: target
            .map(|target| {
                BTreeMap::from([(
                    "targetPermanent".to_string(),
                    TargetRef::Permanent {
                        instance_id: target.to_string(),
                    },
                )])
            })
            .unwrap_or_default(),
    }
}

/// One declared permanent target of the given card type.
fn target_declaration(card_type: &str) -> Value {
    json!({
        "kind": "chooseTargets",
        "id": "targetPermanent",
        "candidates": {
            "kind": "permanents",
            "where": { "kind": "cardTypeContains", "value": card_type },
        },
        "minimum": 1,
        "maximum": 1,
    })
}

/// `becomes the color(s) of your choice` as the Oracle compiler emits it.
fn color_choice_rule(maximum: i64, object: Value) -> Value {
    let decisions = if value_kind(&object) == Some("chosenTarget") {
        vec![target_declaration("Creature")]
    } else {
        Vec::new()
    };
    json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "payMana", "manaCost": "{1}" }],
        "declaration": { "kind": "castingDeclaration", "decisions": decisions },
        "effects": [
            {
                "kind": "chooseColors",
                "id": "characteristicObjects0:chosenColors",
                "minimum": { "kind": "integer", "value": 1 },
                "maximum": { "kind": "integer", "value": maximum },
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            },
            {
                "kind": "setColors",
                "colors": {
                    "kind": "decisionResult",
                    "decisionId": "characteristicObjects0:chosenColors",
                },
                "object": object,
                "duration": { "kind": "untilEndOfCurrentTurn" },
            },
        ],
    })
}

fn resolve(engine: &mut GameEngine, rule: &Value, stack: &StackObject, provider: &mut Choose) {
    engine
        .execute_effects(
            &rule["effects"],
            stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            provider,
        )
        .expect("the choice and the color change resolve");
}

#[test]
fn a_target_becomes_the_two_colors_its_controller_chose() {
    // Quickchange / Scuttlemutt: `the color or colors of your choice`.
    let rule = color_choice_rule(
        5,
        json!({ "kind": "chosenTarget", "id": "targetPermanent" }),
    );
    assert!(rule_is_executable(&rule), "{rule}");
    let mut engine = engine();
    let source = add(&mut engine, "scuttlemutt", "Artifact Creature - Scarecrow");
    let target = add(&mut engine, "bear", "Creature - Bear");
    let stack = stack_for(&engine, &source, &rule, Some(target.as_str()));
    let mut provider = Choose {
        decision: "characteristicObjects0:chosenColors",
        wanted: json!(["U", "R"]),
        offered: Vec::new(),
    };
    resolve(&mut engine, &rule, &stack, &mut provider);
    // Every nonempty set of distinct colors is offered (2^5 - 1).
    assert_eq!(provider.offered, [31]);
    let (player, index) = engine.permanent_position(&target).unwrap();
    assert_eq!(
        super::super::marker_colors(&engine.state.players[player].battlefield[index].definition),
        Some(vec!["U".to_string(), "R".to_string()])
    );
    let (player, index) = engine.permanent_position(&source).unwrap();
    assert_eq!(
        super::super::marker_colors(&engine.state.players[player].battlefield[index].definition),
        None,
        "the source is not the object"
    );
}

#[test]
fn rainbow_crow_becomes_the_one_color_of_its_controllers_choice() {
    let rule = color_choice_rule(1, json!({ "kind": "self" }));
    assert!(rule_is_executable(&rule), "{rule}");
    let mut engine = engine();
    let crow = add(&mut engine, "rainbow-crow", "Creature - Bird");
    let stack = stack_for(&engine, &crow, &rule, None);
    let mut provider = Choose {
        decision: "characteristicObjects0:chosenColors",
        wanted: json!(["G"]),
        offered: Vec::new(),
    };
    resolve(&mut engine, &rule, &stack, &mut provider);
    assert_eq!(provider.offered, [5], "one color among five");
    let (player, index) = engine.permanent_position(&crow).unwrap();
    assert_eq!(
        super::super::marker_colors(&engine.state.players[player].battlefield[index].definition),
        Some(vec!["G".to_string()])
    );
}

#[test]
fn a_chosen_basic_land_type_sets_the_target_lands_subtype() {
    // Reef Shaman: `the basic land type of your choice until end of turn`.
    let rule = json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [target_declaration("Land")],
        },
        "effects": [
            {
                "kind": "chooseBasicLandType",
                "id": "characteristicObjects0:chosenSubtype",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            },
            {
                "kind": "modifySubtypeSet",
                "mode": "replace",
                "subtypeClass": "land",
                "subtypes": {
                    "kind": "decisionResult",
                    "decisionId": "characteristicObjects0:chosenSubtype",
                },
                "object": { "kind": "chosenTarget", "id": "targetPermanent" },
                "duration": { "kind": "untilEndOfCurrentTurn" },
            },
        ],
    });
    assert!(rule_is_executable(&rule), "{rule}");
    let mut engine = engine();
    let shaman = add(&mut engine, "reef-shaman", "Creature - Merfolk Shaman");
    let land = add(&mut engine, "forest", "Basic Land - Forest");
    let stack = stack_for(&engine, &shaman, &rule, Some(land.as_str()));
    let mut provider = Choose {
        decision: "characteristicObjects0:chosenSubtype",
        wanted: json!(["Swamp"]),
        offered: Vec::new(),
    };
    resolve(&mut engine, &rule, &stack, &mut provider);
    assert_eq!(provider.offered, [5]);
    let (player, index) = engine.permanent_position(&land).unwrap();
    let land = engine.state.players[player].battlefield[index].clone();
    let subtype = |value: &str| json!({ "kind": "subtypeContains", "value": value });
    assert!(engine.permanent_matches_effective_filter(&land, &subtype("Swamp")));
    assert!(!engine.permanent_matches_effective_filter(&land, &subtype("Forest")));
}

#[test]
fn color_choices_with_impossible_or_computed_bounds_stay_unsupported() {
    let choice = |minimum: Value, maximum: Value| {
        json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [{
                "kind": "chooseColors",
                "id": "chosen",
                "minimum": minimum,
                "maximum": maximum,
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            }],
        })
    };
    let integer = |value: i64| json!({ "kind": "integer", "value": value });
    assert!(rule_is_executable(&choice(integer(1), integer(5))));
    assert!(!rule_is_executable(&choice(integer(0), integer(5))));
    assert!(!rule_is_executable(&choice(integer(1), integer(6))));
    assert!(!rule_is_executable(&choice(integer(3), integer(2))));
    assert!(!rule_is_executable(&choice(
        integer(1),
        json!({ "kind": "countPermanents", "where": { "kind": "cardTypeContains", "value": "Creature" } }),
    )));
}
