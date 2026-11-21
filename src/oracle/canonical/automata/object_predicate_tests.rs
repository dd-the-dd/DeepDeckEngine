use super::*;

fn permanent_type() -> ObjectPredicate {
    ObjectPredicate::CardType(ObjectCardType::Creature)
}

fn nonpermanent_type() -> ObjectPredicate {
    ObjectPredicate::CardType(ObjectCardType::Instant)
}

fn characteristic() -> ObjectPredicate {
    ObjectPredicate::Color(CardColor::Blue)
}

#[test]
fn supports_permanent_domain_with_the_former_validator_leaf_policy() {
    for predicate in [
        ObjectPredicate::CardType(ObjectCardType::Artifact),
        ObjectPredicate::CardType(ObjectCardType::Battle),
        ObjectPredicate::CardType(ObjectCardType::Creature),
        ObjectPredicate::CardType(ObjectCardType::Enchantment),
        ObjectPredicate::CardType(ObjectCardType::Land),
        ObjectPredicate::CardType(ObjectCardType::Planeswalker),
        ObjectPredicate::Subtype("Wizard".to_string()),
        ObjectPredicate::CharacteristicBatch(CharacteristicBatch::Outlaw),
        ObjectPredicate::Color(CardColor::Blue),
        ObjectPredicate::Supertype(CardSupertype::Legendary),
        ObjectPredicate::NumericComparison {
            left: SelectorNumericProperty::ManaValue,
            operator: NumericComparisonOperator::LessThanOrEqual,
            right: Box::new(NumericAtom::Integer(2).into()),
        },
        ObjectPredicate::ObjectKind(ObjectKind::Permanent),
    ] {
        assert!(predicate.supports_domain(ObjectKind::Permanent));
    }

    for predicate in [
        ObjectPredicate::CardType(ObjectCardType::Instant),
        ObjectPredicate::CardType(ObjectCardType::Sorcery),
        ObjectPredicate::ObjectKind(ObjectKind::Card),
        ObjectPredicate::ObjectKind(ObjectKind::Spell),
    ] {
        assert!(!predicate.supports_domain(ObjectKind::Permanent));
    }
}

#[test]
fn supports_domain_requires_nonempty_fully_compatible_boolean_nodes() {
    assert!(
        ObjectPredicate::Not(Box::new(characteristic())).supports_domain(ObjectKind::Permanent)
    );
    assert!(
        ObjectPredicate::Intersection(vec![permanent_type(), characteristic()])
            .supports_domain(ObjectKind::Permanent)
    );
    assert!(
        ObjectPredicate::Union(vec![permanent_type(), characteristic()])
            .supports_domain(ObjectKind::Permanent)
    );

    for predicate in [
        ObjectPredicate::Not(Box::new(nonpermanent_type())),
        ObjectPredicate::Intersection(vec![permanent_type(), nonpermanent_type()]),
        ObjectPredicate::Union(vec![permanent_type(), nonpermanent_type()]),
        ObjectPredicate::Intersection(vec![]),
        ObjectPredicate::Union(vec![]),
    ] {
        assert!(!predicate.supports_domain(ObjectKind::Permanent));
    }
}

#[test]
fn supports_card_and_spell_domains_without_confusing_explicit_domains() {
    for domain in [ObjectKind::Card, ObjectKind::Spell] {
        assert!(permanent_type().supports_domain(domain));
        assert!(nonpermanent_type().supports_domain(domain));
        assert!(characteristic().supports_domain(domain));
        assert!(ObjectPredicate::ObjectKind(domain).supports_domain(domain));

        for other_domain in [ObjectKind::Card, ObjectKind::Permanent, ObjectKind::Spell] {
            assert_eq!(
                ObjectPredicate::ObjectKind(other_domain).supports_domain(domain),
                other_domain == domain
            );
        }
    }
}

#[test]
fn has_domain_distinguishes_anchors_from_compatible_characteristics() {
    assert!(permanent_type().has_domain(ObjectKind::Permanent));
    assert!(!nonpermanent_type().has_domain(ObjectKind::Permanent));
    assert!(!characteristic().has_domain(ObjectKind::Permanent));

    for domain in [ObjectKind::Card, ObjectKind::Permanent, ObjectKind::Spell] {
        assert!(ObjectPredicate::ObjectKind(domain).has_domain(domain));
        assert!(
            !ObjectPredicate::ObjectKind(domain).has_domain(match domain {
                ObjectKind::Card => ObjectKind::Spell,
                ObjectKind::Permanent | ObjectKind::Spell => ObjectKind::Card,
            })
        );
    }
}

