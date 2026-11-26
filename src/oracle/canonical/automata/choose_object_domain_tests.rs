//! The permanent and number domains of the `choose` frame (CR 608.2d): a
//! counted permanent query with an optional `the rest` action, a bounded
//! number, and the explicit failures of the forms that have no binding.
use crate::oracle::syntax::recognize_entities;

use super::super::{ExpressedTargetCardinality, NumericAtom, ObjectControllerScope};
use super::{
    ChoiceDomainNode, ChoicePartition, ChoiceRemainderAction, NumberChoiceNode, OracleTokenCursor,
    recognize_choose_instruction,
};

fn domain(text: &str) -> ChoiceDomainNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.choose-object-domain", &tokens);
    let node = recognize_choose_instruction(&mut cursor).expect(text);
    assert_eq!(cursor.peek(), Some("."), "{text}");
    node.domain
}

fn failure_state(text: &str) -> String {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.choose-object-domain", &tokens);
    recognize_choose_instruction(&mut cursor)
        .expect_err(text)
        .state
        .to_string()
}

/// Positive card shape (Final Showdown, Guard Dogs): one permanent of the
/// chooser's own, read by the shared object query.
#[test]
fn one_controlled_permanent_is_a_counted_resolution_choice() {
    for text in [
        "Choose a creature you control.",
        "Choose a permanent you control.",
        "Choose a creature or planeswalker you control.",
    ] {
        let ChoiceDomainNode::Objects(choice) = domain(text) else {
            panic!("{text}: a permanent choice");
        };
        assert_eq!(
            choice.quantity,
            ExpressedTargetCardinality::EXACTLY_ONE,
            "{text}"
        );
        assert_eq!(
            choice.query.controller,
            Some(ObjectControllerScope::AbilityController),
            "{text}"
        );
        assert!(choice.singular() && choice.remainder.is_none(), "{text}");
    }
    // Sibling (Duneblast): an unrestricted domain, any controller.
    let ChoiceDomainNode::Objects(choice) = domain("Choose up to one creature. Destroy the rest.")
    else {
        panic!("a permanent choice with a remainder");
    };
    assert_eq!(choice.query.controller, None);
    assert_eq!(choice.remainder, Some(ChoiceRemainderAction::Destroy));
}

/// Siblings (Razia's Purification, Archfiend of Depravity, Limited
/// Resources): a literal or `up to` count followed by a noun counts
/// permanents, never modes, and `the rest` is the complement action.
#[test]
fn counted_permanents_and_the_rest_compose() {
    for (text, minimum, maximum, remainder) in [
        (
            "Choose three permanents you control, then sacrifice the rest.",
            3,
            3,
            ChoiceRemainderAction::Sacrifice,
        ),
        (
            "Choose up to two creatures you control, then sacrifice the rest.",
            0,
            2,
            ChoiceRemainderAction::Sacrifice,
        ),
        (
            "Choose five lands you control and sacrifice the rest.",
            5,
            5,
            ChoiceRemainderAction::Sacrifice,
        ),
        (
            "Choose a creature you control. Destroy the rest.",
            1,
            1,
            ChoiceRemainderAction::Destroy,
        ),
    ] {
        let ChoiceDomainNode::Objects(choice) = domain(text) else {
            panic!("{text}: a permanent choice");
        };
        assert_eq!(
            choice.quantity,
            ExpressedTargetCardinality {
                minimum: NumericAtom::Integer(minimum),
                maximum: NumericAtom::Integer(maximum),
            },
            "{text}"
        );
        assert_eq!(choice.remainder, Some(remainder), "{text}");
    }
}

/// Siblings (Selfless Glyphweaver's Deadly Vanity, Urza's Sylex): `destroy
/// all other QUERY` names every permanent of its own query no chooser chose.
#[test]
fn destroy_all_other_names_the_unchosen_members_of_its_query() {
    for text in [
        "Choose a creature or planeswalker, then destroy all other creatures and planeswalkers.",
        "Choose six lands you control. Destroy all other permanents.",
    ] {
        let ChoiceDomainNode::Objects(choice) = domain(text) else {
            panic!("{text}: a permanent choice");
        };
        let Some(ChoiceRemainderAction::DestroyOthers(others)) = choice.remainder else {
            panic!("{text}: a remainder over its own query");
        };
        assert_eq!(others.controller, None, "{text}");
        assert!(!others.exclude_source, "{text}");
    }
}

