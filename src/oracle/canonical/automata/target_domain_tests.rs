use super::super::super::OracleTokenCursor;
use super::super::{
    LocalEffectNode, ObjectCardType, ObjectControllerScope, ZoneChangeDomains,
    ZoneChangeTargetSupport, recognize_zone_change_effect_with_policy,
};
use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn real_return_union_preserves_common_opponent_controller() {
    let tokens = recognize_entities(
        "Return target spell or nonland permanent an opponent controls to its owner's hand.",
    );
    let mut cursor = OracleTokenCursor::new("return-domain-real", &tokens);
    let LocalEffectNode::ZoneChange(effect) = recognize_zone_change_effect_with_policy(
        &mut cursor,
        ZoneChangeTargetSupport::Scalar,
        ZoneChangeDomains::SpellOrBattlefield,
    )
    .unwrap() else {
        panic!("zone change");
    };
    let TargetSelector::ControlledObject {
        predicate,
        controller,
    } = &effect.target.slots[0].selector
    else {
        panic!("shared controller");
    };
    assert_eq!(
        *controller,
        ObjectControllerScope::OpponentOfAbilityController
    );
    let projected =
        project_object_query_domains(predicate, &[ObjectKind::Spell, ObjectKind::Permanent])
            .unwrap();
    assert_eq!(projected.len(), 2);
    assert_eq!(projected[0].domain, ObjectKind::Spell);
    assert_eq!(projected[1].domain, ObjectKind::Permanent);
}

#[test]
fn return_only_domain_extension_rejects_unknown_alternatives_and_bad_suffix() {
    for text in [
        "Destroy target spell or creature.",
        "Exile target spell or creature.",
        "Return target player or creature to its owner's hand.",
        "Return target card or creature to its owner's hand.",
        "Return target spell or creature to its controller's hand.",
        "Return target spell with mana value 3 or less to its owner's hand.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("return-domain-near-miss", &tokens);
        assert!(
            recognize_zone_change_effect_with_policy(
                &mut cursor,
                ZoneChangeTargetSupport::Scalar,
                ZoneChangeDomains::SpellOrBattlefield
            )
            .is_err(),
            "{text}"
        );
    }
}

#[test]
fn positive_nominal_and_explicit_spell_override_are_required() {
    let creature_spell = ObjectPredicate::Intersection(vec![
        ObjectPredicate::CardType(ObjectCardType::Creature),
        ObjectPredicate::ObjectKind(ObjectKind::Spell),
    ]);
    let projected =
        project_object_query_domains(&creature_spell, &[ObjectKind::Spell, ObjectKind::Permanent])
            .unwrap();
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].domain, ObjectKind::Spell);
    let negative_only =
        ObjectPredicate::Not(Box::new(ObjectPredicate::CardType(ObjectCardType::Land)));
    assert_eq!(
        project_object_query_domains(&negative_only, &[ObjectKind::Spell, ObjectKind::Permanent]),
        Err(TargetDomainProjectionError::MissingPositiveNominal)
    );
    let hidden_card = ObjectPredicate::Union(vec![
        creature_spell,
        ObjectPredicate::ObjectKind(ObjectKind::Card),
    ]);
    assert_eq!(
        project_object_query_domains(&hidden_card, &[ObjectKind::Spell, ObjectKind::Permanent]),
        Err(TargetDomainProjectionError::UnsupportedDomain)
    );
}

#[test]
fn mixed_domain_distribution_has_explicit_cap_but_same_domain_unions_stay_intact() {
    let spell = ObjectPredicate::ObjectKind(ObjectKind::Spell);
    let permanent = ObjectPredicate::ObjectKind(ObjectKind::Permanent);
    let mixed = ObjectPredicate::Union(
        (0..33)
            .map(|i| {
                if i % 2 == 0 {
                    spell.clone()
                } else {
                    permanent.clone()
                }
            })
            .collect(),
    );
    assert_eq!(
        project_object_query_domains(&mixed, &[ObjectKind::Spell, ObjectKind::Permanent]),
        Err(TargetDomainProjectionError::TooManyAlternatives)
    );
    let same = ObjectPredicate::Union(vec![spell; 100]);
    assert_eq!(
        project_object_query_domains(&same, &[ObjectKind::Spell, ObjectKind::Permanent]).unwrap()
            [0]
        .predicate,
        same
    );
}
