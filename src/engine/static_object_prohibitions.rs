//! Passive prohibitions a static ability places on battlefield objects
//! (CR 604.1): what may not happen to them, whoever would do it.
//!
//! - `cantBeTargeted {objects, sources}` (CR 115.4): each `sources` branch
//!   names a kind of targeting stack object (`spell` or `ability`), the
//!   characteristics of its source (`where`, a card filter of the spell or of
//!   the ability's source) and its controller relative to the prohibition's
//!   controller. A matching spell or ability can't target the objects.
//! - `cantBecomeUntapped {objects}` (CR 502.3, 701.26b): every untap attempt,
//!   in the untap step or by an effect, leaves the objects tapped.
//!
//! `cantBeSacrificed` and `preventCounters` (optionally of one `counter`
//! kind, CR 122.1) keep their existing readers; this module validates them
//! with the same selector contract.
use super::*;

fn targeting_source_supported(source: &Value) -> bool {
    matches!(source["stackObject"].as_str(), Some("spell" | "ability"))
        && source.get("where").is_none_or(card_filter_supported)
        && source.get("controller").is_none_or(|controller| {
            player_reference_supported(controller)
                || (value_kind(controller) == Some("opponentsOf")
                    && player_reference_supported(&controller["player"]))
        })
}

/// Validation of the prohibitions this module evaluates; `None` for any
/// other modifier kind.
pub(super) fn static_object_prohibition_supported(modifier: &Value) -> Option<bool> {
    let condition = modifier.get("condition").is_none_or(condition_supported);
    Some(match value_kind(modifier) {
        Some("cantBeTargeted") => {
            static_object_selector_supported(&modifier["objects"])
                && condition
                && modifier["sources"].as_array().is_some_and(|sources| {
                    !sources.is_empty() && sources.iter().all(targeting_source_supported)
                })
        }
        Some("cantBecomeUntapped") => {
            static_object_selector_supported(&modifier["objects"]) && condition
        }
        _ => return None,
    })
}

impl GameEngine {
    /// Whether one targeting relation of `controller`'s spell or ability is
    /// named by a branch: the prohibition's own controller decides `you` and
    /// `your opponents`.
    fn targeting_source_matches(
        prohibition_controller: &str,
        branch: &Value,
        targeting_controller: &str,
        source: &CardDefinition,
        is_spell: bool,
    ) -> bool {
        let kind_matches = match branch["stackObject"].as_str() {
            Some("spell") => is_spell,
            Some("ability") => !is_spell,
            _ => false,
        };
        let controller_matches = match value_kind(&branch["controller"]) {
            None => true,
            Some("opponentsOf") => targeting_controller != prohibition_controller,
            Some(_) => targeting_controller == prohibition_controller,
        };
        kind_matches
            && controller_matches
            && branch
                .get("where")
                .is_none_or(|filter| matches_card_filter(source, filter))
    }

    /// CR 115.4: a static `cantBeTargeted` forbids this spell (`is_spell`)
    /// or ability, controlled by `targeting_controller` and whose source is
    /// `source`, from targeting `permanent`.
    pub(super) fn static_targeting_prohibited(
        &self,
        permanent: &CardInstance,
        targeting_controller: &str,
        source: &CardDefinition,
        is_spell: bool,
    ) -> bool {
        self.battlefield_static_modifiers()
            .iter()
            .any(|(grantor, modifier)| {
                value_kind(modifier) == Some("cantBeTargeted")
                    && Self::static_selector_matches(grantor, &modifier["objects"], permanent)
                    && modifier.get("condition").is_none_or(|condition| {
                        self.condition_value_with_source(
                            condition,
                            &grantor.controller,
                            &BTreeMap::new(),
                            Some(grantor),
                        )
                    })
                    && modifier["sources"].as_array().is_some_and(|branches| {
                        branches.iter().any(|branch| {
                            Self::targeting_source_matches(
                                &grantor.controller,
                                branch,
                                targeting_controller,
                                source,
                                is_spell,
                            )
                        })
                    })
            })
    }

    /// CR 701.26b: a static `cantBecomeUntapped` keeps the permanent tapped.
    pub(super) fn static_untap_prohibited(&self, permanent: &CardInstance) -> bool {
        self.static_modifier_applies_to("cantBecomeUntapped", permanent)
    }

    /// CR 122.1: a static `preventCounters` forbids every counter, or only
    /// its own `counter` kind.
    pub(super) fn static_counter_prohibited(
        &self,
        permanent: &CardInstance,
        counter: &str,
    ) -> bool {
        self.static_modifier_applies_to_where("preventCounters", permanent, |modifier| {
            modifier["counter"]
                .as_str()
                .is_none_or(|kind| kind.eq_ignore_ascii_case(counter))
        })
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "static_object_prohibitions_tests.rs"]
mod tests;
