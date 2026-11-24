//! Permanents and numbers chosen on resolution (CR 608.2d) lowered as one
//! decision step plus the binding later references read: `it` / `that
//! creature` after one chooser's single choice, `the rest` of every
//! chooser's domain, and `that many` / `that much` after a number. A choice
//! that nothing reads is refused, not a no-op decision.
use serde_json::{Value, json};

use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(name: &str, type_line: &str, text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{B}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes")
}

fn lowered(name: &str, type_line: &str, text: &str) -> Vec<Value> {
    let parsed = parse(name, type_line, text);
    let ability = parsed
        .abilities
        .iter()
        .rev()
        .find(|ability| ability.status != "empty")
        .expect("one ability");
    assert_eq!(ability.status, "canonical", "{text}: {parsed:#?}");
    ability.rule.clone().expect("canonical rule")["effects"]
        .as_array()
        .expect("effect program")
        .clone()
}

fn failure_states(text: &str) -> Vec<String> {
    let parsed = parse("Near Miss", "Sorcery", text);
    let ability = parsed
        .abilities
        .iter()
        .rev()
        .find(|ability| ability.status != "empty")
        .expect("one ability");
    assert_ne!(ability.status, "canonical", "{text}");
    ability
        .diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.state.clone())
        .collect()
}

fn chosen(effect_index: usize) -> Value {
    json!({"kind": "singleBoundObject", "binding": format!("effect{effect_index}:chosenObjects")})
}

/// Real card (Imperial Edict): a declared opponent chooses one of its own
/// creatures, and `that creature` is the chosen permanent.
#[test]
fn a_declared_chooser_binds_the_chosen_creature_for_that_creature() {
    let effects = lowered(
        "Imperial Edict",
        "Sorcery",
        "Target opponent chooses a creature they control. Destroy that creature.",
    );
    assert_eq!(effects[0]["kind"], "choosePermanents", "{effects:#?}");
    assert_eq!(
        effects[0]["candidates"]["controller"]["kind"],
        "controllerOf"
    );
    assert!(effects[0].get("performer").is_some(), "{effects:#?}");
    assert_eq!(effects[1]["kind"], "bind");
    assert_eq!(effects[1]["value"]["kind"], "decisionResult");
    assert_eq!(effects[2]["kind"], "destroyPermanent");
    assert_eq!(effects[2]["permanent"], chosen(0));
}

/// Sibling (Final Showdown's mode): the controller's own choice read by
/// `it`; unused combination: the chosen permanent feeds `gain control of`.
#[test]
fn the_chosen_permanent_recombines_with_ordinary_permanent_actions() {
    let effects = lowered(
        "Choice Probe",
        "Instant",
        "Choose a creature you control. It gains indestructible until end of turn.",
    );
    assert_eq!(effects[0]["kind"], "choosePermanents");
    assert_eq!(effects[0]["player"]["kind"], "controllerOf");
    assert!(
        effects[2..]
            .iter()
            .any(|effect| effect.to_string().contains(&chosen(0).to_string())),
        "{effects:#?}"
    );
    let effects = lowered(
        "Control Probe",
        "Sorcery",
        "Target opponent chooses a creature they control. You gain control of it.",
    );
    assert!(
        effects[2..]
            .iter()
            .any(|effect| effect.to_string().contains(&chosen(0).to_string())),
        "{effects:#?}"
    );
}

/// Real cards (Razia's Purification, Single Combat): every player chooses,
/// then every player sacrifices its own unchosen permanents in one batch
/// after the loop (CR 608.2c).
#[test]
fn each_player_chooses_then_the_rest_is_sacrificed_after_every_choice() {
    for (name, text) in [
        (
            "Razia's Purification",
            "Each player chooses three permanents they control, then sacrifices the rest.",
        ),
        (
            "Single Combat",
            "Each player chooses a creature or planeswalker they control, then sacrifices the rest. Players can't cast creature or planeswalker spells until the end of your next turn.",
        ),
    ] {
        let parsed = parse(name, "Sorcery", text);
        let rule = parsed
            .abilities
            .iter()
            .find_map(|ability| ability.rule.clone())
            .unwrap_or_else(|| panic!("{text}: {parsed:#?}"));
        let effects = rule["effects"].as_array().expect("effect program");
        let loop_index = effects
            .iter()
            .position(|effect| effect["kind"] == "forEachPlayer")
            .unwrap_or_else(|| panic!("{text}: {effects:#?}"));
        let body = effects[loop_index]["effects"]
            .as_array()
            .expect("loop body");
        assert_eq!(body[0]["kind"], "choosePermanents", "{text}");
        assert_eq!(body[1]["kind"], "bind", "{text}");
        assert_eq!(body[1]["value"]["kind"], "setDifference", "{text}");
        assert_eq!(
            effects[loop_index]["collect"]["bind"], "effect0:unchosenObjects",
            "{text}"
        );
        assert_eq!(
            effects[loop_index + 1],
            json!({
                "kind": "sacrificePermanents",
                "player": {"kind": "eachPlayer"},
                "objects": {"kind": "boundObjects", "binding": "effect0:unchosenObjects"},
            }),
            "{text}"
        );
    }
}

/// Real card (Divine Reckoning): `Destroy the rest.` as the next sentence
/// destroys every creature no player chose.
#[test]
fn the_rest_of_every_choice_is_destroyed_by_the_next_sentence() {
    let effects = lowered(
        "Divine Reckoning",
        "Sorcery",
        "Each player chooses a creature they control. Destroy the rest.",
    );
    assert_eq!(effects[0]["kind"], "forEachPlayer", "{effects:#?}");
    assert_eq!(effects[0]["collect"]["bind"], "effect0:unchosenObjects");
    assert_eq!(
        effects[1],
        json!({
            "kind": "destroyPermanent",
            "permanent": {"kind": "boundObjects", "binding": "effect0:unchosenObjects"},
        })
    );
}

/// A chosen number is read by `that much` through the shared action-result
/// quantity, which binds the decision step.
#[test]
fn a_chosen_number_is_read_by_that_much() {
    let effects = lowered(
        "Number Probe",
        "Sorcery",
        "Choose a number between 6 and 8. Number Probe deals that much damage to target creature.",
    );
    assert_eq!(effects[0]["kind"], "chooseNumber", "{effects:#?}");
    assert_eq!(effects[0]["minimum"]["value"], 6);
    assert_eq!(effects[0]["maximum"]["value"], 8);
    let binding = effects[0]["bind"]
        .as_str()
        .expect("the reader binds the choice");
    assert!(effects[1].to_string().contains(binding), "{effects:#?}");
}

/// Near-misses: a choice nothing reads, a population of controllers, a
/// plural choice read by a singular pronoun, and a number nothing reads.
#[test]
fn choices_without_an_executable_consumer_fail_explicitly() {
    for (text, state) in [
        (
            "Choose a permanent you control. Draw a card.",
            "lower.choose.objects.consumer",
        ),
        (
            "Choose a creature an opponent controls. Destroy that creature.",
            "lower.choose.objects",
        ),
        (
            "Choose a number between 0 and 13. Draw a card.",
            "lower.choose.number.consumer",
        ),
    ] {
        let states = failure_states(text);
        assert!(
            states.iter().any(|actual| actual == state),
            "{text}: {states:?}"
        );
    }
    // Each player's choice is no singular antecedent of `it`.
    assert!(!failure_states("Each player chooses a creature they control. Destroy it.").is_empty());
}
