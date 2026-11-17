//! Postposed `, where X is EXPR` (CR 107.3): the sentence parent binds X for
//! its actions with the shared numeric reader; actions read one `bind`.
use serde_json::{Value, json};

use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(type_line: &str, mana_cost: &str, text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Card".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn canonical_rule(parsed: &OracleCardParseResult) -> &Value {
    let ability = &parsed.abilities[0];
    assert_eq!(
        ability.status,
        "canonical",
        "{:?}",
        ability.diagnostics.first().map(|d| (&d.state, &d.expected))
    );
    ability.rule.as_ref().expect("canonical rule")
}

fn failure_state(parsed: &OracleCardParseResult) -> Option<&str> {
    let ability = &parsed.abilities[0];
    (ability.status != "canonical").then_some(())?;
    ability.diagnostics.first()?.state.as_deref()
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn bound() -> Value {
    json!({ "kind": "boundValue", "id": "whereX" })
}

fn kinds(rule: &Value) -> Vec<&str> {
    rule["effects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|effect| effect["kind"].as_str().unwrap())
        .collect()
}

#[test]
fn exact_cards_bind_x_with_the_shared_count_children() {
    // Number of colors among permanents (token quantity).
    let kithkeeper = parse(
        "Creature \u{2014} Elf",
        "{3}{G}",
        "Vivid \u{2014} When this creature enters, create X 1/1 green and white Kithkin creature tokens, where X is the number of colors among permanents you control.",
    );
    let rule = canonical_rule(&kithkeeper);
    assert_eq!(kinds(rule), ["bind", "createTokens"]);
    assert_eq!(
        rule["effects"][0]["value"],
        json!({ "kind": "countDistinctColors", "player": controller(), "where": null })
    );
    assert_eq!(rule["effects"][1]["quantity"], bound());

    // One plus the amount of life gained this turn (counters on a target).
    let sickbay = parse(
        "Enchantment",
        "{2}{W}",
        "At the beginning of combat on your turn, put X +1/+1 counters on target creature you control, where X is one plus the amount of life you gained this turn.",
    );
    let rule = canonical_rule(&sickbay);
    assert_eq!(
        rule["effects"][0]["value"],
        json!({
            "kind": "add",
            "left": { "kind": "integer", "value": 1 },
            "right": { "kind": "lifeGainedThisTurn", "player": controller() },
        })
    );
    assert_eq!(rule["effects"][1]["count"], bound());

    // Differently named lands (token quantity).
    let gamemaster = parse(
        "Creature \u{2014} Human",
        "{3}{R}",
        "Strange new worlds \u{2014} When this creature enters, create X 1/1 red Officer creature tokens, where X is the number of differently named lands you control.",
    );
    assert_eq!(
        canonical_rule(&gamemaster)["effects"][0]["value"]["kind"],
        "countDistinctPermanentNames"
    );

    // Keyword action with a variable count.
    let enforcer = parse(
        "Creature \u{2014} Construct",
        "{3}",
        "Whenever this creature attacks, empower Jace X, where X is the number of creatures you control.",
    );
    assert_eq!(canonical_rule(&enforcer)["effects"][0]["id"], "whereX");
}

#[test]
fn one_definition_binds_every_action_of_the_sentence_in_any_combination() {
    // Draw and gain life share X; the expression composes `twice` with a count.
    let parsed = parse(
        "Sorcery",
        "{2}{U}",
        "Draw X cards and gain X life, where X is twice the number of lands you control.",
    );
    let rule = canonical_rule(&parsed);
    assert_eq!(kinds(rule), ["bind", "drawCards", "gainLife"]);
    assert_eq!(rule["effects"][1]["count"], bound());
    assert_eq!(rule["effects"][2]["amount"], bound());
    assert_eq!(rule["effects"][0]["value"]["kind"], "multiply");

    // A different action family and a player fact in an activated ability.
    let activated = parse(
        "Artifact",
        "{2}",
        "{3}, {T}: Target creature gets +X/+X until end of turn, where X is your life total.",
    );
    let rule = canonical_rule(&activated);
    assert_eq!(
        rule["effects"][0]["value"],
        json!({ "kind": "lifeTotal", "player": controller() })
    );
    assert_eq!(rule["effects"][1]["power"], bound());
    assert_eq!(rule["effects"][1]["toughness"], bound());
}

#[test]
fn the_binding_is_scoped_to_its_own_sentence() {
    let parsed = parse(
        "Sorcery",
        "{2}{U}",
        "You gain 2 life. Draw X cards, where X is the number of lands you control.",
    );
    let rule = canonical_rule(&parsed);
    // The bind sits before the sentence that defines X, not before the first.
    assert_eq!(kinds(rule), ["gainLife", "bind", "drawCards"]);
}

#[test]
fn x_of_another_sentence_is_never_borrowed() {
    // The second sentence's X has no producer: it must not take the where-X.
    let parsed = parse(
        "Sorcery",
        "{2}{U}",
        "Draw X cards, where X is the number of lands you control. You gain X life.",
    );
    assert_eq!(failure_state(&parsed), Some("binding.x-value.source"));
}

#[test]
fn a_second_producer_of_x_in_the_face_is_an_explicit_conflict() {
    // Mana-cost X in the same face: two producers are never merged.
    let parsed = parse(
        "Sorcery",
        "{X}{U}",
        "Draw X cards, where X is the number of lands you control.",
    );
    assert_eq!(
        failure_state(&parsed),
        Some("binding.x-value.definition-conflict")
    );
    // Without the definition the mana-cost X still binds as before.
    let plain = parse("Sorcery", "{X}{U}", "Draw X cards.");
    assert_eq!(plain.status, "canonical");
}

#[test]
fn near_misses_fail_at_precise_states() {
    // No action in the sentence uses X.
    let unused = parse(
        "Sorcery",
        "{2}{U}",
        "Draw a card, where X is the number of lands you control.",
    );
    assert_eq!(failure_state(&unused), Some("lower.where-x.scope"));
    // A definition that mentions X is circular.
    let circular = parse("Sorcery", "{2}{U}", "Draw X cards, where X is X.");
    assert_ne!(circular.status, "canonical");
    // An expression the shared reader cannot represent is never replaced by zero.
    let opponents = parse(
        "Sorcery",
        "{2}{U}",
        "Draw X cards, where X is the number of lands your opponents control.",
    );
    assert_eq!(
        failure_state(&opponents),
        Some("numeric.for-each.controller")
    );
    let unsupported = parse(
        "Sorcery",
        "{2}{U}",
        "Draw X cards, where X is the number of mysteries.",
    );
    assert_ne!(unsupported.status, "canonical");
    // X cannot be bound inside a target declaration.
    let declared = parse(
        "Sorcery",
        "{2}{U}",
        "Up to X target creatures get +1/+1 until end of turn, where X is the number of lands you control.",
    );
    assert_ne!(declared.status, "canonical");
}
