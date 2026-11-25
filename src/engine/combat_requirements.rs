//! Combat requirements, block capacity and lone-combatant restrictions at
//! declaration (CR 506.5, 508.1c-d, 509.1a-c).
//!
//! Effects install one `combatRequirement` rule modifier per affected
//! creature: the creature attacks (a named player) or blocks (a named
//! attacker) if able, for the current turn or the current combat. Static
//! abilities grant `blockCapacity` modifiers and the `cantAttackAlone`,
//! `cantBlockAlone`, `canBlockAdditionalCreature` and `canBlockAnyNumber`
//! markers. The declaration loops in `mod.rs` ask this module, one hook per
//! decision:
//!
//! - which defenders and attackers a creature may still be declared against
//!   while one of its requirements can be obeyed (CR 508.1d, 509.1c: the
//!   declaration obeys the maximum number of requirements without breaking a
//!   restriction; a directed requirement only narrows the choice while it is
//!   satisfiable);
//! - whether the declaration may end while an obeyable requirement or a lone
//!   restricted combatant remains;
//! - how many attackers one blocker may block, and how its combat damage is
//!   divided among them (CR 510.1d).
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::{
    ActionKind, CardInstance, EngineError, GameEngine, LegalAction, RuntimeBinding, StackObject,
    TargetRef, object_expression_supported, player_reference_supported,
    static_object_selector_supported, target_reference_supported, value_kind,
};

const REQUIREMENT_KIND: &str = "combatRequirement";
const CAPACITY_KIND: &str = "blockCapacity";

/// `{"kind":"blockCapacity","objects":SELECTOR,"additional":N}` or
/// `"anyNumber":true`: a standing capacity granted by a static ability.
pub(super) fn block_capacity_modifier_supported(modifier: &Value) -> bool {
    static_object_selector_supported(&modifier["objects"])
        && match (modifier.get("additional"), modifier.get("anyNumber")) {
            (Some(additional), None) => additional.as_u64().is_some_and(|count| count >= 1),
            (None, Some(any)) => any.as_bool() == Some(true),
            _ => false,
        }
}

/// The window of an installed requirement: the current turn, or the current
/// combat only (it ends when the combat state is cleared).
fn window_supported(window: &Value) -> bool {
    matches!(
        value_kind(window),
        Some("untilEndOfCurrentTurn" | "thisCombat")
    )
}

fn single_object_supported(reference: &Value) -> bool {
    matches!(value_kind(reference), Some("self" | "abilitySource"))
        || target_reference_supported(reference)
}

/// `installCombatRequirement`: `creatures` is any object expression; an
/// attack requirement may name one `defender` player, a block requirement
/// one `attacker` object.
pub(super) fn combat_requirement_effect_supported(effect: &Value) -> bool {
    let creatures = single_object_supported(&effect["creatures"])
        || object_expression_supported(&effect["creatures"]);
    let direction = match effect["requirement"].as_str() {
        Some("attack") => {
            effect.get("attacker").is_none()
                && effect
                    .get("defender")
                    .is_none_or(player_reference_supported)
        }
        Some("block") => {
            effect.get("defender").is_none()
                && effect.get("attacker").is_none_or(single_object_supported)
        }
        _ => false,
    };
    creatures && direction && window_supported(&effect["window"])
}

/// True for a requirement installed for the current combat only: the
/// combat cleanup removes it with the combat state.
pub(super) fn ends_with_combat(modifier: &Value) -> bool {
    value_kind(modifier) == Some(REQUIREMENT_KIND)
        && modifier["untilEndOfCombat"].as_bool() == Some(true)
}

/// `lockLifeTotal` (CR 119.3 can't change): the player's life total can't
/// change until the end of the turn or the start of a player's next turn.
pub(super) fn life_total_lock_supported(effect: &Value) -> bool {
    player_reference_supported(&effect["player"])
        && match value_kind(&effect["duration"]) {
            Some("untilEndOfCurrentTurn") => true,
            Some("untilNextTurn") => player_reference_supported(&effect["duration"]["player"]),
            _ => false,
        }
}

/// One active requirement of a creature.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Requirement {
    /// Attack, any defender when `defender` is `None`.
    Attack { defender: Option<String> },
    /// Block, any attacker when `attacker` is `None`.
    Block { attacker: Option<String> },
}

impl GameEngine {
    fn requirement_active(&self, modifier: &Value) -> bool {
        value_kind(modifier) == Some(REQUIREMENT_KIND)
            && modifier["expiresAfterTurn"]
                .as_u64()
                .is_some_and(|turn| turn >= u64::from(self.state.turn_number))
    }

