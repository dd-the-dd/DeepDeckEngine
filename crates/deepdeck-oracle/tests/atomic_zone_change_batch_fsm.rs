use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

const OUTLAW_REMINDER: &str = "(Assassins, Mercenaries, Pirates, Rogues, and Warlocks are outlaws. Everyone else is fair game.)";

fn parse_instant(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Selector Test".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{B}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn only_rule(parsed: &OracleCardParseResult) -> &Value {
    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    parsed.abilities[0]
        .rule
        .as_ref()
        .expect("native atomic zone change emits a rule")
}

fn outlaw_union() -> Value {
    json!({
        "kind": "or",
        "operands": [
            { "kind": "subtypeContains", "value": "Assassin" },
            { "kind": "subtypeContains", "value": "Mercenary" },
            { "kind": "subtypeContains", "value": "Pirate" },
            { "kind": "subtypeContains", "value": "Rogue" },
            { "kind": "subtypeContains", "value": "Warlock" },
        ],
    })
}

#[test]
fn printed_batch_reminder_is_consumed_and_batch_lowers_to_subtype_primitives() {
    let parsed = parse_instant(&format!(
        "Destroy target non-outlaw creature. {OUTLAW_REMINDER}"
    ));
    let rule = only_rule(&parsed);
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"],
        json!({
            "kind": "and",
            "operands": [
                { "kind": "not", "operand": outlaw_union() },
                { "kind": "cardTypeContains", "value": "Creature" },
            ],
        })
    );
    assert_eq!(rule["effects"][0]["kind"], "destroyPermanent");
    assert!(!rule.to_string().contains("resolveSpellInstruction"));
    assert!(!rule.to_string().contains("subtypeContains\":\"outlaw"));
}

#[test]
fn subtype_and_positive_batch_qualifiers_reuse_the_same_selector_machine() {
    let non_human = parse_instant("Destroy target non-Human creature.");
    assert_eq!(
        only_rule(&non_human)["declaration"]["decisions"][0]["candidates"]["where"],
        json!({
            "kind": "and",
            "operands": [
                {
                    "kind": "not",
                    "operand": { "kind": "subtypeContains", "value": "Human" },
                },
                { "kind": "cardTypeContains", "value": "Creature" },
            ],
        })
    );

    let outlaw = parse_instant("Exile target outlaw creature.");
    assert_eq!(
        only_rule(&outlaw)["declaration"]["decisions"][0]["candidates"]["where"],
        json!({
            "kind": "and",
            "operands": [
                outlaw_union(),
                { "kind": "cardTypeContains", "value": "Creature" },
            ],
        })
    );
    assert_eq!(only_rule(&outlaw)["effects"][0]["kind"], "exilePermanent");
}

#[test]
fn unknown_lowercase_batch_is_a_committed_local_failure() {
    // `Hero` is now a cataloged creature subtype; use an unknown word.
    let parsed = parse_instant("Destroy target non-widget creature.");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("atomic-targeted-zone-change")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("selector.predicate.start")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("non-widget"));
}

#[test]
fn batch_reminder_members_and_trailing_text_are_fully_validated() {
    // CR 207.2: a balanced reminder has no rules meaning. The shared sequence
    // skips it, so it can never redefine the registry's batch members.
    let incomplete = parse_instant(
        "Destroy target non-outlaw creature. (Assassins, Mercenaries, Pirates, and Rogues are outlaws.)",
    );
    assert_eq!(
        only_rule(&incomplete)["declaration"]["decisions"][0]["candidates"]["where"],
        json!({
            "kind": "and",
            "operands": [
                { "kind": "not", "operand": outlaw_union() },
                { "kind": "cardTypeContains", "value": "Creature" },
            ],
        })
    );

    // An unclosed batch reminder is not skippable reminder text; the atomic
    // family validates its members against the registry and rejects it.
    let unclosed = parse_instant(
        "Destroy target non-outlaw creature. (Assassins, Mercenaries, Pirates, and Rogues are outlaws.",
    );
    assert_eq!(
        unclosed.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert_eq!(unclosed.abilities[0].status, "unsupported");
    assert_eq!(
        unclosed.abilities[0].diagnostics[0].state.as_deref(),
        Some("reminder.characteristic-batch.members-validation")
    );

    // A supported sentence after the reminder is the next effect of the
    // same spell, in printed order.
    let trailing = parse_instant(&format!(
        "Destroy target non-outlaw creature. {OUTLAW_REMINDER} Draw a card."
    ));
    let rule = only_rule(&trailing);
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"]["operands"][0],
        json!({ "kind": "not", "operand": outlaw_union() })
    );
    assert_eq!(
        rule["effects"],
        json!([
            {
                "kind": "destroyPermanent",
                "permanent": { "kind": "chosenTarget", "id": "targetPermanent" },
            },
            {
                "kind": "drawCards",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "count": { "kind": "integer", "value": 1 },
            },
        ])
    );

    // Unsupported trailing text still fails full consumption.
    let malformed_trailing = parse_instant(&format!(
        "Destroy target non-outlaw creature. {OUTLAW_REMINDER} Draw."
    ));
    assert_eq!(
        malformed_trailing.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert_eq!(malformed_trailing.abilities[0].status, "unsupported");
    assert_eq!(
        malformed_trailing.abilities[0].diagnostics[0]
            .state
            .as_deref(),
        Some("effect.zone-change.full-consumption")
    );
    assert_eq!(
        malformed_trailing.abilities[0].diagnostics[0]
            .found
            .as_deref(),
        Some("Draw")
    );
}
