use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_entities};

fn parse(text: &str) -> CanonicalRuleDraft {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: text.len(),
    };
    let tokens = recognize_entities(text);
    let input = AbilityInput {
        face_name: "Generic Tax",
        face_type_line: "Enchantment",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };
    parse_promoted_attack_tax(&input).expect("attack tax lowers")
}

#[test]
fn fixed_tax_lowers_to_the_existing_static_modifier() {
    let draft = parse(
        "Creatures can't attack you unless their controller pays {4} for each creature they control that's attacking you.",
    );

    assert_eq!(draft.rule["kind"], "staticAbility");
    assert_eq!(draft.rule["modifiers"][0]["kind"], "attackTax");
    assert_eq!(
        draft.rule["modifiers"][0]["protectedPlayer"]["kind"],
        "controllerOf"
    );
    assert_eq!(draft.rule["modifiers"][0]["amount"]["value"], 4);
}