    /// The effect-installed requirements of `creature` still in force.
    fn installed_requirements(&self, creature: &CardInstance) -> Vec<Requirement> {
        self.state
            .rule_modifiers
            .iter()
            .filter(|modifier| {
                self.requirement_active(modifier)
                    && modifier["creatureId"].as_str() == Some(creature.instance_id.as_str())
                    && Self::modifier_incarnation_current(modifier, creature)
            })
            .filter_map(|modifier| match modifier["requirement"].as_str() {
                Some("attack") => Some(Requirement::Attack {
                    defender: modifier["defenderPlayerId"].as_str().map(str::to_string),
                }),
                Some("block") => Some(Requirement::Block {
                    attacker: modifier["attackerId"].as_str().map(str::to_string),
                }),
                _ => None,
            })
            .collect()
    }

    /// CR 508.1d: declaring `action` obeys an installed attack requirement
    /// of `card`: any attack for an undirected one, an attack on the named
    /// player for a directed one. The caller keeps the declaration open
    /// while such an action is offered.
    pub(super) fn installed_attack_requirement_obeyed_by(
        &self,
        card: &CardInstance,
        action: &LegalAction,
    ) -> bool {
        let attacked_player = match action.targets.get("defender") {
            Some(TargetRef::Player { player_id }) => Some(player_id.as_str()),
            _ => None,
        };
        self.installed_requirements(card)
            .iter()
            .any(|requirement| match requirement {
                Requirement::Attack { defender: None } => true,
                Requirement::Attack {
                    defender: Some(required),
                } => attacked_player == Some(required.as_str()),
                Requirement::Block { .. } => false,
            })
    }

