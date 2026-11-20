use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

fn parse_land(oracle_text: &str) -> deepdeck_oracle::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Utility Land".to_string(),
        type_line: "Land".to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn tap_and_sacrifice_self_destroy_nonbasic_land_is_native_and_primitive() {
    let parsed =
        parse_land("{T}: Add {C}.\n{T}, Sacrifice this land: Destroy target nonbasic land.");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    let rule = parsed.abilities[1]
        .rule
        .as_ref()
        .expect("compound activation emits a rule");
    assert_eq!(
        rule["costs"],
        json!([
            { "kind": "tap", "object": { "kind": "self" } },
            { "kind": "sacrificePermanent", "permanent": { "kind": "self" } },
        ])
    );
    assert_eq!(rule["effects"][0]["kind"], "destroyPermanent");
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"],
        json!({
            "kind": "and",
            "operands": [
                {
                    "kind": "not",
                    "operand": { "kind": "typeLineContains", "value": "Basic" },
                },
                { "kind": "cardTypeContains", "value": "Land" },
            ],
        })
    );
    assert!(!rule.to_string().contains("resolveTriggeredInstruction"));
}

#[test]
fn source_permanent_type_is_a_reusable_sacrifice_cost_child() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Utility Artifact".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(
            "{T}, Sacrifice this artifact: Destroy target nonbasic land.".to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("sibling parses");
    assert_eq!(rule["costs"][1]["kind"], "sacrificePermanent");
    assert_eq!(rule["effects"][0]["kind"], "destroyPermanent");
}

#[test]
fn promoted_family_reports_precise_near_miss_states() {
    for (oracle_text, state, found) in [
        (
            "{T}, Sacrifice this land: Destroy target nonbasic.",
            "effect.zone-change.target.validation",
            ".",
        ),
        (
            "{T}, Sacrifice this spell: Destroy target nonbasic land.",
            "cost.payment.source.noun",
            "spell",
        ),
    ] {
        let parsed = parse_land(oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(
            ability.recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        assert_ne!(ability.status, "canonical");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some("contextual-effect-program")
        );
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), Some(found));
    }
}

#[test]
fn later_effects_compose_or_fail_without_a_partial_zone_change() {
    for text in [
        "{T}: Exile target creature. Draw a card.",
        "{T}: Exile target creature and draw a card.",
    ] {
        let parsed = parse_land(text);
        let rule = parsed.abilities[0].rule.as_ref().expect(text);
        let kinds = rule["effects"]
            .as_array()
            .expect(text)
            .iter()
            .map(|effect| effect["kind"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(kinds, ["exilePermanent", "drawCards"], "{text}");
    }
    // Manifest dread is now an ordinary keyword-action verb (CR 701.62), so the
    // later effect composes after the exile instead of failing.
    let parsed = parse_land("{T}: Exile target creature and manifest dread.");
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("manifest dread composes");
    assert_eq!(rule["effects"][0]["kind"], "exilePermanent");
    assert!(
        rule["effects"]
            .as_array()
            .is_some_and(|effects| effects.len() >= 2)
    );
    // An unknown later verb still fails without a partial zone change.
    let parsed = parse_land("{T}: Exile target creature and teleport a card.");
    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
}

#[test]
fn activation_without_its_cost_separator_is_never_claimed() {
    // No activated parent can classify a cost list without `:`; the ability
    // stays explicitly unsupported instead of being reinterpreted.
    let parsed = parse_land("{T}, Sacrifice this land Destroy target nonbasic land.");
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Unclaimed);
    assert_ne!(ability.status, "canonical");
    assert!(ability.rule.is_none());
    assert_eq!(ability.diagnostics[0].code, "unsupported_oracle_ability");
}

#[test]
fn promoted_vocabulary_has_no_unknown_tokens() {
    let parsed = parse_land("{T}, Sacrifice this land: Destroy target nonbasic land.");
    let entities = &parsed.abilities[0].entities;

    let unknown = entities
        .iter()
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();
    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
    assert_eq!(
        entities
            .iter()
            .find(|entity| entity.raw.eq_ignore_ascii_case("sacrifice"))
            .map(|entity| entity.token_type.as_str()),
        Some("actionWord")
    );
    assert_eq!(
        entities
            .iter()
            .find(|entity| entity.raw.eq_ignore_ascii_case("destroy"))
            .map(|entity| entity.token_type.as_str()),
        Some("actionWord")
    );
}
