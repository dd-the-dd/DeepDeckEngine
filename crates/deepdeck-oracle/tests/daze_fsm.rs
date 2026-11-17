use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

const DAZE_ORACLE: &str = "You may return an Island you control to its owner's hand rather than pay this spell's mana cost.\nCounter target spell unless its controller pays {1}.";

fn parse_instant(card_name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn alternative_return_rule(where_filter: Value) -> Value {
    json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": {
            "kind": "alternativeCost",
            "costs": [{
                "kind": "returnPermanentToOwnersHand",
                "where": where_filter,
            }],
        },
    })
}

fn counter_unless_rule(where_filter: Option<Value>, mana_cost: &str) -> Value {
    let mut candidates = json!({ "kind": "spells" });
    if let Some(filter) = where_filter {
        candidates["where"] = filter;
    }
    json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "targetSpell",
                "kind": "chooseTargets",
                "minimum": 1,
                "maximum": 1,
                "candidates": candidates,
            }],
        },
        "effects": [
            {
                "kind": "tryPayCost",
                "id": "resolutionPayment0",
                "player": { "kind": "controllerOf", "object": { "kind": "chosenTarget", "id": "targetSpell" } },
                "cost": { "kind": "payMana", "manaCost": mana_cost },
            },
            {
                "kind": "conditional",
                "condition": { "kind": "costWasPaid", "decisionId": "resolutionPayment0", "value": false, "required": true },
                "then": [{ "kind": "counterStackObject", "object": { "kind": "chosenTarget", "id": "targetSpell" } }],
                "else": [],
            },
        ],
    })
}

#[test]
fn daze_is_two_native_primitive_abilities() {
    let parsed = parse_instant("Daze", DAZE_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.status == "canonical"
            && ability.recognizer == RecognizerProvenance::Native
            && ability
                .entities
                .iter()
                .all(|entity| entity.token_type != "unknown")
    }));
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&alternative_return_rule(json!({
            "kind": "subtypeContains",
            "value": "Island",
        })))
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&counter_unless_rule(None, "{1}"))
    );
}

#[test]
fn alternative_return_and_counter_payment_generalize_independently() {
    let alternative = parse_instant(
        "Alternative Return Test",
        "You may return an artifact you control to its owner's hand rather than pay this spell's mana cost.",
    );
    assert_eq!(alternative.status, "canonical");
    assert_eq!(
        alternative.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert_eq!(
        alternative.abilities[0].rule.as_ref(),
        Some(&alternative_return_rule(json!({
            "kind": "cardTypeContains",
            "value": "Artifact",
        })))
    );

    let counter = parse_instant(
        "Conditional Counter Test",
        "Counter target noncreature spell unless its controller pays {2}{U}.",
    );
    assert_eq!(counter.status, "canonical");
    assert_eq!(
        counter.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert_eq!(
        counter.abilities[0].rule.as_ref(),
        Some(&counter_unless_rule(
            Some(json!({
                "kind": "not",
                "operand": { "kind": "cardTypeContains", "value": "Creature" },
            })),
            "{2}{U}",
        ))
    );
}

#[test]
fn malformed_promoted_family_members_fail_at_the_responsible_child() {
    let alternative = parse_instant(
        "Broken Alternative Return",
        "You may return an Island you control to its hand rather than pay this spell's mana cost.",
    );
    let alternative_ability = &alternative.abilities[0];
    assert_eq!(alternative.status, "unsupported");
    assert_eq!(alternative_ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        alternative_ability.diagnostics[0].automaton.as_deref(),
        Some("alternative-return-cost")
    );
    assert_eq!(
        alternative_ability.diagnostics[0].state.as_deref(),
        Some("cost.return-controlled.destination-owner")
    );
    assert_eq!(
        alternative_ability.diagnostics[0].found.as_deref(),
        Some("hand")
    );

    let counter = parse_instant(
        "Broken Conditional Counter",
        "Counter target spell unless its controller pays.",
    );
    let counter_ability = &counter.abilities[0];
    assert_eq!(counter.status, "unsupported");
    assert_eq!(counter_ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        counter_ability.diagnostics[0].automaton.as_deref(),
        Some("counter-unless-paid")
    );
    assert_eq!(
        counter_ability.diagnostics[0].state.as_deref(),
        Some("cost.mana.start")
    );
    assert_eq!(counter_ability.diagnostics[0].found.as_deref(), Some("."));
}
