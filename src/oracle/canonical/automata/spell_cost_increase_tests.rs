use super::super::*;
use crate::oracle::canonical::{MachineResult, OracleTokenCursor};
use crate::oracle::syntax::recognize_entities;

fn parse(text: &str) -> MachineResult<SpellCostIncreaseNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("spell-cost-increase-test", &tokens);
    recognize_spell_cost_increase(&mut cursor)
}

#[test]
fn noncreature_spells_retain_negation_domain_scope_and_cost() {
    let node = parse("Noncreature spells cost {1} more to cast.").expect("static spell tax parses");

    assert_eq!(node.spells.caster, None);
    assert_eq!(
        node.spells.predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::Not(Box::new(ObjectPredicate::CardType(
                ObjectCardType::Creature,
            ))),
            ObjectPredicate::ObjectKind(ObjectKind::Spell),
        ])
    );
    assert_eq!(node.cost.as_oracle_cost(), "{1}");
}

#[test]
fn card_type_and_color_siblings_reuse_the_shared_selector_and_mana_cost() {
    let creature =
        parse("Creature spells cost {2} more to cast.").expect("creature-spell sibling parses");
    assert_eq!(creature.cost.as_oracle_cost(), "{2}");
    assert!(creature.spells.predicate.has_domain(ObjectKind::Spell));

    let red = parse("Red spells cost {2} more to cast.").expect("color-spell sibling parses");
    assert_eq!(red.cost.as_oracle_cost(), "{2}");
    assert!(matches!(
        red.spells.predicate,
        ObjectPredicate::Intersection(ref operands)
            if operands.contains(&ObjectPredicate::Color(CardColor::Red))
                && operands.contains(&ObjectPredicate::ObjectKind(ObjectKind::Spell))
    ));
}

#[test]
fn coordinated_characteristic_branches_form_one_spell_union() {
    let node = parse("Red creature spells and green creature spells cost {1} more to cast.")
        .expect("coordinated spell union parses");

    assert_eq!(node.spells.caster, None);
    assert!(node.spells.predicate.has_domain(ObjectKind::Spell));
    assert!(matches!(
        node.spells.predicate,
        ObjectPredicate::Union(ref branches) if branches.len() == 2
    ));
}

#[test]
fn opponent_scope_is_retained_outside_the_spell_predicate() {
    let node = parse("Noncreature spells your opponents cast cost {1} more to cast.")
        .expect("opponent spell tax parses");

    assert_eq!(
        node.spells.caster,
        Some(SpellCasterScope::OpponentOfAbilityController)
    );
}

#[test]
fn activation_cost_and_reduction_near_misses_are_rejected() {
    assert!(parse("Activated abilities of white enchantments cost {3} more to activate.").is_err());

    let reduction = parse("Noncreature spells cost {1} less to cast.")
        .expect_err("casting-cost reduction belongs to another parent");
    assert_eq!(reduction.state, "static.spell-cost-increase.increase");
    assert_eq!(reduction.actual.as_deref(), Some("less"));
}

#[test]
fn unbound_x_is_rejected_at_the_cost_slot() {
    let failure = parse("Noncreature spells cost {X} more to cast.")
        .expect_err("the tax must not reuse the cast spell's X value");

    assert_eq!(failure.state, "cost.mana.complete");
    assert_eq!(failure.actual.as_deref(), Some("more"));
    assert!(failure.expected.contains("without {X}"));
}
