use mtg_engine::engine::{
    ActionKind, CardDefinition, DecisionChoice, DecisionKind, DecisionProvider,
    DeterministicGamePosition, DeterministicPlayerPosition, EngineDecisionRequest, EngineError,
    GameEngine, GameSetup, GameState, GameStep, PlayerDeck, rule_is_executable,
};
use mtg_engine::model::{PlayableCardInput, builtin_dungeon_definitions, compile_playable_card};
use mtg_engine::oracle::OracleCardParseRequest;

fn forest() -> CardDefinition {
    CardDefinition {
        id: "forest".into(),
        name: "Forest".into(),
        type_line: "Basic Land — Forest".into(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: String::new(),
        power: None,
        toughness: None,
        rules: Vec::new(),
    }
}

fn setup() -> GameSetup {
    GameSetup {
        players: (0..2)
            .map(|index| PlayerDeck {
                id: format!("player-{index}"),
                name: format!("Player {index}"),
                starting_life: 20,
                cards: vec![forest(); 60],
            })
            .collect(),
        opening_hand_size: 7,
        starting_player: 0,
    }
}

#[test]
fn four_canonical_dungeons_are_complete_and_undercity_keeps_its_entry_restriction() {
    let definitions = builtin_dungeon_definitions().unwrap();
    assert_eq!(definitions.len(), 4);
    for dungeon in definitions {
        assert!(dungeon.is_game_piece && !dungeon.is_sideboard && !dungeon.is_token);
        assert!(
            dungeon.rules.iter().all(rule_is_executable),
            "{}",
            dungeon.name
        );
    }
    let undercity = definitions
        .iter()
        .find(|dungeon| dungeon.name == "Undercity")
        .unwrap();
    assert!(undercity.rules.iter().any(|rule| {
        rule["modifiers"].as_array().is_some_and(|modifiers| {
            modifiers.iter().any(|modifier| {
                modifier["kind"] == "dungeonEntryRestriction"
                    && modifier["procedure"] == "venture into Undercity"
            })
        })
    }));
}

#[test]
fn ordinary_games_do_not_load_unused_dungeons_into_observations_or_decks() {
    let engine = GameEngine::new(setup(), 11).unwrap();
    assert!(engine.state().game_pieces.is_empty());
    for player in &engine.state().players {
        assert_eq!(player.library.len() + player.hand.len(), 60);
        assert!(player.sideboard.is_empty());
        assert!(
            player
                .library
                .iter()
                .chain(&player.hand)
                .all(|card| !card.definition.is_game_piece)
        );
    }
}

#[test]
fn an_explicit_dungeon_printing_is_not_duplicated() {
    let mut setup = setup();
    let mut supplied = builtin_dungeon_definitions().unwrap()[0].clone();
    supplied.id = "explicit-dungeon-printing".into();
    setup.players[0].cards.push(supplied);
    let engine = GameEngine::new(setup, 11).unwrap();
    assert_eq!(engine.state().game_pieces.len(), 1);
    assert!(
        engine
            .state()
            .game_pieces
            .iter()
            .any(|piece| piece.id == "explicit-dungeon-printing")
    );
    assert_eq!(
        engine.state().players[0].library.len() + engine.state().players[0].hand.len(),
        60
    );
}

fn compiled(name: &str, kind: &str, mana: &str, text: &str) -> CardDefinition {
    compile_playable_card(PlayableCardInput {
        id: name.to_string(),
        face_id: None,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: (name == "Acererak the Archlich").then(|| "5".into()),
        toughness: (name == "Acererak the Archlich").then(|| "5".into()),
        oracle: OracleCardParseRequest {
            card_name: name.into(),
            type_line: kind.into(),
            mana_cost: Some(mana.into()),
            oracle_text: Some(text.into()),
            layout: None,
            faces: Vec::new(),
        },
    })
    .unwrap()
    .card
}

struct ComboPilot;
impl DecisionProvider for ComboPilot {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.player_id == "player-0" {
            if request.kind == DecisionKind::Priority && state.stack.is_empty() {
                if let Some(index) = request.options.iter().position(|action| {
                    action.kind == ActionKind::CastSpell && action.label.contains("Acererak")
                }) {
                    return Ok(index);
                }
            }
            for preferred in ["Lost Mine of Phandelver", "Mine Tunnels", "Dark Pool"] {
                if let Some(index) = request.options.iter().position(|action| {
                    action.decisions.values().any(|value| {
                        value.as_array().is_some_and(|values| {
                            values.iter().any(|value| value.as_str() == Some(preferred))
                        })
                    })
                }) {
                    return Ok(index);
                }
            }
        }
        Ok(request
            .options
            .iter()
            .position(|action| {
                matches!(
                    action.kind,
                    ActionKind::PassPriority
                        | ActionKind::FinishAttackers
                        | ActionKind::FinishBlockers
                )
            })
            .unwrap_or(0))
    }

    fn choose_number(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        match request.choice.as_ref() {
            Some(DecisionChoice::NumberSelection {
                minimum,
                maximum,
                decision_id,
                ..
            }) => Ok(if decision_id == "loopIterations" {
                *maximum
            } else {
                *minimum
            }),
            _ => Err(EngineError::new("Expected a numeric fixture decision")),
        }
    }
}

