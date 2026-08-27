use mtg_engine::engine::{
    ActionKind, CardDefinition, DecisionKind, DecisionProvider, EngineDecisionRequest, EngineError,
    GameEndReason, GameEngine, GameMode, GameSetup, GameStep, PlayerDeck, RandomSimulationRequest,
    TargetRef, simulate_random_games,
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

fn inert_deck(player_id: &str, card_count: usize) -> PlayerDeck {
    PlayerDeck {
        cards: (0..card_count)
            .map(|index| {
                card(
                    &format!("{player_id}-land-{index}"),
                    "Wastes",
                    "Basic Land - Wastes",
                    "",
                )
            })
            .collect(),
        id: player_id.to_string(),
        name: player_id.to_string(),
        starting_life: 20,
    }
}

fn inert_commander_deck(player_id: &str) -> PlayerDeck {
    let mut deck = inert_deck(player_id, 99);
    let mut commander = card(
        &format!("{player_id}-commander"),
        "Test Commander",
        "Legendary Creature - Avatar",
        "{1}",
    );
    commander.is_commander = true;
    deck.cards.push(commander);
    deck
}

fn lethal_spell(index: usize, player_id: &str) -> CardDefinition {
    let mut spell = card(
        &format!("{player_id}-lethal-{index}"),
        "Lethal Test Spell",
        "Instant",
        "",
    );
    spell.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "targetPlayer",
                "kind": "chooseTargets",
                "minimum": 1,
                "maximum": 1,
                "candidates": { "kind": "players" }
            }]
        },
        "effects": [{
            "kind": "dealDamage",
            "source": { "kind": "self" },
            "amount": { "kind": "integer", "value": 20 },
            "recipient": { "kind": "chosenTarget", "id": "targetPlayer" }
        }]
    })];
    spell
}

fn draw_then_lethal_spell(index: usize) -> CardDefinition {
    let mut spell = card(
        &format!("draw-then-lethal-{index}"),
        "Mutual Loss",
        "Sorcery",
        "",
    );
    spell.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "targetPlayer",
                "kind": "chooseTargets",
                "minimum": 1,
                "maximum": 1,
                "candidates": { "kind": "players" }
            }]
        },
        "effects": [{
            "kind": "drawCards",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "count": 1
        }, {
            "kind": "dealDamage",
            "source": { "kind": "self" },
            "amount": { "kind": "integer", "value": 20 },
            "recipient": { "kind": "chosenTarget", "id": "targetPlayer" }
        }]
    })];
    spell
}

fn token_spell(index: usize) -> CardDefinition {
    let mut spell = card(
        &format!("token-spell-{index}"),
        "Call a Soldier",
        "Sorcery",
        "",
    );
    spell.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": []
        },
        "effects": [{
            "kind": "createTokens",
            "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
            "quantity": { "kind": "integer", "value": 1 },
            "token": {
                "kind": "tokenDefinition",
                "name": "Soldier",
                "types": ["Creature"],
                "subtypes": ["Soldier"],
                "power": 1,
                "toughness": 1,
                "abilities": []
            }
        }]
    })];
    spell
}

fn prowess_token_spell() -> CardDefinition {
    let mut spell = card("prowess-token-spell", "Call a Monk", "Sorcery", "");
    spell.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [{
            "kind": "createTokens",
            "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
            "quantity": 1,
            "token": {
                "colors": ["White"],
                "types": ["Creature"],
                "subtypes": ["Monk"],
                "power": 1,
                "toughness": 1,
                "abilities": [{ "kind": "prowess" }]
            }
        }]
    })];
    spell
}

fn empty_instant() -> CardDefinition {
    let mut spell = card("empty-instant", "Empty Instant", "Instant", "");
    spell.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": []
    })];
    spell
}

fn flashback_grant_spell() -> CardDefinition {
    let mut spell = card("flashback-grant", "Grant Flashback", "Sorcery", "");
    spell.rules = vec![json!({
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
    spell
}

fn improvisation_capstone_spell() -> CardDefinition {
    let mut spell = card(
        "improvisation-capstone",
        "Improvisation Capstone",
        "Sorcery - Lesson",
        "",
    );
    spell.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [{
            "kind": "exileFromTopUntil",
            "zone": {
                "kind": "library",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } }
            },
            "bind": "exiledCards",
            "faceDown": false,
            "stopWhen": {
                "kind": "compare",
                "operator": ">=",
                "left": {
                    "kind": "sumManaValues",
                    "objects": { "kind": "boundObjects", "binding": "exiledCards" },
                    "variableManaSymbolsEqual": 0
                },
                "right": { "kind": "integer", "value": 4 }
            },
            "alsoStopsWhen": { "kind": "sourceZoneEmpty" }
        }, {
            "kind": "castAnyNumber",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "cards": { "kind": "boundObjects", "binding": "exiledCards" },
            "where": { "kind": "canBeCastAsSpell" },
            "timing": { "kind": "duringResolution" },
            "withoutPayingManaCost": true,
            "alternativeCostsAllowed": false,
            "additionalCostsApply": true,
            "variableManaValue": 0
        }]
    })];
    spell
}

fn paradigm_spell() -> CardDefinition {
    let mut spell = card(
        "paradigm-spell",
        "Repeatable Lesson",
        "Sorcery - Lesson",
        "",
    );
    spell.rules = vec![
        json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": []
        }),
        json!({
            "kind": "keywordAbility",
            "source": { "kind": "self" },
            "ability": {
                "kind": "paradigm",
                "spellName": "Repeatable Lesson"
            }
        }),
    ];
    spell
}

fn four_mana_life_spell() -> CardDefinition {
    let mut spell = card("four-mana-life-spell", "Four Mana Lesson", "Sorcery", "{4}");
    spell.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [{
            "kind": "gainLife",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "amount": { "kind": "integer", "value": 3 }
        }]
    })];
    spell
}

fn abrade_modal_spell() -> CardDefinition {
    let mut spell = card("abrade", "Abrade", "Instant", "");
    spell.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "chosenModes",
                "kind": "chooseModes",
                "minimum": 1,
                "maximum": 1,
                "options": ["damageCreature", "destroyArtifact"]
            }, {
                "id": "targetCreature",
                "kind": "chooseTargets",
                "condition": {
                    "kind": "selectionContains",
                    "selection": {
                        "kind": "decisionResult",
                        "decisionId": "chosenModes"
                    },
                    "value": "damageCreature"
                },
                "minimum": 1,
                "maximum": 1,
                "candidates": {
                    "kind": "permanents",
                    "where": { "kind": "cardTypeContains", "value": "Creature" }
                }
            }, {
                "id": "targetArtifact",
                "kind": "chooseTargets",
                "condition": {
                    "kind": "selectionContains",
                    "selection": {
                        "kind": "decisionResult",
                        "decisionId": "chosenModes"
                    },
                    "value": "destroyArtifact"
                },
                "minimum": 1,
                "maximum": 1,
                "candidates": {
                    "kind": "permanents",
                    "where": { "kind": "cardTypeContains", "value": "Artifact" }
                }
            }]
        },
        "effects": [{
            "kind": "conditional",
            "condition": {
                "kind": "selectionContains",
                "selection": {
                    "kind": "decisionResult",
                    "decisionId": "chosenModes"
                },
                "value": "damageCreature"
            },
            "then": [{
                "kind": "dealDamage",
                "source": { "kind": "self" },
                "amount": { "kind": "integer", "value": 3 },
                "recipient": { "kind": "chosenTarget", "id": "targetCreature" }
            }]
        }, {
            "kind": "conditional",
            "condition": {
                "kind": "selectionContains",
                "selection": {
                    "kind": "decisionResult",
                    "decisionId": "chosenModes"
                },
                "value": "destroyArtifact"
            },
            "then": [{
                "kind": "destroyPermanent",
                "permanent": { "kind": "chosenTarget", "id": "targetArtifact" }
            }]
        }]
    })];
    spell
}

fn lethal_deck(player_id: &str) -> PlayerDeck {
    PlayerDeck {
        cards: (0..12)
            .map(|index| lethal_spell(index, player_id))
            .collect(),
        id: player_id.to_string(),
        name: player_id.to_string(),
        starting_life: 20,
    }
}

fn creature_deck(player_id: &str) -> PlayerDeck {
    PlayerDeck {
        cards: (0..12)
            .map(|index| {
                let mut creature = card(
                    &format!("{player_id}-creature-{index}"),
                    "Engine Bear",
                    "Creature - Bear",
                    "",
                );
                creature.power = Some("2".to_string());
                creature.toughness = Some("2".to_string());
                creature
            })
            .collect(),
        id: player_id.to_string(),
        name: player_id.to_string(),
        starting_life: 20,
    }
}

fn restricted_mana_land() -> CardDefinition {
    let mut land = card("restricted-mana-land", "Restricted Mana Land", "Land", "");
    land.rules = vec![
        json!({
            "kind": "manaAbility",
            "source": { "kind": "self" },
            "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
            "effects": [{
                "kind": "addMana",
                "mana": "{C}"
            }]
        }),
        json!({
            "kind": "manaAbility",
            "source": { "kind": "self" },
            "costs": [
                { "kind": "tap", "object": { "kind": "self" } },
                {
                    "kind": "payLife",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "amount": { "kind": "integer", "value": 1 }
                }
            ],
            "effects": [{
                "kind": "addMana",
                "mana": {
                    "kind": "chooseColor",
                    "amount": 1,
                    "spendRestriction": {
                        "kind": "castSpell",
                        "where": {
                            "kind": "or",
                            "operands": [
                                { "kind": "cardTypeContains", "value": "Instant" },
                                { "kind": "cardTypeContains", "value": "Sorcery" }
                            ]
                        }
                    }
                }
            }]
        }),
    ];
    land
}

fn mana_druid() -> CardDefinition {
    let mut creature = card("mana-druid", "Mana Druid", "Creature - Druid", "");
    creature.power = Some("1".to_string());
    creature.toughness = Some("1".to_string());
    creature.rules = vec![json!({
        "kind": "manaAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
        "effects": [{ "kind": "addMana", "mana": "{R}" }]
    })];
    creature
}

fn optional_life_land() -> CardDefinition {
    let mut land = card("optional-life-land", "Optional Life Land", "Land", "");
    land.rules = vec![json!({
        "kind": "replacementEffect",
        "source": { "kind": "self" },
        "event": {
            "kind": "wouldEnterBattlefield",
            "object": { "kind": "self" }
        },
        "decisions": [{
            "id": "payTwoLife",
            "kind": "chooseWhetherToPay",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "cost": {
                "kind": "payLife",
                "amount": { "kind": "integer", "value": 2 }
            }
        }],
        "replacement": [{
            "kind": "conditional",
            "condition": {
                "kind": "not",
                "operand": { "kind": "costWasPaid", "decisionId": "payTwoLife" }
            },
            "then": [{
                "kind": "setEnteringState",
                "object": { "kind": "self" },
                "tapped": true
            }]
        }]
    })];
    land
}

