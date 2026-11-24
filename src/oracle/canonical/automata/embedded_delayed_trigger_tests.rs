use crate::oracle::syntax::recognize_entities;

use super::super::{EffectSequenceNode, OracleTokenCursor, recognize_effect_sequence};
use super::{
    CreatedRuleLifetime, DelayedTriggerEventNode, EffectCreatedRuleNode, ObjectDeparture,
    effect_created_rule_starts,
};

fn parse(text: &str) -> Result<EffectSequenceNode, String> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.created-rule", &tokens);
    recognize_effect_sequence(&mut cursor, &[], None).map_err(|failure| failure.state.to_string())
}

fn only_rule(text: &str) -> (usize, usize, EffectCreatedRuleNode) {
    let sequence = parse(text).unwrap_or_else(|state| panic!("{text}: {state}"));
    let [frame] = sequence.delayed.as_slice() else {
        panic!("{text}: one created rule, got {:?}", sequence.delayed);
    };
    (frame.first_effect, frame.end_effect, frame.rule.clone())
}

#[test]
fn a_bound_object_departure_governs_the_rest_of_its_sentence() {
    for (text, departure) in [
        (
            "Choose target creature an opponent controls. When that creature dies this turn, you gain 3 life. Target creature you control fights that creature.",
            ObjectDeparture::Dies,
        ),
        // Unseen combination: another departure verb and another action.
        (
            "Target creature gets +2/+2 until end of turn. When that creature leaves the battlefield this turn, draw a card.",
            ObjectDeparture::LeavesBattlefield,
        ),
    ] {
        let (first, end, rule) = only_rule(text);
        assert_eq!((first, end), (1, 2), "{text}");
        let EffectCreatedRuleNode::Trigger(trigger) = rule else {
            panic!("{text}: a delayed trigger");
        };
        assert_eq!(trigger.lifetime, CreatedRuleLifetime::ThisTurn);
        assert!(
            matches!(
                trigger.event,
                DelayedTriggerEventNode::BoundObjectDeparture { departure: read, .. } if read == departure
            ),
            "{text}"
        );
    }
}

#[test]
fn the_next_spell_trigger_reads_its_own_cast_record() {
    for text in [
        "When you next cast an instant or sorcery spell this turn, copy that spell. You may choose new targets for the copy.",
        "When you cast your next instant or sorcery spell this turn, copy that spell.",
    ] {
        let (first, end, rule) = only_rule(text);
        assert_eq!((first, end), (0, 1), "{text}");
        assert!(
            matches!(
                rule,
                EffectCreatedRuleNode::Trigger(ref trigger)
                    if matches!(trigger.event, DelayedTriggerEventNode::NextSpellCast { .. })
            ),
            "{text}"
        );
    }
}

#[test]
fn a_created_replacement_requires_its_closing_instead() {
    let (first, end, rule) = only_rule(
        "Target creature gets -2/-2 until end of turn. If that creature would die this turn, exile it instead.",
    );
    assert_eq!((first, end), (1, 2));
    assert!(matches!(
        rule,
        EffectCreatedRuleNode::Replacement(ref replacement) if replacement.instead
    ));
}

#[test]
fn unbound_unscoped_or_repeating_rules_fail_explicitly() {
    for (text, state) in [
        // No antecedent names `that creature`.
        (
            "When that creature dies this turn, you gain 3 life.",
            "permanent.subject.nominal-anaphor.antecedent",
        ),
        // A delayed trigger needs its `this turn` lifetime.
        (
            "Target creature gets +1/+1 until end of turn. When that creature dies, you gain 3 life.",
            "delayed-trigger.lifetime",
        ),
        // A created delayed trigger triggers once (CR 603.7c).
        (
            "Target creature gets +1/+1 until end of turn. Whenever that creature dies this turn, you gain 3 life.",
            "created-rule.introducer",
        ),
    ] {
        assert_eq!(parse(text).err().as_deref(), Some(state), "{text}");
    }
}

#[test]
fn outcome_connectors_and_ordinary_conditions_are_not_created_rules() {
    for text in [
        "When you do, draw a card.",
        "If you do, draw a card.",
        "If a creature dealt damage this way would die this turn, exile it instead.",
        "If you control a creature, draw a card.",
    ] {
        let tokens = recognize_entities(text);
        let cursor = OracleTokenCursor::new("test.created-rule", &tokens);
        assert!(!effect_created_rule_starts(&cursor), "{text}");
    }
}

#[test]
fn a_next_damage_shield_reads_the_shared_damage_event() {
    let (first, end, rule) = only_rule(
        "The next time a white source of your choice would deal damage to you this turn, prevent that damage.",
    );
    assert_eq!((first, end), (0, 1));
    let EffectCreatedRuleNode::NextDamage(shield) = rule else {
        panic!("a next-damage shield");
    };
    assert_eq!(shield.lifetime, CreatedRuleLifetime::ThisTurn);
    assert!(shield.event.source.is_some());
    assert_eq!(shield.event.recipients.len(), 1);
    // A source the controller does not choose is another event pattern.
    assert_eq!(
        parse(
            "The next time a white source would deal damage to you this turn, prevent that damage."
        )
        .err()
        .as_deref(),
        Some("replacement.damage-source.choice")
    );
}
