use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const GHOST_VACUUM_RETURN: &str = "{6}, {T}, Sacrifice this artifact: Put each creature card exiled with this artifact onto the battlefield under your control with a flying counter on it. Each of them is a 1/1 Spirit in addition to its other types. Activate only as a sorcery.";

fn parse(type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Linked Return Artifact".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn ghost_vacuum_return_is_native_and_lowers_to_one_primitive_move() {
    let parsed = parse("Artifact", GHOST_VACUUM_RETURN);
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule.as_ref(),
        Some(&json!({
            "kind": "activatedAbility",
            "source": { "kind": "self" },
            "costs": [
                { "kind": "payMana", "manaCost": "{6}" },
                { "kind": "tap", "object": { "kind": "self" } },
                { "kind": "sacrificePermanent", "permanent": { "kind": "self" } },
            ],
            "activationCondition": { "kind": "sorceryTiming" },
            "effects": [{
                "kind": "moveCards",
                "cards": {
                    "kind": "filterObjects",
                    "objects": { "kind": "cardsExiledWithSource" },
                    "where": { "kind": "cardTypeContains", "value": "Creature" },
                },
                "to": {
                    "kind": "battlefield",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "tapped": false,
                    "enterWithCounters": [{
                        "counter": "flying",
                        "count": { "kind": "integer", "value": 1 },
                    }],
                    "addTypes": ["Creature"],
                    "addSubtypes": ["Spirit"],
                    "basePower": { "kind": "integer", "value": 1 },
                    "baseToughness": { "kind": "integer", "value": 1 },
                    "retainExistingTypes": true,
                },
            }],
        }))
    );
    assert!(
        !ability
            .rule
            .as_ref()
            .unwrap()
            .to_string()
            .contains("resolve")
    );
}

#[test]
fn card_types_counter_kind_statistics_and_subtype_are_reusable() {
    let sibling = "{3}, Sacrifice this enchantment: Put each artifact card exiled with this enchantment onto the battlefield under your control with a shadow counter on it. Each of them is a 2/3 Shark in addition to its other types. Activate only as a sorcery.";
    let parsed = parse("Enchantment", sibling);
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("sibling emits primitive IR");
    assert_eq!(
        rule["effects"][0]["cards"]["where"],
        json!({ "kind": "cardTypeContains", "value": "Artifact" })
    );
    assert_eq!(
        rule["effects"][0]["to"]["enterWithCounters"][0]["counter"],
        "shadow"
    );
    assert_eq!(rule["effects"][0]["to"]["basePower"]["value"], 2);
    assert_eq!(rule["effects"][0]["to"]["baseToughness"]["value"], 3);
    assert_eq!(rule["effects"][0]["to"]["addSubtypes"][0], "Shark");
}

#[test]
fn malformed_linked_characteristic_retention_is_a_committed_native_failure() {
    let near_miss = GHOST_VACUUM_RETURN.replace(
        "in addition to its other types",
        "instead of its other types",
    );
    let parsed = parse("Artifact", &near_miss);
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("linked-exile-return-activation")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("characteristic.linked-set.retention")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("instead"));
}

#[test]
fn promoted_linked_return_vocabulary_has_no_unknown_tokens() {
    let parsed = parse("Artifact", GHOST_VACUUM_RETURN);
    let unknown = parsed.abilities[0]
        .entities
        .iter()
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();

    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
