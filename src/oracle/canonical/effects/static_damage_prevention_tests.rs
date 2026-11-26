use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};
use serde_json::{Value, json};

fn single_rule(name: &str, type_line: &str, text: &str) -> Result<Value, String> {
    let result = parse_oracle_card_native(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    let [ability] = result.abilities.as_slice() else {
        panic!("{name}: expected one ability");
    };
    ability.rule.clone().ok_or_else(|| {
        ability.diagnostics[0]
            .state
            .clone()
            .unwrap_or_else(|| ability.diagnostics[0].code.clone())
    })
}

fn modifier(name: &str, type_line: &str, text: &str) -> Value {
    let rule = single_rule(name, type_line, text).unwrap_or_else(|state| panic!("{name}: {state}"));
    assert_eq!(rule["kind"], "staticAbility", "{name}");
    assert_eq!(rule["activeWhile"]["zone"]["kind"], "battlefield", "{name}");
    rule["modifiers"][0].clone()
}

/// Real cards and a sibling (an unused collection recipient with the combat
/// restriction) of the static prevention frame (CR 615.1a).
#[test]
fn static_prevention_scopes_lower_to_engine_modifiers() {
    let attached = json!({"kind": "attachedPermanent", "attachment": {"kind": "self"}});
    let cases: &[(&str, &str, &str, Value)] = &[
        (
            "Guard Gomazoa",
            "Creature — Gomazoa",
            "Prevent all combat damage that would be dealt to this creature.",
            json!({"kind": "preventDamage", "objects": {"kind": "self"}, "combatOnly": true}),
        ),
        (
            "Cho-Manno, Revolutionary",
            "Legendary Creature — Human Rebel",
            "Prevent all damage that would be dealt to Cho-Manno.",
            json!({"kind": "preventDamage", "objects": {"kind": "self"}}),
        ),
        (
            "General's Kabuto",
            "Legendary Artifact — Equipment",
            "Prevent all combat damage that would be dealt to equipped creature.",
            json!({"kind": "preventDamage", "objects": attached, "combatOnly": true}),
        ),
        (
            "Uncle Istvan",
            "Creature — Human",
            "Prevent all damage that would be dealt to this creature by creatures.",
            json!({"kind": "preventCreatureDamageToSelf", "object": {"kind": "self"}}),
        ),
    ];
    for (name, type_line, text, expected) in cases {
        assert_eq!(&modifier(name, type_line, text), expected, "{name}");
    }
    let sibling = modifier(
        "Generic Bulwark",
        "Enchantment",
        "Prevent all combat damage that would be dealt to creatures you control.",
    );
    assert_eq!(sibling["kind"], "preventDamage");
    assert_eq!(sibling["combatOnly"], true);
    assert_eq!(sibling["objects"]["kind"], "permanents");
    assert_eq!(sibling["objects"]["controller"]["kind"], "controllerOf");
}

/// The prevention filter's independent slots (CR 615.1a): a player
/// recipient, a damage kind, a source collection, and one subject filling
/// both the recipient and the source slot. Real cards and an unused
/// combination (a noncombat prevention for you and your creatures, scoped
/// to artifact sources) lower without a new operation.
#[test]
fn static_prevention_slots_compose_into_scoped_filters() {
    let controller = json!({"kind": "controllerOf", "object": {"kind": "self"}});
    let attached = json!({"kind": "attachedPermanent", "attachment": {"kind": "self"}});
    // Glacial Chasm: the historical player prevention.
    assert_eq!(
        modifier(
            "Glacial Chasm",
            "Land",
            "Prevent all damage that would be dealt to you."
        ),
        json!({"kind": "preventPlayerDamage", "player": controller}),
    );
    // Tajic, Legion's Edge: a noncombat kind over another-creatures recipients.
    let tajic = modifier(
        "Tajic, Legion's Edge",
        "Legendary Creature — Human Soldier",
        "Prevent all noncombat damage that would be dealt to other creatures you control.",
    );
    assert_eq!(tajic["kind"], "preventScopedDamage");
    assert_eq!(tajic["damage"], "noncombat");
    assert_eq!(tajic["recipients"]["objects"]["excludeSource"], true);
    assert!(tajic.get("sources").is_none());
    // Fog Bank: one subject, two scopes.
    let rule = single_rule(
        "Fog Bank",
        "Creature — Wall",
        "Prevent all combat damage that would be dealt to and dealt by this creature.",
    )
    .expect("Fog Bank");
    assert_eq!(
        rule["modifiers"],
        json!([
            {"kind": "preventDamage", "objects": {"kind": "self"}, "combatOnly": true},
            {"kind": "preventScopedDamage", "damage": "combat", "sources": {"kind": "self"}},
        ])
    );
    // Muzzle: the source slot alone.
    assert_eq!(
        modifier(
            "Muzzle",
            "Enchantment — Aura",
            "Prevent all damage that would be dealt by enchanted creature."
        ),
        json!({"kind": "preventScopedDamage", "damage": "any", "sources": attached}),
    );
    // Desert Nomads: a recipient and a source collection.
    let nomads = modifier(
        "Desert Nomads",
        "Creature — Human Nomad",
        "Prevent all damage that would be dealt to this creature by Deserts.",
    );
    assert_eq!(nomads["kind"], "preventScopedDamage");
    assert_eq!(nomads["recipients"], json!({"objects": {"kind": "self"}}));
    assert_eq!(nomads["sources"]["kind"], "permanents");
    // Unused combination: two recipients, a damage kind and a source.
    let rule = single_rule(
        "Generic Bulwark",
        "Enchantment",
        "Prevent all noncombat damage that would be dealt to you and creatures you control by artifact creatures.",
    )
    .expect("sibling");
    let modifiers = rule["modifiers"].as_array().expect("modifiers");
    assert_eq!(modifiers.len(), 2);
    assert_eq!(modifiers[0]["recipients"], json!({"player": controller}));
    assert_eq!(modifiers[1]["recipients"]["objects"]["kind"], "permanents");
    assert!(modifiers.iter().all(|modifier| {
        modifier["kind"] == "preventScopedDamage"
            && modifier["damage"] == "noncombat"
            && modifier["sources"]["kind"] == "permanents"
    }));
}

#[test]
fn unsupported_static_prevention_scopes_fail_inside_the_frame() {
    let cases: &[(&str, &str)] = &[
        // A spell source is not a battlefield permanent the filter reads.
        (
            "Prevent all damage that would be dealt to this creature by instant spells.",
            "engine.gap.source-scoped-prevention",
        ),
        (
            "Prevent all damage that would be dealt to this creature this turn.",
            "static.prevent.terminator",
        ),
        // A relational source (`creatures it's blocking`): the shared query
        // reads the combat relation, whose anchor the static prevention
        // frame does not bind.
        (
            "Prevent all damage that would be dealt to this creature by creatures it's blocking.",
            "selector.relation.anchor.contracted-pronoun",
        ),
    ];
    for (text, state) in cases {
        let actual = single_rule("Generic Wall", "Creature — Wall", text).expect_err(text);
        assert_eq!(actual, *state, "{text}");
    }
}
