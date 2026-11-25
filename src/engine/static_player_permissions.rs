//! Static player permissions whose operand is the permission's own source
//! (CR 609.3): the options they offer belong to the turn-based procedures
//! that read them (attack declaration, combat damage assignment, casting
//! timing and cost). Each permission is a typed static modifier; the
//! procedures call the one reader here instead of re-matching JSON shapes.
//!
//! - `castAsThoughFlash` (CR 702.8a): a card's own timing permission,
//!   functioning from its hand.
//! - `exertAsAttacks` (CR 701.43, 508.1g, 603.12): an attack-declaration
//!   option and the reflexive ability it creates.
//! - `assignCombatDamageAsThoughUnblocked` (CR 510.1c-d).
//! - `castingPermission.alternativeManaCost` (CR 118.9): a static alternative
//!   cost whose mana is paid instead of the waived printed cost.
//!
//! Global rule modifiers of a static ability or of a created continuous
//! effect (CR 611.2a) live here too, each read by the one procedure it
//! changes:
//!
//! - `legendRuleDoesNotApply` (CR 704.5j): the state-based action ignores the
//!   permanents of a static object selector.
//! - `combatParticipantLimit` (CR 508.1c, 509.1c): no more than N creatures
//!   attack (or block) in one declaration.
//! - `activateAsThoughHaste` (CR 302.6): a tap-symbol cost of the selected
//!   creatures may be paid despite summoning sickness.
//! - `cantWinGame` (CR 104.2a), `prohibitDraw` (CR 121.1), `skipUntapStep`
//!   (CR 500.11) and `revealLibraryTop` (CR 401.2) over a player population.
use super::zone_of_function::{ZoneOfFunction, static_zone_of_function};
use super::*;

/// The decision key of the attack option.
pub(super) const EXERT_AS_ATTACKS: &str = "exertAsAttacks";
/// The assignment key of a blocked attacker that assigns its combat damage
/// as though it weren't blocked. It is not an object id.
pub(super) const AS_THOUGH_UNBLOCKED: &str = "asThoughUnblocked";
/// The decision carrying a timing permission's cost increase.
pub(super) const FLASH_TIMING_MANA_COST: &str = "flashTimingManaCost";
/// The pending reflexive ability of an exerted attacker.
const PENDING_EXERT_REFLEXIVE: &str = "pendingExertReflexiveTrigger";

fn names_self(modifier: &Value) -> bool {
    value_kind(&modifier["object"]) == Some("self")
}

/// Validation of the static modifiers this module reads. `false` keeps an
/// unknown modifier unsupported.
pub(super) fn static_player_permission_modifier_supported(modifier: &Value) -> bool {
    match value_kind(modifier) {
        Some("castAsThoughFlash") => {
            names_self(modifier)
                && modifier.get("additionalManaCost").is_none_or(|cost| {
                    cost.as_str()
                        .is_some_and(|cost| !cost.is_empty() && !cost.contains("{X}"))
                })
        }
        Some("assignCombatDamageAsThoughUnblocked") => names_self(modifier),
        Some("legendRuleDoesNotApply" | "activateAsThoughHaste") => {
            static_object_selector_supported(&modifier["objects"])
        }
        Some("combatParticipantLimit") => {
            matches!(modifier["role"].as_str(), Some("attack" | "block"))
                && integer_value(&modifier["maximum"]).is_some_and(|maximum| maximum > 0)
        }
        Some("cantWinGame" | "prohibitDraw" | "skipUntapStep" | "revealLibraryTop") => {
            super::player_static_modifiers::static_player_population_supported(&modifier["player"])
        }
        Some("exertAsAttacks") => {
            names_self(modifier)
                && modifier.get("reflexive").is_none_or(|ability| {
                    value_kind(ability) == Some("triggeredAbility")
                        && value_kind(&ability["event"]) == Some("reflexiveTriggerCreated")
                        && triggered_rule_supported(ability)
                })
        }
        _ => false,
    }
}

/// The mana a static alternative cost names, for a declaration that selected
/// that permission's waived base (CR 118.9, 601.2f: the alternative cost is
/// the total before increases and reductions).
pub(super) fn static_permission_mana_costs(decisions: &BTreeMap<String, Value>) -> Vec<String> {
    let alternative = (decisions
        .get("usesStaticCastingPermissionBaseCost")
        .and_then(Value::as_bool)
        == Some(true))
    .then(|| {
        decisions
            .get("staticCastingPermissionAuthorization")
            .and_then(|authorization| authorization["policy"]["alternativeManaCost"].as_str())
    })
    .flatten();
    // CR 601.2f: the cost increase a timing permission adds when the spell
    // is cast only through it (`if you pay {2} more to cast it`).
    let timing = decisions
        .get(FLASH_TIMING_MANA_COST)
        .and_then(Value::as_str);
    alternative
        .into_iter()
        .chain(timing)
        .map(ToOwned::to_owned)
        .collect()
}

