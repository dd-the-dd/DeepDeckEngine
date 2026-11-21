//! Generic event model, zone-change family (docs/oracle-event-model.md).
//!
//! One move of one object is one record: the object as it last existed in
//! the zone it left (last-known information, CR 603.10a), the zone it left
//! when known, the zone it entered, and the cause the engine knows (`died`
//! reasons). A trigger's zone-change event is a pattern of the same shape:
//! the subject's relation to the trigger source, an object predicate read
//! on the snapshot, origin and destination zones, and typed modifiers.
//! Leaves-the-battlefield patterns are matched by the sources captured
//! before the move (look-back); the moved card's own `from anywhere`
//! abilities are matched where it arrives.
use super::*;

/// One object moving between zones.
pub(super) struct ZoneChangeRecord<'a> {
    /// The object before it moved (its last-known information).
    pub object: &'a CardInstance,
    /// `None` when the publication point does not know the origin zone.
    pub from: Option<&'a str>,
    pub to: &'a str,
    /// The cause of a death (`lethalDamage`, `destroyed`, `sacrificed`, …).
    pub reason: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ControllerRelation<'a> {
    Any,
    SourceController,
    OpponentOfSourceController,
    /// A player reference evaluated against the snapshot's controller.
    Reference(&'a Value),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SubjectRelation<'a> {
    Source,
    AttachedTo,
    Permanents {
        controller: ControllerRelation<'a>,
        exclude_source: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObjectView {
    /// The printed definition (the historical `permanentDied` filter).
    Printed,
    /// Effective characteristics evaluated on the snapshot.
    Effective,
}

/// The established publication point of a zone-change pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ZoneChangeGroup {
    /// The state-based or effect death of a permanent.
    Death,
    /// Any departure from the battlefield, with captured observers.
    Departure,
    /// A card arriving in a graveyard: its own abilities.
    GraveyardArrival,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ZoneChangePattern<'a> {
    subject: SubjectRelation<'a>,
    filter: Option<(&'a Value, ObjectView)>,
    from: Option<&'a str>,
    to: Option<&'a str>,
    /// `leaves the battlefield` without dying.
    without_dying: bool,
    nontoken: bool,
    reason: Option<&'a str>,
    had_counter: Option<&'a str>,
    had_any_counters: bool,
    damaged_by_source_this_turn: bool,
    pub group: ZoneChangeGroup,
}

impl<'a> ZoneChangePattern<'a> {
    fn new(subject: SubjectRelation<'a>, group: ZoneChangeGroup) -> Self {
        Self {
            subject,
            filter: None,
            from: Some("battlefield"),
            to: None,
            without_dying: false,
            nontoken: false,
            reason: None,
            had_counter: None,
            had_any_counters: false,
            damaged_by_source_this_turn: false,
            group,
        }
    }
}

fn known_fields(event: &Value, fields: &[&str]) -> bool {
    event
        .as_object()
        .is_some_and(|object| object.keys().all(|key| fields.contains(&key.as_str())))
}

fn optional_bool(event: &Value, field: &str) -> Option<bool> {
    match event.get(field) {
        None | Some(Value::Null) => Some(false),
        Some(value) => value.as_bool(),
    }
}

fn optional_str<'a>(event: &'a Value, field: &str) -> Option<Option<&'a str>> {
    match event.get(field) {
        None | Some(Value::Null) => Some(None),
        Some(value) => value.as_str().map(Some),
    }
}

fn is_source_controller(player: &Value) -> bool {
    value_kind(player) == Some("controllerOf")
        && matches!(
            value_kind(&player["object"]),
            Some("self" | "abilitySource")
        )
}

fn self_object(value: &Value) -> bool {
    matches!(value_kind(value), Some("self" | "abilitySource"))
}

fn died_pattern(event: &Value) -> Option<ZoneChangePattern<'_>> {
    if !known_fields(
        event,
        &[
            "kind",
            "object",
            "where",
            "player",
            "excludeSource",
            "nontoken",
            "damagedBySourceThisTurn",
            "hadCounter",
            "hadAnyCounters",
            "reason",
            "aggregation",
            "entryState",
        ],
    ) || !event_batch::event_aggregation_supported(event)
    {
        return None;
    }
    let subject = if event.get("object").is_some() {
        // The source itself never aggregates.
        (value_kind(&event["object"]) == Some("self") && !event_batch::event_is_aggregated(event))
            .then_some(SubjectRelation::Source)?
    } else {
        card_filter_supported(&event["where"]).then_some(())?;
        let controller = match value_kind(&event["player"]) {
            None if event["player"].is_null() => ControllerRelation::Any,
            Some("controllerOf") if player_reference_supported(&event["player"]) => {
                ControllerRelation::SourceController
            }
            Some("opponentsOf") if player_reference_supported(&event["player"]) => {
                ControllerRelation::OpponentOfSourceController
            }
            _ => return None,
        };
        SubjectRelation::Permanents {
            controller,
            exclude_source: optional_bool(event, "excludeSource")?,
        }
    };
    let filter = matches!(subject, SubjectRelation::Permanents { .. })
        .then_some((&event["where"], ObjectView::Printed));
    Some(ZoneChangePattern {
        filter,
        to: Some("graveyard"),
        nontoken: optional_bool(event, "nontoken")?,
        reason: optional_str(event, "reason")?,
        had_counter: optional_str(event, "hadCounter")?,
        had_any_counters: optional_bool(event, "hadAnyCounters")?,
        damaged_by_source_this_turn: optional_bool(event, "damagedBySourceThisTurn")?,
        ..ZoneChangePattern::new(subject, ZoneChangeGroup::Death)
    })
}

