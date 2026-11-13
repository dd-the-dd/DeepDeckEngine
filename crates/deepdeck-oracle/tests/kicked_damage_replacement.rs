use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

fn parse_spell(card_name: &str, oracle_text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{R}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .name("kicked-damage-parser-test".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser test thread starts")
        .join()
        .expect("parser test thread completes")
}

fn damage_rule(parsed: &OracleCardParseResult) -> &Value {
    parsed
        .abilities
        .iter()
        .find(|ability| ability.source.text.contains(" deals "))
        .and_then(|ability| ability.rule.as_ref())
        .expect("the damage ability should produce primitive IR")
}

#[test]
fn burst_lightning_compiles_to_one_target_and_two_primitive_damage_branches() {
    let parsed = parse_spell(
        "Burst Lightning",
        "Kicker {4} (You may pay an additional {4} as you cast this spell.)\nBurst Lightning deals 2 damage to any target. If this spell was kicked, it deals 4 damage instead.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    let unknown = parsed
        .abilities
        .iter()
        .flat_map(|ability| ability.entities.iter())
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        unknown,
        ["Burst", "Lightning"],
        "only the contextual card name may remain lexically UNKNOWN"
    );
    let kicker = parsed
        .abilities
        .iter()
        .find(|ability| ability.source.text.starts_with("Kicker"))
        .and_then(|ability| ability.rule.as_ref())
        .expect("the Kicker ability should compile through its native automaton");
    assert_eq!(
        kicker["ability"],
        json!({
            "kind": "kicker",
            "cost": { "kind": "payMana", "manaCost": "{4}" },
            "repeatable": false,
        })
    );
    let rule = damage_rule(&parsed);
    assert_eq!(
        rule["declaration"]["decisions"],
        json!([{
            "id": "targetDamageable",
            "kind": "chooseTargets",
            "minimum": 1,
            "maximum": 1,
            "candidates": { "kind": "anyTarget" },
        }])
    );
    let conditional = &rule["effects"][0];
    assert_eq!(conditional["kind"], "conditionalEffect");
    assert_eq!(
        conditional["condition"],
        json!({ "kind": "wasKicked", "spell": { "kind": "self" } })
    );
    assert_eq!(conditional["then"][0]["kind"], "dealDamage");
    assert_eq!(
        conditional["then"][0]["amount"],
        json!({ "kind": "integer", "value": 4 })
    );
    assert_eq!(conditional["else"][0]["kind"], "dealDamage");
    assert_eq!(
        conditional["else"][0]["amount"],
        json!({ "kind": "integer", "value": 2 })
    );
    assert_eq!(
        conditional["then"][0]["recipient"],
        json!({ "kind": "chosenTarget", "id": "targetDamageable" })
    );
    assert_eq!(
        conditional["else"][0]["recipient"],
        conditional["then"][0]["recipient"]
    );
    let serialized = rule.to_string();
    assert!(!serialized.contains("burstLightning"));
    assert!(!serialized.contains("conditionalValue"));
}

#[test]
fn multi_symbol_mana_kicker_and_matching_reminder_share_the_cost_automaton() {
    let parsed = parse_spell(
        "Test Spell",
        "Kicker {1}{R} (You may pay an additional {1}{R} as you cast this spell.)",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["ability"]["cost"],
        json!({ "kind": "payMana", "manaCost": "{1}{R}" })
    );
}

#[test]
fn mana_kicker_without_reminder_is_still_a_complete_native_keyword() {
    let parsed = parse_spell("Test Spell", "Kicker {2}{B}");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["ability"]["cost"],
        json!({ "kind": "payMana", "manaCost": "{2}{B}" })
    );
}

#[test]
fn reminder_cost_must_equal_the_printed_kicker_cost() {
    let parsed = parse_spell(
        "Test Spell",
        "Kicker {1}{R} (You may pay an additional {2}{R} as you cast this spell.)",
    );

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(diagnostic.code, "oracle_machine_failure");
    assert_eq!(diagnostic.automaton.as_deref(), Some("keyword-kicker"));
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("keyword.kicker.reminder.cost-match")
    );
    assert!(
        diagnostic
            .expected
            .as_deref()
            .is_some_and(|expected| expected.contains("equal"))
    );
}

