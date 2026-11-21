//! Static damage-event replacement and prevention programs (CR 614.1a,
//! 615.1a, 616.1).
//!
//! A `replaceDamage` modifier pairs one damage-event pattern (source,
//! recipients, combat kind and an amount guard) with one modification: a new
//! amount or a prevented amount, both arithmetic over the event's quantity.
//! Each program applies at most once to one damage event (CR 614.5). When
//! several apply, the affected player or the affected permanent's controller
//! chooses the order (CR 616.1); applicability is checked again after each
//! application (CR 616.1e). When every order yields the same amount no choice
//! is asked. Prevention programs do not apply to damage that can't be
//! prevented (CR 615.12). No Oracle text is interpreted here.
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

use super::quantity_replacement::{OptionalQuantityProvider, QuantityExpression};
use super::{
    ActionKind, CardInstance, DecisionKind, EngineDecisionRequest, EngineError, GameEngine,
    LegalAction, TargetRef, card_filter_supported, matches_card_filter, permanent_filter_supported,
    value_kind,
};

/// Bounded order analysis: beyond this many simultaneous programs an order
/// that changes the result is always asked rather than enumerated.
const MAX_ENUMERATED_PROGRAMS: usize = 5;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum DamageKind {
    Any,
    Combat,
    Noncombat,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DamagePattern {
    damage: DamageKind,
    #[serde(default)]
    source: Option<Value>,
    #[serde(default)]
    recipients: Option<Vec<Value>>,
    #[serde(default)]
    minimum: Option<i32>,
    #[serde(default)]
    maximum: Option<i32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum DamageModification {
    ModifyAmount { quantity: QuantityExpression },
    Prevent { amount: QuantityExpression },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DamageProgram {
    kind: String,
    event: DamagePattern,
    replacement: DamageModification,
}

fn player_relation_supported(reference: &Value) -> bool {
    match value_kind(reference) {
        Some("eachPlayer") => true,
        Some("controllerOf") => value_kind(&reference["object"]) == Some("self"),
        Some("opponentsOf" | "playersExcept") => player_relation_supported(&reference["player"]),
        _ => false,
    }
}

fn attached_to_source(selector: &Value) -> bool {
    value_kind(selector) == Some("attachedPermanent")
        && value_kind(&selector["attachment"]) == Some("self")
}

fn source_selector_supported(selector: &Value) -> bool {
    match value_kind(selector) {
        Some("self") => true,
        Some("attachedPermanent") => attached_to_source(selector),
        Some("objects") => {
            selector.as_object().is_some_and(|fields| {
                fields.keys().all(|key| {
                    matches!(
                        key.as_str(),
                        "kind" | "where" | "domain" | "controller" | "excludeSource"
                    )
                })
            }) && selector.get("where").is_none_or(card_filter_supported)
                && selector
                    .get("domain")
                    .is_none_or(|domain| domain.as_str() == Some("spell"))
                && selector
                    .get("controller")
                    .is_none_or(player_relation_supported)
                && selector.get("excludeSource").is_none_or(Value::is_boolean)
        }
        _ => false,
    }
}

fn recipient_selector_supported(selector: &Value) -> bool {
    match value_kind(selector) {
        Some("self") => true,
        Some("attachedPermanent") => attached_to_source(selector),
        Some("player") => player_relation_supported(&selector["player"]),
        Some("permanents") => {
            selector.as_object().is_some_and(|fields| {
                fields.keys().all(|key| {
                    matches!(
                        key.as_str(),
                        "kind" | "where" | "controller" | "excludeSource"
                    )
                })
            }) && selector.get("where").is_none_or(permanent_filter_supported)
                && selector
                    .get("controller")
                    .is_none_or(player_relation_supported)
                && selector.get("excludeSource").is_none_or(Value::is_boolean)
        }
        _ => false,
    }
}

fn parse_program(modifier: &Value) -> Option<DamageProgram> {
    let program: DamageProgram = serde_json::from_value(modifier.clone()).ok()?;
    let event = &program.event;
    (program.kind == "replaceDamage"
        && event.source.as_ref().is_none_or(source_selector_supported)
        && event.recipients.as_ref().is_none_or(|recipients| {
            !recipients.is_empty() && recipients.iter().all(recipient_selector_supported)
        })
        && event.minimum.is_none_or(|minimum| minimum >= 0)
        && event.maximum.is_none_or(|maximum| maximum >= 0))
    .then_some(program)
}

/// Engine coverage of one `replaceDamage` modifier.
pub(super) fn damage_replacement_modifier_supported(modifier: &Value) -> bool {
    parse_program(modifier).is_some()
}

/// The damage event as the replacement programs observe it.
pub(super) struct DamageEventView<'a> {
    pub target: &'a TargetRef,
    pub source_id: &'a str,
    /// The source object with its characteristics, when the engine knows it.
    pub source_card: Option<&'a CardInstance>,
    pub source_controller: Option<&'a str>,
    /// The source is a resolving spell rather than a permanent (CR 120.3).
    pub source_is_spell: bool,
    pub combat: bool,
}

/// One applied prevention, recorded as a prevention event by the caller.
pub(super) struct AppliedDamagePrevention {
    pub program_id: String,
    pub amount: i32,
}

pub(super) struct DamageReplacementOutcome {
    pub amount: i32,
    pub preventions: Vec<AppliedDamagePrevention>,
    /// Some applied program changed the amount by a modification (not a
    /// prevention); fixed-order legacy amount effects then can't be combined.
    pub modified: bool,
    pub applied: Vec<String>,
}

struct ActiveProgram {
    id: String,
    source: CardInstance,
    program: DamageProgram,
}

impl ActiveProgram {
    fn is_prevention(&self) -> bool {
        matches!(self.program.replacement, DamageModification::Prevent { .. })
    }

    /// The amount after this program, and the prevented part for a prevention.
    fn apply(&self, amount: i32) -> Result<(i32, i32), EngineError> {
        Ok(match &self.program.replacement {
            DamageModification::ModifyAmount { quantity } => (quantity.evaluate(amount)?.max(0), 0),
            DamageModification::Prevent { amount: prevented } => {
                let prevented = prevented.evaluate(amount)?.clamp(0, amount.max(0));
                (amount - prevented, prevented)
            }
        })
    }
}

impl GameEngine {
    fn damage_player_relation_matches(
        &self,
        reference: &Value,
        source: &CardInstance,
        player_id: &str,
    ) -> bool {
        match value_kind(reference) {
            Some("eachPlayer") => true,
            Some("controllerOf") => source.controller == player_id,
            Some("opponentsOf" | "playersExcept") => {
                !self.damage_player_relation_matches(&reference["player"], source, player_id)
            }
            _ => false,
        }
    }

    fn damage_source_matches(
        &self,
        selector: &Value,
        modifier_source: &CardInstance,
        event: &DamageEventView<'_>,
    ) -> bool {
        match value_kind(selector) {
            Some("self") => event.source_id == modifier_source.instance_id,
            Some("attachedPermanent") => {
                modifier_source.attached_to.as_deref() == Some(event.source_id)
            }
            Some("objects") => {
                if selector["excludeSource"].as_bool().unwrap_or(false)
                    && event.source_id == modifier_source.instance_id
                {
                    return false;
                }
                if selector["domain"].as_str() == Some("spell") && !event.source_is_spell {
                    return false;
                }
                if let Some(relation) = selector.get("controller")
                    && !event.source_controller.is_some_and(|controller| {
                        self.damage_player_relation_matches(relation, modifier_source, controller)
                    })
                {
                    return false;
                }
                selector.get("where").is_none_or(|filter| {
                    event.source_card.is_some_and(|card| {
                        if !event.source_is_spell
                            && self.permanent_position(&card.instance_id).is_some()
                        {
                            self.permanent_matches_effective_filter(card, filter)
                        } else {
                            matches_card_filter(&card.definition, filter)
                        }
                    })
                })
            }
            _ => false,
        }
    }

    fn damage_recipient_matches(
        &self,
        selector: &Value,
        modifier_source: &CardInstance,
        target: &TargetRef,
    ) -> bool {
        match (value_kind(selector), target) {
            (Some("player"), TargetRef::Player { player_id }) => {
                self.damage_player_relation_matches(&selector["player"], modifier_source, player_id)
            }
            (Some("self"), TargetRef::Permanent { instance_id }) => {
                *instance_id == modifier_source.instance_id
            }
            (Some("attachedPermanent"), TargetRef::Permanent { instance_id }) => {
                modifier_source.attached_to.as_deref() == Some(instance_id.as_str())
            }
            (Some("permanents"), TargetRef::Permanent { instance_id }) => {
                let Some((player_index, card_index)) = self.permanent_position(instance_id) else {
                    return false;
                };
                let permanent = &self.state.players[player_index].battlefield[card_index];
                !(selector["excludeSource"].as_bool().unwrap_or(false)
                    && permanent.instance_id == modifier_source.instance_id)
                    && selector.get("controller").is_none_or(|relation| {
                        self.damage_player_relation_matches(
                            relation,
                            modifier_source,
                            &permanent.controller,
                        )
                    })
                    && selector.get("where").is_none_or(|filter| {
                        self.permanent_matches_effective_filter(permanent, filter)
                    })
            }
            _ => false,
        }
    }

    fn damage_program_applies(
        &self,
        active: &ActiveProgram,
        event: &DamageEventView<'_>,
        amount: i32,
        can_prevent: bool,
    ) -> bool {
        let pattern = &active.program.event;
        amount > 0
            && (can_prevent || !active.is_prevention())
            && match pattern.damage {
                DamageKind::Any => true,
                DamageKind::Combat => event.combat,
                DamageKind::Noncombat => !event.combat,
            }
            && pattern.minimum.is_none_or(|minimum| amount >= minimum)
            && pattern.maximum.is_none_or(|maximum| amount <= maximum)
            && pattern
                .source
                .as_ref()
                .is_none_or(|selector| self.damage_source_matches(selector, &active.source, event))
            && match &pattern.recipients {
                None => matches!(
                    event.target,
                    TargetRef::Player { .. } | TargetRef::Permanent { .. }
                ),
                Some(recipients) => recipients.iter().any(|selector| {
                    self.damage_recipient_matches(selector, &active.source, event.target)
                }),
            }
    }

    fn active_damage_programs(&self) -> Vec<ActiveProgram> {
        self.battlefield_static_modifiers()
            .into_iter()
            .enumerate()
            .filter_map(|(ordinal, (source, modifier))| {
                if value_kind(&modifier) != Some("replaceDamage") {
                    return None;
                }
                let program = parse_program(&modifier)?;
                Some(ActiveProgram {
                    id: format!("{}:damage:{ordinal}", source.instance_id),
                    source,
                    program,
                })
            })
            .collect()
    }

    /// Every final amount reachable by applying the applicable programs in
    /// some order, re-checking applicability after each step.
    fn reachable_damage_amounts(
        &self,
        programs: &[ActiveProgram],
        used: &mut Vec<bool>,
        event: &DamageEventView<'_>,
        amount: i32,
        can_prevent: bool,
        results: &mut Vec<i32>,
    ) -> Result<(), EngineError> {
        let mut any = false;
        for index in 0..programs.len() {
            if used[index]
                || !self.damage_program_applies(&programs[index], event, amount, can_prevent)
            {
                continue;
            }
            any = true;
            let (next, _) = programs[index].apply(amount)?;
            used[index] = true;
            self.reachable_damage_amounts(programs, used, event, next, can_prevent, results)?;
            used[index] = false;
        }
        if !any && !results.contains(&amount) {
            results.push(amount);
        }
        Ok(())
    }

    /// Apply the static damage replacements and preventions to one event.
    pub(super) fn apply_damage_replacements(
        &self,
        event: &DamageEventView<'_>,
        amount: i32,
        can_prevent: bool,
        provider: &mut OptionalQuantityProvider<'_>,
    ) -> Result<DamageReplacementOutcome, EngineError> {
        let programs = self.active_damage_programs();
        let mut outcome = DamageReplacementOutcome {
            amount,
            preventions: Vec::new(),
            modified: false,
            applied: Vec::new(),
        };
        if programs.is_empty() || amount <= 0 {
            return Ok(outcome);
        }
        let mut used = vec![false; programs.len()];
        loop {
            let applicable = (0..programs.len())
                .filter(|index| {
                    !used[*index]
                        && self.damage_program_applies(
                            &programs[*index],
                            event,
                            outcome.amount,
                            can_prevent,
                        )
                })
                .collect::<Vec<_>>();
            let Some(&first) = applicable.first() else {
                break;
            };
            let index = if applicable.len() == 1
                || (applicable.len() <= MAX_ENUMERATED_PROGRAMS && {
                    let mut results = Vec::new();
                    let mut trial = used.clone();
                    self.reachable_damage_amounts(
                        &programs,
                        &mut trial,
                        event,
                        outcome.amount,
                        can_prevent,
                        &mut results,
                    )?;
                    results.len() == 1
                }) {
                first
            } else {
                self.choose_damage_replacement(&programs, &applicable, event, &outcome, provider)?
            };
            let (next, prevented) = programs[index].apply(outcome.amount)?;
            if programs[index].is_prevention() {
                outcome.preventions.push(AppliedDamagePrevention {
                    program_id: programs[index].id.clone(),
                    amount: prevented,
                });
            } else {
                outcome.modified |= next != outcome.amount;
            }
            outcome.amount = next;
            used[index] = true;
            outcome.applied.push(programs[index].id.clone());
        }
        Ok(outcome)
    }

    /// CR 616.1: the affected player, or the affected permanent's controller,
    /// chooses which applicable replacement or prevention to apply next.
    fn choose_damage_replacement(
        &self,
        programs: &[ActiveProgram],
        applicable: &[usize],
        event: &DamageEventView<'_>,
        outcome: &DamageReplacementOutcome,
        provider: &mut OptionalQuantityProvider<'_>,
    ) -> Result<usize, EngineError> {
        if provider.0.is_none() {
            return Err(EngineError::new(
                "damage replacements whose order changes the result require the affected player's choice",
            ));
        }
        let player = match event.target {
            TargetRef::Player { player_id } => player_id.clone(),
            TargetRef::Permanent { instance_id } => self
                .permanent_position(instance_id)
                .map(|(player_index, card_index)| {
                    self.state.players[player_index].battlefield[card_index]
                        .controller
                        .clone()
                })
                .ok_or_else(|| EngineError::new("damaged permanent has no controller"))?,
            _ => {
                return Err(EngineError::new(
                    "damage replacement recipient is not damageable",
                ));
            }
        };
        let request = EngineDecisionRequest {
            id: format!(
                "damage-replacement:{}:{}:{}",
                self.state.turn_number,
                self.state.events.len(),
                outcome.applied.len()
            ),
            kind: DecisionKind::ReplacementChoice,
            player_id: player.clone(),
            source_card: None,
            source_card_instance_id: None,
            choice: None,
            options: applicable
                .iter()
                .map(|index| {
                    let program = &programs[*index];
                    LegalAction {
                        id: program.id.clone(),
                        kind: ActionKind::ChooseResolution,
                        player_id: player.clone(),
                        label: format!("Apply {}", program.source.definition.name),
                        card_instance_id: Some(program.source.instance_id.clone()),
                        payment_sources: Vec::new(),
                        decisions: BTreeMap::from([(
                            "damageReplacementId".to_string(),
                            json!(program.id),
                        )]),
                        targets: BTreeMap::new(),
                        target_order: Vec::new(),
                        attacker_id: None,
                        blocker_id: None,
                    }
                })
                .collect(),
        };
        let choice = self.choose_action(provider, request)?;
        applicable
            .iter()
            .copied()
            .find(|index| programs[*index].id == choice.id)
            .ok_or_else(|| EngineError::new("damage replacement choice has no matching program"))
    }
}
