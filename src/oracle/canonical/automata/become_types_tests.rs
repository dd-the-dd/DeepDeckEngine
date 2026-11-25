use crate::oracle::syntax::recognize_entities;

use super::super::{LocalEffectNode, recognize_effect_sequence};
use super::{EffectDurationNode, ObjectCardType, OracleTokenCursor, TypeChangeRetention};

fn parse(text: &str) -> super::super::EffectSequenceNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.become-types", &tokens);
    recognize_effect_sequence(&mut cursor, &[], None).expect(text)
}

#[test]
fn bare_subtype_replaces_and_additive_clause_retains() {
    let sequence = parse("Until end of turn, target creature becomes a Doctor.");
    let [LocalEffectNode::BecomeTypes(node)] = sequence.effects.as_slice() else {
        panic!("one type change");
    };
    assert_eq!(
        node.subtypes
            .iter()
            .map(|subtype| subtype.ir_name())
            .collect::<Vec<_>>(),
        ["Doctor"]
    );
    assert!(node.card_types.is_empty());
    assert_eq!(node.retention, TypeChangeRetention::ReplacesSameClass);
    assert_eq!(
        node.duration,
        Some(EffectDurationNode::UntilEndOfCurrentTurn)
    );

    let sequence = parse("Target creature becomes an artifact in addition to its other types.");
    let [LocalEffectNode::BecomeTypes(node)] = sequence.effects.as_slice() else {
        panic!("one type change");
    };
    assert_eq!(node.card_types, [ObjectCardType::Artifact]);
    assert_eq!(node.retention, TypeChangeRetention::InAdditionToOtherTypes);
    assert_eq!(node.duration, None);
}

#[test]
fn switch_and_base_power_alone_are_complements_of_the_same_frame() {
    let sequence = parse("Switch target creature's power and toughness until end of turn.");
    let [LocalEffectNode::BecomeTypes(node)] = sequence.effects.as_slice() else {
        panic!("one characteristic change");
    };
    assert!(node.characteristics.switches_power_toughness);
    assert!(matches!(
        node.objects,
        super::super::PermanentOperandNode::Targets(_)
    ));
    assert_eq!(
        node.duration,
        Some(EffectDurationNode::UntilEndOfCurrentTurn)
    );

    let sequence = parse("Target creature has base power 0 until end of turn.");
    let [LocalEffectNode::BecomeTypes(node)] = sequence.effects.as_slice() else {
        panic!("one characteristic change");
    };
    assert!(node.characteristics.base_power.is_some());
    assert!(node.base_power_toughness().is_none());
}

#[test]
fn copula_complement_reads_statistics_colors_and_retention_sentences() {
    use super::super::TypeChangeNumber;
    for (text, number, rest) in [
        (
            "a 3/3 black Ooze creature. It's still a land.",
            TypeChangeNumber::Singular,
            Some("."),
        ),
        (
            "1/1 creatures that are still lands.",
            TypeChangeNumber::Plural,
            Some("."),
        ),
        (
            "2/2 creatures with first strike. They're still lands.",
            TypeChangeNumber::Plural,
            Some("."),
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.copula-complement", &tokens);
        let node =
            super::recognize_characteristic_copula_complement(&mut cursor, number).expect(text);
        assert_eq!(
            node.retention,
            TypeChangeRetention::StillA(ObjectCardType::Land),
            "{text}"
        );
        assert!(node.power_toughness.is_some(), "{text}");
        assert!(
            node.card_types.contains(&ObjectCardType::Creature),
            "{text}"
        );
        assert_eq!(cursor.peek(), rest, "{text}");
    }
    // Near miss: a singular pronoun cannot retain the types of plural objects.
    let tokens = recognize_entities("1/1 creatures. It's still a land.");
    let mut cursor = OracleTokenCursor::new("test.copula-complement", &tokens);
    assert!(
        super::recognize_characteristic_copula_complement(&mut cursor, TypeChangeNumber::Plural)
            .is_err()
    );
}
