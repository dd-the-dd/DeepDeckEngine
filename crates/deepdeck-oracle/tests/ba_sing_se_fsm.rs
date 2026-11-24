use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "This land enters tapped unless you control a basic land.\n{T}: Add {G}.\n{2}{G}, {T}: Earthbend 2. Activate only as a sorcery. (Target land you control becomes a 0/0 creature with haste that's still a land. Put two +1/+1 counters on it. When it dies or is exiled, return it to the battlefield tapped.)";

fn parse(oracle: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Native Grammar Fixture".to_string(),
        type_line: "Land".to_string(),
        mana_cost: None,
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn controller() -> serde_json::Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

#[test]
fn exact_card_has_three_native_abilities_and_only_primitive_earthbend_effects() {
    let parsed = parse(EXACT_ORACLE);
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    assert_eq!(parsed.abilities.len(), 3);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.status == "canonical" && ability.recognizer == RecognizerProvenance::Native
    }));

    let entry = parsed.abilities[0].rule.as_ref().expect("entry rule");
    assert_eq!(entry["kind"], "replacementEffect");
    assert_eq!(entry["condition"]["kind"], "not");
    assert_eq!(
        entry["condition"]["operand"]["where"],
        json!({
            "kind": "and",
            "operands": [
                { "kind": "typeLineContains", "value": "Basic" },
                { "kind": "cardTypeContains", "value": "Land" },
            ],
        })
    );

    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["effects"][0]["kind"],
        "addMana"
    );

    let earthbend = parsed.abilities[2].rule.as_ref().expect("Earthbend rule");
    assert_eq!(earthbend["costs"][0]["manaCost"], "{2}{G}");
    assert_eq!(earthbend["activationCondition"]["kind"], "sorceryTiming");
    assert_eq!(
        earthbend["declaration"]["decisions"][0]["candidates"],
        json!({
            "kind": "permanents",
            "controller": controller(),
            "where": { "kind": "cardTypeContains", "value": "Land" },
        })
    );
    assert_eq!(
        earthbend["effects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|effect| effect["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "becomeCreature",
            "grantKeyword",
            "putCounters",
            "installDelayedZoneChangeTrigger",
            "emitAbilitySignal",
        ]
    );
    assert_eq!(
        earthbend["effects"][3]["effects"][0]["kind"],
        "moveTriggeringCard"
    );
    assert_eq!(earthbend["effects"][4]["abilityId"], "earthbend");

    let ir = earthbend.to_string();
    for forbidden in [
        "\"kind\":\"earthbend\"",
        "resolveTriggeredInstruction",
        "earthbent",
    ] {
        assert!(!ir.contains(forbidden), "forbidden IR: {forbidden}");
    }
}

#[test]
fn sibling_cost_quantity_and_entry_selector_reuse_the_same_automata() {
    let oracle = "This artifact enters tapped unless you control an artifact.\n{G}, {T}: Earthbend 3. Activate only as a sorcery. (Target land you control becomes a 0/0 creature with haste that's still a land. Put three +1/+1 counters on it. When it dies or is exiled, return it to the battlefield tapped.)";
    let parsed = parse(oracle);
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["effects"][2]["count"]["value"],
        3
    );
}

#[test]
fn variable_and_zero_earthbend_quantities_lower_through_the_numeric_contract() {
    let variable = parse(
        "{X}{G}, {T}: Earthbend X. Activate only as a sorcery. (Target land you control becomes a 0/0 creature with haste that's still a land. Put X +1/+1 counters on it. When it dies or is exiled, return it to the battlefield tapped.)",
    );
    assert_eq!(variable.status, "canonical", "{:#?}", variable.abilities);
    let variable_rule = variable.abilities[0].rule.as_ref().unwrap();
    assert_eq!(
        variable_rule["declaration"]["decisions"][0]["kind"],
        "chooseNumber"
    );
    assert_eq!(
        variable_rule["effects"][2]["count"]["kind"],
        "decisionResult"
    );

    let zero = parse("{G}, {T}: Earthbend 0. Activate only as a sorcery.");
    assert_eq!(zero.status, "canonical", "{:#?}", zero.abilities);
    assert_eq!(
        zero.abilities[0].rule.as_ref().unwrap()["effects"][2]["count"]["value"],
        0
    );

    let unbound = parse("{G}, {T}: Earthbend X. Activate only as a sorcery.");
    assert_eq!(unbound.status, "unsupported");
    assert_eq!(
        unbound.abilities[0].diagnostics[0].state.as_deref(),
        Some("earthbend.action.variable-binding")
    );
}

#[test]
fn claimed_near_misses_surface_local_machine_states() {
    for (oracle, automaton, state) in [
        (
            "This land enters tapped unless you control a.",
            "conditional-entry",
            "selector.predicate.start",
        ),
        (
            "{G}, {T}: Earthbend 3. Activate only as a sorcery. (Target land you control becomes a 0/0 creature with haste that's still a land. Put two +1/+1 counters on it. When it dies or is exiled, return it to the battlefield tapped.)",
            "earthbend-activation",
            "earthbend.reminder.quantity-match",
        ),
    ] {
        let parsed = parse(oracle);
        assert_eq!(parsed.status, "unsupported", "{oracle}");
        assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
        let diagnostic = &parsed.abilities[0].diagnostics[0];
        assert_eq!(diagnostic.automaton.as_deref(), Some(automaton));
        assert_eq!(diagnostic.state.as_deref(), Some(state));
    }
}

#[test]
fn another_printed_timing_composes_through_the_shared_restriction_reader() {
    // `Activate only as an instant` is an ordinary activation restriction
    // (CR 602.5d), read by the shared restriction reader after the shared
    // Earthbend action; it is no near miss of the sorcery-timed family.
    let parsed = parse("{G}, {T}: Earthbend 2. Activate only as an instant.");
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.diagnostics);
}

#[test]
fn adjacent_entry_and_other_bend_families_are_not_stolen() {
    let existing = parse("This land enters tapped unless you control two or fewer other lands.");
    assert_eq!(existing.status, "canonical");
    assert_eq!(
        existing.abilities[0].rule.as_ref().unwrap()["condition"]["operand"]["left"]["kind"],
        "countPermanents"
    );

    // Another bend keyword is rejected by the shared program, not claimed by
    // the earthbend family.
    let other_bend = parse("{G}, {T}: Waterbend 2. Activate only as a sorcery.");
    assert_eq!(other_bend.status, "unsupported");
    let ability = &other_bend.abilities[0];
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("contextual-effect-program")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("Waterbend"));
}