#[test]
fn has_domain_uses_existential_intersections_and_exhaustive_unions() {
    assert!(ObjectPredicate::Not(Box::new(permanent_type())).has_domain(ObjectKind::Permanent));
    assert!(
        ObjectPredicate::Intersection(vec![characteristic(), permanent_type()])
            .has_domain(ObjectKind::Permanent)
    );
    assert!(
        ObjectPredicate::Union(vec![
            permanent_type(),
            ObjectPredicate::ObjectKind(ObjectKind::Permanent),
        ])
        .has_domain(ObjectKind::Permanent)
    );

    for predicate in [
        ObjectPredicate::Intersection(vec![characteristic()]),
        ObjectPredicate::Union(vec![permanent_type(), characteristic()]),
        ObjectPredicate::Intersection(vec![]),
        ObjectPredicate::Union(vec![]),
    ] {
        assert!(!predicate.has_domain(ObjectKind::Permanent));
    }
}
#[test]
fn token_characteristic_is_a_domain_neutral_permanent_qualifier() {
    for predicate in [
        ObjectPredicate::IsToken,
        ObjectPredicate::Not(Box::new(ObjectPredicate::IsToken)),
    ] {
        assert!(predicate.supports_domain(ObjectKind::Permanent));
        assert!(!predicate.has_domain(ObjectKind::Permanent));
    }

    let nontoken_creature = ObjectPredicate::Intersection(vec![
        ObjectPredicate::Not(Box::new(ObjectPredicate::IsToken)),
        ObjectPredicate::CardType(ObjectCardType::Creature),
    ]);
    assert!(nontoken_creature.supports_domain(ObjectKind::Permanent));
    assert!(nontoken_creature.has_domain(ObjectKind::Permanent));
}

#[test]
fn nominal_heads_distinguish_object_nouns_from_bare_characteristics() {
    for predicate in [
        ObjectPredicate::IsCommander,
        ObjectPredicate::IsToken,
        ObjectPredicate::CardType(ObjectCardType::Creature),
        ObjectPredicate::Subtype("Beast".to_string()),
        ObjectPredicate::ObjectKind(ObjectKind::Permanent),
        ObjectPredicate::Not(Box::new(ObjectPredicate::CardType(
            ObjectCardType::Creature,
        ))),
    ] {
        assert!(predicate.has_nominal_head(), "{predicate:?}");
    }

    for predicate in [
        ObjectPredicate::CharacteristicBatch(CharacteristicBatch::Outlaw),
        ObjectPredicate::Color(CardColor::Green),
        ObjectPredicate::Supertype(CardSupertype::Legendary),
        ObjectPredicate::HasKeyword(GrantedKeyword::Flying),
        ObjectPredicate::NumericComparison {
            left: SelectorNumericProperty::ManaValue,
            operator: NumericComparisonOperator::Equal,
            right: Box::new(NumericAtom::Integer(2).into()),
        },
    ] {
        assert!(!predicate.has_nominal_head(), "{predicate:?}");
    }
}

#[test]
fn nominal_heads_compose_existentially_in_intersections_and_exhaustively_in_unions() {
    assert!(
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::Color(CardColor::Green),
            ObjectPredicate::Subtype("Beast".to_string()),
        ])
        .has_nominal_head()
    );
    assert!(
        ObjectPredicate::Union(vec![
            ObjectPredicate::Subtype("Beast".to_string()),
            ObjectPredicate::CardType(ObjectCardType::Artifact),
        ])
        .has_nominal_head()
    );

    for predicate in [
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::Color(CardColor::Green),
            ObjectPredicate::Supertype(CardSupertype::Legendary),
        ]),
        ObjectPredicate::Union(vec![
            ObjectPredicate::Subtype("Beast".to_string()),
            ObjectPredicate::Color(CardColor::Green),
        ]),
        ObjectPredicate::Intersection(vec![]),
        ObjectPredicate::Union(vec![]),
    ] {
        assert!(!predicate.has_nominal_head(), "{predicate:?}");
    }
}

#[test]
fn expression_rhs_visits_x_inside_qualified_nominal_predicates() {
    let nominal = ObjectPredicate::NumericComparison {
        left: SelectorNumericProperty::ManaValue,
        operator: NumericComparisonOperator::LessThanOrEqual,
        right: Box::new(NumericAtom::VariableX.into()),
    };
    let expression = NumericExpression::QualifiedTargetProperty {
        object: super::super::TargetBinding {
            effect_index: 0,
            slot_index: 0,
        },
        required_nominal: nominal,
        property: SelectorNumericProperty::ManaValue,
    };
    assert!(expression.uses_x());
    assert!(super::super::object_predicate_uses_x(
        &ObjectPredicate::NumericComparison {
            left: SelectorNumericProperty::ManaValue,
            operator: NumericComparisonOperator::Equal,
            right: Box::new(NumericExpression::add(
                expression,
                NumericExpression::from(1)
            )),
        }
    ));
}
