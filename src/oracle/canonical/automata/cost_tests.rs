use crate::oracle::canonical::{
    ObjectCardType, OracleTokenCursor, SacrificeSelfSourceNoun, recognize_sacrifice_self_cost,
};
use crate::oracle::syntax::recognize_entities;

fn parse(text: &str) -> super::SacrificeSelfCostNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.sacrifice-self-cost", &tokens);
    let cost = recognize_sacrifice_self_cost(&mut cursor).expect("cost parses");
    cursor.finish().expect("cost consumes every token");
    cost
}

#[test]
fn sacrifice_self_accepts_the_generic_token_source_noun() {
    assert_eq!(
        parse("Sacrifice this token").source_type,
        SacrificeSelfSourceNoun::Token
    );
}

#[test]
fn sacrifice_self_preserves_existing_permanent_type_assertions() {
    assert_eq!(
        parse("Sacrifice this creature").source_type,
        SacrificeSelfSourceNoun::PermanentType(ObjectCardType::Creature)
    );
}

#[test]
fn sacrifice_self_rejects_an_unknown_source_noun_precisely() {
    let tokens = recognize_entities("Sacrifice this contraption");
    let mut cursor = OracleTokenCursor::new("test.sacrifice-self-cost", &tokens);

    let failure = recognize_sacrifice_self_cost(&mut cursor)
        .expect_err("an unknown source noun must not be swallowed");

    assert_eq!(failure.state, "cost.sacrifice-self.source-type");
    assert_eq!(failure.actual.as_deref(), Some("contraption"));
}
