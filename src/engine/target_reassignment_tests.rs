use super::*;

fn definition(id: &str, kind: &str) -> CardDefinition {
    CardDefinition {
        id: id.into(),
        name: id.into(),
        type_line: kind.into(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{0}".into(),
        power: Some("2".into()),
        toughness: Some("2".into()),
        rules: vec![],
    }
}
fn setup() -> GameEngine {
    let filler = definition("filler", "Basic Land - Plains");
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("p{i}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 7,
            starting_player: 0,
        },
        401,
    )
    .unwrap()
}
fn creature(engine: &mut GameEngine, player: usize, id: &str) -> TargetRef {
    let owner = format!("p{player}");
    let mut card = card_instance(&owner, &definition(id, "Creature - Bear"), player + 1);
    card.instance_id = id.into();
    GameEngine::assign_tracked_zone_identity(&mut card, &format!("battlefield:original:{id}"));
    engine.state.players[player].battlefield.push(card);
    TargetRef::Permanent {
        instance_id: id.into(),
    }
}
fn stack(
    engine: &GameEngine,
    decisions: Vec<Value>,
    targets: Vec<(&str, TargetRef)>,
) -> StackObject {
    let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"declaration":{"kind":"castingDeclaration","decisions":decisions},"distinctTargets":[["a","b"]],"effects":[]});
    let mut def = definition("original-spell", "Instant");
    def.rules = vec![rule];
    let mut result = StackObject {
        id: "stack:original".into(),
        controller: "p0".into(),
        card: card_instance("p0", &def, 100),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: targets
            .into_iter()
            .map(|(id, target)| (id.into(), target))
            .collect(),
    };
    for decision in &def.rules[0]["declaration"]["decisions"]
        .as_array()
        .unwrap()
        .clone()
    {
        let id = decision["id"].as_str().unwrap();
        let mut selected = result
            .targets
            .iter()
            .filter_map(|(key, target)| {
                if key == id {
                    Some((0, target.clone()))
                } else {
                    key.strip_prefix(&format!("{id}:"))
                        .and_then(|n| n.parse::<usize>().ok())
                        .map(|n| (n, target.clone()))
                }
            })
            .collect::<Vec<_>>();
        selected.sort_by_key(|(n, _)| *n);
        result.decisions.insert(
            id.into(),
            json!(
                selected
                    .into_iter()
                    .map(|(_, target)| target)
                    .collect::<Vec<_>>()
            ),
        );
    }
    for (id, target) in &result.targets {
        engine
            .capture_target_incarnation(&mut result.decisions, id, target)
            .expect("test target must have an admitted incarnation");
    }
    result
}
fn decision(id: &str) -> Value {
    json!({"id":id,"kind":"chooseTargets","minimum":1,"maximum":1,"candidates":{"kind":"permanents","where":{"kind":"cardTypeContains","value":"Creature"}}})
}
fn prospective(original: &StackObject, prepared: &PreparedTargetReassignment) -> StackObject {
    let mut result = original.clone();
    result.targets = prepared.targets.clone();
    result.decisions = prepared.decisions.clone();
    result
}

#[test]
fn final_joint_swap_preserves_old_identities_and_target_decision_values() {
    let mut engine = setup();
    let a = creature(&mut engine, 0, "alpha");
    let b = creature(&mut engine, 1, "beta");
    let original = stack(
        &engine,
        vec![decision("a"), decision("b")],
        vec![("a", a.clone()), ("b", b.clone())],
    );
    let choices = engine.prepare_joint_target_assignments(&original).unwrap();
    let swapped = choices
        .iter()
        .find(|c| c.targets["a"] == b && c.targets["b"] == a)
        .expect("joint swap must survive intermediate distinctness violations");
    assert_eq!(swapped.decisions["a"], json!([b]));
    assert_eq!(swapped.decisions["b"], json!([a]));
    assert!(
        engine
            .newly_targeted_after_joint_reassignment(&original, &prospective(&original, swapped))
            .unwrap()
            .is_empty(),
        "already-targeted objects do not trigger Ward again merely by changing slots"
    );
    let retained = &choices[0];
    assert!(retained.replaced.is_empty());
    assert_eq!(retained.decisions, original.decisions);
}