/// A bounded number (Expel the Interlopers, My Deck is About a Seven).
#[test]
fn a_bounded_number_is_a_resolution_choice() {
    assert_eq!(
        domain("Choose a number between 0 and 10."),
        ChoiceDomainNode::Number(NumberChoiceNode {
            minimum: 0,
            maximum: 10,
        })
    );
    assert_eq!(
        domain("Choose a number from 1 to 5."),
        ChoiceDomainNode::Number(NumberChoiceNode {
            minimum: 1,
            maximum: 5,
        })
    );
}

/// Near-misses: a count closing the clause is still a mode count; cards in
/// another zone, a random selection of a range, one object per coordinated head, an
/// unbounded or object-counted number and a reversed range fail at their
/// own states.
#[test]
fn unsupported_object_and_number_forms_fail_at_their_own_states() {
    for (text, state) in [
        ("Choose one \u{2014}", "choose.domain.mode"),
        ("Choose two.", "choose.domain.mode"),
        // An exact count is picked at random (`choose_instruction.rs`);
        // a range has no random size.
        (
            "Choose up to two creatures at random.",
            "choose.domain.object.selection",
        ),
        // After a choice, `other` names the unchosen permanents; a clause
        // that would read it as a source exclusion is refused.
        (
            "Choose a creature you control. Other creatures you control can't block this turn.",
            "choose.domain.object.other",
        ),
        ("Choose a number.", "choose.domain.number.range"),
        (
            "Choose a number of lands you control.",
            "choose.domain.number.objects",
        ),
        ("Choose a number between 5 and 2.", "choose.domain.number"),
        (
            "Choose a number between 1 and 5 at random.",
            "choose.domain.number.boundary",
        ),
    ] {
        let state_reached = failure_state(text);
        assert!(state_reached.starts_with(state), "{text}: {state_reached}");
    }
    // Cards in another zone (an Alchemy `perpetually` card's choice) and a
    // coordinated list of articled heads (one choice per head) are refused;
    // the failing child keeps its own state.
    for text in [
        "Choose a creature card in your hand.",
        "Choose an artifact, a creature, and a land you control.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.choose-object-domain", &tokens);
        assert!(recognize_choose_instruction(&mut cursor).is_err(), "{text}");
    }
}

/// Sundering Titan: one land per basic land type (CR 305.6), a plural set
/// for `those lands`; sibling (Liliana, Dreadhorde General): one permanent
/// per permanent type (CR 110.4) with `the rest`; and the restated zone
/// `on the battlefield` (Cytoshape).
#[test]
fn one_object_of_each_characteristic_value_is_a_partitioned_choice() {
    let ChoiceDomainNode::Objects(lands) = domain("Choose a land of each basic land type.") else {
        panic!("a partitioned land choice");
    };
    assert_eq!(lands.partition, Some(ChoicePartition::EachBasicLandType));
    assert_eq!(lands.partition.unwrap().parts().len(), 5);
    assert!(!lands.singular());
    let ChoiceDomainNode::Objects(permanents) =
        domain("Choose a permanent you control of each permanent type, then sacrifice the rest.")
    else {
        panic!("a partitioned permanent choice");
    };
    assert_eq!(
        permanents.partition,
        Some(ChoicePartition::EachPermanentType)
    );
    assert_eq!(permanents.remainder, Some(ChoiceRemainderAction::Sacrifice));
    let ChoiceDomainNode::Objects(on_battlefield) =
        domain("Choose a nonlegendary creature on the battlefield.")
    else {
        panic!("a permanent choice restating its zone");
    };
    assert!(on_battlefield.singular() && on_battlefield.partition.is_none());
    // A characteristic choice keeps its own domain.
    for text in ["Choose a basic land type.", "Choose a land type."] {
        assert_eq!(failure_state(text), "choose.domain.land-type", "{text}");
    }
    // Near-misses: a partition of a counted choice, and an unread value.
    for text in [
        "Choose two lands of each basic land type.",
        "Choose a creature of each color.",
    ] {
        let state = failure_state(text);
        assert!(
            state.starts_with("choose.domain.object.partition"),
            "{text}: {state}"
        );
    }
}
