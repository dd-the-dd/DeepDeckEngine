//! Static damage prevention as one filter over the damage event (CR 615.1a).
//!
//! A `preventScopedDamage` modifier of an active static ability prevents a
//! damage event when every slot it states matches the event:
//!
//! - `damage`: `any`, `combat` or `noncombat` (CR 120.2);
//! - `recipients`: `{ "objects": SELECTOR }` (a permanent dealt the damage)
//!   or `{ "player": POPULATION }` (a player dealt the damage); absent, any
//!   recipient;
//! - `sources`: a static object selector the damage source satisfies;
//!   absent, any source.
//!
//! Selectors and populations are relative to the modifier's source and are
//! evaluated when the damage would be dealt, so a change of control or of
//! attachment changes the scope. A prevention never applies to damage that
//! can't be prevented; the caller checks that before asking.
use super::*;

/// Validation of the modifier this module owns. `None` leaves any other
/// modifier to the battlefield modifier validator.
pub(super) fn modifier_supported(modifier: &Value) -> Option<bool> {
    if value_kind(modifier) != Some("preventScopedDamage") {
        return None;
    }
    let damage = matches!(
        modifier["damage"].as_str(),
        Some("any" | "combat" | "noncombat")
    );
    let recipients = match modifier.get("recipients") {
        None => true,
        Some(recipients) => match (recipients.get("objects"), recipients.get("player")) {
            (Some(objects), None) => static_object_selector_supported(objects),
            (None, Some(player)) => {
                super::player_static_modifiers::static_player_population_supported(player)
            }
            _ => false,
        },
    };
    let sources = modifier
        .get("sources")
        .is_none_or(static_object_selector_supported);
    // An unscoped filter would prevent all damage; no printed static says so
    // without a recipient or a source.
    let scoped = modifier.get("recipients").is_some() || modifier.get("sources").is_some();
    Some(damage && recipients && sources && scoped)
}

fn damage_kind_matches(modifier: &Value, combat: bool) -> bool {
    match modifier["damage"].as_str() {
        Some("combat") => combat,
        Some("noncombat") => !combat,
        _ => true,
    }
}

impl GameEngine {
    fn scoped_prevention_recipient_matches(
        &self,
        owner: &CardInstance,
        modifier: &Value,
        target: &TargetRef,
    ) -> bool {
        let Some(recipients) = modifier.get("recipients") else {
            return true;
        };
        match target {
            TargetRef::Player { player_id } => recipients.get("player").is_some_and(|population| {
                self.static_population_includes(owner, population, player_id)
            }),
            TargetRef::Permanent { instance_id } => {
                recipients.get("objects").is_some_and(|objects| {
                    self.permanent_position(instance_id).is_some_and(
                        |(player_index, card_index)| {
                            Self::static_selector_matches(
                                owner,
                                objects,
                                &self.state.players[player_index].battlefield[card_index],
                            )
                        },
                    )
                })
            }
            _ => false,
        }
    }

    /// The event identities of a damage event a static scoped prevention
    /// prevents (CR 615.1a), or `None` when no active filter matches it.
    /// `source_card` is the damage source as it exists (or last existed);
    /// a filter with a source slot never matches an unknown source.
    pub(super) fn scoped_static_prevention_event_ids(
        &self,
        target: &TargetRef,
        source: &str,
        source_card: Option<&CardInstance>,
        combat: bool,
    ) -> Option<(Option<String>, Option<String>)> {
        let prevented = self
            .battlefield_static_modifiers()
            .iter()
            .any(|(owner, modifier)| {
                value_kind(modifier) == Some("preventScopedDamage")
                    && damage_kind_matches(modifier, combat)
                    && self.scoped_prevention_recipient_matches(owner, modifier, target)
                    && modifier.get("sources").is_none_or(|sources| {
                        source_card
                            .is_some_and(|card| Self::static_selector_matches(owner, sources, card))
                    })
            });
        if !prevented {
            return None;
        }
        Some(match target {
            TargetRef::Player { player_id } => (Some(player_id.clone()), Some(source.to_string())),
            TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id } => {
                (None, Some(instance_id.clone()))
            }
            TargetRef::StackObject { .. } => (None, None),
        })
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "static_damage_prevention_tests.rs"]
mod tests;
