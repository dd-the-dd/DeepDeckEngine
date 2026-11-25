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

#[test]
fn the_creatures_that_convoked_the_source_receive_its_counters() {
    // Venerated Loxodon (CR 702.51c): the creatures tapped to pay the
    // convoke cost are recorded with the cast; another creature is not one.
    parsed_rules(
        "Venerated Loxodon",
        "Creature \u{2014} Elephant Cleric",
        "Convoke (Your creatures can help cast this spell. Each creature you tap while casting this spell pays for {1} or one mana of that creature's color.)\nWhen this creature enters, put a +1/+1 counter on each creature that convoked it.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        permanent(
            "loxodon",
            "Creature \u{2014} Elephant Cleric",
            "player-0",
            None,
        ),
        permanent("elf-a", "Creature \u{2014} Elf", "player-0", None),
        permanent("elf-b", "Creature \u{2014} Elf", "player-0", None),
        permanent("bear", "Creature \u{2014} Bear", "player-0", None),
    ];
    engine.record_event(
        "spellCast",
        Some("player-0".to_string()),
        Some("loxodon".to_string()),
        json!({ "decisions": { "convokeSources": ["elf-a", "elf-b"] } }),
    );
    let effect = json!([{
        "kind": "putCounters",
        "counter": "+1/+1",
        "count": { "kind": "integer", "value": 1 },
        "permanent": {
            "kind": "eachPermanent",
            "where": {
                "kind": "and",
                "operands": [
                    { "kind": "cardTypeContains", "value": "Creature" },
                    { "kind": "convoked", "object": { "kind": "self" } },
                ],
            },
        },
    }]);
    assert!(effect_supported(&effect[0]), "{effect:#}");
    let loxodon = engine.state.players[0].battlefield[0].clone();
    let source = stack_source(loxodon, "player-0", "triggeredAbility");
    execute_effects_at_resolution_start(
        &mut engine,
        &effect,
        &source,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the counters are placed");
    let counters = |id: &str| {
        engine.state.players[0]
            .battlefield
            .iter()
            .find(|permanent| permanent.instance_id == id)
            .and_then(|permanent| permanent.counters.get("+1/+1").copied())
            .unwrap_or(0)
    };
    assert_eq!(
        ["loxodon", "elf-a", "elf-b", "bear"].map(counters),
        [0, 1, 1, 0]
    );
}

#[test]
fn a_static_bonus_counts_the_equipment_attached_to_its_source() {
    // Myr Adapter: `it` names the modified source; each Equipment attached
    // to it, and no other Equipment, adds +1/+1.
    let rules = parsed_rules(
        "Myr Adapter",
        "Artifact Creature \u{2014} Myr",
        "This creature gets +1/+1 for each Equipment attached to it.",
    );
    let mut engine = test_engine(2);
    let mut adapter = permanent(
        "adapter",
        "Artifact Creature \u{2014} Myr",
        "player-0",
        None,
    );
    adapter.definition.rules = rules;
    engine.state.players[0].battlefield = vec![
        adapter,
        permanent(
            "blade",
            "Artifact \u{2014} Equipment",
            "player-0",
            Some("adapter"),
        ),
        permanent(
            "mail",
            "Artifact \u{2014} Equipment",
            "player-0",
            Some("adapter"),
        ),
        permanent("loose", "Artifact \u{2014} Equipment", "player-0", None),
    ];
    let adapter = engine.state.players[0].battlefield[0].clone();
    assert_eq!(engine.current_power(&adapter), 4, "2 printed + 2 attached");
    assert_eq!(engine.current_toughness(&adapter), 4);
}

