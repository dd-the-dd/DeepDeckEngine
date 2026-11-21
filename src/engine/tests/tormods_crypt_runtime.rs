use super::*;

// Real Oracle text: a possessive target player owns the exiled zone; the
// shared zone-collection operand lowers it to moveCards(cardsInZone(...)).
const ORACLE_TEXT: &str = "{T}, Sacrifice this artifact: Exile target player's graveyard.";

/// Fixture objects are admitted with an incarnation, as a zone writer would.
fn admitted(engine: &mut GameEngine, mut card: CardInstance) -> CardInstance {
    engine
        .assign_new_object_incarnation(&mut card)
        .expect("admit test object");
    card
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn exiles_exactly_the_target_players_graveyard() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Tormod's Crypt".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{0}".to_string()),
        oracle_text: Some(ORACLE_TEXT.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0]
        .rule
        .clone()
        .expect("canonical crypt ability");
    assert!(rule_is_executable(&rule));
    let decision = &rule["declaration"]["decisions"][0];
    let player_target = decision["id"]
        .as_str()
        .expect("player target id")
        .to_string();
    let effect = rule["effects"][0].clone();
    assert_eq!(effect["cards"]["zone"]["kind"], "graveyard");

    let mut engine = test_engine(2);
    for (player, prefix) in [(0, "mine"), (1, "theirs")] {
        let graveyard = (0..2)
            .map(|index| {
                let id = format!("{prefix}-{index}");
                let card = test_instance(
                    &id,
                    test_definition(&id, "Instant"),
                    &format!("player-{player}"),
                );
                admitted(&mut engine, card)
            })
            .collect();
        engine.state.players[player].graveyard = graveyard;
    }
    let crypt = test_instance("crypt", test_definition("crypt", "Artifact"), "player-0");
    let crypt = admitted(&mut engine, crypt);
    let stack_object = StackObject {
        id: "stack:crypt".to_string(),
        controller: "player-0".to_string(),
        card: crypt,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            player_target,
            TargetRef::Player {
                player_id: "player-1".to_string(),
            },
        )]),
    };
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
        .expect("the zone exile resolves");
    assert!(engine.state.players[1].graveyard.is_empty());
    assert_eq!(engine.state.players[1].exile.len(), 2);
    assert_eq!(
        engine.state.players[0].graveyard.len(),
        2,
        "other graveyards are untouched"
    );
}