fn exile_and_drain_spell() -> CardDefinition {
    let mut spell = card("exile-and-drain", "Exile and Drain", "Instant", "");
    spell.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "targetPermanent",
                "kind": "chooseTargets",
                "minimum": 1,
                "maximum": 1,
                "candidates": {
                    "kind": "permanents",
                    "where": {
                        "kind": "not",
                        "operand": { "kind": "cardTypeContains", "value": "Land" }
                    }
                }
            }]
        },
        "effects": [
            {
                "kind": "bind",
                "id": "targetController",
                "value": {
                    "kind": "controllerOf",
                    "object": { "kind": "chosenTarget", "id": "targetPermanent" }
                }
            },
            {
                "kind": "exilePermanent",
                "permanent": { "kind": "chosenTarget", "id": "targetPermanent" }
            },
            {
                "kind": "loseLife",
                "player": { "kind": "boundValue", "id": "targetController" },
                "amount": { "kind": "integer", "value": 3 }
            },
            {
                "kind": "gainLife",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "amount": { "kind": "integer", "value": 3 }
            }
        ]
    })];
    spell
}

fn exile_token_spell(index: usize) -> CardDefinition {
    let mut spell = card(
        &format!("exile-token-{index}"),
        "Exile a Token",
        "Sorcery",
        "",
    );
    spell.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "targetPermanent",
                "kind": "chooseTargets",
                "minimum": 1,
                "maximum": 1,
                "candidates": {
                    "kind": "permanents",
                    "where": { "kind": "cardTypeContains", "value": "Creature" }
                }
            }]
        },
        "effects": [{
            "kind": "exilePermanent",
            "permanent": { "kind": "chosenTarget", "id": "targetPermanent" }
        }]
    })];
    spell
}

fn tiered_sweeper() -> CardDefinition {
    let mut spell = card("tiered-sweeper", "Tiered Sweeper", "Instant", "");
    spell.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "chosenModes",
                "kind": "chooseModes",
                "minimum": 1,
                "maximum": 1,
                "options": ["fire", "fira"]
            }],
            "additionalCosts": [{
                "kind": "conditional",
                "condition": {
                    "kind": "selectionContains",
                    "selection": {
                        "kind": "decisionResult",
                        "decisionId": "chosenModes"
                    },
                    "value": "fire"
                },
                "then": [{ "kind": "payMana", "manaCost": "{0}" }]
            }, {
                "kind": "conditional",
                "condition": {
                    "kind": "selectionContains",
                    "selection": {
                        "kind": "decisionResult",
                        "decisionId": "chosenModes"
                    },
                    "value": "fira"
                },
                "then": [{ "kind": "payMana", "manaCost": "{2}" }]
            }]
        },
        "effects": [{
            "kind": "conditional",
            "condition": {
                "kind": "selectionContains",
                "selection": {
                    "kind": "decisionResult",
                    "decisionId": "chosenModes"
                },
                "value": "fire"
            },
            "then": [{
                "kind": "dealDamage",
                "source": { "kind": "self" },
                "amount": { "kind": "integer", "value": 1 },
                "recipient": {
                    "kind": "eachPermanent",
                    "where": { "kind": "cardTypeContains", "value": "Creature" }
                }
            }]
        }, {
            "kind": "conditional",
            "condition": {
                "kind": "selectionContains",
                "selection": {
                    "kind": "decisionResult",
                    "decisionId": "chosenModes"
                },
                "value": "fira"
            },
            "then": [{
                "kind": "dealDamage",
                "source": { "kind": "self" },
                "amount": { "kind": "integer", "value": 2 },
                "recipient": {
                    "kind": "eachPermanent",
                    "where": { "kind": "cardTypeContains", "value": "Creature" }
                }
            }]
        }]
    })];
    spell
}

fn stock_up_spell(index: usize) -> CardDefinition {
    let mut spell = card(&format!("stock-up-{index}"), "Stock Up", "Sorcery", "");
    spell.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [{
            "kind": "lookAtTopCards",
            "zone": {
                "kind": "library",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } }
            },
            "count": { "kind": "integer", "value": 5 },
            "bind": "lookedCards"
        }, {
            "kind": "chooseCards",
            "id": "cardsForHand",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "from": { "kind": "boundObjects", "binding": "lookedCards" },
            "count": {
                "kind": "minimum",
                "operands": [{
                    "kind": "integer",
                    "value": 2
                }, {
                    "kind": "countObjects",
                    "objects": { "kind": "boundObjects", "binding": "lookedCards" }
                }]
            }
        }, {
            "kind": "moveCards",
            "cards": { "kind": "decisionResult", "decisionId": "cardsForHand" },
            "to": {
                "kind": "hand",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } }
            }
        }, {
            "kind": "chooseOrder",
            "id": "bottomOrder",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "objects": {
                "kind": "setDifference",
                "left": { "kind": "boundObjects", "binding": "lookedCards" },
                "right": { "kind": "decisionResult", "decisionId": "cardsForHand" }
            }
        }, {
            "kind": "moveCards",
            "cards": { "kind": "decisionResult", "decisionId": "bottomOrder" },
            "to": {
                "kind": "library",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "position": "bottom"
            },
            "order": { "kind": "decisionOrder", "decisionId": "bottomOrder" }
        }]
    })];
    spell
}

fn activated_exile_artifact(index: usize) -> CardDefinition {
    let mut artifact = card(
        &format!("activated-exile-{index}"),
        "Activated Exile",
        "Artifact",
        "",
    );
    artifact.rules = vec![json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
        "effects": [{
            "kind": "moveCard",
            "card": {
                "kind": "topCard",
                "zone": {
                    "kind": "library",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } }
                }
            },
            "to": { "kind": "exile" },
            "bind": "exiledCard"
        }, {
            "kind": "grantPermission",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "action": {
                "kind": "play",
                "card": { "kind": "boundObject", "binding": "exiledCard" },
                "normalTimingApplies": true,
                "normalCostsApply": true
            },
            "duration": {
                "kind": "untilEndOfNextTurn",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } }
            }
        }]
    })];
    artifact
}

fn triggered_mill_artifact(index: usize) -> CardDefinition {
    let mut artifact = card(
        &format!("triggered-mill-{index}"),
        "Triggered Mill",
        "Artifact",
        "",
    );
    artifact.rules = vec![json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "enterBattlefield", "object": { "kind": "self" } },
        "effects": [{
            "kind": "mill",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "count": 1,
            "bind": "milledCards"
        }, {
            "kind": "grantPermission",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "action": {
                "kind": "play",
                "card": { "kind": "singleBoundObject", "binding": "milledCards" },
                "normalTimingApplies": true,
                "normalCostsApply": true
            },
            "duration": { "kind": "untilEndOfCurrentTurn" }
        }]
    })];
    artifact
}

fn outside_game_creature() -> CardDefinition {
    let mut creature = card(
        "outside-game-creature",
        "Outside Game Creature",
        "Creature - Avatar",
        "",
    );
    creature.power = Some("5".to_string());
    creature.toughness = Some("5".to_string());
    creature.rules = vec![json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "enterBattlefield",
            "object": { "kind": "self" }
        },
        "condition": {
            "kind": "wasCast",
            "object": { "kind": "self" }
        },
        "effects": [{
            "kind": "chooseCards",
            "id": "outsideCard",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "minimum": 0,
            "maximum": 1,
            "candidates": {
                "kind": "cards",
                "zone": { "kind": "outsideGame" },
                "where": {
                    "kind": "ownedBy",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } }
                }
            }
        }, {
            "kind": "moveCards",
            "cards": { "kind": "decisionResult", "decisionId": "outsideCard" },
            "to": {
                "kind": "hand",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } }
            }
        }]
    })];
    creature
}

fn animated_trigger_land() -> CardDefinition {
    let mut land = card("animated-trigger-land", "Animated Hall", "Land", "");
    land.rules = vec![json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "payMana", "manaCost": "{0}" }],
        "effects": [{
            "kind": "conditional",
            "condition": {
                "kind": "not",
                "operand": {
                    "kind": "hasCardType",
                    "object": { "kind": "self" },
                    "value": "Creature"
                }
            },
            "then": [{
                "kind": "becomeCreature",
                "object": { "kind": "self" },
                "addTypes": ["Creature"],
                "addSubtypes": ["Wizard"],
                "basePower": 2,
                "baseToughness": 4,
                "retainExistingTypes": true,
                "duration": { "kind": "permanent" }
            }, {
                "kind": "grantAbility",
                "object": { "kind": "self" },
                "duration": { "kind": "permanent" },
                "ability": {
                    "kind": "triggeredAbility",
                    "event": {
                        "kind": "spellCast",
                        "player": {
                            "kind": "controllerOf",
                            "object": { "kind": "abilitySource" }
                        },
                        "where": {
                            "kind": "or",
                            "operands": [
                                { "kind": "cardTypeContains", "value": "Instant" },
                                { "kind": "cardTypeContains", "value": "Sorcery" }
                            ]
                        }
                    },
                    "effects": [{
                        "kind": "modifyPowerToughness",
                        "object": { "kind": "abilitySource" },
                        "power": 1,
                        "toughness": 0,
                        "duration": { "kind": "untilEndOfCurrentTurn" }
                    }]
                }
            }]
        }]
    })];
    land
}

fn counter_spell(index: usize) -> CardDefinition {
    let mut spell = card(
        &format!("counter-spell-{index}"),
        "Counter Spell",
        "Instant",
        "",
    );
    spell.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "targetSpell",
                "kind": "chooseTargets",
                "minimum": 1,
                "maximum": 1,
                "candidates": { "kind": "spells" }
            }]
        },
        "effects": [{
            "kind": "counterSpell",
            "spell": { "kind": "chosenTarget", "id": "targetSpell" }
        }]
    })];
    spell
}

fn setup(first: PlayerDeck, second: PlayerDeck) -> GameSetup {
    GameSetup {
        opening_hand_size: 7,
        players: vec![first, second],
        starting_player: 0,
    }
}

#[test]
fn training_skirmish_format_uses_short_initial_state() {
    let engine = GameEngine::new_with_mode(
        GameSetup {
            opening_hand_size: 5,
            players: vec![inert_deck("player-one", 60), inert_deck("player-two", 60)],
            starting_player: 0,
        },
        17,
        GameMode::Training,
    )
    .expect("training skirmish starts");
    let state = engine.state();

    assert_eq!(state.game_mode, GameMode::Training);
    assert_eq!(state.players[0].life, 5);
    assert_eq!(state.players[0].hand.len(), 5);
    assert_eq!(state.players[0].library.len(), 15);
}