#[test]
fn aluren_acererak_really_advances_rooms_and_drains_to_a_win_without_deck_dungeons() {
    let aluren = compiled(
        "Aluren",
        "Enchantment",
        "{2}{G}{G}",
        "Any player may cast creature spells with mana value 3 or less without paying their mana costs and as though they had flash.",
    );
    let acererak = compiled(
        "Acererak the Archlich",
        "Legendary Creature — Zombie Wizard",
        "{2}{B}",
        "When Acererak the Archlich enters the battlefield, if you haven't completed Tomb of Annihilation, return Acererak the Archlich to its owner's hand and venture into the dungeon.",
    );
    let mut setup = setup();
    setup.players[0].cards.extend([aluren, acererak]);
    let mut engine = GameEngine::new(setup, 11).unwrap();
    engine
        .configure_deterministic_position(&DeterministicGamePosition {
            turn_number: 1,
            active_player_id: "player-0".into(),
            step: GameStep::PostcombatMain,
            players: (0..2)
                .map(|index| DeterministicPlayerPosition {
                    player_id: format!("player-{index}"),
                    life: if index == 0 { 20 } else { 3 },
                    battlefield_definition_ids: if index == 0 {
                        vec!["Aluren".into()]
                    } else {
                        Vec::new()
                    },
                    hand_definition_ids: if index == 0 {
                        vec!["Acererak the Archlich".into()]
                    } else {
                        Vec::new()
                    },
                    library_size: 40,
                    mana_pool: Vec::new(),
                })
                .collect(),
        })
        .unwrap();
    engine.run_from_postcombat_main(&mut ComboPilot, 1).unwrap();
    assert_eq!(engine.state().game_pieces.len(), 4);
    assert_eq!(
        engine
            .state()
            .outcome
            .as_ref()
            .and_then(|outcome| outcome.winner.as_deref()),
        Some("player-0"),
        "Outcome/life/last events: {:?} {:?} {:?}",
        engine.state().outcome,
        engine
            .state()
            .players
            .iter()
            .map(|player| player.life)
            .collect::<Vec<_>>(),
        engine
            .state()
            .events
            .iter()
            .rev()
            .take(12)
            .collect::<Vec<_>>()
    );
    assert!(engine.state().players[1].life <= 0);
    assert!(
        !engine
            .state()
            .events
            .iter()
            .any(|event| event.kind == "dungeonVentureUnavailable")
    );
    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| event.kind == "dungeonRoomEntered" && event.detail["completed"] == true)
    );
}