#[test]
fn retained_blinked_target_keeps_capture_and_replace_same_id_targets_new_incarnation() {
    let mut engine = setup();
    let a = creature(&mut engine, 0, "alpha");
    let original = stack(&engine, vec![decision("a")], vec![("a", a.clone())]);
    let live = &mut engine.state.players[0].battlefield[0];
    GameEngine::assign_tracked_zone_identity(live, "battlefield:returned:alpha");
    let choices = engine.prepare_joint_target_assignments(&original).unwrap();
    let retained = choices.iter().find(|c| c.replaced.is_empty()).unwrap();
    assert_eq!(retained.decisions, original.decisions);
    let replaced = choices
        .iter()
        .find(|c| c.replaced == ["a"])
        .expect("same physical id may name a new object");
    assert_eq!(retained.targets, replaced.targets);
    assert_ne!(retained.decisions, replaced.decisions);
    assert!(
        engine
            .newly_targeted_after_joint_reassignment(&original, &prospective(&original, retained))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        engine
            .newly_targeted_after_joint_reassignment(&original, &prospective(&original, replaced))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn ordinal_target_order_and_original_distribution_are_preserved() {
    let mut engine = setup();
    let a = creature(&mut engine, 0, "alpha");
    let b = creature(&mut engine, 1, "beta");
    let mut d = decision("many");
    d["minimum"] = json!(2);
    d["maximum"] = json!(2);
    let mut original = stack(
        &engine,
        vec![d],
        vec![("many:2", a.clone()), ("many:10", b.clone())],
    );
    original
        .decisions
        .insert("division".into(), json!({"many:2":2,"many:10":1}));
    let choices = engine.prepare_joint_target_assignments(&original).unwrap();
    let swapped = choices
        .iter()
        .find(|c| c.targets["many:2"] == b && c.targets["many:10"] == a)
        .unwrap();
    assert_eq!(swapped.target_order, ["many:2", "many:10"]);
    assert_eq!(swapped.decisions["many"], json!([b, a]));
    assert_eq!(
        swapped.decisions["division"],
        original.decisions["division"]
    );
}

#[test]
fn player_bound_controller_is_not_the_caster_used_for_hexproof() {
    let mut engine = setup();
    let own = creature(&mut engine, 0, "own");
    let other = creature(&mut engine, 1, "other");
    let protected = creature(&mut engine, 1, "protected");
    engine.state.players[1].battlefield[1]
        .definition
        .rules
        .push(json!({"kind":"keywordAbility","ability":{"kind":"hexproof"}}));
    let player = json!({"kind":"chooseTargets","id":"player","minimum":1,"maximum":2,"candidates":{"kind":"players"}});
    let mut permanent = decision("creature");
    permanent["candidates"]["controller"] =
        json!({"kind":"controllerOf","object":{"kind":"chosenTarget","id":"player:0"}});
    let original = stack(
        &engine,
        vec![player, permanent],
        vec![
            (
                "player:0",
                TargetRef::Player {
                    player_id: "p1".into(),
                },
            ),
            ("creature", other.clone()),
        ],
    );
    let choices = engine.prepare_joint_target_assignments(&original).unwrap();
    assert!(choices.iter().any(|c| c.targets["player:0"]
        == TargetRef::Player {
            player_id: "p0".into()
        }
        && c.targets["creature"] == own));
    assert!(
        !choices.iter().any(|c| c.targets["creature"] == protected),
        "bound controller p1 does not turn opponent hexproof into friendly hexproof for caster p0"
    );
    assert!(
        !choices.iter().any(|c| c.targets["player:0"]
            == TargetRef::Player {
                player_id: "p0".into()
            }
            && c.targets["creature"] == other),
        "changing player must not invalidate the retained dependent creature target"
    );
}

#[test]
fn missing_authentication_nonbattlefield_cards_and_forward_dependencies_fail_explicitly() {
    let mut engine = setup();
    let a = creature(&mut engine, 0, "alpha");
    let original = stack(&engine, vec![decision("a")], vec![("a", a)]);
    let mut missing = original.clone();
    missing
        .decisions
        .remove("__deepdeckTargetBattlefieldIdentitiesV1");
    assert!(
        engine
            .prepare_joint_target_assignments(&missing)
            .unwrap_err()
            .message
            .contains("authenticated")
    );
    let mut card = original.clone();
    card.targets.insert(
        "a".into(),
        TargetRef::Card {
            instance_id: "grave-card".into(),
        },
    );
    assert!(
        engine
            .prepare_joint_target_assignments(&card)
            .unwrap_err()
            .message
            .contains("nonbattlefield")
    );
    let mut forward = original;
    forward.card.definition.rules[0]["declaration"]["decisions"][0]["candidates"]["controller"] =
        json!({"kind":"controllerOf","object":{"kind":"chosenTarget","id":"later"}});
    forward.card.definition.rules[0]["declaration"]["decisions"]
        .as_array_mut()
        .unwrap()
        .push(decision("later"));
    assert!(
        engine
            .prepare_joint_target_assignments(&forward)
            .unwrap_err()
            .message
            .contains("forward or cyclic")
    );
    let mut empty_source = missing.clone();
    empty_source.decisions = stack(
        &engine,
        vec![decision("a")],
        vec![(
            "a",
            TargetRef::Permanent {
                instance_id: "alpha".into(),
            },
        )],
    )
    .decisions;
    empty_source.card = card_instance("p0", &definition("source", "Artifact"), 99);
    GameEngine::assign_tracked_zone_identity(&mut empty_source.card, "");
    engine.state.players[0]
        .battlefield
        .push(empty_source.card.clone());
    empty_source.card.definition.rules = forward.card.definition.rules.clone();
    empty_source.card.definition.rules[0]["declaration"]["decisions"] = json!([decision("a")]);
    empty_source.card.definition.rules[0]["declaration"]["decisions"][0]["candidates"]["excludeSource"] =
        json!(true);
    assert!(
        engine
            .prepare_joint_target_assignments(&empty_source)
            .unwrap_err()
            .message
            .contains("authenticated live source")
    );
}

#[test]
fn incomplete_joint_enumeration_is_an_error_never_a_truncated_option_list() {
    let mut engine = setup();
    let mut creatures = Vec::new();
    for i in 0..7 {
        creatures.push(creature(&mut engine, 0, &format!("creature{i}")));
    }
    let ids = ["x0", "x1", "x2", "x3", "x4"];
    let original = stack(
        &engine,
        ids.iter().map(|id| decision(id)).collect(),
        ids.iter()
            .enumerate()
            .map(|(i, id)| (*id, creatures[i].clone()))
            .collect(),
    );
    assert!(
        engine
            .prepare_joint_target_assignments(&original)
            .unwrap_err()
            .message
            .contains("exploration limit")
    );
}
