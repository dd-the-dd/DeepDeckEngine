//! One-shot event modifiers are typed `{capture, match, apply, consume}`
//! programs; linked exile counts are the shared `countCards` primitive.
use super::*;

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.into(),
        name: id.into(),
        type_line: type_line.into(),
        mana_cost: String::new(),
        power: Some("1".into()),
        toughness: Some("1".into()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules: Vec::new(),
    }
}

fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        4401,
    )
    .unwrap()
}

fn next_spell_cant_be_countered() -> Value {
    json!({
        "kind": "installOneShotModifier",
        "capture": { "player": { "kind": "abilityController" } },
        "expires": { "kind": "endOfCurrentTurn" },
        "match": { "kind": "spellCast", "player": { "kind": "capturedValue", "name": "player" } },
        "apply": [{
            "kind": "modifyStackObject",
            "object": { "kind": "eventSpell" },
            "modifier": { "kind": "cantBeCountered" }
        }],
        "consume": { "kind": "firstMatchingEvent" }
    })
}

fn stack_object(source: &CardInstance) -> StackObject {
    StackObject {
        id: "stack:one-shot".into(),
        controller: "player-0".into(),
        card: source.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

#[test]
fn one_shot_modifier_shape_is_typed() {
    let effect = next_spell_cant_be_countered();
    assert!(install_one_shot_modifier_supported(&effect));
    // A spell filter composes with the same frame.
    let mut filtered = effect.clone();
    filtered["match"]["where"] = json!({"kind":"cardTypeContains","value":"Creature"});
    assert!(install_one_shot_modifier_supported(&filtered));
    // Near misses: an unknown capture name and an unknown stack modifier.
    let mut unknown_capture = effect.clone();
    unknown_capture["match"]["player"]["name"] = json!("other");
    assert!(!install_one_shot_modifier_supported(&unknown_capture));
    let mut unknown_modifier = effect;
    unknown_modifier["apply"][0]["modifier"] = json!({"kind":"cantBeCopied"});
    assert!(!install_one_shot_modifier_supported(&unknown_modifier));
}

#[test]
fn first_matching_cast_consumes_the_modifier_once() {
    let mut e = engine();
    let source = card_instance("player-0", &definition("village", "Land"), 300);
    e.install_one_shot_modifier(
        &next_spell_cant_be_countered(),
        &stack_object(&source),
        &BTreeMap::new(),
    )
    .expect("the one-shot modifier is installed");
    let spell = definition("spell", "Instant");
    // Another player's cast does not match the captured player.
    assert!(
        e.consume_one_shot_spell_cast_modifier("player-1", &spell)
            .is_empty()
    );
    let operations = e.consume_one_shot_spell_cast_modifier("player-0", &spell);
    assert!(operations_set_stack_modifier(
        &operations,
        "cantBeCountered"
    ));
    assert!(
        e.consume_one_shot_spell_cast_modifier("player-0", &spell)
            .is_empty()
    );
}

#[test]
fn linked_exile_count_is_a_count_cards_zone() {
    let mut e = engine();
    let source = card_instance("player-0", &definition("warden", "Creature"), 310);
    e.state.players[0].battlefield.push(source.clone());
    for (index, type_line) in ["Creature", "Instant"].into_iter().enumerate() {
        let exiled = card_instance(
            "player-1",
            &definition(&format!("exiled-{index}"), type_line),
            320 + index,
        );
        e.state.rule_modifiers.push(json!({
            "kind": "exiledWithSource",
            "sourceCardInstanceId": source.instance_id,
            "cardInstanceId": exiled.instance_id,
        }));
        e.state.players[1].exile.push(exiled);
    }
    let all = json!({"kind":"countCards","zone":{"kind":"exile","linkedSource":{"kind":"self"}}});
    assert!(numeric_expression_supported(&all));
    assert_eq!(
        e.numeric_expression_with_source(&all, "player-0", Some(&source)),
        Some(2)
    );
    let creatures = json!({"kind":"countCards",
        "zone":{"kind":"exile","linkedSource":{"kind":"self"}},
        "where":{"kind":"cardTypeContains","value":"Creature"}});
    assert_eq!(
        e.numeric_expression_with_source(&creatures, "player-0", Some(&source)),
        Some(1)
    );
    // Without a source object the linked zone is undefined, never zero.
    assert_eq!(
        e.numeric_expression_with_source(&all, "player-0", None),
        None
    );
}
