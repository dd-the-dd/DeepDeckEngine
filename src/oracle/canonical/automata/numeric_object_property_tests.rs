use crate::oracle::syntax::recognize_entities;

use super::super::{LocalObjectReference, NumericAtom, NumericExpression, NumericObjectProperty};
use super::{
    OracleTokenCursor, recognize_source_object_property,
    recognize_source_property_variable_definition,
};

fn source_property(property: NumericObjectProperty) -> NumericExpression {
    NumericExpression::ObjectProperty {
        object: LocalObjectReference::SourceSelf,
        property,
    }
}

#[test]
fn source_property_accepts_the_typed_source_nouns_and_stats() {
    for (text, property) in [
        ("this creature's power", NumericObjectProperty::Power),
        (
            "this permanent's toughness",
            NumericObjectProperty::Toughness,
        ),
        ("this card's power", NumericObjectProperty::Power),
        ("this artifact's power", NumericObjectProperty::Power),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("numeric-object-property-test", &tokens);

        assert_eq!(
            recognize_source_object_property(&mut cursor).expect("source property parses"),
            source_property(property),
        );
        cursor.finish().expect("the source property is complete");
    }
}

#[test]
fn variable_definition_binds_x_to_the_shared_source_property_leaf() {
    let tokens = recognize_entities("where X is this creature's power");
    let mut cursor = OracleTokenCursor::new("numeric-variable-definition-test", &tokens);

    assert_eq!(
        recognize_source_property_variable_definition(&mut cursor, NumericAtom::VariableX)
            .expect("the X definition parses"),
        source_property(NumericObjectProperty::Power),
    );
    cursor
        .finish()
        .expect("the variable definition is complete");
}

#[test]
fn anaphoric_or_unsupported_object_nouns_do_not_become_the_source() {
    for text in ["that creature's power", "this player's power"] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("numeric-object-property-test", &tokens);

        assert!(
            recognize_source_object_property(&mut cursor).is_err(),
            "{text} must not become a self-reference",
        );
    }
}

#[test]
fn variable_definition_rejects_a_parent_request_for_a_different_value() {
    let tokens = recognize_entities("where X is this creature's power");
    let mut cursor = OracleTokenCursor::new("numeric-variable-definition-test", &tokens);

    let failure =
        recognize_source_property_variable_definition(&mut cursor, NumericAtom::Integer(1))
            .expect_err("the child must bind only the variable requested by the parent");

    assert_eq!(failure.state, "numeric.variable-definition.variable");
    assert!(failure.expected.contains("X variable binding"));
}
