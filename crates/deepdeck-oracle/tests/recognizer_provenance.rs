use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};

fn request(
    name: &str,
    type_line: &str,
    mana_cost: &str,
    oracle_text: &str,
) -> OracleCardParseRequest {
    OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    }
}

#[test]
fn lightning_helix_is_entirely_native() {
    let parsed = parse_oracle_card(request(
        "Lightning Helix",
        "Instant",
        "{R}{W}",
        "Lightning Helix deals 3 damage to any target and you gain 3 life.",
    ));

    assert_eq!(parsed.status, "canonical");
    assert!(!parsed.abilities.is_empty());
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
}

#[test]
fn burst_lightning_is_entirely_native_after_kicker_promotion() {
    let parsed = parse_oracle_card(request(
        "Burst Lightning",
        "Instant",
        "{R}",
        "Kicker {4} (You may pay an additional {4} as you cast this spell.)\nBurst Lightning deals 2 damage to any target. If this spell was kicked, it deals 4 damage to that target instead.",
    ));

    assert_eq!(parsed.status, "canonical");
    assert!(!parsed.abilities.is_empty());
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
}

#[test]
fn one_native_ability_does_not_hide_a_legacy_keyword_ability() {
    let parsed = parse_oracle_card(request(
        "Mixed Proof",
        "Instant",
        "{R}",
        "Buyback {3}\nMixed Proof deals 3 damage to any target and you gain 3 life.",
    ));

    assert_eq!(parsed.status, "canonical");
    assert!(
        parsed
            .abilities
            .iter()
            .any(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert!(
        parsed
            .abilities
            .iter()
            .any(|ability| ability.recognizer == RecognizerProvenance::Legacy)
    );
    assert!(
        !parsed
            .abilities
            .iter()
            .all(|ability| ability.status == "canonical"
                && ability.recognizer == RecognizerProvenance::Native)
    );
}

#[test]
fn promoted_simple_keyword_is_reported_as_native() {
    let parsed = parse_oracle_card(request(
        "Wind Drake",
        "Creature — Drake",
        "{2}{U}",
        "Flying",
    ));

    assert_eq!(parsed.status, "canonical");
    assert!(!parsed.abilities.is_empty());
    assert!(parsed.abilities.iter().all(|ability| {
        ability.status == "canonical" && ability.recognizer == RecognizerProvenance::Native
    }));
}

#[test]
fn committed_native_failure_keeps_native_provenance() {
    let parsed = parse_oracle_card(request(
        "Lightning Helix",
        "Instant",
        "{R}{W}",
        "Lightning Helix deals damage to any target and you gain 3 life.",
    ));

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].diagnostics[0].code,
        "oracle_machine_failure"
    );
}
