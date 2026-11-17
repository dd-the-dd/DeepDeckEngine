use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_entities};

fn parse(text: &str, type_line: &str) -> MachineResult<CanonicalRuleDraft> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: text.len(),
    };
    let tokens = recognize_entities(text);
    parse_promoted_static_spell_reduction(&AbilityInput {
        face_name: "Generic Reduction Source",
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn attempt(text: &str, type_line: &str) -> StaticSpellReductionParseAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: text.len(),
    };
    let tokens = recognize_entities(text);
    try_parse_promoted_static_spell_reduction(&AbilityInput {
        face_name: "Generic Reduction Source",
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn relative_color_union_lowers_to_one_controller_reduction() {
    let draft = parse(
        "Each spell you cast that's red or green costs {1} less to cast.",
        "Creature — Goblin Shaman",
    )
    .expect("exact relative ability lowers");

    let modifier = &draft.rule["modifiers"][0];
    assert_eq!(modifier["kind"], "reduceCastingCost");
    assert_eq!(modifier["player"]["kind"], "controllerOf");
    assert_eq!(modifier["where"]["kind"], "or");
    assert_eq!(
        modifier["where"]["operands"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(modifier["amount"]["kind"], "integer");
    assert_eq!(modifier["amount"]["value"], 1);
}

#[test]
fn permanent_source_is_required_for_battlefield_active_reduction() {
    let failure = parse("Artifact spells you cast cost {1} less to cast.", "Instant")
        .expect_err("an instant cannot supply a battlefield-active static modifier");
    assert_eq!(failure.state, "static.spell-cost-reduction.source-zone");
}

#[test]
fn domain_only_spell_set_lowers_to_null_runtime_filter() {
    let draft = parse("Spells you cast cost {1} less to cast.", "Artifact")
        .expect("all-spells sibling lowers");
    assert!(draft.rule["modifiers"][0]["where"].is_null());
}

#[test]
fn parse_once_boundary_preserves_other_families_before_cost_and_commits_after_it() {
    for text in [
        "During turns other than yours, spells you cast cost {1} less to cast.",
        "Spells you cast from your graveyard cost {1} less to cast.",
    ] {
        assert!(matches!(
            attempt(text, "Enchantment"),
            StaticSpellReductionParseAttempt::NotClaimed
        ));
    }
    assert!(matches!(
        attempt("Red spells you cast cost {1} less to cast.", "Emblem"),
        StaticSpellReductionParseAttempt::NotClaimed
    ));

    let StaticSpellReductionParseAttempt::Failed(opponent) = attempt(
        "Red spells your opponents cast cost {1} less to cast.",
        "Enchantment",
    ) else {
        panic!("opponent scope must be a committed native failure");
    };
    assert_eq!(opponent.state, "static.spell-cost-reduction.caster-scope");

    let StaticSpellReductionParseAttempt::Failed(colored) =
        attempt("Red spells you cast cost {R} less to cast.", "Enchantment")
    else {
        panic!("invalid amount after cost must be a committed native failure");
    };
    assert_eq!(colored.state, "static.spell-cost-reduction.cost");
}
