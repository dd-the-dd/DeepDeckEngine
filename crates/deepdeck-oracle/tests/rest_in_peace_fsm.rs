use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

const REST_IN_PEACE_ORACLE: &str = "When this enchantment enters, exile all graveyards.\nIf a card or token would be put into a graveyard from anywhere, exile it instead.";

fn parse_enchantment(oracle_text: &str) -> deepdeck_oracle::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Graveyard Seal".to_string(),
        type_line: "Enchantment".to_string(),
        mana_cost: Some("{1}{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Rest in Peace; see docs/coverage-targets.md"
)]
#[test]
fn rest_in_peace_uses_native_entry_and_zone_destination_automata() {
    let parsed = parse_enchantment(REST_IN_PEACE_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert!(parsed.abilities.iter().all(|ability| {
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    }));

    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&json!({
            "kind": "triggeredAbility",
            "source": { "kind": "self" },
            "event": { "kind": "enterBattlefield", "object": { "kind": "self" } },
            "effects": [{ "kind": "exileAllGraveyards" }],
        }))
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&json!({
            "kind": "staticAbility",
            "source": { "kind": "self" },
            "activeWhile": {
                "kind": "inZone",
                "object": { "kind": "self" },
                "zone": { "kind": "battlefield" },
            },
            "modifiers": [{
                "kind": "replaceGraveyardDestination",
                "graveyardOwners": { "kind": "eachPlayer" },
                "where": null,
                "from": { "kind": "anyZone" },
                "to": { "kind": "exile" },
            }],
        }))
    );
}

#[test]
fn replacement_subautomata_compose_object_kind_and_graveyard_owner_scope() {
    let cases = [
        (
            "If a card would be put into an opponent's graveyard from anywhere, exile it instead.",
            json!({
                "owners": {
                    "kind": "opponentsOf",
                    "player": {
                        "kind": "controllerOf",
                        "object": { "kind": "self" },
                    },
                },
                "where": { "kind": "not", "operand": { "kind": "isToken" } },
            }),
        ),
        (
            "If a token would be put into your graveyard from anywhere, exile it instead.",
            json!({
                "owners": {
                    "kind": "controllerOf",
                    "object": { "kind": "self" },
                },
                "where": { "kind": "isToken" },
            }),
        ),
    ];

    for (oracle, expected) in cases {
        let parsed = parse_enchantment(oracle);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.status, "canonical", "{oracle}");
        assert_eq!(ability.recognizer, RecognizerProvenance::Native, "{oracle}");
        let modifier = &ability.rule.as_ref().expect("native rule")["modifiers"][0];
        assert_eq!(modifier["graveyardOwners"], expected["owners"], "{oracle}");
        assert_eq!(modifier["where"], expected["where"], "{oracle}");
        assert_eq!(modifier["from"]["kind"], "anyZone", "{oracle}");
        assert_eq!(modifier["to"]["kind"], "exile", "{oracle}");
    }
}

#[test]
fn committed_replacement_family_rejects_a_specific_origin_without_widening_it() {
    let parsed = parse_enchantment(
        "If a card would be put into a graveyard from your hand, exile it instead.",
    );

    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_ne!(ability.status, "canonical");
    let diagnostic = ability
        .diagnostics
        .first()
        .expect("committed native failure is surfaced");
    // The Rest in Peace family claims only its own `from anywhere` text;
    // the shared departure frame reads `from anywhere` or `from the
    // battlefield`. A hand origin is neither, so the shared replacement
    // frame refuses the event explicitly instead of widening any reading.
    assert_eq!(diagnostic.automaton.as_deref(), Some("ability-frame"));
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("ability-frame.replacement.event"),
        "{:#?}",
        ability.diagnostics
    );
    assert!(ability.rule.is_none());
}
