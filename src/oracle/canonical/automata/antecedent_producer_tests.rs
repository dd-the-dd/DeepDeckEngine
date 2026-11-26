//! Antecedent producers (batch 12): the block record's other combatant
//! stays the lexical antecedent of a later `that creature` operand, and
//! a condition pronoun reads the program's permanent subject before the
//! antecedents outside the permanent scope. Card-level evidence (exact,
//! sibling and near-miss Oracle texts) lives in the parser fixture
//! `antecedent-producers-batch12.json`.
use super::super::{
    GrammaticalNumber, ObjectCardType, ObjectSubjectAntecedent, PermanentEffectReference,
    TargetNominal, TypedObservedEventObject,
};
use super::{
    BindingEnvironment, ConditionPronounAntecedentOrObject, ObjectSetAntecedent,
    ObjectSetDenotation, resolve_condition_subject_pronoun, resolve_nominal_antecedent,
};

const CREATURE: TargetNominal = TargetNominal::CardType(ObjectCardType::Creature);

fn counterpart() -> PermanentEffectReference {
    PermanentEffectReference::Observed(TypedObservedEventObject::CombatCounterpart)
}

/// After `tap that creature`, the local scope holds the counterpart.
fn after_counterpart_action() -> BindingEnvironment {
    BindingEnvironment {
        local_object_scope: true,
        object: Some(ObjectSubjectAntecedent {
            reference: counterpart(),
            selector: None,
        }),
        effect_index: Some(1),
        ..BindingEnvironment::default()
    }
}

#[test]
fn restated_combat_counterpart_is_an_operand_antecedent_only() {
    let environment = after_counterpart_action();
    assert_eq!(
        resolve_nominal_antecedent(&environment, &[CREATURE], false),
        Ok(counterpart())
    );
    // No last known information is recorded for the counterpart: a read
    // (`that creature's toughness` after `destroy that creature`) keeps its
    // diagnostic instead of reading a departed object.
    assert!(resolve_nominal_antecedent(&environment, &[CREATURE], true).is_err());
    // A nominal the block record does not guarantee is not the counterpart.
    let land = TargetNominal::CardType(ObjectCardType::Land);
    assert!(resolve_nominal_antecedent(&environment, &[land], false).is_err());
}

#[test]
fn condition_pronoun_reads_the_permanent_subject_first() {
    let environment = after_counterpart_action();
    assert_eq!(
        resolve_condition_subject_pronoun(&environment),
        Ok(ConditionPronounAntecedentOrObject::Object(counterpart()))
    );
    // Created tokens are a set: a singular pronoun is a number mismatch,
    // distinct from a missing antecedent.
    let tokens = PermanentEffectReference::Observed(TypedObservedEventObject::CreatedTokens);
    let plural = BindingEnvironment {
        objects: Some(ObjectSetAntecedent {
            denotes: ObjectSetDenotation::Bound(tokens),
            selector: None,
            number: GrammaticalNumber::Plural,
            tokens: true,
        }),
        ..BindingEnvironment::default()
    };
    let missing = resolve_condition_subject_pronoun(&BindingEnvironment::default());
    let mismatch = resolve_condition_subject_pronoun(&plural);
    assert!(missing.is_err() && mismatch.is_err());
    assert_ne!(missing, mismatch);
}
