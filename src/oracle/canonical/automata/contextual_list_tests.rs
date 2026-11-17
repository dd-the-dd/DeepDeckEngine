use crate::oracle::syntax::recognize_entities;

use super::{
    BareCommaPolicy, ContextualListConnector, ContextualListPolicy, OracleTokenCursor,
    recognize_contextual_list,
};

fn recognize_word(cursor: &mut OracleTokenCursor<'_>) -> super::MachineResult<String> {
    cursor.enter("contextual-list-test.item");
    let raw = cursor
        .peek()
        .filter(|raw| !matches!(*raw, "," | "and" | "or" | "then"))
        .ok_or_else(|| cursor.fail("a test list item"))?
        .to_string();
    cursor.consume(&raw)?;
    Ok(raw)
}

#[test]
fn oxford_list_resolves_every_serial_comma_from_the_final_connector() {
    let tokens = recognize_entities("one, two, three, and four");
    let mut cursor = OracleTokenCursor::new("contextual-list-test", &tokens);
    let list = recognize_contextual_list(
        &mut cursor,
        ContextualListPolicy::serial(&[ContextualListConnector::And]),
        recognize_word,
    )
    .expect("Oxford conjunction parses");
    cursor.finish().expect("the complete list is consumed");

    assert_eq!(list.items, ["one", "two", "three", "four"]);
    assert_eq!(
        list.connectors,
        [
            ContextualListConnector::And,
            ContextualListConnector::And,
            ContextualListConnector::And,
        ]
    );
}

#[test]
fn parent_can_assign_then_semantics_to_bare_commas() {
    let tokens = recognize_entities("draw, discard, investigate");
    let mut cursor = OracleTokenCursor::new("contextual-list-test", &tokens);
    let list = recognize_contextual_list(
        &mut cursor,
        ContextualListPolicy::with_comma_meaning(
            &[ContextualListConnector::Then],
            ContextualListConnector::Then,
        ),
        recognize_word,
    )
    .expect("parent-owned comma sequencing parses");
    cursor.finish().expect("the complete list is consumed");

    assert_eq!(
        list.connectors,
        [ContextualListConnector::Then, ContextualListConnector::Then]
    );
}

#[test]
fn unresolved_bare_commas_fail_when_the_parent_requires_a_final_word() {
    let tokens = recognize_entities("one, two, three");
    let mut cursor = OracleTokenCursor::new("contextual-list-test", &tokens);
    let failure = recognize_contextual_list(
        &mut cursor,
        ContextualListPolicy {
            connectors: &[ContextualListConnector::And, ContextualListConnector::Or],
            bare_comma: BareCommaPolicy::RequireLaterConnector,
        },
        recognize_word,
    )
    .expect_err("an unresolved serial list must not guess its Boolean meaning");

    assert_eq!(failure.state, "contextual-list.complete");
    assert!(failure.expected.contains("final connector"));
}

#[test]
fn connector_not_owned_by_the_parent_is_a_committed_failure() {
    let tokens = recognize_entities("one or two");
    let mut cursor = OracleTokenCursor::new("contextual-list-test", &tokens);
    let failure = recognize_contextual_list(
        &mut cursor,
        ContextualListPolicy::serial(&[ContextualListConnector::And]),
        recognize_word,
    )
    .expect_err("a conjunction-only parent must reject disjunction");

    assert_eq!(failure.state, "contextual-list.separator-or-complete");
    assert_eq!(failure.actual.as_deref(), Some("or"));
}
