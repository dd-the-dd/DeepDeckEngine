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
    let parsed = parse(
        "Creature \u{2014} Zombie",
        "Instant and sorcery spells you control have deathtouch.",
    );
    if let Some(rule) = parsed
        .abilities
        .first()
        .and_then(|ability| ability.rule.as_ref())
    {
        assert!(!rule.to_string().contains("\"permanents\""), "{rule:#}");
    }
}

#[test]
fn other_is_refused_by_parents_that_cannot_lower_a_source_exclusion() {
    // The greatest-among aggregate has no source exclusion: the explicit
    // failure is kept instead of silently counting the source.
    let parsed = parse(
        "Creature \u{2014} Elf",
        "When this creature enters, you gain life equal to the greatest toughness among other creatures you control.",
    );
    assert_ne!(parsed.status, "canonical", "{:#?}", parsed.abilities);
}
