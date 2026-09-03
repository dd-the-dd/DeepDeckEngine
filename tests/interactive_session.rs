use mtg_engine::engine::{
    ActionKind, CardDefinition, DecisionKind, GameSetup, LegalAction, PlayerDeck,
};
use mtg_engine::session::{
    CreateGameSessionRequest, GameSessionManager, GameSessionView, SubmitGameSessionAction,
    UpdateGameSessionSettings,
};
use serde_json::json;

fn card(id: &str, name: &str, type_line: &str, mana_cost: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: mana_cost.to_string(),
        name: name.to_string(),
        power: None,
        rules: Vec::new(),
        toughness: None,
        type_line: type_line.to_string(),
    }
}

fn flashback_card(index: usize) -> CardDefinition {
    let mut definition = card(&format!("flashback-{index}"), "Flashback", "Instant", "{R}");
    definition.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "targetCard",
                "kind": "chooseTargets",
                "minimum": 1,
                "maximum": 1,
                "candidates": {
                    "kind": "cards",
                    "zone": {
                        "kind": "graveyard",
                        "player": {
                            "kind": "controllerOf",
                            "object": { "kind": "self" }
                        }
                    },
                    "where": {
                        "kind": "or",
                        "operands": [
                            { "kind": "cardTypeContains", "value": "Instant" },
                            { "kind": "cardTypeContains", "value": "Sorcery" }
                        ]
                    }
                }
            }]
        },
        "effects": [{
            "kind": "grantAbility",
            "object": { "kind": "chosenTarget", "id": "targetCard" },
            "ability": {
                "kind": "flashback",
                "cost": {
                    "kind": "manaCostOf",
                    "card": { "kind": "abilitySource" }
                }
            },
            "duration": { "kind": "untilEndOfCurrentTurn" }
        }]
    })];
    definition
}

fn priority_instant(id: &str) -> CardDefinition {
    let mut definition = card(id, "Priority Bolt", "Instant", "{R}");
    definition.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [{
            "kind": "gainLife",
            "player": {
                "kind": "controllerOf",
                "object": { "kind": "self" }
            },
            "amount": { "kind": "integer", "value": 1 }
        }]
    })];
    definition
}

fn starfield_shepherd(id: &str) -> CardDefinition {
    let mut definition = card(id, "Starfield Shepherd", "Creature - Angel", "{0}");
    definition.power = Some("3".to_string());
    definition.toughness = Some("2".to_string());
    definition.rules = vec![json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "enterBattlefield", "object": { "kind": "self" } },
        "effects": [
            {
                "kind": "chooseCards",
                "id": "searchedCards",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "candidates": {
                    "kind": "cards",
                    "zone": {
                        "kind": "library",
                        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    },
                    "where": {
                        "kind": "and",
                        "operands": [
                            { "kind": "cardTypeContains", "value": "Creature" },
                            {
                                "kind": "compare",
                                "operator": "<=",
                                "left": {
                                    "kind": "manaValueOf",
                                    "object": { "kind": "candidate" },
                                },
                                "right": { "kind": "integer", "value": 1 },
                            },
                        ],
                    },
                },
                "minimum": 0,
                "maximum": 1,
            },
            {
                "kind": "revealCards",
                "cards": { "kind": "decisionResult", "decisionId": "searchedCards" },
            },
            {
                "kind": "moveCards",
                "cards": { "kind": "decisionResult", "decisionId": "searchedCards" },
                "to": {
                    "kind": "hand",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                },
            },
            {
                "kind": "shuffleZone",
                "zone": {
                    "kind": "library",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                },
            },
        ],
    })];
    definition
}

fn multi_action_mana_land() -> CardDefinition {
    let mut definition = card("multi-action-mana-land", "Costed Hall", "Land", "");
    definition.rules = vec![
        json!({
            "kind": "manaAbility",
            "source": { "kind": "self" },
            "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
            "effects": [{
                "kind": "addMana",
                "player": {
                    "kind": "controllerOf",
                    "object": { "kind": "self" }
                },
                "mana": "{C}{C}{C}{C}{C}"
            }]
        }),
        json!({
            "kind": "activatedAbility",
            "source": { "kind": "self" },
            "costs": [{ "kind": "payMana", "manaCost": "{5}" }],
            "effects": [{
                "kind": "gainLife",
                "player": {
                    "kind": "controllerOf",
                    "object": { "kind": "self" }
                },
                "amount": { "kind": "integer", "value": 1 }
            }]
        }),
    ];
    definition
}

