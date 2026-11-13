use crate::oracle::syntax::recognize_entities;

use super::{LocalObjectReference, OracleTokenCursor, recognize_sacrifice_source_effect};

#[test]
fn typed_this_source_is_sacrificed_atomically() {
    let tokens = recognize_entities("sacrifice this land.");
    let mut cursor = OracleTokenCursor::new("test.sacrifice-source", &tokens);

    let effect = recognize_sacrifice_source_effect(
        &mut cursor,
        &[],
        None,
        Some(&LocalObjectReference::SourceSelf),
    )
    .expect("source sacrifice parses");

    assert_eq!(effect.source, LocalObjectReference::SourceSelf);
    assert_eq!(cursor.peek(), Some("."));
}

#[test]
fn bound_source_anaphor_reuses_the_same_sacrifice_action_child() {
    let tokens = recognize_entities("sacrifice it");
    let mut cursor = OracleTokenCursor::new("test.sacrifice-source", &tokens);

    let effect = recognize_sacrifice_source_effect(
        &mut cursor,
        &[],
        None,
        Some(&LocalObjectReference::SourceSelf),
    )
    .expect("bound source anaphor parses");

    assert_eq!(effect.source, LocalObjectReference::SourceSelf);
    cursor
        .finish()
        .expect("the atomic action is fully consumed");
}