/// Whether a castingPermission names its own source card as the only card
/// it permits (`You may cast this card from your graveyard`).
pub(super) fn casting_permission_admits_card(
    modifier: &Value,
    source: &CardInstance,
    card: &CardInstance,
) -> bool {
    !names_self(modifier) || source.instance_id == card.instance_id
}

/// Whether a blocked attacker's assignment chose the unblocked manner.
pub(super) fn assigned_as_though_unblocked(assignments: Option<&BTreeMap<String, i32>>) -> bool {
    assignments.is_some_and(|assignments| assignments.contains_key(AS_THOUGH_UNBLOCKED))
}

impl GameEngine {
    /// The active modifier `kind` whose object is this permanent itself.
    fn own_static_permission(&self, permanent_id: &str, kind: &str) -> Option<Value> {
        self.battlefield_static_modifiers()
            .into_iter()
            .find(|(source, modifier)| {
                source.instance_id == permanent_id
                    && value_kind(modifier) == Some(kind)
                    && names_self(modifier)
            })
            .map(|(_, modifier)| modifier)
    }

    /// CR 702.8a: the card being cast carries a timing permission of its
    /// own that functions from `source_zone` and whose condition holds for
    /// the caster. The definition is the card itself, so `self` needs no
    /// identity lookup.
    pub(super) fn printed_self_flash_permission(
        &self,
        player_id: &str,
        card: &CardDefinition,
        source_zone: &str,
    ) -> bool {
        self.printed_self_flash_modifiers(player_id, card, source_zone)
            .any(|modifier| modifier.get("additionalManaCost").is_none())
    }

    /// The cost increase of a timing permission of the card itself whose use
    /// costs more (`if you pay {2} more to cast it`, CR 601.2f): `None` when
    /// it has none that applies.
    pub(super) fn printed_self_flash_cost(
        &self,
        player_id: &str,
        card: &CardDefinition,
        source_zone: &str,
    ) -> Option<String> {
        self.printed_self_flash_modifiers(player_id, card, source_zone)
            .find_map(|modifier| modifier["additionalManaCost"].as_str())
            .map(ToOwned::to_owned)
    }