fn modal_target_spell() -> CardDefinition {
    let mut definition = card("modal-target", "Modal Target", "Sorcery", "");
    definition.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "chosenModes",
                "kind": "chooseModes",
                "minimum": 1,
                "maximum": 1,
                "options": ["opponent", "player"]
            }, {
                "id": "targetOpponent",
                "kind": "chooseTargets",
                "condition": {
                    "kind": "selectionContains",
                    "selection": {
                        "kind": "decisionResult",
                        "decisionId": "chosenModes"
                    },
                    "value": "opponent"
                },
                "minimum": 1,
                "maximum": 1,
                "candidates": {
                    "kind": "players",
                    "where": {
                        "kind": "isOpponentOf",
                        "player": {
                            "kind": "controllerOf",
                            "object": { "kind": "self" }
                        }
                    }
                }
            }, {
                "id": "targetPlayer",
                "kind": "chooseTargets",
                "condition": {
                    "kind": "selectionContains",
                    "selection": {
                        "kind": "decisionResult",
                        "decisionId": "chosenModes"
                    },
                    "value": "player"
                },
                "minimum": 1,
                "maximum": 1,
                "candidates": { "kind": "players" }
            }]
        },
        "effects": [{
            "kind": "gainLife",
            "player": {
                "kind": "controllerOf",
                "object": { "kind": "self" }
            },
            "amount": { "kind": "integer", "value": 1 }
        }]
    })];
    definition
}

fn setup(players: Vec<PlayerDeck>, opening_hand_size: usize) -> GameSetup {
    GameSetup {
        opening_hand_size,
        players,
        starting_player: 0,
    }
}

fn human_request(setup: GameSetup, seed: u64) -> CreateGameSessionRequest {
    CreateGameSessionRequest {
        ai_controller_by_player_id: Default::default(),
        analytics_deck_session_by_player_id: Default::default(),
        analytics_context_id: None,
        analytics_pilot_by_player_id: Default::default(),
        punching_bag_player_ids: Vec::new(),
        opening_hand_selection_pool_size_by_player_id: Default::default(),
        training_anchor_deadline_round_by_player_id: Default::default(),
        free_mulligans: 0,
        human_decision_timeout_ms: None,
        max_mulligans: None,
        wait_timeout_ms: 30_000,
        game_mode: Default::default(),
        combat_declaration_revision_player_ids: None,
        hold_priority_player_ids: Vec::new(),
        human_player_ids: setup
            .players
            .iter()
            .map(|player| player.id.clone())
            .collect(),
        max_turns: 20,
        mulligan_enabled: false,
        seed,
        setup,
    }
}

fn decision(view: &GameSessionView) -> &mtg_engine::engine::EngineDecisionRequest {
    view.decision
        .as_ref()
        .unwrap_or_else(|| panic!("session {} should await a decision", view.session_id))
}

fn action(view: &GameSessionView, predicate: impl Fn(&LegalAction) -> bool) -> LegalAction {
    decision(view)
        .options
        .iter()
        .find(|action| predicate(action))
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "no matching action in {:?}",
                decision(view)
                    .options
                    .iter()
                    .map(|action| (&action.kind, &action.label))
                    .collect::<Vec<_>>()
            )
        })
}

fn submit(
    manager: &GameSessionManager,
    view: &GameSessionView,
    action: &LegalAction,
) -> GameSessionView {
    manager
        .submit(
            &view.session_id,
            SubmitGameSessionAction {
                action_id: action.id.clone(),
                card_instance_ids: None,
                card_name: None,
                decision_id: decision(view).id.clone(),
                number_value: None,
                revision: view.revision,
            },
        )
        .expect("offered session action resumes the game")
}

