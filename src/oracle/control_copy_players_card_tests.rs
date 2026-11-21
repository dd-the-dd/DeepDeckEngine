//! Control changes, stack copies and one sentence acting for several players,
//! proved on complete cards through the native pipeline: real card, an unused
//! recombination of the same shared children, and near-misses that must fail.
use serde_json::{Value, json};

use crate::oracle::model::OracleCardParseRequest;
use crate::oracle::pipeline::parse_oracle_card_native;

fn abilities(name: &str, type_line: &str, text: &str) -> Vec<(String, Option<Value>)> {
    let result = parse_oracle_card_native(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    result
        .abilities
        .into_iter()
        .map(|ability| {
            let diagnostic = ability.diagnostics.first();
            let state = diagnostic
                .and_then(|diagnostic| diagnostic.state.clone())
                .unwrap_or_default();
            let message = diagnostic.map(|d| d.message.clone()).unwrap_or_default();
            (
                format!("{}|{}|{}", ability.status, state, message),
                ability.rule,
            )
        })
        .collect()
}

fn canonical_rules(name: &str, type_line: &str, text: &str) -> Vec<Value> {
    abilities(name, type_line, text)
        .into_iter()
        .map(|(status, rule)| {
            assert!(status.starts_with("canonical"), "{name}: {status}");
            rule.expect("canonical rule")
        })
        .collect()
}

fn last_rule(name: &str, type_line: &str, text: &str) -> Value {
    canonical_rules(name, type_line, text)
        .pop()
        .expect("one canonical rule")
}

fn assert_not_canonical(name: &str, type_line: &str, text: &str) {
    let statuses = abilities(name, type_line, text);
    assert!(
        statuses
            .iter()
            .any(|(status, _)| !status.starts_with("canonical")),
        "{text}: {statuses:?}"
    );
}

fn controller() -> Value {
    json!({"kind": "controllerOf", "object": {"kind": "self"}})
}

fn target_player() -> Value {
    json!({"kind": "chosenTarget", "id": "targetPlayer"})
}

fn integer(value: i64) -> Value {
    json!({"kind": "integer", "value": value})
}

// --- One sentence, several player subjects (CR 109.5, 608.2c) -------------

#[test]
fn you_and_target_opponent_each_draw_distributes_one_action_per_player() {
    let rule = last_rule(
        "Secret Rendezvous",
        "Sorcery",
        "You and target opponent each draw three cards.",
    );
    assert_eq!(
        rule["effects"],
        json!([
            {"kind": "drawCards", "player": controller(), "count": integer(3)},
            {"kind": "drawCards", "player": target_player(), "count": integer(3)},
        ])
    );
    let decisions = rule["declaration"]["decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0]["id"], "targetPlayer");
    // The activated sibling reuses the same reader under a cost.
    let loran = last_rule(
        "Loran of the Third Path",
        "Legendary Creature — Human Artificer",
        "{T}: You and target opponent each draw a card.",
    );
    assert_eq!(loran["effects"][1]["player"], target_player());
}

#[test]
fn coordinated_subjects_complete_each_action_before_the_next() {
    // Unused recombination: two actions, one of them a life action.
    let rule = last_rule(
        "Probe",
        "Sorcery",
        "You and target player each draw two cards and lose 2 life.",
    );
    assert_eq!(
        rule["effects"],
        json!([
            {"kind": "drawCards", "player": controller(), "count": integer(2)},
            {"kind": "drawCards", "player": target_player(), "count": integer(2)},
            {"kind": "loseLife", "player": controller(), "amount": integer(2)},
            {"kind": "loseLife", "player": target_player(), "amount": integer(2)},
        ])
    );
}

#[test]
fn coordinated_subject_near_misses_fail_explicitly() {
    for text in [
        "You and target opponent each draws a card.",
        "You and target opponent draw a card.",
        "You and target opponent each sing a song.",
    ] {
        assert_not_canonical("Probe", "Sorcery", text);
    }
}

#[test]
fn quantified_player_list_reads_a_then_connector() {
    let rule = last_rule(
        "Geier Reach Sanitarium",
        "Legendary Land",
        "{2}, {T}: Each player draws a card, then discards a card.",
    );
    let effects = rule["effects"].as_array().unwrap();
    assert_eq!(effects.len(), 2);
    assert_eq!(effects[0]["kind"], "forEachPlayer");
    assert_eq!(effects[0]["effects"][0]["kind"], "drawCards");
    assert_eq!(effects[1]["kind"], "forEachPlayer");
    assert_eq!(effects[1]["players"], json!({"kind": "eachPlayer"}));
    // Unused recombination: mill then lose life for each opponent.
    let rule = last_rule(
        "Probe",
        "Sorcery",
        "Each opponent mills two cards, then loses 2 life.",
    );
    let effects = rule["effects"].as_array().unwrap();
    assert_eq!(effects.len(), 2);
    assert_eq!(effects[1]["kind"], "loseLife");
    assert_not_canonical(
        "Probe",
        "Sorcery",
        "Each player draws a card, then sings a song.",
    );
}

// --- Control changes (CR 108.4, 613.1b, 701.12) ---------------------------

#[test]
fn gain_control_of_target_is_indefinite_without_a_duration() {
    let rule = last_rule(
        "Empress Galina",
        "Legendary Creature — Merfolk Noble",
        "{U}{U}, {T}: Gain control of target legendary permanent. (This effect lasts indefinitely.)",
    );
    assert_eq!(rule["effects"].as_array().unwrap().len(), 1);
    let effect = &rule["effects"][0];
    assert_eq!(effect["kind"], "gainControlPermanent");
    assert_eq!(effect["controller"], controller());
    assert_eq!(effect["permanent"]["kind"], "chosenTarget");
    assert!(effect.get("duration").is_none(), "{effect}");
}

#[test]
fn gain_control_composes_with_preposed_duration_and_anaphora() {
    // Unused recombination: preposed duration, explicit `you`, then the
    // shared permanent anaphor for a keyword grant.
    let rule = last_rule(
        "Probe",
        "Sorcery",
        "Until end of turn, you gain control of target creature and it gains haste.",
    );
    let effects = rule["effects"].as_array().unwrap();
    assert_eq!(effects[0]["kind"], "gainControlPermanent");
    assert_eq!(
        effects[0]["duration"],
        json!({"kind": "untilEndOfCurrentTurn"})
    );
    assert_eq!(effects[1]["object"], effects[0]["permanent"]);
    // Threaten shape: untap, then gain control of the same object.
    let rule = last_rule(
        "Probe",
        "Sorcery",
        "Gain control of target artifact until end of turn. Untap that artifact.",
    );
    assert_eq!(rule["effects"][0]["permanent"]["kind"], "chosenTarget");
    assert_eq!(
        rule["effects"][0]["duration"],
        json!({"kind": "untilEndOfCurrentTurn"})
    );
}

#[test]
fn exchange_control_requires_one_pair_of_targets() {
    let rule = last_rule(
        "Switcheroo",
        "Sorcery",
        "Exchange control of two target creatures.",
    );
    assert_eq!(rule["effects"][0]["kind"], "exchangeControl");
    assert_eq!(rule["effects"][0]["objects"]["kind"], "chosenTargets");
    let decision = &rule["declaration"]["decisions"][0];
    assert_eq!(decision["minimum"], 2);
    assert_eq!(decision["maximum"], 2);
    // Unused recombination: another permanent predicate in the same slot.
    let rule = last_rule(
        "Probe",
        "Sorcery",
        "Exchange control of two target artifacts.",
    );
    assert_eq!(rule["effects"][0]["kind"], "exchangeControl");
}

#[test]
fn control_change_near_misses_fail_explicitly() {
    for text in [
        "Exchange control of target creature.",
        "Exchange control of two target creatures until end of turn.",
        "Gain control of target creature until end of your next turn.",
        "Gain control of target player.",
    ] {
        assert_not_canonical("Probe", "Sorcery", text);
    }
}

#[test]
fn you_control_enchanted_permanent_is_a_static_control_effect() {
    for (name, text) in [
        (
            "Control Magic",
            "Enchant creature\nYou control enchanted creature.",
        ),
        (
            "In Bolas's Clutches",
            "Enchant permanent\nYou control enchanted permanent.",
        ),
        // Unused recombination: another attached permanent type.
        ("Probe", "Enchant artifact\nYou control enchanted artifact."),
    ] {
        let rule = last_rule(name, "Enchantment — Aura", text);
        assert_eq!(rule["kind"], "staticAbility");
        assert_eq!(
            rule["modifiers"],
            json!([{"kind": "controlAttachedPermanent", "attachment": {"kind": "self"}}])
        );
    }
    assert_not_canonical(
        "Probe",
        "Enchantment — Aura",
        "Enchant creature\nYou control enchanted creature until end of turn.",
    );
    assert_not_canonical(
        "Probe",
        "Enchantment — Aura",
        "Enchant creature\nYou control each creature.",
    );
    assert_not_canonical("Probe", "Artifact", "You control enchanted creature.");
}

// --- Stack copies (CR 707.10) ----------------------------------------------

#[test]
fn copy_target_spell_with_new_targets_permission() {
    let rule = last_rule(
        "Dualcaster Mage",
        "Creature — Human Wizard",
        "Flash\nWhen this creature enters, copy target instant or sorcery spell. You may choose new targets for the copy.",
    );
    assert_eq!(rule["kind"], "triggeredAbility");
    assert_eq!(
        rule["effects"],
        json!([{
            "kind": "copyStackItem",
            "object": {"kind": "chosenTarget", "id": "targetSpell"},
            "controller": controller(),
            "mayChooseNewTargets": true,
        }])
    );
    let rule = last_rule(
        "Expansion",
        "Instant",
        "Copy target instant or sorcery spell with mana value 4 or less. You may choose new targets for the copy.",
    );
    assert_eq!(rule["effects"][0]["mayChooseNewTargets"], true);
    // Unused recombination: controller-scoped spell target, no permission,
    // then an ordinary draw.
    let rule = last_rule(
        "Probe",
        "Instant",
        "Copy target creature spell you control. Draw a card.",
    );
    assert_eq!(rule["effects"][0]["mayChooseNewTargets"], false);
    assert_eq!(rule["effects"][1]["kind"], "drawCards");
}

#[test]
fn copy_near_misses_fail_explicitly() {
    for text in [
        "Copy target creature.",
        "You may choose new targets for the copy.",
        "Copy target instant or sorcery spell. You may choose new targets for the copies.",
        "Copy target instant spell. Draw a card. You may choose new targets for the copy.",
    ] {
        assert_not_canonical("Probe", "Instant", text);
    }
}
