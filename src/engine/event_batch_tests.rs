use super::*;
use serde_json::json;

#[test]
fn aggregation_field_accepts_only_known_policies() {
    assert!(event_aggregation_supported(
        &json!({"kind": "permanentDied"})
    ));
    assert!(event_aggregation_supported(
        &json!({"aggregation": "oneOrMore"})
    ));
    assert!(event_aggregation_supported(
        &json!({"aggregation": "perObject"})
    ));
    assert!(!event_aggregation_supported(
        &json!({"aggregation": "twoOrMore"})
    ));
    assert!(event_is_aggregated(&json!({"aggregation": "oneOrMore"})));
    assert!(!event_is_aggregated(&json!({"kind": "permanentEntered"})));
}