#[test]
fn commander_format_uses_commander_starting_defaults() {
    let mut first = inert_commander_deck("player-one");
    first.starting_life = 1;
    let mut second = inert_commander_deck("player-two");
    second.starting_life = 1;
    let engine = GameEngine::new_with_mode(
        GameSetup {
            opening_hand_size: 7,
            players: vec![first, second],
            starting_player: 0,
        },
        18,
        GameMode::Commander,
    )
    .expect("commander game starts");
    let state = engine.state();

    assert_eq!(state.game_mode, GameMode::Commander);
    assert_eq!(state.players[0].life, 40);
    assert_eq!(state.players[0].hand.len(), 7);
    assert_eq!(state.players[0].library.len(), 92);
}

struct AggressiveProvider;

impl DecisionProvider for AggressiveProvider {
    fn choose(
        &mut self,
        _state: &mtg_engine::engine::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let preferred = match request.kind {
            DecisionKind::Priority => request
                .options
                .iter()
                .position(|option| option.kind != ActionKind::PassPriority),
            DecisionKind::Discard => request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::Discard),
            DecisionKind::Attackers => request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::DeclareAttacker),
            DecisionKind::Blockers => request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::FinishBlockers),
            DecisionKind::ReplacementChoice => request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::DeclinePayment),
            DecisionKind::OpeningHandSelection
            | DecisionKind::Mulligan
            | DecisionKind::MulliganBottom
            | DecisionKind::CombatDamage
            | DecisionKind::ResolutionChoice
            | DecisionKind::Sideboarding => Some(0),
        };
        Ok(preferred.unwrap_or(0))
    }
}

struct PayLifeProvider;

impl DecisionProvider for PayLifeProvider {
    fn choose(
        &mut self,
        _state: &mtg_engine::engine::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let preferred = match request.kind {
            DecisionKind::Priority => request
                .options
                .iter()
                .position(|option| option.kind != ActionKind::PassPriority),
            DecisionKind::Discard => request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::Discard),
            DecisionKind::Attackers => request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::FinishAttackers),
            DecisionKind::Blockers => request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::FinishBlockers),
            DecisionKind::ReplacementChoice => request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::PayLife),
            DecisionKind::OpeningHandSelection
            | DecisionKind::Mulligan
            | DecisionKind::MulliganBottom
            | DecisionKind::CombatDamage
            | DecisionKind::ResolutionChoice
            | DecisionKind::Sideboarding => Some(0),
        };
        Ok(preferred.unwrap_or(0))
    }
}

struct ActivePlayerProvider;

impl DecisionProvider for ActivePlayerProvider {
    fn choose(
        &mut self,
        state: &mtg_engine::engine::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let active_player_id = &state.players[state.active_player].id;
        let preferred = match request.kind {
            DecisionKind::Priority if request.player_id == *active_player_id => request
                .options
                .iter()
                .position(|option| option.kind != ActionKind::PassPriority),
            DecisionKind::Priority => request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::PassPriority),
            DecisionKind::Attackers => request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::FinishAttackers),
            DecisionKind::Blockers => request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::FinishBlockers),
            _ => None,
        };
        Ok(preferred.unwrap_or(0))
    }
}

#[derive(Default)]
struct CastOnceProvider {
    cast_started: bool,
}

#[derive(Default)]
struct CardChoiceRecordingProvider {
    card_choice: Option<serde_json::Value>,
    cast_started: bool,
    resolution_requests: Vec<serde_json::Value>,
}

#[derive(Default)]
struct CastAtOpponentOnceProvider {
    cast_started: bool,
}

#[derive(Default)]
struct CastOnceAndChooseOutsideProvider {
    cast_started: bool,
}

#[derive(Default)]
struct CastProwessSequenceProvider {
    creator_cast: bool,
    instant_cast: bool,
}

#[derive(Default)]
struct AnimateHallSequenceProvider {
    activation_started: bool,
    instant_cast: bool,
}

#[derive(Default)]
struct FlashbackSequenceProvider {
    first_cast: bool,
    grant_cast: bool,
    flashback_cast: bool,
}

#[derive(Default)]
struct CapstoneSequenceProvider {
    capstone_cast: bool,
    exiled_spell_cast: bool,
}

impl DecisionProvider for CapstoneSequenceProvider {
    fn choose(
        &mut self,
        _state: &mtg_engine::engine::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.kind == DecisionKind::Priority && !self.capstone_cast {
            if let Some(index) = request.options.iter().position(|option| {
                option.kind == ActionKind::CastSpell
                    && option
                        .card_instance_id
                        .as_deref()
                        .is_some_and(|id| id.contains("improvisation-capstone"))
            }) {
                self.capstone_cast = true;
                return Ok(index);
            }
        }
        if request.kind == DecisionKind::ResolutionChoice && !self.exiled_spell_cast {
            if let Some(index) = request.options.iter().position(|option| {
                option.kind == ActionKind::CastSpell
                    && option
                        .card_instance_id
                        .as_deref()
                        .is_some_and(|id| id.contains("four-mana-life-spell"))
            }) {
                self.exiled_spell_cast = true;
                return Ok(index);
            }
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
                        | ActionKind::ChooseResolution
                )
            })
            .unwrap_or(0))
    }
}

#[derive(Default)]
struct ParadigmSequenceProvider {
    original_cast: bool,
    copy_cast: bool,
}

struct CumulativeLifePaymentProvider;

impl DecisionProvider for CumulativeLifePaymentProvider {
    fn choose(
        &mut self,
        _state: &mtg_engine::engine::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.kind == DecisionKind::Priority && request.player_id == "player-one" {
            if let Some(index) = request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::PlayLand)
            {
                return Ok(index);
            }
            if let Some(index) = request.options.iter().position(|option| {
                option.kind == ActionKind::CastSpell
                    && option
                        .card_instance_id
                        .as_deref()
                        .is_some_and(|id| id.contains("double-red-spell"))
            }) {
                return Ok(index);
            }
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
struct AbilityTargetProbeProvider {
    artifact_cast: bool,
    ability_was_offered_as_spell: bool,
}

impl DecisionProvider for AbilityTargetProbeProvider {
    fn choose(
        &mut self,
        state: &mtg_engine::engine::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.kind == DecisionKind::Priority
            && request.player_id == "player-one"
            && !self.artifact_cast
            && let Some(index) = request.options.iter().position(|option| {
                option.kind == ActionKind::CastSpell
                    && option
                        .card_instance_id
                        .as_deref()
                        .is_some_and(|id| id.contains("triggered-mill"))
            })
        {
            self.artifact_cast = true;
            return Ok(index);
        }
        let trigger_source_is_on_battlefield = state.players[0]
            .battlefield
            .iter()
            .any(|card| card.definition.id.starts_with("triggered-mill"));
        if request.kind == DecisionKind::Priority
            && request.player_id == "player-two"
            && trigger_source_is_on_battlefield
            && state
                .stack
                .last()
                .is_some_and(|object| object.ability_rule.is_some())
            && let Some(index) = request.options.iter().position(|option| {
                option.kind == ActionKind::CastSpell
                    && option
                        .card_instance_id
                        .as_deref()
                        .is_some_and(|id| id.contains("counter-spell"))
            })
        {
            self.ability_was_offered_as_spell = true;
            return Ok(index);
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

impl DecisionProvider for ParadigmSequenceProvider {
    fn choose(
        &mut self,
        _state: &mtg_engine::engine::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.kind == DecisionKind::Priority && !self.original_cast {
            if let Some(index) = request.options.iter().position(|option| {
                option.kind == ActionKind::CastSpell
                    && option
                        .card_instance_id
                        .as_deref()
                        .is_some_and(|id| id.contains("paradigm-spell"))
            }) {
                self.original_cast = true;
                return Ok(index);
            }
        }
        if request.kind == DecisionKind::ResolutionChoice && !self.copy_cast {
            if let Some(index) = request.options.iter().position(|option| {
                option.kind == ActionKind::CastSpell && option.decisions["paradigmCopy"] == true
            }) {
                self.copy_cast = true;
                return Ok(index);
            }
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
                        | ActionKind::ChooseResolution
                )
            })
            .unwrap_or(0))
    }
}

impl DecisionProvider for FlashbackSequenceProvider {
    fn choose(
        &mut self,
        _state: &mtg_engine::engine::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.kind == DecisionKind::Priority {
            let desired = if !self.first_cast {
                Some(("empty-instant", "hand"))
            } else if !self.grant_cast {
                Some(("flashback-grant", "hand"))
            } else if !self.flashback_cast {
                Some(("empty-instant", "graveyard"))
            } else {
                None
            };
            if let Some((card_id, source_zone)) = desired
                && let Some(index) = request.options.iter().position(|option| {
                    option.kind == ActionKind::CastSpell
                        && option
                            .card_instance_id
                            .as_deref()
                            .is_some_and(|id| id.contains(card_id))
                        && option.decisions["castSourceZone"] == source_zone
                })
            {
                if !self.first_cast {
                    self.first_cast = true;
                } else if !self.grant_cast {
                    self.grant_cast = true;
                } else {
                    self.flashback_cast = true;
                }
                return Ok(index);
            }
            return Ok(request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::PassPriority)
                .unwrap_or(0));
        }
        Ok(request
            .options
            .iter()
            .position(|option| {
                matches!(
                    option.kind,
                    ActionKind::FinishAttackers | ActionKind::FinishBlockers
                )
            })
            .unwrap_or(0))
    }
}

impl DecisionProvider for AnimateHallSequenceProvider {
    fn choose(
        &mut self,
        state: &mtg_engine::engine::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.kind == DecisionKind::Priority {
            let hall = state.players[0]
                .battlefield
                .iter()
                .find(|card| card.definition.id == "animated-trigger-land");
            let desired = if hall.is_none() {
                request.options.iter().position(|option| {
                    option.kind == ActionKind::PlayLand
                        && option
                            .card_instance_id
                            .as_deref()
                            .is_some_and(|id| id.contains("animated-trigger-land"))
                })
            } else if !self.activation_started {
                request.options.iter().position(|option| {
                    option.kind == ActionKind::ActivateAbility
                        && option
                            .card_instance_id
                            .as_deref()
                            .is_some_and(|id| id.contains("animated-trigger-land"))
                })
            } else if hall.is_some_and(|card| card.definition.type_line.contains("Creature"))
                && !self.instant_cast
            {
                request.options.iter().position(|option| {
                    option.kind == ActionKind::CastSpell
                        && option
                            .card_instance_id
                            .as_deref()
                            .is_some_and(|id| id.contains("empty-instant"))
                })
            } else {
                None
            };
            if let Some(index) = desired {
                match request.options[index].kind {
                    ActionKind::ActivateAbility => self.activation_started = true,
                    ActionKind::CastSpell => self.instant_cast = true,
                    _ => {}
                }
                return Ok(index);
            }
            return Ok(request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::PassPriority)
                .unwrap_or(0));
        }
        Ok(request
            .options
            .iter()
            .position(|option| {
                matches!(
                    option.kind,
                    ActionKind::FinishAttackers | ActionKind::FinishBlockers
                )
            })
            .unwrap_or(0))
    }
}