/// Feature: Analytics observation preserves direct interactive card selections.
#[test]
fn session_moves_starfield_shepherds_selected_card_to_hand() {
    let manager = GameSessionManager::new();
    let first_cards = (0..8)
        .map(|index| starfield_shepherd(&format!("starfield-{index}")))
        .collect();
    let second_cards = (0..8)
        .map(|index| {
            card(
                &format!("wastes-{index}"),
                "Wastes",
                "Basic Land - Wastes",
                "",
            )
        })
        .collect();
    let mut view = manager
        .create(human_request(
            setup(
                vec![
                    PlayerDeck {
                        cards: first_cards,
                        id: "one".to_string(),
                        name: "One".to_string(),
                        starting_life: 20,
                    },
                    PlayerDeck {
                        cards: second_cards,
                        id: "two".to_string(),
                        name: "Two".to_string(),
                        starting_life: 20,
                    },
                ],
                7,
            ),
            9_002,
        ))
        .expect("Starfield session starts");
    let cast = action(&view, |action| action.kind == ActionKind::CastSpell);
    view = submit(&manager, &view, &cast);

    for _ in 0..8 {
        if decision(&view).kind == DecisionKind::ResolutionChoice {
            break;
        }
        let pass = action(&view, |action| action.kind == ActionKind::PassPriority);
        view = submit(&manager, &view, &pass);
    }

    assert_eq!(decision(&view).kind, DecisionKind::ResolutionChoice);
    let candidate = match decision(&view).choice.as_ref() {
        Some(mtg_engine::engine::DecisionChoice::CardSelection {
            candidate_card_instance_ids,
            ..
        }) => candidate_card_instance_ids
            .first()
            .cloned()
            .expect("the library contains a legal creature"),
        choice => panic!("expected a card selection, got {choice:?}"),
    };
    let selection_action = decision(&view)
        .options
        .first()
        .expect("the selection has an engine action");
    view = manager
        .submit(
            &view.session_id,
            SubmitGameSessionAction {
                action_id: selection_action.id.clone(),
                card_instance_ids: Some(vec![candidate.clone()]),
                card_name: None,
                decision_id: decision(&view).id.clone(),
                number_value: None,
                revision: view.revision,
            },
        )
        .expect("the selected creature resumes the trigger");

    let player = view
        .state
        .players
        .iter()
        .find(|player| player.id == "one")
        .expect("the searching player remains in the game");
    assert!(player.hand.iter().any(|card| card.instance_id == candidate));
    assert!(
        player
            .library
            .iter()
            .all(|card| card.instance_id != candidate)
    );
}

/// Feature: An authoritative session never offers Flashback without a legal graveyard target.
#[test]
fn session_filters_required_zone_targets_before_publishing_a_human_decision() {
    let manager = GameSessionManager::new();
    let mut first_cards = vec![card(
        "mountain-one",
        "Mountain",
        "Basic Land - Mountain",
        "",
    )];
    first_cards.extend((0..7).map(flashback_card));
    let second_cards = (0..8)
        .map(|index| {
            card(
                &format!("wastes-{index}"),
                "Wastes",
                "Basic Land - Wastes",
                "",
            )
        })
        .collect();
    let view = manager
        .create(human_request(
            setup(
                vec![
                    PlayerDeck {
                        cards: first_cards,
                        id: "one".to_string(),
                        name: "One".to_string(),
                        starting_life: 20,
                    },
                    PlayerDeck {
                        cards: second_cards,
                        id: "two".to_string(),
                        name: "Two".to_string(),
                        starting_life: 20,
                    },
                ],
                8,
            ),
            7,
        ))
        .expect("session starts");

    assert_eq!(decision(&view).player_id, "one");
    assert!(
        decision(&view)
            .options
            .iter()
            .any(|action| action.kind == ActionKind::PlayLand)
    );
    assert!(decision(&view).options.iter().all(|action| {
        action.kind != ActionKind::CastSpell
            || action
                .card_instance_id
                .as_deref()
                .is_none_or(|id| !id.contains("flashback"))
    }));
}

