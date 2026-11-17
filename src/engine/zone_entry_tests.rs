use super::super::*;
use super::BattlefieldEntryAdmission;

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    CardDefinition {
        id: id.into(),
        name: id.into(),
        type_line: type_line.into(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{0}".into(),
        power: type_line.contains("Creature").then(|| "2".into()),
        toughness: type_line.contains("Creature").then(|| "2".into()),
        rules,
    }
}
fn active(modifier: Value) -> Value {
    json!({"kind":"staticAbility","source":{"kind":"self"},"activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},"modifiers":[modifier]})
}
fn barrier_rules() -> Vec<Value> {
    vec![
        active(
            json!({"kind":"prohibitEnterBattlefieldFromZones","zones":["graveyard","library"],"where":{"kind":"cardTypeContains","value":"Creature"}}),
        ),
        active(
            json!({"kind":"prohibitCastFromZones","players":{"kind":"eachPlayer"},"zones":["graveyard","library"],"where":null}),
        ),
    ]
}
fn engine() -> GameEngine {
    let filler = definition("filler", "Creature", vec![]);
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        19,
    )
    .unwrap();
    for player in &mut engine.state.players {
        player.library.clear();
        player.hand.clear();
        player.battlefield.clear();
        player.graveyard.clear();
    }
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield.push(card_instance(
        "p0",
        &definition("barrier", "Artifact", barrier_rules()),
        0,
    ));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    engine
}
struct NoChoices;
impl DecisionProvider for NoChoices {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Err(EngineError::new(format!(
            "unexpected zone-entry decision {}",
            request.id
        )))
    }
}

#[test]
fn admission_is_shared_and_prohibition_precedes_destination_replacements() {
    let mut engine = engine();
    let creature = definition("entrant", "Creature", vec![]);
    assert_eq!(
        engine.battlefield_entry_admission(&creature, "graveyard"),
        BattlefieldEntryAdmission::RemainInSourceZone
    );
    assert_eq!(
        engine.battlefield_entry_admission(&creature, "hand"),
        BattlefieldEntryAdmission::Allowed
    );
    let replacement = definition(
        "replacement",
        "Enchantment",
        vec![active(
            json!({"kind":"prohibitUncastBattlefieldEntry","where":null,"destination":"exile"}),
        )],
    );
    engine.state.players[0]
        .battlefield
        .insert(0, card_instance("p0", &replacement, 1));
    assert_eq!(
        engine.card_entry_from_zone_replacement(&creature, "graveyard"),
        Some("prohibited")
    );
    engine.state.players[0].battlefield.swap(0, 1);
    assert_eq!(
        engine.card_entry_from_zone_replacement(&creature, "graveyard"),
        Some("prohibited")
    );
}

#[test]
fn casting_zone_prohibition_applies_to_both_players_without_blocking_hand_casts() {
    let mut engine = engine();
    let ability_controller = json!({"kind":"controllerOf","object":{"kind":"self"}});
    let permissions = definition(
        "permission",
        "Enchantment",
        vec![
            active(
                json!({"kind":"castingPermission","players":{"kind":"eachPlayer"},"sourceZone":"graveyard","where":null,"withoutPayingManaCost":false,"asThoughFlash":true}),
            ),
            active(
                json!({"kind":"playCardsFromTopLibrary","player":ability_controller,"where":null}),
            ),
        ],
    );
    assert!(permissions.rules.iter().all(rule_is_executable));
    for index in 0..2 {
        let player_id = format!("p{index}");
        engine.state.players[index]
            .battlefield
            .push(card_instance(&player_id, &permissions, 1));
        for (serial, zone) in ["hand", "graveyard", "library"].into_iter().enumerate() {
            let spell = definition(
                &format!("{zone}-{player_id}"),
                "Instant",
                vec![json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[]})],
            );
            let card = card_instance(&player_id, &spell, serial + 2);
            match zone {
                "hand" => engine.state.players[index].hand.push(card),
                "graveyard" => engine.state.players[index].graveyard.push(card),
                "library" => engine.state.players[index].library.push(card),
                _ => unreachable!(),
            }
        }
        let cast_ids = engine
            .legal_priority_actions(index)
            .into_iter()
            .filter(|a| a.kind == ActionKind::CastSpell)
            .filter_map(|a| a.card_instance_id)
            .collect::<Vec<_>>();
        assert_eq!(cast_ids, vec![format!("{player_id}:hand-{player_id}:2")]);
    }
    engine.state.players[0].battlefield.remove(0);
    for index in 0..2 {
        let cast_ids = engine
            .legal_priority_actions(index)
            .into_iter()
            .filter(|a| a.kind == ActionKind::CastSpell)
            .filter_map(|a| a.card_instance_id)
            .collect::<BTreeSet<_>>();
        let player_id = format!("p{index}");
        assert_eq!(
            cast_ids,
            [
                format!("{player_id}:hand-{player_id}:2"),
                format!("{player_id}:graveyard-{player_id}:3"),
                format!("{player_id}:library-{player_id}:4")
            ]
            .into_iter()
            .collect::<BTreeSet<_>>()
        );
    }
}

