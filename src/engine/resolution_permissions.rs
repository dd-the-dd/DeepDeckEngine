//! Decision-domain facts used by the shared resolution preflight visitor.
use serde_json::Value;

pub(super) fn node_requires_checkpoint(value: &Value) -> bool {
    matches!(
        value["kind"].as_str(),
        Some(
            "lookAtTopCards"
                | "optionalEffects"
                | "chooseOneEffect"
                | "discardCards"
                | "choosePermanents"
                | "sacrificePermanents"
                | "chooseCards"
                | "chooseOrder"
        )
    ) || value["optional"].as_bool() == Some(true)
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "resolution_permissions_tests.rs"]
mod tests;