    /// CR 508.1d: a requirement to attack one player is obeyed by attacking
    /// that player whenever that player is a legal defender of `card`; every
    /// other defender is then excluded. A requirement that can't be obeyed
    /// excludes nothing.
    pub(super) fn attack_requirement_excludes_defender(
        &self,
        card: &CardInstance,
        defender: &TargetRef,
        defenders: &[(TargetRef, String)],
        active_player_id: &str,
    ) -> Result<bool, EngineError> {
        for requirement in self.installed_requirements(card) {
            let Requirement::Attack {
                defender: Some(required),
            } = requirement
            else {
                continue;
            };
            let attacks_required_player = matches!(
                defender,
                TargetRef::Player { player_id } if *player_id == required
            );
            if attacks_required_player {
                continue;
            }
            let required_is_legal = defenders.iter().any(|(candidate, _)| {
                matches!(candidate, TargetRef::Player { player_id } if *player_id == required)
                    && self
                        .attack_defending_player_id(active_player_id, candidate)
                        .is_some()
            }) && !self.defending_player_forbids_attack(card, &required)?;
            // Another player, or a planeswalker or battle of the required
            // player, is not the player the requirement names.
            if required_is_legal {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The most attackers `blocker` may block (CR 509.1a): one, plus every
    /// additional creature granted, or `None` for any number.
    pub(super) fn blocker_capacity(&self, blocker: &CardInstance) -> Option<usize> {
        if self.permanent_has_keyword(blocker, "canBlockAnyNumber") {
            return None;
        }
        let mut capacity = 1_usize;
        if self.permanent_has_keyword(blocker, "canBlockAdditionalCreature") {
            capacity = capacity.saturating_add(1);
        }
        for (source, modifier) in self.battlefield_static_modifiers() {
            if value_kind(&modifier) != Some(CAPACITY_KIND)
                || !self.combat_restriction_selector_matches(&source, &modifier["objects"], blocker)
            {
                continue;
            }
            if modifier["anyNumber"].as_bool() == Some(true) {
                return None;
            }
            let additional = modifier["additional"]
                .as_u64()
                .and_then(|count| usize::try_from(count).ok())
                .unwrap_or(0);
            capacity = capacity.saturating_add(additional);
        }
        Some(capacity)
    }

    fn attackers_blocked_by(blocker_id: &str, blocks: &BTreeMap<String, Vec<String>>) -> usize {
        blocks
            .values()
            .filter(|blockers| blockers.iter().any(|id| id == blocker_id))
            .count()
    }

    /// CR 509.1a: `blocker` may be declared against one more attacker.
    pub(super) fn blocker_has_capacity(
        &self,
        blocker: &CardInstance,
        blocks: &BTreeMap<String, Vec<String>>,
    ) -> bool {
        let blocked = Self::attackers_blocked_by(&blocker.instance_id, blocks);
        blocked == 0
            || self
                .blocker_capacity(blocker)
                .is_none_or(|capacity| blocked < capacity)
    }

    /// The attackers `blocker` is required to block (CR 509.1c) that it
    /// can still legally block in this declaration.
    fn obeyable_block_requirements(
        &self,
        blocker: &CardInstance,
        attackers: &[String],
        blocks: &BTreeMap<String, Vec<String>>,
    ) -> Vec<String> {
        self.installed_requirements(blocker)
            .into_iter()
            .filter_map(|requirement| match requirement {
                Requirement::Block {
                    attacker: Some(attacker),
                } => Some(attacker),
                _ => None,
            })
            .filter(|attacker_id| attackers.contains(attacker_id))
            .filter(|attacker_id| {
                !blocks
                    .get(attacker_id)
                    .is_some_and(|blockers| blockers.contains(&blocker.instance_id))
            })
            .filter(|attacker_id| {
                self.permanent_position(attacker_id)
                    .is_some_and(|(player_index, card_index)| {
                        self.can_block(
                            blocker,
                            &self.state.players[player_index].battlefield[card_index],
                        )
                    })
            })
            .collect()
    }

    /// One block pair offered to the declaration is excluded when the
    /// blocker already blocks that attacker, or when the blocker must block
    /// another attacker it can still block (CR 509.1c) and this pair would
    /// use its last capacity.
    pub(super) fn block_pair_excluded(
        &self,
        blocker: &CardInstance,
        attacker_id: &str,
        attackers: &[String],
        blocks: &BTreeMap<String, Vec<String>>,
    ) -> bool {
        if blocks
            .get(attacker_id)
            .is_some_and(|blockers| blockers.contains(&blocker.instance_id))
        {
            return true;
        }
        let required = self.obeyable_block_requirements(blocker, attackers, blocks);
        if required.is_empty() || required.iter().any(|id| id == attacker_id) {
            return false;
        }
        let remaining = self.blocker_capacity(blocker).map(|capacity| {
            capacity.saturating_sub(Self::attackers_blocked_by(&blocker.instance_id, blocks))
        });
        remaining.is_some_and(|remaining| remaining <= required.len())
    }

    /// CR 509.1c: the block declaration stays open while an untapped
    /// creature of the defending player can still obey one of its
    /// requirements: a directed block it is not making yet, or an
    /// undirected one while it blocks nothing.
    pub(super) fn block_requirement_pending(
        &self,
        defending_index: usize,
        attackers: &[String],
        blocks: &BTreeMap<String, Vec<String>>,
        offered: &[LegalAction],
    ) -> bool {
        let defender = &self.state.players[defending_index];
        defender.battlefield.iter().any(|blocker| {
            let offers = |attacker: Option<&str>| {
                offered.iter().any(|action| {
                    action.blocker_id.as_deref() == Some(blocker.instance_id.as_str())
                        && attacker
                            .is_none_or(|attacker| action.attacker_id.as_deref() == Some(attacker))
                })
            };
            if !offers(None) {
                return false;
            }
            let directed = self
                .obeyable_block_requirements(blocker, attackers, blocks)
                .iter()
                .any(|attacker| offers(Some(attacker.as_str())));
            let undirected = Self::attackers_blocked_by(&blocker.instance_id, blocks) == 0
                && self
                    .installed_requirements(blocker)
                    .iter()
                    .any(|requirement| *requirement == Requirement::Block { attacker: None });
            directed || undirected
        })
    }

    /// CR 506.5: a creature that can't attack alone is offered, while no
    /// attacker is declared yet, only if the declaration offered after it
    /// still contains another attacker. The partner test asks the same
    /// attacker-offer computation the declaration uses (defender, land-only
    /// additional combats, prohibitions, attack-alone rules, participant
    /// limits and taxes), never a separate eligibility heuristic.
    pub(super) fn retain_lone_attack_partners(
        &self,
        selected: &BTreeSet<String>,
        battlefield: &[CardInstance],
        actions: &mut Vec<LegalAction>,
    ) -> Result<(), EngineError> {
        if !selected.is_empty() {
            return Ok(());
        }
        let restricted = actions
            .iter()
            .filter(|action| action.kind == ActionKind::DeclareAttacker)
            .filter_map(|action| action.attacker_id.clone())
            .filter(|id| {
                battlefield.iter().any(|card| {
                    card.instance_id == *id && self.permanent_has_keyword(card, "cantAttackAlone")
                })
            })
            .collect::<BTreeSet<_>>();
        for id in restricted {
            let with = BTreeSet::from([id.clone()]);
            // The nested declaration has a declared attacker, so it never
            // recurses into this test again; it is asked as a revisable
            // declaration so a missing partner is an answer, not a refusal.
            let partner = self
                .legal_attacker_actions_with_revisions(&with, true)?
                .iter()
                .any(|action| {
                    action.kind == ActionKind::DeclareAttacker
                        && action.attacker_id.as_deref() != Some(id.as_str())
                });
            if !partner {
                actions.retain(|action| {
                    !(action.kind == ActionKind::DeclareAttacker
                        && action.attacker_id.as_deref() == Some(id.as_str()))
                });
            }
        }
        Ok(())
    }

    /// CR 506.5: the attack declaration can't end with one attacker that
    /// can't attack alone. While another attacker is offered the declaration
    /// stays open; when none remains (a partner's offer was lost to a payment
    /// made for the first attacker), only a revision can make it legal, and
    /// without revisions the declaration is refused explicitly.
    pub(super) fn lone_attacker_pending(
        &self,
        selected: &BTreeSet<String>,
        battlefield: &[CardInstance],
        offered: &[LegalAction],
        allow_revisions: bool,
    ) -> Result<bool, EngineError> {
        if selected.len() != 1 {
            return Ok(false);
        }
        let Some(only) = selected.iter().next() else {
            return Ok(false);
        };
        let Some(card) = battlefield
            .iter()
            .find(|card| card.instance_id == *only)
            .filter(|card| self.permanent_has_keyword(card, "cantAttackAlone"))
        else {
            return Ok(false);
        };
        let partner_offered = offered.iter().any(|action| {
            action.kind == ActionKind::DeclareAttacker
                && action.attacker_id.as_deref() != Some(only.as_str())
        });
        if partner_offered || allow_revisions {
            return Ok(true);
        }
        Err(EngineError::new(format!(
            "attack declaration can't end: {} can't attack alone and no other creature can still \
             attack (CR 506.5)",
            card.definition.name
        )))
    }

    /// CR 506.5: a creature that can't block alone is offered for one
    /// attacker, while no blocker is declared yet, only if the declaration
    /// offered after that block still contains another blocker. The partner
    /// test reuses the full blocker-offer computation (evasion, menace,
    /// capacity, prohibitions and requirements) for that exact block.
    pub(super) fn retain_lone_block_partners(
        &self,
        defending_index: usize,
        attackers: &[String],
        used_blockers: &BTreeSet<String>,
        blocks: &BTreeMap<String, Vec<String>>,
        actions: &mut Vec<LegalAction>,
    ) {
        if !used_blockers.is_empty() {
            return;
        }
        let battlefield = &self.state.players[defending_index].battlefield;
        let restricted = |blocker_id: &str| {
            battlefield.iter().any(|card| {
                card.instance_id == blocker_id && self.permanent_has_keyword(card, "cantBlockAlone")
            })
        };
        let unpartnered = actions
            .iter()
            .filter(|action| action.kind == ActionKind::DeclareBlocker)
            .filter_map(|action| Some((action.blocker_id.clone()?, action.attacker_id.clone()?)))
            .filter(|(blocker, _)| restricted(blocker))
            .filter(|(blocker, attacker)| {
                let used = BTreeSet::from([blocker.clone()]);
                let mut after = blocks.clone();
                after
                    .entry(attacker.clone())
                    .or_default()
                    .push(blocker.clone());
                // The nested declaration has a declared blocker, so it never
                // recurses into this test again.
                !self
                    .legal_blocker_actions(defending_index, attackers, &used, &after)
                    .iter()
                    .any(|action| {
                        action.kind == ActionKind::DeclareBlocker
                            && action.blocker_id.as_deref() != Some(blocker.as_str())
                    })
            })
            .collect::<BTreeSet<_>>();
        actions.retain(|action| {
            !(action.kind == ActionKind::DeclareBlocker
                && action
                    .blocker_id
                    .clone()
                    .zip(action.attacker_id.clone())
                    .is_some_and(|pair| unpartnered.contains(&pair)))
        });
    }

    /// CR 506.5: the block declaration can't end with one blocker that can't
    /// block alone. It stays open while another block is offered; when none
    /// remains, the declaration loop refuses it explicitly.
    pub(super) fn lone_blocker_pending(
        &self,
        used_blockers: &BTreeSet<String>,
        battlefield: &[CardInstance],
    ) -> bool {
        if used_blockers.len() != 1 {
            return false;
        }
        let Some(only) = used_blockers.iter().next() else {
            return false;
        };
        battlefield
            .iter()
            .find(|card| card.instance_id == *only)
            .is_some_and(|card| self.permanent_has_keyword(card, "cantBlockAlone"))
    }

    /// CR 510.1d: a creature blocking several attackers divides its combat
    /// damage among them. Without a division decision in the protocol yet,
    /// the whole damage goes to the first attacker it blocks in declaration
    /// order (a legal division); the other attackers receive none.
    pub(super) fn blocker_damage_share(
        &self,
        blocker_id: &str,
        attacker_id: &str,
        power: i32,
        blocks: &BTreeMap<String, Vec<String>>,
    ) -> i32 {
        let first_blocked = self
            .state
            .combat
            .blockers
            .iter()
            .filter(|declared| declared.blocker_id == blocker_id)
            .map(|declared| declared.attacker_id.as_str())
            .find(|attacker| {
                blocks
                    .get(*attacker)
                    .is_some_and(|blockers| blockers.iter().any(|id| id == blocker_id))
                    && self.permanent_position(attacker).is_some()
            });
        match first_blocked {
            Some(first) if first != attacker_id => 0,
            _ => power,
        }
    }

    /// Resolves `installCombatRequirement`: one rule modifier per affected
    /// creature, naming the resolved defender player or attacker. A missing
    /// creature, defender or attacker installs nothing (CR 608.2b).
    pub(super) fn install_combat_requirement(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<(), EngineError> {
        let creatures =
            self.requirement_object_ids(&effect["creatures"], stack_object, bindings, decisions);
        let attacker = match effect.get("attacker") {
            None => None,
            Some(reference) => {
                match self
                    .requirement_object_ids(reference, stack_object, bindings, decisions)
                    .as_slice()
                {
                    [attacker] => Some(attacker.clone()),
                    _ => return Ok(()),
                }
            }
        };
        let defender = match effect.get("defender") {
            None => None,
            Some(reference) => match self.resolve_player(reference, stack_object, bindings) {
                Some(player_id) => Some(player_id),
                None => return Ok(()),
            },
        };
        let until_end_of_combat = value_kind(&effect["window"]) == Some("thisCombat");
        for creature_id in creatures {
            let Some((player_index, card_index)) = self.permanent_position(&creature_id) else {
                continue;
            };
            let creature = &self.state.players[player_index].battlefield[card_index];
            let mut modifier = json!({
                "kind": REQUIREMENT_KIND,
                "requirement": effect["requirement"],
                "creatureId": creature_id,
                "zoneIdentity": Self::incarnation_binding(creature),
                "expiresAfterTurn": self.state.turn_number,
                "untilEndOfCombat": until_end_of_combat,
                "sourceCardInstanceId": stack_object.card.instance_id,
            });
            if let Some(attacker) = &attacker {
                modifier["attackerId"] = json!(attacker);
            }
            if let Some(defender) = &defender {
                modifier["defenderPlayerId"] = json!(defender);
            }
            self.state.rule_modifiers.push(modifier);
        }
        Ok(())
    }

    fn requirement_object_ids(
        &self,
        reference: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Vec<String> {
        if matches!(value_kind(reference), Some("self" | "abilitySource")) {
            return vec![stack_object.card.instance_id.clone()];
        }
        if target_reference_supported(reference) {
            return match self.resolve_target(reference, stack_object, bindings) {
                Some(TargetRef::Permanent { instance_id }) => vec![instance_id],
                _ => Vec::new(),
            };
        }
        self.resolve_runtime_object_ids(reference, stack_object, bindings, decisions)
            .unwrap_or_default()
    }

    /// Resolves `lockLifeTotal`: the player's life total can't change (CR
    /// 119.3 is replaced by nothing: gains and losses don't happen) until
    /// the end of this turn or the start of the named player's next turn.
    pub(super) fn install_life_total_lock(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Result<(), EngineError> {
        let Some(player_id) = self.resolve_player(&effect["player"], stack_object, bindings) else {
            return Ok(());
        };
        let mut modifier = json!({
            "kind": "lifeTotalCannotChange",
            "playerId": player_id,
            "sourceCardInstanceId": stack_object.card.instance_id,
        });
        match value_kind(&effect["duration"]) {
            Some("untilNextTurn") => {
                let Some(expiry) =
                    self.resolve_player(&effect["duration"]["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                modifier["expiresAtTurnStart"] = json!(expiry);
            }
            _ => modifier["expiresAfterTurn"] = json!(self.state.turn_number),
        }
        self.state.rule_modifiers.push(modifier);
        Ok(())
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "combat_requirements_tests.rs"]
mod tests;
