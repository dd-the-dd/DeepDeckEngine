//! Direct life-total primitives.
//!
//! Composite drains, optional payments, and multi-player aggregate operations
//! remain in the root dispatcher until they can be decomposed independently.

use serde_json::{Value, json};
use std::collections::BTreeMap;

use super::{
    EngineError, GameEngine, RuntimeBinding, StackObject, integer_value,
    numeric_expression_supported, player_reference_supported, triggered_event_variants,
    triggered_rule_supported, value_kind,
};

pub(super) fn node_requires_checkpoint(value: &Value) -> bool {
    value_kind(value) == Some("loseLife")
        || (value_kind(value) == Some("gainLife") && integer_value(&value["amount"]).is_none())
}

/// Validates direct `gainLife` and `loseLife` IR nodes.
///
/// `None` means this module does not own the effect kind.
pub(super) fn direct_life_effect_supported(effect: &Value) -> Option<bool> {
    match value_kind(effect) {
        Some("gainLife" | "loseLife") => Some(
            numeric_expression_supported(&effect["amount"])
                && player_reference_supported(&effect["player"]),
        ),
        _ => None,
    }
}

#[derive(Clone, Copy)]
pub(super) enum LifeLossEventStyle {
    Direct,
    Legacy,
}

struct PreparedLifeLoss {
    player_index: usize,
    player_id: String,
    amount: i32,
    total_after: i32,
    prevented: bool,
}

impl GameEngine {
    /// Executes direct life-total changes and reports whether the effect kind
    /// belongs to this module.
    pub(super) fn execute_direct_life_effect(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<bool, EngineError> {
        let Some(kind @ ("gainLife" | "loseLife")) = value_kind(effect) else {
            return Ok(false);
        };
        if self.numeric_amount_requires_initially_illegal_target(
            &effect["amount"],
            stack_object,
            bindings,
            decisions,
        )? {
            return Ok(true);
        }
        let amount = self
            .checked_runtime_numeric_expression(
                &effect["amount"],
                stack_object,
                bindings,
                decisions,
                "life.amount",
            )
            .map_err(|error| {
                EngineError::new(format!(
                    "unresolved life amount for stack {}: {error}",
                    stack_object.id
                ))
            })?
            .max(0);
        if amount == 0 {
            return Ok(true);
        }
        if kind == "loseLife" {
            // Random player selectors need a separate choice/RNG transaction.
            // Refuse before consuming RNG rather than approximating a set.
            if matches!(
                value_kind(&effect["player"]),
                Some("randomPlayer" | "randomOpponentOf")
            ) {
                return Err(EngineError::new(
                    "life-loss batch does not support random recipient selection",
                ));
            }
            let recipients =
                self.player_iteration_ids(&effect["player"], stack_object, bindings)?;
            self.execute_life_loss_batch(
                recipients,
                amount,
                &stack_object.card.instance_id,
                Some(&stack_object.controller),
                LifeLossEventStyle::Direct,
            )?;
        } else {
            for player_id in self.resolve_players(&effect["player"], stack_object, bindings) {
                self.gain_life(&player_id, amount)?;
            }
        }
        Ok(true)
    }
}

impl GameEngine {
    fn prepare_life_loss_batch(
        &self,
        recipients: Vec<String>,
        amount: i32,
    ) -> Result<Vec<PreparedLifeLoss>, EngineError> {
        if amount < 0 {
            return Err(EngineError::new("life-loss amount cannot be negative"));
        }
        if amount == 0 {
            return Ok(Vec::new());
        }
        // lifeLost currently has a per-recipient event contract. Do not
        // silently turn an unproved one-or-more observer into several triggers.
        if self
            .state
            .players
            .iter()
            .flat_map(|p| p.battlefield.iter())
            .flat_map(|c| c.definition.rules.iter())
            .any(|rule| {
                triggered_event_variants(&rule["event"])
                    .iter()
                    .any(|event| {
                        value_kind(event) == Some("lifeLost") && event.get("aggregation").is_some()
                    })
            })
        {
            return Err(EngineError::new(
                "life-loss observer aggregation requires a shared batch event contract",
            ));
        }
        let modifiers = self.battlefield_static_modifiers();
        let mut prepared = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for player_id in recipients {
            if !seen.insert(player_id.clone()) {
                continue;
            }
            let player_index = self.player_index(&player_id)?;
            let player = &self.state.players[player_index];
            if player.has_lost {
                continue;
            }
            let prevented = self.state.rule_modifiers.iter().any(|modifier| {
                value_kind(modifier) == Some("lifeTotalCannotChange")
                    && modifier["playerId"].as_str() == Some(player_id.as_str())
            });
            if prevented {
                prepared.push(PreparedLifeLoss {
                    player_index,
                    player_id,
                    amount,
                    total_after: player.life,
                    prevented: true,
                });
                continue;
            }
            let mut modified_amount = amount;
            for (source, modifier) in &modifiers {
                if source.attached_to.as_deref() == Some(player_id.as_str())
                    && value_kind(modifier) == Some("multiplyAttachedPlayerLifeLoss")
                {
                    let factor = integer_value(&modifier["factor"])
                        .filter(|v| *v >= 1)
                        .ok_or_else(|| {
                            EngineError::new(
                                "life-loss batch requires a positive integer replacement factor",
                            )
                        })?;
                    modified_amount = modified_amount
                        .checked_mul(factor)
                        .ok_or_else(|| EngineError::new("life-loss replacement amount overflow"))?;
                }
            }
            let total_after = player
                .life
                .checked_sub(modified_amount)
                .ok_or_else(|| EngineError::new("life-loss total overflow"))?;
            prepared.push(PreparedLifeLoss {
                player_index,
                player_id,
                amount: modified_amount,
                total_after,
                prevented: false,
            });
        }
        Ok(prepared)
    }

