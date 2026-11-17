use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

const GET_LOST_ORACLE: &str = "Destroy target creature, enchantment, or planeswalker. Its controller creates two Map tokens. (They're artifacts with \"{1}, {T}, Sacrifice this token: Target creature you control explores. Activate only as a sorcery.\")";

fn parse(name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn self_ref() -> Value {
    json!({ "kind": "self" })
}

fn target() -> Value {
    json!({ "kind": "chosenTarget", "id": "targetPermanent" })
}

fn target_type_union() -> Value {
    json!({
        "kind": "or",
        "operands": [
            { "kind": "cardTypeContains", "value": "Creature" },
            { "kind": "cardTypeContains", "value": "Enchantment" },
            { "kind": "cardTypeContains", "value": "Planeswalker" },
        ],
    })
}

#[test]
fn get_lost_is_native_primitive_and_uses_the_registered_map_identity() {
    let parsed = parse("Get Lost", GET_LOST_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    );
    assert_eq!(
        ability.rule.as_ref(),
        Some(&json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "id": "targetPermanent",
                    "kind": "chooseTargets",
                    "minimum": 1,
                    "maximum": 1,
                    "candidates": {
                        "kind": "permanents",
                        "where": target_type_union(),
                    },
                }],
            },
            "effects": [
                {
                    "kind": "bind",
                    "id": "changedPermanentController",
                    "value": { "kind": "controllerOf", "object": target() },
                },
                { "kind": "destroyPermanent", "permanent": target() },
                {
                    "kind": "createTokens",
                    "controller": { "kind": "boundValue", "id": "changedPermanentController" },
                    "quantity": { "kind": "integer", "value": 2 },
                    "token": { "kind": "namedToken", "name": "Map" },
                },
            ],
        }))
    );
    let serialized = ability.rule.as_ref().unwrap().to_string();
    assert!(!serialized.contains("createMapToken"));
    assert!(!serialized.contains("resolveSpellInstruction"));
    assert!(!serialized.contains("explore"));
}

#[test]
fn named_token_identity_and_optional_reminder_generalize_beyond_a_hardcoded_trio() {
    for (text, expected_name, expected_quantity) in [
        (
            "Destroy target artifact. Its controller creates a Blood token.",
            "Blood",
            1,
        ),
        (
            "Exile target enchantment. Its controller creates three Treasure tokens. (They're artifacts with \"{T}, Sacrifice this token: Add one mana of any color.\")",
            "Treasure",
            3,
        ),
    ] {
        let parsed = parse("Catalog Token Exchange", text);
        let ability = &parsed.abilities[0];
        assert_eq!(parsed.status, "canonical", "{text}");
        assert_eq!(ability.recognizer, RecognizerProvenance::Native, "{text}");
        let rule = ability.rule.as_ref().expect("native catalog-token rule");
        assert_eq!(rule["effects"][2]["token"]["kind"], "namedToken");
        assert_eq!(rule["effects"][2]["token"]["name"], expected_name);
        assert_eq!(rule["effects"][2]["quantity"]["value"], expected_quantity);
    }
}

#[test]
fn unknown_named_token_is_a_committed_catalog_identity_failure() {
    let parsed = parse(
        "Catalog Token Exchange",
        "Destroy target creature. Its controller creates two Mystery tokens.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("zone-change-create-token")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("token.named.catalog-identity")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("Mystery"));
}

#[test]
fn reminder_must_describe_the_registered_token_semantics() {
    for text in [
        "Destroy target creature. Its controller creates two Map tokens. (They're artifacts with \"{T}, Sacrifice this token: Add one mana of any color.\")",
        "Destroy target creature. Its controller creates two Map tokens. (They're colorless Equipment artifacts with \"Equipped creature gets +1/+1\" and equip {1}.)",
    ] {
        let parsed = parse("Catalog Token Exchange", text);
        let ability = &parsed.abilities[0];

        assert_eq!(ability.recognizer, RecognizerProvenance::Native, "{text}");
        assert_eq!(ability.status, "unsupported", "{text}");
        assert!(ability.rule.is_none(), "{text}");
        assert_eq!(
            ability.diagnostics[0].automaton.as_deref(),
            Some("zone-change-create-token")
        );
        assert_eq!(
            ability.diagnostics[0].state.as_deref(),
            Some("reminder.named-token.semantic-match")
        );
    }
}

#[test]
fn named_token_sequence_requires_full_consumption() {
    let parsed = parse(
        "Catalog Token Exchange",
        "Destroy target creature. Its controller creates two Map tokens. Draw a card.",
    );
    let ability = &parsed.abilities[0];

    // The token family rejects the extra sentence; the shared sequence then
    // composes all three actions over the same target binding.
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "canonical");
    let target = serde_json::json!({ "kind": "chosenTarget", "id": "targetPermanent" });
    assert_eq!(
        ability.rule.as_ref().expect("shared sequence rule")["effects"],
        serde_json::json!([
            { "kind": "destroyPermanent", "permanent": target.clone() },
            {
                "kind": "createTokens",
                "controller": { "kind": "controllerOf", "object": target },
                "quantity": { "kind": "integer", "value": 2 },
                "token": { "kind": "namedToken", "name": "Map" },
            },
            {
                "kind": "drawCards",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "count": { "kind": "integer", "value": 1 },
            },
        ])
    );
}