impl DecisionProvider for CastProwessSequenceProvider {
    fn choose(
        &mut self,
        state: &mtg_engine::engine::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.kind == DecisionKind::Priority {
            let token_exists = state.players.iter().any(|player| {
                player
                    .battlefield
                    .iter()
                    .any(|card| card.definition.is_token)
            });
            let desired_id = if !self.creator_cast {
                Some("prowess-token-spell")
            } else if token_exists && !self.instant_cast {
                Some("empty-instant")
            } else {
                None
            };
            if let Some(desired_id) = desired_id
                && let Some(index) = request.options.iter().position(|option| {
                    option.kind == ActionKind::CastSpell
                        && option
                            .card_instance_id
                            .as_deref()
                            .is_some_and(|id| id.contains(desired_id))
                })
            {
                if desired_id == "prowess-token-spell" {
                    self.creator_cast = true;
                } else {
                    self.instant_cast = true;
                }
                return Ok(index);
            }
            return Ok(request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::PassPriority)
                .unwrap_or(0));
        }
        Ok(request
            .options
            .iter()
            .position(|option| {
                matches!(
                    option.kind,
                    ActionKind::FinishAttackers | ActionKind::FinishBlockers
                )
            })
            .unwrap_or(0))
    }
}

impl DecisionProvider for CastOnceAndChooseOutsideProvider {
    fn choose(
        &mut self,
        _state: &mtg_engine::engine::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        match request.kind {
            DecisionKind::Priority if !self.cast_started => {
                let cast = request
                    .options
                    .iter()
                    .position(|option| option.kind == ActionKind::CastSpell);
                if cast.is_some() {
                    self.cast_started = true;
                }
                Ok(cast.unwrap_or_else(|| {
                    request
                        .options
                        .iter()
                        .position(|option| option.kind == ActionKind::PassPriority)
                        .unwrap_or(0)
                }))
            }
            DecisionKind::Priority => Ok(request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::PassPriority)
                .unwrap_or(0)),
            DecisionKind::ResolutionChoice => Ok(request.options.len() - 1),
            _ => Ok(request
                .options
                .iter()
                .position(|option| {
                    matches!(
                        option.kind,
                        ActionKind::FinishAttackers | ActionKind::FinishBlockers
                    )
                })
                .unwrap_or(0)),
        }
    }
}

impl DecisionProvider for CastAtOpponentOnceProvider {
    fn choose(
        &mut self,
        _state: &mtg_engine::engine::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.kind == DecisionKind::Priority {
            if !self.cast_started
                && let Some(index) = request.options.iter().position(|option| {
                    option.kind == ActionKind::CastSpell
                        && option.targets.values().any(|target| {
                            matches!(
                                target,
                                TargetRef::Player { player_id } if player_id == "player-two"
                            )
                        })
                })
            {
                self.cast_started = true;
                return Ok(index);
            }
            return Ok(request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::PassPriority)
                .unwrap_or(0));
        }
        Ok(request
            .options
            .iter()
            .position(|option| {
                matches!(
                    option.kind,
                    ActionKind::FinishAttackers | ActionKind::FinishBlockers
                )
            })
            .unwrap_or(0))
    }
}

#[derive(Default)]
struct CounterInteractionProvider {
    counter_cast: bool,
    threat_cast: bool,
}

impl DecisionProvider for CounterInteractionProvider {
    fn choose(
        &mut self,
        _state: &mtg_engine::engine::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.kind == DecisionKind::Priority {
            if request.player_id == "player-one"
                && !self.threat_cast
                && let Some(index) = request
                    .options
                    .iter()
                    .position(|option| option.kind == ActionKind::CastSpell)
            {
                self.threat_cast = true;
                return Ok(index);
            }
            if request.player_id == "player-two"
                && !self.counter_cast
                && let Some(index) = request
                    .options
                    .iter()
                    .position(|option| option.kind == ActionKind::CastSpell)
            {
                self.counter_cast = true;
                return Ok(index);
            }
            return Ok(request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::PassPriority)
                .unwrap_or(0));
        }
        Ok(request
            .options
            .iter()
            .position(|option| {
                matches!(
                    option.kind,
                    ActionKind::FinishAttackers | ActionKind::FinishBlockers
                )
            })
            .unwrap_or(0))
    }
}

impl DecisionProvider for CastOnceProvider {
    fn choose(
        &mut self,
        _state: &mtg_engine::engine::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.kind == DecisionKind::Priority {
            if !self.cast_started
                && let Some(index) = request
                    .options
                    .iter()
                    .position(|option| option.kind == ActionKind::CastSpell)
            {
                self.cast_started = true;
                return Ok(index);
            }
            return Ok(request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::PassPriority)
                .unwrap_or(0));
        }
        Ok(0)
    }
}

impl DecisionProvider for CardChoiceRecordingProvider {
    fn choose(
        &mut self,
        _state: &mtg_engine::engine::GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.kind == DecisionKind::ResolutionChoice {
            self.resolution_requests
                .push(serde_json::to_value(request).expect("resolution request serializes"));
        }
        if request.kind == DecisionKind::ResolutionChoice && self.card_choice.is_none() {
            self.card_choice = Some(
                serde_json::to_value(request).expect("resolution request serializes")["choice"]
                    .clone(),
            );
            return Ok(0);
        }
        if request.kind == DecisionKind::Priority {
            if !self.cast_started
                && let Some(index) = request
                    .options
                    .iter()
                    .position(|option| option.kind == ActionKind::CastSpell)
            {
                self.cast_started = true;
                return Ok(index);
            }
            return Ok(request
                .options
                .iter()
                .position(|option| option.kind == ActionKind::PassPriority)
                .unwrap_or(0));
        }
        Ok(0)
    }
}

/// Feature: An empty library is harmless until that player attempts to draw.
#[test]
fn drawing_from_an_empty_library_ends_the_game() {
    let mut engine = GameEngine::new(
        setup(inert_deck("player-one", 7), inert_deck("player-two", 7)),
        11,
    )
    .expect("valid game setup");

    assert!(engine.state().outcome.is_none());
    assert!(engine.state().players[0].library.is_empty());

    engine
        .draw_cards("player-one", 1)
        .expect("draw instruction executes");

    let outcome = engine.state().outcome.as_ref().expect("game ended");
    assert_eq!(outcome.reason, GameEndReason::DrawFromEmptyLibrary);
    assert_eq!(outcome.winner.as_deref(), Some("player-two"));
    assert_eq!(outcome.losers, vec!["player-one"]);
}

