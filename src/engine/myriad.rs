//! Myriad (CR 702.116): "Whenever this creature attacks, for each opponent
//! other than defending player, you may create a token that's a copy of this
//! creature that's tapped and attacking that player or a planeswalker they
//! control. If one or more tokens are created this way, exile the tokens at
//! end of combat."
//!
//! The keyword is read by the shared keyword list (`myriad` on the creature
//! itself, or granted by a static or one-shot effect). Its triggered ability
//! is intrinsic: when a creature with myriad is declared as an attacker
//! (CR 508.3a, never when it is put onto the battlefield attacking), one
//! trigger per instance of the keyword (CR 702.116b) goes on the stack. The
//! trigger records the defending player of that attack. On resolution each
//! other opponent, in turn order, may receive one token copy (the ordinary
//! `createTokens` copy primitive) that enters tapped and attacking that
//! opponent or a planeswalker they control; every created token gets one
//! delayed end-of-combat exile trigger (CR 603.7).
use super::*;

/// The keyword kind a creature with myriad has.
pub(super) const MYRIAD_KEYWORD: &str = "myriad";
/// The runtime effect of one myriad trigger.
const MYRIAD_EFFECT: &str = "myriadAttackingCopies";
/// The defending player of the attack that triggered myriad.
const DEFENDING_PLAYER_DECISION: &str = "myriadDefendingPlayerId";

pub(super) fn myriad_effect_supported(effect: &Value) -> Option<bool> {
    (value_kind(effect) == Some(MYRIAD_EFFECT)).then_some(true)
}

fn target_label(target: &TargetRef) -> String {
    match target {
        TargetRef::Player { player_id } => player_id.clone(),
        TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id } => {
            instance_id.clone()
        }
        TargetRef::StackObject { stack_id } => stack_id.clone(),
    }
}

impl GameEngine {
    /// CR 702.116b: each printed instance of myriad triggers separately; a
    /// granted myriad without a printed one is one instance.
    fn myriad_instances(&self, permanent: &CardInstance) -> usize {
        if !self.permanent_has_effective_keyword(permanent, MYRIAD_KEYWORD) {
            return 0;
        }
        let printed = permanent
            .definition
            .printed_rules()
            .iter()
            .map(|rule| match value_kind(rule) {
                Some("keywordAbility") => {
                    usize::from(value_kind(&rule["ability"]) == Some(MYRIAD_KEYWORD))
                }
                Some("keywordAbilityGroup") => rule["abilities"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|ability| value_kind(ability) == Some(MYRIAD_KEYWORD))
                    .count(),
                _ => 0,
            })
            .sum::<usize>();
        printed.max(1)
    }

    /// Puts one myriad trigger per instance on the stack for each declared
    /// attacker with myriad (CR 508.3a, 702.116a).
    pub(super) fn enqueue_myriad_triggers(&mut self, attacker_ids: &[String]) {
        for attacker_id in attacker_ids {
            let Some(assignment) = self
                .state
                .combat
                .attackers
                .iter()
                .find(|assignment| &assignment.attacker_id == attacker_id)
                .cloned()
            else {
                continue;
            };
            let Some((player_index, card_index)) = self.permanent_position(attacker_id) else {
                continue;
            };
            let source = self.state.players[player_index].battlefield[card_index].clone();
            for _ in 0..self.myriad_instances(&source) {
                let _published = self.put_triggered_ability_on_stack_with_decisions(
                    &source,
                    json!({
                        "kind": "triggeredAbility",
                        "source": { "kind": "self" },
                        // The attack event that triggered it, for effects
                        // that count attack triggers (CR 603.2).
                        "event": { "kind": "declaredAttacker", "object": { "kind": "self" } },
                        "keywordProvenance": {
                            "keyword": MYRIAD_KEYWORD,
                            "rulesSection": "702.116a",
                        },
                        "effects": [{ "kind": MYRIAD_EFFECT }],
                    }),
                    BTreeMap::from([(
                        DEFENDING_PLAYER_DECISION.to_string(),
                        Value::String(assignment.defending_player_id.clone()),
                    )]),
                );
                if self.check_quantity_failure().is_err() {
                    return;
                }
            }
        }
    }

