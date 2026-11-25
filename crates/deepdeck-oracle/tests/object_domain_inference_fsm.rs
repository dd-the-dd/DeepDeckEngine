//! Object domain inference (roadmap #1, CR 109.2a and 205.3d): a type or
//! subtype head without `card` or `spell` names a permanent in every parent
//! that reads the shared object query, and `other` is one field of the
//! collection frame instead of a word each parent scans for.
use deepdeck_oracle::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::{Value, json};

fn parse(type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: "Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{G}".to_string()),
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

fn rule(type_line: &str, text: &str) -> Value {
    let parsed = parse(type_line, text);
    assert_eq!(
        parsed.status, "canonical",
        "{text}: {:#?}",
        parsed.diagnostics
    );
    let [ability] = parsed.abilities.as_slice() else {
        panic!("one ability: {:#?}", parsed.abilities);
    };
    ability.rule.clone().expect("canonical rule")
}

fn subtype(value: &str) -> Value {
    json!({ "kind": "subtypeContains", "value": value })
}

/// Every object filter of the rule, wherever its parent placed it.
fn filters(rule: &Value) -> Vec<Value> {
    let mut found = Vec::new();
    fn walk(value: &Value, found: &mut Vec<Value>) {
        match value {
            Value::Object(map) => {
                if let Some(filter) = map.get("where") {
                    found.push(filter.clone());
                }
                map.values().for_each(|child| walk(child, found));
            }
            Value::Array(items) => items.iter().for_each(|child| walk(child, found)),
            _ => {}
        }
    }
    walk(rule, &mut found);
    found
}

#[test]
fn a_subtype_head_without_card_or_spell_names_permanents_in_every_parent() {
    // Static keyword grant (Goblin Warchief).
    let haste = rule(
        "Creature \u{2014} Goblin Warrior",
        "Goblins you control have haste.",
    );
    assert!(filters(&haste).contains(&subtype("Goblin")), "{haste:#}");
    // Static continuous change with `other` read by the collection frame
    // (Imperious Perfect's lord ability).
    let lord = rule(
        "Creature \u{2014} Elf Warrior",
        "Other Elves you control get +1/+1.",
    );
    assert!(filters(&lord).contains(&subtype("Elf")), "{lord:#}");
    assert!(
        lord.to_string().contains("\"excludeSource\":true"),
        "{lord:#}"
    );
    // Counter placement on a collection (Indulgent Aristocrat).
    let counters = rule(
        "Creature \u{2014} Vampire",
        "{2}, Sacrifice a creature: Put a +1/+1 counter on each Vampire you control.",
    );
    assert!(
        filters(&counters).contains(&subtype("Vampire")),
        "{counters:#}"
    );
    // A targeted permanent (CR 205.3d: a Wall is a creature permanent).
    let destroy = rule("Instant", "Destroy target Wall.");
    assert!(filters(&destroy).contains(&subtype("Wall")), "{destroy:#}");
}

#[test]
fn a_previously_unused_count_combination_needs_no_new_production() {
    // `for each Equipment you control` was never composed with a damage
    // amount by a supported card; the shared count child supplies it.
    let damage = rule(
        "Creature \u{2014} Human Soldier",
        "{T}: This creature deals 1 damage to any target for each Equipment you control.",
    );
    let text = damage.to_string();
    assert!(text.contains("\"subtypeContains\""), "{damage:#}");
    assert!(text.contains("\"Equipment\""), "{damage:#}");
    // A bare subtype count through the same frame, with `other`.
    let gain = rule(
        "Creature \u{2014} Elf Druid",
        "{T}: You gain 1 life for each other Elf you control.",
    );
    assert!(
        gain.to_string().contains("\"excludeSource\":true"),
        "{gain:#}"
    );
}

#[test]
fn explicit_card_and_spell_nouns_keep_their_own_domain() {
    // `Elf card` stays a card query (CR 109.2a near miss).
    let search = rule(
        "Sorcery",
        "Search your library for an Elf card, reveal it, put it into your hand, then shuffle.",
    );
    let text = search.to_string();
    assert!(!text.contains("\"permanents\""), "{search:#}");
    // `spells you control` stays a stack-object collection: it is never
    // lowered as a battlefield permanent set.
    let spells = rule(
        "Creature \u{2014} Zombie",
        "Instant and sorcery spells you control have deathtouch.",
    );
    // The spell-collection keyword grant (`grantKeywordToSpells`): no
    // permanent reading is produced.
    let text = spells.to_string();
    assert!(text.contains("\"grantKeywordToSpells\""), "{spells:#}");
    assert!(!text.contains("\"permanents\""), "{spells:#}");
    // `Goblin permanent card` is a card, never a Goblin permanent.
    let lackey = rule(
        "Creature \u{2014} Goblin",
        "Whenever this creature deals damage to a player, you may put a Goblin permanent card from your hand onto the battlefield.",
    );
    assert_eq!(
        lackey["effects"][0]["candidates"]["kind"], "cards",
        "{lackey:#}"
    );
    // `a Goblin source` is any object (CR 109.2): no permanent domain.
    let prevention = rule(
        "Enchantment",
        "If a Goblin source would deal damage to you, prevent that damage.",
    );
    assert_eq!(
        prevention["modifiers"][0]["event"]["source"],
        json!({ "kind": "objects", "where": subtype("Goblin") })
    );
}