/// Feature: State-based losses wait until the resolving spell has finished.
#[test]
fn failed_draw_and_lethal_damage_are_applied_simultaneously() {
    let player_one = PlayerDeck {
        cards: (0..7).map(draw_then_lethal_spell).collect(),
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(setup(player_one, inert_deck("player-two", 8)), 111)
        .expect("valid simultaneous-loss game");

    engine
        .run(&mut CastAtOpponentOnceProvider::default(), 1)
        .expect("mutual-loss spell resolves");

    let outcome = engine.state().outcome.as_ref().expect("game ended");
    assert_eq!(outcome.reason, GameEndReason::Simultaneous);
    assert_eq!(outcome.winner, None);
    assert_eq!(outcome.losers, vec!["player-one", "player-two"]);
}

/// Feature: A player at zero life loses and the opposing player wins.
#[test]
fn zero_life_ends_the_game() {
    let mut engine = GameEngine::new(
        setup(inert_deck("player-one", 8), inert_deck("player-two", 8)),
        12,
    )
    .expect("valid game setup");

    engine
        .lose_life("player-two", 20, "test")
        .expect("life loss executes");

    let outcome = engine.state().outcome.as_ref().expect("game ended");
    assert_eq!(engine.state().players[1].life, 0);
    assert_eq!(outcome.reason, GameEndReason::LifeTotal);
    assert_eq!(outcome.winner.as_deref(), Some("player-one"));
    assert_eq!(outcome.losers, vec!["player-two"]);
}

/// Feature: Precombat and postcombat main phases share one land play for the turn.
#[test]
fn land_play_resource_is_shared_by_both_main_phases() {
    let mut engine = GameEngine::new(
        setup(inert_deck("player-one", 8), inert_deck("player-two", 8)),
        13,
    )
    .expect("valid game setup");

    engine
        .run(&mut AggressiveProvider, 1)
        .expect("single turn executes");

    assert_eq!(engine.state().players[0].battlefield.len(), 1);
    assert_eq!(engine.state().players[0].land_plays_remaining, 0);
    assert_eq!(
        engine
            .state()
            .events
            .iter()
            .filter(|event| event.kind == "landPlayed")
            .count(),
        1,
    );
}

/// Feature: Untap simultaneously readies only the active player's permanents.
#[test]
fn untap_step_readies_only_the_active_players_permanents() {
    let tapped_land_deck = |player_id: &str| PlayerDeck {
        cards: (0..8)
            .map(|index| {
                let mut land = optional_life_land();
                land.id = format!("{player_id}-tapped-land-{index}");
                land.name = format!("{player_id} Tapped Land {index}");
                land
            })
            .collect(),
        id: player_id.to_string(),
        name: player_id.to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(
        setup(
            tapped_land_deck("player-one"),
            tapped_land_deck("player-two"),
        ),
        18,
    )
    .expect("valid untap game");

    engine
        .run(&mut AggressiveProvider, 3)
        .expect("three turns execute");

    let first_player = &engine.state().players[0];
    let second_player = &engine.state().players[1];
    assert_eq!(
        first_player
            .battlefield
            .iter()
            .filter(|permanent| permanent.tapped)
            .count(),
        1,
    );
    assert_eq!(
        first_player
            .battlefield
            .iter()
            .filter(|permanent| !permanent.tapped)
            .count(),
        1,
    );
    assert!(second_player.battlefield[0].tapped);

    let untap_event = engine
        .state()
        .events
        .iter()
        .find(|event| event.turn_number == 3 && event.kind == "permanentsUntapped")
        .expect("the untap action is recorded");
    assert_eq!(untap_event.step, GameStep::Untap);
    assert_eq!(untap_event.player_id.as_deref(), Some("player-one"));
    assert_eq!(untap_event.detail["count"], 1);
}

/// Feature: Token definitions supplied beside a deck never enter its library or opening hand.
#[test]
fn token_definitions_are_excluded_from_deck_zones() {
    let mut player_one = inert_deck("player-one", 7);
    player_one.cards.push(card(
        "soldier-token",
        "Soldier",
        "Token Creature - Soldier",
        "",
    ));
    let mut helper_card = card("initiative-helper", "The Initiative", "Card", "");
    helper_card.is_game_piece = true;
    player_one.cards.push(helper_card);
    let engine = GameEngine::new(setup(player_one, inert_deck("player-two", 7)), 14)
        .expect("tokens do not invalidate an otherwise valid deck");
    let player = &engine.state().players[0];

    assert_eq!(player.hand.len() + player.library.len(), 7);
    assert!(
        player
            .hand
            .iter()
            .chain(&player.library)
            .all(|card| !card.definition.type_line.starts_with("Token "))
    );
}

/// Feature: Sideboard entries remain outside the library and opening hand.
#[test]
fn sideboard_definitions_are_kept_in_the_sideboard_collection() {
    let mut player_one = inert_deck("player-one", 7);
    let mut sideboard_card = card("sideboard-answer", "Sideboard Answer", "Instant", "");
    sideboard_card.is_sideboard = true;
    player_one.cards.push(sideboard_card);
    let engine = GameEngine::new(setup(player_one, inert_deck("player-two", 7)), 16)
        .expect("sideboard cards do not invalidate an otherwise valid deck");
    let player = &engine.state().players[0];

    assert_eq!(player.hand.len() + player.library.len(), 7);
    assert_eq!(player.sideboard.len(), 1);
    assert_eq!(player.sideboard[0].definition.id, "sideboard-answer");
}

/// Feature: The decision provider chooses required cleanup discards.
#[test]
fn cleanup_requests_discards_until_the_active_players_hand_is_legal() {
    let expensive_deck = |player_id: &str| PlayerDeck {
        cards: (0..8)
            .map(|index| {
                card(
                    &format!("{player_id}-filler-{index}"),
                    "Expensive Filler",
                    "Artifact",
                    "{99}",
                )
            })
            .collect(),
        id: player_id.to_string(),
        name: player_id.to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(
        setup(expensive_deck("player-one"), expensive_deck("player-two")),
        17,
    )
    .expect("valid cleanup game");

    engine
        .run(&mut AggressiveProvider, 2)
        .expect("cleanup decisions execute");

    assert_eq!(engine.state().players[1].hand.len(), 7);
    assert_eq!(engine.state().players[1].graveyard.len(), 1);
    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| event.kind == "cardDiscarded")
    );
}

/// Feature: Cleanup reads the active player's card-modifiable maximum hand size.
#[test]
fn cleanup_skips_discard_when_hand_equals_the_players_modified_maximum() {
    let expensive_deck = |player_id: &str| PlayerDeck {
        cards: (0..8)
            .map(|index| {
                card(
                    &format!("{player_id}-filler-{index}"),
                    "Expensive Filler",
                    "Artifact",
                    "{99}",
                )
            })
            .collect(),
        id: player_id.to_string(),
        name: player_id.to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(
        setup(expensive_deck("player-one"), expensive_deck("player-two")),
        18,
    )
    .expect("valid configurable cleanup game");
    engine
        .set_max_hand_size("player-two", 8)
        .expect("the player's maximum hand size is configurable");

    engine
        .run(&mut AggressiveProvider, 2)
        .expect("cleanup without a discard executes");

    let player = &engine.state().players[1];
    assert_eq!(player.max_hand_size, 8);
    assert_eq!(player.hand.len(), 8);
    assert!(!engine.state().events.iter().any(|event| {
        event.kind == "cardDiscarded" && event.player_id.as_deref() == Some("player-two")
    }));
}

/// Feature: Token effects create new token permanents directly on the battlefield.
#[test]
fn create_token_effect_materializes_battlefield_permanents() {
    let player_one = PlayerDeck {
        cards: (0..7).map(token_spell).collect(),
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(setup(player_one, inert_deck("player-two", 7)), 15)
        .expect("valid token-producing game");

    engine
        .run(&mut AggressiveProvider, 1)
        .expect("token-producing turn executes");

    let player = &engine.state().players[0];
    assert_eq!(
        player
            .battlefield
            .iter()
            .filter(|permanent| permanent.definition.is_token)
            .count(),
        7
    );
    assert!(
        player
            .battlefield
            .iter()
            .filter(|permanent| permanent.definition.is_token)
            .all(|permanent| permanent.definition.is_game_piece)
    );
    assert!(
        player
            .hand
            .iter()
            .chain(&player.library)
            .all(|card| !card.definition.is_token)
    );
}

/// Feature: Prowess tokens trigger above later noncreature spells and reset at cleanup.
#[test]
fn prowess_token_uses_the_spell_cast_hook() {
    let mut cards = vec![prowess_token_spell(), empty_instant()];
    cards.extend((0..5).map(|index| {
        card(
            &format!("prowess-filler-{index}"),
            "Prowess Filler",
            "Artifact",
            "{99}",
        )
    }));
    let player_one = PlayerDeck {
        cards,
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(setup(player_one, inert_deck("player-two", 8)), 152)
        .expect("valid prowess game");

    engine
        .run(&mut CastProwessSequenceProvider::default(), 1)
        .expect("prowess sequence resolves");

    assert!(
        engine
            .state()
            .unsupported_rules
            .iter()
            .all(|rule| rule.card_id != "prowess-token-spell")
    );
    assert!(engine.state().events.iter().any(|event| {
        event.kind == "triggeredAbilityPutOnStack"
            && event
                .card_instance_id
                .as_deref()
                .is_some_and(|id| id.contains(":token:"))
    }));
    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| event.kind == "powerToughnessModified")
    );
    assert!(
        engine.state().players[0]
            .battlefield
            .iter()
            .filter(|card| card.definition.is_token)
            .all(|card| card.power_modifier == 0 && card.toughness_modifier == 0)
    );
}

/// Feature: A land can permanently become a creature and gain a spell-cast trigger.
#[test]
fn animated_land_grants_and_uses_a_triggered_ability() {
    let mut cards = vec![animated_trigger_land(), empty_instant()];
    cards.extend((0..5).map(|index| {
        card(
            &format!("animation-filler-{index}"),
            "Animation Filler",
            "Artifact",
            "{99}",
        )
    }));
    let player_one = PlayerDeck {
        cards,
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(setup(player_one, inert_deck("player-two", 8)), 153)
        .expect("valid animated-land game");

    engine
        .run(&mut AnimateHallSequenceProvider::default(), 1)
        .expect("animation sequence resolves");

    let hall = engine.state().players[0]
        .battlefield
        .iter()
        .find(|card| card.definition.id == "animated-trigger-land")
        .expect("hall remains on the battlefield");
    assert!(hall.definition.type_line.contains("Land"));
    assert!(hall.definition.type_line.contains("Creature"));
    assert!(hall.definition.type_line.contains("Wizard"));
    assert_eq!(hall.definition.power.as_deref(), Some("2"));
    assert_eq!(hall.definition.toughness.as_deref(), Some("4"));
    assert!(hall.summoning_sick);
    assert!(
        engine
            .state()
            .unsupported_rules
            .iter()
            .all(|rule| rule.card_id != "animated-trigger-land")
    );
    assert!(engine.state().events.iter().any(|event| {
        event.kind == "triggeredAbilityPutOnStack"
            && event
                .card_instance_id
                .as_deref()
                .is_some_and(|id| id.contains("animated-trigger-land"))
    }));
    assert!(engine.state().events.iter().any(|event| {
        event.kind == "powerToughnessModified"
            && event.detail["power"] == 1
            && event.detail["toughness"] == 0
    }));
}

/// Feature: A granted flashback permission recasts from graveyard and exiles afterward.
#[test]
fn temporary_flashback_grant_completes_its_zone_lifecycle() {
    let mut cards = vec![empty_instant(), flashback_grant_spell()];
    cards.extend((0..5).map(|index| {
        card(
            &format!("flashback-filler-{index}"),
            "Flashback Filler",
            "Artifact",
            "{99}",
        )
    }));
    let player_one = PlayerDeck {
        cards,
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(setup(player_one, inert_deck("player-two", 8)), 154)
        .expect("valid flashback game");

    engine
        .run(&mut FlashbackSequenceProvider::default(), 1)
        .expect("flashback sequence resolves");

    assert!(
        engine
            .state()
            .unsupported_rules
            .iter()
            .all(|rule| rule.card_id != "flashback-grant")
    );
    let empty_instant_casts = engine
        .state()
        .events
        .iter()
        .filter(|event| {
            event.kind == "spellCast"
                && event
                    .card_instance_id
                    .as_deref()
                    .is_some_and(|id| id.contains("empty-instant"))
        })
        .collect::<Vec<_>>();
    assert_eq!(empty_instant_casts.len(), 2);
    assert_eq!(
        empty_instant_casts[1].detail["decisions"]["castSourceZone"],
        "graveyard"
    );
    assert!(
        engine.state().players[0]
            .exile
            .iter()
            .any(|card| card.definition.id == "empty-instant")
    );
    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| event.kind == "flashbackPermissionGranted")
    );
}

/// Feature: Capstone exiles to a mana-value threshold and casts selected cards during resolution.
#[test]
fn cast_any_number_waives_base_cost_and_preserves_stack_order() {
    let cards = (0..7)
        .map(|_| improvisation_capstone_spell())
        .chain(std::iter::once(four_mana_life_spell()))
        .collect::<Vec<_>>();
    let player_one = PlayerDeck {
        cards,
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let setup = setup(player_one, inert_deck("player-two", 8));
    let seed = (0..256)
        .find(|seed| {
            GameEngine::new(setup.clone(), *seed)
                .expect("candidate Capstone game")
                .state()
                .players[0]
                .library
                .last()
                .is_some_and(|card| card.definition.id == "four-mana-life-spell")
        })
        .expect("a deterministic seed leaves the four-mana spell on top");
    let mut engine = GameEngine::new(setup, seed).expect("valid Capstone game");

    engine
        .run(&mut CapstoneSequenceProvider::default(), 1)
        .expect("Capstone sequence resolves");

    assert!(
        engine
            .state()
            .unsupported_rules
            .iter()
            .all(|rule| rule.card_id != "improvisation-capstone")
    );
    assert_eq!(engine.state().players[0].life, 23);
    let capstone_resolved = engine
        .state()
        .events
        .iter()
        .find(|event| {
            event.kind == "spellResolved"
                && event
                    .card_instance_id
                    .as_deref()
                    .is_some_and(|id| id.contains("improvisation-capstone"))
        })
        .expect("Capstone resolved");
    let free_spell_cast = engine
        .state()
        .events
        .iter()
        .find(|event| {
            event.kind == "spellCast"
                && event
                    .card_instance_id
                    .as_deref()
                    .is_some_and(|id| id.contains("four-mana-life-spell"))
        })
        .expect("the exiled spell was cast");
    let free_spell_started = engine
        .state()
        .events
        .iter()
        .find(|event| {
            event.kind == "stackObjectStartedResolving"
                && event
                    .card_instance_id
                    .as_deref()
                    .is_some_and(|id| id.contains("four-mana-life-spell"))
        })
        .expect("the exiled spell started resolving");
    assert_eq!(
        free_spell_cast.detail["decisions"]["withoutPayingManaCost"],
        true
    );
    assert!(free_spell_cast.sequence < capstone_resolved.sequence);
    assert!(capstone_resolved.sequence < free_spell_started.sequence);
}

/// Feature: Paradigm exiles the first resolved original and offers a fresh copy each first main phase.
#[test]
fn paradigm_creates_and_casts_an_ephemeral_copy_on_the_next_turn() {
    let player_one = PlayerDeck {
        cards: (0..8).map(|_| paradigm_spell()).collect(),
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(setup(player_one, inert_deck("player-two", 10)), 155)
        .expect("valid paradigm game");

    engine
        .run(&mut ParadigmSequenceProvider::default(), 3)
        .expect("paradigm sequence resolves");

    assert!(
        engine
            .state()
            .unsupported_rules
            .iter()
            .all(|rule| rule.card_id != "paradigm-spell")
    );
    assert_eq!(
        engine
            .state()
            .events
            .iter()
            .filter(|event| event.kind == "paradigmEstablished")
            .count(),
        1
    );
    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| event.kind == "paradigmTriggered")
    );
    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| event.kind == "paradigmCopyCreated")
    );
    assert!(engine.state().events.iter().any(|event| {
        event.kind == "spellCast" && event.detail["decisions"]["paradigmCopy"] == true
    }));
    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| event.kind == "spellCopyCeasedToExist")
    );
    assert!(
        engine.state().players[0]
            .exile
            .iter()
            .any(|card| card.definition.id == "paradigm-spell")
    );
    assert!(engine.state().players.iter().all(|player| {
        [
            &player.library,
            &player.hand,
            &player.battlefield,
            &player.graveyard,
            &player.exile,
            &player.sideboard,
        ]
        .into_iter()
        .flatten()
        .all(|card| !card.flags.get("isSpellCopy").copied().unwrap_or(false))
    }));
}

