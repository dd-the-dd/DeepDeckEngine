use super::super::{
    ResourcePaymentNode, StaticCollectionKeywordRecognition, recognize_standalone_keyword_ability,
    try_recognize_static_collection_keyword_grant,
};
use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn life_ward_is_typed_resource_and_keeps_list_boundaries() {
    let tokens = recognize_entities("Flying, ward—Pay 3 life.");
    let mut cursor = OracleTokenCursor::new("test.ward-resource", &tokens);
    let list = recognize_standalone_keyword_ability(&mut cursor).unwrap();
    assert_eq!(list.keywords.len(), 2);
    assert!(matches!(&list.keywords[1],KeywordAtomNode::Ward {
        cost:ResourcePaymentNode::Life(cost)
    } if cost.amount==3));
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn ward_life_reminder_uses_third_person_shared_payment_and_exact_resource() {
    let good = "Ward—Pay 2 life (Whenever this creature becomes the target of a spell or ability an opponent controls, counter it unless that player pays 2 life.)";
    let tokens = recognize_entities(good);
    let mut cursor = OracleTokenCursor::new("test.ward-life-reminder", &tokens);
    assert!(recognize_standalone_keyword_ability(&mut cursor).is_ok());
    for bad in [
        good.replace("pays 2 life", "pays 3 life"),
        good.replace("pays 2 life", "pays {2}"),
    ] {
        let tokens = recognize_entities(&bad);
        let mut cursor = OracleTokenCursor::new("test.ward-life-reminder-negative", &tokens);
        let error = recognize_standalone_keyword_ability(&mut cursor).unwrap_err();
        assert_eq!(error.state, "keyword.ward.reminder.cost-match");
    }
}

#[test]
fn variable_and_source_relative_keyword_payments_are_not_payer_resources() {
    for text in [
        "Ward—Pay X life.",
        "Ward—Sacrifice this creature.",
        "Ward—Discard a card.",
        "Ward—Pay 2147483648 life.",
        "Ward {X}",
        "Ward {NOPE}",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.ward-resource-negative", &tokens);
        assert!(
            recognize_standalone_keyword_ability(&mut cursor).is_err(),
            "{text}"
        );
    }
}

#[test]
fn quoted_grant_retains_keyword_parameters_and_other_source_binding() {
    let tokens = recognize_entities("Other creatures you control have \"Ward—Pay 2 life.\"");
    assert!(
        !tokens
            .iter()
            .any(|token| token.raw == "Ward" && token.category == "unknown")
    );
    let mut cursor = OracleTokenCursor::new("test.quoted-resource", &tokens);
    let StaticCollectionKeywordRecognition::Parsed(grant) =
        try_recognize_static_collection_keyword_grant(&mut cursor)
    else {
        panic!("the complete quoted keyword grant is recognized")
    };
    assert!(grant.exclude_source);
    assert!(
        matches!(&grant.keywords[0],KeywordAtomNode::Ward{cost:ResourcePaymentNode::Life(cost)} if cost.amount==2)
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn quoted_and_unquoted_parameterless_lists_keep_their_typed_atoms() {
    for text in [
        "Creatures you control have flying and vigilance.",
        "Creatures you control have \"Flying and vigilance.\"",
        "Creatures you control have \"Flying and vigilance\".",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.keyword-quote-list", &tokens);
        let StaticCollectionKeywordRecognition::Parsed(grant) =
            try_recognize_static_collection_keyword_grant(&mut cursor)
        else {
            panic!("{text}")
        };
        assert_eq!(grant.keywords.len(), 2);
        assert!(matches!(
            grant.keywords[0],
            KeywordAtomNode::Parameterless(GrantedKeyword::Flying)
        ));
        assert!(!grant.exclude_source);
    }
}

#[test]
fn quotes_and_conditional_temporal_tails_cannot_be_discarded() {
    for text in [
        "Creatures you control have \"Ward—Pay 2 life.",
        "Creatures you control have \"Ward—Pay 2 life.\" until end of turn.",
        "Creatures you control have \"Draw a card.\"",
        "Creatures you control have ward {2} as long as you control an Island.",
        "Creatures you control have \"Ward—Pay 2 life.\" You gain 1 life.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.keyword-quote-negative", &tokens);
        assert!(
            !matches!(
                try_recognize_static_collection_keyword_grant(&mut cursor),
                StaticCollectionKeywordRecognition::Parsed(_)
            ),
            "{text}"
        );
    }
}
