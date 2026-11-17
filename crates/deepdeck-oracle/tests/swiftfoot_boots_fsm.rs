use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "Equipped creature has hexproof and haste. (It can't be the target of spells or abilities your opponents control. It can attack and {T} no matter when it came under your control.)\nEquip {1} ({1}: Attach to target creature you control. Equip only as a sorcery.)";

fn parse_equipment(card_name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Artifact — Equipment".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn swiftfoot_boots_is_two_native_primitive_abilities_without_losing_a_keyword() {
    let truth: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/oracle-ground-truth/cards/swiftfoot-boots.json"
    ))
    .expect("Swiftfoot Boots ground truth is valid JSON");
    assert_eq!(truth["context"]["oracleText"], EXACT_ORACLE);

    let parsed = parse_equipment("Swiftfoot Boots", EXACT_ORACLE);
    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert!(
        parsed
            .abilities
            .iter()
            .flat_map(|ability| ability.entities.iter())
            .all(|entity| entity.token_type != "unknown")
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&truth["abilities"][0]["expectedRule"])
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&truth["abilities"][1]["expectedRule"])
    );
    let modifiers = parsed.abilities[0].rule.as_ref().unwrap()["modifiers"]
        .as_array()
        .unwrap();
    assert_eq!(modifiers.len(), 2, "every Oracle keyword must survive");
    assert_eq!(modifiers[0]["keyword"], "hexproof");
    assert_eq!(modifiers[1]["keyword"], "haste");
}

#[test]
fn other_keyword_pairs_and_mana_costs_reuse_the_same_children() {
    let parsed = parse_equipment(
        "Test Winged Equipment",
        "Equipped creature has flying and vigilance.\nEquip {3}",
    );

    assert_eq!(parsed.status, "canonical");
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["modifiers"],
        json!([
            {
                "kind": "grantKeyword",
                "objects": {
                    "kind": "attachedPermanent",
                    "attachment": { "kind": "self" }
                },
                "keyword": "flying"
            },
            {
                "kind": "grantKeyword",
                "objects": {
                    "kind": "attachedPermanent",
                    "attachment": { "kind": "self" }
                },
                "keyword": "vigilance"
            }
        ])
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["ability"]["costs"][0],
        json!({ "kind": "payMana", "manaCost": "{3}" })
    );
}

#[test]
fn altered_attachment_keyword_reminder_is_never_accepted() {
    // Reminder text is validated against the granted keyword; an altered body
    // must not be discarded as an arbitrary parenthetical.
    let parsed = parse_equipment(
        "Test Malformed Equipment",
        "Equipped creature has hexproof. (It can't be the target of spells or abilities your opponents own.)",
    );
    let ability = &parsed.abilities[0];
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("own"));
}

#[test]
fn claimed_near_misses_fail_in_the_responsible_child_state() {
    for (oracle_text, automaton, state, found) in [
        (
            "Equipped creature has flying and.",
            "static-continuous-effect",
            "keyword-list.item",
            ".",
        ),
        (
            "Equip {2} ({3}: Attach to target creature you control. Equip only as a sorcery.)",
            "keyword-equip",
            "keyword.equip.reminder.cost-match",
            ":",
        ),
        (
            "Equip {1} ({1}: Attach to target creature you control. Equip only as an instant.)",
            "keyword-equip",
            "keyword.equip.reminder.article",
            "an",
        ),
    ] {
        let parsed = parse_equipment("Test Malformed Equipment", oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.status, "unsupported");
        assert!(ability.rule.is_none());
        assert_eq!(ability.diagnostics[0].automaton.as_deref(), Some(automaton));
        assert_eq!(ability.diagnostics[0].state.as_deref(), Some(state));
        assert_eq!(ability.diagnostics[0].found.as_deref(), Some(found));
    }
}
