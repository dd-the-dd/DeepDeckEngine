use super::*;
use crate::oracle::model::{AbilitySource, RecognizedEntity};
use crate::oracle::syntax::{AbilityInput, recognize_entities};

fn input<'a>(source: &'a AbilitySource, tokens: &'a [RecognizedEntity]) -> AbilityInput<'a> {
    AbilityInput {
        face_name: "Generic Spell Tax",
        face_type_line: "Enchantment",
        source,
        tokens,
        name_links: &[],
        name_resolution_failure: None,
    }
}

fn parse(text: &str) -> CanonicalRuleDraft {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: text.len(),
    };
    let tokens = recognize_entities(text);
    parse_promoted_spell_cost_increase(&input(&source, &tokens))
        .expect("spell cost increase lowers")
}

#[test]
fn thalia_lowers_to_one_atomic_additional_casting_cost() {
    let draft = parse("Noncreature spells cost {1} more to cast.");

    assert_eq!(draft.rule["kind"], "staticAbility");
    assert_eq!(draft.rule["activeWhile"]["zone"]["kind"], "battlefield");
    assert_eq!(draft.rule["modifiers"][0]["kind"], "additionalCastingCost");
    assert_eq!(draft.rule["modifiers"][0]["players"]["kind"], "eachPlayer");
    assert_eq!(draft.rule["modifiers"][0]["where"]["kind"], "not");
    assert_eq!(
        draft.rule["modifiers"][0]["where"]["operand"]["value"],
        "Creature"
    );
    assert_eq!(draft.rule["modifiers"][0]["cost"]["kind"], "payMana");
    assert_eq!(draft.rule["modifiers"][0]["cost"]["manaCost"], "{1}");
}

#[test]
fn opponent_scope_and_composed_union_lower_without_new_operations() {
    let opposing = parse("Noncreature spells your opponents cast cost {1} more to cast.");
    assert_eq!(
        opposing.rule["modifiers"][0]["players"]["kind"],
        "opponentsOf"
    );

    let union = parse("Red creature spells and green creature spells cost {1} more to cast.");
    assert_eq!(union.rule["modifiers"][0]["where"]["kind"], "or");
    assert_eq!(
        union.rule["modifiers"][0]["where"]["operands"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
}

#[test]
fn token_claim_excludes_activation_tax_reduction_and_trailing_condition() {
    for text in [
        "Activated abilities of white enchantments cost {3} more to activate.",
        "Noncreature spells cost {1} less to cast.",
        "Noncreature spells cost {1} more to cast if it is your turn.",
    ] {
        let source = AbilitySource {
            face_id: None,
            text: text.to_string(),
            line_start: 0,
            line_end: text.len(),
        };
        let tokens = recognize_entities(text);
        assert!(!claims_promoted_spell_cost_increase_family(&input(
            &source, &tokens
        )));
    }
}

#[test]
fn nonpermanent_source_fails_before_battlefield_active_lowering() {
    let text = "Noncreature spells cost {1} more to cast.";
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: text.len(),
    };
    let tokens = recognize_entities(text);
    let mut ability = input(&source, &tokens);
    ability.face_type_line = "Instant";

    let failure = parse_promoted_spell_cost_increase(&ability)
        .expect_err("an instant cannot host a battlefield-active static rule");
    assert_eq!(failure.state, "static.spell-cost-increase.source-zone");
    assert!(failure.expected.contains("permanent card face"));
}
