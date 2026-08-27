use mtg_engine::game::{DecisionRequest, build_player_decision_options};
use mtg_engine::http::route_json;
use mtg_engine::oracle::{OracleCardParseRequest, parse_oracle_card};
use serde_json::{Value, json};

fn count_nodes_with_kind(value: &Value, expected_kind: &str) -> usize {
    match value {
        Value::Array(values) => values
            .iter()
            .map(|value| count_nodes_with_kind(value, expected_kind))
            .sum(),
        Value::Object(object) => {
            usize::from(value["kind"] == expected_kind)
                + object
                    .values()
                    .map(|value| count_nodes_with_kind(value, expected_kind))
                    .sum::<usize>()
        }
        _ => 0,
    }
}

/// Feature: Rust Oracle backend returns canonical rules and layered parser audit data.
#[test]
fn oracle_backend_returns_canonical_rules_and_audit_stages() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Parser Fixture".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{1}{R}".to_string()),
        oracle_text: Some(
            "Parser Fixture deals 3 damage to target creature or planeswalker.".to_string(),
        ),
        type_line: "Instant".to_string(),
    });
    let value = serde_json::to_value(result).expect("oracle result serializes");

    assert_eq!(value["status"], "canonical");
    assert_eq!(value["abilities"][0]["rule"]["kind"], "spellAbility");
    assert_eq!(
        value["abilities"][0]["rule"]["effects"][0]["kind"],
        "dealDamage",
    );
    assert_eq!(
        value["abilities"][0]["rule"]["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
        "or",
    );
    assert_eq!(value["stages"][0]["key"], "entityRecognition");
    assert_eq!(
        value["stages"][0]["items"][0]["label"],
        "Ability 1: Parser | Fixture | deals | 3 | damage | to | target | creature | or | planeswalker | .",
    );
    assert_eq!(
        value["stages"][0]["items"][0]["entities"][0]["raw"],
        "Parser",
    );
    assert_eq!(value["stages"][1]["key"], "entitySimplifier");
    let iterations = value["stages"][1]["abilities"][0]["iterations"]
        .as_array()
        .expect("iterations");
    assert!(iterations.len() > 2);
    assert_eq!(iterations[0]["depth"], 0);
    assert_eq!(iterations[0]["result"]["kind"], "spellAbility");
    assert_eq!(
        iterations[0]["result"]["declaration"]["kind"],
        "unresolvedEntities"
    );
    assert_eq!(
        iterations[0]["result"]["effects"][0]["kind"],
        "unresolvedEntities"
    );
    assert_eq!(
        iterations.last().expect("canonical iteration")["status"],
        "canonical",
    );
    assert_eq!(
        iterations
            .iter()
            .filter(|iteration| iteration["result"] == value["abilities"][0]["rule"])
            .count(),
        1,
    );
}

#[test]
fn oracle_backend_recognizes_a_spell_self_exile_instruction() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Spirit Water Revival".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{1}{U}".to_string()),
        oracle_text: Some("Exile Spirit Water Revival.".to_string()),
        type_line: "Sorcery".to_string(),
    });
    let value = serde_json::to_value(result).expect("oracle result serializes");

    assert_eq!(value["status"], "canonical");
    assert_eq!(value["abilities"][0]["rule"]["kind"], "rulesMarker");
    assert_eq!(value["abilities"][0]["rule"]["exileAfterResolution"], true,);
}

#[test]
fn oracle_backend_builds_variable_combat_bonus_from_existing_primitives() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Frantic Confrontation".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{X}{R}".to_string()),
        oracle_text: Some(
            "Target creature you control gets +X/+0 and gains first strike and trample until end of turn."
                .to_string(),
        ),
        type_line: "Instant".to_string(),
    });
    let value = serde_json::to_value(result).expect("oracle result serializes");

    assert_eq!(value["status"], "canonical");
    assert_eq!(value["abilities"][0]["rule"]["kind"], "spellAbility");
    assert_eq!(
        value["abilities"][0]["rule"]["effects"][0]["power"]["decisionId"],
        "xValue",
    );
    assert_eq!(
        value["abilities"][0]["rule"]["effects"][1]["keyword"],
        "firstStrike",
    );
    assert_eq!(
        value["abilities"][0]["rule"]["effects"][2]["keyword"],
        "trample",
    );
}

