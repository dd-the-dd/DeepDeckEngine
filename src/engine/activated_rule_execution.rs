use super::*;

/// Parser-independent target identities for deterministic activated-ability
/// proofs. Definition-based selections are resolved to exactly one current
/// game object before matching an engine-generated legal declaration.
#[derive(Clone, Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CanonicalTargetSelection {
    Player { player_id: String },
    PermanentDefinition { definition_id: String },
    CardDefinition { definition_id: String },
    StackObject { stack_id: String },
    StackObjectDefinition { definition_id: String },
}

/// A deterministic request to activate one real non-mana ability through the
/// engine's legal-action generator. Costs, target legality, and stack behavior
/// remain owned by the normal priority-action path.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanonicalActivatedAbilityActivation {
    pub controller_id: String,
    pub source_definition_id: String,
    #[serde(default)]
    pub ability_rule_index: Option<usize>,
    #[serde(default)]
    pub targets: BTreeMap<String, CanonicalTargetSelection>,
}

impl GameEngine {
    pub(super) fn resolve_canonical_target(
        &self,
        selection: &CanonicalTargetSelection,
    ) -> Result<TargetRef, EngineError> {
        match selection {
            CanonicalTargetSelection::Player { player_id } => {
                self.player_index(player_id)?;
                Ok(TargetRef::Player {
                    player_id: player_id.clone(),
                })
            }
            CanonicalTargetSelection::PermanentDefinition { definition_id } => {
                let matches = self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| player.battlefield.iter())
                    .filter(|card| card.definition.id == *definition_id)
                    .collect::<Vec<_>>();
                let [permanent] = matches.as_slice() else {
                    return Err(EngineError::new(format!(
                        "target permanent definition {definition_id} must identify exactly one battlefield object; found {}",
                        matches.len()
                    )));
                };
                Ok(TargetRef::Permanent {
                    instance_id: permanent.instance_id.clone(),
                })
            }
            CanonicalTargetSelection::CardDefinition { definition_id } => {
                let matches = self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| {
                        player
                            .library
                            .iter()
                            .chain(player.hand.iter())
                            .chain(player.graveyard.iter())
                            .chain(player.exile.iter())
                            .chain(player.command_zone.iter())
                    })
                    .filter(|card| card.definition.id == *definition_id)
                    .collect::<Vec<_>>();
                let [card] = matches.as_slice() else {
                    return Err(EngineError::new(format!(
                        "target card definition {definition_id} must identify exactly one nonbattlefield object; found {}",
                        matches.len()
                    )));
                };
                Ok(TargetRef::Card {
                    instance_id: card.instance_id.clone(),
                })
            }
            CanonicalTargetSelection::StackObjectDefinition { definition_id } => {
                let matches = self
                    .state
                    .stack
                    .iter()
                    .filter(|object| object.card.definition.id == *definition_id)
                    .collect::<Vec<_>>();
                let [object] = matches.as_slice() else {
                    return Err(EngineError::new(format!(
                        "target stack definition {definition_id} must identify exactly one stack object; found {}",
                        matches.len()
                    )));
                };
                self.resolve_canonical_target(&CanonicalTargetSelection::StackObject {
                    stack_id: object.id.clone(),
                })
            }
            CanonicalTargetSelection::StackObject { stack_id } => {
                let count = self
                    .state
                    .stack
                    .iter()
                    .filter(|object| object.id == *stack_id)
                    .count();
                if count != 1 {
                    return Err(EngineError::new(format!(
                        "target stack object {stack_id} must identify exactly one stack object; found {count}"
                    )));
                }
                Ok(TargetRef::StackObject {
                    stack_id: stack_id.clone(),
                })
            }
        }
    }

    fn canonical_activated_ability_action(
        &self,
        activation: &CanonicalActivatedAbilityActivation,
        declaration_decisions: &BTreeMap<String, Value>,
    ) -> Result<Option<LegalAction>, EngineError> {
        let player_index = self.player_index(&activation.controller_id)?;
        let player = &self.state.players[player_index];
        let sources = player
            .battlefield
            .iter()
            .chain(player.hand.iter())
            .chain(player.graveyard.iter())
            .filter(|card| card.definition.id == activation.source_definition_id)
            .collect::<Vec<_>>();
        let [source] = sources.as_slice() else {
            return match sources.len() {
                0 => Err(EngineError::new(format!(
                    "activated source definition {} is absent from {}'s supported activation zones",
                    activation.source_definition_id, activation.controller_id
                ))),
                count => Err(EngineError::new(format!(
                    "activated source definition {} is ambiguous across {}'s supported activation zones ({count} matches)",
                    activation.source_definition_id, activation.controller_id
                ))),
            };
        };
        let expected_targets = activation
            .targets
            .iter()
            .map(|(id, selection)| {
                self.resolve_canonical_target(selection)
                    .map(|target| (id.clone(), target))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        let mut actions = self
            .try_legal_priority_actions(player_index)?
            .into_iter()
            .filter(|action| {
                action.kind == ActionKind::ActivateAbility
                    && action.card_instance_id.as_deref() == Some(source.instance_id.as_str())
                    && action.decisions.get("manaAbility").and_then(Value::as_bool) != Some(true)
                    && action
                        .decisions
                        .get("abilityRuleIndex")
                        .and_then(Value::as_u64)
                        .is_some_and(|rule_index| {
                            activation
                                .ability_rule_index
                                .is_none_or(|expected| rule_index == expected as u64)
                        })
                    && action.targets == expected_targets
                    && declaration_decisions
                        .iter()
                        .all(|(id, value)| action.decisions.get(id) == Some(value))
            })
            .collect::<Vec<_>>();
        if actions.len() > 1 {
            return Err(EngineError::new(format!(
                "activated source definition {} has {} matching legal declarations; provide abilityRuleIndex, exact targets, and declaration decisions to disambiguate them",
                activation.source_definition_id,
                actions.len()
            )));
        }
        Ok(actions.pop())
    }

    pub fn canonical_activated_ability_available(
        &self,
        activation: &CanonicalActivatedAbilityActivation,
    ) -> Result<bool, EngineError> {
        self.canonical_activated_ability_available_with_decisions(activation, &BTreeMap::new())
    }

    pub fn canonical_activated_ability_available_with_decisions(
        &self,
        activation: &CanonicalActivatedAbilityActivation,
        declaration_decisions: &BTreeMap<String, Value>,
    ) -> Result<bool, EngineError> {
        self.canonical_activated_ability_action(activation, declaration_decisions)
            .map(|action| action.is_some())
    }

    pub fn activate_canonical_ability<P: DecisionProvider>(
        &mut self,
        activation: &CanonicalActivatedAbilityActivation,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        self.activate_canonical_ability_with_decisions(activation, &BTreeMap::new(), provider)
    }

    /// Selects one engine-generated legal declaration using explicit numeric
    /// or option choices. Unspecified ambiguity remains an error.
    pub fn activate_canonical_ability_with_decisions<P: DecisionProvider>(
        &mut self,
        activation: &CanonicalActivatedAbilityActivation,
        declaration_decisions: &BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let action = self
            .canonical_activated_ability_action(activation, declaration_decisions)?
            .ok_or_else(|| {
                EngineError::new("canonical activated ability is not currently legal")
            })?;
        self.apply_checked_priority_action(&action, provider)
    }
}

// ---------------------------------------------------------------------------
// Activation and cast restrictions (CR 601.3, 602.5): the one restriction
// reader of the engine. Parsed restrictions are rule metadata (an
// `activationCondition`, an `activationLimit`, `anyPlayerMayActivate`, an
// `activationZone`, or a spell's `castOnlyIf` static ability); none of them
// is ever an effect.
// ---------------------------------------------------------------------------

/// Engine identifiers of turn steps in turn order (`GameStep`), plus the end
/// of combat step (CR 511), which the turn structure does not model yet.
const WINDOW_STEPS: [(&str, Option<GameStep>); 12] = [
    ("untap", Some(GameStep::Untap)),
    ("upkeep", Some(GameStep::Upkeep)),
    ("draw", Some(GameStep::Draw)),
    ("precombatMain", Some(GameStep::PrecombatMain)),
    ("beginningOfCombat", Some(GameStep::BeginningOfCombat)),
    ("declareAttackers", Some(GameStep::DeclareAttackers)),
    ("declareBlockers", Some(GameStep::DeclareBlockers)),
    ("combatDamage", Some(GameStep::CombatDamage)),
    ("endOfCombat", None),
    ("postcombatMain", Some(GameStep::PostcombatMain)),
    ("endStep", Some(GameStep::EndStep)),
    ("cleanup", Some(GameStep::Cleanup)),
];

/// `duringSteps` is executable when every named step is known and at least
/// one of them exists in the engine's turn structure: a window of only the
/// end of combat step would never be offered, which is not its meaning.
pub(super) fn during_steps_supported(condition: &Value) -> bool {
    condition["steps"].as_array().is_some_and(|steps| {
        !steps.is_empty()
            && steps.iter().all(|step| {
                step.as_str()
                    .is_some_and(|step| WINDOW_STEPS.iter().any(|(name, _)| *name == step))
            })
            && steps.iter().any(|step| {
                WINDOW_STEPS
                    .iter()
                    .any(|(name, modeled)| step.as_str() == Some(*name) && modeled.is_some())
            })
    })
}

/// Whether the current step is one the window admits.
pub(super) fn during_steps_holds(current: &GameStep, condition: &Value) -> bool {
    condition["steps"].as_array().is_some_and(|steps| {
        steps.iter().any(|step| {
            WINDOW_STEPS.iter().any(|(name, modeled)| {
                step.as_str() == Some(*name) && modeled.as_ref() == Some(current)
            })
        })
    })
}

/// The per-object or per-turn use counter of a counted limit, keyed by the
/// ability so two restricted abilities of one object never share it.
pub(super) fn activation_limit_use_prefix(
    limit: &Value,
    turn_number: u32,
    rule_index: usize,
) -> Option<String> {
    let id = limit["id"].as_str()?;
    match value_kind(limit) {
        Some("oncePerGameObjectAbility") => Some(format!("{id}:ability:{rule_index}:use:")),
        Some("timesPerTurn") => Some(format!("{id}:ability:{rule_index}:turn:{turn_number}:use:")),
        _ => None,
    }
}

pub(super) fn activation_limit_use_count(
    permanent: &CardInstance,
    limit: &Value,
    turn_number: u32,
    rule_index: usize,
) -> usize {
    activation_limit_use_prefix(limit, turn_number, rule_index).map_or(0, |prefix| {
        permanent
            .flags
            .iter()
            .filter(|(flag, used)| **used && flag.starts_with(&prefix))
            .count()
    })
}

/// The single-use flag of an uncounted limit. `oncePerTurn` is keyed by its
/// id, so abilities sharing an id (loyalty abilities, CR 606.3) share it.
pub(super) fn activation_limit_flag(
    limit: &Value,
    turn_number: u32,
    _rule_index: usize,
) -> Option<String> {
    let id = limit["id"].as_str()?;
    Some(match value_kind(limit) {
        Some("oncePerTurn") => format!("{id}:turn:{turn_number}"),
        Some("oncePerGameObject") => id.to_string(),
        _ => return None,
    })
}

/// Executable activation limits of a non-mana activated ability. Abilities
/// activated from a hand or graveyard keep no per-object use record, so a
/// limit there stays explicitly unexecutable instead of being ignored.
pub(super) fn activated_rule_limit_supported(rule: &Value) -> bool {
    let Some(limit) = rule.get("activationLimit") else {
        return true;
    };
    let off_battlefield = rule
        .get("activationZone")
        .and_then(Value::as_str)
        .is_some_and(|zone| zone != "battlefield");
    (!off_battlefield || zone_of_function::hand_activation_limit_supported(rule))
        && limit["id"].is_string()
        && match value_kind(limit) {
            Some("oncePerGameObject" | "oncePerGameObjectAbility" | "oncePerTurn") => true,
            Some("timesPerTurn") => limit["maximum"]
                .as_u64()
                .is_some_and(|maximum| maximum >= 1),
            _ => false,
        }
}

/// A spell's `Cast this spell only ...` restriction (CR 601.3): a static
/// ability of the card being cast whose only modifier is its condition.
pub(super) fn cast_restriction_rule_supported(rule: &Value) -> bool {
    zone_of_function::static_zone_of_function(rule) == Some(ZoneOfFunction::StackOrCast)
        && rule["modifiers"].as_array().is_some_and(|modifiers| {
            !modifiers.is_empty()
                && modifiers.iter().all(|modifier| {
                    value_kind(modifier) == Some("castOnlyIf")
                        && value_kind(&modifier["object"]) == Some("self")
                        && condition_supported(&modifier["condition"])
                })
        })
}

/// `Any player may activate this ability` (CR 602.2): a player who does not
/// control the source may activate exactly that printed or granted ability.
pub(super) fn any_player_may_activate(
    source: &CardInstance,
    action: &LegalAction,
    rule_index: usize,
) -> bool {
    let rule = action
        .decisions
        .get("grantedAbilityRule")
        .or_else(|| source.definition.printed_rules().get(rule_index));
    rule.is_some_and(|rule| rule["anyPlayerMayActivate"].as_bool() == Some(true))
}

impl GameEngine {
    /// Whether the ability has reached its activation limit (CR 602.5b).
    pub(super) fn activation_limit_reached(
        &self,
        permanent: &CardInstance,
        rule: &Value,
        rule_index: usize,
    ) -> bool {
        let Some(limit) = rule.get("activationLimit") else {
            return false;
        };
        let maximum = match value_kind(limit) {
            // CR 702.193a: effects may permit additional activations of a
            // class (`additionalActivation`).
            Some("oncePerGameObjectAbility") => Some(1_i32.saturating_add(
                self.activation_class_modifier_amount("additionalActivation", permanent, rule),
            ) as usize),
            Some("timesPerTurn") => limit["maximum"]
                .as_u64()
                .and_then(|maximum| usize::try_from(maximum).ok()),
            _ => None,
        };
        if let Some(maximum) = maximum {
            return activation_limit_use_count(
                permanent,
                limit,
                self.state.turn_number,
                rule_index,
            ) >= maximum;
        }
        activation_limit_flag(limit, self.state.turn_number, rule_index)
            .is_none_or(|flag| permanent.flags.get(&flag) == Some(&true))
    }

    /// Whether `player_id` may begin to cast `card` from `source_zone` at
    /// all (CR 601.3): no effect prohibits the card, spell casting or casting
    /// from that zone, and the card's own `Cast this spell only ...`
    /// restrictions hold. Every cast path (priority, prototype, casts during
    /// resolution and free casts) asks this one question. A face-down spell
    /// has no abilities, so it never carries the card's restrictions.
    pub(super) fn spell_cast_legality_allows(
        &self,
        player_id: &str,
        card: &CardInstance,
        source_zone: &str,
    ) -> bool {
        !self.card_casting_prohibited(player_id, &card.definition)
            && !self.spell_casting_prohibited(player_id)
            && !self.spell_cast_from_zone_prohibited(player_id, source_zone, &card.definition)
            && self.spell_cast_restrictions_allow(player_id, card)
            && self.aftermath_cast_allowed(player_id, card, source_zone)
    }

    /// Whether `player_id` may begin to cast `card` now under the card's own
    /// cast restrictions (CR 601.3). `your` in the condition names the caster.
    pub(super) fn spell_cast_restrictions_allow(
        &self,
        player_id: &str,
        card: &CardInstance,
    ) -> bool {
        card.definition.printed_rules().iter().all(|rule| {
            value_kind(rule) != Some("staticAbility")
                || !cast_restriction_rule_supported(rule)
                || rule["modifiers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .all(|modifier| {
                        self.condition_value_with_source(
                            &modifier["condition"],
                            player_id,
                            &BTreeMap::new(),
                            Some(card),
                        )
                    })
        })
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "activated_rule_execution_tests.rs"]
mod tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "activation_restriction_runtime_tests.rs"]
mod restriction_tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "canonical_stack_definition_tests.rs"]
mod stack_definition_tests;