    /// Resolves one myriad trigger. `Ok(false)` when the effect is not a
    /// myriad effect.
    pub(super) fn execute_myriad_effect<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        decisions: &BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        if value_kind(effect) != Some(MYRIAD_EFFECT) {
            return Ok(false);
        }
        let controller_id = stack_object.controller.clone();
        let defending_player_id = stack_object
            .decisions
            .get(DEFENDING_PLAYER_DECISION)
            .and_then(Value::as_str)
            .ok_or_else(|| EngineError::new("a myriad trigger without its defending player"))?
            .to_string();
        // Each opponent other than defending player, in turn order from the
        // active player (CR 101.4).
        let player_count = self.state.players.len();
        let active = self.state.active_player;
        let mut opponents = self
            .state
            .players
            .iter()
            .enumerate()
            .filter(|(_, player)| {
                !player.has_lost && player.id != controller_id && player.id != defending_player_id
            })
            .map(|(index, player)| {
                (
                    (index + player_count - active) % player_count,
                    player.id.clone(),
                )
            })
            .collect::<Vec<_>>();
        opponents.sort();
        let copy = json!({
            "kind": "createTokens",
            "token": { "kind": "copyOf", "object": { "kind": "self" }, "except": [] },
            "quantity": { "kind": "integer", "value": 1 },
            "tapped": true,
        });
        for (_, opponent_id) in opponents {
            // `you may create`: the controller chooses per opponent.
            let choice = self.choose_resolution_option(
                stack_object,
                provider,
                &controller_id,
                vec!["create".to_string(), "decline".to_string()],
                "Create a myriad token copy attacking this opponent or a planeswalker they control?",
            )?;
            if choice.as_deref() != Some("create") {
                continue;
            }
            let created = self.create_tokens_with_provider(
                &copy,
                stack_object,
                &BTreeMap::new(),
                decisions,
                provider,
            )?;
            for token_id in created {
                self.put_myriad_token_attacking(&token_id, &opponent_id, stack_object, provider)?;
                self.install_end_of_combat_exile(&token_id, &controller_id, stack_object);
            }
        }
        Ok(true)
    }

    /// CR 702.116a, 508.4: the token attacks the chosen opponent or a
    /// planeswalker that player controls. Outside combat, or under the
    /// control of a player other than the active player, it is not attacking
    /// (CR 506.3).
    fn put_myriad_token_attacking<P: DecisionProvider>(
        &mut self,
        token_id: &str,
        opponent_id: &str,
        stack_object: &StackObject,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let Some((player_index, card_index)) = self.permanent_position(token_id) else {
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
        let mut defenders = std::iter::once(TargetRef::Player {
            player_id: opponent_id.to_string(),
        })
        .chain(self.state.players.iter().flat_map(|player| {
            player
                .battlefield
                .iter()
                .filter(|permanent| {
                    permanent.controller == opponent_id
                        && has_card_type(&permanent.definition, "planeswalker")
                })
                .map(|permanent| TargetRef::Permanent {
                    instance_id: permanent.instance_id.clone(),
                })
        }))
        .filter_map(|defender| {
            self.attack_defending_player_id(&active_player_id, &defender)
                .map(|defending_player_id| (defender, defending_player_id))
        })
        .collect::<Vec<_>>();
        let (defender, defending_player_id) = match defenders.len() {
            0 => return Ok(()),
            1 => defenders.remove(0),
            _ => {
                let options = defenders
                    .iter()
                    .map(|(defender, _)| target_label(defender))
                    .collect();
                let selected = self.choose_resolution_option(
                    stack_object,
                    provider,
                    &controller,
                    options,
                    "Choose the player or planeswalker the myriad token is attacking.",
                )?;
                let index = selected
                    .and_then(|selected| {
                        defenders
                            .iter()
                            .position(|(defender, _)| target_label(defender) == selected)
                    })
                    .ok_or_else(|| EngineError::new("a myriad token needs a chosen defender"))?;
                defenders.remove(index)
            }
        };
        if let Some((player_index, card_index)) = self.permanent_position(token_id) {
            self.state.players[player_index].battlefield[card_index]
                .flags
                .insert("attacking".to_string(), true);
        }
        self.state.combat.attackers.push(DeclaredAttacker {
            attacker_id: token_id.to_string(),
            defender,
            defending_player_id,
        });
        Ok(())
    }

    /// The delayed `exile the tokens at end of combat` trigger (CR 603.7),
    /// one per created token, tracking that token.
    fn install_end_of_combat_exile(
        &mut self,
        token_id: &str,
        controller_id: &str,
        stack_object: &StackObject,
    ) {
        self.state.rule_modifiers.push(json!({
            "kind": "delayedStepTrigger",
            "step": "endOfCombat",
            "controllerId": controller_id,
            "effects": [{
                "kind": "exilePermanent",
                "permanent": { "kind": "triggeringPermanent" },
            }],
            "sourceCard": stack_object.card,
            "trackedObjectId": token_id,
            "capturedBindings": {},
            "expiresAfterTurn": self.state.turn_number,
        }));
    }
}
