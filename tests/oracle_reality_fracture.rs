use mtg_engine::engine::rule_is_executable;
use mtg_engine::http::route_json;
use mtg_engine::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::{Value, json};

fn parse_creature_ability(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Generic Graveyard Adept".to_string(),
        type_line: "Creature - Human Wizard".to_string(),
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn submit_action(session_id: &str, view: &Value, action: &Value) -> Value {
    let response = route_json(
        "POST",
        &format!("/game/sessions/{session_id}/actions"),
        &json!({
            "revision": view["revision"],
            "decisionId": view["decision"]["id"],
            "actionId": action["id"],
        })
        .to_string(),
    );
    assert_eq!(response.status, 200, "{}", response.body);
    serde_json::from_str(&response.body).expect("session response parses")
}

#[test]
fn graveyard_source_exile_activations_are_canonical_and_executable() {
    for (oracle_text, expected_minimum) in [
        (
            "{3}{U}, Exile this card from your graveyard: Tap target creature and put a stun counter on it. Activate only as a sorcery.",
            1,
        ),
        (
            "Exile this card from your graveyard: Tap up to one target artifact and put a stun counter on it.",
            0,
        ),
    ] {
        let parsed = parse_creature_ability(oracle_text);
        assert_eq!(parsed.status, "canonical", "{oracle_text}");
        assert_eq!(parsed.abilities[0].ability_type, "activatedAbility");
        let rule = parsed.abilities[0]
            .rule
            .as_ref()
            .expect("activation has canonical IR");
        assert_eq!(rule["activationZone"], "graveyard");
        assert!(rule["costs"].as_array().is_some_and(|costs| {
            costs
                .iter()
                .any(|cost| cost["kind"] == "exileSource" && cost["zone"] == "graveyard")
        }));
        assert_eq!(rule["effects"][0]["kind"], "tapPermanent");
        assert_eq!(rule["effects"][1]["kind"], "putCounters");
        assert_eq!(
            rule["declaration"]["decisions"][0]["minimum"],
            expected_minimum,
        );
        assert!(rule_is_executable(rule), "{oracle_text}");
    }

    let malformed = parse_creature_ability(
        "Admire this card from your graveyard: Tap target creature and put a stun counter on it.",
    );
    assert_eq!(malformed.status, "unsupported");
}

#[test]
fn activation_classifier_ignores_nested_quoted_and_empty_colons() {
    for oracle_text in [
        "Ward {2} (Whenever this becomes the target of a spell: counter it.)",
        "This creature has \"{T}: Add {G}.\"",
        ": Draw a card.",
        "Exile this card from your graveyard:",
    ] {
        let parsed = parse_creature_ability(oracle_text);
        assert_ne!(
            parsed.abilities[0].ability_type, "activatedAbility",
            "{oracle_text}",
        );
    }
}

#[test]
fn activation_classifier_delegates_new_cost_vocabulary_to_the_cost_parser() {
    let parsed = parse_creature_ability("Waterbend 2: Draw a card.");
    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].ability_type, "activatedAbility");
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("the cost parser owns the newly recognized cost atom");
    assert_eq!(rule["costs"][0]["kind"], "payWaterbend");
    assert_eq!(rule["costs"][0]["amount"]["value"], 2);
}

#[test]
fn top_level_loyalty_colons_keep_their_existing_canonical_paths() {
    for oracle_text in [
        "+1: Surveil 1. If you put a noncreature, nonland card into your graveyard this way, put that card into your hand.",
        "−6: You get an emblem with \"Whenever you cast a spell, draw a card.\"",
        "+1: Exile the top card of your library. You may cast that card. If you don't, Chandra deals 2 damage to each opponent.",
        "−7: You get an emblem with \"Whenever you cast a spell, this emblem deals 5 damage to any target.\"",
        "−X: Tap target artifact or creature. Put X stun counters on it.",
    ] {
        let parsed = parse_creature_ability(oracle_text);
        assert_eq!(parsed.status, "canonical", "{oracle_text}");
        assert!(
            parsed.abilities[0]
                .rule
                .as_ref()
                .is_some_and(rule_is_executable),
            "{oracle_text}",
        );
    }
}