#[test]
fn a_declared_target_filter_binds_its_relation_from_the_source() {
    // `Destroy target Aura attached to a creature` / `target creature
    // blocking it`: the anchor is bound from the source when the targets
    // are chosen (CR 601.2c), so candidate matching reads game state only.
    let mut engine = test_engine(2);
    engine.state.players[1].battlefield = vec![
        permanent("bear", "Creature \u{2014} Bear", "player-1", None),
        permanent("forest", "Land \u{2014} Forest", "player-1", None),
        permanent(
            "on-bear",
            "Enchantment \u{2014} Aura",
            "player-1",
            Some("bear"),
        ),
        permanent(
            "on-forest",
            "Enchantment \u{2014} Aura",
            "player-1",
            Some("forest"),
        ),
    ];
    let source = permanent("harpist", "Creature \u{2014} Human", "player-0", None);
    engine.state.players[0].battlefield = vec![source.clone()];
    let mut candidates = json!({
        "kind": "permanents",
        "where": {
            "kind": "and",
            "operands": [
                { "kind": "subtypeContains", "value": "Aura" },
                {
                    "kind": "attachedTo",
                    "object": {
                        "kind": "eachPermanent",
                        "where": { "kind": "cardTypeContains", "value": "Creature" },
                    },
                },
            ],
        },
    });
    assert!(super::super::target_candidates_supported(&candidates));
    // A declared-target anchor cannot be bound from the source.
    assert!(!super::super::target_candidates_supported(&json!({
        "kind": "permanents",
        "where": { "kind": "attachedTo", "object": { "kind": "chosenTarget", "id": "t" } },
    })));
    engine
        .bind_source_candidates(&mut candidates, &source)
        .expect("the source binds the anchor");
    let legal = engine.state.players[1]
        .battlefield
        .iter()
        .filter(|permanent| {
            engine.permanent_matches_target_filter(permanent, &candidates["where"], "player-0")
        })
        .map(|permanent| permanent.instance_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(legal, vec!["on-bear"]);
}

/// Whether the battlefield permanent `id` satisfies a prepared relation.
fn relation_holds(engine: &GameEngine, atom: &Value, id: &str) -> bool {
    let candidate = engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|permanent| permanent.instance_id == id)
        .expect("a battlefield candidate");
    engine.eval_object_relation(
        atom,
        super::super::runtime_object_filter::ObjectFilterView::Permanent(candidate),
    )
}

fn attacker(attacker_id: &str, defender: TargetRef, defending_player_id: &str) -> DeclaredAttacker {
    DeclaredAttacker {
        attacker_id: attacker_id.to_string(),
        defender,
        defending_player_id: defending_player_id.to_string(),
    }
}

