//! Static frames compiled from real Oracle text and executed by the engine:
//! static prevention (CR 615.1a), protection keyword lines (CR 702.16),
//! turn conditions (CR 611.3a), attachment combat restrictions (CR 508.1c,
//! 509.1b) and untap exclusions (CR 502.3).
use super::*;

/// The executable rules the Oracle compiler produces for one printed line.
fn compiled_rules(name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("static-frame-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:#?}",
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

fn permanent_with(
    id: &str,
    type_line: &str,
    mana_cost: &str,
    rules: Vec<Value>,
    controller: &str,
) -> CardInstance {
    let mut definition = test_definition(id, type_line);
    definition.mana_cost = mana_cost.to_string();
    if definition.toughness.is_some() {
        definition.toughness = Some("5".to_string());
    }
    definition.rules = rules;
    test_instance(id, definition, controller)
}

fn engine_with(battlefields: [Vec<CardInstance>; 2]) -> GameEngine {
    let mut engine = test_engine(2);
    let [first, second] = battlefields;
    engine.state.players[0].battlefield = first;
    engine.state.players[1].battlefield = second;
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
}

fn permanent_ref(id: &str) -> TargetRef {
    TargetRef::Permanent {
        instance_id: id.to_string(),
    }
}

fn damage(engine: &mut GameEngine, target: &str, amount: i32, source: &str, combat: bool) {
    engine
        .deal_damage_amount_with_kind_and_origin(
            permanent_ref(target),
            amount,
            source,
            combat,
            QuantityOrigin::Effect,
            None,
        )
        .expect("damage resolves");
}

fn find<'a>(engine: &'a GameEngine, id: &str) -> &'a CardInstance {
    engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|card| card.instance_id == id)
        .expect("permanent")
}

/// Guard Gomazoa: combat damage to it is prevented, other damage is dealt.
#[test]
fn combat_only_static_prevention_spares_noncombat_damage() {
    let wall = permanent_with(
        "gomazoa",
        "Creature - Gomazoa",
        "{2}{U}",
        compiled_rules(
            "Guard Gomazoa",
            "Creature — Gomazoa",
            "Prevent all combat damage that would be dealt to this creature.",
        ),
        "player-1",
    );
    let attacker = permanent_with("attacker", "Creature - Test", "{1}", Vec::new(), "player-0");
    let mut engine = engine_with([vec![attacker], vec![wall]]);

    damage(&mut engine, "gomazoa", 3, "attacker", true);
    assert_eq!(find(&engine, "gomazoa").damage_marked, 0);
    damage(&mut engine, "gomazoa", 2, "attacker", false);
    assert_eq!(find(&engine, "gomazoa").damage_marked, 2);
}

/// A protection keyword line prevents damage from a source of that color
/// (CR 702.16e) and admits damage from any other source.
#[test]
fn protection_keyword_line_prevents_damage_from_its_quality() {
    let knight = permanent_with(
        "knight",
        "Creature - Human Knight",
        "{B}{B}",
        compiled_rules(
            "Black Knight",
            "Creature — Human Knight",
            "Protection from white",
        ),
        "player-1",
    );
    let white = permanent_with("white", "Creature - Test", "{W}", Vec::new(), "player-0");
    let red = permanent_with("red", "Creature - Test", "{R}", Vec::new(), "player-0");
    let mut engine = engine_with([vec![white, red], vec![knight]]);

    damage(&mut engine, "knight", 2, "white", false);
    assert_eq!(find(&engine, "knight").damage_marked, 0);
    damage(&mut engine, "knight", 2, "red", false);
    assert_eq!(find(&engine, "knight").damage_marked, 2);
}

/// Pacifism: the attached creature can't block, its other creature can.
#[test]
fn attachment_combat_restriction_applies_to_the_enchanted_creature() {
    let pacifism = permanent_with(
        "pacifism",
        "Enchantment - Aura",
        "{1}{W}",
        compiled_rules(
            "Pacifism",
            "Enchantment — Aura",
            "Enchanted creature can't attack or block.",
        ),
        "player-0",
    );
    let enchanted = permanent_with(
        "enchanted",
        "Creature - Test",
        "{1}",
        Vec::new(),
        "player-1",
    );
    let free = permanent_with("free", "Creature - Test", "{1}", Vec::new(), "player-1");
    let mut aura = pacifism;
    aura.attached_to = Some("enchanted".to_string());
    let attacker = permanent_with("attacker", "Creature - Test", "{1}", Vec::new(), "player-0");
    let engine = engine_with([vec![aura, attacker.clone()], vec![enchanted, free]]);

    assert!(engine.permanent_has_keyword(find(&engine, "enchanted"), "cantBlock"));
    assert!(engine.permanent_has_keyword(find(&engine, "enchanted"), "cantAttack"));
    assert!(!engine.permanent_has_keyword(find(&engine, "free"), "cantBlock"));
}

/// Keyword lines compiled by the shared keyword atoms are read by the
/// engine's own keyword procedures (CR 702.37, 702.51, 702.109).
#[test]
fn keyword_line_atoms_are_read_by_the_engine_procedures() {
    let definition = |name: &str, oracle: &str| {
        let mut definition = test_definition(name, "Creature - Test");
        definition.rules = compiled_rules(name, "Creature — Test", oracle);
        definition
    };
    let dasher = definition(
        "Goblin Heelcutter",
        "Dash {2}{R} (You may cast this spell for its dash cost. If you do, it gains haste, and it's returned from the battlefield to its owner's hand at the beginning of the next end step.)",
    );
    assert_eq!(dash_cost(&dasher), Some("{2}{R}"));
    let morph = definition(
        "Den Protector",
        "Megamorph {1}{G} (You may cast this card face down as a 2/2 creature for {3}. Turn it face up any time for its megamorph cost and put a +1/+1 counter on it.)",
    );
    let ability = morph_ability(&morph).expect("morph ability");
    assert_eq!(ability["cost"]["manaCost"], "{1}{G}");
    assert_eq!(ability["putCounterWhenTurnedFaceUp"], true);
    let convoker = definition(
        "Chord of Calling",
        "Convoke (Your creatures can help cast this spell. Each creature you tap while casting this spell pays for {1} or one mana of that creature's color.)",
    );
    let engine = engine_with([Vec::new(), Vec::new()]);
    assert!(engine.card_has_convoke(&convoker, "player-0"));
}

/// `During your turn` gates the grant on the active player (CR 611.3a);
/// `doesn't untap` applies to the source itself (CR 502.3).
#[test]
fn turn_condition_and_untap_exclusion_follow_the_static_frame() {
    let lance_bearer = permanent_with(
        "bearer",
        "Creature - Human",
        "{1}",
        compiled_rules(
            "Skilled Battlecarver",
            "Creature — Human Warrior",
            "During your turn, this creature has first strike.\nThis creature doesn't untap during your untap step.",
        ),
        "player-0",
    );
    let mut engine = engine_with([vec![lance_bearer], Vec::new()]);

    engine.state.active_player = 0;
    assert!(engine.permanent_has_keyword(find(&engine, "bearer"), "firstStrike"));
    engine.state.active_player = 1;
    assert!(!engine.permanent_has_keyword(find(&engine, "bearer"), "firstStrike"));
    assert!(engine.static_modifier_applies_to("doesNotUntap", find(&engine, "bearer")));
}
