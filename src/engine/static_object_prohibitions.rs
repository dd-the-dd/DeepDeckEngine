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

    /// CR 303.4d, 702.16: an Aura with a quality the enchanted permanent has
    /// protection from is put into its owner's graveyard, unless every
    /// protection from that quality is a static `grantProtection` whose
    /// `sparesAuras` names this Aura (`source`, `This effect doesn't remove
    /// this Aura`) or every Aura (`all`). Called only for a host already
    /// protected from the Aura; any other protecting effect still removes it.
    pub(super) fn protection_spares_aura(&self, host: &CardInstance, aura: &CardInstance) -> bool {
        let source = &aura.definition;
        let protects = |qualities: &Value| {
            qualities.as_array().is_some_and(|qualities| {
                qualities
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|quality| card_matches_protection_quality(source, quality))
            })
        };
        let mut spared = false;
        for (grantor, modifier) in self.battlefield_static_modifiers() {
            if !Self::static_selector_matches(&grantor, &modifier["objects"], host) {
                continue;
            }
            match value_kind(&modifier) {
                Some("grantProtection") if protects(&modifier["from"]) => {
                    let spares = match modifier["sparesAuras"].as_str() {
                        Some("source") => grantor.instance_id == aura.instance_id,
                        Some("all") => true,
                        _ => false,
                    };
                    if !spares {
                        return false;
                    }
                    spared = true;
                }
                Some("grantProtectionChosenCardType")
                    if modifier["decisionId"]
                        .as_str()
                        .and_then(|id| stored_decision_value(&grantor.definition, id))
                        .is_some_and(|card_type| has_card_type(source, card_type)) =>
                {
                    return false;
                }
                _ => {}
            }
        }
        let temporary = self.state.rule_modifiers.iter().any(|modifier| {
            value_kind(modifier) == Some("temporaryProtection")
                && modifier["cardInstanceId"].as_str() == Some(host.instance_id.as_str())
                && protects(&modifier["from"])
        });
        let keyword = self.effective_rules(host).iter().any(|rule| {
            let abilities = match value_kind(rule) {
                Some("keywordAbility") => vec![&rule["ability"]],
                Some("keywordAbilityGroup") => {
                    rule["abilities"].as_array().into_iter().flatten().collect()
                }
                _ => Vec::new(),
            };
            abilities.into_iter().any(|ability| {
                value_kind(ability) == Some("protection") && protects(&ability["from"])
            })
        });
        spared && !temporary && !keyword
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
