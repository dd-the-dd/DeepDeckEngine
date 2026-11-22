//! The effect-subject policy: an explicit subject followed by a clause.
//! `you VERB` is the controller's clause, read by the same action readers as
//! the imperative; verbs no reader takes fail at `clause.verb.unsupported`.
use serde_json::Value;

use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};

fn parse(type_line: &str, oracle: &str) -> (String, Vec<Value>, Vec<String>) {
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
    let rules = parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .collect();
    (parsed.status, rules, states)
}

fn effects(type_line: &str, oracle: &str) -> Vec<Value> {
    let (status, rules, states) = parse(type_line, oracle);
    assert_eq!(status, "canonical", "{oracle}: {states:?}");
    rules
        .iter()
        .flat_map(|rule| rule["effects"].as_array().cloned().unwrap_or_default())
        .collect()
}

const CREATURE: &str = "Creature \u{2014} Elf";
const SORCERY: &str = "Sorcery";

/// `you VERB` and the imperative `VERB` are one clause: the same IR.
#[test]
fn the_controller_subject_reads_the_imperative_clause() {
    // Real cards: Eivor, Wolf-Kissed (mill), Cruel Ultimatum (a coordinated
    // list continuing the subject). Unused recombinations: scry, surveil and
    // investigate after an explicit `you`.
    for (type_line, explicit, imperative) in [
        (
            CREATURE,
            "Whenever this creature deals combat damage to a player, you mill that many cards.",
            "Whenever this creature deals combat damage to a player, mill that many cards.",
        ),
        (
            SORCERY,
            "Each opponent sacrifices a creature. You return a creature card from your graveyard to your hand, draw three cards, and gain 5 life.",
            "Each opponent sacrifices a creature. Return a creature card from your graveyard to your hand, draw three cards, and gain 5 life.",
        ),
        (
            CREATURE,
            "When this creature enters, you scry 2.",
            "When this creature enters, scry 2.",
        ),
        (
            CREATURE,
            "When this creature enters, you surveil 1, then you investigate.",
            "When this creature enters, surveil 1, then investigate.",
        ),
    ] {
        assert_eq!(
            effects(type_line, explicit),
            effects(type_line, imperative),
            "{explicit}"
        );
    }
}

/// An explicit `you` replaces the subject the previous clause would lend a
/// subjectless verb: the controller mills, not the target player.
#[test]
fn the_controller_subject_is_not_the_inherited_player() {
    let effects = effects(
        SORCERY,
        "Target player draws two cards, then you mill two cards.",
    );
    let mill = effects
        .iter()
        .find(|effect| effect["kind"] == "millCards" || effect["kind"] == "mill")
        .unwrap_or_else(|| panic!("a mill effect: {effects:#?}"));
    assert_ne!(mill["player"]["kind"], "targetPlayer", "{mill:#?}");
}

/// Near misses: a verb no action reader takes names its lemma and the
/// subject; modal continuations keep their own frames.
#[test]
fn an_unread_verb_of_the_controller_fails_at_the_clause() {
    let (status, _, states) = parse(
        CREATURE,
        "When this creature enters, you get an emblem with \"Creatures you control get +1/+1.\"",
    );
    assert_ne!(status, "canonical");
    assert!(
        states
            .iter()
            .any(|state| state == "clause.verb.unsupported"),
        "{states:?}"
    );
    let (_, _, states) = parse(CREATURE, "When this creature enters, you may draw a card.");
    assert!(
        !states
            .iter()
            .any(|state| state == "clause.verb.unsupported"),
        "{states:?}"
    );
}

fn player_counters(type_line: &str, oracle: &str) -> Vec<Value> {
    fn collect(effect: &Value, found: &mut Vec<Value>) {
        if effect["kind"] == "addPlayerCounters" {
            found.push(effect.clone());
        }
        for child in effect["effects"].as_array().into_iter().flatten() {
            collect(child, found);
        }
    }
    let mut found = Vec::new();
    for effect in effects(type_line, oracle) {
        collect(&effect, &mut found);
    }
    found
}