#[test]
fn damage_to_a_bare_subtype_collection_keeps_its_damageable_type_check() {
    // A Goblin may be a noncreature Kindred permanent, which cannot be dealt
    // damage (CR 120.1): the damage parent keeps requiring a damageable type.
    let parsed = parse("Sorcery", "Probe deals 2 damage to each Goblin.");
    assert_ne!(parsed.status, "canonical");
    assert!(
        parsed.diagnostics.iter().any(|diagnostic| {
            diagnostic.state.as_deref() == Some("effect.damage.collection.validation")
        }),
        "{:#?}",
        parsed.diagnostics
    );
    let creatures = rule("Sorcery", "Probe deals 2 damage to each Goblin creature.");
    assert!(
        filters(&creatures)
            .iter()
            .any(|filter| filter.to_string().contains("Goblin"))
    );
}

#[test]
fn a_noun_with_a_control_relation_names_permanents_and_a_bare_one_does_not() {
    for (text, atom) in [
        ("Tokens you control have haste.", "isToken"),
        ("Commanders you control have hexproof.", "isCommander"),
    ] {
        let grant = rule("Creature \u{2014} Human", text);
        assert_eq!(
            grant["modifiers"][0]["objects"]["where"],
            json!({ "kind": atom }),
            "{text}"
        );
        assert_eq!(
            grant["modifiers"][0]["objects"]["kind"], "permanents",
            "{text}"
        );
    }
    // The same relation in a count condition (an unused combination).
    let count = rule(
        "Sorcery",
        "If you control three or more tokens, draw a card.",
    );
    assert_eq!(
        count["effects"][0]["condition"]["left"]["where"],
        json!({ "kind": "isToken" })
    );
    // Near miss: without a control relation `Commanders` is not anchored.
    assert_ne!(
        parse("Creature \u{2014} Human", "Commanders have hexproof.").status,
        "canonical"
    );
}

#[test]
fn other_names_the_source_only_when_the_source_is_the_affected_object() {
    // The source is the modified object: `other` excludes it.
    let lord = rule(
        "Creature \u{2014} Elf",
        "This creature gets +1/+1 for each other Elf you control.",
    );
    assert_eq!(
        lord["modifiers"][0]["power"]["excludeSource"], true,
        "{lord:#}"
    );
    let rabble = rule(
        "Creature \u{2014} Goblin",
        "Whenever this creature attacks, it gets +1/+0 until end of turn for each other attacking Goblin.",
    );
    assert_eq!(
        rabble["effects"][0]["power"]["excludeSource"], true,
        "{rabble:#}"
    );
    // Each modified object is the antecedent (Nephilim Epochal, Heroes'
    // Podium, Bravado): refused until the affected object is bound.
    for (type_line, text) in [
        (
            "Creature \u{2014} Nephilim",
            "Nephilim you control get +1/+1 for each other Nephilim you control.",
        ),
        (
            "Legendary Artifact",
            "Each legendary creature you control gets +1/+1 for each other legendary creature you control.",
        ),
        (
            "Enchantment \u{2014} Aura",
            "Enchant creature\nEnchanted creature gets +1/+1 for each other creature you control.",
        ),
    ] {
        let parsed = parse(type_line, text);
        assert_ne!(parsed.status, "canonical", "{text}");
        assert!(
            parsed.diagnostics.iter().any(|diagnostic| {
                diagnostic.state.as_deref() == Some("static.continuous.for-each.other-antecedent")
            }),
            "{text}: {:#?}",
            parsed.diagnostics
        );
    }
}

#[test]
fn other_is_lowered_by_the_shared_aggregate_as_its_set_exclusion() {
    // The shared aggregate (`numeric_aggregate`) reads `other` as the source
    // exclusion of its set: the source is never silently counted.
    let rule = rule(
        "Creature \u{2014} Elf",
        "When this creature enters, you gain life equal to the greatest toughness among other creatures you control.",
    );
    let serialized = rule.to_string();
    assert!(
        serialized.contains("\"aggregateObjects\"") && serialized.contains("\"excludeSource\":true"),
        "{rule:#}"
    );
}

#[test]
fn coordinators_the_query_returns_are_refused_or_read_by_their_parent() {
    // `and player` left by the permanent query: the target-list parent owns
    // the refusal.
    let parsed = parse(
        "Instant",
        "Target creature and player gets +2/+2 until end of turn.",
    );
    assert_ne!(parsed.status, "canonical");
    assert_eq!(
        parsed.diagnostics[0].state.as_deref(),
        Some("target-list.complete"),
        "{:#?}",
        parsed.diagnostics
    );
    assert_eq!(parsed.diagnostics[0].found.as_deref(), Some("and"));
    // `or player` is a branch of the target list (CR 115.1): the list reads
    // one creature-or-player target, and the verb a player cannot perform is
    // refused for that subject.
    let parsed = parse(
        "Instant",
        "Target creature or player gets +2/+2 until end of turn.",
    );
    assert_ne!(parsed.status, "canonical");
    assert_eq!(
        parsed.diagnostics[0].state.as_deref(),
        Some("clause.verb.unsupported"),
        "{:#?}",
        parsed.diagnostics
    );
    // A parent clause after the condition's query: the condition counts
    // artifacts only; `creatures or planeswalkers` is the effect's subject.
    let pump = rule(
        "Sorcery",
        "If you control an artifact, creatures or planeswalkers you control get +1/+1 until end of turn.",
    );
    let condition = &pump["effects"][0]["condition"]["left"]["where"];
    assert_eq!(
        condition,
        &json!({ "kind": "cardTypeContains", "value": "Artifact" })
    );
    assert_eq!(
        pump["effects"][0]["then"][0]["object"]["where"]["kind"],
        "or"
    );
}