fn generic_move_pattern(event: &Value) -> Option<ZoneChangePattern<'_>> {
    if !known_fields(event, &["kind", "lemma", "subject", "from", "to"]) {
        return None;
    }
    let object = &event["subject"]["object"];
    if !known_fields(&event["subject"], &["object"]) {
        return None;
    }
    let from = optional_str(event, "from")?;
    let to = optional_str(event, "to")?;
    let (subject, filter) = match value_kind(object)? {
        "self" => (SubjectRelation::Source, None),
        "permanents" => {
            if !known_fields(object, &["kind", "controller", "where", "excludeSource"]) {
                return None;
            }
            let controller = match object.get("controller") {
                None => ControllerRelation::Any,
                Some(player) if player_action_events::event_player_reference_supported(player) => {
                    ControllerRelation::Reference(player)
                }
                Some(_) => return None,
            };
            let filter = match object.get("where") {
                None => None,
                Some(filter) => Some((
                    card_filter_supported(filter).then_some(filter)?,
                    ObjectView::Effective,
                )),
            };
            (
                SubjectRelation::Permanents {
                    controller,
                    exclude_source: optional_bool(object, "excludeSource")?,
                },
                filter,
            )
        }
        _ => return None,
    };
    let group = match (from, to, subject) {
        // Look-back departures, observed by the sources captured before it.
        (Some("battlefield"), _, _) => ZoneChangeGroup::Departure,
        // A card's own ability, wherever it comes from, as it arrives.
        (None, Some("graveyard"), SubjectRelation::Source) => ZoneChangeGroup::GraveyardArrival,
        _ => return None,
    };
    Some(ZoneChangePattern {
        filter,
        from,
        to,
        ..ZoneChangePattern::new(subject, group)
    })
}

/// Reads a historical zone-change kind or a generic `move` pattern.
/// `None` for another family; `Some(Err(()))` when an attribute cannot be
/// evaluated by the matcher (the rule is unsupported, never widened).
pub(super) fn zone_change_pattern(event: &Value) -> Option<Result<ZoneChangePattern<'_>, ()>> {
    let pattern = match value_kind(event)? {
        "permanentDied" => died_pattern(event),
        "attachedPermanentDied" => (known_fields(event, &["kind", "object"])
            && event.get("object").is_none_or(self_object))
        .then(|| ZoneChangePattern {
            to: Some("graveyard"),
            ..ZoneChangePattern::new(SubjectRelation::AttachedTo, ZoneChangeGroup::Death)
        }),
        "permanentLeftBattlefield" => (|| {
            known_fields(
                event,
                &["kind", "object", "where", "destination", "withoutDying"],
            )
            .then_some(())?;
            let (subject, filter) = match event.get("object") {
                Some(object) => (
                    self_object(object).then_some(SubjectRelation::Source)?,
                    None,
                ),
                None => (
                    SubjectRelation::Permanents {
                        controller: ControllerRelation::SourceController,
                        exclude_source: false,
                    },
                    Some((
                        card_filter_supported(&event["where"]).then_some(&event["where"])?,
                        ObjectView::Effective,
                    )),
                ),
            };
            Some(ZoneChangePattern {
                filter,
                to: optional_str(event, "destination")?,
                without_dying: optional_bool(event, "withoutDying")?,
                ..ZoneChangePattern::new(subject, ZoneChangeGroup::Departure)
            })
        })(),
        "controlledPermanentLeftBattlefield" => (|| {
            (known_fields(
                event,
                &["kind", "player", "where", "excludeSource", "destination"],
            ) && is_source_controller(&event["player"])
                && card_filter_supported(&event["where"]))
            .then_some(())?;
            Some(ZoneChangePattern {
                filter: Some((&event["where"], ObjectView::Effective)),
                to: optional_str(event, "destination")?,
                ..ZoneChangePattern::new(
                    SubjectRelation::Permanents {
                        controller: ControllerRelation::SourceController,
                        exclude_source: event["excludeSource"].as_bool()?,
                    },
                    ZoneChangeGroup::Departure,
                )
            })
        })(),
        "eventPattern" if event["lemma"].as_str() == Some("move") => generic_move_pattern(event),
        _ => return None,
    };
    Some(pattern.ok_or(()))
}

