use serde_json::json;

use super::{mana_cost_supported, resolve_mana_cost};

#[test]
fn fixed_and_dynamic_mana_costs_are_mutually_exclusive() {
    let fixed = json!({ "kind": "payMana", "manaCost": "{2}" });
    let dynamic = json!({
        "kind": "payMana",
        "genericAmount": { "kind": "powerOf", "object": { "kind": "self" } },
    });
    let ambiguous = json!({
        "kind": "payMana",
        "manaCost": "{2}",
        "genericAmount": { "kind": "integer", "value": 2 },
    });

    assert!(mana_cost_supported(&fixed, |_| true, |_| false));
    assert!(mana_cost_supported(&dynamic, |_| false, |_| true));
    assert!(!mana_cost_supported(&ambiguous, |_| true, |_| true));
}

#[test]
fn dynamic_generic_amount_materializes_and_clamps_at_zero() {
    let cost = json!({
        "kind": "payMana",
        "genericAmount": { "kind": "powerOf", "object": { "kind": "self" } },
    });

    assert_eq!(
        resolve_mana_cost(&cost, |_| Some(3)).as_deref(),
        Some("{3}")
    );
    assert_eq!(
        resolve_mana_cost(&cost, |_| Some(-2)).as_deref(),
        Some("{0}")
    );
}
