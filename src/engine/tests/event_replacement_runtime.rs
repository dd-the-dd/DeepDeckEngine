//! CR 614.1a, 615.1a, 616.1: static damage and life-gain replacements and
//! damage preventions, parsed from Oracle text and applied at the event. Each program
//! applies once per event; the affected player orders programs whose order
//! changes the result; prevention is skipped for damage that can't be
//! prevented (CR 615.12).
use super::*;

fn parsed_rule(name: &str, type_line: &str, text: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{R}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("event-replacement-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0].rule.clone().expect("primitive IR");
    assert!(rule_is_executable(&rule), "{rule}");
    rule
}

fn permanent_with(id: &str, type_line: &str, mana_cost: &str, rules: Vec<Value>) -> CardDefinition {
    let mut definition = test_definition(id, type_line);
    definition.mana_cost = mana_cost.to_string();
    definition.toughness = definition.toughness.map(|_| "9".to_string());
    definition.rules = rules;
    definition
}

/// player-0 controls `replacer` (with `rules`), a red and a green creature;
/// player-1 controls one creature.
fn engine_with(rules: Vec<Value>, replacer_type: &str) -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        test_instance(
            "replacer",
            permanent_with("replacer", replacer_type, "{2}{R}", rules),
            "player-0",
        ),
        test_instance(
            "red-creature",
            permanent_with("red-creature", "Creature - Goblin", "{R}", Vec::new()),
            "player-0",
        ),
        test_instance(
            "green-creature",
            permanent_with("green-creature", "Creature - Elf", "{G}", Vec::new()),
            "player-0",
        ),
    ];
    engine.state.players[1].battlefield = vec![test_instance(
        "opposing-creature",
        permanent_with("opposing-creature", "Creature - Bear", "{G}", Vec::new()),
        "player-1",
    )];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
}

fn player(id: &str) -> TargetRef {
    TargetRef::Player {
        player_id: id.to_string(),
    }
}

fn permanent(id: &str) -> TargetRef {
    TargetRef::Permanent {
        instance_id: id.to_string(),
    }
}

fn damage(
    engine: &mut GameEngine,
    target: TargetRef,
    amount: i32,
    source: &str,
    combat: bool,
) -> Result<i32, EngineError> {
    engine.deal_damage_amount_with_kind_and_origin(
        target,
        amount,
        source,
        combat,
        QuantityOrigin::Effect,
        None,
    )
}

fn life_lost(engine: &GameEngine, id: &str) -> i32 {
    20 - engine
        .state
        .players
        .iter()
        .find(|player| player.id == id)
        .expect("player")
        .life
}

fn marked(engine: &GameEngine, id: &str) -> i32 {
    engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|card| card.instance_id == id)
        .expect("permanent")
        .damage_marked
}

