use super::super::*;
use crate::oracle::canonical::{MachineResult, OracleTokenCursor};
use crate::oracle::syntax::recognize_entities;

fn parse(text: &str) -> MachineResult<StaticSpellReductionNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("static-spell-reduction-test", &tokens);
    recognize_static_spell_reduction(&mut cursor)
}

#[test]
fn relative_color_union_is_one_reduction_predicate() {
    let node = parse("Each spell you cast that's red or green costs {1} less to cast.")
        .expect("fixed generic relative reduction parses");

    assert_eq!(
        spells(&node).caster,
        Some(SpellCasterScope::AbilityController)
    );
    assert_eq!(node.amount, NumericExpression::from(1_i64));
    assert!(spells(&node).predicate.has_domain(ObjectKind::Spell));
    assert!(matches!(
        &spells(&node).predicate,
        ObjectPredicate::Intersection(outer)
            if outer.iter().any(|operand| matches!(operand,
                ObjectPredicate::Union(colors)
                    if colors.contains(&ObjectPredicate::Color(CardColor::Red))
                        && colors.contains(&ObjectPredicate::Color(CardColor::Green))))
    ));
}

#[test]
fn shared_object_query_handles_domain_type_color_and_composed_siblings() {
    let all = parse("Spells you cast cost {1} less to cast.").expect("domain-only sibling parses");
    assert_eq!(all.amount, NumericExpression::from(1_i64));
    assert_eq!(
        spells(&all).caster,
        Some(SpellCasterScope::AbilityController)
    );

    let artifact =
        parse("Artifact spells you cast cost {2} less to cast.").expect("card-type sibling parses");
    assert_eq!(artifact.amount, NumericExpression::from(2_i64));
    assert!(spells(&artifact).predicate.has_domain(ObjectKind::Spell));

    let composed = parse("Red or artifact spells you cast cost {1} less to cast.")
        .expect("color/type union sibling parses");
    assert!(matches!(
        &spells(&composed).predicate,
        ObjectPredicate::Intersection(operands)
            if operands.iter().any(|operand| matches!(operand, ObjectPredicate::Union(_)))
    ));
}

/// The cost-modification frame reads every caster relation of the spell
/// set (Helm of Awakening, Arcane Melee): opponents' spells are one more
/// typed relation, not a separate family.
#[test]
fn opponent_scope_is_a_typed_caster_relation() {
    let node = parse("Red spells your opponents cast cost {1} less to cast.")
        .expect("the opponent caster relation parses");
    assert_eq!(
        spells(&node).caster,
        Some(SpellCasterScope::OpponentOfAbilityController)
    );
    assert_eq!(node.amount, NumericExpression::from(1_i64));
}

#[test]
fn cast_relation_is_typed_and_does_not_change_the_default_control_relation() {
    let spell = parse("Creature spells you cast cost {1} less to cast.")
        .expect("spell caster relation parses in the dedicated collection");
    assert_eq!(
        spells(&spell).caster,
        Some(SpellCasterScope::AbilityController)
    );

    let failure = parse("Creatures you cast cost {1} less to cast.")
        .expect_err("the cast relation does not turn permanents into spells");
    assert_eq!(failure.state, "spell-cast-set.domain");
}

/// Colored deltas (`{R}`, `{1}{R}`) are typed colored reductions (CR
/// 118.7b-c); an undefined `{X}`, a zero and an overflowing amount still
/// fail at their own states.
#[test]
fn colored_reductions_are_typed_and_undefined_zero_or_overflow_amounts_fail() {
    for (text, generic, red) in [
        ("Red spells you cast cost {R} less to cast.", 0_i64, 1_u8),
        ("Red spells you cast cost {1}{R} less to cast.", 1, 1),
    ] {
        let node = parse(text).expect(text);
        assert_eq!(node.amount, NumericExpression::from(generic), "{text}");
        let colored = node.colored.as_ref().expect(text);
        assert_eq!(colored.counts, [0, 0, 0, red, 0, 0], "{text}");
        assert!(colored.excess_reduces_generic, "{text}");
    }
    let undefined = parse("Red spells you cast cost {X} less to cast.")
        .expect_err("an {X} reduction needs its where-X definition");
    assert_eq!(undefined.state, "casting-reduction.where-x");
    for text in [
        "Red spells you cast cost {0} less to cast.",
        "Red spells you cast cost {2147483648} less to cast.",
    ] {
        let failure = parse(text).expect_err(text);
        assert_eq!(failure.state, "static.spell-cost-reduction.cost", "{text}");
        assert!(
            failure.expected.contains("generic and whole colored"),
            "{text}: {}",
            failure.expected
        );
    }
}

fn spells(node: &StaticSpellReductionNode) -> &SpellCastQueryNode {
    let CastingReductionSubject::SpellSet(spells) = &node.subject else {
        panic!("the static spell adapter must retain the spell-set subject");
    };
    spells
}
