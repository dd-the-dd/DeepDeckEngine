use crate::oracle::syntax::recognize_entities;

use super::super::{
    ObjectCardType, ObjectKind, ObjectPredicate, PlayerPredicate, TargetSelector,
    recognize_damage_effect,
};
use super::{
    OracleTokenCursor, recognize_card_type_union_selector, recognize_target_slot_selector,
};

#[test]
fn player_or_planeswalker_is_one_cross_domain_target_slot() {
    let tokens = recognize_entities("player or planeswalker");
    let mut cursor = OracleTokenCursor::new("selector-union-test", &tokens);

    let selector = recognize_target_slot_selector(&mut cursor).expect("cross-domain union parses");

    assert_eq!(
        selector,
        TargetSelector::Union(vec![
            TargetSelector::Player(PlayerPredicate::Any),
            TargetSelector::Object(ObjectPredicate::CardType(ObjectCardType::Planeswalker)),
        ])
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn damage_rejects_a_cross_domain_union_with_a_nondamageable_branch() {
    let tokens = recognize_entities("This spell deals 4 damage to target player or spell.");
    let mut cursor = OracleTokenCursor::new("selector-union-test", &tokens);

    let failure = recognize_damage_effect(&mut cursor, &[], None)
        .expect_err("a spell cannot be one branch of a damage recipient union");

    assert!(failure.expected.contains("damageable"));
}

#[test]
fn instant_or_sorcery_is_one_boolean_card_predicate() {
    let tokens = recognize_entities("instant or sorcery card");
    let mut cursor = OracleTokenCursor::new("selector-union-test", &tokens);

    let predicate = recognize_card_type_union_selector(&mut cursor).expect("type union parses");

    assert_eq!(
        predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::ObjectKind(ObjectKind::Card),
            ObjectPredicate::Union(vec![
                ObjectPredicate::CardType(ObjectCardType::Instant),
                ObjectPredicate::CardType(ObjectCardType::Sorcery),
            ]),
        ])
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn unsupported_type_branch_fails_at_that_branch() {
    let tokens = recognize_entities("instant or lesson card");
    let mut cursor = OracleTokenCursor::new("selector-union-test", &tokens);

    let failure = recognize_card_type_union_selector(&mut cursor)
        .expect_err("unregistered type must not be accepted as a card type");

    assert_eq!(failure.actual.as_deref(), Some("lesson"));
    assert!(failure.expected.contains("card type"));
}

#[test]
fn trailing_spell_domain_scopes_the_complete_type_union() {
    let tokens = recognize_entities("artifact or enchantment spell");
    let mut cursor = OracleTokenCursor::new("selector-union-test", &tokens);

    let selector = recognize_target_slot_selector(&mut cursor).expect("spell union parses");

    assert_eq!(
        selector,
        super::TargetSelector::Object(ObjectPredicate::Intersection(vec![
            ObjectPredicate::Union(vec![
                ObjectPredicate::CardType(ObjectCardType::Artifact),
                ObjectPredicate::CardType(ObjectCardType::Enchantment),
            ]),
            ObjectPredicate::ObjectKind(ObjectKind::Spell),
        ]))
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn oxford_complex_branches_keep_intersections_and_share_controller_scope() {
    let tokens = recognize_entities("artifact, enchantment, or nonbasic land an opponent controls");
    let mut cursor = OracleTokenCursor::new("selector-union-test", &tokens);

    let selector = recognize_target_slot_selector(&mut cursor).expect("Oxford target parses");

    assert_eq!(
        selector,
        super::TargetSelector::ControlledObject {
            predicate: ObjectPredicate::Union(vec![
                ObjectPredicate::CardType(ObjectCardType::Artifact),
                ObjectPredicate::CardType(ObjectCardType::Enchantment),
                ObjectPredicate::Intersection(vec![
                    ObjectPredicate::Not(Box::new(ObjectPredicate::Supertype(
                        super::CardSupertype::Basic,
                    ))),
                    ObjectPredicate::CardType(ObjectCardType::Land),
                ]),
            ]),
            controller: super::ObjectControllerScope::OpponentOfAbilityController,
        }
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn a_trailing_permanent_noun_keeps_owning_coordinated_subtype_heads() {
    // CR 109.2 inference must not change the coordination shape: the
    // trailing noun still scopes both subtypes.
    let tokens = recognize_entities("Elf or Goblin permanent");
    let mut cursor = OracleTokenCursor::new("selector", &tokens);
    let query = super::recognize_object_query(&mut cursor).expect("query parses");
    assert_eq!(
        query.predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::Union(vec![
                ObjectPredicate::Subtype("Elf".to_string()),
                ObjectPredicate::Subtype("Goblin".to_string()),
            ]),
            ObjectPredicate::ObjectKind(ObjectKind::Permanent),
        ])
    );
}
