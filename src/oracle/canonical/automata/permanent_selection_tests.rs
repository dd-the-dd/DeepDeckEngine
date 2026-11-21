use super::super::{LocalEffectNode, ObjectControllerScope, recognize_effect_sequence};
use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn selection_has_bounds_and_query_without_implicit_controller_or_target() {
    for text in [
        "Untap up to three lands.",
        "Untap up to 2 artifacts or creatures.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("selected-untap", &tokens);
        let node = recognize_untap_permanents_effect(&mut cursor).unwrap().node;
        assert!(node.operand.query.controller.is_none());
        assert_eq!(
            node.operand.bounds.unwrap().minimum,
            super::super::NumericAtom::Integer(0)
        );
        cursor.consume(".").unwrap();
        cursor.finish().unwrap();
    }
}

#[test]
fn object_coordination_and_parent_comma_have_separate_boundaries() {
    for text in [
        "Untap up to 2 artifacts, creatures, or lands, then you gain 1 life.",
        "Untap up to 2 artifacts or creatures you control and you gain 1 life.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("selected-untap-list", &tokens);
        let sequence = recognize_effect_sequence(&mut cursor, &[], None).unwrap();
        assert_eq!(sequence.effects.len(), 2);
        let LocalEffectNode::SelectedPermanents(node) = &sequence.effects[0] else {
            panic!("ordinary selected untap");
        };
        assert_eq!(
            node.operand.query.controller,
            if text.contains("you control") {
                Some(ObjectControllerScope::AbilityController)
            } else {
                None
            }
        );
        cursor.finish().unwrap();
    }
}

#[test]
fn targeted_untap_is_the_shared_tap_frame_not_a_selected_collection() {
    // `target` routes to the shared tap verb frame (CR 115.1): the declared
    // slot owns the bounds; the selected-collection child never claims it.
    let tokens = recognize_entities("Untap up to three target lands.");
    let mut cursor = OracleTokenCursor::new("targeted-untap", &tokens);
    assert!(recognize_untap_permanents_effect(&mut cursor).is_err());
    let mut cursor = OracleTokenCursor::new("targeted-untap", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None).unwrap();
    let [LocalEffectNode::TapPermanents(node)] = sequence.effects.as_slice() else {
        panic!("one shared tap-state action: {sequence:?}");
    };
    assert!(node.untap);
    let super::super::PermanentOperandNode::Targets(targets) = &node.targets else {
        panic!("declared target slot: {node:?}");
    };
    assert_eq!(targets.slots.len(), 1);
}

#[test]
fn selected_untap_does_not_invent_targets_or_accept_wrong_domains_and_suffixes() {
    for text in [
        "Untap two lands.",
        "Untap up to three target.",
        "Untap up to three players.",
        "Untap up to three creature cards.",
        "Untap up to three lands or.",
        "Untap up to three lands, then.",
        "Untap up to three lands nonsense.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("selected-untap-negative", &tokens);
        assert!(
            recognize_effect_sequence(&mut cursor, &[], None).is_err(),
            "{text}"
        );
    }
}

#[test]
fn relative_characteristics_preserve_effect_comma_and_reject_a_second_controller() {
    let tokens =
        recognize_entities("Untap up to one creature that's red or green, then you gain 1 life.");
    let mut cursor = OracleTokenCursor::new("selected-untap-relative", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None).unwrap();
    assert_eq!(sequence.effects.len(), 2);
    cursor.finish().unwrap();
    for text in [
        "Untap up to one creature that's red or green you control, then you gain 1 life.",
        "Untap up to one creature that's red or green, then nonsense.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("selected-untap-relative-negative", &tokens);
        assert!(
            recognize_effect_sequence(&mut cursor, &[], None).is_err(),
            "{text}"
        );
    }
}
