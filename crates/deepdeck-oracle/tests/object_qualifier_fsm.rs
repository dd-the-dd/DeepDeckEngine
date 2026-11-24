//! Object qualifiers (C4): the status-predicate registry, relative clauses,
//! controller/owner relations and chosen values compose in the shared
//! object query and in conditions, never in an action-specific reader.
use deepdeck_oracle::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::{Value, json};

fn parse(name: &str, type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread")
        .join()
        .expect("parser")
}

fn single_rule(parsed: &OracleCardParseResult) -> &Value {
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.diagnostics);
    let [ability] = parsed.abilities.as_slice() else {
        panic!("one ability: {:#?}", parsed.abilities);
    };
    ability.rule.as_ref().expect("canonical rule")
}

fn rule(type_line: &str, text: &str) -> Value {
    single_rule(&parse("Probe", type_line, text)).clone()
}

#[test]
fn status_adjectives_come_from_one_registry_for_selectors_and_conditions() {
    // Kicked (CR 702.33d): a permanent's status from its kicked spell.
    let kicked = rule(
        "Creature \u{2014} Human",
        "When this creature enters, if it was kicked, draw a card.",
    );
    assert_eq!(
        kicked["condition"],
        json!({
            "kind": "objectMatchesFilter",
            "object": { "kind": "self" },
            "where": { "kind": "hasDesignation", "designation": "kicked" },
        })
    );
    // Equipped / enchanted / modified (CR 301.5, 303.4, 700.9) as a source
    // state, coordinated, and as selector adjectives.
    let either = rule(
        "Creature \u{2014} Human Knight",
        "As long as this creature is enchanted or equipped, this creature has flying.",
    );
    assert_eq!(
        either["condition"]["where"],
        json!({ "kind": "or", "operands": [{ "kind": "hasAttachedAura" }, { "kind": "isEquipped" }] })
    );
    let bounce = rule(
        "Instant",
        "Return target enchanted permanent to its owner's hand.",
    );
    assert_eq!(
        bounce["declaration"]["decisions"][0]["candidates"]["where"],
        json!({ "kind": "hasAttachedAura" })
    );
    // Sibling: an unused combination, the modified adjective counted.
    let count = rule(
        "Creature \u{2014} Elf",
        "{T}: You gain 1 life for each modified creature you control.",
    );
    assert_eq!(
        count["effects"][0]["amount"]["where"],
        json!({ "kind": "and", "operands": [{ "kind": "isModified" }, { "kind": "cardTypeContains", "value": "Creature" }] })
    );
    // Near misses: a word outside the registry, and a status list mixing
    // both connectors.
    for text in [
        "As long as this creature is shiny, this creature has flying.",
        "As long as this creature is enchanted or equipped and tapped, this creature has flying.",
    ] {
        assert_eq!(
            parse("Near Miss", "Creature \u{2014} Elf", text).status,
            "unsupported",
            "{text}"
        );
    }
}

#[test]
fn relative_clauses_fill_the_shared_object_query_slot() {
    let creature = json!({ "kind": "cardTypeContains", "value": "Creature" });
    let target_filter = |text: &str| {
        rule("Sorcery", text)["declaration"]["decisions"][0]["candidates"]["where"].clone()
    };
    // Past tense with `this turn`: the object's this-turn history.
    assert_eq!(
        target_filter("Destroy target creature that was dealt damage this turn."),
        json!({ "kind": "and", "operands": [creature, { "kind": "wasDealtDamageThisTurn" }] })
    );
    // Present possession: the shared counter qualifier.
    assert_eq!(
        target_filter("Destroy target creature that has a -1/-1 counter on it."),
        json!({ "kind": "and", "operands": [creature, { "kind": "hasCounter", "counter": "-1/-1" }] })
    );
    // Negated copula over the status registry.
    let wrath = rule("Sorcery", "Destroy all creatures that aren't enchanted.");
    assert_eq!(
        wrath["effects"][0]["permanent"]["where"],
        json!({ "kind": "and", "operands": [creature, { "kind": "not", "operand": { "kind": "hasAttachedAura" } }] })
    );
    // `didn't attack or enter`: the negation distributes over the verbs.
    let spared = rule(
        "Sorcery",
        "Put a +1/+1 counter on each creature you control that didn't attack or enter this turn.",
    );
    assert_eq!(
        spared["effects"][0]["permanent"]["where"]["operands"][1],
        json!({ "kind": "and", "operands": [
            { "kind": "not", "operand": { "kind": "attackedThisTurn" } },
            { "kind": "not", "operand": { "kind": "enteredThisTurn" } },
        ] })
    );
    // Sibling: the same history clause in a count, no new code.
    let count = rule(
        "Sorcery",
        "You gain 1 life for each creature that attacked this turn.",
    );
    assert_eq!(
        count["effects"][0]["amount"]["where"]["operands"][1],
        json!({ "kind": "attackedThisTurn" })
    );
    // Sibling: a cost reads the same slot (the former unsupported example).
    let cost = parse(
        "Sibling",
        "Sorcery",
        "As an additional cost to cast this spell, sacrifice a creature that dealt damage this turn.
Draw two cards.",
    );
    assert_eq!(cost.status, "canonical", "{:#?}", cost.diagnostics);
    let cost = cost.abilities[0].rule.as_ref().expect("canonical rule");
    assert!(cost.to_string().contains("dealtDamageThisTurn"), "{cost}");
    // Near misses: a past clause without `this turn`, and a stack relation
    // on an object that has no targets.
    for text in [
        "Destroy target creature that was dealt damage.",
        // A permanent has no targets: the stack relation is a spell's
        // (`Counter target spell that targets a creature` now reads it).
        "Destroy target creature that targets a creature.",
    ] {
        assert_eq!(
            parse("Near Miss", "Instant", text).status,
            "unsupported",
            "{text}"
        );
    }
}

