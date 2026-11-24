//! Reader-level contract of the sentence condition: the pronoun resolves to
//! exactly one proven antecedent, and the clause comma stays a boundary.
use crate::oracle::syntax::recognize_entities;

use super::super::{
    BooleanConditionNode, EffectRecognitionContext, ObjectCardType, ObjectKind, ObjectPredicate,
    PermanentEffectReference, PermanentSubjectScope, TargetBinding,
};
use super::{
    ConditionAtom, ConditionObjectReference, OracleTokenCursor, ReferencedObjectConditionNode,
    SentenceConditionContext, recognize_sentence_condition,
};

const TARGET: TargetBinding = TargetBinding {
    effect_index: 0,
    slot_index: 0,
};

fn read(
    text: &str,
    effects: &EffectRecognitionContext,
    card_target: Option<TargetBinding>,
    card_focus: bool,
    departed: &[TargetBinding],
) -> Result<BooleanConditionNode<ConditionAtom>, (String, String)> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("sentence-condition-test", &tokens);
    let context = SentenceConditionContext {
        name_links: &[],
        name_failure: None,
        effects,
        card_target,
        card_focus,
        departed_targets: departed,
    };
    recognize_sentence_condition(&mut cursor, &context)
        .map_err(|failure| (failure.state.to_string(), failure.expected))
}

fn permanent_target_context() -> EffectRecognitionContext {
    EffectRecognitionContext {
        permanent_subject_scope: PermanentSubjectScope::Local(Some(
            PermanentEffectReference::Target(TARGET),
        )),
        ..EffectRecognitionContext::default()
    }
}

#[test]
fn the_pronoun_binds_the_single_proven_antecedent() {
    let card = ObjectPredicate::Intersection(vec![
        ObjectPredicate::CardType(ObjectCardType::Land),
        ObjectPredicate::ObjectKind(ObjectKind::Card),
    ]);
    assert_eq!(
        read(
            "If it's a land card,",
            &EffectRecognitionContext::default(),
            None,
            true,
            &[]
        ),
        Ok(BooleanConditionNode::Atom(ConditionAtom::ObjectSatisfies(
            ReferencedObjectConditionNode {
                object: ConditionObjectReference::CardFocus,
                predicate: card,
            }
        )))
    );
    let permanent = read(
        "If it's a creature,",
        &permanent_target_context(),
        None,
        false,
        &[],
    )
    .expect("the target is the only antecedent");
    let BooleanConditionNode::Atom(ConditionAtom::ObjectSatisfies(node)) = permanent else {
        panic!("a referenced-object atom");
    };
    assert_eq!(
        node.object,
        ConditionObjectReference::Permanent(PermanentEffectReference::Target(TARGET))
    );
}

#[test]
fn an_ambiguous_missing_or_departed_antecedent_fails_explicitly() {
    let ambiguous = read(
        "If it's a creature,",
        &permanent_target_context(),
        None,
        true,
        &[],
    )
    .expect_err("two proven antecedents are ambiguous");
    assert_eq!(ambiguous.0, "condition.sentence.object.antecedent");
    assert!(ambiguous.1.contains("not several"));

    let missing = read(
        "If that card is a creature card,",
        &EffectRecognitionContext::default(),
        None,
        false,
        &[],
    )
    .expect_err("no card antecedent");
    assert_eq!(missing.0, "condition.sentence.object.card-antecedent");

    // CR 608.2h: a moved permanent's recorded type line answers a type
    // predicate as last-known information.
    let departed = read(
        "If it was a creature,",
        &permanent_target_context(),
        None,
        false,
        &[TARGET],
    )
    .expect("the departure records the type line");
    let BooleanConditionNode::Atom(ConditionAtom::ObjectSatisfies(node)) = departed else {
        panic!("a referenced-object atom");
    };
    assert_eq!(
        node.object,
        ConditionObjectReference::DepartedTarget(TARGET)
    );
    // The departure records its tapped status (CR 608.2h), so the same
    // last-known reader answers it.
    let tapped = read(
        "If it was tapped,",
        &permanent_target_context(),
        None,
        false,
        &[TARGET],
    )
    .expect("the departure records the tapped status");
    let BooleanConditionNode::Atom(ConditionAtom::ObjectSatisfies(node)) = tapped else {
        panic!("a referenced-object atom");
    };
    assert_eq!(
        node.object,
        ConditionObjectReference::DepartedTarget(TARGET)
    );
    // A characteristic the departure does not record (its colors) is
    // refused, not read on the new object.
    let uncaptured = read(
        "If it was white,",
        &permanent_target_context(),
        None,
        false,
        &[TARGET],
    )
    .expect_err("a moved permanent's colors are not captured");
    assert_eq!(
        uncaptured.0,
        "condition.sentence.object.last-known-information"
    );
}

#[test]
fn a_permanent_antecedent_rejects_a_card_only_characteristic() {
    let failure = read(
        "If it's a land card,",
        &permanent_target_context(),
        None,
        false,
        &[],
    )
    .expect_err("`card` is not a battlefield-permanent characteristic");
    assert_eq!(failure.0, "condition.sentence.object.domain");
}
