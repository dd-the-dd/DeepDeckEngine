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
        node.spells.caster,
        Some(SpellCasterScope::AbilityController)
    );
    assert_eq!(node.amount, 1);
    assert!(node.spells.predicate.has_domain(ObjectKind::Spell));
    assert!(matches!(
        node.spells.predicate,
        ObjectPredicate::Intersection(ref outer)
            if outer.iter().any(|operand| matches!(operand,
                ObjectPredicate::Union(colors)
                    if colors.contains(&ObjectPredicate::Color(CardColor::Red))
                        && colors.contains(&ObjectPredicate::Color(CardColor::Green))))
    ));
}

#[test]
fn shared_object_query_handles_domain_type_color_and_composed_siblings() {
    let all = parse("Spells you cast cost {1} less to cast.").expect("domain-only sibling parses");
    assert_eq!(all.amount, 1);
    assert_eq!(all.spells.caster, Some(SpellCasterScope::AbilityController));

    let artifact =
        parse("Artifact spells you cast cost {2} less to cast.").expect("card-type sibling parses");
    assert_eq!(artifact.amount, 2);
    assert!(artifact.spells.predicate.has_domain(ObjectKind::Spell));

    let composed = parse("Red or artifact spells you cast cost {1} less to cast.")
        .expect("color/type union sibling parses");
    assert!(matches!(
        composed.spells.predicate,
        ObjectPredicate::Intersection(ref operands)
            if operands.iter().any(|operand| matches!(operand, ObjectPredicate::Union(_)))
    ));
}

#[test]
fn opponent_scope_is_not_claimed_until_runtime_executes_generic_player_refs() {
    let failure = parse("Red spells your opponents cast cost {1} less to cast.")
        .expect_err("executor is currently controller-scoped");
    assert_eq!(failure.state, "static.spell-cost-reduction.caster-scope");
}

#[test]
fn cast_relation_is_typed_and_does_not_change_the_default_control_relation() {
    let spell = parse("Creature spells you cast cost {1} less to cast.")
        .expect("spell caster relation parses in the dedicated collection");
    assert_eq!(
        spell.spells.caster,
        Some(SpellCasterScope::AbilityController)
    );

    let failure = parse("Creatures you cast cost {1} less to cast.")
        .expect_err("the cast relation does not turn permanents into spells");
    assert_eq!(failure.state, "spell-cast-set.domain");
}

#[test]
fn colored_variable_zero_multi_symbol_and_overflow_reductions_are_rejected() {
    for text in [
        "Red spells you cast cost {R} less to cast.",
        "Red spells you cast cost {X} less to cast.",
        "Red spells you cast cost {0} less to cast.",
        "Red spells you cast cost {1}{R} less to cast.",
        "Red spells you cast cost {2147483648} less to cast.",
    ] {
        let failure = parse(text).expect_err("only one positive generic symbol is supported");
        assert_eq!(failure.state, "static.spell-cost-reduction.cost");
        assert!(failure.expected.contains("positive generic"));
    }
}