/// Feature: Spell stack events retain one ID from cast through completed resolution.
#[test]
fn spell_stack_lifecycle_events_are_correlatable() {
    let player_one = PlayerDeck {
        cards: (0..7).map(token_spell).collect(),
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(setup(player_one, inert_deck("player-two", 8)), 151)
        .expect("valid stack trace game");

    engine
        .run(&mut AggressiveProvider, 1)
        .expect("spells resolve");

    let cast_ids = engine
        .state()
        .events
        .iter()
        .filter(|event| event.kind == "spellCast")
        .filter_map(|event| event.detail["stackId"].as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let start_ids = engine
        .state()
        .events
        .iter()
        .filter(|event| event.kind == "stackObjectStartedResolving")
        .filter_map(|event| event.detail["stackId"].as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let resolved_ids = engine
        .state()
        .events
        .iter()
        .filter(|event| event.kind == "spellResolved")
        .filter_map(|event| event.detail["stackId"].as_str())
        .collect::<std::collections::BTreeSet<_>>();

    assert!(!cast_ids.is_empty());
    assert_eq!(start_ids, cast_ids);
    assert_eq!(resolved_ids, cast_ids);
}

/// Feature: Tokens that leave the battlefield cease to exist at the next state check.
#[test]
fn exiled_tokens_do_not_remain_in_card_zones() {
    let token_player = PlayerDeck {
        cards: (0..8).map(token_spell).collect(),
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let exile_player = PlayerDeck {
        cards: (0..8).map(exile_token_spell).collect(),
        id: "player-two".to_string(),
        name: "player-two".to_string(),
        starting_life: 20,
    };
    let mut engine =
        GameEngine::new(setup(token_player, exile_player), 16).expect("valid token exile game");

    engine
        .run(&mut AggressiveProvider, 2)
        .expect("token exile game executes");

    assert!(engine.state().players.iter().all(|player| {
        player
            .library
            .iter()
            .chain(&player.hand)
            .chain(&player.graveyard)
            .chain(&player.exile)
            .chain(&player.sideboard)
            .all(|card| !card.definition.is_token)
    }));
    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| { event.kind == "tokenCeasedToExist" && event.detail["zone"] == "exile" })
    );
}

/// Feature: Seeded random AIs finish inert games through the empty-library draw rule.
#[test]
fn random_ai_games_terminate_on_empty_library_draws() {
    let summary = simulate_random_games(RandomSimulationRequest {
        games: 24,
        max_turns: 20,
        seed: 20260724,
        setup: setup(inert_deck("player-one", 7), inert_deck("player-two", 7)),
    })
    .expect("simulations run");

    assert_eq!(summary.completed_games, 24);
    assert_eq!(summary.empty_library_games, 24);
    assert_eq!(summary.life_total_games, 0);
    assert_eq!(summary.stalled_games, 0);
}

/// Feature: Random AIs can cast canonical damage spells and remain reproducible by seed.
#[test]
fn random_ai_games_execute_canonical_damage_rules_deterministically() {
    let request = RandomSimulationRequest {
        games: 64,
        max_turns: 40,
        seed: 424242,
        setup: setup(lethal_deck("player-one"), lethal_deck("player-two")),
    };
    let first = simulate_random_games(request.clone()).expect("first simulations run");
    let second = simulate_random_games(request).expect("second simulations run");

    assert_eq!(first.games, second.games);
    assert_eq!(first.completed_games, 64);
    assert_eq!(first.life_total_games, 64);
    assert_eq!(first.stalled_games, 0);
    assert!(first.games.iter().all(|game| game.turns <= 40));
    assert!(
        first
            .games
            .iter()
            .all(|game| game.event_counts.get("spellCast").copied().unwrap_or(0) > 0)
    );
}

/// Feature: Modal cast legality keeps any mode with a complete legal declaration.
#[test]
fn canonical_engine_casts_the_legal_modal_branch_only() {
    let mut cards = vec![abrade_modal_spell()];
    cards.extend((0..6).map(|index| {
        card(
            &format!("artifact-{index}"),
            &format!("Artifact {index}"),
            "Artifact",
            "",
        )
    }));
    let player_one = PlayerDeck {
        cards,
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(setup(player_one, inert_deck("player-two", 7)), 4243)
        .expect("valid modal game");

    engine
        .run(&mut AggressiveProvider, 1)
        .expect("modal spell turn executes");

    assert!(
        engine
            .state()
            .unsupported_rules
            .iter()
            .all(|rule| rule.card_id != "abrade")
    );
    assert!(engine.state().events.iter().any(|event| {
        event.kind == "spellCast"
            && event
                .card_instance_id
                .as_deref()
                .is_some_and(|id| id.contains("abrade"))
    }));
    assert!(engine.state().events.iter().any(|event| {
        event.kind == "permanentToGraveyard" && event.detail["reason"] == "destroyed"
    }));
}

/// Feature: Core creature templates enter, lose summoning sickness, attack, and deal combat damage.
#[test]
fn creature_core_rules_can_end_a_game_through_combat() {
    let mut engine = GameEngine::new(
        setup(creature_deck("player-one"), creature_deck("player-two")),
        88,
    )
    .expect("valid creature game");

    engine
        .run(&mut AggressiveProvider, 12)
        .expect("creature game runs");

    let outcome = engine.state().outcome.as_ref().expect("combat ended game");
    assert_eq!(outcome.reason, GameEndReason::LifeTotal);
    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| event.kind == "combatDamage")
    );
}

/// Feature: Restricted mana pays its life cost and funds only a matching spell.
#[test]
fn restricted_mana_ability_pays_for_an_instant_without_using_the_stack() {
    let mut red_spell = lethal_spell(0, "player-one");
    red_spell.mana_cost = "{R}".to_string();
    let player_one = PlayerDeck {
        cards: vec![
            restricted_mana_land(),
            red_spell,
            card("filler-1", "Filler", "Artifact", "{99}"),
            card("filler-2", "Filler", "Artifact", "{99}"),
            card("filler-3", "Filler", "Artifact", "{99}"),
            card("filler-4", "Filler", "Artifact", "{99}"),
            card("filler-5", "Filler", "Artifact", "{99}"),
        ],
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let mut engine =
        GameEngine::new(setup(player_one, inert_deck("player-two", 8)), 73).expect("valid game");

    engine
        .run(&mut AggressiveProvider, 1)
        .expect("single turn executes");

    assert!(
        engine
            .state()
            .unsupported_rules
            .iter()
            .all(|rule| rule.card_id != "restricted-mana-land")
    );
    assert!(engine.state().events.iter().any(|event| {
        event.kind == "spellCast"
            && event
                .card_instance_id
                .as_deref()
                .is_some_and(|id| id.contains("player-one-lethal"))
    }));
    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| { event.kind == "manaAbilityActivated" && event.detail["lifePaid"] == 1 })
    );
    assert!(
        engine
            .state()
            .events
            .iter()
            .all(|event| event.kind != "activatedAbilityPutOnStack")
    );
}

/// Feature: Restricted instant-or-sorcery mana cannot cast a creature spell.
#[test]
fn restricted_mana_ability_rejects_a_nonmatching_spell() {
    let mut red_creature = card("red-creature", "Red Creature", "Creature - Test", "{R}");
    red_creature.power = Some("2".to_string());
    red_creature.toughness = Some("2".to_string());
    let player_one = PlayerDeck {
        cards: vec![
            restricted_mana_land(),
            red_creature,
            card("filler-1", "Filler", "Artifact", "{99}"),
            card("filler-2", "Filler", "Artifact", "{99}"),
            card("filler-3", "Filler", "Artifact", "{99}"),
            card("filler-4", "Filler", "Artifact", "{99}"),
            card("filler-5", "Filler", "Artifact", "{99}"),
        ],
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let mut engine =
        GameEngine::new(setup(player_one, inert_deck("player-two", 8)), 74).expect("valid game");

    engine
        .run(&mut AggressiveProvider, 1)
        .expect("single turn executes");

    assert!(engine.state().events.iter().all(|event| {
        event.kind != "spellCast"
            || !event
                .card_instance_id
                .as_deref()
                .is_some_and(|id| id.contains("red-creature"))
    }));
}

/// Feature: Paying the last life for mana loses after casting and before resolution.
#[test]
fn restricted_mana_life_payment_can_eliminate_the_caster() {
    let mut red_spell = lethal_spell(0, "player-one");
    red_spell.mana_cost = "{R}".to_string();
    let player_one = PlayerDeck {
        cards: vec![
            restricted_mana_land(),
            red_spell,
            card("filler-1", "Filler", "Artifact", "{99}"),
            card("filler-2", "Filler", "Artifact", "{99}"),
            card("filler-3", "Filler", "Artifact", "{99}"),
            card("filler-4", "Filler", "Artifact", "{99}"),
            card("filler-5", "Filler", "Artifact", "{99}"),
        ],
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 1,
    };
    let mut engine =
        GameEngine::new(setup(player_one, inert_deck("player-two", 8)), 76).expect("valid game");

    engine
        .run(&mut AggressiveProvider, 1)
        .expect("single turn executes");

    let outcome = engine.state().outcome.as_ref().expect("caster lost");
    assert_eq!(outcome.winner.as_deref(), Some("player-two"));
    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| event.kind == "stackObjectCeasedOnPlayerExit")
    );
    assert!(
        engine
            .state()
            .events
            .iter()
            .all(|event| event.kind != "stackObjectStartedResolving")
    );
}

