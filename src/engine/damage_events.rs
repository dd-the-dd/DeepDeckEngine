//! Generic event model, damage family (docs/oracle-event-model.md).
//!
//! Each damage a source deals to one recipient is one typed record: source,
//! the source's last-known controller, recipient (player or permanent),
//! amount, combat flag and the simultaneous event it belongs to (CR 120.1,
//! 120.2, 603.2c). A trigger's damage event is a pattern of the same shape;
//! one matcher evaluates every pattern. The historical damage kinds are read
//! as patterns with their established publication point and bindings.
use super::*;

/// Binding of the simultaneous damage event an aggregated trigger observes.
const DAMAGE_EVENT_BATCH: &str = "damageEventBatch";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DamageRecipientRecord {
    Player(String),
    Permanent(String),
}

/// One damage dealt by one source to one recipient.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DamageEventRecord {
    pub source: String,
    /// The source's controller when it dealt the damage (its last-known
    /// information if it is no longer on the battlefield or the stack).
    pub source_controller: Option<String>,
    pub recipient: DamageRecipientRecord,
    pub amount: i32,
    pub combat: bool,
    /// The simultaneous event (one instruction, one combat damage step);
    /// `None` when the damage is its own event.
    pub batch: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DamageSourcePattern<'a> {
    /// The trigger source itself.
    Source,
    /// The permanent the trigger source is attached to.
    AttachedTo,
    /// A battlefield permanent matching the shared damage query
    /// (`controller` relation, `where`, `excludeSource`).
    Permanents(&'a Value),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DamageRecipientPattern<'a> {
    /// Any recipient: the pattern observes the source's whole damage event.
    Any,
    /// A player in the given relation to the trigger source.
    Player(&'a Value),
    /// A permanent matching the shared damage query.
    Permanents(&'a Value),
    /// Any player (the historical combat-damage-to-player kinds).
    AnyPlayer,
    /// The trigger source itself.
    Source,
    /// A permanent its controller controls, with the effective filter.
    ControlledBySourceController(&'a Value),
    /// The permanent the trigger source is attached to (`enchanted creature
    /// is dealt damage`).
    AttachedTo,
}

/// The established publication point of a damage pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DamageGroup {
    /// Every battlefield observer of a dealt-damage record.
    SourceObservers,
    /// Observers of a permanent their controller controls being dealt damage.
    ControlledRecipient,
    /// The damaged permanent's own abilities.
    RecipientSelf,
    /// Observers of combat damage dealt to a player.
    CombatToPlayer,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DamageEventPattern<'a> {
    source: Option<DamageSourcePattern<'a>>,
    recipient: DamageRecipientPattern<'a>,
    /// `Some(true)` combat damage only, `Some(false)` noncombat only.
    combat: Option<bool>,
    /// One trigger per source and simultaneous event, with the total amount
    /// (`whenever SOURCE deals damage`).
    aggregated: bool,
    pub group: DamageGroup,
}

impl DamageEventPattern<'_> {
    /// One trigger per source and simultaneous event (`one or more`).
    pub(super) fn aggregated(&self) -> bool {
        self.aggregated
    }
}

fn damage_kind_flags(event: &Value) -> Option<Option<bool>> {
    let combat_only = match event.get("combatOnly") {
        None => false,
        Some(value) => value.as_bool()?,
    };
    let noncombat_only = match event.get("noncombatOnly") {
        None => false,
        Some(value) => value.as_bool()?,
    };
    match (combat_only, noncombat_only) {
        (true, true) => None,
        (true, false) => Some(Some(true)),
        (false, true) => Some(Some(false)),
        (false, false) => Some(None),
    }
}

fn damage_source_pattern(source: &Value) -> Option<DamageSourcePattern<'_>> {
    match value_kind(source) {
        Some("self") => Some(DamageSourcePattern::Source),
        Some("attachedPermanent") if value_kind(&source["attachment"]) == Some("self") => {
            Some(DamageSourcePattern::AttachedTo)
        }
        Some("permanents")
            if damage_permanent_query_supported(source)
                && source.get("excludeSource").is_none_or(Value::is_boolean) =>
        {
            Some(DamageSourcePattern::Permanents(source))
        }
        _ => None,
    }
}

fn damage_permanent_query_supported(query: &Value) -> bool {
    query
        .get("where")
        .is_none_or(event_model::event_subject_filter_supported)
        && query
            .get("controller")
            .is_none_or(player_action_events::event_player_reference_supported)
}

fn damage_recipient_pattern(recipient: &Value) -> Option<DamageRecipientPattern<'_>> {
    match value_kind(recipient) {
        Some("player")
            if player_action_events::event_player_reference_supported(&recipient["player"]) =>
        {
            Some(DamageRecipientPattern::Player(&recipient["player"]))
        }
        Some("permanent") if damage_permanent_query_supported(recipient) => {
            Some(DamageRecipientPattern::Permanents(recipient))
        }
        Some("attachedPermanent") if value_kind(&recipient["attachment"]) == Some("self") => {
            Some(DamageRecipientPattern::AttachedTo)
        }
        _ => None,
    }
}

