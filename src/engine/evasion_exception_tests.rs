//! Runtime proofs of the static evasion exception (CR 702.14): the keyword
//! stays on the attacker, only its blocking restriction is ignored, and only
//! for the landwalk keyword the exception names.
use super::*;

fn test_definition(id: &str, type_line: &str) -> CardDefinition {
    serde_json::from_value(json!({
        "id": id, "name": id, "typeLine": type_line,
        "power": type_line.starts_with("Creature").then_some("2"),
        "toughness": type_line.starts_with("Creature").then_some("2"),
    }))
    .expect("standalone test definition is valid")
}

fn test_instance(id: &str, definition: CardDefinition, owner: &str) -> CardInstance {
    let mut instance = card_instance(owner, &definition, 0);
    instance.instance_id = id.into();
    instance
}

fn test_engine() -> GameEngine {
    let filler = test_definition("filler", "Basic Land - Plains");
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        702,
    )
    .expect("standalone test engine is valid")
}

fn walker(id: &str, keyword: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature - Zombie");
    definition.rules.push(json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": keyword },
    }));
    test_instance(id, definition, "player-0")
}

/// `Creatures with swampwalk can be blocked as though they didn't have
/// swampwalk.` (Quagmire).
fn exception(keyword: &str) -> CardInstance {
    let mut definition = test_definition("exception", "Enchantment");
    definition.rules.push(json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": { "kind": "inZone", "object": { "kind": "self" }, "zone": { "kind": "battlefield" } },
        "modifiers": [{
            "kind": "ignoreBlockEvasion",
            "objects": {
                "kind": "permanents",
                "where": { "kind": "hasKeyword", "value": keyword },
            },
            "keyword": keyword,
        }],
    }));
    assert!(battlefield_static_rule_supported(&definition.rules[0]));
    test_instance("exception", definition, "player-1")
}

fn attack(engine: &mut GameEngine, attacker: &CardInstance) {
    engine.state.combat.attackers.push(DeclaredAttacker {
        attacker_id: attacker.instance_id.clone(),
        defender: TargetRef::Player {
            player_id: "player-1".to_string(),
        },
        defending_player_id: "player-1".to_string(),
    });
}

#[test]
fn landwalk_exception_ignores_only_the_named_evasion() {
    let mut engine = test_engine();
    let blocker = test_instance(
        "blocker",
        test_definition("blocker", "Creature - Soldier"),
        "player-1",
    );
    let swampwalker = walker("swampwalker", "swampwalk");
    let islandwalker = walker("islandwalker", "islandwalk");
    attack(&mut engine, &swampwalker);
    attack(&mut engine, &islandwalker);
    for land in [
        ("swamp", "Basic Land - Swamp"),
        ("island", "Basic Land - Island"),
    ] {
        engine.state.players[1].battlefield.push(test_instance(
            land.0,
            test_definition(land.0, land.1),
            "player-1",
        ));
    }
    assert!(!engine.can_block(&blocker, &swampwalker));
    assert!(!engine.can_block(&blocker, &islandwalker));
    engine.state.players[1]
        .battlefield
        .push(exception("swampwalk"));
    assert!(
        engine.can_block(&blocker, &swampwalker),
        "the swampwalk restriction is ignored"
    );
    assert!(
        engine.permanent_has_keyword(&swampwalker, "swampwalk"),
        "the attacker keeps the keyword"
    );
    assert!(
        !engine.can_block(&blocker, &islandwalker),
        "another landwalk keyword still applies"
    );
}

#[test]
fn evasion_exception_requires_a_landwalk_keyword() {
    for keyword in ["flying", "swamp", ""] {
        let modifier = json!({
            "kind": "ignoreBlockEvasion",
            "objects": { "kind": "self" },
            "keyword": keyword,
        });
        assert_eq!(modifier_supported(&modifier), Some(false), "{keyword}");
    }
    assert_eq!(modifier_supported(&json!({ "kind": "grantKeyword" })), None);
}
