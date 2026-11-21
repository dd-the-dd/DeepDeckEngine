//! A quoted ability is one complete ability of the object that gains it
//! (CR 113.1). Each case is a full ability parsed through the ordinary pipeline.
use serde_json::{Value, json};

struct Parsed {
    status: String,
    rule: Option<Value>,
    diagnostic: String,
}

fn parse(type_line: &str, text: &str) -> Parsed {
    let result = crate::oracle::parse_oracle_card(crate::oracle::OracleCardParseRequest {
        card_name: "Quoted Grant Test".to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(result.abilities.len(), 1, "one ability in {text}");
    let ability = &result.abilities[0];
    Parsed {
        status: ability.status.clone(),
        rule: ability.rule.clone(),
        diagnostic: ability
            .diagnostics
            .first()
            .map(|diagnostic| diagnostic.message.clone())
            .unwrap_or_default(),
    }
}

fn canonical_effects(type_line: &str, text: &str) -> Vec<Value> {
    let parsed = parse(type_line, text);
    assert_eq!(parsed.status, "canonical", "{text}: {}", parsed.diagnostic);
    parsed.rule.expect("canonical rule")["effects"]
        .as_array()
        .expect("effects")
        .clone()
}

fn kinds(effects: &[Value]) -> Vec<&str> {
    effects
        .iter()
        .map(|effect| effect["kind"].as_str().expect("effect kind"))
        .collect()
}

fn each_creature_you_control() -> Value {
    json!({
        "kind": "eachPermanent",
        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        "where": { "kind": "cardTypeContains", "value": "Creature" },
    })
}

fn attack_life_trigger() -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "declaredAttacker", "object": { "kind": "self" } },
        "effects": [{
            "kind": "gainLife",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "amount": { "kind": "integer", "value": 1 },
        }],
    })
}

#[test]
fn real_card_collection_gets_stats_a_keyword_and_a_quoted_trigger() {
    // Root Manipulation
    let effects = canonical_effects(
        "Instant",
        "Until end of turn, creatures you control get +2/+2 and gain menace and \"Whenever this creature attacks, you gain 1 life.\"",
    );
    assert_eq!(
        kinds(&effects),
        ["modifyPowerToughness", "grantKeyword", "grantAbility"]
    );
    assert_eq!(
        effects[2],
        json!({
            "kind": "grantAbility",
            "object": each_creature_you_control(),
            "duration": { "kind": "untilEndOfCurrentTurn" },
            "ability": attack_life_trigger(),
        })
    );
}

#[test]
fn real_card_created_token_carries_its_quoted_trigger_as_a_complete_rule() {
    // Essenceknit Scholar
    let effects = canonical_effects(
        "Creature \u{2014} Test",
        "When this creature enters, create a 1/1 black and green Pest creature token with \"Whenever this token attacks, you gain 1 life.\"",
    );
    assert_eq!(effects[0]["kind"], "createTokens");
    assert_eq!(
        effects[0]["token"]["abilities"],
        json!([attack_life_trigger()])
    );
}

#[test]
fn quoted_abilities_compose_with_the_target_keyword_list_and_stat_frames() {
    let target = json!({ "kind": "chosenTarget", "id": "targetPermanent" });
    let until = json!({ "kind": "untilEndOfCurrentTurn" });
    let effects = canonical_effects(
        "Instant",
        "Until end of turn, target creature gains deathtouch and \"Whenever this creature attacks, you gain 1 life.\"",
    );
    assert_eq!(kinds(&effects), ["grantKeyword", "grantAbility"]);
    assert_eq!(effects[1]["object"], target);
    assert_eq!(effects[1]["ability"], attack_life_trigger());
    assert_eq!(effects[1]["duration"], until);

    // Quote ends before the trailing duration; the stat frame owns the list.
    let effects = canonical_effects(
        "Instant",
        "Target creature gets +2/+0 and gains \"When this creature dies, draw a card.\" until end of turn.",
    );
    assert_eq!(kinds(&effects), ["modifyPowerToughness", "grantAbility"]);
    assert_eq!(effects[1]["ability"]["event"]["kind"], "permanentDied");
}

