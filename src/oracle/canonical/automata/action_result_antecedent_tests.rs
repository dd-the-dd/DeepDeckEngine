//! Action-result object antecedents (CR 608.2c): the one object an earlier
//! action of the program selected or moved is the antecedent of `that NOUN`,
//! `its PROPERTY`, `that NOUN's PROPERTY` and `the VERBed NOUN's PROPERTY`.
//! Real cards, a generalized sibling, an unused recombination and the
//! departed-operand near miss. Runtime proofs live in
//! `engine/effect_domains/bound_object_lki_tests.rs`.
use serde_json::{Value, json};

use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};

fn parse(type_line: &str, oracle: &str) -> (String, Vec<Value>, Vec<String>, Vec<String>) {
    let request = OracleCardParseRequest {
        card_name: "Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card_native(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    let states = parsed
        .abilities
        .iter()
        .flat_map(|ability| ability.diagnostics.iter())
        .filter_map(|diagnostic| diagnostic.state.clone())
        .collect();
    let messages = parsed
        .abilities
        .iter()
        .flat_map(|ability| ability.diagnostics.iter())
        .map(|diagnostic| diagnostic.message.clone())
        .collect();
    let rules = parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .collect();
    (parsed.status, rules, states, messages)
}

/// Every object a numeric property read of the rule names, in order.
fn property_objects(value: &Value, found: &mut Vec<(String, Value)>) {
    match value {
        Value::Object(map) => {
            if let Some(kind @ ("powerOf" | "toughnessOf" | "manaValueOf")) =
                map.get("kind").and_then(Value::as_str)
            {
                found.push((kind.to_string(), map["object"].clone()));
            }
            map.values()
                .for_each(|child| property_objects(child, found));
        }
        Value::Array(items) => items
            .iter()
            .for_each(|child| property_objects(child, found)),
        _ => {}
    }
}

/// The `bind` every runtime action of the rule records, by kind.
fn binds(value: &Value, found: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            if let (Some(kind), Some(bind)) = (
                map.get("kind").and_then(Value::as_str),
                map.get("bind").and_then(Value::as_str),
            ) {
                found.push((kind.to_string(), bind.to_string()));
            }
            assert!(
                !map.contains_key("pendingActionResultObject"),
                "no planning marker escapes the lowering"
            );
            map.values().for_each(|child| binds(child, found));
        }
        Value::Array(items) => items.iter().for_each(|child| binds(child, found)),
        _ => {}
    }
}

fn canonical(type_line: &str, oracle: &str) -> Value {
    let (status, rules, states, _) = parse(type_line, oracle);
    assert_eq!(status, "canonical", "{oracle}: {states:?}");
    Value::Array(rules)
}

const SORCERY: &str = "Sorcery";
const CREATURE: &str = "Creature \u{2014} Rat";

#[test]
fn a_sacrificed_creature_is_the_antecedent_of_its_genitive_property() {
    // Consuming Vapors / Tribute to Hunger (exact), Devour Flesh (`then`).
    for oracle in [
        "Target player sacrifices a creature of their choice. You gain life equal to that creature's toughness.",
        "Target player sacrifices a creature of their choice, then gains life equal to that creature's toughness.",
    ] {
        let rules = canonical(SORCERY, oracle);
        let mut reads = Vec::new();
        property_objects(&rules, &mut reads);
        let mut recorded = Vec::new();
        binds(&rules, &mut recorded);
        assert_eq!(
            reads,
            vec![(
                "toughnessOf".to_string(),
                json!({"kind": "singleBoundObject", "binding": "actionResult:0"})
            )],
            "{oracle}"
        );
        assert!(
            recorded
                .iter()
                .any(|(kind, bind)| kind == "sacrificePermanents" && bind == "actionResult:0"),
            "{oracle}: {recorded:?}"
        );
    }
}

#[test]
fn a_revealed_top_card_is_the_antecedent_of_its_possessive_property() {
    // Dark Confidant: the card a reveal bound, after it was put into hand.
    let rules = canonical(
        CREATURE,
        "At the beginning of your upkeep, reveal the top card of your library and put that card into your hand. You lose life equal to its mana value.",
    );
    let mut reads = Vec::new();
    property_objects(&rules, &mut reads);
    let [(kind, object)] = reads.as_slice() else {
        panic!("one mana-value read: {reads:?}");
    };
    assert_eq!(kind, "manaValueOf");
    assert_eq!(object["kind"], "singleBoundObject");
    assert_ne!(
        object["binding"], "actionResult:0",
        "the reveal's own bound card set is read"
    );
}