#[test]
fn oracle_backend_preserves_during_combat_on_enter_triggers() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "The Blue Spirit".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some(
            "Whenever a nontoken creature you control enters during combat, draw a card."
                .to_string(),
        ),
        type_line: "Creature — Spirit".to_string(),
    });
    let value = serde_json::to_value(result).expect("oracle result serializes");

    assert_eq!(value["status"], "canonical");
    assert_eq!(
        value["abilities"][0]["rule"]["event"]["kind"],
        "permanentEntered",
    );
    assert_eq!(value["abilities"][0]["rule"]["event"]["duringCombat"], true,);
}

/// Feature: Entity simplification classifies, partitions, then expands one semantic JSON depth per iteration.
#[test]
fn oracle_simplifier_builds_the_complete_rule_tree_by_depth() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Cori Mountain Monastery".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some(
            "This land enters tapped unless you control a Plains or an Island.\n{T}: Add {R}.\n{3}{R}, {T}: Exile the top card of your library. Until the end of your next turn, you may play that card.".to_string(),
        ),
        type_line: "Land".to_string(),
    });
    let value = serde_json::to_value(result).expect("oracle result serializes");
    let abilities = value["stages"][1]["abilities"]
        .as_array()
        .expect("simplifier abilities");
    let iterations = abilities[0]["iterations"]
        .as_array()
        .expect("simplifier iterations");
    let partitioned = &iterations[0]["result"];

    assert_eq!(partitioned["kind"], "replacementEffect");
    assert_eq!(partitioned["source"]["kind"], "unresolvedEntities");
    assert_eq!(partitioned["source"]["entities"], json!(["This", "land"]));
    assert_eq!(partitioned["event"]["kind"], "unresolvedEntities");
    assert_eq!(
        partitioned["condition"]["entities"],
        json!([
            "unless", "you", "control", "a", "Plains", "or", "an", "Island"
        ])
    );
    assert_eq!(
        partitioned["replacement"][0]["entities"],
        json!(["enters", "tapped"])
    );

    for (expected_depth, iteration) in iterations.iter().enumerate() {
        assert_eq!(iteration["depth"], expected_depth);
        if expected_depth + 1 < iterations.len() {
            assert!(
                count_nodes_with_kind(&iteration["result"], "unresolvedEntities") > 0,
                "depth {expected_depth} should retain unresolved descendants"
            );
        }
    }
    let canonical = &value["abilities"][0]["rule"];
    let final_iteration = iterations.last().expect("canonical iteration");
    assert_eq!(final_iteration["result"], *canonical);
    assert_eq!(
        count_nodes_with_kind(&final_iteration["result"], "unresolvedEntities"),
        0
    );

    let mana_partition = &abilities[1]["iterations"][0]["result"];
    assert_eq!(abilities[1]["abilityType"], "manaAbility");
    assert_eq!(mana_partition["costs"][0]["entities"], json!(["{T}"]));
    assert_eq!(
        mana_partition["effects"][0]["entities"],
        json!(["Add", "{R}"])
    );

    let activated_partition = &abilities[2]["iterations"][0]["result"];
    assert_eq!(abilities[2]["abilityType"], "activatedAbility");
    assert_eq!(
        activated_partition["costs"][0]["entities"],
        json!(["{3}", "{R}", ",", "{T}"])
    );
    assert_eq!(
        activated_partition["effects"][0]["entities"],
        json!([
            "Exile", "the", "top", "card", "of", "your", "library", ".", "Until", "the", "end",
            "of", "your", "next", "turn", ",", "you", "may", "play", "that", "card"
        ])
    );
    assert_eq!(
        abilities[2]["iterations"][1]["result"]["source"]["kind"],
        "self"
    );
}

