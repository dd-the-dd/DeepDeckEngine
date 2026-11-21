use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_entities};

fn parse(text: &str) -> MachineResult<CanonicalRuleDraft> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: text.len(),
    };
    let tokens = recognize_entities(text);
    parse_promoted_permanent_scaled_casting_reduction(&AbilityInput {
        face_name: "Reduction Source",
        face_type_line: "Creature",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn modifier(text: &str) -> Value {
    parse(text).expect("conditional reduction parses").rule["modifiers"][0].clone()
}

#[test]
fn presence_condition_gates_an_own_generic_reduction() {
    let m = modifier("This spell costs {1} less to cast if you control a Kithkin.");
    assert_eq!(m["kind"], "reduceOwnGenericCastingCost");
    assert_eq!(m["condition"]["kind"], "compare");
    assert_eq!(m["condition"]["left"]["kind"], "countPermanents");
}

#[test]
fn count_and_boolean_composition_reuse_the_shared_condition() {
    let m = modifier(
        "This spell costs {2} less to cast if you control a Kithkin or you control three or more artifacts.",
    );
    assert_eq!(m["condition"]["kind"], "or");
    assert_eq!(m["condition"]["operands"].as_array().map(Vec::len), Some(2));
}

#[test]
fn unconditional_reduction_has_no_condition_field() {
    let m = modifier("This spell costs {1} less to cast.");
    assert!(m.get("condition").is_none());
}

#[test]
fn near_misses_fail_at_precise_states() {
    let unknown = parse("This spell costs {1} less to cast if it's your birthday.")
        .expect_err("unknown condition atom");
    assert_eq!(unknown.state, "condition.trigger-guard.atom-kind");
    let spell_set =
        parse("Creature spells you cast cost {1} less to cast if you control a Kithkin.")
            .expect_err("spell-set conditions are not executable");
    assert_eq!(spell_set.state, "casting-reduction.condition.subject");
    // The counted query owns its own clause boundary; no condition is composed.
    parse(
        "This spell costs {1} less to cast for each artifact you control if you control a Kithkin.",
    )
    .expect_err("scaled amount plus condition is not composed");
}

#[test]
fn target_query_lowers_to_the_shared_permanent_predicate() {
    let m = modifier("This spell costs {1} less to cast if it targets an attacking creature.");
    assert_eq!(m["kind"], "reduceOwnGenericCastingCost");
    assert!(m.get("condition").is_none());
    assert_eq!(m["targetWhere"]["kind"], "and");
    // A new combination of supported predicate concepts needs no new production code.
    let m = modifier("This spell costs {2} less to cast if it targets a tapped artifact.");
    assert_eq!(m["targetWhere"]["kind"], "and");
    assert_eq!(m["amount"]["value"], 2);
}

#[test]
fn target_query_near_misses_fail_explicitly() {
    let controller =
        parse("This spell costs {1} less to cast if it targets a creature you control.")
            .expect_err("controller relation is not a target filter");
    assert_eq!(controller.state, "casting-reduction.condition.target-query");
    parse("This spell costs {1} less to cast if it targets.").expect_err("missing query");
}
