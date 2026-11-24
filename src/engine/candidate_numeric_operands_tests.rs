//! The amount on the right of a candidate comparison is read for the
//! enumerating player each time candidates are enumerated (CR 601.2c,
//! 608.2b): adding a land changes which candidates qualify.
use super::super::*;

fn definition(id: &str, type_line: &str, mana_cost: &str) -> CardDefinition {
    serde_json::from_value(json!({
        "id": id,
        "name": id,
        "typeLine": type_line,
        "manaCost": mana_cost,
        "power": "2",
        "toughness": "2",
        "rules": [],
    }))
    .unwrap()
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Artifact", "");
    GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 4],
                })
                .collect(),
        },
        4417,
    )
    .unwrap()
}

/// `target creature with mana value less than or equal to the number of
/// lands you control`, as the oracle lowers it.
fn creatures_up_to_land_count() -> Value {
    json!({
        "kind": "permanents",
        "where": {
            "kind": "and",
            "operands": [
                { "kind": "cardTypeContains", "value": "Creature" },
                {
                    "kind": "compare",
                    "left": { "kind": "manaValueOf", "object": { "kind": "candidate" } },
                    "operator": "<=",
                    "right": {
                        "kind": "countPermanents",
                        "where": { "kind": "cardTypeContains", "value": "Land" },
                        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    },
                },
            ],
        },
    })
}

fn candidate_ids(engine: &GameEngine, candidates: &Value) -> BTreeSet<String> {
    engine
        .target_candidates("p0", candidates)
        .expect("a supported candidate domain")
        .into_iter()
        .filter_map(|target| match target {
            TargetRef::Permanent { instance_id } => Some(instance_id),
            _ => None,
        })
        .collect()
}

#[test]
fn controller_relative_threshold_is_read_at_each_enumeration() {
    let mut engine = engine();
    let cheap = card_instance("p1", &definition("cheap", "Creature — Bear", "{1}{G}"), 1);
    let costly = card_instance("p1", &definition("costly", "Creature — Giant", "{4}{R}"), 2);
    let cheap_id = cheap.instance_id.clone();
    let costly_id = costly.instance_id.clone();
    engine.state.players[1].battlefield.extend([cheap, costly]);
    for index in 0..2 {
        engine.state.players[0].battlefield.push(card_instance(
            "p0",
            &definition("forest", "Basic Land — Forest", ""),
            10 + index,
        ));
    }
    let candidates = creatures_up_to_land_count();
    assert_eq!(
        candidate_ids(&engine, &candidates),
        BTreeSet::from([cheap_id.clone()]),
    );
    // The enumerating player's lands, not the candidate's controller's.
    for index in 0..3 {
        engine.state.players[1].battlefield.push(card_instance(
            "p1",
            &definition("mountain", "Basic Land — Mountain", ""),
            20 + index,
        ));
    }
    assert_eq!(
        candidate_ids(&engine, &candidates),
        BTreeSet::from([cheap_id.clone()]),
    );
    for index in 0..3 {
        engine.state.players[0].battlefield.push(card_instance(
            "p0",
            &definition("forest", "Basic Land — Forest", ""),
            30 + index,
        ));
    }
    assert_eq!(
        candidate_ids(&engine, &candidates),
        BTreeSet::from([cheap_id, costly_id]),
    );
}

#[test]
fn only_controller_relative_amounts_are_bound() {
    let engine = engine();
    // A literal, a declared X and a player the binder cannot read are left
    // to their own consumers; nothing is copied.
    for right in [
        json!(3),
        json!({ "kind": "decisionResult", "decisionId": "xValue" }),
        json!({ "kind": "countPermanents", "where": null, "player": { "kind": "triggeringPlayer" } }),
        json!({ "kind": "countPermanents", "where": null, "excludeSource": true }),
    ] {
        let candidates = json!({
            "kind": "permanents",
            "where": {
                "kind": "compare",
                "left": { "kind": "powerOf", "object": { "kind": "candidate" } },
                "operator": ">=",
                "right": right,
            },
        });
        assert!(matches!(
            engine.bind_candidate_amounts(&candidates, "p0"),
            std::borrow::Cow::Borrowed(_)
        ));
    }
    // Arithmetic over a life total is read as one integer.
    let candidates = json!({
        "kind": "union",
        "sets": [{
            "kind": "permanents",
            "where": {
                "kind": "not",
                "operand": {
                    "kind": "compare",
                    "left": { "kind": "powerOf", "object": { "kind": "candidate" } },
                    "operator": ">=",
                    "right": {
                        "kind": "add",
                        "left": 1,
                        "right": { "kind": "lifeTotal", "player": { "kind": "controllerOf", "object": { "kind": "self" } } },
                    },
                },
            },
        }],
    });
    let bound = engine.bind_candidate_amounts(&candidates, "p0");
    assert_eq!(bound["sets"][0]["where"]["operand"]["right"], json!(21));
}
