//! The qualifier atoms of the shared object query (`selector_qualifiers.rs`):
//! set constraints, superlatives, damage history, keyword abilities with
//! parameters and mana-cost symbol classes, each read where any object noun
//! phrase reads its postnominal qualifiers.
use super::*;
use crate::oracle::canonical::{
    DistinctProperty, Extreme, ExtremumPopulation, KeywordAbilityName, ManaCostSymbolClass,
    NumericAtom, NumericExpression, ObjectQualifier, ObjectQueryNode, ObjectStatus,
};
use crate::oracle::syntax::recognize_entities;

fn query(text: &str) -> MachineResult<ObjectQueryNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("selector-qualifier-atom-test", &tokens);
    let query = recognize_object_query(&mut cursor)?;
    cursor.finish()?;
    Ok(query)
}

fn failure_state(text: &str) -> &'static str {
    query(text).expect_err(text).state
}

fn qualifiers(predicate: &SelectorPredicate) -> Vec<ObjectQualifier> {
    match predicate {
        SelectorPredicate::Qualifier(qualifier) => vec![qualifier.clone()],
        SelectorPredicate::Intersection(operands) => operands.iter().flat_map(qualifiers).collect(),
        _ => Vec::new(),
    }
}

fn creature() -> SelectorPredicate {
    SelectorPredicate::CardType(ObjectCardType::Creature)
}

#[test]
fn distinct_values_are_set_constraints_of_a_plural_head() {
    for (text, property) in [
        ("creatures with different powers", DistinctProperty::Power),
        ("lands with different names", DistinctProperty::Name),
        (
            "cards with different mana values",
            DistinctProperty::ManaValue,
        ),
    ] {
        let parsed = query(text).expect(text);
        assert_eq!(
            qualifiers(&parsed.predicate),
            [ObjectQualifier::DistinctValues(property)],
            "{text}"
        );
        let (rest, constraints) = parsed.predicate.split_set_constraints().expect(text);
        assert_eq!(constraints.len(), 1);
        assert!(!rest.contains_set_constraint());
    }
}

#[test]
fn a_set_constraint_on_a_single_object_fails() {
    // Near miss: `different names` constrains a set; one creature has none.
    assert_eq!(
        failure_state("creature with different names"),
        "selector.set-constraint.distinct.head"
    );
    assert_eq!(
        failure_state("creature card with total mana value 4 or less"),
        "selector.set-constraint.total.head"
    );
}

#[test]
fn coordinated_set_constraints_constrain_one_selection() {
    // A combination no card was used to design: distinct and total together.
    let parsed = query("creature cards with different mana values and total mana value 6 or less")
        .expect("coordinated set constraints parse");
    let (rest, constraints) = parsed
        .predicate
        .split_set_constraints()
        .expect("both constraints are top-level conjuncts");
    assert_eq!(
        constraints,
        [
            ObjectQualifier::DistinctValues(DistinctProperty::ManaValue),
            ObjectQualifier::TotalAtMost {
                property: SelectorNumericProperty::ManaValue,
                maximum: Box::new(NumericExpression::Atom(NumericAtom::Integer(6))),
            },
        ]
    );
    assert!(rest.is_in_domain(ObjectKind::Card));
}

#[test]
fn total_power_and_toughness_of_one_object_is_not_a_set_total() {
    assert_eq!(
        failure_state("creatures with total power and toughness 5 or less"),
        "selector.set-constraint.total.property"
    );
}

#[test]
fn a_superlative_names_its_population() {
    let parsed = query("creature with the greatest power among creatures they control")
        .expect("superlative over the candidate's controller parses");
    assert_eq!(
        qualifiers(&parsed.predicate),
        [ObjectQualifier::Extremum {
            property: SelectorNumericProperty::Power,
            extreme: Extreme::Greatest,
            among: Box::new(creature()),
            population: ExtremumPopulation::CandidateController,
        }]
    );
    let parsed = query("creature with the least toughness among creatures on the battlefield")
        .expect("battlefield superlative parses");
    assert!(matches!(
        qualifiers(&parsed.predicate).as_slice(),
        [ObjectQualifier::Extremum {
            extreme: Extreme::Least,
            population: ExtremumPopulation::Battlefield,
            ..
        }]
    ));
}

#[test]
fn a_superlative_population_unites_its_coordinated_nouns() {
    let parsed = query(
        "creature or planeswalker with the greatest mana value among creatures and planeswalkers they control",
    )
    .expect("a coordinated population parses");
    let [ObjectQualifier::Extremum { among, .. }] = qualifiers(&parsed.predicate).as_slice() else {
        panic!("one superlative: {:?}", parsed.predicate);
    };
    assert_eq!(
        **among,
        SelectorPredicate::Union(vec![
            creature(),
            SelectorPredicate::CardType(ObjectCardType::Planeswalker),
        ])
    );
}

#[test]
fn a_superlative_without_a_population_fails() {
    // Near miss: `the greatest mana value` has no domain to rank in.
    assert_eq!(
        failure_state("creature with the greatest mana value"),
        "selector.superlative.domain"
    );
}

#[test]
fn the_damage_history_participle_reads_the_source() {
    let parsed = query("creature dealt damage by this creature this turn")
        .expect("source damage history parses");
    assert_eq!(
        qualifiers(&parsed.predicate),
        [ObjectQualifier::DamagedBySourceThisTurn]
    );
    // Without `by`, the object was dealt damage by any source.
    let parsed = query("creature dealt damage this turn").expect("damage history parses");
    assert_eq!(
        parsed.predicate,
        SelectorPredicate::Intersection(vec![
            creature(),
            SelectorPredicate::Status(ObjectStatus::WasDealtDamageThisTurn),
        ])
    );
}

#[test]
fn a_damage_history_from_another_object_is_not_claimed() {
    // Near miss: `target creature` is not the source; the participle stays
    // unread and the query ends before it.
    assert!(query("creature dealt damage by target creature this turn").is_err());
}

#[test]
fn keyword_abilities_with_parameters_are_qualifiers() {
    for text in [
        "cards with cycling",
        "card with a cycling ability",
        "permanent cards with cycling abilities",
    ] {
        let parsed = query(text).expect(text);
        assert_eq!(
            qualifiers(&parsed.predicate),
            [ObjectQualifier::KeywordAbility(KeywordAbilityName::Cycling)],
            "{text}"
        );
    }
    let parsed = query("card with a kicker ability").expect("kicker ability parses");
    assert_eq!(
        qualifiers(&parsed.predicate),
        [ObjectQualifier::KeywordAbility(KeywordAbilityName::Kicker)]
    );
    // A parameterless keyword keeps its own route.
    let parsed = query("creature with flying").expect("flying parses");
    assert!(qualifiers(&parsed.predicate).is_empty());
}

#[test]
fn a_variable_mana_symbol_is_a_mana_cost_qualifier() {
    let parsed = query("creature card with {X} in its mana cost").expect("{X} qualifier parses");
    assert_eq!(
        qualifiers(&parsed.predicate),
        [ObjectQualifier::ManaCostSymbol(
            ManaCostSymbolClass::VariableX
        )]
    );
}