/// Feature: Rust game backend exposes zone-aware decision packets for players.
#[test]
fn game_backend_lists_zone_aware_decision_options() {
    let request: DecisionRequest = serde_json::from_value(json!({
        "phase": "main",
        "player": {
            "key": "you",
            "name": "You",
            "role": "human",
            "zones": {
                "battlefield": {
                    "creatures": [],
                    "lands": [{
                        "id": "mountain:0",
                        "name": "mountain",
                        "state": { "tapped": false },
                        "typeLine": "Basic Land - Mountain"
                    }],
                    "nonCreaturePermanents": [{
                        "id": "tablet:0",
                        "manaCost": "{1}",
                        "name": "tablet",
                        "oracleText": "{T}: Add {R}.\n{T}: Draw a card.",
                        "state": { "tapped": false },
                        "typeLine": "Artifact"
                    }]
                },
                "exile": { "cards": [], "recoverable": [] },
                "graveyard": {
                    "cards": [{
                        "id": "grave spark:0",
                        "manaCost": "{R}",
                        "name": "grave spark",
                        "oracleText": "Grave Spark deals 1 damage to any target. You may cast this card from your graveyard.",
                        "typeLine": "Instant"
                    }],
                    "recoverable": []
                },
                "hand": [{
                    "id": "mountain:hand",
                    "name": "mountain",
                    "typeLine": "Basic Land - Mountain"
                }, {
                    "id": "flame slash:0",
                    "manaCost": "{R}",
                    "name": "flame slash",
                    "oracleText": "Flame Slash deals 4 damage to target creature.",
                    "typeLine": "Sorcery"
                }],
                "landPlaysAvailable": 1,
                "manaPool": {},
                "playableHand": []
            }
        },
        "targetPlayers": [{
            "key": "opponent",
            "name": "Opponent",
            "role": "ai",
            "zones": {
                "battlefield": {
                    "creatures": [{
                        "id": "bear:0",
                        "name": "bear",
                        "state": { "tapped": false },
                        "typeLine": "Creature - Bear"
                    }],
                    "lands": [],
                    "nonCreaturePermanents": []
                },
                "exile": { "cards": [], "recoverable": [] },
                "graveyard": { "cards": [], "recoverable": [] },
                "hand": [],
                "landPlaysAvailable": 0,
                "manaPool": {},
                "playableHand": []
            }
        }]
    })).expect("decision request parses");

    let packet = build_player_decision_options(request);
    let value = serde_json::to_value(packet).expect("decision packet serializes");
    let options = value["options"].as_array().expect("options is an array");

    assert!(
        options
            .iter()
            .any(|option| option["kind"] == "advanceStep" && option["sourceZone"] == "game")
    );
    assert!(options.iter().any(|option| {
        option["kind"] == "playLand"
            && option["sourceZone"] == "hand"
            && option["conditions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|condition| {
                    condition["name"] == "landPlayAvailable"
                        && condition["params"]["remaining"] == 1
                })
    }));
    assert!(options.iter().any(|option| {
        option["kind"] == "cast"
            && option["card"]["name"] == "flame slash"
            && option["targets"]["required"] == true
            && option["targets"]["candidates"]["cards"][0]["card"]["name"] == "bear"
    }));
    assert!(
        options
            .iter()
            .any(|option| option["kind"] == "cast" && option["sourceZone"] == "graveyard")
    );
    assert!(
        options
            .iter()
            .any(|option| option["kind"] == "mana" && option["card"]["name"] == "tablet")
    );
    assert!(
        options
            .iter()
            .any(|option| option["kind"] == "activate" && option["card"]["name"] == "tablet")
    );
}

