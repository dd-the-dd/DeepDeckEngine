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

#[test]
fn subtype_heads_denote_permanents_unless_a_card_or_spell_noun_is_present() {
    let subtype = |name: &str| ObjectPredicate::Subtype(name.to_string());
    // CR 109.2: creature and permanent-family subtypes without `card` or
    // `spell` name permanents (`Goblins`, `target Wall`, `Equipment`).
    for name in ["Goblin", "Elf", "Vampire", "Wall", "Equipment", "Forest"] {
        assert!(subtype(name).has_domain(ObjectKind::Permanent), "{name}");
        assert!(
            ObjectPredicate::Intersection(vec![characteristic(), subtype(name)])
                .has_domain(ObjectKind::Permanent),
            "{name}"
        );
    }
    assert!(
        ObjectPredicate::Union(vec![subtype("Elf"), subtype("Goblin")])
            .has_domain(ObjectKind::Permanent)
    );
    // Spell subtypes never name permanents.
    for name in ["Arcane", "Lesson"] {
        assert!(!subtype(name).has_domain(ObjectKind::Permanent), "{name}");
    }
    // Near misses: `Elf card` and `Goblin spell` keep their explicit domain,
    // and a negated subtype is not a head.
    for (domain, kind) in [(ObjectKind::Card, "card"), (ObjectKind::Spell, "spell")] {
        let qualified = ObjectPredicate::Intersection(vec![
            subtype("Elf"),
            ObjectPredicate::ObjectKind(domain),
        ]);
        assert!(!qualified.has_domain(ObjectKind::Permanent), "Elf {kind}");
        assert!(qualified.has_domain(domain), "Elf {kind}");
        let coordinated = ObjectPredicate::Intersection(vec![
            ObjectPredicate::Union(vec![subtype("Elf"), subtype("Goblin")]),
            ObjectPredicate::ObjectKind(domain),
        ]);
        assert!(
            !coordinated.has_domain(ObjectKind::Permanent),
            "Elf or Goblin {kind}"
        );
    }
    assert!(!ObjectPredicate::Not(Box::new(subtype("Elf"))).has_domain(ObjectKind::Permanent));
    // A noncreature permanent subtype keeps its former anchor (CR 205.3d).
    assert!(ObjectPredicate::Not(Box::new(subtype("Equipment"))).has_domain(ObjectKind::Permanent));
}

#[test]
fn permanent_card_category_and_explicit_reading_cancel_subtype_heads() {
    let goblin = || ObjectPredicate::Subtype("Goblin".to_string());
    // `Goblin permanent card` (Goblin Lackey, Moggcatcher): a card noun.
    let permanent_card =
        ObjectPredicate::Intersection(vec![goblin(), ObjectPredicate::PermanentCard]);
    assert!(!permanent_card.has_domain(ObjectKind::Permanent));
    // A parent whose own noun decides (`a Goblin source`, `Elf or Goblin
    // permanent`) reads the explicit domain only.
    assert!(!goblin().has_explicit_domain(ObjectKind::Permanent));
    assert!(goblin().has_domain(ObjectKind::Permanent));
    let equipment = ObjectPredicate::Subtype("Equipment".to_string());
    assert!(equipment.has_explicit_domain(ObjectKind::Permanent));
}

#[test]
fn single_family_subtype_guarantees_its_card_type() {
    // CR 205.3d: a subtype corresponds to its card type, so `Forest` and
    // `Equipment` guarantee land and artifact.
    let subtype = |name: &str| ObjectPredicate::Subtype(name.to_string());
    assert!(subtype("Forest").guarantees_card_type(ObjectCardType::Land));
    assert!(subtype("Equipment").guarantees_card_type(ObjectCardType::Artifact));
    assert!(subtype("Aura").guarantees_card_type(ObjectCardType::Enchantment));
    // A union keeps the guarantee only when every branch has it.
    assert!(
        ObjectPredicate::Union(vec![subtype("Forest"), subtype("Island")])
            .guarantees_card_type(ObjectCardType::Land)
    );
    // Near misses: Kindred spells carry creature subtypes, and spell
    // subtypes are shared by instants and sorceries.
    assert!(!subtype("Elf").guarantees_card_type(ObjectCardType::Creature));
    assert!(!subtype("Arcane").guarantees_card_type(ObjectCardType::Instant));
    assert!(!subtype("Forest").guarantees_card_type(ObjectCardType::Artifact));
}

#[test]
fn declarable_target_uses_the_one_positive_domain_authority() {
    let creature = permanent_type;
    let not = |predicate: ObjectPredicate| ObjectPredicate::Not(Box::new(predicate));
    // `nonattacking creature`: a negated status is an ordinary filter.
    let nonattacking = ObjectPredicate::Intersection(vec![
        not(ObjectPredicate::Status(ObjectStatus::Attacking)),
        creature(),
    ]);
    assert!(nonattacking.is_declarable_target_in(ObjectKind::Permanent));
    // CR 109.2a: `target Wall` names a permanent.
    assert!(
        ObjectPredicate::Subtype("Wall".to_string()).is_declarable_target_in(ObjectKind::Permanent)
    );
    // A negated noun never anchors a target domain by itself.
    assert!(not(creature()).has_domain(ObjectKind::Permanent));
    assert!(!not(creature()).is_declarable_target_in(ObjectKind::Permanent));
    // A resolution-time choice cannot filter a declared target.
    let chosen_type =
        ObjectPredicate::Intersection(vec![creature(), ObjectPredicate::ChosenCreatureType]);
    assert!(chosen_type.is_in_domain(ObjectKind::Permanent));
    assert!(!chosen_type.is_declarable_target_in(ObjectKind::Permanent));
    // A negated object noun has no filter to lower.
    let nonpermanent = ObjectPredicate::Intersection(vec![
        creature(),
        not(ObjectPredicate::ObjectKind(ObjectKind::Card)),
    ]);
    assert!(!nonpermanent.is_declarable_target_in(ObjectKind::Permanent));
    // `instant spell` is a spell target, never a permanent one.
    let instant_spell = ObjectPredicate::Intersection(vec![
        nonpermanent_type(),
        ObjectPredicate::ObjectKind(ObjectKind::Spell),
    ]);
    assert!(instant_spell.is_declarable_target_in(ObjectKind::Spell));
    assert!(!instant_spell.is_declarable_target_in(ObjectKind::Permanent));
    assert!(instant_spell.is_in_domain(ObjectKind::Spell));
    assert!(!instant_spell.is_in_domain_when_controlled(ObjectKind::Permanent, true));
    // A token related by control names a permanent (CR 108.4, 110.2).
    assert!(!ObjectPredicate::IsToken.is_in_domain(ObjectKind::Permanent));
    assert!(ObjectPredicate::IsToken.is_in_domain_when_controlled(ObjectKind::Permanent, true));
}
