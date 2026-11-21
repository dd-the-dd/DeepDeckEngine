use super::class_level_header_from_tokens;
use crate::oracle::syntax::recognize_entities;

fn recognize(text: &str) -> Option<i64> {
    let tokens = recognize_entities(text);
    class_level_header_from_tokens(&tokens)
}

#[test]
fn class_level_header_consumes_mana_colon_level_and_integer() {
    assert_eq!(recognize("{1}{B}: Level 2"), Some(2));
    assert_eq!(recognize("{3}{G}{G}: LEVEL 12"), Some(12));
}

#[test]
fn class_level_header_rejects_levels_below_two_and_non_integer_values() {
    assert_eq!(recognize("{1}{B}: Level 1"), None);
    assert_eq!(recognize("{1}{B}: Level X"), None);
}

#[test]
fn class_level_header_requires_complete_token_consumption() {
    assert_eq!(recognize("{1}{B}: Level 2 now"), None);
    assert_eq!(recognize("{1}{B}: Level 2."), None);
}
