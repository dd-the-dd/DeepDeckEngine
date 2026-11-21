use super::*;
use crate::oracle::canonical::automata::{
    EffectRecognitionContext, LocalEffectNode, ObjectCardType, ObjectControllerScope,
    ObjectPredicate, recognize_effect_sequence,
};
use crate::oracle::syntax::{recognize_entities, resolve_counter_name_spans};

#[test]
fn each_operand_preserves_shared_query_and_following_effect_boundary() {
    let mut tokens =
        recognize_entities("Put a +1/+1 counter on each creature you control, then gain 2 life.");
    resolve_counter_name_spans(&mut tokens);
    let mut cursor = OracleTokenCursor::new("permanent-operand-test", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None).unwrap();
    let [
        LocalEffectNode::PutCounters(counters),
        LocalEffectNode::GainLife(_),
    ] = sequence.effects.as_slice()
    else {
        panic!("the operand must not consume the following action");
    };
    assert_eq!(
        counters.permanent,
        PermanentOperandNode::Collection(ObjectQueryNode {
            predicate: ObjectPredicate::CardType(ObjectCardType::Creature),
            controller: Some(ObjectControllerScope::AbilityController),
        })
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn collection_and_source_share_the_same_counter_action() {
    for (text, is_collection) in [
        ("Put two ice counters on all artifacts and creatures.", true),
        ("Put a +1/+1 counter on this creature.", false),
    ] {
        let mut tokens = recognize_entities(text);
        resolve_counter_name_spans(&mut tokens);
        let mut cursor = OracleTokenCursor::new("permanent-operand-test", &tokens);
        let sequence = recognize_effect_sequence(&mut cursor, &[], None).unwrap();
        let [LocalEffectNode::PutCounters(counters)] = sequence.effects.as_slice() else {
            panic!("one ordinary counter-placement action");
        };
        assert_eq!(
            matches!(counters.permanent, PermanentOperandNode::Collection(_)),
            is_collection
        );
        assert_eq!(cursor.position(), tokens.len());
    }
}

#[test]
fn card_collection_cannot_become_a_battlefield_collection() {
    let mut tokens = recognize_entities("Put a +1/+1 counter on each creature card you control.");
    resolve_counter_name_spans(&mut tokens);
    let mut cursor = OracleTokenCursor::new("permanent-operand-test", &tokens);
    let failure = recognize_effect_sequence(&mut cursor, &[], None).unwrap_err();
    assert_eq!(failure.state, "permanent.operand.collection.domain");
}

#[test]
fn anaphor_keeps_the_callers_source_context() {
    let tokens = recognize_entities("him");
    let mut cursor = OracleTokenCursor::new("permanent-operand-test", &tokens);
    let context = EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf);
    let operand =
        recognize_permanent_operand(&mut cursor, &[], None, context.source.as_ref()).unwrap();
    assert_eq!(
        operand,
        PermanentOperandNode::Reference(LocalObjectReference::SourceSelf)
    );
}
