use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn ordinary_query_and_source_are_distinct_operands() {
    for text in ["sacrifice a land.", "sacrifice two artifacts."] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.sacrifice", &tokens);
        assert!(matches!(
            recognize_ordinary_sacrifice_effect(&mut cursor, &[], None, None).unwrap(),
            SacrificeEffectNode::Selected { .. }
        ));
        assert_eq!(cursor.peek(), Some("."));
    }
    let tokens = recognize_entities("sacrifice this land.");
    let mut cursor = OracleTokenCursor::new("test.sacrifice", &tokens);
    assert!(matches!(
        recognize_ordinary_sacrifice_effect(&mut cursor, &[], None, None).unwrap(),
        SacrificeEffectNode::Source(_)
    ));
}

#[test]
fn coordinated_heads_keep_the_parent_quantity_contract() {
    for (text, valid) in [
        ("sacrifice a creature or an artifact.", true),
        ("sacrifice a creature, an artifact, or a land.", true),
        ("sacrifice two artifacts and/or creatures.", true),
        ("sacrifice a creature and/or an artifact.", false),
        ("sacrifice two artifacts or a creature.", false),
        ("sacrifice a creature, an artifact, and a land.", false),
        ("sacrifice another creature.", false),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.sacrifice.coordination", &tokens);
        let result =
            recognize_ordinary_sacrifice_effect(&mut cursor, &[], None, None).and_then(|_| {
                cursor.consume(".")?;
                cursor.finish()
            });
        assert_eq!(result.is_ok(), valid, "{text}");
    }
}
