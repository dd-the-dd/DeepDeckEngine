use super::*;
use crate::oracle::syntax::recognize_entities;

fn complete(text: &str) -> MachineResult<super::super::ObjectQueryNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("coordinated-article", &tokens);
    let result = recognize_object_query(&mut cursor)?;
    cursor.finish()?;
    Ok(result)
}

#[test]
fn coordinated_articles_preserve_the_same_typed_query_and_qualifier_scopes() {
    for (with_articles, without_articles) in [
        ("Island or a Swamp", "Island or Swamp"),
        ("Island, a Swamp, or a Forest", "Island, Swamp, or Forest"),
        (
            "artifact or an enchantment or a creature",
            "artifact or enchantment or creature",
        ),
        (
            "artifact or a creature with flying",
            "artifact or creature with flying",
        ),
        (
            "artifact or a creature with mana value 3 or less",
            "artifact or creature with mana value 3 or less",
        ),
        (
            "artifact or a creature that's red or green",
            "artifact or creature that's red or green",
        ),
        (
            "artifact or a creature you control",
            "artifact or creature you control",
        ),
    ] {
        assert_eq!(
            complete(with_articles).unwrap(),
            complete(without_articles).unwrap()
        );
    }
}

#[test]
fn articles_do_not_change_parent_boundaries_or_admit_missing_heads() {
    let tokens = recognize_entities("Island or a Swamp or you control a Forest,");
    let mut cursor = OracleTokenCursor::new("coordinated-article", &tokens);
    recognize_object_query_before_condition_connector(&mut cursor).unwrap();
    assert_eq!(cursor.peek(), Some("or"));

    let tokens = recognize_entities("artifact or a creature you control, draw");
    let mut cursor = OracleTokenCursor::new("coordinated-article", &tokens);
    recognize_object_query_before_separator(&mut cursor).unwrap();
    assert_eq!(cursor.peek(), Some(","));

    for text in [
        "Island or a",
        "Island or a an Swamp",
        "Island or an opponent",
        "creature you control or an artifact an opponent controls",
    ] {
        assert!(complete(text).is_err(), "{text}");
    }
}
