use mtg_engine::engine::{
    ActionKind, CardDefinition, DecisionKind, DecisionProvider, EngineDecisionRequest, EngineError,
    GameEngine, GameSetup, GameStatus, PlayerDeck, RandomClientPool, RandomSimulationRequest,
    simulate_random_games, simulate_traced_random_game,
};

fn land_definition() -> CardDefinition {
    CardDefinition {
        id: "test-plains".to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: String::new(),
        name: "Test Plains".to_string(),
        power: None,
        rules: Vec::new(),
        toughness: None,
        type_line: "Basic Land — Plains".to_string(),
    }
}

fn player_deck(index: usize, card_count: usize) -> PlayerDeck {
    PlayerDeck {
        cards: vec![land_definition(); card_count],
        id: format!("player-{index}"),
        name: format!("Player {index}"),
        starting_life: 20,
    }
}

fn setup_with_players(player_count: usize) -> GameSetup {
    GameSetup {
        opening_hand_size: 7,
        players: (0..player_count)
            .map(|index| player_deck(index, 16))
            .collect(),
        starting_player: 0,
    }
}

#[derive(Default)]
struct PassingClients {
    priority_requests: Vec<String>,
    combat_requests: Vec<(DecisionKind, String)>,
}

impl DecisionProvider for PassingClients {
    fn choose(
        &mut self,
        _state: &mtg_engine::engine::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.kind == DecisionKind::Priority {
            self.priority_requests.push(request.player_id.clone());
        }
        if matches!(
            request.kind,
            DecisionKind::Attackers | DecisionKind::Blockers
        ) {
            self.combat_requests
                .push((request.kind.clone(), request.player_id.clone()));
        }
        Ok(request
            .options
            .iter()
            .position(|option| {
                matches!(
                    option.kind,
                    ActionKind::PassPriority
                        | ActionKind::FinishAttackers
                        | ActionKind::FinishBlockers
                )
            })
            .unwrap_or(0))
    }
}

#[derive(Default)]
struct CastAndAttackClients {
    cast_players: std::collections::BTreeSet<String>,
    defending_players_seen: std::collections::BTreeSet<String>,
}

impl DecisionProvider for CastAndAttackClients {
    fn choose(
        &mut self,
        state: &mtg_engine::engine::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let preferred = match request.kind {
            DecisionKind::Priority if !self.cast_players.contains(&request.player_id) => {
                let cast = request
                    .options
                    .iter()
                    .position(|option| option.kind == ActionKind::CastSpell);
                if cast.is_some() {
                    self.cast_players.insert(request.player_id.clone());
                }
                cast
            }
            DecisionKind::Priority => request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::PassPriority),
            DecisionKind::Attackers => {
                if state.turn_number == 4 && request.player_id == "player-0" {
                    for option in request
                        .options
                        .iter()
                        .filter(|option| option.kind == ActionKind::DeclareAttacker)
                    {
                        if let Some(mtg_engine::engine::TargetRef::Player { player_id }) =
                            option.targets.get("defender")
                        {
                            self.defending_players_seen.insert(player_id.clone());
                        }
                    }
                }
                request
                    .options
                    .iter()
                    .position(|option| option.kind == ActionKind::DeclareAttacker)
                    .or_else(|| {
                        request
                            .options
                            .iter()
                            .position(|option| option.kind == ActionKind::FinishAttackers)
                    })
            }
            DecisionKind::Blockers => request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::FinishBlockers),
            DecisionKind::OpeningHandSelection
            | DecisionKind::Mulligan
            | DecisionKind::MulliganBottom
            | DecisionKind::Discard
            | DecisionKind::CombatDamage
            | DecisionKind::ReplacementChoice
            | DecisionKind::ResolutionChoice
            | DecisionKind::Sideboarding => Some(0),
        };
        Ok(preferred.unwrap_or(0))
    }
}

/// Feature: The engine accepts only two-to-four seated Magic players.
#[test]
fn engine_accepts_the_supported_client_counts() {
    for player_count in 2..=4 {
        GameEngine::new(setup_with_players(player_count), 100 + player_count as u64)
            .unwrap_or_else(|error| panic!("{player_count} players should be accepted: {error}"));
    }

    assert!(GameEngine::new(setup_with_players(0), 1).is_err());
    assert!(GameEngine::new(setup_with_players(1), 1).is_err());
    assert!(GameEngine::new(setup_with_players(5), 1).is_err());
}