/// Feature: Rust cast choices require a complete payment and one legal declaration branch.
#[test]
fn game_backend_filters_casts_by_combined_mana_and_modal_targets() {
    let request: DecisionRequest = serde_json::from_value(json!({
        "phase": "main",
        "player": {
            "key": "you",
            "name": "You",
            "role": "human",
            "zones": {
                "battlefield": {
                    "creatures": [],
                    "lands": [{
                        "id": "mountain:0",
                        "name": "mountain",
                        "state": { "tapped": false },
                        "typeLine": "Basic Land - Mountain"
                    }, {
                        "id": "wastes:0",
                        "name": "wastes",
                        "state": { "tapped": false },
                        "typeLine": "Basic Land - Wastes"
                    }],
                    "nonCreaturePermanents": []
                },
                "hand": [{
                    "id": "paid spell:0",
                    "manaCost": "{1}{R}",
                    "name": "paid spell",
                    "oracleText": "Scry 1.",
                    "typeLine": "Sorcery"
                }, {
                    "id": "flexible spell:0",
                    "manaCost": "{R}",
                    "name": "flexible spell",
                    "oracleText": "Choose one \u{2014}\n\u{2022} Destroy target artifact.\n\u{2022} Scry 1.",
                    "typeLine": "Sorcery"
                }, {
                    "id": "impossible spell:0",
                    "manaCost": "{R}{U}",
                    "name": "impossible spell",
                    "oracleText": "Scry 1.",
                    "typeLine": "Sorcery"
                }],
                "landPlaysAvailable": 0,
                "manaPool": {}
            }
        },
        "targetPlayers": [{
            "key": "opponent",
            "name": "Opponent",
            "role": "ai",
            "zones": {
                "battlefield": {
                    "creatures": [],
                    "lands": [],
                    "nonCreaturePermanents": []
                }
            }
        }],
        "includeAdvanceStep": false
    }))
    .expect("decision request parses");

    let value = serde_json::to_value(build_player_decision_options(request))
        .expect("decision packet serializes");
    let options = value["options"].as_array().expect("options is an array");
    let paid_spell = options
        .iter()
        .find(|option| option["card"]["name"] == "paid spell")
        .expect("the two-source payment makes the spell castable");

    assert_eq!(
        paid_spell["option"]["paymentOptions"][0]["sources"]
            .as_array()
            .expect("payment sources are an array")
            .len(),
        2
    );
    assert!(
        options
            .iter()
            .any(|option| option["card"]["name"] == "flexible spell")
    );
    assert!(
        options
            .iter()
            .all(|option| option["card"]["name"] != "impossible spell")
    );
}

/// Feature: Rust engine backend routes JSON requests for Oracle analysis.
#[test]
fn backend_routes_oracle_parse_json() {
    let response = route_json(
        "POST",
        "/oracle/parse",
        &json!({
            "cardName": "HTTP Fixture",
            "typeLine": "Instant",
            "manaCost": "{1}{R}",
            "oracleText": "HTTP Fixture deals 2 damage to target creature or planeswalker."
        })
        .to_string(),
    );
    let body: Value = serde_json::from_str(&response.body).expect("response body parses");

    assert_eq!(response.status, 200);
    assert_eq!(body["status"], "canonical");
    assert_eq!(
        body["abilities"][0]["rule"]["effects"][0]["kind"],
        "dealDamage"
    );

    let preflight = route_json("OPTIONS", "/oracle/parse", "");
    assert_eq!(preflight.status, 200);
}

/// Feature: Yenna is represented by reusable copy, condition, untap, and scry primitives.
#[test]
fn backend_parses_yenna_without_card_specific_copy_flags() {
    let response = route_json(
        "POST",
        "/oracle/parse",
        &json!({
            "cardName": "Yenna, Redtooth Regent",
            "typeLine": "Legendary Creature — Elf Druid",
            "manaCost": "{2}{G}{W}",
            "oracleText": "{2}, {T}: Choose target enchantment you control that doesn't have the same name as another permanent you control. Create a token that's a copy of it, except it isn't legendary. If the token is an Aura, untap Yenna, Redtooth Regent, then scry 2. Activate only as a sorcery."
        })
        .to_string(),
    );
    let body: Value = serde_json::from_str(&response.body).expect("response body parses");

    assert_eq!(response.status, 200);
    assert_eq!(body["status"], "canonical");
    let effects = body["abilities"][0]["rule"]["effects"]
        .as_array()
        .expect("Yenna has canonical effects");
    assert_eq!(effects[0]["kind"], "createModifiedTokenCopy");
    assert!(effects[0].get("attachIfAura").is_none());
    assert!(effects[0].get("untapSourceAndScryIfAura").is_none());
    assert_eq!(effects[1]["kind"], "conditionalEffect");
    assert_eq!(effects[1]["condition"]["kind"], "objectMatchesFilter");
    assert_eq!(effects[1]["then"][0]["kind"], "untapPermanent");
    assert_eq!(effects[1]["then"][1]["kind"], "scry");
}

