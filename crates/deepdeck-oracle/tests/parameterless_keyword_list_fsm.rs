use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Generic Keyword Creature".to_string(),
        type_line: "Creature — Shapeshifter".to_string(),
        mana_cost: Some("{3}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn ability_kinds(rule: &Value) -> Vec<&str> {
    match rule["kind"].as_str() {
        Some("keywordAbility") => vec![
            rule["ability"]["kind"]
                .as_str()
                .expect("singular keyword kind"),
        ],
        Some("keywordAbilityGroup") => rule["abilities"]
            .as_array()
            .expect("keyword group")
            .iter()
            .map(|ability| ability["kind"].as_str().expect("group keyword kind"))
            .collect(),
        actual => panic!("expected keyword primitive, got {actual:?}"),
    }
}

#[test]
fn reality_fracture_parameterless_lists_share_one_native_automaton() {
    for (oracle_text, expected) in [
        ("Flying, trample", vec!["flying", "trample"]),
        ("Flying, prowess", vec!["flying", "prowess"]),
        ("Flying, haste", vec!["flying", "haste"]),
        ("Vigilance, trample", vec!["vigilance", "trample"]),
    ] {
        let parsed = parse(oracle_text);
        let ability = &parsed.abilities[0];

        assert_eq!(parsed.status, "canonical", "{oracle_text}");
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(
            ability_kinds(ability.rule.as_ref().expect("native primitive rule")),
            expected,
            "{oracle_text}"
        );
    }
}

#[test]
fn parameterized_ward_composes_with_parameterless_atoms_without_becoming_one() {
    let parsed = parse("Flying, vigilance, lifelink, ward {2}");
    let ability = &parsed.abilities[0];
    let rule = ability.rule.as_ref().expect("native primitive rule");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        rule,
        &json!({
            "kind": "keywordAbilityGroup",
            "source": { "kind": "self" },
            "abilities": [
                { "kind": "flying" },
                { "kind": "vigilance" },
                { "kind": "lifelink" },
                {
                    "kind": "ward",
                    "cost": { "kind": "payMana", "manaCost": "{2}" },
                },
            ],
        })
    );
}

#[test]
fn sibling_list_supports_multiword_atoms_from_the_same_registry() {
    let parsed = parse("Deathtouch, first strike, menace");
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("sibling emits primitive IR");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(
        ability_kinds(rule),
        vec!["deathtouch", "firstStrike", "menace"]
    );
}

#[test]
fn claimed_bare_ward_is_a_committed_local_failure() {
    let parsed = parse("Flying, ward");
    let ability = &parsed.abilities[0];

    assert_eq!(ability.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
    let diagnostic = &ability.diagnostics[0];
    assert_eq!(diagnostic.automaton.as_deref(), Some("simple-keyword-list"));
    assert_eq!(diagnostic.state.as_deref(), Some("keyword.ward.cost"));
    assert_eq!(diagnostic.expected.as_deref(), Some("a mana cost for Ward"));
}

#[test]
fn keyword_adjective_at_the_start_of_an_effect_is_not_stolen() {
    let parsed = parse("Flying creatures you control have vigilance.");
    let ability = &parsed.abilities[0];

    assert_ne!(ability.recognizer, RecognizerProvenance::Native);
    assert!(
        ability
            .diagnostics
            .iter()
            .all(|diagnostic| { diagnostic.automaton.as_deref() != Some("simple-keyword-list") })
    );
}

#[test]
fn promoted_keyword_lists_have_no_unknown_tokens() {
    for oracle_text in [
        "Flying, trample",
        "Flying, prowess",
        "Flying, haste",
        "Flying, vigilance, lifelink, ward {2}",
        "Vigilance, trample",
        "Deathtouch, first strike, menace",
    ] {
        let parsed = parse(oracle_text);
        let unknown = parsed.abilities[0]
            .entities
            .iter()
            .filter(|entity| entity.token_type == "unknown")
            .map(|entity| entity.raw.as_str())
            .collect::<Vec<_>>();

        assert!(
            unknown.is_empty(),
            "{oracle_text}: unknown tokens {unknown:?}"
        );
    }
}
