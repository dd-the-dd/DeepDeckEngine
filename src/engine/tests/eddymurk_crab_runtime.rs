use super::*;

// Real Oracle text (Bloomburrow): proves the parser -> IR -> runtime path for
// a turn-conditioned entry replacement and a plural permanent target slot.
const ORACLE_TEXT: &str = "Flash\nThis spell costs {1} less to cast for each instant and sorcery card in your graveyard.\nThis creature enters tapped if it's not your turn.\nWhen this creature enters, tap up to two target creatures.";

/// Fixture objects are admitted with an incarnation, as a zone writer would.
fn admitted(engine: &mut GameEngine, mut card: CardInstance) -> CardInstance {
    engine
        .assign_new_object_incarnation(&mut card)
        .expect("admit test object");
    card
}

fn crab_definition() -> CardDefinition {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Eddymurk Crab".to_string(),
        type_line: "Creature — Elemental Crab".to_string(),
        mana_cost: Some("{5}{U}{U}".to_string()),
        oracle_text: Some(ORACLE_TEXT.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let mut definition = test_definition("eddymurk-crab", "Creature — Elemental Crab");
    definition.rules = parsed
        .abilities
        .into_iter()
        .map(|ability| ability.rule.expect("canonical crab ability"))
        .collect();
    assert!(definition.rules.iter().all(rule_is_executable));
    definition
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn entry_is_tapped_only_outside_its_controllers_turn() {
    for (active_player, expected_tapped) in [(1, false), (0, true)] {
        let mut engine = test_engine(2);
        engine.state.active_player = active_player;
        let mut crab = test_instance("eddymurk-crab", crab_definition(), "player-1");
        engine
            .apply_enter_replacements(&mut crab, &mut EmeritusDecisionProvider)
            .expect("the turn-conditioned entry replacement resolves");
        assert_eq!(
            crab.tapped, expected_tapped,
            "active player {active_player}"
        );
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn plural_target_reference_taps_every_chosen_creature_and_nothing_else() {
    let definition = crab_definition();
    let trigger = definition
        .rules
        .iter()
        .find(|rule| rule["kind"] == "triggeredAbility")
        .cloned()
        .expect("the entry trigger");
    let decision = &trigger["declaration"]["decisions"][0];
    assert_eq!(decision["maximum"], 2);
    let decision_id = decision["id"].as_str().unwrap().to_string();
    let effect = trigger["effects"][0].clone();
    assert_eq!(effect["permanent"]["kind"], "chosenTargets");

    let mut engine = test_engine(2);
    let battlefield = ["first", "second", "bystander"]
        .into_iter()
        .map(|id| {
            let card = test_instance(id, test_definition(id, "Creature — Fish"), "player-0");
            admitted(&mut engine, card)
        })
        .collect();
    engine.state.players[0].battlefield = battlefield;
    let mut targets = BTreeMap::new();
    for (index, id) in ["first", "second"].into_iter().enumerate() {
        targets.insert(
            format!("{decision_id}:{index}"),
            TargetRef::Permanent {
                instance_id: id.to_string(),
            },
        );
    }
    let crab = test_instance("eddymurk-crab", definition, "player-1");
    let mut stack_object = StackObject {
        id: "stack:crab-trigger".to_string(),
        controller: "player-1".to_string(),
        card: admitted(&mut engine, crab),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(trigger.clone()),
        decisions: BTreeMap::new(),
        targets,
    };
    // Targeting captures each chosen permanent's incarnation.
    for (key, target) in stack_object.targets.clone() {
        engine
            .capture_target_incarnation(&mut stack_object.decisions, &key, &target)
            .expect("capture the chosen permanent");
    }
    // Resolution snapshots target legality once, before any effect runs.
    let context = engine.target_numeric_context(&stack_object);
    engine
        .with_target_numeric_context(context, |engine| {
            engine.execute_effect(
                &effect,
                None,
                &stack_object,
                &mut BTreeMap::new(),
                &mut BTreeMap::new(),
                &mut EmeritusDecisionProvider,
            )
        })
        .expect("the plural tap resolves");
    let tapped = engine.state.players[0]
        .battlefield
        .iter()
        .map(|permanent| (permanent.instance_id.as_str(), permanent.tapped))
        .collect::<Vec<_>>();
    assert_eq!(
        tapped,
        [("first", true), ("second", true), ("bystander", false)]
    );
}