const TORBRAN: &str = "If a red source you control would deal damage to an opponent or a permanent an opponent controls, it deals that much damage plus 2 instead.";
const FURNACE: &str = "If a source would deal damage to a permanent or player, it deals double that damage to that permanent or player instead.";

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn torbran_adds_to_red_sources_of_its_controller_against_opponents_only() {
    let torbran = parsed_rule(
        "Torbran, Thane of Red Fell",
        "Legendary Creature - Dwarf Noble",
        TORBRAN,
    );
    let mut engine = engine_with(vec![torbran], "Legendary Creature - Dwarf Noble");

    damage(&mut engine, player("player-1"), 1, "red-creature", false).expect("damage");
    assert_eq!(
        life_lost(&engine, "player-1"),
        3,
        "red source, opponent recipient"
    );
    damage(
        &mut engine,
        permanent("opposing-creature"),
        1,
        "red-creature",
        true,
    )
    .expect("damage");
    assert_eq!(
        marked(&engine, "opposing-creature"),
        3,
        "combat damage is replaced too"
    );

    // Near misses: a green source, and a red source damaging its own side.
    damage(&mut engine, player("player-1"), 1, "green-creature", false).expect("damage");
    assert_eq!(
        life_lost(&engine, "player-1"),
        4,
        "a nonred source is unchanged"
    );
    damage(
        &mut engine,
        permanent("green-creature"),
        1,
        "red-creature",
        false,
    )
    .expect("damage");
    assert_eq!(
        marked(&engine, "green-creature"),
        1,
        "own permanent is not a recipient"
    );
    damage(&mut engine, player("player-0"), 1, "red-creature", false).expect("damage");
    assert_eq!(
        life_lost(&engine, "player-0"),
        1,
        "the controller is not an opponent"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn commuting_doublers_apply_once_each_without_a_choice() {
    let furnace = parsed_rule("Furnace of Rath", "Enchantment", FURNACE);
    // Two separate sources of the same replacement: each applies once
    // (CR 614.5), and doubling twice has one result in either order.
    let mut engine = engine_with(vec![furnace.clone(), furnace], "Enchantment");
    damage(&mut engine, player("player-1"), 3, "green-creature", false).expect("damage");
    assert_eq!(life_lost(&engine, "player-1"), 12);
}

struct ChooseReplacementSource {
    first: &'static str,
    asked: usize,
}

impl DecisionProvider for ChooseReplacementSource {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        assert_eq!(request.kind, DecisionKind::ReplacementChoice);
        self.asked += 1;
        Ok(request
            .options
            .iter()
            .position(|option| option.card_instance_id.as_deref() == Some(self.first))
            .unwrap_or(0))
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn the_affected_player_orders_replacements_that_do_not_commute() {
    let torbran = parsed_rule(
        "Torbran, Thane of Red Fell",
        "Legendary Creature - Dwarf Noble",
        TORBRAN,
    );
    let furnace = parsed_rule("Furnace of Rath", "Enchantment", FURNACE);
    for (first, expected) in [("replacer", 6), ("furnace", 4)] {
        let mut engine = engine_with(vec![torbran.clone()], "Legendary Creature - Dwarf Noble");
        let furnace_card = test_instance(
            "furnace",
            permanent_with("furnace", "Enchantment", "{1}{R}{R}", vec![furnace.clone()]),
            "player-0",
        );
        engine.state.players[0].battlefield.push(furnace_card);
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        let mut provider = ChooseReplacementSource { first, asked: 0 };
        // 1 damage: Torbran first gives (1+2)*2 = 6, Furnace first 1*2+2 = 4.
        engine
            .deal_damage_amount_with_kind_and_origin(
                player("player-1"),
                1,
                "red-creature",
                false,
                QuantityOrigin::Effect,
                Some(&mut provider),
            )
            .expect("damage");
        assert_eq!(life_lost(&engine, "player-1"), expected, "{first} first");
        assert_eq!(
            provider.asked, 1,
            "the second program is then the only one left"
        );
    }
    // Without a chooser, an order-dependent result is an explicit error and
    // never an arbitrary order.
    let mut engine = engine_with(vec![torbran, furnace], "Legendary Creature - Dwarf Noble");
    let error = damage(&mut engine, player("player-1"), 1, "red-creature", false)
        .expect_err("ambiguous order without a chooser");
    assert!(
        error.to_string().contains("affected player's choice"),
        "{error}"
    );
    assert_eq!(life_lost(&engine, "player-1"), 0);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn prevention_reduces_matching_damage_and_respects_unpreventable_damage() {
    let sphere = parsed_rule(
        "Sphere of Law",
        "Enchantment",
        "If a red source would deal damage to you, prevent 2 of that damage.",
    );
    let mut engine = engine_with(vec![sphere], "Enchantment");
    // The Sphere protects its controller (player-0).
    damage(&mut engine, player("player-0"), 3, "red-creature", false).expect("damage");
    assert_eq!(life_lost(&engine, "player-0"), 1);
    assert!(
        engine
            .state
            .events
            .iter()
            .any(|event| event.kind == "damagePrevented"
                && event.detail["reason"] == "replacementEffect"
                && event.detail["amount"] == 2)
    );
    damage(&mut engine, player("player-0"), 3, "green-creature", false).expect("damage");
    assert_eq!(
        life_lost(&engine, "player-0"),
        4,
        "a nonred source is not prevented"
    );
    damage(&mut engine, player("player-1"), 3, "red-creature", false).expect("damage");
    assert_eq!(
        life_lost(&engine, "player-1"),
        3,
        "another player is not protected"
    );

    // CR 615.12: damage that can't be prevented ignores the prevention.
    let outcome = engine
        .apply_damage_replacements(
            &super::super::damage_replacement::DamageEventView {
                target: &player("player-0"),
                source_id: "red-creature",
                source_card: engine.state.players[0].battlefield.get(1),
                source_controller: Some("player-0"),
                source_is_spell: false,
                combat: false,
            },
            3,
            false,
            &mut super::super::quantity_replacement::OptionalQuantityProvider(None),
        )
        .expect("replacement analysis");
    assert_eq!(outcome.amount, 3);
    assert!(outcome.applied.is_empty());
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn damage_subject_passive_reads_kind_and_coordinated_recipients_without_new_code() {
    // Unused combination of the shared damage clause: the damage as passive
    // subject + the noncombat kind + an `or` recipient list.
    let shield = parsed_rule(
        "Noncombat Shield Probe",
        "Enchantment",
        "If noncombat damage would be dealt to you or a planeswalker you control, prevent that damage.",
    );
    let mut engine = engine_with(vec![shield], "Enchantment");
    damage(&mut engine, player("player-0"), 3, "red-creature", false).expect("damage");
    assert_eq!(life_lost(&engine, "player-0"), 0, "noncombat damage to you");
    damage(&mut engine, player("player-0"), 3, "red-creature", true).expect("damage");
    assert_eq!(life_lost(&engine, "player-0"), 3, "combat damage is dealt");
    damage(&mut engine, player("player-1"), 3, "red-creature", false).expect("damage");
    assert_eq!(
        life_lost(&engine, "player-1"),
        3,
        "another player is not protected"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn amount_guards_and_damage_kinds_select_the_event() {
    let callous = parsed_rule(
        "Callous Giant",
        "Creature - Giant",
        "If a source would deal 3 or less damage to this creature, prevent that damage.",
    );
    let mut engine = engine_with(vec![callous], "Creature - Giant");
    damage(
        &mut engine,
        permanent("replacer"),
        3,
        "opposing-creature",
        false,
    )
    .expect("damage");
    assert_eq!(marked(&engine, "replacer"), 0, "3 or less is prevented");
    damage(
        &mut engine,
        permanent("replacer"),
        4,
        "opposing-creature",
        false,
    )
    .expect("damage");
    assert_eq!(marked(&engine, "replacer"), 4, "4 is dealt in full");
    damage(
        &mut engine,
        permanent("red-creature"),
        1,
        "opposing-creature",
        false,
    )
    .expect("damage");
    assert_eq!(
        marked(&engine, "red-creature"),
        1,
        "another recipient is not prevented"
    );

    let tuskodon = parsed_rule(
        "Charging Tuskodon",
        "Creature - Dinosaur",
        "If this creature would deal combat damage to a player, it deals double that damage to that player instead.",
    );
    let mut engine = engine_with(vec![tuskodon], "Creature - Dinosaur");
    damage(&mut engine, player("player-1"), 2, "replacer", true).expect("damage");
    assert_eq!(
        life_lost(&engine, "player-1"),
        4,
        "combat damage to a player doubles"
    );
    damage(&mut engine, player("player-1"), 2, "replacer", false).expect("damage");
    assert_eq!(
        life_lost(&engine, "player-1"),
        6,
        "noncombat damage is unchanged"
    );
    damage(
        &mut engine,
        permanent("opposing-creature"),
        2,
        "replacer",
        true,
    )
    .expect("damage");
    assert_eq!(
        marked(&engine, "opposing-creature"),
        2,
        "a permanent recipient is unchanged"
    );
    damage(&mut engine, player("player-1"), 2, "red-creature", true).expect("damage");
    assert_eq!(
        life_lost(&engine, "player-1"),
        8,
        "another source is unchanged"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_resolving_spell_is_a_spell_source_with_its_own_characteristics() {
    let servant = parsed_rule(
        "Fire Servant",
        "Creature - Elemental",
        "If a red instant or sorcery spell you control would deal damage, it deals double that damage instead.",
    );
    let mut engine = engine_with(vec![servant], "Creature - Elemental");
    let bolt = parsed_rule(
        "Searing Spark",
        "Instant",
        "Searing Spark deals 3 damage to any target.",
    );
    let decision = bolt["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("target decision")
        .to_string();
    let mut spell = test_definition("searing-spark", "Instant");
    spell.mana_cost = "{R}".to_string();
    spell.rules = vec![bolt.clone()];
    let stack_object = StackObject {
        id: "stack:spark".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("searing-spark", spell, "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        // A spell, not an ability: its rules come from its card (CR 112.1).
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(decision, player("player-1"))]),
    };
    engine
        .execute_effects(
            &bolt["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the spell resolves");
    assert_eq!(
        life_lost(&engine, "player-1"),
        6,
        "a red instant spell doubles"
    );

    // Near miss: a red creature permanent is not a spell.
    damage(&mut engine, player("player-1"), 3, "red-creature", false).expect("damage");
    assert_eq!(life_lost(&engine, "player-1"), 9);
}

fn gain(engine: &mut GameEngine, player_id: &str, amount: i32) -> Result<i32, EngineError> {
    let plan = engine
        .prepare_life_gain(player_id, amount)?
        .expect("a life-gain plan");
    engine.commit_prepared_life_gain(&plan)?;
    engine.publish_prepared_life_gain(&plan);
    Ok(plan.amount)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn life_gain_replacements_modify_only_the_affected_players_gain() {
    let vitality = parsed_rule(
        "Angel of Vitality",
        "Creature - Angel",
        "If you would gain life, you gain that much life plus 1 instead.",
    );
    let mut engine = engine_with(vec![vitality.clone()], "Creature - Angel");
    assert_eq!(gain(&mut engine, "player-0", 2).expect("gain"), 3);
    // Near miss: an opponent's gain is not this player's event.
    assert_eq!(gain(&mut engine, "player-1", 2).expect("gain"), 2);

    // Doubling and adding do not commute: without the affected player's
    // choice the order is an explicit error, never an arbitrary result.
    let reflection = parsed_rule(
        "Boon Reflection",
        "Enchantment",
        "If you would gain life, you gain twice that much life instead.",
    );
    let mut engine = engine_with(vec![reflection.clone(), reflection.clone()], "Enchantment");
    assert_eq!(
        gain(&mut engine, "player-0", 3).expect("gain"),
        12,
        "two doublers commute"
    );
    let mut engine = engine_with(vec![vitality, reflection], "Creature - Angel");
    let error = gain(&mut engine, "player-0", 1).expect_err("order-dependent result");
    assert!(
        error.to_string().contains("affected player's choice"),
        "{error}"
    );
}
