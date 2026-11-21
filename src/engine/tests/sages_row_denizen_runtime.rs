use super::*;

// Real Oracle text (Guilds of Ravnica): `mill` is a shared player verb whose
// subject is a declared player target; it lowers to mill(chosenTarget).
const ORACLE_TEXT: &str =
    "Whenever another blue creature you control enters, target player mills two cards.";

/// Fixture objects are admitted with an incarnation, as a zone writer would.
fn admitted(engine: &mut GameEngine, mut card: CardInstance) -> CardInstance {
    engine
        .assign_new_object_incarnation(&mut card)
        .expect("admit test object");
    card
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn mills_exactly_the_chosen_players_top_cards() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Sage's Row Denizen".to_string(),
        type_line: "Creature — Vedalken Wizard".to_string(),
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some(ORACLE_TEXT.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0]
        .rule
        .clone()
        .expect("canonical denizen trigger");
    assert!(rule_is_executable(&rule));
    let decision = &rule["declaration"]["decisions"][0];
    assert_eq!(decision["candidates"]["kind"], "players");
    let player_target = decision["id"]
        .as_str()
        .expect("player target id")
        .to_string();
    let effect = rule["effects"][0].clone();
    assert_eq!(effect["kind"], "mill");
    assert_eq!(effect["player"]["kind"], "chosenTarget");

    let mut engine = test_engine(2);
    for (player, prefix) in [(0, "mine"), (1, "theirs")] {
        let library = (0..3)
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
        engine.state.players[player].library = library;
    }
    let denizen = test_instance(
        "denizen",
        test_definition("denizen", "Creature"),
        "player-0",
    );
    let denizen = admitted(&mut engine, denizen);
    let stack_object = StackObject {
        id: "stack:denizen-trigger".to_string(),
        controller: "player-0".to_string(),
        card: denizen,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
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
        .expect("the targeted mill resolves");
    let ids = |cards: &[CardInstance]| {
        cards
            .iter()
            .map(|card| card.instance_id.clone())
            .collect::<Vec<_>>()
    };
    // The top of a library is its last element.
    assert_eq!(ids(&engine.state.players[1].library), ["theirs-0"]);
    assert_eq!(
        ids(&engine.state.players[1].graveyard),
        ["theirs-2", "theirs-1"]
    );
    assert_eq!(
        engine.state.players[0].library.len(),
        3,
        "the controller is not milled"
    );
    assert!(engine.state.players[0].graveyard.is_empty());
}
