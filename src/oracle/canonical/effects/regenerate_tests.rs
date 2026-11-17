use serde_json::Value;

use crate::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};

fn parse(type_line: &str, text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Generalized Regenerator".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{G}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn only_effects(parsed: &OracleCardParseResult) -> &Vec<Value> {
    let ability = &parsed.abilities[0];
    assert_eq!(ability.status, "canonical", "{parsed:#?}");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    ability.rule.as_ref().expect("native rule")["effects"]
        .as_array()
        .expect("effect program")
}

const SOURCE_REMINDER: &str = "(The next time this creature would be destroyed this turn, instead tap it, remove it from combat, and heal all damage on it.)";

#[test]
fn source_regeneration_with_its_printed_reminder_installs_one_shield() {
    let parsed = parse(
        "Creature - Skeleton",
        &format!("{{B}}: Regenerate this creature. {SOURCE_REMINDER}"),
    );
    let effects = only_effects(&parsed);
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0]["kind"], "installRegenerationShield");
    assert_eq!(effects[0]["object"]["kind"], "self");
    assert_eq!(effects[0]["duration"]["kind"], "untilEndOfCurrentTurn");
}

#[test]
fn target_object_uses_the_shared_target_declaration_and_reminder_pronoun() {
    let parsed = parse(
        "Creature - Elf",
        "{1}, {T}: Regenerate target creature. (The next time that creature would be destroyed this turn, instead tap it, remove it from combat, and heal all damage on it.)",
    );
    let effects = only_effects(&parsed);
    assert_eq!(effects[0]["kind"], "installRegenerationShield");
    assert_eq!(effects[0]["object"]["kind"], "chosenTarget");
}

#[test]
fn collections_and_bound_references_compose_without_new_routing() {
    let parsed = parse("Instant", "Regenerate each creature you control.");
    let effects = only_effects(&parsed);
    assert_eq!(effects[0]["kind"], "installRegenerationShields");
    assert_eq!(effects[0]["objects"]["kind"], "eachPermanent");

    let parsed = parse("Instant", "Tap target creature. Regenerate it.");
    let effects = only_effects(&parsed);
    assert_eq!(effects[0]["kind"], "tapPermanent");
    assert_eq!(effects[1]["kind"], "installRegenerationShield");
    assert_eq!(effects[1]["object"], effects[0]["permanent"]);
}

#[test]
fn near_misses_remain_explicit_failures() {
    for text in [
        // The reminder must restate this exact keyword action.
        "{B}: Regenerate this creature. (The next time this creature would be destroyed this turn, instead untap it, remove it from combat, and heal all damage on it.)",
        "{B}: Regenerate this creature. (The next time this creature would be destroyed this turn, instead tap it.)",
        // A collection has no printed reminder.
        "Regenerate each creature you control. (The next time it would be destroyed this turn, instead tap it, remove it from combat, and heal all damage on it.)",
        // The keyword action requires an object.
        "{B}: Regenerate.",
    ] {
        let parsed = parse("Instant", text);
        assert_ne!(
            parsed.abilities[0].status, "canonical",
            "{text}: {parsed:#?}"
        );
    }
}
