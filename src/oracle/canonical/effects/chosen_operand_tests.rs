//! Lowered program of an operand chosen on resolution (CR 608.2d): the
//! shared `choosePermanents` and its `bind` run immediately before the
//! action in the same instruction list (so a performer's loop contains
//! both), and the action and a later anaphor read the bound result.
use serde_json::Value;

use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};

fn canonical_rules(name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
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
        .collect::<Vec<_>>();
    assert_eq!(parsed.status, "canonical", "{oracle}: {states:?}");
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .collect()
}

/// Every instruction list in `value` (JSON arrays of objects).
fn instruction_lists(value: &Value) -> Vec<&Vec<Value>> {
    let mut found = Vec::new();
    let mut stack = vec![value];
    while let Some(next) = stack.pop() {
        match next {
            Value::Array(values) => {
                found.push(values);
                stack.extend(values);
            }
            Value::Object(fields) => stack.extend(fields.values()),
            _ => {}
        }
    }
    found
}

/// The instruction list holding the choice, with the choice's position.
fn choice_list(rule: &Value) -> (&Vec<Value>, usize) {
    instruction_lists(rule)
        .into_iter()
        .find_map(|list| {
            list.iter()
                .position(|effect| effect["kind"] == "choosePermanents")
                .map(|index| (list, index))
        })
        .unwrap_or_else(|| panic!("no resolution choice in {rule:#}"))
}

#[test]
fn the_choice_and_its_binding_run_right_before_the_action() {
    let rules = canonical_rules(
        "Kayla's Command Mode",
        "Sorcery",
        "Put a +1/+1 counter on a creature you control. It gains double strike until end of turn.",
    );
    let (list, index) = choice_list(&rules[0]);
    let choose = &list[index];
    assert_eq!(choose["minimum"], list[index]["maximum"]);
    let bind = &list[index + 1];
    assert_eq!(bind["kind"], "bind");
    assert_eq!(bind["value"]["kind"], "decisionResult");
    assert_eq!(bind["value"]["decisionId"], choose["id"]);
    let binding = bind["id"].as_str().expect("binding id");
    let put = &list[index + 2];
    assert_eq!(put["kind"], "putCounters");
    assert_eq!(put["permanent"]["kind"], "singleBoundObject");
    assert_eq!(put["permanent"]["binding"], binding);
    // `It gains double strike`: the same chosen object, never a target.
    let reads = list[index + 3..]
        .iter()
        .any(|effect| effect.to_string().contains(binding));
    assert!(reads, "{list:#?}");
    assert!(
        !rules[0].to_string().contains("chooseTargets"),
        "{:#}",
        rules[0]
    );
}

#[test]
fn a_performer_chooses_among_its_own_permanents_inside_its_loop() {
    // Agitator Ant's first sentence (each player, `they control` read in
    // the chooser's person) and Hunted Nightmare (a declared opponent).
    for (name, oracle) in [
        (
            "Agitator Ant",
            "At the beginning of your end step, each player may put two +1/+1 counters on a creature they control.",
        ),
        (
            "Hunted Nightmare",
            "When this creature enters, target opponent puts a deathtouch counter on a creature they control.",
        ),
    ] {
        let rules = canonical_rules(name, "Creature \u{2014} Insect", oracle);
        let (list, index) = choice_list(&rules[0]);
        assert!(
            list[index + 1..]
                .iter()
                .any(|effect| effect.to_string().contains("putCounters")),
            "{oracle}: the action shares the choice's instruction list {list:#?}"
        );
        assert!(
            list[index]["candidates"].get("controller").is_some(),
            "{oracle}: the chooser's own permanents"
        );
    }
}