#[test]
fn malformed_kicker_reminder_fails_after_the_native_claim() {
    let parsed = parse_spell(
        "Test Spell",
        "Kicker {3} (You can pay an additional {3} as you cast this spell.)",
    );

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(diagnostic.code, "oracle_machine_failure");
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("keyword.kicker.reminder.permission")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("can"));
}

#[test]
fn sibling_amounts_and_creature_target_use_the_same_generic_slots() {
    let parsed = parse_spell(
        "Test Spark",
        "Test Spark deals 3 damage to target artifact creature. If this spell was kicked, it deals 7 damage instead.",
    );

    assert_eq!(parsed.status, "canonical");
    let rule = damage_rule(&parsed);
    assert_eq!(
        rule["declaration"]["decisions"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"],
        json!({
            "kind": "and",
            "operands": [
                { "kind": "cardTypeContains", "value": "Artifact" },
                { "kind": "cardTypeContains", "value": "Creature" },
            ],
        })
    );
    assert_eq!(rule["effects"][0]["then"][0]["amount"]["value"], 7);
    assert_eq!(rule["effects"][0]["else"][0]["amount"]["value"], 3);
}

#[test]
fn explicit_creature_target_anaphora_reuses_the_original_decision() {
    let parsed = parse_spell(
        "Test Strike",
        "Test Strike deals 2 damage to target creature. If this spell was kicked, it deals 4 damage to that creature instead.",
    );

    assert_eq!(parsed.status, "canonical");
    let rule = damage_rule(&parsed);
    assert_eq!(
        rule["declaration"]["decisions"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        rule["effects"][0]["then"][0]["recipient"],
        rule["effects"][0]["else"][0]["recipient"]
    );
}

#[test]
fn player_target_and_matching_anaphora_use_the_same_shared_target_slot() {
    let parsed = parse_spell(
        "Test Bolt",
        "Test Bolt deals 1 damage to target player. If this spell was kicked, it deals 5 damage to that player instead.",
    );

    assert_eq!(parsed.status, "canonical");
    let rule = damage_rule(&parsed);
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"],
        json!({ "kind": "players" })
    );
    assert_eq!(
        rule["effects"][0]["then"][0]["recipient"],
        rule["effects"][0]["else"][0]["recipient"]
    );
}

#[test]
fn unsupported_kicked_condition_is_reported_instead_of_approximated() {
    let parsed = parse_spell(
        "Test Spark",
        "Test Spark deals 2 damage to any target. If this spell was cast, it deals 4 damage instead.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(diagnostic.code, "oracle_machine_failure");
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("conditional-effect-sequence")
    );
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("condition.kicked.predicate")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("cast"));
    assert!(
        diagnostic
            .expected
            .as_deref()
            .is_some_and(|expected| expected.contains("kicked"))
    );
}

#[test]
fn mismatched_source_anaphora_is_not_accepted() {
    let parsed = parse_spell(
        "Test Spark",
        "Test Spark deals 2 damage to any target. If this spell was kicked, you deal 4 damage instead.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
}

#[test]
fn a_different_named_source_is_not_treated_as_the_spell_itself() {
    let parsed = parse_spell(
        "Test Spark",
        "Other Spark deals 2 damage to any target. If this spell was kicked, it deals 4 damage instead.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
}

#[test]
fn incompatible_target_anaphora_fails_at_the_target_reference_state() {
    let parsed = parse_spell(
        "Test Strike",
        "Test Strike deals 2 damage to target creature. If this spell was kicked, it deals 4 damage to that player instead.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(diagnostic.code, "oracle_machine_failure");
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("reference.target-anaphor.noun")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("player"));
}

#[test]
fn a_non_damageable_object_selector_is_rejected_before_lowering() {
    let parsed = parse_spell(
        "Test Strike",
        "Test Strike deals 2 damage to target artifact. If this spell was kicked, it deals 4 damage instead.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(diagnostic.code, "oracle_machine_failure");
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("effect.damage.selector.validation")
    );
    assert!(
        diagnostic
            .expected
            .as_deref()
            .is_some_and(|expected| expected.contains("damageable permanent"))
    );
}