    pub(super) fn execute_life_loss_batch(
        &mut self,
        recipients: Vec<String>,
        amount: i32,
        source: &str,
        ability_controller: Option<&str>,
        event_style: LifeLossEventStyle,
    ) -> Result<(), EngineError> {
        let prepared = self.prepare_life_loss_batch(recipients, amount)?;
        let source_controller = ability_controller.map(str::to_owned).or_else(|| {
            self.state
                .players
                .iter()
                .flat_map(|p| p.battlefield.iter().chain(p.command_zone.iter()))
                .find(|card| card.instance_id == source)
                .map(|card| card.controller.clone())
                .or_else(|| {
                    self.state
                        .stack
                        .iter()
                        .find(|object| object.card.instance_id == source)
                        .map(|object| object.controller.clone())
                })
        });
        // All totals are committed before any event or observer callback.
        for loss in &prepared {
            if !loss.prevented {
                self.state.players[loss.player_index].life = loss.total_after;
            }
        }
        // Publish every loss/prevention record before any triggered condition.
        for loss in &prepared {
            if loss.prevented {
                self.record_event(
                    "lifeChangePrevented",
                    Some(loss.player_id.clone()),
                    Some(source.to_string()),
                    json!({"amount":-loss.amount}),
                );
            } else {
                let (source_id, detail) = match event_style {
                    LifeLossEventStyle::Direct => {
                        (Some(source.to_string()), json!({"amount":loss.amount}))
                    }
                    LifeLossEventStyle::Legacy => {
                        (None, json!({"amount":loss.amount,"source":source}))
                    }
                };
                self.record_event("lifeLost", Some(loss.player_id.clone()), source_id, detail);
            }
        }
        for loss in &prepared {
            if !loss.prevented {
                self.publish_life_loss_consequences(
                    &loss.player_id,
                    loss.amount,
                    source,
                    source_controller.as_deref(),
                );
            }
        }
        // State-based actions belong to the enclosing resolution boundary.
        self.check_quantity_failure()
    }

