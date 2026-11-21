use crate::oracle::syntax::recognize_entities;

use super::super::{CardColor, LocalEffectNode, recognize_effect_sequence};
use super::{
    CreatureSubtype, EffectDurationNode, GrantedKeyword, LocalObjectReference, ObjectCardType,
    OracleTokenCursor, PowerToughnessNode,
};

#[test]
fn temporary_animation_composes_registered_characteristic_children() {
    let tokens = recognize_entities(
        "Until end of turn, this land becomes a 4/4 blue and black Shark creature with deathtouch. It's still a land.",
    );
    let mut cursor = OracleTokenCursor::new("test.temporary-animation", &tokens);
    let sequence =
        recognize_effect_sequence(&mut cursor, &[], None).expect("animation effect parses");
    let [LocalEffectNode::AnimateSource(animation)] = sequence.effects.as_slice() else {
        panic!("expected one temporary animation effect");
    };
    assert_eq!(animation.source, LocalObjectReference::SourceSelf);
    assert_eq!(
        animation.duration,
        Some(EffectDurationNode::UntilEndOfCurrentTurn)
    );
    assert_eq!(
        animation.power_toughness,
        PowerToughnessNode {
            power: 4,
            toughness: 4,
        }
    );
    assert_eq!(animation.colors.colors, [CardColor::Blue, CardColor::Black]);
    assert_eq!(animation.subtypes, [CreatureSubtype::Shark]);
    assert_eq!(animation.card_types, [ObjectCardType::Creature]);
    assert_eq!(animation.keywords, [GrantedKeyword::Deathtouch]);
    assert!(!animation.all_creature_types);
    assert_eq!(animation.retained_card_types, [ObjectCardType::Land]);
}

#[test]
fn permanent_animation_allows_empty_color_and_subtype_sets_and_all_creature_types() {
    let tokens = recognize_entities(
        "This land becomes a 3/3 creature with vigilance and all creature types. It's still a land.",
    );
    let mut cursor = OracleTokenCursor::new("test.permanent-animation", &tokens);
    let sequence =
        recognize_effect_sequence(&mut cursor, &[], None).expect("animation effect parses");
    let [LocalEffectNode::AnimateSource(animation)] = sequence.effects.as_slice() else {
        panic!("expected one source animation effect");
    };
    assert_eq!(animation.duration, None);
    assert!(animation.colors.colors.is_empty());
    assert!(animation.subtypes.is_empty());
    assert_eq!(animation.card_types, [ObjectCardType::Creature]);
    assert_eq!(animation.keywords, [GrantedKeyword::Vigilance]);
    assert!(animation.all_creature_types);
    assert_eq!(animation.retained_card_types, [ObjectCardType::Land]);
}

#[test]
fn unregistered_animation_subtype_fails_locally() {
    let registered = recognize_entities(
        "Until end of turn, this land becomes a 4/4 blue and black Fish creature with deathtouch. It's still a land.",
    );
    let mut registered_cursor = OracleTokenCursor::new("test.registered-fish", &registered);
    let sequence = recognize_effect_sequence(&mut registered_cursor, &[], None)
        .expect("Fish is a registered creature subtype");
    let [LocalEffectNode::AnimateSource(animation)] = sequence.effects.as_slice() else {
        panic!("expected one ordinary source animation")
    };
    assert_eq!(
        animation
            .subtypes
            .iter()
            .map(|subtype| subtype.ir_name())
            .collect::<Vec<_>>(),
        vec!["Fish"]
    );
    registered_cursor.finish().unwrap();
    let tokens = recognize_entities(
        "Until end of turn, this land becomes a 4/4 blue and black QzxvUnregisteredSubtype creature with deathtouch. It's still a land.",
    );
    let mut cursor = OracleTokenCursor::new("test.invalid-animation-subtype", &tokens);
    let failure = recognize_effect_sequence(&mut cursor, &[], None)
        .expect_err("unknown subtype must remain unsupported");
    assert_eq!(failure.actual.as_deref(), Some("QzxvUnregisteredSubtype"));
    assert!(failure.expected.contains("registered creature subtype"));
}
