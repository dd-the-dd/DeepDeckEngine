//! `where X is EXPR` read by a target declaration (CR 601.2c, 603.3d), by a
//! later sentence or condition of the same program (CR 107.3), and by a
//! countered spell's mana value read before it leaves the stack.
use serde_json::Value;

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

fn rule(parsed: &OracleCardParseResult, index: usize) -> &Value {
    let ability = &parsed.abilities[index];
    assert_eq!(
        ability.status,
        "canonical",
        "{:?}",
        ability.diagnostics.first().map(|d| (&d.state, &d.expected))
    );
    ability.rule.as_ref().expect("canonical rule")
}

fn failure_state(parsed: &OracleCardParseResult, index: usize) -> Option<&str> {
    let ability = &parsed.abilities[index];
    (ability.status != "canonical").then_some(())?;
    ability.diagnostics.first()?.state.as_deref()
}

fn target_decisions(rule: &Value) -> Vec<&Value> {
    rule["declaration"]["decisions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|decision| decision["kind"] == "chooseTargets")
        .collect()
}

fn mentions_x(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            (object.get("kind").and_then(Value::as_str) == Some("decisionResult")
                && object.get("decisionId").and_then(Value::as_str) == Some("xValue"))
                || object.values().any(mentions_x)
        }
        Value::Array(items) => items.iter().any(mentions_x),
        _ => false,
    }
}

fn contains_kind(value: &Value, kind: &str) -> bool {
    match value {
        Value::Object(object) => {
            object.get("kind").and_then(Value::as_str) == Some(kind)
                || object.values().any(|child| contains_kind(child, kind))
        }
        Value::Array(items) => items.iter().any(|child| contains_kind(child, kind)),
        _ => false,
    }
}

fn effect_kinds(rule: &Value) -> Vec<&str> {
    rule["effects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|effect| effect["kind"].as_str())
        .collect()
}

#[test]
fn spellstutter_sprite_reads_the_faerie_count_in_its_candidate_threshold() {
    let parsed = parse(
        "Creature \u{2014} Faerie Wizard",
        "{1}{U}",
        "Flash\nFlying\nWhen this creature enters, counter target spell with mana value X or less, where X is the number of Faeries you control.",
    );
    let rule = rule(&parsed, 2);
    let [decision] = target_decisions(rule)[..] else {
        panic!("one declared spell target: {rule}");
    };
    assert!(!mentions_x(decision), "{decision}");
    assert!(contains_kind(&decision["candidates"], "countPermanents"));
    // No action reads X: nothing is bound while the ability resolves.
    assert!(!effect_kinds(rule).contains(&"bind"));
}

#[test]
fn martok_reads_the_attacked_opponents_as_the_declared_count() {
    let parsed = parse(
        "Legendary Creature \u{2014} Klingon Warrior",
        "{2}{R}{W}",
        "Battlefront \u{2014} Whenever you attack, put a +1/+1 counter on each of up to X target creatures you control, where X is the number of opponents you attacked this turn.",
    );
    let rule = rule(&parsed, 0);
    let [decision] = target_decisions(rule)[..] else {
        panic!("one declared slot: {rule}");
    };
    assert_eq!(decision["minimum"], 0);
    assert_eq!(
        decision["maximum"]["kind"],
        "countOpponentsAttackedThisTurn"
    );
}

#[test]
fn an_unused_combination_of_count_and_threshold_reads_the_same_amount() {
    // A new combination: `up to X` lands with a power threshold, both X.
    let parsed = parse(
        "Sorcery",
        "{3}{G}",
        "Return up to X target creatures with power X or less to their owners' hands, where X is the number of lands you control.",
    );
    let rule = rule(&parsed, 0);
    let [decision] = target_decisions(rule)[..] else {
        panic!("one declared slot: {rule}");
    };
    assert!(!mentions_x(decision), "{decision}");
    assert_eq!(decision["maximum"]["kind"], "countPermanents");
    assert!(contains_kind(
        &decision["candidates"]["where"],
        "countPermanents"
    ));
}

#[test]
fn a_declared_amount_without_a_value_at_declaration_fails() {
    // The damage the trigger's event records has no value when the target
    // is chosen from the program's point of view: never zero, never empty.
    let parsed = parse(
        "Creature \u{2014} Spirit Cleric",
        "{1}{R}{W}",
        "Whenever this creature deals combat damage to a player, you may return target creature card with mana value X or less from your graveyard to the battlefield, where X is the amount of damage this creature dealt to that player.",
    );
    assert_eq!(
        failure_state(&parsed, 0),
        Some("lower.where-x.declarations")
    );
}

#[test]
fn a_declared_x_count_without_a_producer_is_never_zero() {
    // `each of up to X target creatures` on a face with no {X} and no
    // definition: the missing producer is reported, X is never 0.
    let parsed = parse(
        "Creature \u{2014} Klingon Warrior",
        "{2}{R}{W}",
        "Whenever you attack, put a +1/+1 counter on each of up to X target creatures you control.",
    );
    assert_eq!(failure_state(&parsed, 0), Some("binding.x-value.source"));
}

#[test]
fn spell_swindle_reads_the_countered_spell_before_the_counter() {
    let parsed = parse(
        "Instant",
        "{3}{U}{U}",
        "Counter target spell. Create X Treasure tokens, where X is that spell's mana value. (They're artifacts with \"{T}, Sacrifice this token: Add one mana of any color.\")",
    );
    let rule = rule(&parsed, 0);
    let kinds = effect_kinds(rule);
    let bind = kinds.iter().position(|kind| *kind == "bind").expect("bind");
    let counter = kinds
        .iter()
        .position(|kind| *kind == "counterSpell")
        .expect("counter");
    assert!(bind < counter, "{kinds:?}");
    assert_eq!(rule["effects"][bind]["value"]["kind"], "manaValueOf");
}

#[test]
fn a_definition_reaches_a_later_condition_on_x() {
    let parsed = parse(
        "Enchantment",
        "{3}{B}",
        "Whenever you attack, put X +1/+1 counters on target attacking creature, where X is the number of permanents you've sacrificed this turn. If X is three or more, that creature gains lifelink until end of turn.",
    );
    let rule = rule(&parsed, 0);
    assert!(!mentions_x(rule), "{rule}");
    let kinds = effect_kinds(rule);
    assert_eq!(kinds.first(), Some(&"bind"), "{kinds:?}");
    assert!(kinds.contains(&"conditional"), "{kinds:?}");
}
