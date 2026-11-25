//! Static exceptions to an evasion keyword (CR 702.14, 509.1b): `Creatures
//! with swampwalk can be blocked as though they didn't have swampwalk.`
//!
//! `ignoreBlockEvasion { objects, keyword }` keeps the keyword on the
//! objects (selectors and counts still see it); only the blocking
//! restriction that keyword imposes is not applied while the modifier's
//! static ability is active. The objects are evaluated relative to the
//! modifier's source when blockers are declared.
use super::*;

/// Validation of the modifier this module owns. `None` leaves any other
/// modifier to the battlefield modifier validator.
pub(super) fn modifier_supported(modifier: &Value) -> Option<bool> {
    if value_kind(modifier) != Some("ignoreBlockEvasion") {
        return None;
    }
    Some(
        static_object_selector_supported(&modifier["objects"])
            && modifier["keyword"]
                .as_str()
                .is_some_and(super::landwalk::is_landwalk_keyword),
    )
}

impl GameEngine {
    /// Whether an active static exception lets `attacker` be blocked as
    /// though it didn't have the evasion `keyword`.
    pub(super) fn block_evasion_ignored(&self, attacker: &CardInstance, keyword: &str) -> bool {
        self.battlefield_static_modifiers()
            .iter()
            .any(|(source, modifier)| {
                value_kind(modifier) == Some("ignoreBlockEvasion")
                    && modifier["keyword"].as_str() == Some(keyword)
                    && Self::static_selector_matches(source, &modifier["objects"], attacker)
            })
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "evasion_exception_tests.rs"]
mod tests;
