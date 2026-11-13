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
            .legal_priority_actions(player_index)
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
        self.apply_priority_action(&action, provider)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "activated_rule_execution_tests.rs"]
mod tests;
