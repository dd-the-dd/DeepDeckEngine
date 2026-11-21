//! Shared characteristic concepts: color quantity (CR 105.2) and historic
//! (CR 700.6) object predicates, copula type changes in the static frame
//! (CR 205.1, 305.7) and activation prohibitions over object collections
//! (CR 602.5). Each concept has a real card, an unused sibling combination
//! and a near miss; runtime behavior is proved in the engine runtime tests.
use serde_json::{Value, json};

use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(name: &str, type_line: &str, text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
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

/// The single ability of a one-line text is canonical; returns its rule.
fn rule(name: &str, type_line: &str, text: &str) -> Value {
    let parsed = parse(name, type_line, text);
    assert_eq!(parsed.status, "canonical", "{text}: {parsed:#?}");
    let [ability] = parsed.abilities.as_slice() else {
        panic!("one ability: {parsed:#?}");
    };
    ability.rule.clone().expect("canonical rule")
}

fn rejected(type_line: &str, text: &str) {
    let parsed = parse("Probe", type_line, text);
    let ability = parsed
        .abilities
        .iter()
        .find(|ability| ability.status != "empty")
        .expect("one ability");
    assert_ne!(ability.status, "canonical", "{text}: {:#?}", ability.rule);
}

fn color_count(operator: &str, value: i64) -> Value {
    json!({
        "kind": "compare",
        "left": { "kind": "colorCountOf", "object": { "kind": "candidate" } },
        "operator": operator,
        "right": { "kind": "integer", "value": value },
    })
}

fn static_modifiers(rule: &Value) -> &Value {
    assert_eq!(rule["kind"], "staticAbility", "{rule:#}");
    &rule["modifiers"]
}

#[test]
fn color_quantity_is_one_shared_predicate_atom() {
    let tome = rule(
        "Tome of the Guildpact",
        "Artifact",
        "Whenever you cast a multicolored spell, draw a card.",
    );
    assert_eq!(tome["event"]["where"], color_count(">=", 2));

    // Unused siblings: `monocolored` in a target and `colorless` in a count.
    let destroy = rule("Probe", "Instant", "Destroy target monocolored creature.");
    let candidates = destroy["declaration"]["decisions"][0]["candidates"]["where"].to_string();
    assert!(
        candidates.contains(&color_count("==", 1).to_string()),
        "{candidates}"
    );
    let tomb = rule(
        "Tomb of the Spirit Dragon",
        "Land",
        "{2}, {T}: You gain 1 life for each colorless creature you control.",
    );
    assert_eq!(
        tomb["effects"][0]["amount"]["where"]["operands"][0],
        color_count("==", 0)
    );

    // Near miss: an unregistered color-count adjective.
    rejected("Instant", "Destroy target bicolored creature.");
}

#[test]
fn historic_is_a_characteristic_batch_of_shared_atoms() {
    let excavator = rule(
        "Diligent Excavator",
        "Creature - Human Artificer",
        "Whenever you cast a historic spell, target player mills two cards. (Artifacts, legendaries, and Sagas are historic.)",
    );
    assert_eq!(
        excavator["event"]["where"],
        json!({ "kind": "or", "operands": [
            { "kind": "cardTypeContains", "value": "Artifact" },
            { "kind": "isLegendary" },
            { "kind": "subtypeContains", "value": "Saga" },
        ]})
    );

    // Unused sibling: the negated batch in a targeted zone change, whose
    // typed reminder must describe the same batch.
    let destroy = rule(
        "Probe",
        "Instant",
        "Destroy target nonhistoric permanent. (Artifacts, legendaries, and Sagas are historic.)",
    );
    assert!(
        destroy.to_string().contains("\"isLegendary\""),
        "{destroy:#}"
    );

    // Near miss: a similar adjective that is not the registered batch word.
    rejected("Instant", "Destroy target historical permanent.");
}

#[test]
fn copula_type_changes_compose_subject_number_types_and_retention() {
    let nonbasic_lands = json!({
        "kind": "permanents",
        "where": { "kind": "and", "operands": [
            { "kind": "not", "operand": { "kind": "typeLineContains", "value": "Basic" } },
            { "kind": "cardTypeContains", "value": "Land" },
        ]},
    });
    let magus = rule(
        "Magus of the Moon",
        "Creature - Human Wizard",
        "Nonbasic lands are Mountains.",
    );
    assert_eq!(
        static_modifiers(&magus),
        &json!([{ "kind": "setLandSubtype", "objects": nonbasic_lands, "subtype": "Mountain" }])
    );

    let yavimaya = rule(
        "Yavimaya, Cradle of Growth",
        "Legendary Land",
        "Each land is a Forest in addition to its other land types.",
    );
    assert_eq!(static_modifiers(&yavimaya)[0]["kind"], "addSubtype");
    assert_eq!(static_modifiers(&yavimaya)[0]["subtype"], "Forest");

    let dryad = rule(
        "Dryad of the Ilysian Grove",
        "Enchantment Creature - Nymph Dryad",
        "Lands you control are every basic land type in addition to their other types.",
    );
    assert_eq!(static_modifiers(&dryad)[0]["kind"], "addAllBasicLandTypes");
    assert_eq!(
        static_modifiers(&dryad)[0]["objects"]["controller"],
        json!({ "kind": "controllerOf", "object": { "kind": "self" } })
    );

    let toph = rule(
        "Toph, the First Metalbender",
        "Legendary Creature - Human Warrior Ally",
        "Nontoken artifacts you control are lands in addition to their other types. (They don't gain the ability to {T} for mana.)",
    );
    assert_eq!(static_modifiers(&toph)[0]["kind"], "addCardType");
    assert_eq!(static_modifiers(&toph)[0]["cardType"], "Land");

    // Mixed predicate list with a serial comma and an added supertype.
    let wings = rule(
        "On Serra's Wings",
        "Legendary Enchantment - Aura",
        "Enchanted creature is legendary, gets +1/+1, and has flying, vigilance, and lifelink.",
    );
    let kinds = static_modifiers(&wings)
        .as_array()
        .expect("modifiers")
        .iter()
        .map(|modifier| modifier["kind"].as_str().expect("kind"))
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            "addSupertype",
            "modifyPowerToughness",
            "grantKeyword",
            "grantKeyword",
            "grantKeyword"
        ]
    );

    // Unused sibling: an Aura's attached land proves the land-only subject
    // of a replacing land-subtype change.
    let presence = rule("Probe", "Enchantment - Aura", "Enchanted land is a Swamp.");
    assert_eq!(
        static_modifiers(&presence),
        &json!([{
            "kind": "setLandSubtype",
            "objects": { "kind": "attachedPermanent", "attachment": { "kind": "self" } },
            "subtype": "Swamp",
        }])
    );

    // Near misses: a replacing land subtype on non-lands, copula agreement,
    // and a replacing card-type change without a battlefield primitive.
    rejected("Enchantment", "Nontoken creatures are Mountains.");
    rejected("Enchantment", "Lands you control is a Forest.");
    rejected("Enchantment", "Creatures you control are artifacts.");
}

