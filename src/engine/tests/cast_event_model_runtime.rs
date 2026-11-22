//! Cast family of the generic event model (docs/oracle-event-model.md):
//! one cast record, one matcher, the source spell as a subject relation and
//! the per-turn ordinal over matching spells as a pattern modifier.
use super::*;

fn parsed_rules(name: &str, type_line: &str, text: &str) -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{text}: {:?}", parsed.abilities);
    parsed
        .abilities
        .iter()
        .map(|ability| {
            let rule = ability.rule.clone().expect("canonical rule");
            assert!(rule_is_executable(&rule), "{text}: {rule}");
            rule
        })
        .collect()
}

/// Puts the spell on the stack, logs the cast and collects its triggers, as
/// casting does once the spell became cast (CR 601.2i).
fn cast(engine: &mut GameEngine, id: &str, type_line: &str, rules: Vec<Value>) {
    cast_with_targets(engine, id, type_line, rules, BTreeMap::new());
}

fn cast_with_targets(
    engine: &mut GameEngine,
    id: &str,
    type_line: &str,
    rules: Vec<Value>,
    targets: BTreeMap<String, TargetRef>,
) {
    let mut definition = test_definition(id, type_line);
    definition.rules = rules;
    let card = test_instance(id, definition.clone(), "player-0");
    engine.state.stack.push(StackObject {
        id: format!("stack:{id}"),
        controller: "player-0".to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets,
    });
    engine.record_event(
        "spellCast",
        Some("player-0".to_string()),
        Some(id.to_string()),
        json!({"typeLine": type_line, "cardName": id}),
    );
    engine.enqueue_spell_cast_triggers("player-0", &definition, &BTreeMap::new());
}

fn triggered_sources(engine: &GameEngine) -> Vec<String> {
    engine
        .state
        .stack
        .iter()
        .filter(|object| object.ability_kind.is_some())
        .map(|object| object.card.instance_id.clone())
        .collect()
}

#[test]
fn the_source_spell_observes_only_its_own_cast() {
    let rules = parsed_rules(
        "Cast Probe",
        "Creature — Eldrazi",
        "When you cast this spell, draw a card.",
    );
    assert_eq!(rules[0]["event"]["kind"], "spellCast");
    assert_eq!(rules[0]["event"]["sourceIsSelf"], true);
    let mut engine = test_engine(2);
    // The same card already on the battlefield is not the cast spell.
    engine.state.players[0].battlefield = vec![test_instance(
        "resolved-copy",
        CardDefinition {
            rules: rules.clone(),
            ..test_definition("resolved-copy", "Creature — Eldrazi")
        },
        "player-0",
    )];
    cast(&mut engine, "other-spell", "Instant", Vec::new());
    assert!(triggered_sources(&engine).is_empty());
    cast(&mut engine, "eldrazi", "Creature — Eldrazi", rules);
    assert_eq!(triggered_sources(&engine), ["eldrazi"]);
}

#[test]
fn a_matching_spell_ordinal_counts_only_matching_spells_this_turn() {
    // `your first instant or sorcery spell each turn`.
    let rules = parsed_rules(
        "Ordinal Watcher",
        "Enchantment",
        "Whenever you cast your first instant or sorcery spell each turn, draw a card.",
    );
    assert_eq!(rules[0]["event"]["spellCastOrdinal"], 1);
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![test_instance(
        "watcher",
        CardDefinition {
            rules,
            ..test_definition("watcher", "Enchantment")
        },
        "player-0",
    )];
    cast(&mut engine, "bear", "Creature — Bear", Vec::new());
    assert!(triggered_sources(&engine).is_empty(), "not an instant");
    cast(&mut engine, "bolt", "Instant", Vec::new());
    assert_eq!(triggered_sources(&engine), ["watcher"]);
    cast(&mut engine, "shock", "Instant", Vec::new());
    assert_eq!(
        triggered_sources(&engine),
        ["watcher"],
        "the second instant"
    );
    engine.state.turn_number += 1;
    cast(&mut engine, "sorcery", "Sorcery", Vec::new());
    assert_eq!(triggered_sources(&engine), ["watcher", "watcher"]);
}

fn permanent_target(id: &str) -> BTreeMap<String, TargetRef> {
    BTreeMap::from([(
        "target".to_string(),
        TargetRef::Permanent {
            instance_id: id.to_string(),
        },
    )])
}

#[test]
fn a_spell_that_targets_matches_the_cast_records_targets() {
    // Heroic: only a spell targeting the source itself.
    let rules = parsed_rules(
        "Heroic Probe",
        "Creature — Human Warrior",
        "Whenever you cast a spell that targets this creature, draw a card.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        test_instance(
            "hero",
            CardDefinition {
                rules,
                ..test_definition("hero", "Creature — Human Warrior")
            },
            "player-0",
        ),
        test_instance(
            "other",
            test_definition("other", "Creature — Bear"),
            "player-0",
        ),
    ];
    cast_with_targets(
        &mut engine,
        "pump-a",
        "Instant",
        Vec::new(),
        permanent_target("other"),
    );
    assert!(triggered_sources(&engine).is_empty(), "another target");
    cast(&mut engine, "untargeted", "Sorcery", Vec::new());
    assert!(triggered_sources(&engine).is_empty(), "no target");
    cast_with_targets(
        &mut engine,
        "pump-b",
        "Instant",
        Vec::new(),
        permanent_target("hero"),
    );
    assert_eq!(triggered_sources(&engine), ["hero"]);
}

#[test]
fn only_requires_every_target_to_match() {
    let rules = parsed_rules(
        "Only Probe",
        "Enchantment",
        "Whenever you cast a spell that targets only a creature you control, draw a card.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        test_instance(
            "watcher",
            CardDefinition {
                rules,
                ..test_definition("watcher", "Enchantment")
            },
            "player-0",
        ),
        test_instance(
            "mine",
            test_definition("mine", "Creature — Bear"),
            "player-0",
        ),
    ];
    engine.state.players[1].battlefield = vec![test_instance(
        "theirs",
        test_definition("theirs", "Creature — Bear"),
        "player-1",
    )];
    let mut both = permanent_target("mine");
    both.insert(
        "second".to_string(),
        TargetRef::Permanent {
            instance_id: "theirs".to_string(),
        },
    );
    cast_with_targets(&mut engine, "split", "Instant", Vec::new(), both);
    assert!(
        triggered_sources(&engine).is_empty(),
        "one target is an opponent's"
    );
    cast_with_targets(
        &mut engine,
        "single",
        "Instant",
        Vec::new(),
        permanent_target("mine"),
    );
    assert_eq!(triggered_sources(&engine), ["watcher"]);
}
