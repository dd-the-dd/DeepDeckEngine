use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

const RAMPANT_GROWTH_ORACLE: &str = "Search your library for a basic land card, put that card onto the battlefield tapped, then shuffle.";

#[test]
fn rampant_growth_reuses_the_native_tapped_basic_land_search_machine() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Rampant Growth".to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{1}{G}".to_string()),
        oracle_text: Some(RAMPANT_GROWTH_ORACLE.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

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
    let rule = ability.rule.as_ref().expect("primitive search rule");
    assert_eq!(
        rule["effects"][0]["candidates"]["where"],
        json!({
            "kind": "and",
            "operands": [
                { "kind": "typeLineContains", "value": "Basic" },
                { "kind": "cardTypeContains", "value": "Land" },
            ],
        })
    );
    assert_eq!(rule["effects"][1]["kind"], "moveCards");
    assert_eq!(rule["effects"][1]["to"]["kind"], "battlefield");
    assert_eq!(rule["effects"][1]["to"]["tapped"], true);
    assert_eq!(rule["effects"][2]["kind"], "shuffleZone");
    assert_eq!(rule["effects"][3]["kind"], "recordLibrarySearch");
    assert!(!rule.to_string().contains("resolveSpellInstruction"));
}
