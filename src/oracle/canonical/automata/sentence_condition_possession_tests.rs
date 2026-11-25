//! Reader-level contract of what a referenced object has and of the
//! participle subjects: a possession verb (negated by its auxiliary) states
//! the predicate of the matching `with` qualifier of the shared object
//! query, and `the VERBed NOUN` names the object an earlier action or a
//! cost of the ability VERBed. A missing producer stays an explicit failure.
use crate::oracle::syntax::recognize_entities;

use super::super::{
    BooleanConditionNode, EffectRecognitionContext, KeywordAbilityName, LocalObjectReference,
    NumericExpression, NumericObjectProperty, ObjectPredicate, ObjectQualifier,
    PermanentEffectReference, PermanentSubjectScope, SelectorNumericProperty, TargetBinding,
};
use super::{
    ConditionAtom, ConditionObjectReference, CostPaidConditionObject, OracleTokenCursor,
    SentenceConditionContext, recognize_sentence_condition,
};

const TARGET: TargetBinding = TargetBinding {
    effect_index: 0,
    slot_index: 0,
};

fn read_with(
    text: &str,
    effects: &EffectRecognitionContext,
) -> Result<BooleanConditionNode<ConditionAtom>, (String, String)> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("object-possession-test", &tokens);
    let context = SentenceConditionContext {
        name_links: &[],
        name_failure: None,
        effects,
        card_target: None,
        card_focus: false,
        departed_targets: &[],
    };
    recognize_sentence_condition(&mut cursor, &context)
        .map_err(|failure| (failure.state.to_string(), failure.expected))
}

fn read(text: &str) -> Result<BooleanConditionNode<ConditionAtom>, (String, String)> {
    read_with(text, &EffectRecognitionContext::default())
}

fn target_context() -> EffectRecognitionContext {
    EffectRecognitionContext {
        permanent_subject_scope: PermanentSubjectScope::Local(Some(
            PermanentEffectReference::Target(TARGET),
        )),
        ..EffectRecognitionContext::default()
    }
}

fn object_atom(
    text: &str,
    effects: &EffectRecognitionContext,
) -> (ConditionObjectReference, ObjectPredicate) {
    let node = read_with(text, effects).unwrap_or_else(|failure| panic!("{text}: {failure:?}"));
    let BooleanConditionNode::Atom(ConditionAtom::ObjectSatisfies(node)) = node else {
        panic!("{text}: one object atom, got {node:?}");
    };
    (node.object, node.predicate)
}

#[test]
fn a_negated_auxiliary_negates_the_possessed_characteristic() {
    let (object, positive) = object_atom("If it has flying,", &target_context());
    assert_eq!(
        object,
        ConditionObjectReference::Permanent(PermanentEffectReference::Target(TARGET))
    );
    // Sibling forms of the same negation: the auxiliary contracted or not,
    // in the present and the past (last known information).
    for text in [
        "If it doesn't have flying,",
        "If it didn't have flying,",
        "If it does not have flying,",
    ] {
        let (_, predicate) = object_atom(text, &target_context());
        assert_eq!(
            predicate,
            ObjectPredicate::Not(Box::new(positive.clone())),
            "{text}"
        );
    }
    // The negation composes with every possessed form (counters).
    let (_, counters) = object_atom(
        "If it doesn't have a +1/+1 counter on it,",
        &target_context(),
    );
    assert!(
        matches!(counters, ObjectPredicate::Not(inner) if matches!(*inner, ObjectPredicate::HasCounter(_)))
    );
    // A two-word keyword is the shared keyword reader's, not a first token.
    let (_, first_strike) = object_atom("If it has first strike,", &target_context());
    assert!(matches!(first_strike, ObjectPredicate::HasKeyword(_)));
}

#[test]
fn a_keyword_with_parameters_is_the_with_qualifier_atom() {
    let (_, toxic) = object_atom("If it has toxic,", &target_context());
    assert_eq!(
        toxic,
        ObjectPredicate::Qualifier(ObjectQualifier::KeywordAbility(KeywordAbilityName::Toxic))
    );
    // Sibling: the article form of the same qualifier.
    let (_, kicker) = object_atom("If it has a kicker ability,", &target_context());
    assert_eq!(
        kicker,
        ObjectPredicate::Qualifier(ObjectQualifier::KeywordAbility(KeywordAbilityName::Kicker))
    );
    // Near-miss: an unregistered word is no keyword and no property.
    let (state, _) = read_with("If it has wings,", &target_context()).expect_err("no keyword");
    assert_eq!(state, "condition.sentence.object.property");
}

#[test]
fn a_comparative_compares_each_property_with_the_anchor() {
    let (_, power) = object_atom(
        "If it has greater power than this creature,",
        &target_context(),
    );
    assert_eq!(
        power,
        ObjectPredicate::NumericComparison {
            left: SelectorNumericProperty::Power,
            operator: super::super::NumericComparisonOperator::GreaterThan,
            right: Box::new(NumericExpression::ObjectProperty {
                object: LocalObjectReference::SourceSelf,
                property: NumericObjectProperty::Power,
            }),
        }
    );
    // Coordinated properties are a disjunction over the same anchor.
    let (_, either) = object_atom(
        "If it has greater power or toughness than this creature,",
        &target_context(),
    );
    let ObjectPredicate::Union(branches) = either else {
        panic!("a disjunction of comparisons");
    };
    assert_eq!(branches.len(), 2);
    // Near-miss: a comparative without its `than` anchor.
    let (state, _) =
        read_with("If it has greater power,", &target_context()).expect_err("no anchor");
    assert_eq!(state, "condition.sentence.object.comparative-anchor");
}

#[test]
fn a_cost_participle_names_the_object_its_cost_paid_with() {
    let (object, legendary) = object_atom(
        "If the sacrificed creature was legendary,",
        &EffectRecognitionContext::default(),
    );
    assert_eq!(
        object,
        ConditionObjectReference::CostPaid(CostPaidConditionObject::SacrificedPermanent)
    );
    assert!(matches!(legendary, ObjectPredicate::Supertype(_)));
    // Sibling: another nominal and a negated copula over the discarded card.
    let (object, _) = object_atom(
        "If the discarded card wasn't a land card,",
        &EffectRecognitionContext::default(),
    );
    assert_eq!(
        object,
        ConditionObjectReference::CostPaid(CostPaidConditionObject::DiscardedCard)
    );
    let (object, _) = object_atom(
        "If the sacrificed permanent was an artifact,",
        &EffectRecognitionContext::default(),
    );
    assert_eq!(
        object,
        ConditionObjectReference::CostPaid(CostPaidConditionObject::SacrificedPermanent)
    );
}

#[test]
fn a_participle_without_a_producer_fails_at_its_antecedent() {
    // Near-miss: no action of the program milled a card and no cost mills.
    let (state, _) = read("If the milled card was a land card,").expect_err("no producer");
    assert_eq!(state, "condition.sentence.object.participle-antecedent");
    // Near-miss: a discard pays with a card, never with a creature.
    let (state, _) = read("If the discarded creature was red,").expect_err("a creature");
    assert_eq!(state, "condition.sentence.object.participle-antecedent");
    // Near-miss: the cost object has no current zone to read.
    let (state, _) = read("If the sacrificed creature is in your graveyard,")
        .expect_err("a zone of a cost object");
    assert_eq!(state, "condition.sentence.object.zone-object");
}