#[test]
fn block_history_relations_read_this_turns_declarations_per_candidate() {
    // Venomous Breath / Glyph of Doom: `that blocked or were blocked by it
    // this turn` reads the turn's block declarations (CR 509.1) for each
    // candidate, anchored on one bound object; a pair of an earlier turn
    // does not count.
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        permanent("hydra", "Creature \u{2014} Hydra", "player-0", None),
        permanent("elf", "Creature \u{2014} Elf", "player-0", None),
    ];
    engine.state.players[1].battlefield = vec![
        permanent("wall", "Creature \u{2014} Wall", "player-1", None),
        permanent("bear", "Creature \u{2014} Bear", "player-1", None),
    ];
    engine.record_event(
        "blockerDeclared",
        Some("player-1".to_string()),
        Some("wall".to_string()),
        json!({ "attackerId": "hydra" }),
    );
    engine.record_event(
        "blockerDeclared",
        Some("player-1".to_string()),
        Some("bear".to_string()),
        json!({ "attackerId": "elf" }),
    );
    let hydra = engine.state.players[0].battlefield[0].clone();
    let hydra_source = stack_source(hydra, "player-0", "activatedAbility");
    let unprepared = json!({ "kind": "blockedAnchorThisTurn", "object": { "kind": "self" } });
    assert!(super::super::object_relation_eval::relation_atom_supported(
        &unprepared
    ));
    let blocked_it = engine
        .prepare_object_relation(
            &unprepared,
            &hydra_source,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("the source anchor is read");
    assert!(relation_holds(&engine, &blocked_it, "wall"));
    assert!(
        !relation_holds(&engine, &blocked_it, "bear"),
        "the bear blocked another attacker"
    );
    let wall = engine.state.players[1].battlefield[0].clone();
    let wall_source = stack_source(wall, "player-1", "activatedAbility");
    let blocked_by = engine
        .prepare_object_relation(
            &json!({ "kind": "blockedByAnchorThisTurn", "object": { "kind": "self" } }),
            &wall_source,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("the source anchor is read");
    assert!(relation_holds(&engine, &blocked_by, "hydra"));
    assert!(!relation_holds(&engine, &blocked_by, "elf"));
    // An unprepared atom never matches, and the declarations of an earlier
    // turn are not this turn's history.
    assert!(!relation_holds(&engine, &unprepared, "wall"));
    engine.state.turn_number += 1;
    assert!(!relation_holds(&engine, &blocked_it, "wall"));
}

#[test]
fn damage_to_an_anchor_reads_the_turns_damage_records() {
    // Autumn Willow and Baron Sengir: `each Vampire you control that dealt
    // damage to that creature this turn` (CR 120.1). Zero damage is not
    // dealt (CR 120.8), and the combat form ignores noncombat damage.
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        permanent("biter", "Creature \u{2014} Vampire", "player-0", None),
        permanent("pinger", "Creature \u{2014} Vampire", "player-0", None),
        permanent("idle", "Creature \u{2014} Vampire", "player-0", None),
    ];
    engine.state.players[1].battlefield = vec![permanent(
        "victim",
        "Creature \u{2014} Bear",
        "player-1",
        None,
    )];
    engine.record_event(
        "damageDealt",
        None,
        Some("victim".to_string()),
        json!({ "amount": 2, "combat": true, "source": "biter" }),
    );
    engine.record_event(
        "damageDealt",
        None,
        Some("victim".to_string()),
        json!({ "amount": 1, "combat": false, "source": "pinger" }),
    );
    engine.record_event(
        "damageDealt",
        None,
        Some("victim".to_string()),
        json!({ "amount": 0, "combat": true, "source": "idle" }),
    );
    let victim = engine.state.players[1].battlefield[0].clone();
    let source = stack_source(victim, "player-1", "triggeredAbility");
    let any = engine
        .prepare_object_relation(
            &json!({ "kind": "dealtDamageToAnchorThisTurn", "object": { "kind": "self" } }),
            &source,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("the anchor is read");
    let combat = engine
        .prepare_object_relation(
            &json!({ "kind": "dealtCombatDamageToAnchorThisTurn", "object": { "kind": "self" } }),
            &source,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("the anchor is read");
    assert!(relation_holds(&engine, &any, "biter"));
    assert!(relation_holds(&engine, &any, "pinger"));
    assert!(!relation_holds(&engine, &any, "idle"), "zero damage");
    assert!(relation_holds(&engine, &combat, "biter"));
    assert!(
        !relation_holds(&engine, &combat, "pinger"),
        "noncombat damage"
    );
}

#[test]
fn the_attacked_player_relation_reads_each_attackers_defender() {
    // `creature attacking you [or a planeswalker you control]` (CR 508.1b):
    // an attacker of a planeswalker is not attacking its controller (CR
    // 506.3) unless the planeswalker alternative is stated.
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![permanent(
        "jace",
        "Legendary Planeswalker \u{2014} Jace",
        "player-0",
        None,
    )];
    engine.state.players[1].battlefield = vec![
        permanent("orc", "Creature \u{2014} Orc", "player-1", None),
        permanent("imp", "Creature \u{2014} Imp", "player-1", None),
        permanent("bear", "Creature \u{2014} Bear", "player-1", None),
    ];
    engine.state.combat.attackers = vec![
        attacker(
            "orc",
            TargetRef::Player {
                player_id: "player-0".to_string(),
            },
            "player-0",
        ),
        attacker(
            "imp",
            TargetRef::Permanent {
                instance_id: "jace".to_string(),
            },
            "player-0",
        ),
    ];
    let jace = engine.state.players[0].battlefield[0].clone();
    let source = stack_source(jace, "player-0", "activatedAbility");
    let you_atom = json!({
        "kind": "attackingPlayer",
        "player": "abilityController",
        "orPlaneswalkers": false,
    });
    let walkers_atom = json!({
        "kind": "attackingPlayer",
        "player": "abilityController",
        "orPlaneswalkers": true,
    });
    assert!(super::super::object_relation_eval::relation_atom_supported(
        &walkers_atom
    ));
    assert!(
        !super::super::object_relation_eval::relation_atom_supported(&json!({
            "kind": "attackingPlayer",
            "player": "anyPlayer",
        })),
        "an unresolvable role is refused"
    );
    let you = engine
        .prepare_object_relation(&you_atom, &source, &BTreeMap::new(), &BTreeMap::new())
        .expect("the role resolves");
    let you_or_walkers = engine
        .prepare_object_relation(&walkers_atom, &source, &BTreeMap::new(), &BTreeMap::new())
        .expect("the role resolves");
    assert!(relation_holds(&engine, &you, "orc"));
    assert!(
        !relation_holds(&engine, &you, "imp"),
        "it attacks the planeswalker"
    );
    assert!(!relation_holds(&engine, &you, "bear"), "not attacking");
    assert!(relation_holds(&engine, &you_or_walkers, "imp"));
    // A declared target filter binds the role from its source (Ice Floe).
    let mut candidates = json!({
        "kind": "permanents",
        "where": {
            "kind": "and",
            "operands": [
                { "kind": "cardTypeContains", "value": "Creature" },
                you_atom,
            ],
        },
    });
    assert!(super::super::target_candidates_supported(&candidates));
    let land = permanent("floe", "Land", "player-0", None);
    engine
        .bind_source_candidates(&mut candidates, &land)
        .expect("the source binds the role");
    let legal = engine.state.players[1]
        .battlefield
        .iter()
        .filter(|permanent| {
            engine.permanent_matches_target_filter(permanent, &candidates["where"], "player-0")
        })
        .map(|permanent| permanent.instance_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(legal, vec!["orc"]);
}

#[test]
fn a_stack_item_target_relates_each_stack_object_to_its_own_targets() {
    // Siren Stormtamer: `target spell or ability that targets you or a
    // creature you control` (CR 115.1), read relative to the player
    // choosing the target; a trigger selector keeps refusing the relation.
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield =
        vec![permanent("pet", "Creature \u{2014} Cat", "player-0", None)];
    engine.state.players[1].battlefield = vec![permanent(
        "brute",
        "Creature \u{2014} Ogre",
        "player-1",
        None,
    )];
    let filter = json!({
        "kind": "and",
        "operands": [
            {
                "kind": "or",
                "operands": [
                    { "kind": "isSpell" },
                    { "kind": "isActivatedAbility" },
                    { "kind": "isTriggeredAbility" },
                ],
            },
            {
                "kind": "targets",
                "players": "abilityController",
                "permanents": {
                    "where": { "kind": "cardTypeContains", "value": "Creature" },
                    "controller": "abilityController",
                },
            },
        ],
    });
    assert!(super::super::object_predicate::stack_item_candidate_filter_supported(&filter));
    assert!(!super::super::stack_item_filter_supported(&filter));
    let targeting = |id: &str, target: TargetRef| {
        let mut object = stack_source(
            test_instance(id, test_definition(id, "Instant"), "player-1"),
            "player-1",
            "spellAbility",
        );
        object.targets.insert("t".to_string(), target);
        object
    };
    let at_you = targeting(
        "bolt",
        TargetRef::Player {
            player_id: "player-0".to_string(),
        },
    );
    let at_pet = targeting(
        "shock",
        TargetRef::Permanent {
            instance_id: "pet".to_string(),
        },
    );
    let at_brute = targeting(
        "grow",
        TargetRef::Permanent {
            instance_id: "brute".to_string(),
        },
    );
    let at_them = targeting(
        "blast",
        TargetRef::Player {
            player_id: "player-1".to_string(),
        },
    );
    assert!(engine.stack_item_candidate_matches(&at_you, &filter, "player-0"));
    assert!(engine.stack_item_candidate_matches(&at_pet, &filter, "player-0"));
    assert!(!engine.stack_item_candidate_matches(&at_brute, &filter, "player-0"));
    assert!(!engine.stack_item_candidate_matches(&at_them, &filter, "player-0"));
    // Relative to the other player, the roles swap.
    assert!(engine.stack_item_candidate_matches(&at_them, &filter, "player-1"));
}

#[test]
fn siren_stormtamer_and_blessed_reversal_lower_executable_relations() {
    // Parser to runtime: the stack-item relative lowers onto the declared
    // stack target, and the attacked-player count is executable and counts
    // only attackers of the ability controller (CR 508.1b).
    let siren = parsed_rules(
        "Siren Stormtamer",
        "Creature \u{2014} Siren Pirate Wizard",
        "Flying\n{U}, Sacrifice this creature: Counter target spell or ability that targets you or a creature you control.",
    );
    assert!(
        siren
            .iter()
            .any(|rule| rule.to_string().contains("\"kind\":\"targets\"")),
        "{siren:#?}"
    );
    let rules = parsed_rules(
        "Blessed Reversal",
        "Instant",
        "You gain 3 life for each creature attacking you.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![permanent(
        "jace",
        "Legendary Planeswalker \u{2014} Jace",
        "player-0",
        None,
    )];
    engine.state.players[1].battlefield = vec![
        permanent("orc", "Creature \u{2014} Orc", "player-1", None),
        permanent("ogre", "Creature \u{2014} Ogre", "player-1", None),
        permanent("imp", "Creature \u{2014} Imp", "player-1", None),
    ];
    let you = TargetRef::Player {
        player_id: "player-0".to_string(),
    };
    engine.state.combat.attackers = vec![
        attacker("orc", you.clone(), "player-0"),
        attacker("ogre", you, "player-0"),
        attacker(
            "imp",
            TargetRef::Permanent {
                instance_id: "jace".to_string(),
            },
            "player-0",
        ),
    ];
    let life_before = engine.state.players[0].life;
    let source = stack_source(
        test_instance(
            "reversal",
            test_definition("reversal", "Instant"),
            "player-0",
        ),
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
    .expect("the spell resolves");
    assert_eq!(engine.state.players[0].life, life_before + 6);
}