/// Feature: An instant response activates and pays its mana source during another player's turn.
#[test]
fn session_publishes_instant_responses_during_clockwise_priority() {
    let manager = GameSessionManager::new();
    let player = |id: &str| {
        let mut cards = (0..5)
            .map(|index| {
                card(
                    &format!("{id}-mountain-{index}"),
                    "Mountain",
                    "Basic Land - Mountain",
                    "",
                )
            })
            .collect::<Vec<_>>();
        cards.extend((0..5).map(|index| priority_instant(&format!("{id}-instant-{index}"))));
        PlayerDeck {
            cards,
            id: id.to_string(),
            name: id.to_string(),
            starting_life: 20,
        }
    };
    let mut view = manager
        .create(human_request(
            setup(vec![player("one"), player("two")], 9),
            11,
        ))
        .expect("session starts");

    let first_land = action(&view, |action| action.kind == ActionKind::PlayLand);
    view = submit(&manager, &view, &first_land);
    while decision(&view).player_id == "one" {
        let next = action(&view, |action| {
            action.kind == ActionKind::PassPriority || action.kind == ActionKind::Discard
        });
        view = submit(&manager, &view, &next);
    }

    assert_eq!(decision(&view).player_id, "two");
    let second_land = action(&view, |action| action.kind == ActionKind::PlayLand);
    view = submit(&manager, &view, &second_land);
    let spell = action(&view, |action| {
        action.kind == ActionKind::CastSpell && action.label.contains("Priority Bolt")
    });
    view = submit(&manager, &view, &spell);

    assert_eq!(view.state.stack.len(), 1);
    assert_eq!(decision(&view).player_id, "one");
    let response = action(&view, |action| {
        action.kind == ActionKind::CastSpell && action.label.contains("Priority Bolt")
    });
    assert_eq!(response.payment_sources.len(), 1);
    let payment_source_id = response.payment_sources[0].clone();

    view = submit(&manager, &view, &response);

    let responder = view
        .state
        .players
        .iter()
        .find(|player| player.id == "one")
        .expect("responding player remains in the game");
    assert!(
        responder
            .battlefield
            .iter()
            .find(|card| card.instance_id == payment_source_id)
            .expect("the selected mana source remains on the battlefield")
            .tapped,
        "casting during another player's turn must tap its selected mana source"
    );
    assert!(view.state.events.iter().any(|event| {
        event.kind == "manaAbilityActivated"
            && event.player_id.as_deref() == Some("one")
            && event.card_instance_id.as_deref() == Some(payment_source_id.as_str())
    }));
}

/// Feature: Every session cast option is a complete engine-owned casting declaration.
#[test]
fn session_carries_modes_targets_and_payment_in_the_offered_cast_action() {
    let manager = GameSessionManager::new();
    let mut first_cards = vec![modal_target_spell()];
    first_cards.extend((0..7).map(|index| {
        card(
            &format!("mountain-{index}"),
            "Mountain",
            "Basic Land - Mountain",
            "",
        )
    }));
    let second_cards = (0..8)
        .map(|index| {
            card(
                &format!("forest-{index}"),
                "Forest",
                "Basic Land - Forest",
                "",
            )
        })
        .collect();
    let view = manager
        .create(human_request(
            setup(
                vec![
                    PlayerDeck {
                        cards: first_cards,
                        id: "one".to_string(),
                        name: "One".to_string(),
                        starting_life: 20,
                    },
                    PlayerDeck {
                        cards: second_cards,
                        id: "two".to_string(),
                        name: "Two".to_string(),
                        starting_life: 20,
                    },
                ],
                8,
            ),
            17,
        ))
        .expect("session starts");

    let cast_actions = decision(&view)
        .options
        .iter()
        .filter(|action| {
            action.kind == ActionKind::CastSpell
                && action
                    .card_instance_id
                    .as_deref()
                    .is_some_and(|id| id.contains("modal-target"))
        })
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(cast_actions.len(), 3);
    assert!(cast_actions.iter().all(|action| {
        action.decisions["chosenModes"]
            .as_array()
            .is_some_and(|modes| modes.len() == 1)
            && action.targets.len() == 1
    }));

    let selected = cast_actions
        .iter()
        .find(|action| action.decisions["chosenModes"] == json!(["opponent"]))
        .expect("opponent mode is available")
        .clone();
    let next = submit(&manager, &view, &selected);
    let cast_event = next
        .state
        .events
        .iter()
        .find(|event| {
            event.kind == "spellCast"
                && event.card_instance_id.as_deref() == selected.card_instance_id.as_deref()
        })
        .expect("selected declaration is recorded when the spell is cast");

    assert_eq!(cast_event.detail["decisions"], json!(selected.decisions));
    assert_eq!(cast_event.detail["targets"], json!(selected.targets));
}

