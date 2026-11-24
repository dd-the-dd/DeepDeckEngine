use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities},
};
use serde_json::{Value, json};

use crate::oracle::canonical::dispatch::{CanonicalParseOutcome, parse_canonical_rule};

fn parse(text: &str) -> Result<Value, String> {
    parse_as(text, "Instant", "spellAbility")
}

fn parse_as(text: &str, face_type_line: &str, ability_kind: &str) -> Result<Value, String> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    let input = AbilityInput {
        face_name: "Generic Modifier",
        face_type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };
    match parse_canonical_rule(&input, ability_kind) {
        CanonicalParseOutcome::Parsed { draft, .. } => Ok(draft.rule),
        CanonicalParseOutcome::Failed { failure, .. }
        | CanonicalParseOutcome::FailedWithCandidateEvidence { failure, .. } => {
            Err(format!("{}: {}", failure.state, failure.expected))
        }
        CanonicalParseOutcome::Unsupported => Err("unsupported".to_string()),
    }
}

fn subtype_change(rule: &Value, index: usize) -> &Value {
    let effect = &rule["effects"][index];
    assert_eq!(effect["kind"], "modifySubtypeSet", "{effect}");
    effect
}

/// A real card shape: a subject ellipsis after `and`, a preposed duration
/// shared by both verbs, and the replacement meaning of a bare subtype.
#[test]
fn counter_untap_stats_and_subtype_replacement_share_one_preposed_duration() {
    let rule = parse(
        "Put a +1/+1 counter on target creature you control. Untap it. Until end of turn, it gets +1/+1 and becomes a Doctor. (It loses all other creature types.)",
    )
    .expect("stats and replacement subtype compose");
    let effects = rule["effects"].as_array().expect("effects");
    assert_eq!(effects.len(), 4, "{effects:?}");
    assert_eq!(effects[0]["kind"], "putCounters");
    assert_eq!(effects[2]["kind"], "modifyPowerToughness");
    let change = subtype_change(&rule, 3);
    assert_eq!(change["mode"], "replace");
    assert_eq!(change["subtypes"], json!(["Doctor"]));
    assert_eq!(
        change["duration"],
        json!({ "kind": "untilEndOfCurrentTurn" })
    );
    assert_eq!(change["object"], rule["effects"][2]["object"]);
}

/// A previously unused combination: `in addition to its other types` adds
/// instead of replacing, several subtypes, a postposed duration, and a keyword
/// list attached to the shared subject through verb coordination.
#[test]
fn additive_subtypes_and_keyword_grant_share_the_subject() {
    let rule = parse(
        "Until end of turn, target creature you control becomes an Avatar in addition to its other types and gains flying, first strike, and lifelink.",
    )
    .expect("additive subtype with keyword grant");
    let change = subtype_change(&rule, 0);
    assert_eq!(change["mode"], "add");
    assert_eq!(change["subtypes"], json!(["Avatar"]));
    assert_eq!(
        change["duration"],
        json!({ "kind": "untilEndOfCurrentTurn" })
    );
    assert_eq!(rule["effects"][1]["kind"], "grantKeyword");
    assert_eq!(rule["effects"][1]["object"], change["object"]);

    let rule = parse("Until end of turn, target creature becomes a Zombie Wizard.")
        .expect("replacement with two subtypes");
    let change = subtype_change(&rule, 0);
    assert_eq!(change["mode"], "replace");
    assert_eq!(change["subtypes"], json!(["Zombie", "Wizard"]));
}

#[test]
fn preposed_and_postposed_durations_lower_identically() {
    assert_eq!(
        parse("Until end of turn, target creature becomes a Doctor.").unwrap(),
        parse("Target creature becomes a Doctor until end of turn.").unwrap(),
    );
}

/// CR 205.1a: without a retention clause a card-type list replaces the
/// object's card types; `in addition to its other types` adds them (205.1b).
#[test]
fn card_type_list_replaces_unless_a_retention_clause_adds_it() {
    let rule = parse(
        "Until end of turn, target creature becomes an artifact in addition to its other types and gains indestructible.",
    )
    .expect("additive card type");
    let effect = &rule["effects"][0];
    assert_eq!(effect["kind"], "addCardType");
    assert_eq!(effect["cardType"], "Artifact");
    assert_eq!(effect["retainExistingTypes"], true);
    assert_eq!(rule["effects"][1]["kind"], "grantKeyword");

    let rule = parse("Until end of turn, target creature becomes an artifact.")
        .expect("replacing card types");
    assert_eq!(
        rule["effects"][0],
        json!({
            "kind": "setCardTypes",
            "object": { "kind": "chosenTarget", "id": "targetPermanent" },
            "cardTypes": ["Artifact"],
            "duration": { "kind": "untilEndOfCurrentTurn" },
        })
    );
}

