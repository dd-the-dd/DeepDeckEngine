use crate::oracle::syntax::{NameLinkKind, ResolvedNameLink, recognize_entities};

use super::super::{
    DelayedTurnStep, LocalObjectReference, LocalTriggerEventNode, NumericAtom, NumericExpression,
    TargetCardinality, ZoneChangeAction,
};
use super::{OracleTokenCursor, recognize_attack_delayed_return_counter};

const EXACT: &str = "Whenever Phelia attacks, exile up to one other target nonland permanent. At the beginning of the next end step, return that card to the battlefield under its owner's control. If it entered under your control, put a +1/+1 counter on Phelia.";

fn current_name_links(tokens: &[crate::oracle::model::RecognizedEntity]) -> Vec<ResolvedNameLink> {
    tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| token.raw.eq_ignore_ascii_case("Phelia"))
        .map(|(index, token)| ResolvedNameLink {
            kind: NameLinkKind::CurrentCard,
            canonical_name: "Phelia, Exuberant Shepherd".to_string(),
            raw: "Phelia".to_string(),
            token_start: index,
            token_end: index + 1,
            span_start: token.span_start,
            span_end: token.span_end,
        })
        .collect()
}

#[test]
fn attack_delayed_return_composes_shared_typed_children() {
    let tokens = recognize_entities(EXACT);
    let links = current_name_links(&tokens);
    let mut cursor = OracleTokenCursor::new("test.attack-delayed-return", &tokens);

    let node = recognize_attack_delayed_return_counter(&mut cursor, &links, None)
        .expect("sequence parses");

    assert_eq!(
        node.event,
        LocalTriggerEventNode::SourceAttacked {
            source: LocalObjectReference::SourceSelf,
        }
    );
    assert_eq!(node.exile.action, ZoneChangeAction::Exile);
    assert_eq!(node.exile.target.slots.len(), 1);
    assert_eq!(
        node.exile.target.slots[0].cardinality,
        TargetCardinality {
            minimum: 0,
            maximum: 1,
        }
    );
    assert!(node.exile.target.slots[0].exclude_source);
    assert_eq!(node.delayed_return.step, DelayedTurnStep::EndStep);
    assert_eq!(node.delayed_return.object, node.condition.object);
    assert_eq!(
        node.counters.permanent,
        super::super::PermanentOperandNode::Reference(LocalObjectReference::SourceSelf)
    );
    assert_eq!(node.counters.counter.as_str(), "+1/+1");
    assert_eq!(
        node.counters.count,
        NumericExpression::Atom(NumericAtom::Integer(1))
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn source_and_selector_are_generalized() {
    let text = "Whenever this artifact attacks, exile up to one other target creature. At the beginning of the next end step, return that card to the battlefield under its owner's control. If it entered under your control, put two +1/+1 counters on this artifact.";
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.attack-delayed-return", &tokens);

    let node = recognize_attack_delayed_return_counter(&mut cursor, &[], None)
        .expect("generalized sequence parses");

    assert_eq!(node.counters.counter.as_str(), "+1/+1");
    assert_eq!(
        node.counters.count,
        NumericExpression::Atom(NumericAtom::Integer(2))
    );
}

#[test]
fn unsupported_delayed_step_fails_at_the_temporal_child() {
    let text = EXACT.replace("next end step", "next combat");
    let tokens = recognize_entities(&text);
    let links = current_name_links(&tokens);
    let mut cursor = OracleTokenCursor::new("test.attack-delayed-return", &tokens);

    let failure = recognize_attack_delayed_return_counter(&mut cursor, &links, None)
        .expect_err("unsupported step must fail visibly");

    assert_eq!(failure.state, "duration.next-step.step");
    assert_eq!(failure.actual.as_deref(), Some("combat"));
    assert!(failure.expected.contains("end"));
}
