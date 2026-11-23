use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

const WATERY_GRAVE_ORACLE: &str = "({T}: Add {U} or {B}.)\nAs this land enters, you may pay 2 life. If you don't, it enters tapped.";
const STEAM_VENTS_ORACLE: &str = "({T}: Add {U} or {R}.)\nAs this land enters, you may pay 2 life. If you don't, it enters tapped.";

fn parse_land(
    card_name: &str,
    type_line: &str,
    oracle_text: &str,
) -> deepdeck_oracle::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn assert_optional_life_entry(rule: &serde_json::Value, amount: i64) {
    assert_eq!(rule["kind"], "replacementEffect");
    assert_eq!(rule["event"]["kind"], "wouldEnterBattlefield");
    let decision = &rule["decisions"][0];
    assert_eq!(decision["kind"], "chooseWhetherToPay");
    assert_eq!(decision["cost"]["kind"], "payLife");
    assert_eq!(
        decision["cost"]["amount"],
        json!({ "kind": "integer", "value": amount })
    );
    let decision_id = decision["id"].as_str().expect("payment binding id");
    assert_eq!(
        rule["replacement"][0]["condition"],
        json!({
            "kind": "not",
            "operand": { "kind": "costWasPaid", "decisionId": decision_id },
        })
    );
    assert_eq!(
        rule["replacement"][0]["then"][0],
        json!({
            "kind": "setEnteringState",
            "object": { "kind": "self" },
            "tapped": true,
        })
    );
}

#[test]
fn watery_grave_composes_parenthetical_mana_and_optional_entry_automata() {
    let parsed = parse_land("Watery Grave", "Land — Island Swamp", WATERY_GRAVE_ORACLE);

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
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "mana": { "kind": "chooseOne", "options": ["{U}", "{B}"] },
            }],
        }))
    );
    assert_optional_life_entry(
        parsed.abilities[1]
            .rule
            .as_ref()
            .expect("native optional-entry rule"),
        2,
    );
}

#[test]
fn steam_vents_reuses_the_same_machines_with_different_mana_symbols() {
    let parsed = parse_land("Steam Vents", "Land — Island Mountain", STEAM_VENTS_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref().expect("mana rule")["effects"][0]["mana"],
        json!({ "kind": "chooseOne", "options": ["{U}", "{R}"] })
    );
    assert_optional_life_entry(
        parsed.abilities[1]
            .rule
            .as_ref()
            .expect("native optional-entry rule"),
        2,
    );
}

#[test]
fn parenthetical_envelope_reuses_fixed_mana_choice_lists() {
    for (oracle, options) in [
        ("({T}: Add {G}.)", json!("{G}")),
        (
            "({T}: Add {G}, {W}, or {U}.)",
            json!({ "kind": "chooseOne", "options": ["{G}", "{W}", "{U}"] }),
        ),
    ] {
        let parsed = parse_land("Test Typed Land", "Land", oracle);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.status, "canonical", "{oracle}");
        assert_eq!(ability.recognizer, RecognizerProvenance::Native, "{oracle}");
        assert_eq!(
            ability.rule.as_ref().expect("native parenthetical mana")["effects"][0]["mana"],
            options,
            "{oracle}"
        );
    }
}

#[test]
fn committed_parenthetical_family_reports_a_missing_closing_boundary() {
    let parsed = parse_land("Test Typed Land", "Land", "({T}: Add {U} or {B}.");

    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_ne!(ability.status, "canonical");
    let diagnostic = ability.diagnostics.first().expect("native wrapper failure");
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("parenthetical-ability-envelope")
    );
    assert!(
        diagnostic
            .state
            .as_deref()
            .is_some_and(|state| state.contains("close"))
    );
}

#[test]
fn malformed_two_option_comma_list_is_not_accepted() {
    // CR 207.2: a reminder-only line that the committed mana family rejects
    // is dropped as reminder text; it never becomes a canonical mana ability.
    let reminder = parse_land("Test Typed Land", "Land", "({T}: Add {U}, or {B}.)");
    assert!(reminder.abilities.is_empty(), "{:#?}", reminder.abilities);

    // As rules text, the same list fails in the committed mana-list child.
    let parsed = parse_land("Test Typed Land", "Land", "{T}: Add {U}, or {B}.");

    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_ne!(ability.status, "canonical");
    let diagnostic = ability
        .diagnostics
        .first()
        .expect("native mana-list failure");
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("activated-mana-ability")
    );
    assert!(
        diagnostic
            .state
            .as_deref()
            .is_some_and(|state| state.contains("complete"))
    );
}

#[test]
fn committed_optional_entry_family_rejects_a_different_payment_subject() {
    let parsed = parse_land(
        "Test Shock Land",
        "Land",
        "As this land enters, you may pay 2 life. If they don't, it enters tapped.",
    );

    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_ne!(ability.status, "canonical");
    let diagnostic = ability.diagnostics.first().expect("native entry failure");
    assert_eq!(diagnostic.automaton.as_deref(), Some("optional-life-entry"));
    // `If they don't` is read by the shared condition language, which has
    // no `they` performer for the paying player: it fails at the atom.
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("condition.envelope.atom-kind")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("they"));
}
