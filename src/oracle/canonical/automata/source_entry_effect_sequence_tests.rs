use crate::oracle::canonical::{EffectConnector, LocalEffectNode, TargetBinding};
use crate::oracle::syntax::recognize_oracle_entities;

use super::{LocalObjectReference, OracleTokenCursor, recognize_source_entry_effect_sequence};

fn parse(text: &str) -> super::MachineResult<super::SourceEntryEffectSequenceNode> {
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("source-entry-effect-sequence-test", &tokens);
    recognize_source_entry_effect_sequence(&mut cursor, &[], None)
}

#[test]
fn delegates_source_entry_draw_to_shared_event_and_effect_children() {
    let node = parse("When this creature enters, draw a card.")
        .expect("the source-entry parent should compose the shared children");

    assert_eq!(node.source, LocalObjectReference::SourceSelf);
    assert!(!node.optional);
    assert!(matches!(
        node.effects.effects.as_slice(),
        [LocalEffectNode::DrawCards(_)]
    ));
    assert!(node.effects.connectors.is_empty());
}

#[test]
fn optionality_is_retained_by_the_entry_parent_not_the_move_atom() {
    let node = parse(
        "When this creature enters, you may return target card from your graveyard to your hand.",
    )
    .expect("the parent should retain optionality around one shared card move");

    assert!(node.optional);
    assert!(matches!(
        node.effects.effects.as_slice(),
        [LocalEffectNode::TargetCardZoneChange(_)]
    ));
}

#[test]
fn rejects_a_required_continuation_outside_the_optional_effect_scope() {
    let failure = parse(
        "When this creature enters, you may return target card from your graveyard to your hand. Draw a card.",
    )
    .expect_err("a parent-level optional wrapper must not swallow a required continuation");

    assert_eq!(
        failure.state,
        "trigger.source-entry-sequence.optional-scope"
    );
    assert_eq!(
        failure.expected,
        "exactly one atomic effect governed by 'you may'"
    );
}

#[test]
fn preserves_target_slot_identity_for_a_later_untap_anaphor() {
    let node =
        parse("When this creature enters, target creature gets +1/+1 until end of turn. Untap it.")
            .expect("the shared sequence should retain its target binding");

    assert_eq!(node.effects.connectors, [EffectConnector::Then]);
    let [
        LocalEffectNode::ModifyStats(_),
        LocalEffectNode::UntapTarget(untap),
    ] = node.effects.effects.as_slice()
    else {
        panic!("expected an ordinary modifier followed by an ordinary untap action");
    };
    assert_eq!(
        untap.permanent,
        TargetBinding {
            effect_index: 0,
            slot_index: 0,
        }
    );
}

#[test]
fn malformed_promoted_draw_reports_the_shared_child_state() {
    let failure = parse("When this creature enters, draw target player a card.")
        .expect_err("a malformed promoted child must not be partially accepted");

    assert_eq!(failure.state, "numeric.atom.start");
    assert_eq!(failure.actual.as_deref(), Some("target"));
}
