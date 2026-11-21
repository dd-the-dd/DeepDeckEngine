use deepdeck_engine_runtime::{
    model::{PlayableCardInput, compile_playable_card},
    oracle::{OracleCardFace, OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};
use deepdeck_oracle_engine_contracts::engine::rule_is_executable;

const FRONT_ORACLE: &str = "Flying, ward {2}\nThis creature enters prepared.\nWhenever this creature attacks, you may exile eight cards from your graveyard. If you do, this creature becomes prepared.";
const SPELL_ORACLE: &str = "Target player draws three cards.";

fn request() -> OracleCardParseRequest {
    OracleCardParseRequest {
        card_name: "Emeritus of Ideation // Ancestral Recall".to_string(),
        type_line: "Creature — Human Wizard // Instant".to_string(),
        mana_cost: Some("{3}{U}{U} // {U}".to_string()),
        oracle_text: None,
        layout: Some("prepare".to_string()),
        faces: vec![
            OracleCardFace {
                id: "emeritus-of-ideation".to_string(),
                name: "Emeritus of Ideation".to_string(),
                type_line: "Creature — Human Wizard".to_string(),
                mana_cost: Some("{3}{U}{U}".to_string()),
                oracle_text: FRONT_ORACLE.to_string(),
                power: Some("5".to_string()),
                toughness: Some("5".to_string()),
                loyalty: None,
            },
            OracleCardFace {
                id: "ancestral-recall".to_string(),
                name: "Ancestral Recall".to_string(),
                type_line: "Instant".to_string(),
                mana_cost: Some("{U}".to_string()),
                oracle_text: SPELL_ORACLE.to_string(),
                power: None,
                toughness: None,
                loyalty: None,
            },
        ],
    }
}

#[test]
fn emeritus_rules_are_executable_and_prepare_layout_links_the_spell_face() {
    let parsed = parse_oracle_card(request());
    assert_eq!(parsed.status, "canonical");
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    for ability in &parsed.abilities {
        let rule = ability.rule.as_ref().expect("native rule");
        assert!(rule_is_executable(rule), "unexecutable rule: {rule}");
        assert!(!rule.to_string().contains("resolveTriggeredInstruction"));
    }

    let compiled = compile_playable_card(PlayableCardInput {
        id: "emeritus-of-ideation".to_string(),
        face_id: Some("emeritus-of-ideation".to_string()),
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: Some("5".to_string()),
        toughness: Some("5".to_string()),
        oracle: request(),
    })
    .expect("prepare card compiles");
    let prepare = compiled
        .card
        .rules
        .iter()
        .find(|rule| rule["kind"] == "prepareSpell")
        .expect("permanent face receives prepareSpell linkage");
    assert_eq!(prepare["spell"]["id"], "ancestral-recall");
    assert_eq!(prepare["spell"]["rules"][0]["kind"], "spellAbility");
    assert_eq!(
        prepare["spell"]["rules"][0]["effects"][0]["kind"],
        "drawCards"
    );
    assert!(rule_is_executable(prepare));
}
