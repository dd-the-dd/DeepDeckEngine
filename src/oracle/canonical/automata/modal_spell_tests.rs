use crate::oracle::syntax::{NameLinkKind, ResolvedNameLink, recognize_entities};

use super::{
    LocalEffectNode, ModalPrimitiveActionNode, NumericAtom, OracleTokenCursor, SelectorPredicate,
    TargetSelector, recognize_effect_sequence_modal, recognize_primitive_modal,
};

fn parse(text: &str) -> super::PrimitiveModalNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.primitive-modal", &tokens);
    recognize_primitive_modal(&mut cursor, &[], None).expect("primitive modal parses")
}

#[test]
fn exact_modal_parent_retains_cardinality_and_delegates_exile_modes() {
    let modal = parse(
        "Choose two — • Exile target creature with mana value X or less. • Exile up to X target cards from graveyards.",
    );

    assert_eq!(modal.cardinality.minimum, 2);
    assert_eq!(modal.cardinality.maximum, 2);
    assert!(!modal.cardinality.allow_repeated);
    assert!(matches!(
        modal.modes[0],
        ModalPrimitiveActionNode::ZoneChange(_)
    ));
    assert!(matches!(
        modal.modes[1],
        ModalPrimitiveActionNode::TargetCardZoneChange(_)
    ));
    assert!(modal.modes.iter().all(ModalPrimitiveActionNode::uses_x));

    let ModalPrimitiveActionNode::ZoneChange(zone_change) = &modal.modes[0] else {
        unreachable!()
    };
    let TargetSelector::Object(SelectorPredicate::Intersection(predicates)) =
        &zone_change.target.slots[0].selector
    else {
        panic!("creature and mana-value predicate stay intersected")
    };
    assert!(predicates.iter().any(|predicate| matches!(
        predicate,
        SelectorPredicate::NumericComparison {
            right,
            ..
        } if right.uses_x()
    )));
}

#[test]
fn unknown_modal_action_fails_in_the_action_dispatch_state() {
    let tokens = recognize_entities("Choose one — • Target player sings. • Exile target creature.");
    let mut cursor = OracleTokenCursor::new("test.primitive-modal", &tokens);

    let failure = recognize_primitive_modal(&mut cursor, &[], None)
        .expect_err("an unknown action must not fall into keyword grants");

    assert_eq!(failure.state, "modal.mode.action");
    assert_eq!(failure.actual.as_deref(), Some("Target"));
}

#[test]
fn choose_one_damage_and_keyword_grant_keep_the_existing_generic_children() {
    let modal = parse(
        "Choose one — • This spell deals 3 damage to any target. • Target creature gains flying until end of turn.",
    );

    assert_eq!(modal.cardinality.minimum, 1);
    assert_eq!(modal.cardinality.maximum, 1);
    assert!(matches!(
        modal.modes[0],
        ModalPrimitiveActionNode::Damage(_)
    ));
    assert!(matches!(
        modal.modes[1],
        ModalPrimitiveActionNode::TemporaryKeywordGrant(_)
    ));
}

#[test]
fn graveyard_card_exile_dispatch_handles_exact_and_literal_cardinalities() {
    let modal = parse(
        "Choose one — • Exile target card from any graveyard. • Exile two target cards from graveyards.",
    );

    assert!(
        modal
            .modes
            .iter()
            .all(|mode| matches!(mode, ModalPrimitiveActionNode::TargetCardZoneChange(_)))
    );
}

#[test]
fn effect_sequence_parent_delegates_any_literal_number_of_collection_modes() {
    let tokens = recognize_entities(
        "Choose three — • Destroy all artifacts. • Destroy all enchantments. • Destroy all creatures.",
    );
    let mut cursor = OracleTokenCursor::new("test.effect-sequence-modal", &tokens);
    let modal = recognize_effect_sequence_modal(&mut cursor, &[], None)
        .expect("three ordinary effect-sequence modes parse");

    assert_eq!(modal.cardinality.minimum, 3);
    assert_eq!(modal.cardinality.maximum, 3);
    assert!(!modal.cardinality.allow_repeated);
    assert_eq!(modal.modes.len(), 3);
    assert!(modal.modes.iter().all(|mode| {
        matches!(
            mode.effects.as_slice(),
            [LocalEffectNode::DestroyCollection(_)]
        )
    }));
}

#[test]
fn effect_sequence_parent_reports_trailing_mode_tokens_from_the_child() {
    let tokens =
        recognize_entities("Choose one — • Destroy all artifacts. Sing. • Destroy all creatures.");
    let mut cursor = OracleTokenCursor::new("test.effect-sequence-modal", &tokens);
    let failure = recognize_effect_sequence_modal(&mut cursor, &[], None)
        .expect_err("a bullet must be one fully consumed effect sequence");

    assert_eq!(failure.automaton, "modal-effect-sequence-mode");
    assert_eq!(failure.actual.as_deref(), Some("Sing"));
}

#[test]
fn mode_subcursor_rebases_current_card_name_links() {
    let tokens = recognize_entities(
        "Choose one — • Example Blast deals 3 damage to target creature. • Destroy target artifact.",
    );
    let token_start = tokens
        .iter()
        .position(|token| token.raw == "Example")
        .expect("name begins");
    let token_end = token_start + 2;
    let links = [ResolvedNameLink {
        kind: NameLinkKind::CurrentCard,
        canonical_name: "Example Blast".to_string(),
        raw: "Example Blast".to_string(),
        token_start,
        token_end,
        span_start: tokens[token_start].span_start,
        span_end: tokens[token_end - 1].span_end,
    }];
    let mut cursor = OracleTokenCursor::new("test.effect-sequence-modal-name", &tokens);
    let modal = recognize_effect_sequence_modal(&mut cursor, &links, None)
        .expect("the bullet-local source link must retain its meaning");

    assert!(matches!(
        modal.modes[0].effects.as_slice(),
        [LocalEffectNode::DealDamage(_)]
    ));
    assert!(matches!(
        modal.modes[1].effects.as_slice(),
        [LocalEffectNode::ZoneChange(_)]
    ));
}
