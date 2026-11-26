use super::*;
use crate::oracle::syntax::recognize_entities;

fn parse(text: &str) -> MachineResult<CastingCostReductionNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("shared-cost-query", &tokens);
    recognize_casting_cost_reduction(&mut cursor)
}

#[test]
fn coordinated_heads_are_a_union_but_relative_properties_are_logical() {
    let node = parse(
        "This spell costs {1} less to cast for each instant and sorcery card in your graveyard.",
    )
    .unwrap();
    let NumericExpression::CountQuery(query) = node.amount else {
        panic!("symbolic query count")
    };
    let ObjectPredicate::Intersection(items) = &query.objects.objects.predicate else {
        panic!("shared card head")
    };
    assert!(items.contains(&ObjectPredicate::ObjectKind(ObjectKind::Card)));
    assert!(items.iter().any(|item| matches!(item, ObjectPredicate::Union(branches)
        if branches == &vec![ObjectPredicate::CardType(ObjectCardType::Instant), ObjectPredicate::CardType(ObjectCardType::Sorcery)])));
    assert_eq!(
        query.holders,
        Some(ZoneHolderQueryNode::Player(
            LocalPlayerReference::Controller
        ))
    );

    let relative =
        parse("Each spell you cast that's red and green costs {1} less to cast.").unwrap();
    let CastingReductionSubject::SpellSet(spells) = relative.subject else {
        panic!("spell subject")
    };
    assert!(
        matches!(spells.predicate, ObjectPredicate::Intersection(ref items)
        if items.iter().any(|p| matches!(p, ObjectPredicate::Intersection(colors)
            if colors.contains(&ObjectPredicate::Color(CardColor::Red)) && colors.contains(&ObjectPredicate::Color(CardColor::Green))))
        || (items.contains(&ObjectPredicate::Color(CardColor::Red)) && items.contains(&ObjectPredicate::Color(CardColor::Green))))
    );
}

#[test]
fn amount_is_symbolic_and_zone_holders_are_separate_from_object_relations() {
    let node = parse(
        "Artifact spells you cast cost {2} less to cast for each artifact card in all graveyards.",
    )
    .unwrap();
    assert!(matches!(node.subject, CastingReductionSubject::SpellSet(_)));
    let NumericExpression::Multiply { left, right } = node.amount else {
        panic!("ordinary arithmetic")
    };
    assert_eq!(*left, NumericExpression::from(2_i64));
    let NumericExpression::CountQuery(query) = *right else {
        panic!("shared query child")
    };
    assert_eq!(
        query.holders,
        Some(ZoneHolderQueryNode::Players(
            PlayerSetReferenceNode::EachPlayer
        ))
    );
    assert!(query.objects.objects.controller.is_none());
    assert!(query.objects.ownership.is_none());
}

#[test]
fn unavailable_roles_and_partial_envelopes_are_explicit_failures() {
    for text in [
        "This spell costs {1} less to cast for each artifact card in target graveyard.",
        "This spell costs {1} less to cast for each artifact card in two graveyards of your choice.",
        "This spell costs {1} less to cast for each artifact card you own in your graveyard.",
        "This spell costs {1} less to cast for each artifact card in your library.",
        "This spell costs {1} less to cast for each artifact card in your graveyard. Draw a card.",
        "Artifact spells you cast that's red and green, cost {1} less to cast.",
        "Artifact cards you own and creature cards in your graveyard cost {1} less to cast.",
    ] {
        assert!(parse(text).is_err(), "must not approximate: {text}");
    }
}