    /// The `castAsThoughFlash` modifiers of the card's own printed statics
    /// that function from `source_zone` and whose condition holds.
    fn printed_self_flash_modifiers<'a>(
        &'a self,
        player_id: &'a str,
        card: &'a CardDefinition,
        source_zone: &'a str,
    ) -> impl Iterator<Item = &'a Value> + 'a {
        card.printed_rules()
            .iter()
            .filter(move |rule| {
                value_kind(rule) == Some("staticAbility")
                    && static_zone_of_function(rule).is_some_and(|zone| {
                        zone == ZoneOfFunction::Hand && zone.functions_in(source_zone)
                    })
                    && rule.get("condition").is_none_or(|condition| {
                        self.condition_value_with_source(
                            condition,
                            player_id,
                            &BTreeMap::new(),
                            None,
                        )
                    })
            })
            .flat_map(|rule| rule["modifiers"].as_array().into_iter().flatten())
            .filter(|modifier| {
                value_kind(modifier) == Some("castAsThoughFlash") && names_self(modifier)
            })
    }

    /// CR 508.1g: each declared-attacker option of a creature with an exert
    /// permission is offered again with the creature exerted.
    pub(super) fn add_exert_attack_options(&self, actions: &mut Vec<LegalAction>) {
        let exerting = actions
            .iter()
            .filter(|action| action.kind == ActionKind::DeclareAttacker)
            .filter(|action| {
                action.attacker_id.as_deref().is_some_and(|attacker_id| {
                    self.own_static_permission(attacker_id, "exertAsAttacks")
                        .is_some()
                })
            })
            .map(|action| {
                let mut exerted = action.clone();
                exerted.id = format!("{}:exert", action.id);
                exerted.label = format!("{} and exert it", action.label);
                exerted
                    .decisions
                    .insert(EXERT_AS_ATTACKS.to_string(), Value::Bool(true));
                exerted
            })
            .collect::<Vec<_>>();
        actions.extend(exerting);
    }

    /// CR 701.43a-b: an attacker declared exerted won't untap during its
    /// controller's next untap step (the shared skip-next-untap rule), and
    /// its reflexive ability waits for the declaration to finish.
    pub(super) fn apply_exert_attack_option(
        &mut self,
        action: &LegalAction,
        attacker_id: &str,
    ) -> Result<(), EngineError> {
        if action
            .decisions
            .get(EXERT_AS_ATTACKS)
            .and_then(Value::as_bool)
            != Some(true)
        {
            return Ok(());
        }
        let permission = self
            .own_static_permission(attacker_id, "exertAsAttacks")
            .ok_or_else(|| EngineError::new("the attacker has no exert permission"))?;
        self.state.rule_modifiers.push(json!({
            "kind": "skipNextUntap",
            "sourceCardInstanceId": attacker_id,
            "cardInstanceId": attacker_id,
        }));
        self.record_event(
            "permanentExerted",
            Some(action.player_id.clone()),
            Some(attacker_id.to_string()),
            json!({ "reason": "attack" }),
        );
        if let Some(ability) = permission.get("reflexive") {
            self.state.rule_modifiers.push(json!({
                "kind": PENDING_EXERT_REFLEXIVE,
                "sourceCardInstanceId": attacker_id,
                "ability": ability,
            }));
        }
        Ok(())
    }

    /// CR 603.12, 603.3: the reflexive abilities of creatures exerted while
    /// attackers were declared go on the stack the next time a player would
    /// receive priority, with the other attack triggers.
    pub(super) fn enqueue_exert_reflexive_triggers(&mut self) {
        let mut pending = Vec::new();
        self.state.rule_modifiers.retain(|modifier| {
            if value_kind(modifier) == Some(PENDING_EXERT_REFLEXIVE) {
                pending.push(modifier.clone());
                false
            } else {
                true
            }
        });
        for modifier in pending {
            let Some(source) = modifier["sourceCardInstanceId"]
                .as_str()
                .and_then(|source_id| self.permanent_position(source_id))
                .map(|(player_index, card_index)| {
                    self.state.players[player_index].battlefield[card_index].clone()
                })
            else {
                continue;
            };
            self.put_triggered_ability_on_stack_with_origin(
                &source,
                modifier["ability"].clone(),
                BTreeMap::new(),
                trigger_multiplicity::TriggerOrigin::ObjectAbility,
            );
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }

    /// CR 510.1c-d: a blocked attacker with this permission may assign all
    /// its combat damage to the player or permanent it attacks. The choice
    /// is its controller's, made as combat damage is assigned.
    pub(super) fn choose_unblocked_combat_damage<P: DecisionProvider>(
        &mut self,
        provider: &mut P,
        attacker: &CardInstance,
        power: i32,
    ) -> Result<Option<BTreeMap<String, i32>>, EngineError> {
        if self
            .own_static_permission(&attacker.instance_id, "assignCombatDamageAsThoughUnblocked")
            .is_none()
        {
            return Ok(None);
        }
        let option = |unblocked: bool| LegalAction {
            id: format!(
                "combatDamageManner:{}:{}",
                attacker.instance_id,
                if unblocked { "unblocked" } else { "blocked" }
            ),
            kind: ActionKind::AssignCombatDamage,
            player_id: attacker.controller.clone(),
            label: if unblocked {
                format!(
                    "Assign {}'s combat damage as though it weren't blocked",
                    attacker.definition.name
                )
            } else {
                format!(
                    "Assign {}'s combat damage to its blockers",
                    attacker.definition.name
                )
            },
            card_instance_id: Some(attacker.instance_id.clone()),
            payment_sources: Vec::new(),
            decisions: BTreeMap::from([(AS_THOUGH_UNBLOCKED.to_string(), Value::Bool(unblocked))]),
            targets: BTreeMap::new(),
            target_order: Vec::new(),
            attacker_id: Some(attacker.instance_id.clone()),
            blocker_id: None,
        };
        let choice = self.choose_action(
            provider,
            EngineDecisionRequest {
                id: format!(
                    "combatDamageManner:{}:{}",
                    self.state.turn_number, attacker.instance_id
                ),
                kind: DecisionKind::CombatDamage,
                player_id: attacker.controller.clone(),
                source_card: Some(attacker.clone()),
                source_card_instance_id: Some(attacker.instance_id.clone()),
                choice: None,
                options: vec![option(true), option(false)],
            },
        )?;
        if choice
            .decisions
            .get(AS_THOUGH_UNBLOCKED)
            .and_then(Value::as_bool)
            != Some(true)
        {
            return Ok(None);
        }
        let assignments = BTreeMap::from([(AS_THOUGH_UNBLOCKED.to_string(), power.max(0))]);
        self.record_event(
            "combatDamageAssigned",
            Some(attacker.controller.clone()),
            Some(attacker.instance_id.clone()),
            json!({ "assignments": assignments, "asThoughUnblocked": true }),
        );
        Ok(Some(assignments))
    }

    /// The damage of an attacker assigned as though it weren't blocked: all
    /// of it to the player or permanent it attacks (CR 510.1a).
    pub(super) fn as_though_unblocked_damage(
        &self,
        attacker_id: &str,
        defender: &TargetRef,
        power: i32,
    ) -> Option<(String, TargetRef, i32)> {
        if power <= 0 {
            return None;
        }
        match defender {
            TargetRef::Player { player_id } => Some((
                attacker_id.to_string(),
                TargetRef::Player {
                    player_id: player_id.clone(),
                },
                power,
            )),
            TargetRef::Permanent { instance_id }
                if self.permanent_position(instance_id).is_some() =>
            {
                Some((
                    attacker_id.to_string(),
                    TargetRef::Permanent {
                        instance_id: instance_id.clone(),
                    },
                    power,
                ))
            }
            _ => None,
        }
    }
}