    fn publish_life_loss_consequences(
        &mut self,
        player_id: &str,
        amount: i32,
        source: &str,
        source_controller: Option<&str>,
    ) {
        if amount == 1 {
            if source_controller == Some(player_id) {
                let triggers = self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| player.battlefield.iter())
                    .filter(|trigger_source| trigger_source.controller == player_id)
                    .flat_map(|trigger_source| {
                        trigger_source
                            .definition
                            .rules
                            .iter()
                            .filter(|rule| {
                                triggered_rule_supported(rule)
                                    && triggered_event_variants(&rule["event"])
                                        .iter()
                                        .any(|event| {
                                            value_kind(event)
                                                == Some("controlledEffectCausedExactLifeLossToController")
                                                && event["amount"].as_i64() == Some(1)
                                        })
                            })
                            .cloned()
                            .map(|rule| (trigger_source.clone(), rule))
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                for (trigger_source, rule) in triggers {
                    self.put_triggered_ability_on_stack_with_decisions(
                        &trigger_source,
                        rule,
                        BTreeMap::from([("lifeLostAmount".to_string(), Value::from(amount))]),
                    );
                    if self.check_quantity_failure().is_err() {
                        return;
                    }
                }
            }
        }
        if amount > 0 {
            let active_player_id = self.state.players[self.state.active_player].id.clone();
            let already_increased = self.state.events.iter().any(|event| {
                event.turn_number == self.state.turn_number
                    && event.kind == "speedIncreased"
                    && event.player_id.as_deref() == Some(active_player_id.as_str())
            });
            if player_id != active_player_id && !already_increased {
                let active_index = self.state.active_player;
                let speed = self.state.players[active_index]
                    .counters
                    .get("speed")
                    .copied()
                    .unwrap_or(0);
                if (1..4).contains(&speed) {
                    self.state.players[active_index]
                        .counters
                        .insert("speed".to_string(), speed + 1);
                    self.record_event(
                        "speedIncreased",
                        Some(active_player_id),
                        Some(source.to_string()),
                        json!({ "speed": speed + 1 }),
                    );
                }
            }
            self.enqueue_life_lost_triggers(player_id, amount);
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "life_tests.rs"]
mod tests;

#[derive(Clone)]
pub(super) struct PreparedLifeGain {
    pub player_id: String,
    pub requested: i32,
    pub amount: i32,
    pub prevented: bool,
    pub life_before: i32,
    pub life_after: i32,
}
impl GameEngine {
    pub(super) fn prepare_life_gain(
        &self,
        player_id: &str,
        amount: i32,
    ) -> Result<Option<PreparedLifeGain>, EngineError> {
        let index = self.player_index(player_id)?;
        self.prepare_life_gain_at_life(player_id, amount, self.state.players[index].life)
    }
    pub(super) fn prepare_life_gain_at_life(
        &self,
        player_id: &str,
        amount: i32,
        before: i32,
    ) -> Result<Option<PreparedLifeGain>, EngineError> {
        self.player_index(player_id)?;
        let prevented = self.state.rule_modifiers.iter().any(|modifier| {
            value_kind(modifier) == Some("lifeTotalCannotChange")
                && modifier["playerId"].as_str() == Some(player_id)
        }) || self.battlefield_static_modifiers().iter().any(
            |(source, modifier)| {
                value_kind(modifier) == Some("preventOpponentsGainLife")
                    && source.controller != player_id
                    || value_kind(modifier) == Some("preventPlayersGainLife")
            },
        );
        let multiplier = if prevented {
            1
        } else {
            self.battlefield_static_modifiers()
                .iter()
                .filter(|(source, modifier)| {
                    source.controller == player_id
                        && value_kind(modifier) == Some("multiplyLifeGain")
                })
                .try_fold(1_i32, |factor, (_, modifier)| {
                    let next =
                        super::integer_value(&modifier["factor"]).filter(|next| *next >= 1)?;
                    factor.checked_mul(next)
                })
                .ok_or_else(|| {
                    EngineError::new("life-gain multiplier product overflow or invalid factor")
                })?
        };
        let actual = if prevented {
            0
        } else {
            // CR 614.1a: static life-gain replacements. The fixed-order legacy
            // multiplier cannot be interleaved with them (CR 616.1).
            let (replaced, applied) = self.apply_life_gain_replacements(player_id, amount)?;
            if applied && multiplier != 1 {
                return Err(EngineError::new(
                    "life gain has competing replacements requiring the affected player's choice",
                ));
            }
            replaced
                .checked_mul(multiplier)
                .ok_or_else(|| EngineError::new("modified life gain overflow"))?
        };
        let after = before
            .checked_add(actual)
            .ok_or_else(|| EngineError::new("life gain exceeds its integer bound"))?;
        Ok(Some(PreparedLifeGain {
            player_id: player_id.into(),
            requested: amount,
            amount: actual,
            prevented,
            life_before: before,
            life_after: after,
        }))
    }
    pub(super) fn commit_prepared_life_gain(
        &mut self,
        plan: &PreparedLifeGain,
    ) -> Result<(), EngineError> {
        let index = self.player_index(&plan.player_id)?;
        if self.state.players[index].life != plan.life_before {
            return Err(EngineError::new("prepared life-gain context changed"));
        }
        self.state.players[index].life = plan.life_after;
        Ok(())
    }
    pub(super) fn publish_prepared_life_gain(&mut self, plan: &PreparedLifeGain) {
        self.record_prepared_life_gain(plan);
        self.notify_prepared_life_gain(plan);
    }
    pub(super) fn record_prepared_life_gain(&mut self, plan: &PreparedLifeGain) {
        if plan.prevented {
            self.record_event(
                "lifeChangePrevented",
                Some(plan.player_id.clone()),
                None,
                json!({"amount":plan.requested}),
            );
        } else {
            self.record_event(
                "lifeGained",
                Some(plan.player_id.clone()),
                None,
                json!({"amount":plan.amount}),
            );
        }
    }
    pub(super) fn notify_prepared_life_gain(&mut self, plan: &PreparedLifeGain) {
        if !plan.prevented {
            self.enqueue_life_gained_triggers(&plan.player_id, plan.amount);
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "life_batch_tests.rs"]
mod batch_tests;