fn kinds(rule: &Value) -> Vec<&str> {
    rule["effects"]
        .as_array()
        .expect("effects")
        .iter()
        .map(|effect| effect["kind"].as_str().expect("kind"))
        .collect()
}

/// Real card (Skarrg Guildmage): a targeted land animation whose separate
/// `It's still a land.` sentence retains every prior type and subtype, so the
/// new creature subtype is added (CR 205.1b), and base P/T is layer 7b.
#[test]
fn still_a_land_sentence_retains_prior_types_of_a_targeted_animation() {
    let rule = parse_as(
        "{1}{R}{G}: Target land you control becomes a 4/4 Elemental creature until end of turn. It's still a land.",
        "Creature",
        "activatedAbility",
    )
    .expect("targeted land animation");
    let land = json!({ "kind": "chosenTarget", "id": "targetPermanent" });
    let until_end_of_turn = json!({ "kind": "untilEndOfCurrentTurn" });
    assert_eq!(
        rule["effects"],
        json!([
            {
                "kind": "addCardType", "object": land, "cardType": "Creature",
                "retainExistingTypes": true, "duration": until_end_of_turn,
            },
            {
                "kind": "modifySubtypeSet", "object": land, "subtypeClass": "creature",
                "mode": "add", "subtypes": ["Elemental"], "duration": until_end_of_turn,
            },
            {
                "kind": "setBasePowerToughness", "object": land,
                "power": { "kind": "integer", "value": 4 },
                "toughness": { "kind": "integer", "value": 4 },
                "duration": until_end_of_turn,
            },
        ])
    );
}

/// Real card (Iron Suitcase): `[creature type] artifact creature` keeps the
/// prior card types but replaces creature types (CR 205.1b); the `with`
/// keyword list is a layer-6 grant on the same source.
#[test]
fn artifact_creature_phrase_adds_types_and_replaces_creature_types() {
    let rule = parse_as(
        "{3}: This artifact becomes a 3/3 Construct artifact creature with flying until end of turn.",
        "Artifact",
        "activatedAbility",
    )
    .expect("self artifact animation");
    assert_eq!(
        kinds(&rule),
        [
            "addCardType",
            "addCardType",
            "modifySubtypeSet",
            "grantKeyword",
            "setBasePowerToughness"
        ]
    );
    assert!(
        rule["effects"]
            .as_array()
            .unwrap()
            .iter()
            .all(|effect| effect["object"] == json!({ "kind": "self" }))
    );
    assert_eq!(rule["effects"][2]["mode"], "replace");
    assert_eq!(rule["effects"][3]["keyword"], "flying");
}

/// Previously unused combination of supported children: a preposed duration,
/// a color set, `that's still a land`, two keywords and `all creature types`
/// in one `with` list, on a target instead of the source.
#[test]
fn colors_relative_retention_and_with_list_compose_on_a_target() {
    let rule = parse(
        "Until end of turn, target land becomes a 2/2 red and green Elemental creature with haste, trample, and all creature types that's still a land.",
    )
    .expect("composed targeted animation");
    assert_eq!(
        kinds(&rule),
        [
            "addCardType",
            "modifySubtypeSet",
            "modifySubtypeSet",
            "setColors",
            "grantKeyword",
            "grantKeyword",
            "setBasePowerToughness"
        ]
    );
    assert_eq!(rule["effects"][1]["mode"], "add");
    assert_eq!(rule["effects"][2]["mode"], "all");
    assert_eq!(rule["effects"][3]["colors"], json!(["R", "G"]));
}

/// Real card (Serpentine Ambush) and an enchantment animation: without a
/// retention clause the card types are set (CR 205.1a); a color and creature
/// subtype with `base power and toughness` need no card type at all.
#[test]
fn replacement_animation_and_base_power_toughness_tail() {
    let rule = parse(
        "Until end of turn, target creature becomes a blue Serpent with base power and toughness 5/5.",
    )
    .expect("Serpentine Ambush");
    assert_eq!(
        kinds(&rule),
        ["modifySubtypeSet", "setColors", "setBasePowerToughness"]
    );
    assert_eq!(rule["effects"][0]["mode"], "replace");
    assert_eq!(rule["effects"][1]["colors"], json!(["U"]));

    let rule = parse_as(
        "{G}{G}: This enchantment becomes a 4/3 Centaur creature until end of turn.",
        "Enchantment",
        "activatedAbility",
    )
    .expect("card types are set without retention");
    assert_eq!(rule["effects"][0]["kind"], "setCardTypes");
    assert_eq!(rule["effects"][0]["cardTypes"], json!(["Creature"]));
    assert_eq!(rule["effects"][1]["mode"], "replace");
}

