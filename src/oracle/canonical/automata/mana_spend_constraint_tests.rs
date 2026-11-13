use super::super::{CardColor, ObjectCardType, ObjectKind};
use super::*;

fn unbound(predicate: ObjectPredicate) -> ManaSpendPredicateNode {
    ManaSpendPredicateNode {
        predicate,
        binding: None,
    }
}

#[test]
fn positive_cast_constraint_preserves_predicate_binding_and_consequence() {
    let clause = ManaSpendClauseNode {
        constraint: ManaSpendConstraintNode::Only {
            usage: ManaSpendUsageNode::CastSpell {
                spell: ManaSpendPredicateNode {
                    predicate: ObjectPredicate::CardType(ObjectCardType::Creature),
                    binding: Some(CharacteristicChoiceBinding::CreatureType),
                },
            },
            alternatives: vec![],
        },
        consequences: vec![ManaSpendConsequenceNode::SpellCantBeCountered],
    };

    let (spell, alternatives) = match clause.constraint {
        ManaSpendConstraintNode::Only {
            usage: ManaSpendUsageNode::CastSpell { spell },
            alternatives,
        } => (spell, alternatives),
    };
    assert_eq!(
        spell.predicate,
        ObjectPredicate::CardType(ObjectCardType::Creature)
    );
    assert_eq!(
        spell.binding,
        Some(CharacteristicChoiceBinding::CreatureType)
    );
    assert!(alternatives.is_empty());
    assert_eq!(
        clause.consequences,
        vec![ManaSpendConsequenceNode::SpellCantBeCountered]
    );
}

#[test]
fn spell_qualities_and_colors_reuse_the_shared_object_predicate() {
    let planeswalker = ManaSpendUsageNode::CastSpell {
        spell: unbound(ObjectPredicate::CardType(ObjectCardType::Planeswalker)),
    };
    let blue_spell = ManaSpendUsageNode::CastSpell {
        spell: unbound(ObjectPredicate::Color(CardColor::Blue)),
    };
    let constraint = ManaSpendConstraintNode::Only {
        usage: planeswalker,
        alternatives: vec![blue_spell],
    };

    assert!(matches!(
        constraint,
        ManaSpendConstraintNode::Only { alternatives, .. } if alternatives.len() == 1
    ));
}

#[test]
fn spell_uses_compose_as_nonempty_alternatives() {
    let constraint = ManaSpendConstraintNode::Only {
        usage: ManaSpendUsageNode::CastSpell {
            spell: unbound(ObjectPredicate::CardType(ObjectCardType::Creature)),
        },
        alternatives: vec![ManaSpendUsageNode::CastSpell {
            spell: unbound(ObjectPredicate::Color(CardColor::Green)),
        }],
    };

    let alternatives = match constraint {
        ManaSpendConstraintNode::Only { alternatives, .. } => alternatives,
    };
    assert!(matches!(
        alternatives.as_slice(),
        [ManaSpendUsageNode::CastSpell {
            spell: ManaSpendPredicateNode {
                predicate: ObjectPredicate::Color(CardColor::Green),
                binding: None,
            },
        }]
    ));
}

#[test]
fn every_constraint_has_a_mandatory_primary_usage() {
    fn primary(constraint: &ManaSpendConstraintNode) -> &ManaSpendUsageNode {
        match constraint {
            ManaSpendConstraintNode::Only { usage, .. } => usage,
        }
    }

    let only = ManaSpendConstraintNode::Only {
        usage: ManaSpendUsageNode::CastSpell {
            spell: unbound(ObjectPredicate::ObjectKind(ObjectKind::Spell)),
        },
        alternatives: vec![],
    };

    assert!(matches!(
        primary(&only),
        ManaSpendUsageNode::CastSpell { .. }
    ));
}