fn is_source_controller(player: &Value) -> bool {
    value_kind(player) == Some("controllerOf")
        && matches!(
            value_kind(&player["object"]),
            Some("self" | "abilitySource")
        )
}

/// Reads a historical damage kind or a generic `dealDamage` pattern.
/// `None` for another family; `Some(Err(()))` when an attribute cannot be
/// evaluated by the matcher (the rule is unsupported, never widened).
pub(super) fn damage_event_pattern(event: &Value) -> Option<Result<DamageEventPattern<'_>, ()>> {
    let pattern = match value_kind(event)? {
        "damageDealtBySource" => (|| {
            Some(DamageEventPattern {
                source: Some(damage_source_pattern(&event["source"])?),
                recipient: damage_recipient_pattern(&event["recipient"])?,
                combat: damage_kind_flags(event)?,
                aggregated: false,
                group: DamageGroup::SourceObservers,
            })
        })(),
        "eventPattern" if event["lemma"].as_str() == Some("dealDamage") => (|| {
            let known = event.as_object()?.keys().all(|key| {
                matches!(
                    key.as_str(),
                    "kind"
                        | "lemma"
                        | "source"
                        | "recipient"
                        | "combatOnly"
                        | "noncombatOnly"
                        | "aggregation"
                )
            });
            if !known {
                return None;
            }
            // `deals damage to one or more PERMANENTS`: one trigger per
            // source and simultaneous event for a written recipient too.
            let batch = match event.get("aggregation") {
                None => false,
                Some(aggregation) if aggregation.as_str() == Some("oneOrMore") => true,
                Some(_) => return None,
            };
            let recipient = match event.get("recipient") {
                None => DamageRecipientPattern::Any,
                Some(recipient) => damage_recipient_pattern(recipient)?,
            };
            // A received-damage pattern may omit the source: damage from
            // any source to the recipient (CR 120.3). A whole-event pattern
            // always names its source.
            let source = match event.get("source") {
                None if recipient != DamageRecipientPattern::Any => None,
                None => return None,
                Some(source) => Some(damage_source_pattern(source)?),
            };
            // A batch names its source: one event of one source.
            if batch && source.is_none() {
                return None;
            }
            Some(DamageEventPattern {
                source,
                aggregated: batch || recipient == DamageRecipientPattern::Any,
                recipient,
                combat: damage_kind_flags(event)?,
                group: DamageGroup::SourceObservers,
            })
        })(),
        "controlledPermanentDealtDamage" => (|| {
            (is_source_controller(&event["player"]) && card_filter_supported(&event["where"]))
                .then_some(())?;
            Some(DamageEventPattern {
                source: None,
                recipient: DamageRecipientPattern::ControlledBySourceController(&event["where"]),
                combat: damage_kind_flags(event)?,
                aggregated: false,
                group: DamageGroup::ControlledRecipient,
            })
        })(),
        "permanentDealtDamage" => (|| {
            (value_kind(&event["object"]) == Some("self")).then_some(())?;
            Some(DamageEventPattern {
                source: None,
                recipient: DamageRecipientPattern::Source,
                combat: damage_kind_flags(event)?,
                aggregated: false,
                group: DamageGroup::RecipientSelf,
            })
        })(),
        "combatDamageToPlayer" | "attachedPermanentCombatDamageToPlayer" => (|| {
            let source = if value_kind(event) == Some("combatDamageToPlayer") {
                matches!(value_kind(&event["source"]), Some("self" | "abilitySource"))
                    .then_some(DamageSourcePattern::Source)?
            } else {
                DamageSourcePattern::AttachedTo
            };
            Some(DamageEventPattern {
                source: Some(source),
                recipient: DamageRecipientPattern::AnyPlayer,
                combat: Some(true),
                aggregated: false,
                group: DamageGroup::CombatToPlayer,
            })
        })(),
        _ => return None,
    };
    Some(pattern.ok_or(()))
}

