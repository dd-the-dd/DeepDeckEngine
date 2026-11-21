use super::*;
use crate::oracle::syntax::{
    ExactNameTokenIndex, recognize_oracle_entities, resolve_named_token_spans,
};

fn tokens(text: &str) -> Vec<crate::oracle::model::RecognizedEntity> {
    let mut tokens = recognize_oracle_entities(text);
    let index = ExactNameTokenIndex::from_names(crate::card_catalog::named_token_names().unwrap());
    resolve_named_token_spans(&mut tokens, &index);
    tokens
}

#[test]
fn alternatives_are_unbounded_and_sequence_connectors_remain_with_the_parent() {
    for text in [
        "Create a Food token or two Treasure tokens or a Clue token.",
        "Create a Food token, two Treasure tokens, or a Clue token.",
    ] {
        let tokens = tokens(text);
        let mut cursor = OracleTokenCursor::new("test.token-options", &tokens);
        let creation =
            super::super::recognize_create_named_tokens_effect(&mut cursor, None).unwrap();
        let NamedTokenCreationPattern::OneOf(specs) = creation.creation else {
            panic!("typed alternatives")
        };
        assert_eq!(specs.len(), 3);
        assert_eq!(specs[1].quantity, NumericAtom::Integer(2));
        cursor.consume(".").unwrap();
        cursor.finish().unwrap();
    }
    let tokens = tokens("Create a Food token and you gain 2 life.");
    let mut cursor = OracleTokenCursor::new("test.token-boundary", &tokens);
    let creation = super::super::recognize_create_named_tokens_effect(&mut cursor, None).unwrap();
    assert!(matches!(
        creation.creation,
        NamedTokenCreationPattern::Single(_)
    ));
    assert_eq!(cursor.peek(), Some("and"));
}

#[test]
fn unknown_dangling_and_unproved_simultaneous_or_inclusive_lists_fail() {
    for text in [
        "Create a Food token or.",
        "Create a Food token or a Missingthing token.",
        "Create a Food token and a Treasure token.",
        "Create a Food token and/or a Treasure token.",
        "Create a Food token, a Treasure token.",
    ] {
        let tokens = tokens(text);
        let mut cursor = OracleTokenCursor::new("test.token-negative", &tokens);
        assert!(
            super::super::recognize_create_named_tokens_effect(&mut cursor, None).is_err(),
            "{text}"
        );
    }
}

#[test]
fn composed_reminders_share_catalog_semantics_and_require_all_descriptions() {
    for subjects in [false, true] {
        let (food, treasure) = if subjects {
            ("A Food token", "A Treasure token")
        } else {
            ("Food", "Treasure")
        };
        for life in [3, 4] {
            let text = format!(
                "Create a Food token or a Treasure token. ({food} is an artifact with \"{{2}}, {{T}}, Sacrifice this token: You gain {life} life.\" {treasure} is an artifact with \"{{T}}, Sacrifice this token: Add one mana of any color.\")"
            );
            let tokens = tokens(&text);
            let mut cursor = OracleTokenCursor::new("test.token-reminders", &tokens);
            let creation =
                super::super::recognize_create_named_tokens_effect(&mut cursor, None).unwrap();
            cursor.consume(".").unwrap();
            let result = super::super::recognize_named_token_creation_reminder(
                &mut cursor,
                &creation.creation,
            );
            assert_eq!(result.is_ok(), life == 3);
            if life == 3 {
                cursor.finish().unwrap();
            }
        }
    }
}

#[test]
fn an_earlier_keyword_does_not_own_the_coordinated_token_reminder() {
    let text = "Scry 1, then create a Food token or a Treasure token. (Food is an artifact with \"{2}, {T}, Sacrifice this token: You gain 3 life.\" Treasure is an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")";
    let tokens = tokens(text);
    let mut cursor = OracleTokenCursor::new("test.token-reminder-owner", &tokens);
    let sequence = super::super::recognize_effect_sequence(&mut cursor, &[], None).unwrap();
    assert!(matches!(
        sequence.effects[0],
        super::super::LocalEffectNode::Scry(_)
    ));
    assert!(matches!(
        sequence.effects[1],
        super::super::LocalEffectNode::CreateNamedTokens(_)
    ));
    cursor.finish().unwrap();
}
