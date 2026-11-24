//! Runtime proofs of the object relations of the shared query
//! (`object_relation_eval.rs`): an attachment read from a declared target,
//! a count of the creatures blocking the source, a shared characteristic
//! prepared from its anchor, and the counted library-top operand.
use super::*;

fn parsed_rules(name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}".to_string()),
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
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:#?}",
        parsed.abilities
    );
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn stack_source(card: CardInstance, controller: &str, kind: &str) -> StackObject {
    StackObject {
        id: format!("stack:{}", card.instance_id),
        controller: controller.to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some(kind.to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn permanent(id: &str, type_line: &str, owner: &str, attached_to: Option<&str>) -> CardInstance {
    let mut instance = test_instance(id, test_definition(id, type_line), owner);
    instance.attached_to = attached_to.map(ToOwned::to_owned);
    instance
}

fn battlefield_ids(engine: &GameEngine, player: usize) -> Vec<String> {
    engine.state.players[player]
        .battlefield
        .iter()
        .map(|permanent| permanent.instance_id.clone())
        .collect()
}

#[test]
fn only_the_equipment_attached_to_the_declared_target_is_destroyed() {
    // Blastfire Bolt: the relation's anchor is the creature the damage
    // targeted (CR 608.2b); an Equipment on another creature and an Aura on
    // the target are not members of the collection.
    let rules = parsed_rules(
        "Blastfire Bolt",
        "Instant",
        "Blastfire Bolt deals 5 damage to target creature. Destroy all Equipment attached to that creature.",
    );
    let mut engine = test_engine(2);
    let mut giant = permanent("giant", "Creature \u{2014} Giant", "player-1", None);
    giant.definition.toughness = Some("9".to_string());
    engine.state.players[1].battlefield = vec![
        giant,
        permanent("bear", "Creature \u{2014} Bear", "player-1", None),
        permanent(
            "sword",
            "Artifact \u{2014} Equipment",
            "player-1",
            Some("giant"),
        ),
        permanent(
            "plate",
            "Artifact \u{2014} Equipment",
            "player-1",
            Some("bear"),
        ),
        permanent(
            "halo",
            "Enchantment \u{2014} Aura",
            "player-1",
            Some("giant"),
        ),
    ];
    let mut source = stack_source(
        test_instance("bolt", test_definition("bolt", "Instant"), "player-0"),
        "player-0",
        "spellAbility",
    );
    source.targets.insert(
        "targetPermanent".to_string(),
        TargetRef::Permanent {
            instance_id: "giant".to_string(),
        },
    );
    execute_effects_at_resolution_start(
        &mut engine,
        &rules[0]["effects"],
        &source,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the program resolves");
    assert_eq!(
        battlefield_ids(&engine, 1),
        vec!["giant", "bear", "plate", "halo"]
    );
}

#[test]
fn the_creatures_blocking_the_source_scale_its_bonus() {
    // Elvish Berserker: `for each creature blocking it` counts the block
    // pairs of the current combat whose attacker is the source (CR 509.1).
    let rules = parsed_rules(
        "Elvish Berserker",
        "Creature \u{2014} Elf Berserker",
        "Whenever this creature becomes blocked, it gets +1/+1 until end of turn for each creature blocking it.",
    );
    let rule = rules
        .iter()
        .find(|rule| rule.get("effects").is_some())
        .expect("the triggered ability has effects");
    let mut engine = test_engine(2);
    let berserker = permanent(
        "berserker",
        "Creature \u{2014} Elf Berserker",
        "player-0",
        None,
    );
    engine.state.players[0].battlefield = vec![berserker.clone()];
    engine.state.players[1].battlefield = vec![
        permanent("wall-a", "Creature \u{2014} Wall", "player-1", None),
        permanent("wall-b", "Creature \u{2014} Wall", "player-1", None),
        permanent("idle", "Creature \u{2014} Bear", "player-1", None),
    ];
    engine.state.combat.attackers = vec![DeclaredAttacker {
        attacker_id: "berserker".to_string(),
        defender: TargetRef::Player {
            player_id: "player-1".to_string(),
        },
        defending_player_id: "player-1".to_string(),
    }];
    engine.state.combat.blockers = ["wall-a", "wall-b"]
        .into_iter()
        .map(|blocker| DeclaredBlocker {
            attacker_id: "berserker".to_string(),
            blocker_id: blocker.to_string(),
        })
        .collect();
    let source = stack_source(berserker, "player-0", "triggeredAbility");
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &source,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the trigger resolves");
    let pumped = &engine.state.players[0].battlefield[0];
    assert_eq!(engine.current_power(pumped), 4, "2 printed + 2 blockers");
}

#[test]
fn a_prepared_share_reads_its_anchor_and_a_missing_anchor_is_an_error() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        permanent("lord", "Creature \u{2014} Elf Warrior", "player-0", None),
        permanent("scout", "Creature \u{2014} Elf Scout", "player-0", None),
        permanent("goblin", "Creature \u{2014} Goblin", "player-0", None),
        permanent("relic", "Artifact", "player-0", None),
    ];
    let lord = engine.state.players[0].battlefield[0].clone();
    let source = stack_source(lord, "player-0", "activatedAbility");
    let atom = json!({
        "kind": "sharesCharacteristic",
        "characteristic": "creatureType",
        "object": { "kind": "self" },
    });
    assert!(super::super::object_relation_eval::relation_atom_supported(
        &atom
    ));
    let prepared = engine
        .prepare_object_relation(&atom, &source, &BTreeMap::new(), &BTreeMap::new())
        .expect("the source anchor is read");
    let shares = |id: &str| {
        let candidate = engine.state.players[0]
            .battlefield
            .iter()
            .find(|permanent| permanent.instance_id == id)
            .expect("a candidate");
        engine.eval_object_relation(
            &prepared,
            super::super::runtime_object_filter::ObjectFilterView::Permanent(candidate),
        )
    };
    assert!(shares("scout"), "Elf in common");
    assert!(!shares("goblin"));
    assert!(!shares("relic"), "a noncreature has no creature type");
    // An unprepared atom never matches.
    let scout = engine.state.players[0].battlefield[1].clone();
    assert!(!engine.eval_object_relation(
        &atom,
        super::super::runtime_object_filter::ObjectFilterView::Permanent(&scout)
    ));
    // A bound anchor with no object and no last known information is not
    // read as an empty set.
    let missing = json!({
        "kind": "sharesCharacteristic",
        "characteristic": "color",
        "object": { "kind": "chosenTarget", "id": "targetPermanent" },
    });
    assert!(
        engine
            .prepare_object_relation(&missing, &source, &BTreeMap::new(), &BTreeMap::new())
            .is_err()
    );
    // An anchor kind no resolver reads is refused by the validator.
    assert!(
        !super::super::object_relation_eval::relation_atom_supported(
            &json!({ "kind": "attachedTo", "object": { "kind": "declaredTargetAnchor" } })
        )
    );
}

#[test]
fn a_static_count_reads_the_attachments_of_its_source() {
    // `for each Equipment attached to NAME` evaluated for a static source:
    // the anchor is the source itself, without a resolving ability.
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        permanent(
            "kemba",
            "Legendary Creature \u{2014} Cat Cleric",
            "player-0",
            None,
        ),
        permanent(
            "blade",
            "Artifact \u{2014} Equipment",
            "player-0",
            Some("kemba"),
        ),
        permanent(
            "mail",
            "Artifact \u{2014} Equipment",
            "player-0",
            Some("kemba"),
        ),
        permanent("other", "Artifact \u{2014} Equipment", "player-0", None),
    ];
    let kemba = engine.state.players[0].battlefield[0].clone();
    let count = json!({
        "kind": "countPermanents",
        "allPlayers": true,
        "where": {
            "kind": "and",
            "operands": [
                { "kind": "subtypeContains", "value": "Equipment" },
                { "kind": "attachedTo", "object": { "kind": "self" } },
            ],
        },
    });
    assert_eq!(
        super::super::object_relation_eval::related_count_supported(&count),
        Some(true)
    );
    assert_eq!(
        engine.related_permanent_count_with_source(&count, "player-0", Some(&kemba)),
        Some(Some(2))
    );
    // Without a source the anchor is undefined, never zero.
    assert_eq!(
        engine.related_permanent_count_with_source(&count, "player-0", None),
        Some(None)
    );
}

#[test]
fn counted_cards_from_the_top_of_the_library_are_exiled() {
    let rules = parsed_rules(
        "Exile Probe",
        "Sorcery",
        "Exile two cards from the top of your library.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].library = (0..5)
        .map(|index| {
            let id = format!("library-{index}");
            test_instance(&id, test_definition(&id, "Instant"), "player-0")
        })
        .collect();
    let exiled = engine.state.players[0].exile.len();
    let source = stack_source(
        test_instance("probe", test_definition("probe", "Sorcery"), "player-0"),
        "player-0",
        "spellAbility",
    );
    execute_effects_at_resolution_start(
        &mut engine,
        &rules[0]["effects"],
        &source,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the exile resolves");
    assert_eq!(engine.state.players[0].library.len(), 3);
    assert_eq!(engine.state.players[0].exile.len(), exiled + 2);
}