/// Feature: Taigam composes stack-copy and suspend primitives in Oracle order.
#[test]
fn backend_parses_taigam_as_copy_then_suspend_original_spell() {
    let response = route_json(
        "POST",
        "/oracle/parse",
        &json!({
            "cardName": "Taigam, Master Opportunist",
            "typeLine": "Legendary Creature — Human Monk",
            "manaCost": "{1}{U}",
            "oracleText": "Flurry — Whenever you cast your second spell each turn, copy it, then exile the spell you cast with four time counters on it. If it doesn't have suspend, it gains suspend. (At the beginning of its owner's upkeep, they remove a time counter. When the last is removed, they may play it without paying its mana cost. If it's a creature, it has haste.)"
        })
        .to_string(),
    );
    let body: Value = serde_json::from_str(&response.body).expect("response body parses");

    assert_eq!(response.status, 200);
    assert_eq!(body["status"], "canonical");
    let rule = &body["abilities"][0]["rule"];
    assert_eq!(rule["event"]["spellCastOrdinal"], 2);
    assert_eq!(rule["effects"][0]["kind"], "copyStackItem");
    assert_eq!(
        rule["effects"][0]["object"]["kind"],
        "triggeringStackObject"
    );
    assert_eq!(rule["effects"][1]["kind"], "suspendStackItem");
    assert_eq!(rule["effects"][1]["timeCounters"]["value"], 4);
}

/// Feature: Interactive play retrieves compact canonical rules without parser audit stages.
#[test]
fn backend_routes_compact_oracle_rules_json() {
    let response = route_json(
        "POST",
        "/oracle/rules",
        &json!({
            "cardName": "Consult the Star Charts",
            "typeLine": "Instant",
            "manaCost": "{1}{U}",
            "oracleText": "Kicker {1}{U} (You may pay an additional {1}{U} as you cast this spell.)\nLook at the top X cards of your library, where X is the number of lands you control. Put one of those cards into your hand. If this spell was kicked, put two of those cards into your hand instead. Put the rest on the bottom of your library in a random order."
        })
        .to_string(),
    );
    let body: Value = serde_json::from_str(&response.body).expect("response body parses");

    assert_eq!(response.status, 200);
    assert_eq!(body["schemaVersion"], "oracle-rules/v1");
    assert_eq!(body["parserVersion"], "oracle-parser/v1");
    assert_eq!(body["canonicalSchemaVersion"], "canonical-rule-ir/v1");
    assert_eq!(body["engineVersion"], "mtg-engine/0.1.0");
    assert_eq!(body["status"], "canonical");
    assert_eq!(body["engineStatus"], "executable");
    assert_eq!(body["executableRuleCount"], 2);
    assert_eq!(body["unexecutableRuleCount"], 0);
    assert_eq!(body["ruleCoverage"][0]["index"], 0);
    assert_eq!(body["ruleCoverage"][0]["kind"], "keywordAbility");
    assert_eq!(body["ruleCoverage"][0]["executable"], true);
    assert!(
        body["functionCoverage"]
            .as_array()
            .expect("canonical function coverage")
            .iter()
            .any(|function| {
                function["kind"] == "chooseCards"
                    && function["path"] == "$.effects[2]"
                    && function["coveredByExecutableRule"] == true
            })
    );
    assert_eq!(body["uncoveredCanonicalFunctionCount"], 0);
    assert_eq!(body["candidateRules"].as_array().map(Vec::len), Some(2));
    assert_eq!(body["rules"][0]["kind"], "keywordAbility");
    assert_eq!(body["rules"][0]["ability"]["kind"], "kicker");
    assert_eq!(body["rules"][1]["kind"], "spellAbility");
    assert_eq!(body["rules"][1]["effects"][2]["kind"], "chooseCards");
    assert!(body.get("stages").is_none());
    assert_eq!(body["abilities"].as_array().map(Vec::len), Some(2));
    assert!(
        body["abilities"]
            .as_array()
            .expect("compact ability coverage")
            .iter()
            .all(|ability| ability["status"] == "canonical")
    );
}

#[test]
fn backend_returns_working_rules_even_when_other_oracle_text_is_unsupported() {
    let response = route_json(
        "POST",
        "/oracle/rules",
        &json!({
            "cardName": "Talon Gates of Madara",
            "typeLine": "Land - Gate",
            "manaCost": null,
            "oracleText": "When Talon Gates of Madara enters the battlefield, up to one target creature phases out.\n{T}: Add {C}.\n{1}, {T}: Add one mana of any color.\n{4}: Put Talon Gates of Madara from your hand onto the battlefield."
        })
        .to_string(),
    );
    let body: Value = serde_json::from_str(&response.body).expect("response body parses");
    let rules = body["rules"].as_array().expect("authoritative rules");

    assert_eq!(response.status, 200);
    assert_eq!(body["status"], "unsupported");
    assert_eq!(body["engineStatus"], "incomplete");
    assert_eq!(rules.len(), 2);
    assert!(rules.iter().all(|rule| rule["kind"] == "manaAbility"));
    assert!(
        body["ruleCoverage"]
            .as_array()
            .expect("rule coverage")
            .iter()
            .all(|coverage| coverage["executable"] == true)
    );
}

