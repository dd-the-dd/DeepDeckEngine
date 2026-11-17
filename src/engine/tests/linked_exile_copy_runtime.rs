use super::*;

const ORACLE_TEXT: &str = "Enchant creature you control\nWhen this Aura enters, exile up to one target creature other than enchanted creature until this Aura leaves the battlefield. Enchanted creature becomes a copy of that creature until this Aura leaves the battlefield.\nEnchanted creature has ward {2}.";

fn aura_definition() -> CardDefinition {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Infiltration".to_string(),
        type_line: "Enchantment — Aura".to_string(),
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some(ORACLE_TEXT.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let mut definition = test_definition("test-infiltration", "Enchantment — Aura");
    definition.rules = parsed
        .abilities
        .into_iter()
        .map(|ability| ability.rule.expect("canonical Aura ability"))
        .collect();
    assert!(definition.rules.iter().all(rule_is_executable));
    definition
}

fn entry_rule(definition: &CardDefinition) -> Value {
    definition.rules[1].clone()
}

fn linked_stack_object(aura: &CardInstance, rule: &Value) -> StackObject {
    StackObject {
        id: "stack:linked-exile-copy".to_string(),
        controller: aura.controller.clone(),
        card: aura.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn creature_definition(id: &str, power: &str, toughness: &str) -> CardDefinition {
    let mut definition = test_definition(id, "Creature — Shapeshifter");
    definition.name = id.to_string();
    definition.power = Some(power.to_string());
    definition.toughness = Some(toughness.to_string());
    definition
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn optional_zero_target_binds_empty_and_leaves_the_enchanted_creature_unchanged() {
    let mut aura = test_instance("test-aura", aura_definition(), "player-0");
    aura.attached_to = Some("enchanted-creature".to_string());
    let enchanted = test_instance(
        "enchanted-creature",
        creature_definition("Original Bearer", "2", "2"),
        "player-0",
    );
    let rule = entry_rule(&aura.definition);
    let stack_object = linked_stack_object(&aura, &rule);
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![aura, enchanted];

    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the optional linked sequence resolves with no target");

    let bearer = engine.state.players[0]
        .battlefield
        .iter()
        .find(|permanent| permanent.instance_id == "enchanted-creature")
        .expect("the enchanted creature remains");
    assert_eq!(bearer.definition.name, "Original Bearer");
    assert!(
        engine
            .state
            .players
            .iter()
            .all(|player| player.exile.is_empty())
    );
    assert!(
        engine
            .state
            .rule_modifiers
            .iter()
            .all(|modifier| { value_kind(modifier) != Some("copyWhileSourceOnBattlefield") })
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn opposing_target_is_copied_from_lki_then_returns_and_copy_expires_with_the_aura() {
    let mut aura = test_instance("test-aura", aura_definition(), "player-0");
    aura.attached_to = Some("enchanted-creature".to_string());
    let enchanted = test_instance(
        "enchanted-creature",
        creature_definition("Original Bearer", "2", "2"),
        "player-0",
    );
    let copied = test_instance(
        "opposing-creature",
        creature_definition("Opposing Template", "6", "5"),
        "player-1",
    );
    let rule = entry_rule(&aura.definition);
    let mut stack_object = linked_stack_object(&aura, &rule);
    stack_object.targets.insert(
        "exileTarget".to_string(),
        TargetRef::Permanent {
            instance_id: "opposing-creature".to_string(),
        },
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![aura, enchanted];
    engine.state.players[1].battlefield = vec![copied];

    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("linked exile and LKI copy resolve");

    assert!(engine.permanent_position("opposing-creature").is_none());
    assert!(
        engine.state.players[1]
            .exile
            .iter()
            .any(|card| card.instance_id == "opposing-creature")
    );
    let bearer = engine.state.players[0]
        .battlefield
        .iter()
        .find(|permanent| permanent.instance_id == "enchanted-creature")
        .expect("bearer remains");
    assert_eq!(bearer.definition.name, "Opposing Template");
    assert_eq!(bearer.definition.power.as_deref(), Some("6"));
    assert_eq!(engine.ward_costs_for_permanent(bearer, "player-1").len(), 1);

    engine
        .destroy_permanent_from_stack("test-aura", &stack_object)
        .expect("the Aura leaves the battlefield");
    let restored = engine.state.players[0]
        .battlefield
        .iter()
        .find(|permanent| permanent.instance_id == "enchanted-creature")
        .expect("bearer remains after the Aura leaves");
    assert_eq!(restored.definition.name, "Original Bearer");
    assert!(
        engine.state.players[1]
            .exile
            .iter()
            .any(|card| card.instance_id == "opposing-creature")
    );

    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the linked exiled card return resolves");
    assert!(
        engine.state.players[1]
            .battlefield
            .iter()
            .any(|card| card.instance_id == "opposing-creature")
    );
    assert!(
        engine.state.players[1]
            .exile
            .iter()
            .all(|card| card.instance_id != "opposing-creature")
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn trigger_declaration_excludes_the_permanent_attached_to_the_source() {
    let mut aura = test_instance("test-aura", aura_definition(), "player-0");
    aura.attached_to = Some("enchanted-creature".to_string());
    let enchanted = test_instance(
        "enchanted-creature",
        creature_definition("Original Bearer", "2", "2"),
        "player-0",
    );
    let other = test_instance(
        "other-creature",
        creature_definition("Other Creature", "3", "3"),
        "player-1",
    );
    let rule = entry_rule(&aura.definition);
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![aura.clone(), enchanted];
    engine.state.players[1].battlefield = vec![other];

    let declarations = engine
        .declarations_for_triggered_rule(&rule, "player-0", &aura)
        .expect("checked trigger declaration dependencies")
        .expect("trigger declarations are generated");
    assert!(declarations.iter().all(|declaration| {
        declaration.targets.values().all(|target| {
            !matches!(target, TargetRef::Permanent { instance_id } if instance_id == "enchanted-creature")
        })
    }));
    assert!(declarations.iter().any(|declaration| {
        declaration.targets.values().any(|target| {
            matches!(target, TargetRef::Permanent { instance_id } if instance_id == "other-creature")
        })
    }));
}
