use super::*;
use serde_json::json;

fn forbidden(values: &[&str]) -> HashSet<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn card(value: Value) -> ScryfallCard {
    serde_json::from_value(value).expect("valid audit card fixture")
}

#[test]
fn recursively_finds_forbidden_kinds_and_sorts_them() {
    let rule = json!({
        "kind": "triggered",
        "effects": [
            { "kind": "resolveTriggeredInstruction" },
            { "branches": [{ "kind": "drawThenDiscard" }] }
        ]
    });
    assert_eq!(
        classify_rule_values(
            [Some(&rule)],
            &forbidden(&["resolveTriggeredInstruction", "drawThenDiscard"])
        ),
        IrArchitecture::Composite(vec![
            "drawThenDiscard".to_string(),
            "resolveTriggeredInstruction".to_string()
        ])
    );
}

#[test]
fn accepts_nested_ir_when_every_kind_is_primitive() {
    let rule = json!({
        "kind": "spell",
        "effects": [
            { "kind": "exilePermanent" },
            { "kind": "gainLife", "amount": { "kind": "powerOf" } }
        ]
    });
    assert_eq!(
        classify_rule_values([Some(&rule)], &forbidden(&["resolveSpellInstruction"])),
        IrArchitecture::Primitive
    );
}

#[test]
fn canonical_ability_without_rule_is_not_primitive() {
    assert_eq!(
        classify_rule_values([None], &HashSet::new()),
        IrArchitecture::Composite(vec!["missingCanonicalRule".to_string()])
    );
}

#[test]
fn audit_key_disambiguates_cards_with_the_same_collector_number() {
    let first = card(json!({
        "id": "print-a",
        "name": "First",
        "collector_number": "1",
        "audit_key": "queue-00000001",
        "layout": "normal"
    }));
    let second = card(json!({
        "id": "print-b",
        "name": "Second",
        "collector_number": "1",
        "audit_key": "queue-00000002",
        "layout": "normal"
    }));

    assert_eq!(first.audit_identity(), "queue-00000001");
    assert_eq!(second.audit_identity(), "queue-00000002");
}

#[test]
fn audit_identity_falls_back_to_collector_for_full_set_audits() {
    let card = card(json!({
        "id": "print-a",
        "name": "Full Set Card",
        "collector_number": "42",
        "layout": "normal"
    }));

    assert_eq!(card.audit_identity(), "42");
}

#[test]
fn strict_prefix_stops_on_every_non_native_recognizer_class() {
    assert!(!should_stop_at_first_non_native(
        true,
        CardRecognizerStatus::Native
    ));
    assert!(should_stop_at_first_non_native(
        true,
        CardRecognizerStatus::Mixed
    ));
    assert!(should_stop_at_first_non_native(
        true,
        CardRecognizerStatus::Legacy
    ));
    assert!(should_stop_at_first_non_native(
        true,
        CardRecognizerStatus::Unsupported
    ));
    assert!(!should_stop_at_first_non_native(
        false,
        CardRecognizerStatus::Legacy
    ));
}
