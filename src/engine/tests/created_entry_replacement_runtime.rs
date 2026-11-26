//! Real Oracle text -> primitive IR -> engine behavior for an entry
//! replacement a resolving program creates (CR 611.2a, 614.1c): the static
//! uncast-entry modifier installed as a created continuous effect.
use super::*;

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_created_uncast_entry_replacement_exiles_nontoken_creatures_this_turn() {
    // Mistcaller.
    let oracle = "Sacrifice this creature: Until end of turn, if a nontoken creature would enter and it wasn't cast, exile it instead.";
    let request = OracleCardParseRequest {
        card_name: "Mistcaller".to_string(),
        type_line: "Creature \u{2014} Merfolk Wizard".to_string(),
        mana_cost: Some("{U}".to_string()),
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
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed
        .abilities
        .into_iter()
        .find_map(|ability| ability.rule)
        .expect("one rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    assert_eq!(
        rule["effects"][0]["kind"],
        "installTemporaryStaticModifiers"
    );
    let mut engine = test_engine(2);
    let mut definition = test_definition("mistcaller", "Creature - Merfolk Wizard");
    definition.rules = vec![rule.clone()];
    let source = test_instance("mistcaller", definition, "player-0");
    engine.state.players[0].battlefield.push(source.clone());
    let stack = StackObject {
        id: "stack:mistcaller".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the effect resolves");
    let creature = test_definition("bear", "Creature - Bear");
    assert_eq!(
        engine.battlefield_entry_destination_replacement(&creature),
        Some("exile")
    );
    let mut token = test_definition("token", "Creature - Bear");
    token.is_token = true;
    assert_eq!(
        engine.battlefield_entry_destination_replacement(&token),
        None,
        "a token is not affected"
    );
}
