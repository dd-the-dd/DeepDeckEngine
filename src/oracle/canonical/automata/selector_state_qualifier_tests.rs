//! Shared object-query predicates: battlefield status (`untapped`), counter
//! qualifiers after `with`/`without`, keyword negation, the snow supertype,
//! the `permanent card` category and the library-search reuse of the shared
//! numeric qualifier. Parser-level evidence; runtime behavior is proved in
//! `src/engine/tests/object_state_predicate_runtime.rs`.
use serde_json::{Value, json};

use super::super::{GrantedKeyword, ObjectStatus};
use super::*;
use crate::oracle::canonical::{NumericAtom, NumericExpression, ObjectQueryNode};
use crate::oracle::counter_name::CounterName;
use crate::oracle::syntax::{recognize_entities, resolve_counter_name_spans};
use crate::oracle::{OracleCardParseRequest, parse_oracle_card};

fn query(text: &str) -> MachineResult<ObjectQueryNode> {
    let mut tokens = recognize_entities(text);
    // The pipeline's contextual counter-name pass (`charge counter`).
    resolve_counter_name_spans(&mut tokens);
    let mut cursor = OracleTokenCursor::new("selector-state-qualifier-test", &tokens);
    let query = recognize_object_query(&mut cursor)?;
    cursor.finish()?;
    Ok(query)
}

/// Conjuncts of a predicate, flattening nested intersections.
fn operands(predicate: &SelectorPredicate) -> Vec<SelectorPredicate> {
    match predicate {
        SelectorPredicate::Intersection(items) => items.iter().flat_map(operands).collect(),
        atom => vec![atom.clone()],
    }
}

fn counter(name: &str) -> Option<CounterName> {
    CounterName::from_parts([name])
}

#[test]
fn counter_qualifiers_compose_with_any_head_and_relation() {
    for (text, expected) in [
        (
            "creature with a +1/+1 counter on it",
            SelectorPredicate::HasCounter(counter("+1/+1")),
        ),
        (
            "land with a counter on it",
            SelectorPredicate::HasCounter(None),
        ),
        (
            "creatures with one or more -1/-1 counters on them",
            SelectorPredicate::HasCounter(counter("-1/-1")),
        ),
        (
            "creature without a counter on it",
            SelectorPredicate::Not(Box::new(SelectorPredicate::HasCounter(None))),
        ),
        (
            "creature without flying",
            SelectorPredicate::Not(Box::new(SelectorPredicate::HasKeyword(
                GrantedKeyword::Flying,
            ))),
        ),
    ] {
        let parsed = query(text).unwrap_or_else(|failure| panic!("{text}: {failure:?}"));
        assert!(
            operands(&parsed.predicate).contains(&expected),
            "{text}: {parsed:?}"
        );
    }
    // Previously unused combination: status + counter + controller relation.
    let parsed = query("untapped artifact you control with a charge counter on it")
        .expect("independent qualifiers recombine");
    let all = operands(&parsed.predicate);
    assert!(all.contains(&SelectorPredicate::Status(ObjectStatus::Untapped)));
    assert!(all.contains(&SelectorPredicate::HasCounter(counter("charge"))));
    assert_eq!(
        parsed.controller,
        Some(ObjectControllerScope::AbilityController)
    );
}

#[test]
fn counter_and_without_near_misses_fail_or_leave_the_parent_suffix() {
    for (text, state) in [
        (
            "creature with a counter",
            "selector.counter-qualifier.location",
        ),
        (
            "creature with a counter on its",
            "selector.counter-qualifier.location",
        ),
        // Not a qualifier: the query completes before `without` and the
        // unconsumed suffix is reported where the query completes.
        ("creature without a name", "selector.predicate.complete"),
    ] {
        let failure = query(text).expect_err(text);
        assert_eq!(failure.state, state, "{text}");
    }
    // `without paying ...` belongs to the enclosing casting permission.
    let tokens = recognize_entities("creature spells without paying their mana costs");
    let mut cursor = OracleTokenCursor::new("selector-without-suffix", &tokens);
    let parsed = recognize_object_query(&mut cursor).expect("the query stops before `without`");
    assert_eq!(cursor.peek(), Some("without"));
    assert!(!format!("{parsed:?}").contains("Not("));
}