pub(super) fn damage_event_supported(event: &Value) -> Option<bool> {
    damage_event_pattern(event).map(|pattern| pattern.is_ok())
}

pub(super) fn rule_damage_patterns(rule: &Value) -> Vec<DamageEventPattern<'_>> {
    triggered_event_variants(&rule["event"])
        .into_iter()
        .filter_map(|event| damage_event_pattern(event).and_then(Result::ok))
        .collect()
}

impl GameEngine {
    fn damage_query_matches(
        &self,
        query: &Value,
        observer: &CardInstance,
        permanent: &CardInstance,
    ) -> bool {
        query.get("controller").is_none_or(|controller| {
            self.event_player_reference_matches(controller, observer, &permanent.controller)
        }) && query
            .get("where")
            .is_none_or(|filter| self.permanent_matches_effective_filter(permanent, filter))
    }

    fn damage_battlefield_object(&self, instance_id: &str) -> Option<&CardInstance> {
        self.permanent_position(instance_id)
            .map(|(player_index, card_index)| {
                &self.state.players[player_index].battlefield[card_index]
            })
    }

    /// The one matcher of the damage family.
    pub(super) fn damage_event_matches(
        &self,
        pattern: &DamageEventPattern<'_>,
        record: &DamageEventRecord,
        observer: &CardInstance,
    ) -> bool {
        if record.amount <= 0 || pattern.combat.is_some_and(|combat| combat != record.combat) {
            return false;
        }
        let source_matches = match pattern.source {
            None => true,
            Some(DamageSourcePattern::Source) => observer.instance_id == record.source,
            Some(DamageSourcePattern::AttachedTo) => {
                observer.attached_to.as_deref() == Some(record.source.as_str())
            }
            Some(DamageSourcePattern::Permanents(query)) => {
                !(query["excludeSource"].as_bool() == Some(true)
                    && observer.instance_id == record.source)
                    && self
                        .damage_battlefield_object(&record.source)
                        .is_some_and(|source| self.damage_query_matches(query, observer, source))
            }
        };
        source_matches
            && match (pattern.recipient, &record.recipient) {
                (DamageRecipientPattern::Any, _) => true,
                (DamageRecipientPattern::AnyPlayer, DamageRecipientRecord::Player(_)) => true,
                (DamageRecipientPattern::Player(relation), DamageRecipientRecord::Player(id)) => {
                    self.event_player_reference_matches(relation, observer, id)
                }
                (
                    DamageRecipientPattern::Permanents(query),
                    DamageRecipientRecord::Permanent(id),
                ) => self
                    .damage_battlefield_object(id)
                    .is_some_and(|permanent| self.damage_query_matches(query, observer, permanent)),
                (DamageRecipientPattern::Source, DamageRecipientRecord::Permanent(id)) => {
                    observer.instance_id == *id
                }
                (DamageRecipientPattern::AttachedTo, DamageRecipientRecord::Permanent(id)) => {
                    observer.attached_to.as_deref() == Some(id.as_str())
                }
                (
                    DamageRecipientPattern::ControlledBySourceController(filter),
                    DamageRecipientRecord::Permanent(id),
                ) => self.damage_battlefield_object(id).is_some_and(|damaged| {
                    damaged.controller == observer.controller
                        && self.permanent_matches_effective_filter(damaged, filter)
                }),
                _ => false,
            }
    }

    /// Whether some damage pattern of `rule` in `group` matches `record`;
    /// returns whether the matching pattern aggregates the event.
    pub(super) fn rule_matches_damage_record(
        &self,
        rule: &Value,
        group: DamageGroup,
        record: &DamageEventRecord,
        observer: &CardInstance,
    ) -> Option<bool> {
        rule_damage_patterns(rule)
            .iter()
            .filter(|pattern| pattern.group == group)
            .find(|pattern| self.damage_event_matches(pattern, record, observer))
            .map(|pattern| pattern.aggregated)
    }

