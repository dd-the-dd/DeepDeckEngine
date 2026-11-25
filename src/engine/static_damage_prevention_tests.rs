//! Runtime proofs of the static scoped prevention filter (CR 615.1a): the
//! damage kind, the recipient and the source are independent slots, each
//! evaluated relative to the prevention's source when damage would be dealt.
use super::*;

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.into(),
        name: id.into(),
        type_line: type_line.into(),
        mana_cost: "{1}".into(),
        power: type_line.contains("Creature").then(|| "2".into()),
        toughness: type_line.contains("Creature").then(|| "4".into()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules: Vec::new(),
    }
}

fn game() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Enchantment"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        615,
    )
    .unwrap()
}

fn static_rule(modifiers: Vec<Value>) -> Value {
    json!({
        "kind": "staticAbility",
        "source": {"kind": "self"},
        "activeWhile": {"kind": "inZone", "object": {"kind": "self"}, "zone": {"kind": "battlefield"}},
        "modifiers": modifiers,
    })
}

fn put(
    e: &mut GameEngine,
    owner: usize,
    id: &str,
    type_line: &str,
    rules: Vec<Value>,
    attached: Option<&str>,
) -> String {
    let mut d = definition(id, type_line);
    d.rules = rules;
    for rule in &d.rules {
        assert!(battlefield_static_rule_supported(rule), "{rule}");
    }
    let mut card = card_instance(
        &format!("player-{owner}"),
        &d,
        200 + e.state.players[owner].battlefield.len(),
    );
    card.attached_to = attached.map(str::to_owned);
    e.prepare_battlefield_entry_identity(&mut card);
    let instance_id = card.instance_id.clone();
    e.state.players[owner].battlefield.push(card);
    instance_id
}

fn damage(e: &GameEngine, instance_id: &str) -> i32 {
    e.state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|card| card.instance_id == instance_id)
        .map(|card| card.damage_marked)
        .expect("permanent on the battlefield")
}

fn permanent(id: &str) -> TargetRef {
    TargetRef::Permanent {
        instance_id: id.to_string(),
    }
}

fn player(index: usize) -> TargetRef {
    TargetRef::Player {
        player_id: format!("player-{index}"),
    }
}

/// `Prevent all damage that would be dealt to this creature by Walls.`
/// shape: only a source satisfying the selector is prevented.
#[test]
fn source_slot_scopes_the_prevention_to_matching_sources() {
    let mut e = game();
    let guarded = put(
        &mut e,
        0,
        "guarded",
        "Creature - Test",
        vec![static_rule(vec![json!({
            "kind": "preventScopedDamage",
            "damage": "any",
            "recipients": {"objects": {"kind": "self"}},
            "sources": {"kind": "permanents", "where": {"kind": "cardTypeContains", "value": "Artifact"}},
        })])],
        None,
    );
    let artifact = put(
        &mut e,
        1,
        "artifact-source",
        "Artifact Creature - Golem",
        vec![],
        None,
    );
    let creature = put(
        &mut e,
        1,
        "creature-source",
        "Creature - Bear",
        vec![],
        None,
    );
    e.deal_damage_amount_with_kind(permanent(&guarded), 2, &artifact, false)
        .unwrap();
    assert_eq!(damage(&e, &guarded), 0, "an artifact source is prevented");
    e.deal_damage_amount_with_kind(permanent(&guarded), 1, &creature, false)
        .unwrap();
    assert_eq!(damage(&e, &guarded), 1, "another source still deals damage");
}

/// `Prevent all noncombat damage that would be dealt to other creatures you
/// control.`: combat damage and the source itself are outside the scope.
#[test]
fn damage_kind_and_recipient_slots_are_independent() {
    let mut e = game();
    let shield = put(
        &mut e,
        0,
        "shield",
        "Creature - Test",
        vec![static_rule(vec![json!({
            "kind": "preventScopedDamage",
            "damage": "noncombat",
            "recipients": {"objects": {
                "kind": "permanents",
                "controller": {"kind": "controllerOf", "object": {"kind": "self"}},
                "where": {"kind": "cardTypeContains", "value": "Creature"},
                "excludeSource": true,
            }},
        })])],
        None,
    );
    let ally = put(&mut e, 0, "ally", "Creature - Bear", vec![], None);
    let enemy = put(&mut e, 1, "enemy", "Creature - Bear", vec![], None);
    e.deal_damage_amount_with_kind(permanent(&ally), 2, &enemy, false)
        .unwrap();
    assert_eq!(
        damage(&e, &ally),
        0,
        "noncombat damage to another creature you control"
    );
    e.deal_damage_amount_with_kind(permanent(&ally), 1, &enemy, true)
        .unwrap();
    assert_eq!(damage(&e, &ally), 1, "combat damage is outside the scope");
    e.deal_damage_amount_with_kind(permanent(&shield), 1, &enemy, false)
        .unwrap();
    assert_eq!(damage(&e, &shield), 1, "the source is not another creature");
}