#[test]
fn permanent_card_and_snow_are_shared_characteristic_atoms() {
    let parsed = query("nonland permanent card").expect("permanent card category");
    assert_eq!(
        operands(&parsed.predicate),
        [
            SelectorPredicate::Not(Box::new(SelectorPredicate::CardType(ObjectCardType::Land))),
            SelectorPredicate::PermanentCard,
            SelectorPredicate::ObjectKind(ObjectKind::Card),
        ]
    );
    assert!(parsed.predicate.has_domain(ObjectKind::Card));
    // A bare `permanent` keeps the battlefield-permanent domain.
    let bare = query("nonland permanent").expect("permanent domain");
    assert!(bare.predicate.has_domain(ObjectKind::Permanent));
    let snow = query("snow lands").expect("snow supertype");
    assert!(operands(&snow.predicate).contains(&SelectorPredicate::Supertype(CardSupertype::Snow)));
}

#[test]
fn library_search_reuses_permanent_card_and_numeric_qualifier() {
    let search = |text: &str| {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("search-card-state-test", &tokens);
        let parsed = recognize_search_card_selector(&mut cursor, false);
        parsed.and_then(|predicate| cursor.finish().map(|()| predicate))
    };
    let dragon = search("Dragon permanent card with mana value 3 or less").expect("subtype");
    assert_eq!(
        operands(&dragon),
        [
            ObjectPredicate::Subtype("Dragon".to_string()),
            ObjectPredicate::PermanentCard,
            ObjectPredicate::NumericComparison {
                left: SelectorNumericProperty::ManaValue,
                operator: NumericComparisonOperator::LessThanOrEqual,
                right: Box::new(NumericExpression::Atom(NumericAtom::Integer(3))),
            },
        ]
    );
    assert!(search("creature card with toughness 2 or less").is_ok());
    assert!(search("card with mana cost {1}{G} or {G}{G}").is_ok());
    let near_miss = search("creature card with mana value").expect_err("missing threshold");
    assert_eq!(near_miss.state, "numeric.atom.start");
}

#[test]
fn coordinated_stack_ability_kinds_share_the_ability_head() {
    let tokens = recognize_entities("activated or triggered ability");
    let mut cursor = OracleTokenCursor::new("stack-item-shared-head", &tokens);
    let parsed = recognize_stack_item_selector(&mut cursor).expect("shared head noun");
    cursor.finish().expect("complete selector");
    assert_eq!(
        parsed,
        StackItemPredicate::Union(vec![
            StackItemPredicate::Ability(StackAbilityKind::Activated),
            StackItemPredicate::Ability(StackAbilityKind::Triggered),
        ])
    );
    let tokens = recognize_entities("activated or triggered");
    let mut cursor = OracleTokenCursor::new("stack-item-missing-head", &tokens);
    assert!(recognize_stack_item_selector(&mut cursor).is_err());
}

fn card_rule(type_line: &str, oracle: &str) -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Predicate Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:?}",
        parsed.diagnostics
    );
    serde_json::to_value(&parsed.abilities[0].rule).expect("rule serializes")
}

#[test]
fn whole_cards_lower_status_counter_and_source_exclusion() {
    let untapped = card_rule("Sorcery", "Destroy all untapped creatures.");
    assert_eq!(
        untapped["effects"][0]["permanent"]["where"],
        json!({"kind":"and","operands":[
            {"kind":"not","operand":{"kind":"isTapped"}},
            {"kind":"cardTypeContains","value":"Creature"}
        ]}),
        "{untapped}"
    );
    let other = card_rule("Creature - Giant", "{1}: Destroy all other creatures.");
    assert_eq!(
        other["effects"][0]["permanent"]["excludeSource"],
        json!(true),
        "{other}"
    );
    let counters = card_rule(
        "Creature - Elf",
        "When this creature enters, put a +1/+1 counter on each other creature you control with a +1/+1 counter on it.",
    );
    let text = counters.to_string();
    assert!(text.contains(r#""excludeSource":true"#), "{text}");
    assert!(
        text.contains(r#"{"counter":"+1/+1","kind":"hasCounter"}"#),
        "{text}"
    );
}
