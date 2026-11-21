use super::super::*;
use crate::oracle::canonical::{MachineResult, OracleTokenCursor};
use crate::oracle::syntax::recognize_entities;

fn parse(text: &str) -> MachineResult<MandatoryAdditionalSacrificeCostNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("mandatory-additional-sacrifice-test", &tokens);
    match try_recognize_mandatory_additional_sacrifice_cost(&mut cursor) {
        MandatoryAdditionalSacrificeCostAttempt::Parsed(node) => Ok(node),
        MandatoryAdditionalSacrificeCostAttempt::Unpromoted(failure)
        | MandatoryAdditionalSacrificeCostAttempt::Failed(failure) => Err(failure),
    }
}

#[test]
fn land_cost_is_one_permanent_query() {
    let node = parse("As an additional cost to cast this spell, sacrifice a land.")
        .expect("land sacrifice parses");

    assert_eq!(node.object.controller, None);
    assert!(node.object.predicate.has_domain(ObjectKind::Permanent));
    assert!(
        node.object
            .predicate
            .guarantees_card_type(ObjectCardType::Land)
    );
}

#[test]
fn unions_and_negative_permanent_types_share_the_selector() {
    let union =
        parse("As an additional cost to cast this spell, sacrifice an artifact or creature.")
            .expect("permanent-type union parses");
    assert!(
        matches!(union.object.predicate, ObjectPredicate::Union(ref values) if values.len() == 2)
    );

    let nonland = parse("As an additional cost to cast this spell, sacrifice a nonland permanent.")
        .expect("negative permanent predicate parses");
    assert!(nonland.object.predicate.has_domain(ObjectKind::Permanent));
}

#[test]
fn cost_rejects_nonpermanent_and_explicit_foreign_controller() {
    let nonpermanent = parse("As an additional cost to cast this spell, sacrifice an instant.")
        .expect_err("an instant cannot be sacrificed from the battlefield");
    assert_eq!(
        nonpermanent.state,
        "additional-cost.mandatory-sacrifice.object-query"
    );

    let foreign = parse(
        "As an additional cost to cast this spell, sacrifice a creature an opponent controls.",
    )
    .expect_err("the caster cannot pay with an opponent-controlled permanent");
    assert_eq!(
        foreign.state,
        "additional-cost.mandatory-sacrifice.object-query"
    );
}

#[test]
fn optional_counted_and_alternative_costs_are_near_misses() {
    for text in [
        "As an additional cost to cast this spell, you may sacrifice a land.",
        "As an additional cost to cast this spell, sacrifice two lands.",
        "As an additional cost to cast this spell, sacrifice a land or pay {1}.",
    ] {
        assert!(
            parse(text).is_err(),
            "near-miss unexpectedly parsed: {text}"
        );
    }
}

#[test]
fn sacrifice_context_accepts_subtype_nouns_but_not_bare_adjectives() {
    let node = parse("As an additional cost to cast this spell, sacrifice a Goblin.")
        .expect("the sacrifice action supplies the permanent domain");
    assert_eq!(
        node.object.predicate,
        ObjectPredicate::Subtype("Goblin".into())
    );
    assert!(parse("As an additional cost to cast this spell, sacrifice a green.").is_err());
}

#[test]
fn unsupported_suffix_and_variable_restore_the_transaction_checkpoint() {
    for (text, state) in [
        (
            "As an additional cost to cast this spell, sacrifice a creature that dealt damage this turn.",
            "additional-cost.mandatory-sacrifice.terminator",
        ),
        (
            "As an additional cost to cast this spell, sacrifice a creature with mana value X.",
            "additional-cost.mandatory-sacrifice.numeric-context",
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("mandatory-sacrifice-transaction-test", &tokens);
        let MandatoryAdditionalSacrificeCostAttempt::Unpromoted(failure) =
            try_recognize_mandatory_additional_sacrifice_cost(&mut cursor)
        else {
            panic!("unsupported grammar must retain a tentative failure: {text}");
        };
        assert_eq!(failure.state, state);
        assert_eq!(cursor.position(), 0);
    }
}
