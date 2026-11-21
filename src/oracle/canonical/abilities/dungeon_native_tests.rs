//! Dungeon room frame (CR 309.4): label, shared effect program, typed graph.
use crate::card_catalog::builtin_dungeon_printings;
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::Value;

fn parse(name: &str, type_line: &str, text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn rules(parsed: &OracleCardParseResult) -> Vec<&Value> {
    parsed
        .abilities
        .iter()
        .filter_map(|ability| ability.rule.as_ref())
        .collect()
}

fn kinds(effects: &Value) -> Vec<&str> {
    effects
        .as_array()
        .expect("effects")
        .iter()
        .map(|effect| effect["kind"].as_str().expect("kind"))
        .collect()
}

#[test]
fn the_four_canonical_dungeons_compile_every_room_and_route() {
    for (name, printing) in builtin_dungeon_printings().unwrap() {
        let parsed = parse(
            name,
            printing.type_line.as_deref().unwrap_or_default(),
            printing.oracle_text.as_deref().unwrap_or_default(),
        );
        assert_eq!(
            parsed.status,
            "canonical",
            "{name}: {:?}",
            parsed
                .abilities
                .iter()
                .flat_map(|ability| ability.diagnostics.iter())
                .collect::<Vec<_>>()
        );
        let rooms = rules(&parsed)
            .into_iter()
            .filter(|rule| rule["kind"] == "dungeonRoom")
            .collect::<Vec<_>>();
        // Every route names a room of the same Dungeon (CR 309.5a).
        for room in &rooms {
            for next in room["nextRooms"].as_array().unwrap() {
                assert!(
                    rooms.iter().any(|other| other["room"] == *next),
                    "{name}: {next}"
                );
            }
        }
        assert_eq!(
            rooms
                .iter()
                .filter(|room| room["nextRooms"].as_array().unwrap().is_empty())
                .count(),
            1,
            "{name} has one bottommost room"
        );
    }
}

#[test]
fn a_room_is_its_label_the_shared_program_and_its_routes() {
    let parsed = parse(
        "Lost Mine of Phandelver",
        "Dungeon",
        "Cave Entrance \u{2014} Scry 1. (Leads to: Goblin Lair, Mine Tunnels)\nTemple of Dumathoin \u{2014} Draw a card.",
    );
    let rules = rules(&parsed);
    assert_eq!(rules[0]["room"], "Cave Entrance");
    assert_eq!(
        rules[0]["nextRooms"],
        serde_json::json!(["Goblin Lair", "Mine Tunnels"])
    );
    assert_eq!(rules[0]["effects"][0]["kind"], "lookAtTopCards");
    assert_eq!(rules[1]["nextRooms"], serde_json::json!([]));
    assert_eq!(kinds(&rules[1]["effects"]), ["drawCards"]);
}

#[test]
fn an_unused_combination_of_room_concepts_needs_no_new_code() {
    // Sibling: a targeted keyword action, an `unless they` payment over a
    // different population, and a named entry procedure.
    let parsed = parse(
        "Test Vault",
        "Dungeon \u{2014} Test",
        "You can't enter this dungeon unless you \"venture into the Deep Vault.\"\nSealed Door \u{2014} Each opponent loses 3 life unless they sacrifice a creature or land of their choice. (Leads to: Final Vault, Side Hall)\nSide Hall \u{2014} Goad target creature an opponent controls. (Leads to: Final Vault)\nFinal Vault \u{2014} Create two 1/1 black Skeleton creature tokens.",
    );
    assert_eq!(parsed.status, "canonical", "{:?}", parsed.diagnostics);
    let rules = rules(&parsed);
    assert_eq!(rules[0]["modifiers"][0]["kind"], "dungeonEntryRestriction");
    assert_eq!(
        rules[0]["modifiers"][0]["procedure"],
        "venture into the Deep Vault"
    );
    assert_eq!(rules[1]["effects"][0]["kind"], "forEachPlayer");
    assert_eq!(rules[1]["effects"][0]["players"]["kind"], "opponentsOf");
    assert_eq!(rules[2]["effects"][0]["kind"], "goadPermanents");
    assert_eq!(rules[3]["nextRooms"], serde_json::json!([]));
}

#[test]
fn malformed_rooms_and_unknown_room_effects_fail_explicitly() {
    for text in [
        // No route list after `to`, a route list without its colon, and an
        // effect the shared grammar does not know are never partially kept.
        "Cave Entrance \u{2014} Scry 1. (Leads to: )",
        "Cave Entrance \u{2014} Scry 1. (Leads to Goblin Lair)",
        "Unclear Room \u{2014} Target creature gets much weaker until your next turn.",
        "You can't enter this dungeon unless you \"venture into the dungeon.\"",
    ] {
        let parsed = parse("Test Dungeon", "Dungeon", text);
        assert_ne!(parsed.status, "canonical", "{text}");
    }
    // A labeled line of a non-Dungeon face is not a room.
    let parsed = parse("Not A Dungeon", "Sorcery", "Cave Entrance \u{2014} Scry 1.");
    assert!(
        rules(&parsed)
            .iter()
            .all(|rule| rule["kind"] != "dungeonRoom")
    );
}
