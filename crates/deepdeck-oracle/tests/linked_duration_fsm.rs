//! Oracle text -> primitive IR for one-shot `until` zone changes (CR 610.3)
//! and delayed step triggers created by an effect program (CR 603.7).
use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

fn parse(card_name: &str, type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .name("linked-duration-fsm-test".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser test thread starts")
        .join()
        .expect("parser test thread completes")
}

fn only_rule(parsed: &OracleCardParseResult) -> &Value {
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let ability = parsed.abilities.last().expect("one ability");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    ability.rule.as_ref().expect("canonical rule")
}

fn failure_state(parsed: &OracleCardParseResult) -> Vec<String> {
    let ability = parsed.abilities.last().expect("one ability");
    assert_eq!(ability.status, "unsupported", "{:#?}", ability.rule);
    ability
        .diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.state.clone())
        .collect()
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

#[test]
fn seam_rip_exiles_its_composed_target_until_the_source_leaves() {
    let parsed = parse(
        "Seam Rip",
        "Enchantment",
        "When this enchantment enters, exile target nonland permanent an opponent controls with mana value 2 or less until this enchantment leaves the battlefield.",
    );
    let rule = only_rule(&parsed);
    assert_eq!(
        rule["event"],
        json!({ "kind": "enterBattlefield", "object": { "kind": "self" } })
    );
    let decision = &rule["declaration"]["decisions"][0];
    assert_eq!(decision["kind"], "chooseTargets");
    assert_eq!(
        (decision["minimum"].as_i64(), decision["maximum"].as_i64()),
        (Some(1), Some(1))
    );
    assert_eq!(
        decision["candidates"]["controller"],
        json!({ "kind": "opponentsOf", "player": controller() })
    );
    let target_id = decision["id"].as_str().expect("target id");
    assert_eq!(
        rule["effects"],
        json!([{
            "kind": "exilePermanent",
            "permanent": { "kind": "chosenTarget", "id": target_id },
            "until": { "kind": "sourceLeavesBattlefield" },
        }])
    );
}

/// Unused combination: optional cardinality, own-controller relation, a
/// different source noun and a trailing reminder sentence.
#[test]
fn until_source_leaves_reuses_cardinality_controller_and_reminder_children() {
    let parsed = parse(
        "Test Containment Device",
        "Artifact",
        "When this artifact enters, exile up to one target creature you control with mana value 4 or greater until this artifact leaves the battlefield. (That creature returns under its owner's control.)",
    );
    let rule = only_rule(&parsed);
    let decision = &rule["declaration"]["decisions"][0];
    assert_eq!(
        (decision["minimum"].as_i64(), decision["maximum"].as_i64()),
        (Some(0), Some(1))
    );
    assert_eq!(decision["candidates"]["controller"], controller());
    assert_eq!(
        rule["effects"][0]["until"],
        json!({ "kind": "sourceLeavesBattlefield" })
    );
}

#[test]
fn until_other_event_or_non_exile_action_is_explicitly_unsupported() {
    let graveyard = parse(
        "Test Broken Seal",
        "Enchantment",
        "When this enchantment enters, exile target creature until this enchantment leaves your graveyard.",
    );
    assert!(failure_state(&graveyard).contains(&"zone-change.until.zone".to_string()));
    let destroy = parse(
        "Test Broken Seal",
        "Enchantment",
        "When this enchantment enters, destroy target creature until this enchantment leaves the battlefield.",
    );
    assert!(failure_state(&destroy).contains(&"sequence.connector-or-end".to_string()));
}

#[test]
fn flickerwisp_returns_the_exiled_card_from_a_delayed_end_step_trigger() {
    let parsed = parse(
        "Flickerwisp",
        "Creature — Elemental",
        "Flying\nWhen this creature enters, exile another target permanent. Return that card to the battlefield under its owner's control at the beginning of the next end step.",
    );
    let rule = only_rule(&parsed);
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("target id");
    let exiled = rule["effects"][0]["bind"].as_str().expect("exile result");
    assert_eq!(rule["effects"][0]["permanent"]["id"], target_id);
    assert_eq!(
        rule["effects"][1],
        json!({
            "kind": "installDelayedStepTrigger",
            "step": "endStep",
            "controller": controller(),
            "captureBindings": [exiled],
            "effects": [{
                "kind": "moveCards",
                "cards": { "kind": "boundObjects", "binding": exiled },
                "to": {
                    "kind": "battlefield",
                    "player": { "kind": "ownerOf", "object": { "kind": "candidate" } },
                    "tapped": false,
                },
                "bind": "movedOntoBattlefield",
            }],
        })
    );
}

/// Unused combination: a preposed delayed sentence, the controller's next
/// upkeep and a targetless sweep with no captured binding.
#[test]
fn preposed_controller_upkeep_and_unbound_sweep_reuse_the_same_frame() {
    let parsed = parse(
        "Test Slow Return",
        "Sorcery",
        "Exile target creature. At the beginning of your next upkeep, return that card to the battlefield under its owner's control.",
    );
    let rule = only_rule(&parsed);
    let delayed = &rule["effects"][1];
    assert_eq!(delayed["kind"], "installDelayedStepTrigger");
    assert_eq!(delayed["step"], "upkeep");
    assert_eq!(delayed["turnOf"], controller());
    assert_eq!(
        delayed["captureBindings"],
        json!([rule["effects"][0]["bind"]])
    );

    let parsed = parse(
        "Bearer of the Heavens",
        "Creature — Giant",
        "When this creature dies, destroy all permanents at the beginning of the next end step.",
    );
    let rule = only_rule(&parsed);
    assert_eq!(rule["effects"][0]["kind"], "installDelayedStepTrigger");
    assert!(rule["effects"][0].get("captureBindings").is_none());
    assert_eq!(rule["effects"][0]["effects"][0]["kind"], "destroyPermanent");
}

#[test]
fn delayed_targets_and_unscoped_trailing_timing_fail_explicitly() {
    // A single declared target is captured with its identity (CR 603.7c);
    // a plural declaration has no capture and stays refused.
    let target = parse(
        "Test Late Removal",
        "Sorcery",
        "Draw a card. Destroy target creature at the beginning of the next end step.",
    );
    let rule = only_rule(&target);
    assert_eq!(rule["effects"][1]["kind"], "installDelayedStepTrigger");
    assert_eq!(
        rule["effects"][1]["captureReferences"][0]["reference"]["kind"],
        "chosenTarget"
    );
    let plural = parse(
        "Test Late Purge",
        "Sorcery",
        "Draw a card. Destroy two target creatures at the beginning of the next end step.",
    );
    assert!(failure_state(&plural).contains(&"lower.delayed.object-references".to_string()));
    let continued = parse(
        "Test Late Removal",
        "Sorcery",
        "Draw a card. Exile target creature at the beginning of the next end step and draw a card.",
    );
    assert!(failure_state(&continued).contains(&"sequence.delayed.trailing-scope".to_string()));
}