/// A bare color is a characteristic of the same `becomes` frame: it sets
/// the color (CR 105.3, layer 5) and leaves the types untouched.
#[test]
fn a_bare_color_is_a_characteristic_of_the_becomes_frame() {
    let rule = parse("Until end of turn, target creature becomes blue.").expect("color change");
    assert_eq!(rule["effects"].as_array().unwrap().len(), 1);
    assert_eq!(rule["effects"][0]["kind"], "setColors");
    assert_eq!(rule["effects"][0]["colors"], json!(["U"]));
    assert_eq!(
        rule["effects"][0]["duration"]["kind"],
        "untilEndOfCurrentTurn"
    );
    // A land subtype names the land subtype class, never the creature one.
    let rule = parse("Until end of turn, target land becomes a Forest.").expect("land subtype");
    assert_eq!(rule["effects"][0]["kind"], "modifySubtypeSet");
    assert_eq!(rule["effects"][0]["subtypeClass"], "land");
    // Coordinated verbs share one subject and one trailing duration, as a
    // coordinated `gets +N/+N and gains K until end of turn` does.
    let rule = parse("Target creature becomes a Doctor and gains flying until end of turn.")
        .expect("coordinated characteristic chain");
    assert_eq!(
        rule["effects"][0]["duration"], rule["effects"][1]["duration"],
        "{rule}"
    );
}

/// Near misses stay explicit failures instead of being approximated.
#[test]
fn unsupported_type_change_shapes_fail_explicitly() {
    for text in [
        // Two durations for one continuous effect.
        "Until end of turn, target creature becomes a Doctor until your next turn.",
        // A duration that the runtime cannot end.
        "Until your next turn, target creature becomes a Doctor.",
        // Elision needs a preceding permanent subject.
        "Draw a card and becomes a Doctor.",
        // Only a creature has power and toughness (CR 208.1).
        "Until end of turn, target land becomes a 3/3 Elemental.",
        // Additive colors have no color-set primitive here (CR 105.3).
        "Target creature becomes a black Demon in addition to its other colors and types.",
        // A characteristic-defined power/toughness is not a literal base P/T.
        "Until end of turn, target artifact becomes an artifact creature with power and toughness each equal to its mana value.",
        // The retained type must be a prior type, not a new one.
        "Until end of turn, target land becomes a 3/3 creature. It's still a creature.",
        // Two base power/toughness values for one effect.
        "Until end of turn, target land becomes a 3/3 creature with base power and toughness 1/1.",
    ] {
        assert!(parse(text).is_err(), "{text}");
    }
}

/// Real cards (Humble, Turn to Frog): `loses all abilities`, `has base power
/// and toughness` and `becomes` are verbs of the same characteristic frame,
/// so a coordinated chain shares its subject and preposed duration.
#[test]
fn ability_loss_and_base_power_toughness_verbs_share_the_chain_subject() {
    let rule = parse(
        "Until end of turn, target creature loses all abilities and has base power and toughness 0/1.",
    )
    .expect("Humble");
    assert_eq!(
        kinds(&rule),
        ["installLoseAllAbilities", "setBasePowerToughness"]
    );
    assert_eq!(rule["effects"][0]["object"], rule["effects"][1]["object"]);

    let rule = parse(
        "Until end of turn, target creature loses all abilities and becomes a blue Frog with base power and toughness 1/1.",
    )
    .expect("Turn to Frog");
    assert_eq!(
        kinds(&rule),
        [
            "installLoseAllAbilities",
            "modifySubtypeSet",
            "setColors",
            "setBasePowerToughness"
        ]
    );
}

/// Previously unused combination: a battlefield collection subject. The
/// affected set is bound once and every primitive reads the same locked-in
/// objects (CR 611.2c), even after an earlier primitive changes card types.
#[test]
fn collection_subject_is_locked_in_once_for_every_primitive() {
    let rule = parse(
        "Until end of turn, each land you control becomes a 3/3 Elemental creature with haste. It's still a land.",
    )
    .expect("collection animation");
    assert_eq!(rule["effects"][0]["kind"], "bind");
    let locked = json!({ "kind": "boundObjects", "binding": rule["effects"][0]["id"] });
    assert!(
        rule["effects"].as_array().unwrap()[1..]
            .iter()
            .all(|effect| effect["object"] == locked)
    );
}

/// The runtime removes later grants of an object that lost all abilities, so
/// `loses all abilities, and gains flying` is refused instead of approximated.
#[test]
fn a_grant_after_ability_loss_is_refused() {
    let failure = parse(
        "Until end of turn, target creature you control becomes a blue and red Dragon with base power and toughness 4/4, loses all abilities, and gains flying.",
    )
    .expect_err("layer-6 timestamp order");
    assert!(failure.contains("layer-6 timestamp"), "{failure}");
}
