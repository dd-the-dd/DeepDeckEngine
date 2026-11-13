use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

const KARAKAS_ORACLE: &str =
    "{T}: Add {W}.\n{T}: Return target legendary creature to its owner's hand.";

fn parse_land(
    card_name: &str,
    oracle_text: &str,
) -> deepdeck_oracle::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Legendary Land".to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn karakas_composes_native_mana_selector_and_owner_hand_automata() {
    let parsed = parse_land("Karakas", KARAKAS_ORACLE);

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
            "kind": "manaAbility",
            "source": { "kind": "self" },
            "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
            "effects": [{
                "kind": "addMana",
                "player": {
                    "kind": "controllerOf",
                    "object": { "kind": "self" },
                },
                "mana": "{W}",
            }],
        }))
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&json!({
            "kind": "activatedAbility",
            "source": { "kind": "self" },
            "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "id": "targetPermanent",
                    "kind": "chooseTargets",
                    "minimum": 1,
                    "maximum": 1,
                    "candidates": {
                        "kind": "permanents",
                        "where": {
                            "kind": "and",
                            "operands": [
                                { "kind": "isLegendary" },
                                { "kind": "cardTypeContains", "value": "Creature" },
                            ],
                        },
                    },
                }],
            },
            "effects": [{
                "kind": "returnToOwnersHand",
                "object": { "kind": "chosenTarget", "id": "targetPermanent" },
            }],
        }))
    );
}

#[test]
fn owner_hand_zone_change_reuses_legendary_with_another_permanent_type() {
    let parsed = parse_land(
        "Test Sanctuary",
        "{T}: Return target legendary artifact to its owner's hand.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("sibling rule");
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"],
        json!({
            "kind": "and",
            "operands": [
                { "kind": "isLegendary" },
                { "kind": "cardTypeContains", "value": "Artifact" },
            ],
        })
    );
    assert_eq!(rule["effects"][0]["kind"], "returnToOwnersHand");
}

#[test]
fn committed_owner_hand_family_rejects_a_different_possessive_destination() {
    let parsed = parse_land(
        "Test Sanctuary",
        "{T}: Return target legendary creature to its controller's hand.",
    );

    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_ne!(ability.status, "canonical");
    let diagnostic = ability
        .diagnostics
        .first()
        .expect("committed native failure is surfaced");
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("compound-zone-change-activation")
    );
    assert!(
        diagnostic
            .state
            .as_deref()
            .is_some_and(|state| state.contains("destination"))
    );
    assert_eq!(diagnostic.found.as_deref(), Some("controller's"));
}
