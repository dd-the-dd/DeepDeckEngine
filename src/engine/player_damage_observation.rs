//! Shared damage event matching with explicit per-recipient or one-or-more scope.
use super::player_damage_batch::PlayerDamageRecord;
use super::*;

impl GameEngine {
    pub(super) fn require_player_damage_batch_observers(&self) -> Result<(), EngineError> {
        for source in self.state.players.iter().flat_map(|p| p.battlefield.iter()) {
            for rule in self.effective_rules(&source).iter() {
                for event in triggered_event_variants(&rule["event"]) {
                    if matches!(
                        value_kind(event),
                        Some("damageDealtToOpponent" | "opponentDealtDamage" | "playerDealtDamage")
                    ) && event
                        .get("aggregation")
                        .is_some_and(|v| !matches!(v.as_str(), Some("perRecipient" | "oneOrMore")))
                    {
                        return Err(EngineError::new("unsupported damage observer aggregation"));
                    }
                }
                // An event union with conflicting quantifier scope cannot be
                // safely collapsed into one of its branches' interpretations.
                let scopes = triggered_event_variants(&rule["event"])
                    .into_iter()
                    .filter(|e| {
                        matches!(
                            value_kind(e),
                            Some(
                                "damageDealtToOpponent"
                                    | "opponentDealtDamage"
                                    | "playerDealtDamage"
                            )
                        )
                    })
                    .map(|e| e["aggregation"].as_str().unwrap_or("perRecipient"))
                    .collect::<BTreeSet<_>>();
                if scopes.len() > 1 {
                    return Err(EngineError::new(
                        "mixed damage observer scopes require typed union semantics",
                    ));
                }
            }
        }
        Ok(())
    }
    pub(super) fn enqueue_player_damage_observers(
        &mut self,
        controller: &str,
        source_id: &str,
        combat: bool,
        records: &[PlayerDamageRecord],
        batch: Option<usize>,
    ) {
        let sources = self
            .state
            .players
            .iter()
            .filter(|p| !p.has_lost)
            .flat_map(|p| p.battlefield.iter().cloned())
            .collect::<Vec<_>>();
        for source in sources {
            for rule in self
                .effective_rules(&source)
                .iter()
                .filter(|r| triggered_rule_supported(r))
                .cloned()
                .collect::<Vec<_>>()
            {
                let variants = triggered_event_variants(&rule["event"]);
                let matches = records
                    .iter()
                    .filter(|r| {
                        r.amount > 0
                            && variants.iter().any(|event| match value_kind(event) {
                                Some("damageDealtToOpponent") => {
                                    source.controller == controller && controller != r.player_id
                                }
                                Some("opponentDealtDamage") => {
                                    source.controller != r.player_id
                                        && (!combat
                                            || event["noncombatOnly"].as_bool() != Some(true))
                                        && (combat || event["combatOnly"].as_bool() != Some(true))
                                }
                                Some("playerDealtDamage") => self.received_damage_observed(
                                    event,
                                    &source,
                                    &r.player_id,
                                    combat,
                                ),
                                _ => false,
                            })
                    })
                    .collect::<Vec<_>>();
                if matches.is_empty() {
                    continue;
                }
                let one_or_more = variants.iter().any(|event| {
                    matches!(
                        value_kind(event),
                        Some("damageDealtToOpponent" | "opponentDealtDamage" | "playerDealtDamage")
                    ) && event["aggregation"].as_str() == Some("oneOrMore")
                });
                let groups = if one_or_more {
                    vec![matches]
                } else {
                    matches.into_iter().map(|r| vec![r]).collect()
                };
                for group in groups {
                    // Checked sums were bounded during preparation. A legacy
                    // scalar caller has one record; no sum overflow is possible.
                    let amount = group.iter().map(|r| i64::from(r.amount)).sum::<i64>();
                    let mut decisions = BTreeMap::from([
                        ("damageAmount".into(), json!(amount)),
                        ("damageSourceId".into(), json!(source_id)),
                    ]);
                    if !one_or_more || group.len() == 1 {
                        decisions.insert("triggeringPlayerId".into(), json!(group[0].player_id));
                    }
                    if let Some(batch) = batch {
                        decisions.insert("damageBatchId".into(), json!(batch));
                        decisions.insert(
                            "damagedPlayerIds".into(),
                            json!(group.iter().map(|r| &r.player_id).collect::<Vec<_>>()),
                        );
                    }
                    self.put_triggered_ability_on_stack_with_decisions(
                        &source,
                        rule.clone(),
                        decisions,
                    );
                    if self.check_quantity_failure().is_err() {
                        return;
                    }
                }
            }
        }
    }
}

impl GameEngine {
    pub(super) fn enqueue_player_damage_extra_observers(
        &mut self,
        source_controller: &str,
        player_id: &str,
        source: &str,
    ) {
        let grants_draw =
            self.battlefield_static_modifiers()
                .iter()
                .any(|(attachment, modifier)| {
                    value_kind(modifier) == Some("attachedDamageDraw")
                        && attachment.attached_to.as_deref() == Some(source)
                });
        if grants_draw
            && source_controller != player_id
            && let Some((source_player, source_index)) = self.permanent_position(source)
        {
            let trigger_source =
                self.state.players[source_player].battlefield[source_index].clone();
            self.put_triggered_ability_on_stack_with_origin(
                &trigger_source,
                json!({
                    "kind": "triggeredAbility",
                    "source": { "kind": "self" },
                    "event": {
                        "kind": "damageDealtToOpponent",
                        "source": { "kind": "self" },
                    },
                    "effects": [{
                        "kind": "optionalAction",
                        "player": {
                            "kind": "controllerOf",
                            "object": { "kind": "self" },
                        },
                        "action": {
                            "kind": "drawCards",
                            "player": {
                                "kind": "controllerOf",
                                "object": { "kind": "self" },
                            },
                            "count": { "kind": "integer", "value": 1 },
                        },
                        "onPerformed": [],
                    }],
                }),
                BTreeMap::new(),
                trigger_multiplicity::TriggerOrigin::Unproven,
            );
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }
}
