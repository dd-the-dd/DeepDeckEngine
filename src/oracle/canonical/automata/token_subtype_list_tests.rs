use super::recognize_token_subtype_list;
use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

fn parse_subtypes(text: &str) -> Vec<&'static str> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.token-subtype-list", &tokens);
    let parsed = recognize_token_subtype_list(&mut cursor).expect("subtype list parses");
    cursor.consume("creature").expect("parent noun remains");
    cursor.finish().expect("the phrase is fully consumed");
    parsed
        .subtypes
        .into_iter()
        .map(|subtype| subtype.ir_name())
        .collect()
}

#[test]
fn recognizes_exact_single_and_multiple_creature_subtypes() {
    assert_eq!(parse_subtypes("Elephant creature"), ["Elephant"]);
    assert_eq!(
        parse_subtypes("Phyrexian Goblin creature"),
        ["Phyrexian", "Goblin"]
    );
}

#[test]
fn recognizes_a_registered_multiword_subtype_as_one_value_in_type_context() {
    assert_eq!(parse_subtypes("Time Lord creature"), ["Time Lord"]);
}

#[test]
fn rejects_an_unregistered_capitalized_word_at_the_local_state() {
    let tokens = recognize_entities("Imaginarytype creature");
    let mut cursor = OracleTokenCursor::new("test.token-subtype-list", &tokens);
    let failure = recognize_token_subtype_list(&mut cursor).expect_err("unknown subtype fails");

    assert_eq!(
        failure.state,
        "effect.create-token.subtype-list.item-or-creature"
    );
    assert_eq!(failure.actual.as_deref(), Some("Imaginarytype"));
    assert_eq!(failure.expected, "a registered singular creature subtype");
}

#[test]
fn rejects_unversioned_plural_guessing() {
    let tokens = recognize_entities("Elephants creature");
    let mut cursor = OracleTokenCursor::new("test.token-subtype-list", &tokens);
    let failure = recognize_token_subtype_list(&mut cursor).expect_err("plural is not inferred");

    assert_eq!(failure.actual.as_deref(), Some("Elephants"));
    assert_eq!(failure.expected, "a registered singular creature subtype");
}