/// `get` with a player-counters frame (CR 122.1): every player subject the
/// clause reads, one lowering.
#[test]
fn player_subjects_get_counters() {
    use serde_json::json;
    // Real cards: Pit Scorpion (event player), Pistus Strike (its
    // controller), Aethertorch Renegade (energy symbols and reminder).
    // Unused recombinations: a quantified population with a counter name
    // and a counted experience counter for the controller.
    for (type_line, oracle, player, counter, count) in [
        (
            "Creature \u{2014} Scorpion",
            "Whenever this creature deals damage to a player, that player gets a poison counter.",
            json!({"kind":"triggeringPlayer"}),
            "poison",
            1,
        ),
        (
            "Instant",
            "Destroy target creature with flying. Its controller gets a poison counter.",
            json!("controllerOf"),
            "poison",
            1,
        ),
        (
            CREATURE,
            "When this creature enters, you get {E}{E}{E}{E} (four energy counters).",
            json!({"kind":"controllerOf","object":{"kind":"self"}}),
            "energy",
            4,
        ),
        (
            CREATURE,
            "When this creature enters, each opponent gets two rad counters.",
            json!(null),
            "rad",
            2,
        ),
        (
            CREATURE,
            "When this creature enters, you get an experience counter.",
            json!({"kind":"controllerOf","object":{"kind":"self"}}),
            "experience",
            1,
        ),
    ] {
        let effects = player_counters(type_line, oracle);
        assert_eq!(effects.len(), 1, "{oracle}: {effects:#?}");
        let effect = &effects[0];
        assert_eq!(effect["counter"], counter, "{oracle}");
        assert_eq!(effect["count"]["value"], count, "{oracle}: {effect:#}");
        match player {
            Value::Null => {}
            Value::String(kind) => assert_eq!(effect["player"]["kind"], kind, "{oracle}"),
            player => assert_eq!(effect["player"], player, "{oracle}"),
        }
    }
}

/// Near misses: a reminder that miscounts the symbols fails; counters put
/// `on` an object stay with the counter-placement frame.
#[test]
fn player_counters_near_misses() {
    let (status, _, states) = parse(
        CREATURE,
        "When this creature enters, you get {E}{E} (three energy counters).",
    );
    assert_ne!(status, "canonical");
    assert!(
        states.iter().any(
            |state| state.starts_with("player-action.counters") || state.starts_with("numeric")
        ),
        "{states:?}"
    );
    assert!(
        player_counters(
            CREATURE,
            "When this creature enters, put a +1/+1 counter on target creature.",
        )
        .is_empty()
    );
}

/// `they` names the player `that player` would name, with the plural verb:
/// the same IR. A creature-population damage trigger records the damaged
/// player (CR 510.2), so its `that player` is the event's player.
#[test]
fn they_and_that_player_are_one_player_reference() {
    // Real cards: Sheoldred, the Apocalypse (draw event, life), Larceny
    // (population combat damage, discard). Unused recombinations: `they`
    // after a declared target player, `they` with discard.
    for (type_line, pronoun, phrase) in [
        (
            CREATURE,
            "Whenever an opponent draws a card, they lose 2 life.",
            "Whenever an opponent draws a card, that player loses 2 life.",
        ),
        (
            SORCERY,
            "Target player draws a card. They lose 1 life.",
            "Target player draws a card. That player loses 1 life.",
        ),
        (
            "Enchantment",
            "Whenever a creature you control deals combat damage to a player, they discard a card.",
            "Whenever a creature you control deals combat damage to a player, that player discards a card.",
        ),
    ] {
        assert_eq!(
            effects(type_line, pronoun),
            effects(type_line, phrase),
            "{pronoun}"
        );
    }
    let discard = effects(
        "Enchantment",
        "Whenever a creature you control deals combat damage to a player, that player discards a card.",
    );
    assert!(
        discard
            .iter()
            .any(|effect| effect["player"]["kind"] == "triggeringPlayer"),
        "{discard:#?}"
    );
}

