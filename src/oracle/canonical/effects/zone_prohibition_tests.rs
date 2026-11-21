use super::*;
use crate::oracle::syntax::recognize_entities;
fn lower(text: &str) -> MachineResult<CanonicalRuleDraft> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("zone-lower", &tokens);
    let ZoneProhibitionAttempt::Parsed(node) = try_recognize_zone_prohibition(&mut cursor) else {
        panic!("complete typed input: {text}")
    };
    lower_zone_prohibition(&mut cursor, node, "Artifact")
}
#[test]
fn exact_two_rules_preserve_disjoint_entry_and_cast_semantics() {
    assert_eq!(
        lower("Creature cards in graveyards and libraries can't enter the battlefield.")
            .unwrap()
            .rule["modifiers"][0],
        json!({"kind":"prohibitEnterBattlefieldFromZones","zones":["graveyard","library"],"where":{"kind":"cardTypeContains","value":"Creature"}})
    );
    assert_eq!(
        lower("Players can't cast spells from graveyards or libraries.")
            .unwrap()
            .rule["modifiers"][0],
        json!({"kind":"prohibitCastFromZones","players":{"kind":"eachPlayer"},"zones":["graveyard","library"],"where":null})
    );
}
#[test]
fn generic_type_union_and_exile_casting_stay_composed() {
    assert_eq!(lower("Each player cannot cast artifact or creature spells from graveyards, libraries, or exile.").unwrap().rule["modifiers"][0],json!({"kind":"prohibitCastFromZones","players":{"kind":"eachPlayer"},"zones":["graveyard","library","exile"],"where":{"kind":"or","operands":[{"kind":"cardTypeContains","value":"Artifact"},{"kind":"cardTypeContains","value":"Creature"}]}}));
}
#[test]
fn narrower_player_and_unproved_entry_zone_contracts_fail_explicitly() {
    for text in [
        "Your opponents can't cast spells from graveyards.",
        "Creature cards in hands can't enter the battlefield.",
        "Creature cards in exile can't enter the battlefield.",
        "Players can't cast spells from hands.",
    ] {
        assert!(lower(text).is_err(), "{text}");
    }
}

#[test]
fn universal_branches_are_never_dropped_from_boolean_filters() {
    let bare = ObjectPredicate::ObjectKind(ObjectKind::Card);
    let creature = ObjectPredicate::CardType(ObjectCardType::Creature);
    assert!(domain_filter_is_representable(&bare));
    assert!(!domain_filter_is_representable(&ObjectPredicate::Union(
        vec![bare.clone(), creature.clone()]
    )));
    assert!(!domain_filter_is_representable(&ObjectPredicate::Not(
        Box::new(bare)
    )));
    assert!(domain_filter_is_representable(&ObjectPredicate::Union(
        vec![creature.clone(), ObjectPredicate::Not(Box::new(creature))]
    )));
}