/// Feature: A mana plan is illegal when its combined life costs exceed the payer's life total.
#[test]
fn mana_planner_rejects_cumulative_unpayable_life_costs() {
    let mut double_red_spell = empty_instant();
    double_red_spell.id = "double-red-spell".to_string();
    double_red_spell.name = "Double Red Spell".to_string();
    double_red_spell.mana_cost = "{R}{R}".to_string();
    let cards = vec![
        restricted_mana_land(),
        restricted_mana_land(),
        double_red_spell,
        card("life-filler-1", "Filler", "Artifact", "{99}"),
        card("life-filler-2", "Filler", "Artifact", "{99}"),
        card("life-filler-3", "Filler", "Artifact", "{99}"),
        card("life-filler-4", "Filler", "Artifact", "{99}"),
        card("life-filler-5", "Filler", "Artifact", "{99}"),
    ];
    let player_one = PlayerDeck {
        cards,
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 1,
    };
    let setup = setup(player_one, inert_deck("player-two", 10));
    let seed = (0..256)
        .find(|seed| {
            let engine = GameEngine::new(setup.clone(), *seed).expect("candidate life-cost game");
            let hand = &engine.state().players[0].hand;
            hand.iter()
                .filter(|card| card.definition.id == "restricted-mana-land")
                .count()
                == 2
                && hand
                    .iter()
                    .any(|card| card.definition.id == "double-red-spell")
        })
        .expect("a deterministic seed leaves all payment pieces in hand");
    let mut engine = GameEngine::new(setup, seed).expect("valid cumulative life-cost game");

    engine
        .run(&mut CumulativeLifePaymentProvider, 3)
        .expect("illegal cumulative payment is never offered");

    assert_eq!(engine.state().players[0].life, 1);
    assert!(engine.state().events.iter().all(|event| {
        event.kind != "spellCast"
            || event
                .card_instance_id
                .as_deref()
                .is_none_or(|id| !id.contains("double-red-spell"))
    }));
}

/// Feature: Summoning-sick creatures cannot pay a tap cost for a spell.
#[test]
fn summoning_sick_mana_creature_is_not_a_cast_payment_source() {
    let mut red_spell = token_spell(0);
    red_spell.id = "red-token-spell".to_string();
    red_spell.mana_cost = "{R}".to_string();
    let mut cards = vec![mana_druid(), red_spell];
    cards.extend((0..5).map(|index| {
        card(
            &format!("expensive-{index}"),
            &format!("Expensive {index}"),
            "Artifact",
            "{99}",
        )
    }));
    let player_one = PlayerDeck {
        cards,
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(setup(player_one, inert_deck("player-two", 7)), 75)
        .expect("valid mana-creature game");

    engine
        .run(&mut AggressiveProvider, 1)
        .expect("single turn executes");

    assert!(engine.state().events.iter().any(|event| {
        event.kind == "permanentResolved"
            && event
                .card_instance_id
                .as_deref()
                .is_some_and(|id| id.contains("mana-druid"))
    }));
    assert!(engine.state().events.iter().all(|event| {
        event.kind != "spellCast"
            || !event
                .card_instance_id
                .as_deref()
                .is_some_and(|id| id.contains("red-token-spell"))
    }));
}

/// Feature: Core combat keywords are executable engine vocabulary.
#[test]
fn supported_combat_keyword_is_not_reported_as_a_gap() {
    let mut creature = card("wind-reader", "Wind Reader", "Creature - Wizard", "");
    creature.power = Some("1".to_string());
    creature.toughness = Some("1".to_string());
    creature.rules = vec![json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "flying" }
    })];
    let deck = PlayerDeck {
        cards: (0..8).map(|_| creature.clone()).collect(),
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };

    let engine =
        GameEngine::new(setup(deck, inert_deck("player-two", 8)), 74).expect("valid game setup");

    assert!(
        engine
            .state()
            .unsupported_rules
            .iter()
            .all(|rule| rule.card_id != "wind-reader")
    );
}

/// Feature: Optional life payment controls the conditional tapped replacement.
#[test]
fn conditional_entry_replacement_uses_the_players_decision() {
    let player_deck = |id: &str| PlayerDeck {
        cards: vec![
            optional_life_land(),
            card("filler-1", "Filler", "Artifact", "{99}"),
            card("filler-2", "Filler", "Artifact", "{99}"),
            card("filler-3", "Filler", "Artifact", "{99}"),
            card("filler-4", "Filler", "Artifact", "{99}"),
            card("filler-5", "Filler", "Artifact", "{99}"),
            card("filler-6", "Filler", "Artifact", "{99}"),
        ],
        id: id.to_string(),
        name: id.to_string(),
        starting_life: 20,
    };
    let mut declined = GameEngine::new(
        setup(player_deck("player-one"), inert_deck("player-two", 8)),
        75,
    )
    .expect("valid declined-payment game");
    declined
        .run(&mut AggressiveProvider, 1)
        .expect("declined-payment turn executes");

    assert_eq!(declined.state().players[0].life, 20);
    assert!(declined.state().players[0].battlefield[0].tapped);

    let mut paid = GameEngine::new(
        setup(player_deck("player-one"), inert_deck("player-two", 8)),
        76,
    )
    .expect("valid paid-payment game");
    paid.run(&mut PayLifeProvider, 1)
        .expect("paid-payment turn executes");

    assert_eq!(paid.state().players[0].life, 18);
    assert!(!paid.state().players[0].battlefield[0].tapped);
    assert!(
        paid.state()
            .events
            .iter()
            .any(|event| event.kind == "lifePaid")
    );
}

/// Feature: A controller captured before exile remains available to later life effects.
#[test]
fn ordered_spell_effects_preserve_bound_controller_values() {
    let player_one = PlayerDeck {
        cards: vec![
            card("relic", "Relic", "Artifact", ""),
            card("one-filler-1", "Filler", "Artifact", "{99}"),
            card("one-filler-2", "Filler", "Artifact", "{99}"),
            card("one-filler-3", "Filler", "Artifact", "{99}"),
            card("one-filler-4", "Filler", "Artifact", "{99}"),
            card("one-filler-5", "Filler", "Artifact", "{99}"),
            card("one-filler-6", "Filler", "Artifact", "{99}"),
        ],
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let player_two = PlayerDeck {
        cards: vec![
            exile_and_drain_spell(),
            card("two-filler-1", "Filler", "Artifact", "{99}"),
            card("two-filler-2", "Filler", "Artifact", "{99}"),
            card("two-filler-3", "Filler", "Artifact", "{99}"),
            card("two-filler-4", "Filler", "Artifact", "{99}"),
            card("two-filler-5", "Filler", "Artifact", "{99}"),
            card("two-filler-6", "Filler", "Artifact", "{99}"),
        ],
        id: "player-two".to_string(),
        name: "player-two".to_string(),
        starting_life: 20,
    };
    let mut engine =
        GameEngine::new(setup(player_one, player_two), 77).expect("valid interaction game");

    engine
        .run(&mut AggressiveProvider, 1)
        .expect("interaction turn executes");

    assert_eq!(engine.state().players[0].life, 17);
    assert_eq!(engine.state().players[1].life, 23);
    assert!(
        engine.state().players[0]
            .exile
            .iter()
            .any(|card| card.definition.id == "relic")
    );
    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| event.kind == "lifeLost")
    );
}

/// Feature: Tiered additional costs gate legal modes and each-permanent damage uses a snapshot.
#[test]
fn tiered_sweeper_pays_the_selected_cost_and_damages_each_creature() {
    let creature_cards = (0..8)
        .map(|index| {
            let mut creature = card(
                &format!("one-toughness-{index}"),
                "One Toughness Creature",
                "Creature",
                "",
            );
            creature.power = Some("1".to_string());
            creature.toughness = Some("1".to_string());
            creature
        })
        .collect();
    let sweeper_cards = (0..8).map(|_| tiered_sweeper()).collect();
    let mut engine = GameEngine::new(
        setup(
            PlayerDeck {
                cards: creature_cards,
                id: "player-one".to_string(),
                name: "player-one".to_string(),
                starting_life: 20,
            },
            PlayerDeck {
                cards: sweeper_cards,
                id: "player-two".to_string(),
                name: "player-two".to_string(),
                starting_life: 20,
            },
        ),
        912,
    )
    .expect("valid tiered spell game");

    engine
        .run(&mut ActivePlayerProvider, 2)
        .expect("tiered spell game executes");

    assert!(
        engine
            .state()
            .unsupported_rules
            .iter()
            .all(|rule| rule.card_id != "tiered-sweeper")
    );
    assert!(engine.state().players[0].battlefield.is_empty());
    assert!(
        engine
            .state()
            .events
            .iter()
            .filter(|event| event.kind == "damageDealt")
            .count()
            >= 7
    );
    assert!(
        engine
            .state()
            .events
            .iter()
            .filter(|event| {
                event.kind == "spellCast"
                    && event.detail["decisions"]["chosenModes"] == json!(["fire"])
            })
            .count()
            > 0
    );
}

/// Feature: Ordered library reads request decisions only when their resolution step is reached.
#[test]
fn stock_up_resolution_moves_two_chosen_cards_and_bottoms_the_rest_in_order() {
    let player_one = PlayerDeck {
        cards: (0..12).map(stock_up_spell).collect(),
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 1,
            players: vec![player_one, inert_deck("player-two", 12)],
            starting_player: 0,
        },
        913,
    )
    .expect("valid ordered-library game");

    engine
        .run(&mut CastOnceProvider::default(), 1)
        .expect("ordered library spell resolves");

    let player = &engine.state().players[0];
    assert_eq!(player.hand.len(), 2);
    assert_eq!(player.library.len(), 9);
    assert_eq!(player.graveyard.len(), 1);
    assert!(
        engine
            .state()
            .events
            .iter()
            .filter(|event| event.kind == "resolutionChoiceMade")
            .count()
            >= 2
    );
}