#[test]
fn a_discarded_or_milled_card_is_the_antecedent_of_that_card() {
    // Hellhole Rats (exact discard); a generalized sibling over mill; the
    // unused recombination of a discarded card's mana value feeding a draw.
    for oracle in [
        "When this creature enters, target player discards a card. This creature deals damage to that player equal to that card's mana value.",
        "Target player mills a card. You gain life equal to that card's mana value.",
        "Discard a card. Draw cards equal to that card's mana value.",
    ] {
        let type_line = if oracle.starts_with("When") {
            CREATURE
        } else {
            SORCERY
        };
        let rules = canonical(type_line, oracle);
        let mut reads = Vec::new();
        property_objects(&rules, &mut reads);
        let [(kind, object)] = reads.as_slice() else {
            panic!("{oracle}: one read: {reads:?}");
        };
        assert_eq!(kind, "manaValueOf", "{oracle}");
        assert_eq!(object["kind"], "singleBoundObject", "{oracle}");
        let binding = object["binding"].as_str().expect("a bound result");
        let mut recorded = Vec::new();
        binds(&rules, &mut recorded);
        assert!(
            recorded.iter().any(|(_, bind)| bind == binding),
            "{oracle}: the producer records {binding}: {recorded:?}"
        );
    }
}

#[test]
fn an_action_that_produces_nothing_new_keeps_the_result_nearest() {
    // Twisted Justice: the draw reads the creature sacrificed before it.
    let rules = canonical(
        SORCERY,
        "Target player sacrifices a creature of their choice. You draw cards equal to that creature's power.",
    );
    let mut reads = Vec::new();
    property_objects(&rules, &mut reads);
    assert_eq!(reads.len(), 1, "{reads:?}");
    assert_eq!(reads[0].0, "powerOf");
}

#[test]
fn a_departed_result_is_no_battlefield_operand() {
    // Near miss: the sacrificed creature is a card in a graveyard now (CR
    // 400.7); an action on a permanent cannot name it.
    let (status, _, states, messages) = parse(
        SORCERY,
        "Target player sacrifices a creature of their choice. Tap that creature.",
    );
    assert_ne!(status, "canonical");
    assert!(
        states
            .iter()
            .any(|state| state == "permanent.subject.nominal-anaphor.antecedent"),
        "{states:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("not a permanent any more")),
        "{messages:?}"
    );
}

#[test]
fn a_result_nominal_must_follow_from_the_producing_selection() {
    // Near miss: a discarded card is not named by `that creature`.
    let (status, _, states, _) = parse(
        SORCERY,
        "Discard a card. You gain life equal to that creature's toughness.",
    );
    assert_ne!(status, "canonical", "{states:?}");
}

#[test]
fn a_later_object_producer_ends_the_earlier_result() {
    // Near miss: the token creation is nearer than the sacrifice, so `that
    // card` has no antecedent instead of borrowing the sacrificed one.
    let (status, _, _, _) = parse(
        SORCERY,
        "Sacrifice a creature. Create a 1/1 white Spirit creature token with flying. You gain life equal to that card's mana value.",
    );
    assert_ne!(status, "canonical");
}

#[test]
fn a_single_created_token_is_the_antecedent_of_a_delayed_sacrifice() {
    // Kiki-Jiki, Mirror Breaker (exact) and Tidal Wave (described token).
    for oracle in [
        "{T}: Create a token that's a copy of target nonlegendary creature you control, except it has haste. Sacrifice it at the beginning of the next end step.",
        "Create a 5/5 blue Wall creature token with defender. Sacrifice it at the beginning of the next end step.",
    ] {
        let type_line = if oracle.starts_with('{') {
            "Legendary Creature \u{2014} Goblin Shaman"
        } else {
            "Instant"
        };
        let rules = canonical(type_line, oracle);
        let text = rules.to_string();
        assert!(text.contains("\"sacrificePermanent\""), "{oracle}: {text}");
        assert!(
            text.contains("\"singleBoundObject\""),
            "{oracle}: the one token is read as one object: {text}"
        );
    }
}

#[test]
fn several_created_tokens_are_no_singular_antecedent() {
    // Near miss: `it` never names one of several created tokens.
    let (status, _, _, _) = parse(
        SORCERY,
        "Create two 1/1 white Spirit creature tokens with flying. Sacrifice it at the beginning of the next end step.",
    );
    assert_ne!(status, "canonical");
}

#[test]
fn a_manifested_card_is_a_permanent_result_an_operand_names() {
    // Dissection Tools (exact) and a counters sibling: the face-down 2/2
    // creature manifest dread put onto the battlefield (CR 701.62a).
    for (type_line, oracle) in [
        (
            "Artifact \u{2014} Equipment",
            "When this Equipment enters, manifest dread, then attach this Equipment to that creature.",
        ),
        (
            SORCERY,
            "Manifest dread, then put three +1/+1 counters on that creature.",
        ),
    ] {
        let rules = canonical(type_line, oracle);
        let mut recorded = Vec::new();
        binds(&rules, &mut recorded);
        assert!(
            recorded
                .iter()
                .any(|(kind, bind)| kind == "moveCards" && bind.starts_with("actionResult:")),
            "{oracle}: the face-down move records the manifested card: {recorded:?}"
        );
        assert!(
            rules.to_string().contains("\"singleBoundObject\""),
            "{oracle}: the reader names that one permanent"
        );
    }
}