/// Feature: Compact prepare payloads keep the permanent cast separate from the linked spell face.
#[test]
fn backend_keeps_prepare_spell_declarations_out_of_the_permanent_face() {
    let response = route_json(
        "POST",
        "/oracle/rules",
        &json!({
            "cardName": "Emeritus of Ideation // Ancestral Recall",
            "typeLine": "Creature - Human Wizard // Instant",
            "manaCost": "{3}{U}{U} // {U}",
            "oracleText": null,
            "layout": "prepare",
            "faces": [
                {
                    "id": "emeritus-of-ideation",
                    "name": "Emeritus of Ideation",
                    "typeLine": "Creature - Human Wizard",
                    "manaCost": "{3}{U}{U}",
                    "oracleText": "Flying, ward {2}\nThis creature enters prepared.\nWhenever this creature attacks, you may exile eight cards from your graveyard. If you do, this creature becomes prepared."
                },
                {
                    "id": "ancestral-recall",
                    "name": "Ancestral Recall",
                    "typeLine": "Instant",
                    "manaCost": "{U}",
                    "oracleText": "Target player draws three cards."
                }
            ]
        })
        .to_string(),
    );
    let body: Value = serde_json::from_str(&response.body).expect("response body parses");
    let rules = body["rules"].as_array().expect("compact rules");

    assert_eq!(response.status, 200);
    assert_eq!(body["status"], "canonical");
    assert!(
        rules
            .iter()
            .all(|rule| rule["kind"].as_str() != Some("spellAbility")),
        "the prepare spell declaration must not become part of the permanent cast"
    );
    let prepare = rules
        .iter()
        .find(|rule| rule["kind"] == "prepareSpell")
        .expect("prepare spell linkage");
    assert_eq!(prepare["spell"]["name"], "Ancestral Recall");
    assert_eq!(prepare["spell"]["manaCost"], "{U}");
    assert_eq!(prepare["spell"]["rules"][0]["kind"], "spellAbility");
    assert_eq!(
        prepare["spell"]["rules"][0]["declaration"]["decisions"][0]["id"],
        "targetPlayer"
    );
}

/// Feature: Rust game backend runs seeded random-AI games through the HTTP contract.
#[test]
fn backend_routes_random_game_simulations() {
    let cards = (0..7)
        .map(|index| {
            json!({
                "id": format!("wastes-{index}"),
                "name": "Wastes",
                "typeLine": "Basic Land - Wastes",
                "manaCost": "",
                "rules": []
            })
        })
        .collect::<Vec<_>>();
    let response = route_json(
        "POST",
        "/game/simulate",
        &json!({
            "games": 4,
            "maxTurns": 10,
            "seed": 99,
            "setup": {
                "openingHandSize": 7,
                "startingPlayer": 0,
                "players": [{
                    "id": "one",
                    "name": "One",
                    "startingLife": 20,
                    "cards": cards
                }, {
                    "id": "two",
                    "name": "Two",
                    "startingLife": 20,
                    "cards": cards
                }]
            }
        })
        .to_string(),
    );
    let body: Value = serde_json::from_str(&response.body).expect("response body parses");

    assert_eq!(response.status, 200);
    assert_eq!(body["completedGames"], 4);
    assert_eq!(body["emptyLibraryGames"], 4);
    assert_eq!(body["stalledGames"], 0);
}

#[test]
fn backend_validates_commander_setups_without_starting_a_session() {
    let response = route_json(
        "POST",
        "/game/setups/validate",
        &json!({
            "gameMode": "commander",
            "setup": {
                "openingHandSize": 7,
                "startingPlayer": 0,
                "players": [{
                    "id": "player-1",
                    "name": "Incomplete Commander",
                    "startingLife": 20,
                    "cards": [{
                        "id": "commander",
                        "name": "Test Commander",
                        "typeLine": "Legendary Creature - Human",
                        "isCommander": true,
                        "manaCost": "{W}"
                    }]
                }]
            }
        })
        .to_string(),
    );
    let payload: Value = serde_json::from_str(&response.body).expect("validation response");

    assert_eq!(response.status, 200);
    assert_eq!(payload["valid"], false);
    assert!(payload["violations"].as_array().is_some_and(|violations| {
        violations
            .iter()
            .any(|violation| violation["code"] == "commander-card-count")
    }));
}

