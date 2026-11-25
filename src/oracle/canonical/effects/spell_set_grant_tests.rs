use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_entities};

fn attempt(text: &str) -> SpellSetGrantParseAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: text.len(),
    };
    let tokens = recognize_entities(text);
    try_parse_spell_set_grant(&AbilityInput {
        face_name: "Generic Spell Grant Source",
        face_type_line: "Artifact Creature — Construct",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn modifier(text: &str) -> Value {
    match attempt(text) {
        SpellSetGrantParseAttempt::Parsed(draft) => draft.rule["modifiers"][0].clone(),
        SpellSetGrantParseAttempt::Failed(failure) => panic!("{text}: {failure}"),
        SpellSetGrantParseAttempt::NotClaimed => panic!("{text}: not claimed"),
    }
}

fn failure_state(text: &str) -> String {
    match attempt(text) {
        SpellSetGrantParseAttempt::Failed(failure) => failure.state.to_string(),
        SpellSetGrantParseAttempt::Parsed(draft) => panic!("{text}: parsed {}", draft.rule),
        SpellSetGrantParseAttempt::NotClaimed => panic!("{text}: not claimed"),
    }
}

#[test]
fn chief_engineer_lowers_to_the_convoke_spell_grant() {
    let modifier = modifier(
        "Artifact spells you cast have convoke. (Your creatures can help cast those spells. Each creature you tap while casting an artifact spell pays for {1} or one mana of that creature's color.)",
    );
    assert_eq!(
        modifier,
        json!({
            "kind": "grantConvoke",
            "spells": {
                "kind": "spells",
                "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
                "where": { "kind": "cardTypeContains", "value": "Artifact" },
            },
        })
    );
}

#[test]
fn sibling_keywords_reuse_the_same_spell_subject() {
    let improvise = modifier("Noncreature spells you cast have improvise.");
    assert_eq!(improvise["kind"], "grantImprovise");
    assert_eq!(improvise["spells"]["where"]["kind"], "not");
    let conspire = modifier("Each noncreature spell you cast has conspire.");
    assert_eq!(conspire["kind"], "grantConspire");
    let replicate = modifier("Artifact spells you cast have replicate {2}{U}.");
    assert_eq!(replicate["kind"], "grantReplicate");
    assert_eq!(replicate["cost"]["manaCost"], "{2}{U}");
    // A previously unused combination: a numeric spell filter on cascade.
    let cascade = modifier("Spells you cast with mana value 6 or greater have cascade.");
    assert_eq!(cascade["kind"], "grantCascadeToSpells");
    assert_eq!(cascade["minimumManaValue"], 0);
    assert_eq!(cascade["where"]["kind"], "compare");
    let all = modifier("Spells you cast have split second.");
    assert_eq!(all["kind"], "grantKeywordToSpells");
    assert_eq!(all["keyword"], "splitSecond");
    assert!(all["where"].is_null());
}

#[test]
fn keywords_and_casters_without_a_spell_grant_fail_explicitly() {
    assert_eq!(
        failure_state("Instant and sorcery spells you cast have storm."),
        "static.spell-grant.keyword-modifier"
    );
    assert_eq!(
        failure_state("Spells your opponents cast have convoke."),
        "static.spell-grant.caster"
    );
}

#[test]
fn a_spell_set_prohibition_lowers_to_the_filtered_cast_prohibition() {
    // The symbolic chosen name; the card-level entry binder links it to the
    // face's stored choice or refuses the card.
    assert_eq!(
        modifier("Spells with the chosen name can't be cast."),
        json!({
            "kind": "prohibitCast",
            "players": { "kind": "eachPlayer" },
            "where": {
                "kind": "chosenCardName",
                "decisionId": crate::oracle::contextual_binding::RESOLUTION_CARD_NAME_DECISION,
            },
        })
    );
    // A previously unused combination: a characteristic filter.
    let creatures = modifier("Creature spells can't be cast.");
    assert_eq!(
        creatures["where"],
        json!({ "kind": "cardTypeContains", "value": "Creature" })
    );
    assert_eq!(
        failure_state("Spells you cast can't be cast."),
        "static.spell-grant.prohibition"
    );
}
