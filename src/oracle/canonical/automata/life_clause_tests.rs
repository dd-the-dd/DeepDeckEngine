//! One life clause, every role: real cards read through the effect, trigger,
//! replacement, condition and numeric-fact interpretations of the shared
//! clause; an unused subject/verb combination; and near misses that stay
//! explicit failures inside the frame that claimed them.
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

fn rule(name: &str, type_line: &str, text: &str) -> Value {
    let parsed = parse(name, type_line, text);
    assert_eq!(
        parsed.status, "canonical",
        "{text}: {:#?}",
        parsed.abilities
    );
    parsed.abilities[0].rule.clone().expect("canonical rule")
}

fn controller() -> Value {
    json!({"kind":"controllerOf","object":{"kind":"self"}})
}

fn integer(value: i64) -> Value {
    json!({"kind":"integer","value":value})
}

#[test]
fn effect_subjects_keep_their_established_life_actions() {
    // Sovereign's Bite: a declared target, then an explicit controller.
    let bite = rule(
        "Sovereign's Bite",
        "Sorcery",
        "Target player loses 3 life and you gain 3 life.",
    );
    assert_eq!(
        bite["effects"],
        json!([
            {"kind":"loseLife","player":{"kind":"chosenTarget","id":"targetPlayer"},"amount":integer(3)},
            {"kind":"gainLife","player":controller(),"amount":integer(3)},
        ])
    );
    // Swords to Plowshares: a possessive subject whose object is also the
    // antecedent of the amount.
    let swords = rule(
        "Swords to Plowshares",
        "Instant",
        "Exile target creature. Its controller gains life equal to its power.",
    );
    let gain = swords["effects"]
        .as_array()
        .and_then(|effects| effects.iter().find(|effect| effect["kind"] == "gainLife"))
        .unwrap_or_else(|| panic!("{swords:#}"));
    assert_eq!(gain["player"]["kind"], "boundValue", "{swords:#}");
    assert_eq!(gain["amount"]["kind"], "boundValue", "{swords:#}");
    // The coordinated subjects share one bare verb.
    let coordinated = rule(
        "Coordinated Drain",
        "Sorcery",
        "You and target opponent each lose 2 life.",
    );
    assert_eq!(
        coordinated["effects"],
        json!([
            {"kind":"loseLife","player":controller(),"amount":integer(2)},
            {"kind":"loseLife","player":{"kind":"chosenTarget","id":"targetPlayer"},"amount":integer(2)},
        ])
    );
}

#[test]
fn trigger_replacement_condition_and_fact_interpret_the_same_clause() {
    // Marauding Blight-Priest: the event pattern has no amount.
    let priest = rule(
        "Marauding Blight-Priest",
        "Creature — Vampire Cleric",
        "Whenever you gain life, each opponent loses 1 life.",
    );
    assert_eq!(
        priest["event"],
        json!({"kind":"lifeGained","player":controller()})
    );
    // The same pattern with a third-person actor and the other lemma.
    let opponent = rule(
        "Opponent Loss Probe",
        "Enchantment",
        "Whenever an opponent loses life, you gain 1 life.",
    );
    assert_eq!(
        opponent["event"],
        json!({"kind":"lifeLost","player":{"kind":"opponentsOf","player":controller()}})
    );
    // Boon Reflection: `would` modality, then the replacing life gain.
    let boon = rule(
        "Boon Reflection",
        "Enchantment",
        "If you would gain life, you gain twice that much life instead.",
    );
    assert!(boon.to_string().contains("replaceLifeGain"), "{boon:#}");
    // Past history, simple and perfect, and a threshold.
    for text in [
        "At the beginning of your end step, if you gained life this turn, draw a card.",
        "At the beginning of your end step, if you've gained life this turn, draw a card.",
        "At the beginning of your end step, if you have gained 2 or more life this turn, draw a card.",
        "At the beginning of your end step, if an opponent lost life this turn, draw a card.",
        // Any player: the same life-loss history with no player relation.
        "At the beginning of your end step, if a player lost life this turn, draw a card.",
        "At the beginning of your end step, if an opponent gained life this turn, draw a card.",
    ] {
        assert_eq!(
            parse("History Probe", "Enchantment", text).status,
            "canonical",
            "{text}"
        );
    }
    // The relative clause fills the gapped amount (`the amount of life`).
    let fact = rule(
        "Fact Probe",
        "Enchantment",
        "At the beginning of your end step, each opponent loses X life, where X is the amount of life you gained this turn.",
    );
    assert!(fact.to_string().contains("lifeGainedThisTurn"), "{fact:#}");
}

#[test]
fn causative_player_set_is_an_unused_combination_with_no_new_code() {
    let body = rule(
        "Causative Drain Probe",
        "Creature — Horror",
        "When this creature enters, you may have each opponent lose 1 life.",
    );
    assert_eq!(body["effects"][0]["kind"], "optionalEffects", "{body:#}");
    assert_eq!(
        body["effects"][0]["effects"],
        json!([{"kind":"loseLife","player":{"kind":"opponentsOf","player":controller()},"amount":integer(1)}])
    );
}

#[test]
fn life_clause_near_misses_fail_in_the_claiming_frame() {
    for (type_line, text) in [
        // Agreement with the subject, the causative and the tense.
        ("Sorcery", "Target opponent lose 2 life."),
        ("Sorcery", "Each opponent lose 2 life."),
        (
            "Creature — Horror",
            "When this creature enters, you may have each opponent loses 1 life.",
        ),
        ("Enchantment", "Whenever you gains life, draw a card."),
        (
            "Enchantment",
            "At the beginning of your end step, if you gain life this turn, draw a card.",
        ),
        // Recognized clauses without a recorded replacement.
        (
            "Enchantment",
            "If you would lose life, you lose that much life plus 1 instead.",
        ),
        // An event pattern takes no amount.
        ("Enchantment", "Whenever you gain 2 life, draw a card."),
    ] {
        assert_ne!(
            parse("Near Miss", type_line, text).status,
            "canonical",
            "{text}"
        );
    }
}
