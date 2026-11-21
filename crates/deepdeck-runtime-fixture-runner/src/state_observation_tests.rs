use super::*;
use serde_json::json;
#[test]
fn field_matching_distinguishes_absent_null_case_and_scalar_types() {
    let value =
        json!({"viewerId":"p0","zonePlayerId":"p1","count":2,"nullable":null,"cards":["A","B"]});
    for fields in [
        json!({"viewerId":"p0","zonePlayerId":"p1","count":2}),
        json!({"nullable":null}),
        json!({"cards":["A","B"]}),
    ] {
        let fields: BTreeMap<String, Value> = serde_json::from_value(fields).unwrap();
        assert!(matches_fields(&value, &fields));
    }
    for fields in [
        json!({"absent":null}),
        json!({"viewerId":"P0"}),
        json!({"ViewerId":"p0"}),
        json!({"count":"2"}),
        json!({"cards":["B","A"]}),
    ] {
        let fields: BTreeMap<String, Value> = serde_json::from_value(fields).unwrap();
        assert!(!matches_fields(&value, &fields));
    }
    assert!(!matches_fields(&Value::Null, &BTreeMap::new()));
}
#[test]
fn observation_variants_deserialize_without_changing_existing_event_count_schema() {
    for value in [
        json!({"kind":"eventDetails","eventKind":"cardsLookedAt","playerId":"p0","fields":{"zonePlayerId":"p1"},"equals":1}),
        json!({"kind":"ruleModifierCount","fields":{"kind":"knownLibraryCards","viewerId":"p0"},"equals":1}),
        json!({"kind":"eventCount","eventKind":"cardsLookedAt","equals":1}),
    ] {
        serde_json::from_value::<crate::FixtureAssertion>(value).unwrap();
    }
    assert!(
        serde_json::from_value::<crate::FixtureAssertion>(
            json!({"kind":"eventDetails","eventKind":"cardsLookedAt","equals":1})
        )
        .is_err()
    );
}
