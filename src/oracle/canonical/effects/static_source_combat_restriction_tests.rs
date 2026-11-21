use super::*;
use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities},
};

fn parse(text: &str, face_type_line: &str) -> StaticSourceCombatRestrictionAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.into(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    try_parse_static_source_combat_restriction(&AbilityInput {
        face_name: "Generic Restriction",
        face_type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn parsed(text: &str) -> Value {
    match parse(text, "Creature") {
        StaticSourceCombatRestrictionAttempt::Parsed(parsed) => parsed.rule,
        StaticSourceCombatRestrictionAttempt::Rejected(failure) => {
            panic!("{text}: rejected at {} ({failure:?})", failure.state)
        }
        StaticSourceCombatRestrictionAttempt::NotClaimed => panic!("{text}: not claimed"),
    }
}

fn rejected_at(text: &str) -> &'static str {
    match parse(text, "Creature") {
        StaticSourceCombatRestrictionAttempt::Rejected(failure) => failure.state,
        StaticSourceCombatRestrictionAttempt::Parsed(_) => panic!("{text}: unexpectedly parsed"),
        StaticSourceCombatRestrictionAttempt::NotClaimed => panic!("{text}: not claimed"),
    }
}

#[test]
fn independent_restrictions_keep_their_existing_runtime_markers() {
    for (text, marker) in [
        ("This creature can't block.", "cantBlock"),
        ("This creature can't attack.", "cantAttack"),
        ("This creature can\u{2019}t be blocked.", "cantBeBlocked"),
        (
            "This creature can't be blocked by more than one creature.",
            "maxOneBlocker",
        ),
    ] {
        let rule = parsed(text);
        assert_eq!(rule["modifiers"][0]["keyword"], marker, "{text}");
        assert_ne!(rule["modifiers"][0]["keyword"], "defender");
        assert!(rule.get("condition").is_none(), "{text}");
    }
}

#[test]
fn attack_requirement_and_compound_clauses_compose_independent_modifiers() {
    let rule = parsed("This creature attacks each combat if able.");
    assert_eq!(rule["modifiers"][0]["kind"], "attackEachCombatIfAble");
    assert_eq!(rule["modifiers"][0]["object"], json!({ "kind": "self" }));

    for text in [
        "This creature can't attack or block.",
        "This creature can't attack and can't block.",
    ] {
        let rule = parsed(text);
        let keywords = rule["modifiers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|modifier| modifier["keyword"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(keywords, ["cantAttack", "cantBlock"], "{text}");
    }
    // A previously unused combination: a block prohibition beside a blocked-by one.
    let rule = parsed("This creature can't block and can't be blocked.");
    assert_eq!(rule["modifiers"][0]["keyword"], "cantBlock");
    assert_eq!(rule["modifiers"][1]["keyword"], "cantBeBlocked");
}

#[test]
fn blocker_qualification_is_the_shared_object_query() {
    let rule = parsed("This creature can't be blocked by creatures with power 3 or greater.");
    let modifier = &rule["modifiers"][0];
    assert_eq!(modifier["kind"], "blockRestriction");
    assert_eq!(modifier["attackers"], json!({ "kind": "self" }));
    assert_eq!(modifier["blockers"]["kind"], "permanents");
    assert!(modifier["blockers"]["where"].to_string().contains("power"));

    // The same child serves another predicate with no new production code.
    for text in [
        "This creature can't be blocked by Walls.",
        "This creature can't be blocked by artifact creatures.",
        "This creature can't be blocked by creatures with flying.",
        "This creature can't be blocked by black and/or red creatures.",
    ] {
        let rule = parsed(text);
        assert_eq!(rule["modifiers"][0]["kind"], "blockRestriction", "{text}");
        assert_eq!(rule["modifiers"][0]["attackers"], json!({ "kind": "self" }));
    }
}

#[test]
fn except_by_prohibits_the_complement_inside_the_creature_domain() {
    let rule = parsed("This creature can't be blocked except by creatures with flying.");
    let blockers = &rule["modifiers"][0]["blockers"]["where"];
    assert_eq!(blockers["kind"], "and");
    assert_eq!(blockers["operands"][0], card_type("Creature"));
    assert_eq!(blockers["operands"][1]["kind"], "not");

    let walls = parsed("This creature can't be blocked except by Walls.");
    assert_eq!(
        walls["modifiers"][0]["blockers"]["where"]["operands"][1],
        not(subtype("Wall"))
    );
    let either = parsed("This creature can't be blocked except by creatures with flying or reach.");
    assert_eq!(
        either["modifiers"][0]["blockers"]["where"]["operands"][1]["kind"],
        "not"
    );
}

#[test]
fn block_prohibition_with_attacker_query_swaps_the_restriction_sides() {
    let rule = parsed("This creature can't block creatures with power 3 or greater.");
    let modifier = &rule["modifiers"][0];
    assert_eq!(modifier["kind"], "blockRestriction");
    assert_eq!(modifier["blockers"], json!({ "kind": "self" }));
    assert_eq!(modifier["attackers"]["kind"], "permanents");
    // The prefix sibling stays the unqualified marker.
    assert_eq!(
        parsed("This creature can't block.")["modifiers"][0]["keyword"],
        "cantBlock"
    );
}

#[test]
fn as_long_as_gates_the_whole_static_ability() {
    for text in [
        "This creature can't block as long as you control an Island.",
        "As long as you control an Island, this creature can't block.",
    ] {
        let rule = parsed(text);
        assert_eq!(rule["modifiers"][0]["keyword"], "cantBlock", "{text}");
        assert!(rule["condition"].is_object(), "{text}");
    }
    // The condition composes with a qualified blocker clause as well.
    let rule = parsed(
        "This creature can't be blocked by creatures with power 3 or greater as long as you control an Island.",
    );
    assert_eq!(rule["modifiers"][0]["kind"], "blockRestriction");
    assert!(rule["condition"].is_object());
}

#[test]
fn near_misses_fail_explicitly_or_stay_with_other_parents() {
    // Not a combat restriction at all: no family commitment.
    for text in [
        "You gain 1 life.",
        "If you control an Island, this creature can't block.",
    ] {
        assert!(
            matches!(
                parse(text, "Creature"),
                StaticSourceCombatRestrictionAttempt::NotClaimed
            ),
            "{text}"
        );
    }
    // Committed shapes the engine cannot represent keep a precise state.
    for (text, state) in [
        (
            // A duration ends the bare prohibition; a static frame owns none.
            "This creature can't block this turn.",
            "static.combat-restriction.complete",
        ),
        (
            "This creature can't block unless you control an Island.",
            "selector.predicate.start",
        ),
        (
            "This creature can't attack unless defending player controls an Island.",
            "static.combat-restriction.complete",
        ),
        (
            "This creature can't attack or block alone.",
            "static.combat-restriction.complete",
        ),
        (
            "This creature can't block. You gain 1 life.",
            "static.combat-restriction.complete",
        ),
        (
            "This creature attacks each combat if able unless you control an Island.",
            "static.combat-restriction.complete",
        ),
        (
            "This creature can't be blocked except by three or more creatures.",
            "selector.predicate.start",
        ),
        (
            "This creature can't be blocked as long as defending player controls an Island.",
            "condition.static.atom-kind",
        ),
    ] {
        assert_eq!(rejected_at(text), state, "{text}");
    }
    assert!(matches!(
        parse("This spell can't block.", "Instant"),
        StaticSourceCombatRestrictionAttempt::NotClaimed
    ));
}