#[test]
fn a_keyword_and_two_quoted_abilities_of_different_kinds_share_one_list() {
    // The comma that closes the first quote sits inside the quotation marks.
    let effects = canonical_effects(
        "Instant",
        "Until end of turn, creatures you control gain flying, \"When this creature dies, draw a card,\" and \"{T}: Add {G}.\"",
    );
    assert_eq!(
        kinds(&effects),
        ["grantKeyword", "grantAbility", "grantAbility"]
    );
    assert_eq!(effects[1]["ability"]["kind"], "triggeredAbility");
    assert_eq!(effects[2]["ability"]["kind"], "manaAbility");
    assert_eq!(effects[2]["object"], each_creature_you_control());
}

#[test]
fn a_static_has_predicate_grants_a_quoted_activated_or_mana_ability() {
    let parsed = parse(
        "Artifact \u{2014} Equipment",
        "Equipped creature gets +1/+1 and has flying and \"{1}, {T}: Draw a card.\"",
    );
    assert_eq!(parsed.status, "canonical", "{}", parsed.diagnostic);
    let rule = parsed.rule.expect("rule");
    let kinds = rule["modifiers"]
        .as_array()
        .expect("modifiers")
        .iter()
        .map(|modifier| modifier["kind"].as_str().expect("kind"))
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            "modifyPowerToughness",
            "grantKeyword",
            "grantActivatedAbility"
        ]
    );
    assert_eq!(rule["modifiers"][2]["ability"]["kind"], "activatedAbility");
}

#[test]
fn a_quote_with_a_non_canonical_inner_ability_fails_with_the_inner_diagnostic() {
    // A dealt-damage event without a recipient has no proved aggregation, so
    // the inner trigger is not representable and the grant must not be
    // accepted with the quote dropped.
    let parsed = parse(
        "Instant",
        "Until end of turn, target creature you control gains indestructible and \"Whenever this creature deals damage, put that many +1/+1 counters on it.\"",
    );
    assert_ne!(parsed.status, "canonical");
    assert!(parsed.rule.is_none());
    assert!(
        parsed.diagnostic.contains("quoted-ability.body") && parsed.diagnostic.contains("inner:"),
        "{}",
        parsed.diagnostic
    );
}

#[test]
fn a_granted_received_damage_trigger_keeps_its_event_and_amount_binding() {
    // Strength of Will: the quote is a complete ability of the grantee.
    let effects = canonical_effects(
        "Instant",
        "Until end of turn, target creature you control gains indestructible and \"Whenever this creature is dealt damage, put that many +1/+1 counters on it.\"",
    );
    let granted = effects
        .iter()
        .find(|effect| effect["kind"] == "grantAbility")
        .map(|effect| &effect["ability"])
        .expect("a granted triggered ability");
    assert_eq!(granted["kind"], "triggeredAbility");
    assert_eq!(
        granted["event"],
        json!({ "kind": "permanentDealtDamage", "object": { "kind": "self" } })
    );
    assert_eq!(
        granted["effects"][0]["count"],
        json!({ "kind": "decisionResult", "decisionId": "damageAmount" })
    );
}

#[test]
fn a_grant_the_engine_cannot_scope_to_its_duration_fails_explicitly() {
    let parsed = parse(
        "Instant",
        "Until your next turn, target creature gains \"Whenever this creature attacks, you gain 1 life.\"",
    );
    assert_ne!(parsed.status, "canonical");
    assert!(
        parsed.diagnostic.contains("granted rule"),
        "{}",
        parsed.diagnostic
    );
}

#[test]
fn a_static_grant_of_a_triggered_ability_fails_because_the_engine_does_not_publish_it() {
    // Medic's Kitesail
    let parsed = parse(
        "Artifact \u{2014} Equipment",
        "Equipped creature gets +1/+0 and has flying and \"Whenever this creature attacks, you gain 1 life.\"",
    );
    assert_ne!(parsed.status, "canonical");
    assert!(
        parsed
            .diagnostic
            .contains("static.continuous.quoted-ability"),
        "{}",
        parsed.diagnostic
    );
}

#[test]
fn an_unclosed_quotation_is_a_grammar_failure() {
    let parsed = parse(
        "Instant",
        "Until end of turn, target creature gains \"Whenever this creature attacks, you gain 1 life.",
    );
    assert_ne!(parsed.status, "canonical");
}