/// Near misses: `they` without a player binding is not a player subject, and
/// a plural object antecedent keeps its own readers.
#[test]
fn they_without_a_player_is_not_claimed() {
    let (status, _, _) = parse(CREATURE, "When this creature enters, they lose 2 life.");
    assert_ne!(status, "canonical");
    let (status, _, _) = parse(CREATURE, "When this creature enters, they discard a card.");
    assert_ne!(status, "canonical");
}

/// `For each COUNTABLE, ACTION.` is the trailing multiplier written first:
/// the same IR as `ACTION for each COUNTABLE`.
#[test]
fn a_preposed_for_each_scales_its_action() {
    // Real card shape: Tribune of Rot / Moogles' Valor (count, then the one
    // action). Unused recombinations: draw and damage scaled by a count.
    for (preposed, trailing) in [
        (
            "For each creature you control, create a 1/1 white Soldier creature token.",
            "Create a 1/1 white Soldier creature token for each creature you control.",
        ),
        (
            "For each artifact you control, draw a card.",
            "Draw a card for each artifact you control.",
        ),
        (
            "For each creature you control, this spell deals 1 damage to any target.",
            "This spell deals 1 damage to any target for each creature you control.",
        ),
    ] {
        assert_eq!(
            effects(SORCERY, preposed),
            effects(SORCERY, trailing),
            "{preposed}"
        );
    }
}

/// Near misses: the introducer scopes one action ending its sentence, and a
/// player population is not a countable (a distributive frame, not a count).
#[test]
fn a_preposed_for_each_scopes_one_counted_action() {
    let (status, _, states) = parse(
        SORCERY,
        "For each creature you control, draw a card and gain 1 life.",
    );
    assert_ne!(status, "canonical");
    assert!(
        states
            .iter()
            .any(|state| state == "sequence.for-each.preposed-action"),
        "{states:?}"
    );
    let (status, _, _) = parse(
        SORCERY,
        "For each opponent, create a 1/1 white Soldier creature token.",
    );
    assert_ne!(status, "canonical");
}

fn untap_restrictions(type_line: &str, oracle: &str) -> Vec<Value> {
    effects(type_line, oracle)
        .into_iter()
        .filter(|effect| effect["kind"] == "installUntapRestriction")
        .collect()
}

/// The negated `untap` clause of a permanent subject (CR 502.3): the
/// subject is the shared permanent reference, the window is the controller's
/// next untap step or the source's control duration.
#[test]
fn a_permanent_subject_does_not_untap() {
    // Real cards: Frost Lynx (`that creature`, next step), Chill of the Grave
    // (`it`), Tidebinder Mage (while the source is controlled). Unused
    // recombination: the restriction after a stat change.
    for (type_line, oracle, duration) in [
        (
            CREATURE,
            "When this creature enters, tap target creature an opponent controls. That creature doesn't untap during its controller's next untap step.",
            "nextUntapStep",
        ),
        (
            "Instant",
            "Tap target creature. It doesn't untap during its controller's next untap step.",
            "nextUntapStep",
        ),
        (
            CREATURE,
            "When this creature enters, tap target red or green creature an opponent controls. That creature doesn't untap during its controller's untap step for as long as you control this creature.",
            "whileSourceControlled",
        ),
        (
            "Instant",
            "Target creature gets -2/-0 until end of turn. It doesn't untap during its controller's next untap step.",
            "nextUntapStep",
        ),
    ] {
        let restrictions = untap_restrictions(type_line, oracle);
        assert_eq!(restrictions.len(), 1, "{oracle}");
        assert_eq!(restrictions[0]["duration"]["kind"], duration, "{oracle}");
        assert_eq!(
            restrictions[0]["permanent"]["kind"], "chosenTarget",
            "{oracle}: {restrictions:#?}"
        );
    }
}

/// Near misses: the negation agrees with a declared subject, and only the
/// permanent's controller's untap step is a supported window.
#[test]
fn a_permanent_subject_does_not_untap_near_misses() {
    for oracle in [
        "Tap target creature. That creature don't untap during its controller's next untap step.",
        "Tap target creature. It doesn't untap during your next untap step.",
    ] {
        let (status, _, _) = parse("Instant", oracle);
        assert_ne!(status, "canonical", "{oracle}");
    }
}