/// Feature: Opening-hand choices are owned by the same resumable Rust session.
#[test]
fn session_runs_london_mulligan_and_bottom_choices_before_the_first_turn() {
    let manager = GameSessionManager::new();
    let player = |id: &str, land: &str| PlayerDeck {
        cards: (0..8)
            .map(|index| {
                card(
                    &format!("{id}-{index}"),
                    land,
                    &format!("Basic Land - {land}"),
                    "",
                )
            })
            .collect(),
        id: id.to_string(),
        name: id.to_string(),
        starting_life: 20,
    };
    let mut request = human_request(
        setup(vec![player("one", "Mountain"), player("two", "Forest")], 7),
        23,
    );
    request.mulligan_enabled = true;
    let mut view = manager.create(request).expect("mulligan session starts");

    assert_eq!(decision(&view).kind, DecisionKind::Mulligan);
    let take = action(&view, |action| action.kind == ActionKind::TakeMulligan);
    view = submit(&manager, &view, &take);
    assert_eq!(view.state.players[0].hand.len(), 7);
    assert_eq!(decision(&view).kind, DecisionKind::Mulligan);

    let keep = action(&view, |action| action.kind == ActionKind::KeepHand);
    view = submit(&manager, &view, &keep);
    assert_eq!(decision(&view).kind, DecisionKind::MulliganBottom);
    assert_eq!(
        decision(&view)
            .options
            .iter()
            .filter(|action| action.kind == ActionKind::BottomCard)
            .count(),
        7
    );

    let bottom = action(&view, |action| action.kind == ActionKind::BottomCard);
    view = submit(&manager, &view, &bottom);
    assert_eq!(view.state.players[0].hand.len(), 6);
    assert_eq!(view.state.players[0].library.len(), 2);
    assert_ne!(decision(&view).kind, DecisionKind::MulliganBottom);
}

/// Feature: Hold priority exposes deliberate mana actions alongside an explicit pass.
#[test]
fn session_can_pause_for_manual_mana_actions_and_an_explicit_pass() {
    let manager = GameSessionManager::new();
    let player = |id: &str, land: &str| PlayerDeck {
        cards: (0..8)
            .map(|index| {
                card(
                    &format!("{id}-{index}"),
                    land,
                    &format!("Basic Land - {land}"),
                    "",
                )
            })
            .collect(),
        id: id.to_string(),
        name: id.to_string(),
        starting_life: 20,
    };
    let request = human_request(
        setup(vec![player("one", "Mountain"), player("two", "Forest")], 8),
        31,
    );
    let view = manager
        .create(request)
        .expect("hold-priority session starts");
    manager
        .update_settings(
            &view.session_id,
            UpdateGameSessionSettings {
                hold_priority_player_ids: vec!["one".to_string()],
            },
        )
        .expect("session priority setting updates");
    let land = action(&view, |action| action.kind == ActionKind::PlayLand);
    let held = submit(&manager, &view, &land);

    assert_eq!(held.state.turn_number, 1);
    assert_eq!(decision(&held).player_id, "one");
    assert!(
        decision(&held)
            .options
            .iter()
            .any(|action| action.kind == ActionKind::PassPriority)
    );
    assert!(decision(&held).options.iter().any(|action| {
        action.kind == ActionKind::ActivateAbility
            && action.decisions.get("manaAbility") == Some(&json!(true))
    }));
}