pub(super) fn zone_change_event_supported(event: &Value) -> Option<bool> {
    zone_change_pattern(event).map(|pattern| pattern.is_ok())
}

pub(super) fn rule_zone_change_patterns(rule: &Value) -> Vec<ZoneChangePattern<'_>> {
    triggered_event_variants(&rule["event"])
        .into_iter()
        .filter_map(|event| zone_change_pattern(event).and_then(Result::ok))
        .collect()
}

pub(super) fn rule_observes_zone_change(rule: &Value, group: ZoneChangeGroup) -> bool {
    rule_zone_change_patterns(rule)
        .iter()
        .any(|pattern| pattern.group == group)
}

impl GameEngine {
    /// The one matcher of the zone-change family.
    pub(super) fn zone_change_matches(
        &self,
        pattern: &ZoneChangePattern<'_>,
        record: &ZoneChangeRecord<'_>,
        observer: &CardInstance,
    ) -> bool {
        let object = record.object;
        if pattern.from.is_some_and(|from| record.from != Some(from))
            || pattern.to.is_some_and(|to| to != record.to)
            || (pattern.without_dying && record.to == "graveyard")
            || (pattern.nontoken && object.definition.is_token)
            || pattern
                .reason
                .is_some_and(|expected| record.reason != Some(expected))
        {
            return false;
        }
        let related = match pattern.subject {
            SubjectRelation::Source => observer.instance_id == object.instance_id,
            SubjectRelation::AttachedTo => {
                observer.attached_to.as_deref() == Some(object.instance_id.as_str())
            }
            SubjectRelation::Permanents {
                controller,
                exclude_source,
            } => {
                !(exclude_source && observer.instance_id == object.instance_id)
                    && match controller {
                        ControllerRelation::Any => true,
                        ControllerRelation::SourceController => {
                            object.controller == observer.controller
                        }
                        ControllerRelation::OpponentOfSourceController => {
                            object.controller != observer.controller
                        }
                        ControllerRelation::Reference(player) => self
                            .event_player_reference_matches(player, observer, &object.controller),
                    }
            }
        };
        related
            && pattern.filter.is_none_or(|(filter, view)| match view {
                ObjectView::Printed => matches_card_filter(&object.definition, filter),
                ObjectView::Effective => self.permanent_matches_effective_filter(object, filter),
            })
            && pattern
                .had_counter
                .is_none_or(|counter| object.counters.get(counter).copied().unwrap_or(0) > 0)
            && (!pattern.had_any_counters || object.counters.values().copied().sum::<i32>() > 0)
            && (!pattern.damaged_by_source_this_turn
                || self.state.events.iter().any(|damage| {
                    damage.turn_number == self.state.turn_number
                        && damage.kind == "damageDealt"
                        && damage.card_instance_id.as_deref() == Some(object.instance_id.as_str())
                        && damage.detail["source"].as_str() == Some(observer.instance_id.as_str())
                        && damage.detail["amount"]
                            .as_i64()
                            .is_some_and(|amount| amount > 0)
                }))
    }

    pub(super) fn rule_matches_zone_change(
        &self,
        rule: &Value,
        group: ZoneChangeGroup,
        record: &ZoneChangeRecord<'_>,
        observer: &CardInstance,
    ) -> bool {
        rule_zone_change_patterns(rule).iter().any(|pattern| {
            pattern.group == group && self.zone_change_matches(pattern, record, observer)
        })
    }
}
