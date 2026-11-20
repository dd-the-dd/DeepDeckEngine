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

#[test]
fn unsupported_static_prevention_scopes_fail_inside_the_frame() {
    let cases: &[(&str, &str)] = &[
        (
            "Prevent all damage that would be dealt to you.",
            "static.prevent.recipient",
        ),
        (
            "Prevent all combat damage that would be dealt to and dealt by this creature.",
            "static.prevent.recipient",
        ),
        (
            "Prevent all damage that would be dealt to this creature by artifacts.",
            "static.prevent.lowering",
        ),
        (
            "Prevent all damage that would be dealt to this creature this turn.",
            "static.prevent.terminator",
        ),
    ];
    for (text, state) in cases {
        let actual = single_rule("Generic Wall", "Creature — Wall", text).expect_err(text);
        assert_eq!(actual, *state, "{text}");
    }
}