/// Feature: A card-selection request describes candidates and bounds to every client.
#[test]
fn library_card_choice_publishes_engine_owned_selection_metadata() {
    let player_one = PlayerDeck {
        cards: (0..12).map(stock_up_spell).collect(),
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 1,
            players: vec![player_one, inert_deck("player-two", 12)],
            starting_player: 0,
        },
        914,
    )
    .expect("valid card-choice game");
    let mut provider = CardChoiceRecordingProvider::default();

    engine
        .run(&mut provider, 1)
        .expect("card-choice spell resolves");

    let choice = provider
        .card_choice
        .expect("card selection reaches the decision provider");
    assert_eq!(choice["kind"], "cardSelection");
    assert_eq!(choice["decisionId"], "cardsForHand");
    assert_eq!(choice["minimum"], 2);
    assert_eq!(choice["maximum"], 2);
    assert_eq!(
        choice["candidateCardInstanceIds"].as_array().map(Vec::len),
        Some(5)
    );
}

/// Feature: Ordered-card requests explain their source without exposing instance IDs.
#[test]
fn ordered_card_choice_publishes_source_prompt_and_readable_options() {
    let player_one = PlayerDeck {
        cards: (0..12).map(stock_up_spell).collect(),
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 1,
            players: vec![player_one, inert_deck("player-two", 12)],
            starting_player: 0,
        },
        915,
    )
    .expect("valid ordered-card-choice game");
    let mut provider = CardChoiceRecordingProvider::default();

    engine
        .run(&mut provider, 1)
        .expect("ordered-card-choice spell resolves");

    let request = provider
        .resolution_requests
        .iter()
        .find(|request| request["choice"]["kind"] == "cardOrder")
        .expect("card order reaches the decision provider");
    assert_eq!(request["sourceCard"]["definition"]["name"], "Stock Up");
    assert_eq!(
        request["choice"]["prompt"],
        "Order the remaining cards for the bottom of the library, bottommost first."
    );
    assert!(request["options"].as_array().is_some_and(|options| {
        !options.is_empty()
            && options.iter().all(|option| {
                option["label"]
                    .as_str()
                    .is_some_and(|label| label.starts_with("Order cards: "))
                    && !option["label"]
                        .as_str()
                        .is_some_and(|label| label.contains("player-one:"))
            })
    }));
}

/// Feature: Activated abilities pay costs, keep their source in play, and resolve their script.
#[test]
fn activated_ability_exiles_the_top_card_and_grants_a_temporary_permission() {
    let player_one = PlayerDeck {
        cards: (0..8).map(activated_exile_artifact).collect(),
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(setup(player_one, inert_deck("player-two", 8)), 914)
        .expect("valid activated-ability game");

    engine
        .run(&mut ActivePlayerProvider, 1)
        .expect("activated ability resolves");

    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| { event.kind == "activatedAbilityResolved" })
    );
    assert!(engine.state().players[0].exile.is_empty());
    assert!(engine.state().permissions.is_empty());
    assert!(engine.state().events.iter().any(|event| {
        event.kind == "spellCast" && event.detail["decisions"]["castSourceZone"] == "exile"
    }));
    assert!(
        engine.state().players[0]
            .battlefield
            .iter()
            .any(|permanent| permanent.definition.name == "Activated Exile")
    );
}

/// Feature: Next-turn permissions use the recipient's next live turn in multiplayer.
#[test]
fn next_turn_permission_expiration_follows_live_seat_order() {
    let player_one = PlayerDeck {
        cards: (0..9).map(activated_exile_artifact).collect(),
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let setup = GameSetup {
        opening_hand_size: 7,
        players: vec![
            player_one,
            inert_deck("player-two", 8),
            inert_deck("player-three", 8),
        ],
        starting_player: 0,
    };
    let mut engine = GameEngine::new(setup, 920).expect("valid multiplayer permission game");

    engine
        .run(&mut ActivePlayerProvider, 1)
        .expect("permission ability resolves");

    let grants = engine
        .state()
        .events
        .iter()
        .filter(|event| event.kind == "permissionGranted")
        .collect::<Vec<_>>();
    assert!(!grants.is_empty());
    assert!(
        grants
            .iter()
            .all(|event| event.detail["expiresAfterTurn"] == 4)
    );
}

/// Feature: Enter-the-battlefield triggers use the stack and retain their source permanent.
#[test]
fn triggered_ability_mills_and_grants_permission_after_the_source_enters() {
    let player_one = PlayerDeck {
        cards: (0..8).map(triggered_mill_artifact).collect(),
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(setup(player_one, inert_deck("player-two", 8)), 915)
        .expect("valid triggered-ability game");

    engine
        .run(&mut ActivePlayerProvider, 1)
        .expect("triggered ability resolves");

    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| { event.kind == "triggeredAbilityResolved" })
    );
    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| event.kind == "cardMilled")
    );
    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| event.kind == "permissionGranted")
    );
    assert!(engine.state().permissions.is_empty());
    assert!(
        engine.state().players[0]
            .battlefield
            .iter()
            .any(|permanent| permanent.definition.name == "Triggered Mill")
    );
}

/// Feature: A cast permanent may move an optional owned sideboard card into hand.
#[test]
fn cast_conditioned_trigger_chooses_from_the_sideboard() {
    let mut sideboard_card = card("outside-choice", "Outside Choice", "Instant", "{1}");
    sideboard_card.is_sideboard = true;
    let mut cards = vec![outside_game_creature(), sideboard_card];
    cards.extend((0..6).map(|index| {
        card(
            &format!("outside-filler-{index}"),
            "Outside Filler",
            "Artifact",
            "{99}",
        )
    }));
    let player_one = PlayerDeck {
        cards,
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(setup(player_one, inert_deck("player-two", 8)), 916)
        .expect("valid outside-game choice");

    engine
        .run(&mut CastOnceAndChooseOutsideProvider::default(), 1)
        .expect("outside-game trigger resolves");

    assert!(
        engine
            .state()
            .unsupported_rules
            .iter()
            .all(|rule| rule.card_id != "outside-game-creature")
    );
    assert!(engine.state().players[0].sideboard.is_empty());
    assert!(
        engine.state().players[0]
            .hand
            .iter()
            .any(|card| card.definition.id == "outside-choice")
    );
    assert!(engine.state().events.iter().any(|event| {
        event.kind == "triggeredAbilityResolved"
            && event
                .card_instance_id
                .as_deref()
                .is_some_and(|id| id.contains("outside-game-creature"))
    }));
}

/// Feature: Counter vocabulary removes its chosen spell object from the stack.
#[test]
fn counter_spell_moves_the_target_spell_to_its_owners_graveyard() {
    let player_one = PlayerDeck {
        cards: (0..2)
            .map(|index| lethal_spell(index, "player-one"))
            .collect(),
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let player_two = PlayerDeck {
        cards: (0..2).map(counter_spell).collect(),
        id: "player-two".to_string(),
        name: "player-two".to_string(),
        starting_life: 20,
    };
    let player_three = PlayerDeck {
        cards: (0..2)
            .map(|index| card(&format!("observer-{index}"), "Observer", "Artifact", "{99}"))
            .collect(),
        id: "player-three".to_string(),
        name: "player-three".to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 1,
            players: vec![player_one, player_two, player_three],
            starting_player: 0,
        },
        916,
    )
    .expect("valid counter interaction");

    engine
        .run(&mut CounterInteractionProvider::default(), 1)
        .expect("counter interaction resolves");

    assert!(engine.state().outcome.is_none());
    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| event.kind == "spellCountered")
    );
    assert_eq!(engine.state().players[0].life, 20);
    assert_eq!(engine.state().players[1].life, 20);
    assert_eq!(engine.state().players[2].life, 20);
    assert!(
        engine.state().players[0]
            .graveyard
            .iter()
            .any(|card| card.definition.id.starts_with("player-one-lethal"))
    );
    assert!(
        engine.state().players[1]
            .graveyard
            .iter()
            .any(|card| card.definition.id.starts_with("counter-spell"))
    );
}

/// Feature: A target restricted to spells never includes an activated or triggered ability.
#[test]
fn spell_target_candidates_exclude_ability_stack_objects() {
    let player_one = PlayerDeck {
        cards: (0..7).map(triggered_mill_artifact).collect(),
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let player_two = PlayerDeck {
        cards: (0..7).map(counter_spell).collect(),
        id: "player-two".to_string(),
        name: "player-two".to_string(),
        starting_life: 20,
    };
    let mut engine =
        GameEngine::new(setup(player_one, player_two), 95).expect("valid ability-target game");
    let mut provider = AbilityTargetProbeProvider::default();

    engine
        .run(&mut provider, 1)
        .expect("trigger resolves without becoming a spell target");

    assert!(!provider.ability_was_offered_as_spell);
    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| event.kind == "triggeredAbilityResolved")
    );
    assert!(
        engine.state().players[0]
            .graveyard
            .iter()
            .all(|card| !card.definition.id.starts_with("triggered-mill"))
    );
}

/// Feature: A stack static modifier prevents counter vocabulary from moving that spell.
#[test]
fn cant_be_countered_static_rule_keeps_the_spell_on_the_stack() {
    let player_one = PlayerDeck {
        cards: (0..2)
            .map(|index| {
                let mut threat = lethal_spell(index, "player-one");
                threat.rules[0]["declaration"]["decisions"][0]["candidates"]["where"] = json!({
                    "kind": "isOpponentOf",
                    "player": { "kind": "abilityController" }
                });
                threat.rules.insert(
                    0,
                    json!({
                        "kind": "staticAbility",
                        "source": { "kind": "self" },
                        "activeWhile": {
                            "kind": "inZone",
                            "object": { "kind": "self" },
                            "zone": { "kind": "stack" }
                        },
                        "modifiers": [{
                            "kind": "cantBeCountered",
                            "object": { "kind": "self" }
                        }]
                    }),
                );
                threat
            })
            .collect(),
        id: "player-one".to_string(),
        name: "player-one".to_string(),
        starting_life: 20,
    };
    let player_two = PlayerDeck {
        cards: (0..2).map(counter_spell).collect(),
        id: "player-two".to_string(),
        name: "player-two".to_string(),
        starting_life: 20,
    };
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 1,
            players: vec![player_one, player_two],
            starting_player: 0,
        },
        917,
    )
    .expect("valid uncounterable interaction");

    engine
        .run(&mut CounterInteractionProvider::default(), 1)
        .expect("uncounterable interaction resolves");

    assert!(
        engine
            .state()
            .events
            .iter()
            .any(|event| { event.kind == "counterPrevented" })
    );
    assert_eq!(engine.state().players[1].life, 0);
    assert_eq!(
        engine
            .state()
            .outcome
            .as_ref()
            .map(|outcome| outcome.reason.clone()),
        Some(GameEndReason::LifeTotal),
    );
}