/// Feature: Multiplayer priority visits every live client clockwise before a phase can close.
#[test]
fn multiplayer_priority_cycles_clockwise() {
    let mut engine =
        GameEngine::new(setup_with_players(4), 22).expect("four-player setup is valid");
    let mut clients = PassingClients::default();

    assert_eq!(
        engine.run(&mut clients, 1).expect("one turn runs"),
        GameStatus::TurnLimitReached,
    );
    let passes = engine
        .state()
        .events
        .iter()
        .filter(|event| event.kind == "priorityPassed")
        .take(4)
        .map(|event| event.player_id.as_deref().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(passes, ["player-0", "player-1", "player-2", "player-3"]);
}

/// Feature: Priority clients are skipped when passing is their only legal action.
#[test]
fn priority_without_a_real_choice_is_automatic() {
    let mut engine =
        GameEngine::new(setup_with_players(3), 22).expect("three-player setup is valid");
    let mut clients = PassingClients::default();

    engine.run(&mut clients, 1).expect("one turn runs");

    assert!(
        clients
            .priority_requests
            .iter()
            .all(|player_id| player_id == "player-0")
    );
    assert!(engine.state().events.iter().any(|event| {
        event.kind == "priorityPassed" && event.player_id.as_deref() == Some("player-1")
    }));
}

/// Feature: Empty combat declarations are automatic rather than fake client decisions.
#[test]
fn combat_without_a_real_choice_does_not_request_a_client_decision() {
    let mut engine =
        GameEngine::new(setup_with_players(3), 23).expect("three-player setup is valid");
    let mut clients = PassingClients::default();

    engine.run(&mut clients, 1).expect("one turn runs");

    assert!(clients.combat_requests.is_empty());
}

/// Feature: A player eliminated during their draw step receives no later turn decisions.
#[test]
fn eliminated_active_player_receives_no_combat_decision() {
    let setup = GameSetup {
        opening_hand_size: 7,
        players: vec![player_deck(0, 7), player_deck(1, 16), player_deck(2, 16)],
        starting_player: 0,
    };
    let trace =
        simulate_traced_random_game(setup, 24, 1).expect("eliminated-player turn completes");

    assert!(trace.findings.is_empty(), "{:#?}", trace.findings);
    assert!(trace.decisions.iter().all(|decision| {
        decision.client_id != "player-0"
            || !decision
                .state
                .players
                .iter()
                .any(|player| player.id == "player-0" && player.has_lost)
    }));
}

/// Feature: Only a two-player starting player skips the draw step of the first turn.
#[test]
fn first_turn_draw_depends_on_game_kind() {
    let mut duel = GameEngine::new(setup_with_players(2), 31).expect("duel setup");
    let mut duel_clients = PassingClients::default();
    duel.run(&mut duel_clients, 1).expect("duel turn runs");
    assert!(!duel.state().events.iter().any(|event| {
        event.kind == "cardDrawn" && event.player_id.as_deref() == Some("player-0")
    }));

    let mut multiplayer = GameEngine::new(setup_with_players(3), 31).expect("multiplayer setup");
    let mut multiplayer_clients = PassingClients::default();
    multiplayer
        .run(&mut multiplayer_clients, 1)
        .expect("multiplayer turn runs");
    assert!(multiplayer.state().events.iter().any(|event| {
        event.kind == "cardDrawn" && event.player_id.as_deref() == Some("player-0")
    }));
}

/// Feature: A multiplayer loss removes one client while the game continues to a sole winner.
#[test]
fn multiplayer_eliminates_players_without_ending_early() {
    let mut engine =
        GameEngine::new(setup_with_players(3), 41).expect("three-player setup is valid");

    engine
        .lose_life("player-1", 20, "audit")
        .expect("first player loses life");
    assert!(engine.state().outcome.is_none());
    assert!(engine.state().players[1].has_lost);
    assert!(!engine.state().players[0].has_lost);

    engine
        .lose_life("player-2", 20, "audit")
        .expect("second player loses life");
    let outcome = engine.state().outcome.as_ref().expect("one player remains");
    assert_eq!(outcome.winner.as_deref(), Some("player-0"));
    assert_eq!(outcome.losers, ["player-1", "player-2"]);
}

/// Feature: Every attacker in Free-for-All can choose any live opponent as its defender.
#[test]
fn multiplayer_attack_options_name_each_opponent() {
    let creature = CardDefinition {
        id: "free-creature".to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: "{0}".to_string(),
        name: "Free Creature".to_string(),
        power: Some("1".to_string()),
        rules: Vec::new(),
        toughness: Some("1".to_string()),
        type_line: "Creature — Test".to_string(),
    };
    let setup = GameSetup {
        opening_hand_size: 7,
        players: (0..3)
            .map(|index| PlayerDeck {
                cards: vec![creature.clone(); 20],
                id: format!("player-{index}"),
                name: format!("Player {index}"),
                starting_life: 20,
            })
            .collect(),
        starting_player: 0,
    };
    let mut engine = GameEngine::new(setup, 51).expect("three-player creature setup");
    let mut clients = CastAndAttackClients::default();

    engine
        .run(&mut clients, 4)
        .expect("four turns reach the first repeat attacker");

    assert_eq!(
        clients.defending_players_seen,
        ["player-1".to_string(), "player-2".to_string()]
            .into_iter()
            .collect(),
    );
}

/// Feature: Independent seeded random clients leave a deterministic, invariant-checked decision trace.
#[test]
fn random_clients_trace_the_decisions_they_receive() {
    let setup = setup_with_players(4);
    let first = simulate_traced_random_game(setup.clone(), 20260727, 4)
        .expect("first traced simulation runs");
    let second =
        simulate_traced_random_game(setup, 20260727, 4).expect("repeated traced simulation runs");

    assert_eq!(first.decisions, second.decisions);
    assert_eq!(first.events, second.events);
    assert!(first.findings.is_empty(), "{:#?}", first.findings);
    assert!(!first.decisions.is_empty());
    assert!(first.decisions.iter().all(|trace| {
        trace.request.player_id == trace.client_id
            && trace.selected_action == trace.request.options[trace.selected_option_index]
    }));

    let clients = first
        .decisions
        .iter()
        .map(|trace| trace.client_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        clients,
        ["player-0", "player-1", "player-2", "player-3"]
            .into_iter()
            .collect(),
    );

    let _provider_type_is_public = RandomClientPool::seeded(20260727);
}

/// Feature: Seeded random clients complete Magic games at every supported seat count.
#[test]
fn random_simulations_complete_for_every_supported_seat_count() {
    for player_count in 2..=4 {
        let summary = simulate_random_games(RandomSimulationRequest {
            games: 3,
            max_turns: 100,
            seed: 20260727 + player_count as u64,
            setup: setup_with_players(player_count),
        })
        .unwrap_or_else(|error| {
            panic!("{player_count}-client random simulations should run: {error}")
        });

        assert_eq!(summary.completed_games, 3, "{player_count} clients");
        assert_eq!(summary.stalled_games, 0, "{player_count} clients");
    }
}