/// `Prevent all combat damage that would be dealt to and dealt by enchanted
/// creature.`: two scopes of one attachment subject, re-read as the Aura
/// moves.
#[test]
fn dealt_to_and_dealt_by_scopes_follow_the_attachment() {
    let mut e = game();
    let first = put(&mut e, 0, "first", "Creature - Bear", vec![], None);
    let second = put(&mut e, 1, "second", "Creature - Bear", vec![], None);
    let attached = json!({"kind": "attachedPermanent", "attachment": {"kind": "self"}});
    let aura = put(
        &mut e,
        0,
        "aura",
        "Enchantment - Aura",
        vec![static_rule(vec![
            json!({"kind": "preventScopedDamage", "damage": "combat", "recipients": {"objects": attached.clone()}}),
            json!({"kind": "preventScopedDamage", "damage": "combat", "sources": attached}),
        ])],
        Some(&first),
    );
    e.deal_damage_amount_with_kind(permanent(&second), 2, &first, true)
        .unwrap();
    assert_eq!(
        damage(&e, &second),
        0,
        "combat damage dealt by the enchanted creature"
    );
    e.deal_damage_amount_with_kind(permanent(&first), 2, &second, true)
        .unwrap();
    assert_eq!(
        damage(&e, &first),
        0,
        "combat damage dealt to the enchanted creature"
    );
    e.deal_damage_amount_with_kind(permanent(&second), 1, &first, false)
        .unwrap();
    assert_eq!(
        damage(&e, &second),
        1,
        "noncombat damage is outside the scope"
    );
    let aura_card = e.state.players[0]
        .battlefield
        .iter_mut()
        .find(|card| card.instance_id == aura)
        .unwrap();
    aura_card.attached_to = Some(second.clone());
    e.deal_damage_amount_with_kind(permanent(&first), 2, &second, true)
        .unwrap();
    assert_eq!(
        damage(&e, &first),
        0,
        "the new enchanted creature deals no combat damage"
    );
}

/// `Prevent all damage that would be dealt to you by creatures.` shape:
/// the player slot follows the prevention source's controller.
#[test]
fn player_recipient_slot_reads_the_controller_population() {
    let mut e = game();
    put(
        &mut e,
        0,
        "ward",
        "Enchantment",
        vec![static_rule(vec![json!({
            "kind": "preventScopedDamage",
            "damage": "any",
            "recipients": {"player": {"kind": "controllerOf", "object": {"kind": "self"}}},
            "sources": {"kind": "permanents", "where": {"kind": "cardTypeContains", "value": "Creature"}},
        })])],
        None,
    );
    let creature = put(&mut e, 1, "creature", "Creature - Bear", vec![], None);
    let artifact = put(&mut e, 1, "rock", "Artifact", vec![], None);
    e.deal_damage_amount_with_kind(player(0), 3, &creature, false)
        .unwrap();
    assert_eq!(
        e.state.players[0].life, 20,
        "creature damage to you is prevented"
    );
    e.deal_damage_amount_with_kind(player(0), 2, &artifact, false)
        .unwrap();
    assert_eq!(
        e.state.players[0].life, 18,
        "another source is not in scope"
    );
    e.deal_damage_amount_with_kind(player(1), 1, &creature, false)
        .unwrap();
    assert_eq!(
        e.state.players[1].life, 19,
        "another player is not in scope"
    );
}

#[test]
fn unscoped_or_malformed_filters_are_not_supported() {
    for modifier in [
        json!({"kind": "preventScopedDamage", "damage": "any"}),
        json!({"kind": "preventScopedDamage", "damage": "ranged", "sources": {"kind": "self"}}),
        json!({"kind": "preventScopedDamage", "damage": "any", "recipients": {"objects": {"kind": "self"}, "player": {"kind": "controllerOf", "object": {"kind": "self"}}}}),
    ] {
        assert!(
            !battlefield_static_rule_supported(&static_rule(vec![modifier.clone()])),
            "{modifier}"
        );
    }
}

/// `During your turn, prevent all damage that would be dealt to this
/// creature.` (Guardian Naga): the wrapper's gate is the rule's condition,
/// re-evaluated whenever damage would be dealt, so the turn toggles it.
#[test]
fn rule_gate_toggles_a_static_prevention_with_the_active_player() {
    let mut e = game();
    let mut rule = static_rule(vec![
        json!({"kind": "preventDamage", "objects": {"kind": "self"}}),
    ]);
    rule["condition"] = json!({
        "kind": "duringControllerTurn",
        "player": {"kind": "controllerOf", "object": {"kind": "self"}},
    });
    let naga = put(&mut e, 0, "naga", "Creature - Naga", vec![rule], None);
    let enemy = put(&mut e, 1, "enemy", "Creature - Bear", vec![], None);
    e.state.active_player = 0;
    e.deal_damage_amount_with_kind(permanent(&naga), 2, &enemy, false)
        .unwrap();
    assert_eq!(
        damage(&e, &naga),
        0,
        "prevented during its controller's turn"
    );
    e.state.active_player = 1;
    e.deal_damage_amount_with_kind(permanent(&naga), 1, &enemy, false)
        .unwrap();
    assert_eq!(damage(&e, &naga), 1, "dealt during another player's turn");
}
