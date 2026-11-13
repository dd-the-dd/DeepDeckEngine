use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities},
};

use super::*;

fn source(text: &str) -> AbilitySource {
    AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    }
}

fn parse(text: &str) -> MachineResult<CanonicalRuleDraft> {
    let source = source(text);
    let tokens = recognize_oracle_entities(text);
    parse_promoted_collection_destroy(&AbilityInput {
        face_name: "Generic Destruction",
        face_type_line: "Sorcery",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn opponents_nonland_collection_lowers_to_existing_primitives() {
    let rule = parse("Destroy all nonland permanents your opponents control.")
        .expect("opponent collection destruction parses")
        .rule;
    assert_eq!(rule["effects"][0]["kind"], "destroyPermanent");
    assert_eq!(rule["effects"][0]["permanent"]["kind"], "eachPermanent");
    assert_eq!(
        rule["effects"][0]["permanent"]["player"]["kind"],
        "opponentsOf"
    );
    assert!(rule.get("declaration").is_none());
    assert!(crate::engine::rule_is_executable(&rule));
}

#[test]
fn sibling_target_action_keeps_its_declaration() {
    let rule = parse("Destroy all creatures. Target creature gets +1/+1 until end of turn.")
        .expect("collection destruction composes with a targeted sibling")
        .rule;
    assert_eq!(rule["effects"][0]["kind"], "destroyPermanent");
    assert_eq!(rule["declaration"]["decisions"][0]["id"], "targetPermanent");
    assert_eq!(rule["effects"][1]["kind"], "modifyPowerToughness");
    assert_eq!(rule["effects"][1]["object"]["id"], "targetPermanent");
    assert!(crate::engine::rule_is_executable(&rule));
}

#[test]
fn targetless_sibling_remains_an_independent_action() {
    let rule = parse("Destroy all creatures, then draw a card.")
        .expect("collection destruction composes with draw")
        .rule;
    assert_eq!(rule["effects"][0]["kind"], "destroyPermanent");
    assert_eq!(rule["effects"][1]["kind"], "drawCards");
    assert!(crate::engine::rule_is_executable(&rule));
}