/// Feature: One permanent publishes its mana and costed non-mana abilities distinctly.
#[test]
fn session_labels_every_legal_action_for_a_multi_ability_land() {
    let manager = GameSessionManager::new();
    let mut first_cards = vec![multi_action_mana_land()];
    first_cards.extend((0..7).map(|index| {
        card(
            &format!("multi-action-filler-{index}"),
            "Multi-action Filler",
            "Artifact",
            "{99}",
        )
    }));
    let second_cards = (0..8)
        .map(|index| {
            card(
                &format!("multi-action-wastes-{index}"),
                "Wastes",
                "Basic Land - Wastes",
                "",
            )
        })
        .collect();
    let mut request = human_request(
        setup(
            vec![
                PlayerDeck {
                    cards: first_cards,
                    id: "one".to_string(),
                    name: "one".to_string(),
                    starting_life: 20,
                },
                PlayerDeck {
                    cards: second_cards,
                    id: "two".to_string(),
                    name: "two".to_string(),
                    starting_life: 20,
                },
            ],
            8,
        ),
        53,
    );
    request.hold_priority_player_ids = vec!["one".to_string()];
    let view = manager
        .create(request)
        .expect("multi-ability land session starts");
    let land = action(&view, |action| {
        action.kind == ActionKind::PlayLand
            && action
                .card_instance_id
                .as_deref()
                .is_some_and(|id| id.contains("multi-action-mana-land"))
    });
    let held = submit(&manager, &view, &land);
    let actions = decision(&held)
        .options
        .iter()
        .filter(|action| action.card_instance_id == land.card_instance_id)
        .collect::<Vec<_>>();

    assert!(actions.iter().any(|action| {
        action.decisions.get("manaAbility") == Some(&json!(true))
            && action.label == "Tap Costed Hall: Add {C}{C}{C}{C}{C}"
    }));
    assert!(actions.iter().any(|action| {
        action.decisions.get("manaAbility") != Some(&json!(true))
            && action.label == "Activate Costed Hall ({5})"
    }));
}

/// Feature: Floating mana resolves off-stack and pays a later action in the same step.
#[test]
fn manual_mana_ability_adds_to_and_spends_the_authoritative_pool() {
    let manager = GameSessionManager::new();
    let mut first_cards = vec![
        card("pool-mountain", "Mountain", "Basic Land - Mountain", ""),
        priority_instant("pool-instant"),
    ];
    first_cards.extend((0..6).map(|index| {
        card(
            &format!("pool-filler-{index}"),
            "Pool Filler",
            "Artifact",
            "{99}",
        )
    }));
    let second_cards = (0..8)
        .map(|index| {
            card(
                &format!("pool-wastes-{index}"),
                "Wastes",
                "Basic Land - Wastes",
                "",
            )
        })
        .collect();
    let mut request = human_request(
        setup(
            vec![
                PlayerDeck {
                    cards: first_cards,
                    id: "one".to_string(),
                    name: "one".to_string(),
                    starting_life: 20,
                },
                PlayerDeck {
                    cards: second_cards,
                    id: "two".to_string(),
                    name: "two".to_string(),
                    starting_life: 20,
                },
            ],
            8,
        ),
        47,
    );
    request.hold_priority_player_ids = vec!["one".to_string()];
    let mut view = manager.create(request).expect("manual-mana session starts");

    let land = action(&view, |action| {
        action.kind == ActionKind::PlayLand
            && action
                .card_instance_id
                .as_deref()
                .is_some_and(|id| id.contains("pool-mountain"))
    });
    view = submit(&manager, &view, &land);
    let mana = action(&view, |action| {
        action.kind == ActionKind::ActivateAbility
            && action.card_instance_id == land.card_instance_id
            && action.decisions.get("manaAbility") == Some(&json!(true))
    });
    view = submit(&manager, &view, &mana);

    assert!(
        view.state.stack.is_empty(),
        "mana abilities do not use the stack"
    );
    assert_eq!(view.state.players[0].mana_pool.len(), 1);
    assert_eq!(view.state.players[0].mana_pool[0].symbol, "R");
    assert!(view.state.players[0].battlefield[0].tapped);

    let cast = action(&view, |action| {
        action.kind == ActionKind::CastSpell
            && action
                .card_instance_id
                .as_deref()
                .is_some_and(|id| id.contains("pool-instant"))
    });
    assert!(cast.payment_sources.is_empty());
    assert_eq!(cast.decisions["manaPoolPayment"], json!([0]));
    view = submit(&manager, &view, &cast);

    assert!(view.state.players[0].mana_pool.is_empty());
    assert_eq!(view.state.stack.len(), 1);
}
