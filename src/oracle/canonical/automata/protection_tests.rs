use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};
use serde_json::{Value, json};

fn single_rule(name: &str, type_line: &str, text: &str) -> Result<Value, (String, String)> {
    let result = parse_oracle_card_native(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    let [ability] = result.abilities.as_slice() else {
        panic!(
            "{name}: expected one ability, got {}",
            result.abilities.len()
        );
    };
    match &ability.rule {
        Some(rule) => Ok(rule.clone()),
        None => {
            let diagnostic = &ability.diagnostics[0];
            Err((
                diagnostic.code.clone(),
                diagnostic.state.clone().unwrap_or_default(),
            ))
        }
    }
}

/// Real cards, a sibling with an unused quality combination, and near-misses
/// of the shared protection atom in a standalone keyword line (CR 702.16a).
#[test]
fn protection_keyword_lines_lower_typed_qualities() {
    let cases: &[(&str, &str, &str, Value)] = &[
        (
            "Mountain Yeti",
            "Creature — Yeti",
            "Protection from white",
            json!({"kind": "keywordAbility", "ability": {"kind": "protection", "from": ["white"]}}),
        ),
        (
            "Black Knight",
            "Creature — Human Knight",
            "Protection from white (This creature can't be blocked, targeted, dealt damage, or enchanted by anything white.)",
            json!({"kind": "keywordAbility", "ability": {"kind": "protection", "from": ["white"]}}),
        ),
        (
            "Sparkhunter Masticore",
            "Artifact Creature — Masticore",
            "Protection from planeswalkers",
            json!({"kind": "keywordAbility", "ability": {"kind": "protection", "from": ["Planeswalker"]}}),
        ),
        (
            "Generic Paladin",
            "Creature — Human Knight",
            "Flying, protection from red and from artifacts",
            json!({"kind": "keywordAbilityGroup", "abilities": [
                {"kind": "flying"},
                {"kind": "protection", "from": ["red", "Artifact"]},
            ]}),
        ),
        (
            "Generic Prism",
            "Creature — Spirit",
            "Protection from each color",
            json!({"kind": "keywordAbility", "ability": {"kind": "protection",
                "from": ["white", "blue", "black", "red", "green"]}}),
        ),
    ];
    for (name, type_line, text, expected) in cases {
        let rule =
            single_rule(name, type_line, text).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let kind = expected["kind"].as_str().unwrap();
        assert_eq!(rule["kind"], kind, "{name}: {rule}");
        if kind == "keywordAbility" {
            assert_eq!(rule["ability"], expected["ability"], "{name}");
        } else {
            assert_eq!(rule["abilities"], expected["abilities"], "{name}");
        }
    }
}

#[test]
fn unsupported_protection_qualities_and_reminders_fail_inside_the_atom() {
    let cases: &[(&str, &str)] = &[
        // A subtype quality is not yet evaluated by the engine.
        ("Protection from Vampires", "keyword.protection.quality"),
        ("Protection from multicolored", "keyword.protection.quality"),
        // A reminder naming another color than the printed quality.
        (
            "Protection from white (This creature can't be blocked, targeted, dealt damage, or enchanted by anything black.)",
            "keyword.protection.reminder.quality",
        ),
    ];
    for (text, state) in cases {
        let (code, actual) =
            single_rule("Generic Knight", "Creature — Knight", text).expect_err(text);
        assert_eq!(code, "oracle_machine_failure", "{text}");
        assert_eq!(actual, *state, "{text}");
    }
}

/// The same atom in the static grant frame lowers to a protection grant.
#[test]
fn granted_protection_reuses_the_atom_in_the_static_frame() {
    let rule = single_rule(
        "Generic Ward",
        "Enchantment — Aura",
        "Enchanted creature gets +1/+1 and has protection from black.",
    )
    .expect("parses");
    assert_eq!(
        rule["modifiers"][1],
        json!({
            "kind": "grantProtection",
            "objects": {"kind": "attachedPermanent", "attachment": {"kind": "self"}},
            "from": ["black"],
        })
    );
}
