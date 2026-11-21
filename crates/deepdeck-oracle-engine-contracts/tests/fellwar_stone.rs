use deepdeck_oracle_engine_contracts::oracle::{
    OracleCardParseRequest, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

#[test]
fn opposing_land_color_domain_preserves_the_executable_mana_contract() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Fellwar Stone".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(
            "{T}: Add one mana of any color that a land an opponent controls could produce."
                .to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["effects"][0]["mana"],
        json!({
            "kind": "manaColorsLandsOpponentsControlCouldProduce",
            "amount": 1,
        })
    );
}
