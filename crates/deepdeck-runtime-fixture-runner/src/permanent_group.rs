use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use deepdeck_engine_runtime::engine::{GameEngine, GameState};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpectedPowerToughness {
    pub power: i32,
    pub toughness: i32,
}

/// An exact nonempty group, with every property checked on every member.
/// This does not weaken the existing intentionally singular assertions.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermanentGroupExpectation {
    pub player_id: String,
    pub definition_id: String,
    pub count: NonZeroUsize,
    #[serde(default)]
    pub controller: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub power_toughness: Option<ExpectedPowerToughness>,
    #[serde(default)]
    pub counters: Option<BTreeMap<String, i32>>,
}

pub(super) fn evaluate_permanent_group(
    state: &GameState,
    expected: &PermanentGroupExpectation,
) -> (bool, String) {
    let group = state
        .players
        .iter()
        .find(|player| player.id == expected.player_id)
        .into_iter()
        .flat_map(|player| player.battlefield.iter())
        .filter(|permanent| permanent.definition.id == expected.definition_id)
        .collect::<Vec<_>>();
    let heading = format!(
        "{} permanentGroup {} count expected {}, actual {}",
        expected.player_id,
        expected.definition_id,
        expected.count,
        group.len(),
    );
    if group.len() != expected.count.get() {
        return (false, heading);
    }
    let stats = expected
        .power_toughness
        .as_ref()
        .map(|_| GameEngine::effective_power_toughness(state));
    let mut failures = Vec::new();
    for permanent in group {
        let id = &permanent.instance_id;
        if let Some(controller) = &expected.controller {
            if permanent.controller != *controller {
                failures.push(format!(
                    "{id} controller expected {controller}, actual {}",
                    permanent.controller
                ));
            }
        }
        if let Some(owner) = &expected.owner {
            if permanent.owner != *owner {
                failures.push(format!(
                    "{id} owner expected {owner}, actual {}",
                    permanent.owner
                ));
            }
        }
        if let Some(power_toughness) = &expected.power_toughness {
            let actual = stats
                .as_ref()
                .and_then(|stats| stats.get(id))
                .map(|stats| (stats.current_power, stats.current_toughness));
            if actual != Some((power_toughness.power, power_toughness.toughness)) {
                failures.push(format!(
                    "{id} effective power/toughness expected {}/{}, actual {actual:?}",
                    power_toughness.power, power_toughness.toughness
                ));
            }
        }
        if let Some(counters) = &expected.counters {
            for (counter, count) in counters {
                let actual = permanent.counters.get(counter).copied().unwrap_or(0);
                if actual != *count {
                    failures.push(format!(
                        "{id} counter {counter} expected {count}, actual {actual}"
                    ));
                }
            }
        }
    }
    if failures.is_empty() {
        (
            true,
            format!("{heading}; every member satisfies its expected properties"),
        )
    } else {
        (false, format!("{heading}; {}", failures.join("; ")))
    }
}

#[cfg(test)]
#[path = "permanent_group_tests.rs"]
mod tests;