/// Feature: Rust exposes authoritative game formats before game session routing.
#[test]
fn backend_routes_game_format_catalog() {
    let response = route_json("GET", "/game/formats", "");
    let body: Value = serde_json::from_str(&response.body).expect("response body parses");

    assert_eq!(response.status, 200);
    assert_eq!(body["schemaVersion"], "mtg-game-format-catalog/v1");
    assert!(body["formats"].as_array().unwrap().iter().any(|format| {
        format["id"] == "training" && format["defaults"]["openingHandSize"] == 5
    }));
    assert!(body["formats"].as_array().unwrap().iter().any(|format| {
        format["id"] == "training2"
            && format["defaults"]["openingHandSize"] == 6
            && format["defaults"]["startingLife"] == 10
            && format["defaults"]["freeMulligans"] == 1
            && format["defaults"]["maxMulligans"] == 3
    }));
}

/// Feature: HTTP game sessions resume only from one action offered by the Rust engine.
#[test]
fn backend_routes_authoritative_game_session_actions() {
    let cards = (0..8)
        .map(|index| {
            json!({
                "id": format!("mountain-{index}"),
                "name": "Mountain",
                "typeLine": "Basic Land - Mountain",
                "manaCost": "",
                "rules": []
            })
        })
        .collect::<Vec<_>>();
    let create = route_json(
        "POST",
        "/game/sessions",
        &json!({
            "humanPlayerIds": ["one"],
            "maxTurns": 10,
            "seed": 29,
            "setup": {
                "openingHandSize": 8,
                "startingPlayer": 0,
                "players": [{
                    "id": "one",
                    "name": "One",
                    "startingLife": 20,
                    "cards": cards
                }, {
                    "id": "two",
                    "name": "Two",
                    "startingLife": 20,
                    "cards": cards
                }]
            }
        })
        .to_string(),
    );
    let created: Value = serde_json::from_str(&create.body).expect("session response parses");

    assert_eq!(create.status, 200);
    assert_eq!(created["schemaVersion"], "mtg-game-session/v1");
    assert_eq!(created["decision"]["playerId"], "one");
    let play_land = created["decision"]["options"]
        .as_array()
        .expect("session options")
        .iter()
        .find(|action| action["kind"] == "playLand")
        .expect("Rust offers a land play");
    let session_id = created["sessionId"].as_str().expect("session ID");
    let submit = route_json(
        "POST",
        &format!("/game/sessions/{session_id}/actions"),
        &json!({
            "revision": created["revision"],
            "decisionId": created["decision"]["id"],
            "actionId": play_land["id"]
        })
        .to_string(),
    );
    let resumed: Value = serde_json::from_str(&submit.body).expect("resumed response parses");

    assert_eq!(submit.status, 200);
    assert_eq!(resumed["sessionId"], session_id);
    assert!(resumed["revision"].as_u64() > created["revision"].as_u64());
    assert_eq!(
        resumed["state"]["players"][0]["battlefield"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let stale = route_json(
        "POST",
        &format!("/game/sessions/{session_id}/actions"),
        &json!({
            "revision": created["revision"],
            "decisionId": created["decision"]["id"],
            "actionId": play_land["id"]
        })
        .to_string(),
    );
    assert_eq!(stale.status, 409);

    let settings = route_json(
        "PUT",
        &format!("/game/sessions/{session_id}/settings"),
        &json!({
            "holdPriorityPlayerIds": ["one"]
        })
        .to_string(),
    );
    let settings_body: Value =
        serde_json::from_str(&settings.body).expect("settings response parses");
    assert_eq!(settings.status, 200);
    assert_eq!(settings_body["sessionId"], session_id);

    let removed = route_json("DELETE", &format!("/game/sessions/{session_id}"), "");
    assert_eq!(removed.status, 200);
    assert_eq!(
        route_json("GET", &format!("/game/sessions/{session_id}"), "").status,
        404
    );
}