#[test]
fn graveyard_source_exile_cost_and_targeted_stun_mutate_session_state() {
    let discard_source = parse_creature_ability("Discard a card: Draw a card.");
    let discard_rule = discard_source.abilities[0]
        .rule
        .clone()
        .expect("discard-and-draw ability parses");
    assert!(rule_is_executable(&discard_rule));
    assert_eq!(discard_rule["kind"], "activatedAbility");

    let graveyard_source = parse_creature_ability(
        "Exile this card from your graveyard: Tap target artifact and put a stun counter on it. Activate only as a sorcery.",
    );
    let graveyard_rule = graveyard_source.abilities[0]
        .rule
        .clone()
        .expect("graveyard activation parses");
    assert!(rule_is_executable(&graveyard_rule));

    let station_definition = json!({
        "id": "discard-station",
        "name": "Discard Station",
        "typeLine": "Artifact Land",
        "manaCost": "",
        "rules": [discard_rule],
    });
    let source_definition = json!({
        "id": "graveyard-source",
        "name": "Generic Graveyard Adept",
        "typeLine": "Creature - Human Wizard",
        "manaCost": "{2}{U}",
        "rules": [graveyard_rule],
    });
    let first_cards = (0..5)
        .flat_map(|_| [station_definition.clone(), source_definition.clone()])
        .collect::<Vec<_>>();
    let opponent_cards = (0..10)
        .map(|index| {
            json!({
                "id": format!("opponent-filler-{index}"),
                "name": "Wastes",
                "typeLine": "Basic Land - Wastes",
                "manaCost": "",
                "rules": [],
            })
        })
        .collect::<Vec<_>>();

    let response = route_json(
        "POST",
        "/game/sessions",
        &json!({
            "humanPlayerIds": ["one"],
            "maxTurns": 2,
            "seed": 314,
            "setup": {
                "openingHandSize": 8,
                "startingPlayer": 0,
                "players": [{
                    "id": "one",
                    "name": "One",
                    "startingLife": 20,
                    "cards": first_cards,
                }, {
                    "id": "two",
                    "name": "Two",
                    "startingLife": 20,
                    "cards": opponent_cards,
                }],
            },
        })
        .to_string(),
    );
    assert_eq!(response.status, 200, "{}", response.body);
    let mut view: Value = serde_json::from_str(&response.body).expect("session response parses");
    let session_id = view["sessionId"].as_str().expect("session ID").to_string();
    let station_instance_id = view["state"]["players"][0]["hand"]
        .as_array()
        .expect("opening hand")
        .iter()
        .find(|card| card["definition"]["id"] == "discard-station")
        .and_then(|card| card["instanceId"].as_str())
        .expect("station instance")
        .to_string();
    let source_instance_id = view["state"]["players"][0]["hand"]
        .as_array()
        .expect("opening hand")
        .iter()
        .find(|card| card["definition"]["id"] == "graveyard-source")
        .and_then(|card| card["instanceId"].as_str())
        .expect("graveyard source instance")
        .to_string();

    let play_station = view["decision"]["options"]
        .as_array()
        .expect("priority options")
        .iter()
        .find(|action| {
            action["kind"] == "playLand" && action["cardInstanceId"] == station_instance_id
        })
        .expect("station can be played")
        .clone();
    view = submit_action(&session_id, &view, &play_station);

    let discard_and_draw = view["decision"]["options"]
        .as_array()
        .expect("priority options")
        .iter()
        .find(|action| {
            action["kind"] == "activateAbility"
                && action["cardInstanceId"] == station_instance_id
                && action["targets"]["discardCost1"]["card"]["instanceId"] == source_instance_id
        })
        .expect("source can be discarded for the draw ability")
        .clone();
    view = submit_action(&session_id, &view, &discard_and_draw);
    assert!(
        view["state"]["players"][0]["graveyard"]
            .as_array()
            .expect("graveyard")
            .iter()
            .any(|card| card["instanceId"] == source_instance_id)
    );

    let pass = view["decision"]["options"]
        .as_array()
        .expect("priority options")
        .iter()
        .find(|action| action["kind"] == "passPriority")
        .expect("controller can pass priority to resolve the draw ability")
        .clone();
    view = submit_action(&session_id, &view, &pass);

    let activate_from_graveyard = view["decision"]["options"]
        .as_array()
        .expect("priority options")
        .iter()
        .find(|action| {
            action["kind"] == "activateAbility"
                && action["cardInstanceId"] == source_instance_id
                && action["targets"]["targetPermanent"]["permanent"]["instanceId"]
                    == station_instance_id
        })
        .expect("graveyard source can target the creature land")
        .clone();
    view = submit_action(&session_id, &view, &activate_from_graveyard);
    assert!(
        view["state"]["players"][0]["exile"]
            .as_array()
            .expect("exile")
            .iter()
            .any(|card| card["instanceId"] == source_instance_id)
    );

    let pass = view["decision"]["options"]
        .as_array()
        .expect("priority options")
        .iter()
        .find(|action| action["kind"] == "passPriority")
        .expect("controller can pass priority")
        .clone();
    view = submit_action(&session_id, &view, &pass);
    let station = view["state"]["players"][0]["battlefield"]
        .as_array()
        .expect("battlefield")
        .iter()
        .find(|card| card["instanceId"] == station_instance_id)
        .expect("station remains on the battlefield");
    assert_eq!(station["tapped"], true);
    assert_eq!(station["counters"]["stun"], 1);

    assert_eq!(
        route_json("DELETE", &format!("/game/sessions/{session_id}"), "").status,
        200
    );
}