#[test]
fn a_controller_relation_can_name_the_player_the_event_records() {
    let candidates = |type_line: &str, text: &str| {
        rule(type_line, text)["declaration"]["decisions"][0]["candidates"].clone()
    };
    // `that player controls` after an event that names one player.
    assert_eq!(
        candidates(
            "Creature \u{2014} Dinosaur",
            "Whenever this creature deals combat damage to a player, destroy target artifact that player controls.",
        )["controller"],
        json!({ "kind": "triggeringPlayer" })
    );
    // Sibling: `defending player controls` in an attack trigger, with a
    // qualified attacker, no new code.
    assert_eq!(
        candidates(
            "Creature \u{2014} Lizard",
            "Whenever a creature you control with a +1/+1 counter on it attacks, tap target creature defending player controls.",
        )["controller"],
        json!({ "kind": "triggeringPlayer" })
    );
    // Near misses: no event or prior binding names `that player`; a static
    // ability has no defending player.
    for (type_line, text) in [
        ("Sorcery", "Destroy target creature that player controls."),
        (
            "Enchantment",
            "Creatures defending player controls get -1/-0.",
        ),
    ] {
        assert_eq!(
            parse("Near Miss", type_line, text).status,
            "unsupported",
            "{text}"
        );
    }
}

#[test]
fn ownership_relations_and_distributive_target_lists() {
    // `target permanent you own` / `permanents you don't own` (CR 108.3):
    // the collection's ownership relation.
    let owned = rule("Artifact", "{T}: Destroy target permanent you own.");
    assert_eq!(
        owned["declaration"]["decisions"][0]["candidates"],
        json!({ "kind": "permanents", "ownership": "owned", "where": null })
    );
    let exiled = rule(
        "Sorcery",
        "Exile target creature you own, then return it to the battlefield under your control.",
    );
    assert_eq!(
        exiled["declaration"]["decisions"][0]["candidates"]["ownership"],
        "owned"
    );
    // `each of up to two other target creatures you control`: `each of`
    // distributes the action over the declared targets.
    let mentor = rule(
        "Creature \u{2014} Human",
        "When this creature enters, put a +1/+1 counter on each of up to two other target creatures you control.",
    );
    let decision = &mentor["declaration"]["decisions"][0];
    assert_eq!(
        (decision["minimum"].clone(), decision["maximum"].clone()),
        (json!(0), json!(2))
    );
    assert_eq!(mentor["effects"][0]["permanent"]["kind"], "chosenTargets");
    // Sibling: `each of two target creatures` with another action.
    let tapped = rule("Sorcery", "Tap each of two target creatures.");
    assert_eq!(tapped["declaration"]["decisions"][0]["maximum"], 2);
    // Near misses: two ownership relations on one collection; `each of
    // them` is not a target list.
    for text in [
        "Destroy target creature you own and you don't own.",
        "Put a +1/+1 counter on each of them.",
    ] {
        assert_eq!(
            parse("Near Miss", "Sorcery", text).status,
            "unsupported",
            "{text}"
        );
    }
}
