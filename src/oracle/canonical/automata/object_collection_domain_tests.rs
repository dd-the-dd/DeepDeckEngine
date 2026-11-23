use super::super::{
    ObjectCardType, ObjectControllerScope, StaticCollectionActivatedGrantAttempt,
    try_recognize_static_collection_activated_grant,
};
use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn explicit_domain_retains_subtype_without_inventing_creature() {
    for text in [
        "Each land and Ally you control",
        "All lands and Allies you control",
        "lands and Allies you control",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("collection-domain", &tokens);
        let collection =
            recognize_object_collection_in_domain(&mut cursor, ObjectKind::Permanent).expect(text);
        cursor.finish().expect("the complete collection");
        assert_eq!(collection.quantifier, ObjectCollectionQuantifier::Universal);
        assert_eq!(collection.domain, ObjectKind::Permanent);
        assert_eq!(
            collection.objects.controller,
            Some(ObjectControllerScope::AbilityController)
        );
        assert_eq!(
            collection.objects.predicate,
            SelectorPredicate::Union(vec![
                SelectorPredicate::CardType(ObjectCardType::Land),
                SelectorPredicate::Subtype("Ally".to_owned()),
            ])
        );
        // CR 109.2a: `Allies` without `card` or `spell` names permanents.
        assert!(
            collection
                .objects
                .predicate
                .has_domain(ObjectKind::Permanent)
        );
    }
    let tokens = recognize_entities("All permanents you control");
    let mut cursor = OracleTokenCursor::new("collection-domain", &tokens);
    let collection =
        recognize_object_collection_in_domain(&mut cursor, ObjectKind::Permanent).unwrap();
    cursor.finish().unwrap();
    assert_eq!(
        collection.objects.predicate,
        SelectorPredicate::ObjectKind(ObjectKind::Permanent)
    );
}

#[test]
fn incompatible_or_unrepresentable_domain_fails_without_cursor_commit() {
    for text in [
        "Each instant",
        "Each land and instant",
        "Each red",
        "All permanents or artifacts",
        "Each card",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("collection-domain", &tokens);
        assert!(
            recognize_object_collection_in_domain(&mut cursor, ObjectKind::Permanent).is_err(),
            "{text}"
        );
        assert_eq!(cursor.position(), 0, "failed child must not commit: {text}");
    }
}

#[test]
fn collection_preserves_local_qualifiers_controller_and_exclusion() {
    let tokens = recognize_entities(
        "Each other artifact with flying and creature with reach you control, draw",
    );
    let mut cursor = OracleTokenCursor::new("collection-domain", &tokens);
    let collection =
        recognize_object_collection_in_domain(&mut cursor, ObjectKind::Permanent).unwrap();
    assert!(collection.exclude_source);
    assert_eq!(
        collection.objects.controller,
        Some(ObjectControllerScope::AbilityController)
    );
    assert!(
        matches!(collection.objects.predicate, SelectorPredicate::Union(ref branches)
        if branches.len() == 2 && branches.iter().all(|branch| matches!(branch, SelectorPredicate::Intersection(_))))
    );
    assert_eq!(cursor.peek(), Some(","));
    let tokens = recognize_entities("Each artifact you control or creature");
    let mut cursor = OracleTokenCursor::new("collection-domain", &tokens);
    recognize_object_collection_in_domain(&mut cursor, ObjectKind::Permanent).unwrap();
    assert!(
        cursor.finish().is_err(),
        "local controller cannot be hoisted over the later branch"
    );
}

#[test]
fn domain_is_opt_in_without_weakening_legacy_collection_contract() {
    let tokens = recognize_entities("Each Ally you control");
    let mut cursor = OracleTokenCursor::new("collection-domain", &tokens);
    assert!(recognize_object_collection(&mut cursor).is_err());
    let tokens = recognize_entities("Ally you control");
    let mut cursor = OracleTokenCursor::new("collection-domain", &tokens);
    let query = recognize_object_collection(&mut cursor).unwrap();
    assert!(query.predicate.has_nominal_head());
    // CR 109.2a: a subtype head without `card` or `spell` is a permanent.
    assert!(query.predicate.has_domain(ObjectKind::Permanent));
    assert!(query.denotes_battlefield_permanents());
}

#[test]
fn quoted_grant_commits_only_its_complete_supported_envelope() {
    for text in [
        "Each land and Ally you control has \"{T}: Add one mana of any color.\"",
        "All other artifacts you control have \"{T}: Add {C}.\"",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("collection-domain-grant", &tokens);
        assert!(
            matches!(
                try_recognize_static_collection_activated_grant(&mut cursor, &[], None),
                StaticCollectionActivatedGrantAttempt::Parsed(_)
            ),
            "{text}"
        );
        cursor.finish().unwrap();
    }
    for text in [
        "Each land and Ally you control has \"{T}: Add {G}.\" until end of turn.",
        "Each land and Ally you control has \"{T}: Add {G}.\" Draw a card.",
        "Each land and Ally you control has flying.",
        "Each land and Ally you control has \"{T}: Add {G}.\" if you control an artifact.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("collection-domain-grant", &tokens);
        assert!(
            !matches!(
                try_recognize_static_collection_activated_grant(&mut cursor, &[], None),
                StaticCollectionActivatedGrantAttempt::Parsed(_)
            ),
            "{text}"
        );
        assert_eq!(
            cursor.position(),
            0,
            "unclaimed or rejected full envelope: {text}"
        );
    }
}

#[test]
fn registered_plural_surfaces_are_shared_by_tokens_and_query_vocabulary() {
    use crate::oracle::characteristic_vocabulary::{AcceptedForms, lookup_creature_subtype};
    let text = "Allies and Wizards and Imaginarytypes";
    let tokens = recognize_entities(text);
    for (surface, canonical) in [("Allies", "Ally"), ("Wizards", "Wizard")] {
        assert!(lookup_creature_subtype(surface, AcceptedForms::SingularOnly).is_none());
        assert_eq!(
            lookup_creature_subtype(surface, AcceptedForms::SingularOrPlural)
                .map(|subtype| subtype.ir_name()),
            Some(canonical),
        );
        let token = tokens.iter().find(|token| token.raw == surface).unwrap();
        assert_eq!(token.token_type, "subtypeWord");
        assert_eq!(token.category, "entityReference");
        assert_eq!(token.span_start, text.find(surface).unwrap());
        assert_eq!(&text[token.span_start..token.span_end], surface);
    }
    let unknown = tokens
        .iter()
        .find(|token| token.raw == "Imaginarytypes")
        .unwrap();
    assert_eq!(unknown.token_type, "unknown");
    assert!(lookup_creature_subtype("Allys", AcceptedForms::SingularOrPlural).is_none());
    assert!(lookup_creature_subtype("Wizardzz", AcceptedForms::SingularOrPlural).is_none());
}