impl GameEngine {
    /// CR 704.5j: the permanents the legend rule ignores, by the static
    /// object selector of each active waiver.
    pub(super) fn legend_rule_waived_permanents(&self) -> BTreeSet<String> {
        let waivers = self
            .battlefield_static_modifiers()
            .into_iter()
            .filter(|(_, modifier)| value_kind(modifier) == Some("legendRuleDoesNotApply"))
            .collect::<Vec<_>>();
        if waivers.is_empty() {
            return BTreeSet::new();
        }
        self.state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .filter(|permanent| {
                waivers.iter().any(|(source, modifier)| {
                    Self::static_selector_matches(source, &modifier["objects"], permanent)
                })
            })
            .map(|permanent| permanent.instance_id.clone())
            .collect()
    }

    /// The fewest creatures one declaration may hold for `role` (`attack` or
    /// `block`), or `usize::MAX` without a limit (CR 508.1c, 509.1c).
    pub(super) fn combat_participant_limit(&self, role: &str) -> usize {
        self.battlefield_static_modifiers()
            .iter()
            .filter(|(_, modifier)| {
                value_kind(modifier) == Some("combatParticipantLimit")
                    && modifier["role"].as_str() == Some(role)
            })
            .filter_map(|(_, modifier)| integer_value(&modifier["maximum"]))
            .filter_map(|maximum| usize::try_from(maximum).ok())
            .min()
            .unwrap_or(usize::MAX)
    }

    /// CR 302.6: the controller of an `activateAsThoughHaste` permission may
    /// pay the tap-symbol costs of the selected creatures it controls despite
    /// summoning sickness.
    pub(super) fn activates_as_though_haste(&self, permanent: &CardInstance) -> bool {
        self.battlefield_static_modifiers()
            .iter()
            .any(|(source, modifier)| {
                value_kind(modifier) == Some("activateAsThoughHaste")
                    && source.controller == permanent.controller
                    && Self::static_selector_matches(source, &modifier["objects"], permanent)
            })
    }

    /// CR 104.2a: the last player in the game wins unless a rule says that
    /// player can't win the game.
    pub(super) fn player_can_win(&self, player_id: &str) -> bool {
        !self.player_static_modifier_present(player_id, "cantWinGame")
    }

    /// CR 121.1: a draw of this player is prohibited.
    pub(super) fn player_draw_prohibited(&self, player_id: &str) -> bool {
        self.player_static_modifier_present(player_id, "prohibitDraw")
    }

    /// CR 500.11: the active player skips the untap step about to begin;
    /// none of their permanents untap in it.
    pub(super) fn skip_untap_step_by_static_rule(
        &self,
        active: usize,
        remain_tapped: &mut BTreeSet<String>,
    ) {
        let player = &self.state.players[active];
        if self.player_static_modifier_present(&player.id, "skipUntapStep") {
            remain_tapped.extend(
                player
                    .battlefield
                    .iter()
                    .map(|permanent| permanent.instance_id.clone()),
            );
        }
    }

    /// CR 401.2: the players whose top library card is public.
    pub(super) fn players_with_revealed_library_top(&self) -> BTreeSet<String> {
        self.state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .filter(|player| self.player_static_modifier_present(&player.id, "revealLibraryTop"))
            .map(|player| player.id.clone())
            .collect()
    }
}