#[test]
fn revealed_creature_remains_in_library_and_is_included_in_shuffle_without_etb() {
    let mut engine = engine();
    let entrant = card_instance("p0", &definition("entrant", "Creature", vec![]), 2);
    engine.state.players[0].library = vec![
        card_instance("p0", &definition("other", "Artifact", vec![]), 3),
        entrant.clone(),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    let before_incarnations = engine.state.battlefield_incarnation_sequence;
    engine
        .reveal_creatures_to_battlefield("p0", 1, "observer", &mut NoChoices)
        .unwrap();
    assert_eq!(engine.state.players[0].library.len(), 2);
    assert_eq!(
        engine.state.battlefield_incarnation_sequence,
        before_incarnations
    );
    assert!(
        engine.state.players[0]
            .library
            .iter()
            .any(|c| c.instance_id == entrant.instance_id)
    );
    assert!(
        !engine.state.players[0]
            .battlefield
            .iter()
            .any(|c| c.instance_id == entrant.instance_id)
    );
    assert!(engine.state.events.iter().any(|e| {
        e.kind == "cardsRevealed"
            && e.detail["cardIds"]
                .as_array()
                .unwrap()
                .iter()
                .any(|id| id.as_str() == Some(entrant.instance_id.as_str()))
    }));
}

#[test]
fn delayed_graveyard_return_is_one_attempt_and_does_not_remove_prohibited_card() {
    let mut engine = engine();
    let before_incarnations = engine.state.battlefield_incarnation_sequence;
    let entrant = card_instance("p0", &definition("entrant", "Creature", vec![]), 2);
    engine.state.players[0].graveyard.push(entrant.clone());
    engine.state.rule_modifiers.push(json!({"kind":"returnGraveyardCardAtEndStep","cardInstanceId":entrant.instance_id,"controllerId":"p0","notBeforeTurn":0}));
    engine.process_end_step_returns(&mut NoChoices).unwrap();
    assert_eq!(
        engine.state.players[0].graveyard[0].instance_id,
        entrant.instance_id
    );
    assert!(engine.state.rule_modifiers.is_empty());
    assert_eq!(
        engine.state.battlefield_incarnation_sequence,
        before_incarnations
    );
    engine.state.players[0].battlefield.clear();
    engine.process_end_step_returns(&mut NoChoices).unwrap();
    assert_eq!(
        engine.state.players[0].graveyard[0].instance_id,
        entrant.instance_id
    );
}

#[test]
fn land_plays_distinguish_creature_cards_in_source_zones_from_ordinary_lands_and_hand() {
    for zone in ["library", "graveyard", "hand"] {
        for creature in [false, true] {
            let mut engine = engine();
            let controller = json!({"kind":"controllerOf","object":{"kind":"self"}});
            let permission = definition(
                "land-permission",
                "Enchantment",
                vec![
                    active(
                        json!({"kind":"playCardsFromTopLibrary","player":controller,"where":null}),
                    ),
                    active(json!({"kind":"playLandsFromGraveyard","player":controller})),
                ],
            );
            assert!(permission.rules.iter().all(rule_is_executable));
            engine.state.players[0]
                .battlefield
                .push(card_instance("p0", &permission, 1));
            let land = card_instance(
                "p0",
                &definition(
                    "land-candidate",
                    if creature {
                        "Land Creature — Dryad"
                    } else {
                        "Land"
                    },
                    vec![],
                ),
                2,
            );
            match zone {
                "library" => engine.state.players[0].library.push(land.clone()),
                "graveyard" => engine.state.players[0].graveyard.push(land.clone()),
                "hand" => engine.state.players[0].hand.push(land.clone()),
                _ => unreachable!(),
            }
            engine
                .admit_explicit_test_fixture_objects()
                .expect("fixture construction precedes references");
            let action = engine.legal_priority_actions(0).into_iter().find(|a| {
                a.kind == ActionKind::PlayLand
                    && a.card_instance_id.as_deref() == Some(land.instance_id.as_str())
            });
            let prohibited = creature && zone != "hand";
            assert_eq!(action.is_none(), prohibited, "{zone} creature={creature}");
            if let Some(action) = action {
                engine
                    .apply_priority_action(&action, &mut NoChoices)
                    .unwrap();
                assert!(
                    engine.state.players[0]
                        .battlefield
                        .iter()
                        .any(|card| card.instance_id == land.instance_id)
                );
            } else {
                // An action selected before the barrier appeared must fail
                // before consuming a card, land play, RNG or zone identity.
                let barrier = engine.state.players[0].battlefield.remove(0);
                let action = engine
                    .legal_priority_actions(0)
                    .into_iter()
                    .find(|a| {
                        a.kind == ActionKind::PlayLand
                            && a.card_instance_id.as_deref() == Some(land.instance_id.as_str())
                    })
                    .unwrap();
                engine.state.players[0].battlefield.insert(0, barrier);
                let before = serde_json::to_value(&engine.state).unwrap();
                assert!(
                    engine
                        .apply_priority_action(&action, &mut NoChoices)
                        .unwrap_err()
                        .to_string()
                        .contains("cannot enter")
                );
                assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
            }
        }
    }
}

#[path = "zone_entry_snapshot_tests.rs"]
mod snapshot;

#[path = "zone_entry_reveal_timing_tests.rs"]
mod reveal_timing;

#[path = "zone_entry_cast_apply_tests.rs"]
mod cast_apply;
