//! Causative `have SUBJECT VERB`: the shared subject-verb readers own the
//! action with a bare verb. Real cards, an unused subject/verb combination,
//! the coordinated bare verb, and finite-verb near misses.
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::{Value, json};

fn parse(name: &str, type_line: &str, text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{B}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn optional_body(name: &str, text: &str) -> Value {
    let parsed = parse(name, "Creature — Horror", text);
    assert_eq!(
        parsed.status, "canonical",
        "{text}: {:#?}",
        parsed.abilities
    );
    let rule = parsed.abilities[0].rule.clone().expect("canonical rule");
    assert_eq!(rule["effects"][0]["kind"], "optionalEffects", "{rule:#}");
    assert_eq!(
        rule["effects"][0]["player"],
        json!({"kind":"controllerOf","object":{"kind":"self"}})
    );
    rule["effects"][0]["effects"].clone()
}

fn kinds(effects: &Value) -> Vec<&str> {
    effects
        .as_array()
        .expect("effects")
        .iter()
        .map(|effect| effect["kind"].as_str().unwrap_or_default())
        .collect()
}

#[test]
fn causative_player_subject_keeps_the_ordinary_sacrifice_action() {
    // Predatory Nightstalker: the targeted opponent chooses and sacrifices.
    let body = optional_body(
        "Predatory Nightstalker",
        "When this creature enters, you may have target opponent sacrifice a creature of their choice.",
    );
    assert_eq!(kinds(&body), ["chooseCards", "sacrificePermanents"]);
    let opponent = json!({"kind":"chosenTarget","id":"targetPlayer"});
    assert_eq!(body[0]["player"], opponent);
    assert_eq!(body[1]["player"], opponent);
}

#[test]
fn causative_source_subject_reads_the_shared_damage_frame() {
    // Hissing Iguanar: `have this creature deal` is the source's damage.
    let body = optional_body(
        "Hissing Iguanar",
        "Whenever another creature dies, you may have this creature deal 1 damage to target player or planeswalker.",
    );
    assert_eq!(kinds(&body), ["dealDamage"]);
    assert_eq!(body[0]["source"], json!({"kind":"self"}));
    assert_eq!(body[0]["amount"], json!({"kind":"integer","value":1}));
}

#[test]
fn causative_permanent_subject_and_its_coordinated_bare_verb() {
    // Hazoret's Favor shape without its delayed sacrifice: the second verb
    // after `and` keeps the causative's bare agreement.
    let body = optional_body(
        "Probe Shaman",
        "When this creature enters, you may have target creature get +2/+0 and gain haste until end of turn.",
    );
    assert_eq!(kinds(&body), ["modifyPowerToughness", "grantKeyword"]);
    assert_eq!(body[1]["keyword"], "haste");
    assert_eq!(body[0]["object"], body[1]["object"]);
}

#[test]
fn unused_combination_composes_without_new_production_code() {
    // No measured card asks a targeted opponent to draw through `have`.
    let body = optional_body(
        "Probe Benefactor",
        "When this creature enters, you may have target opponent draw a card.",
    );
    assert_eq!(kinds(&body), ["drawCards"]);
    assert_eq!(
        body[0]["player"],
        json!({"kind":"chosenTarget","id":"targetPlayer"})
    );
}

#[test]
fn finite_verbs_after_a_causative_stay_explicit_failures() {
    for text in [
        "When this creature enters, you may have target creature gets +2/+0 until end of turn.",
        "When this creature enters, you may have target player loses 3 life.",
        "When this creature enters, you may have this creature deals 1 damage to any target.",
    ] {
        let parsed = parse("Probe Near Miss", "Creature — Horror", text);
        assert_ne!(parsed.status, "canonical", "{text}");
    }
}
