//! Permanents put onto the battlefield attacking (CR 508.4).
//!
//! One writer for every entry that joins combat without a declaration: a
//! moved card (`put it onto the battlefield tapped and attacking`) and a
//! created token (`create a token that's tapped and attacking`). The
//! permanent was never declared as an attacker, so no ability that triggers
//! on declaring attackers sees it (CR 508.3a, 508.4).
use super::*;

/// The defender policy an attacking entry states: any defender, one named
/// player, or that player and the planeswalkers they control.
pub(super) fn entering_attack_supported(policy: &Value) -> bool {
    match value_kind(policy) {
        Some("anyDefender") => true,
        Some("player" | "playerOrPlaneswalkers") => player_reference_supported(&policy["player"]),
        _ => false,
    }
}

impl GameEngine {
    /// Puts each permanent into combat attacking, as `policy` admits
    /// (CR 508.4). A named player who is gone leaves no legal defender, so
    /// the permanents stay on the battlefield without attacking.
    pub(super) fn put_permanents_into_combat_attacking<P: DecisionProvider>(
        &mut self,
        permanent_ids: &[String],
        policy: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let named_player = match value_kind(policy) {
            Some("player" | "playerOrPlaneswalkers") => {
                match self.resolve_player(&policy["player"], stack_object, bindings) {
                    Some(player_id) => Some(player_id),
                    None => return Ok(()),
                }
            }
            _ => None,
        };
        let planeswalkers = value_kind(policy) == Some("playerOrPlaneswalkers");
        for permanent_id in permanent_ids {
            self.put_permanent_into_combat_attacking(
                permanent_id,
                named_player.as_deref(),
                planeswalkers,
                stack_object,
                provider,
            )?;
        }
        Ok(())
    }

    /// CR 508.4: only during combat and only under the active player's
    /// control does an entering permanent attack; its controller chooses
    /// the defender among those the effect admits (CR 508.4a, 506.2).
    pub(super) fn put_permanent_into_combat_attacking<P: DecisionProvider>(
        &mut self,
        permanent_id: &str,
        named_player: Option<&str>,
        planeswalkers: bool,
        stack_object: &StackObject,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let Some((player_index, card_index)) = self.permanent_position(permanent_id) else {
            return Ok(());
        };
        let controller = self.state.players[player_index].battlefield[card_index]
            .controller
            .clone();
        let active_player_id = self.state.players[self.state.active_player].id.clone();
        let in_combat = matches!(
            self.state.step,
            GameStep::DeclareAttackers | GameStep::DeclareBlockers | GameStep::CombatDamage
        );
        if !in_combat || controller != active_player_id {
            return Ok(());
        }
        let admitted = |engine: &Self, defender: &TargetRef| match (named_player, defender) {
            (None, _) => true,
            (Some(named), TargetRef::Player { player_id }) => player_id == named,
            (Some(named), TargetRef::Permanent { instance_id }) => {
                planeswalkers
                    && engine
                        .permanent_position(instance_id)
                        .is_some_and(|(owner_index, index)| {
                            let permanent = &engine.state.players[owner_index].battlefield[index];
                            has_card_type(&permanent.definition, "planeswalker")
                                && permanent.controller == named
                        })
            }
            _ => false,
        };
        let mut defenders = self
            .state
            .players
            .iter()
            .map(|player| TargetRef::Player {
                player_id: player.id.clone(),
            })
            .chain(self.state.players.iter().flat_map(|player| {
                player
                    .battlefield
                    .iter()
                    .map(|permanent| TargetRef::Permanent {
                        instance_id: permanent.instance_id.clone(),
                    })
            }))
            .filter(|defender| admitted(self, defender))
            .filter_map(|defender| {
                self.attack_defending_player_id(&active_player_id, &defender)
                    .map(|defending_player_id| (defender, defending_player_id))
            })
            .collect::<Vec<_>>();
        let label = |defender: &TargetRef| match defender {
            TargetRef::Player { player_id } => player_id.clone(),
            TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id } => {
                instance_id.clone()
            }
            TargetRef::StackObject { stack_id } => stack_id.clone(),
        };
        let (defender, defending_player_id) = match defenders.len() {
            0 => return Ok(()),
            1 => defenders.remove(0),
            _ => {
                let options = defenders
                    .iter()
                    .map(|(defender, _)| label(defender))
                    .collect();
                let selected = self.choose_resolution_option(
                    stack_object,
                    provider,
                    &controller,
                    options,
                    "Choose the player, planeswalker or battle the permanent is attacking.",
                )?;
                let index = selected
                    .and_then(|selected| {
                        defenders
                            .iter()
                            .position(|(defender, _)| label(defender) == selected)
                    })
                    .ok_or_else(|| {
                        EngineError::new("an attacking permanent needs a chosen defender")
                    })?;
                defenders.remove(index)
            }
        };
        if let Some((player_index, card_index)) = self.permanent_position(permanent_id) {
            self.state.players[player_index].battlefield[card_index]
                .flags
                .insert("attacking".to_string(), true);
        }
        self.state.combat.attackers.push(DeclaredAttacker {
            attacker_id: permanent_id.to_string(),
            defender,
            defending_player_id,
        });
        Ok(())
    }
}