#[test]
fn activation_prohibition_reads_shared_permanent_collections() {
    let null_rod = rule(
        "Null Rod",
        "Artifact",
        "Activated abilities of artifacts can't be activated.",
    );
    assert_eq!(
        static_modifiers(&null_rod),
        &json!([{
            "kind": "prohibitActivation",
            "abilities": {
                "kind": "activatedAbilities",
                "sourceWhere": { "kind": "cardTypeContains", "value": "Artifact" },
                "where": null,
            },
        }])
    );

    // Unused sibling: a color-quantity predicate inside the source set, with
    // the mana-ability exception.
    let probe = rule(
        "Probe",
        "Enchantment",
        "Activated abilities of colorless creatures can't be activated unless they're mana abilities.",
    );
    let abilities = &static_modifiers(&probe)[0]["abilities"];
    assert_eq!(
        abilities["sourceWhere"]["operands"][0],
        color_count("==", 0)
    );
    assert_eq!(
        abilities["where"],
        json!({ "kind": "not", "operand": { "kind": "isManaAbility" } })
    );

    // Near miss: the runtime source filter has no controller relation slot.
    rejected(
        "Enchantment",
        "Activated abilities of creatures your opponents control can't be activated.",
    );
}

#[test]
fn batch_reminders_and_searched_card_adjectives_reuse_the_shared_atoms() {
    let familiar = rule(
        "Jhoira's Familiar",
        "Artifact Creature - Bird",
        "Historic spells you cast cost {1} less to cast. (Artifacts, legendaries, and Sagas are historic.)",
    );
    assert_eq!(static_modifiers(&familiar)[0]["kind"], "reduceCastingCost");
    assert!(
        static_modifiers(&familiar)[0]["where"]
            .to_string()
            .contains("Saga")
    );

    // Unused siblings: color quantity and a negated supertype as searched-card
    // adjectives, sharing the object-predicate atoms.
    let eye = rule(
        "Eye of Ugin",
        "Legendary Land",
        "{7}, {T}: Search your library for a colorless creature card, reveal it, put it into your hand, then shuffle.",
    );
    assert_eq!(
        eye["effects"][0]["candidates"]["where"]["operands"][0],
        color_count("==", 0)
    );
    let grave = rule(
        "Unmarked Grave",
        "Sorcery",
        "Search your library for a nonlegendary card, put that card into your graveyard, then shuffle.",
    );
    assert_eq!(
        grave["effects"][0]["candidates"]["where"],
        json!({ "kind": "not", "operand": { "kind": "isLegendary" } })
    );

    // Near miss: the reminder must define a batch the sentence uses.
    rejected(
        "Enchantment",
        "Creature spells you cast cost {1} less to cast. (Artifacts, legendaries, and Sagas are historic.)",
    );
}
