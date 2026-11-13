use crate::oracle::syntax::recognize_entities;

use super::recognize_activated_card_zone_change;
use crate::oracle::canonical::{
    CardZoneChangeAction, ExpressedTargetCardinality, LocalActivationCostNode, NumericAtom,
    OracleTokenCursor, TargetCardSourceZone,
};

#[test]
fn card_zone_change_composes_cost_action_and_source_zone_children() {
    let tokens = recognize_entities("{2}, {T}, Pay 2 life: Exile target card from any graveyard.");
    let mut cursor = OracleTokenCursor::new("test.activated-card-zone-change", &tokens);

    let node = recognize_activated_card_zone_change(&mut cursor).expect("activation parses");

    assert!(matches!(node.effect.action, CardZoneChangeAction::Exile));
    assert_eq!(
        node.effect.cardinality,
        ExpressedTargetCardinality::EXACTLY_ONE
    );
    assert!(matches!(
        node.effect.source_zone,
        TargetCardSourceZone::AnyGraveyard
    ));
    assert!(matches!(
        node.costs.costs.as_slice(),
        [
            LocalActivationCostNode::PayMana(_),
            LocalActivationCostNode::TapSelf(_),
            LocalActivationCostNode::PayLife(_),
        ]
    ));
}

#[test]
fn discard_self_cost_composes_with_a_plural_target_collection() {
    let tokens =
        recognize_entities("Discard this card: Exile up to two target cards from graveyards.");
    let mut cursor = OracleTokenCursor::new("test.activated-card-zone-change", &tokens);

    let node = recognize_activated_card_zone_change(&mut cursor).expect("activation parses");

    assert!(matches!(
        node.costs.costs.as_slice(),
        [LocalActivationCostNode::DiscardSelf(_)]
    ));
    assert_eq!(
        node.effect.cardinality,
        ExpressedTargetCardinality {
            minimum: NumericAtom::Integer(0),
            maximum: NumericAtom::Integer(2),
        }
    );
}

#[test]
fn unsupported_source_zone_fails_in_the_zone_state() {
    let tokens = recognize_entities("{T}: Exile target card from a library.");
    let mut cursor = OracleTokenCursor::new("test.activated-card-zone-change", &tokens);

    let failure = recognize_activated_card_zone_change(&mut cursor)
        .expect_err("library is outside this source-zone grammar");

    assert_eq!(failure.state, "effect.target-card-zone-change.source-zone");
    assert_eq!(failure.actual.as_deref(), Some("library"));
}

#[test]
fn return_to_hand_composes_mana_tap_and_source_sacrifice_costs() {
    let tokens = recognize_entities(
        "{2}, {T}, Sacrifice this land: Return target artifact card from your graveyard to your hand.",
    );
    let mut cursor = OracleTokenCursor::new("test.activated-card-zone-change", &tokens);

    let node = recognize_activated_card_zone_change(&mut cursor)
        .expect("shared costs and targeted card move should compose");

    assert!(matches!(
        node.costs.costs.as_slice(),
        [
            LocalActivationCostNode::PayMana(_),
            LocalActivationCostNode::TapSelf(_),
            LocalActivationCostNode::SacrificeSelf(_),
        ]
    ));
    assert!(matches!(node.effect.action, CardZoneChangeAction::Move(_)));
    assert_eq!(
        node.effect.source_zone,
        TargetCardSourceZone::ControllerGraveyard
    );
}
