use crate::oracle::canonical::{
    CardZoneChangeAction, CardZoneDestination, ExpressedTargetCardinality, NumericAtom,
    ObjectCardType, ObjectKind, ObjectPredicate, OracleTokenCursor, TargetCardSourceZone,
    recognize_target_card_zone_change_effect,
};
use crate::oracle::syntax::recognize_entities;

fn parse(text: &str) -> super::TargetCardZoneChangeEffectNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.target-card-zone-change", &tokens);
    let node = recognize_target_card_zone_change_effect(&mut cursor).expect("effect parses");
    cursor.finish().expect("effect consumes all tokens");
    node
}

#[test]
fn bare_card_is_the_generic_card_domain_not_a_card_type() {
    let node = parse("Return target card from your graveyard to your hand");

    assert_eq!(
        node.predicate,
        ObjectPredicate::ObjectKind(ObjectKind::Card)
    );
}

#[test]
fn parses_an_optional_plural_target_collection_across_graveyards() {
    let node = parse("Exile up to two target cards from graveyards");

    assert_eq!(
        node.cardinality,
        ExpressedTargetCardinality {
            minimum: NumericAtom::Integer(0),
            maximum: NumericAtom::Integer(2),
        }
    );
    assert_eq!(node.source_zone, TargetCardSourceZone::AnyGraveyard);
}

#[test]
fn preserves_the_exact_single_card_form() {
    let node = parse("Exile target card from any graveyard");

    assert_eq!(node.cardinality, ExpressedTargetCardinality::EXACTLY_ONE);
}

#[test]
fn distinguishes_a_collection_constrained_to_one_graveyard() {
    let node = parse("Exile up to two target cards from a single graveyard");

    assert_eq!(node.source_zone, TargetCardSourceZone::SingleGraveyard);
    assert_eq!(node.cardinality.minimum, NumericAtom::Integer(0));
    assert_eq!(node.cardinality.maximum, NumericAtom::Integer(2));
}

#[test]
fn variable_collection_cardinality_reuses_the_shared_expressed_state() {
    let node = parse("Exile up to X target cards from graveyards");

    assert_eq!(node.cardinality.minimum, NumericAtom::Integer(0));
    assert_eq!(node.cardinality.maximum, NumericAtom::VariableX);
}

#[test]
fn unknown_graveyard_scope_fails_before_the_zone_is_accepted() {
    let tokens = recognize_entities("Exile up to two target cards from neighboring graveyards");
    let mut cursor = OracleTokenCursor::new("test.target-card-zone-change", &tokens);

    let failure = recognize_target_card_zone_change_effect(&mut cursor)
        .expect_err("an unknown scope must not be swallowed");

    assert_eq!(
        failure.state,
        "effect.target-card-zone-change.source-article"
    );
    assert_eq!(failure.actual.as_deref(), Some("neighboring"));
}

#[test]
fn graveyard_articles_and_zone_number_must_agree() {
    for text in [
        "Exile target card from any graveyards",
        "Exile up to two target cards from a graveyards",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.target-card-zone-change", &tokens);

        let failure = recognize_target_card_zone_change_effect(&mut cursor)
            .expect_err("a singular article must not accept a plural zone noun");

        assert_eq!(failure.state, "effect.target-card-zone-change.source-zone");
        assert_eq!(failure.actual.as_deref(), Some("graveyards"));
    }
}

#[test]
fn return_to_controller_hand_reuses_the_shared_card_selector() {
    let node = parse("Return target artifact or enchantment card from your graveyard to your hand");

    assert_eq!(
        node.action,
        CardZoneChangeAction::Move(CardZoneDestination::ControllerHand)
    );
    assert_eq!(node.source_zone, TargetCardSourceZone::ControllerGraveyard);
    assert_eq!(node.cardinality, ExpressedTargetCardinality::EXACTLY_ONE);
    assert_eq!(
        node.predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::ObjectKind(ObjectKind::Card),
            ObjectPredicate::Union(vec![
                ObjectPredicate::CardType(ObjectCardType::Artifact),
                ObjectPredicate::CardType(ObjectCardType::Enchantment),
            ]),
        ])
    );
}

#[test]
fn put_from_any_graveyard_retains_the_target_owner_destination() {
    for text in [
        "Put target card from a graveyard on top of its owner's library",
        "Put target card from a graveyard on the top of its owner’s library",
    ] {
        let node = parse(text);

        assert_eq!(
            node.action,
            CardZoneChangeAction::Move(CardZoneDestination::TargetOwnerLibraryTop)
        );
        assert_eq!(node.source_zone, TargetCardSourceZone::AnyGraveyard);
        assert_eq!(
            node.predicate,
            ObjectPredicate::ObjectKind(ObjectKind::Card)
        );
    }
}

#[test]
fn targeted_card_moves_reject_incomplete_zone_relations() {
    for (text, expected_state) in [
        (
            "Return target card from your graveyard your hand",
            "effect.target-card-move.return.destination-relation",
        ),
        (
            "Put target card from a graveyard on top of its library",
            "effect.target-card-move.put.destination-owner",
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.target-card-zone-change", &tokens);
        let failure = recognize_target_card_zone_change_effect(&mut cursor)
            .expect_err("an incomplete zone relation must remain unsupported");

        assert_eq!(failure.state, expected_state);
    }
}