    /// The current simultaneous event (instruction, combat damage step).
    pub(super) fn current_damage_batch(&self) -> Option<u64> {
        self.event_batches.current_batch()
    }

    /// An aggregated trigger already published for this source in this
    /// event: its amount grows by the new damage instead of triggering again.
    pub(super) fn merge_aggregated_damage_trigger(
        &mut self,
        observer: &CardInstance,
        rule: &Value,
        record: &DamageEventRecord,
    ) -> bool {
        let Some(batch) = record.batch else {
            return false;
        };
        // Every copy published for this event (trigger multipliers, CR
        // 603.2d) observes the same event and sees the same total.
        let mut merged = false;
        for pending in self.state.stack.iter_mut().filter(|object| {
            object.card.instance_id == observer.instance_id
                && object.ability_rule.as_ref() == Some(rule)
                && object
                    .decisions
                    .get(DAMAGE_EVENT_BATCH)
                    .and_then(Value::as_u64)
                    == Some(batch)
                && object
                    .decisions
                    .get("damageSourceId")
                    .and_then(Value::as_str)
                    == Some(record.source.as_str())
        }) {
            let total = pending
                .decisions
                .get("damageAmount")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                + i64::from(record.amount);
            pending
                .decisions
                .insert("damageAmount".to_string(), Value::from(total));
            merged = true;
        }
        merged
    }

    /// Publishes one dealt-damage record to every matching battlefield
    /// observer (`damageDealtBySource` and generic `dealDamage` patterns).
    pub(super) fn publish_damage_source_observers(&mut self, record: &DamageEventRecord) {
        if record.amount <= 0 {
            return;
        }
        let triggers = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| player.battlefield.iter())
            .flat_map(|observer| {
                self.effective_rules(&observer)
                    .iter()
                    .filter(|rule| {
                        triggered_rule_supported(rule)
                            && self.rule_active_for_source(observer, rule)
                    })
                    .filter_map(|rule| {
                        self.rule_matches_damage_record(
                            rule,
                            DamageGroup::SourceObservers,
                            record,
                            observer,
                        )
                        .map(|aggregated| (observer.clone(), rule.clone(), aggregated))
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        if triggers.is_empty() {
            return;
        }
        // `that player` names a damaged player; `that creature` names a
        // damaged permanent, otherwise the damage source (as for the
        // established combat-damage observers).
        let mut decisions = BTreeMap::from([
            ("damageAmount".to_string(), Value::from(record.amount)),
            (
                "damageSourceId".to_string(),
                Value::String(record.source.clone()),
            ),
        ]);
        match &record.recipient {
            DamageRecipientRecord::Player(player_id) => {
                decisions.insert(
                    "triggeringPlayerId".to_string(),
                    Value::String(player_id.clone()),
                );
                decisions.insert(
                    "triggeringPermanentId".to_string(),
                    Value::String(record.source.clone()),
                );
            }
            DamageRecipientRecord::Permanent(instance_id) => {
                decisions.insert(
                    "triggeringPermanentId".to_string(),
                    Value::String(instance_id.clone()),
                );
            }
        }
        for (observer, rule, aggregated) in triggers {
            let decisions = if aggregated {
                if self.merge_aggregated_damage_trigger(&observer, &rule, record) {
                    continue;
                }
                // The whole event has no single recipient: only the source
                // and the (growing) amount are bound.
                let mut decisions = BTreeMap::from([
                    ("damageAmount".to_string(), Value::from(record.amount)),
                    (
                        "damageSourceId".to_string(),
                        Value::String(record.source.clone()),
                    ),
                    (
                        "triggeringPermanentId".to_string(),
                        Value::String(record.source.clone()),
                    ),
                ]);
                if let Some(batch) = record.batch {
                    decisions.insert(DAMAGE_EVENT_BATCH.to_string(), Value::from(batch));
                }
                decisions
            } else {
                decisions.clone()
            };
            self.put_triggered_ability_on_stack_with_decisions(&observer, rule, decisions);
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }
}
