//! Player-designation, turn and zone atoms of the condition language.
//!
//! - `playerIsMonarch { player }` (CR 724.1): the player is the monarch.
//! - `playerWasStartingPlayer { player }` (CR 103.1): the player took the
//!   game's first turn.
//! - `isPlayersTurn { player }` (CR 500.1): the player is the active player.
//! - `objectInZone { object, zones }` (CR 400.1): the referenced object is in
//!   one of the zones.
//!
//! The continuous evaluator knows its reference player and the source only:
//! a player atom there names the reference player (`controllerOf self`,
//! which a `playerQuantifier` member is bound to), and a zone atom names the
//! source. At resolution every player and object reference the shared
//! resolvers know is read; an unresolved one makes the atom false, as for
//! every other atom of the infallible evaluator.
use super::*;

const PLAYER_ATOMS: &[&str] = &[
    "playerIsMonarch",
    "playerWasStartingPlayer",
    "isPlayersTurn",
];
const OBJECT_IN_ZONE: &str = "objectInZone";

/// The engine zone names an `objectInZone` atom may list.
const ZONES: &[&str] = &[
    "battlefield",
    "graveyard",
    "exile",
    "hand",
    "library",
    "stack",
    "commandZone",
];

/// Whether this module evaluates conditions of `kind`.
pub(super) fn owns(kind: &str) -> bool {
    kind == OBJECT_IN_ZONE || PLAYER_ATOMS.contains(&kind)
}

pub(super) fn supported(condition: &Value) -> bool {
    match value_kind(condition) {
        Some(kind) if PLAYER_ATOMS.contains(&kind) => {
            singular_player_reference_supported(&condition["player"])
        }
        Some(OBJECT_IN_ZONE) => {
            object_condition::object_condition_reference_supported(&condition["object"])
                && condition["zones"].as_array().is_some_and(|zones| {
                    !zones.is_empty()
                        && zones
                            .iter()
                            .all(|zone| zone.as_str().is_some_and(|zone| ZONES.contains(&zone)))
                })
        }
        _ => false,
    }
}

/// Whether `zones` (an `objectInZone` list) names `zone`.
fn zones_contain(zones: &Value, zone: &str) -> bool {
    zones.as_array().is_some_and(|zones| {
        zones
            .iter()
            .any(|candidate| candidate.as_str() == Some(zone))
    })
}

/// The permanent state a departure record keeps beside its type line (CR
/// 608.2h): its counters, tapped and combat status and token status as it
/// last existed on the battlefield.
pub(super) fn departure_state_record(permanent: &CardInstance) -> Value {
    let flag = |name: &str| permanent.flags.get(name).copied().unwrap_or(false);
    json!({
        "counters": permanent.counters,
        "tapped": permanent.tapped,
        "attacking": flag("attacking"),
        "blocking": flag("blocking"),
        "token": permanent.definition.is_token,
    })
}

/// The permanent-state atoms a departure record answers.
pub(super) fn departure_record_atom(kind: &str) -> bool {
    matches!(
        kind,
        "hasCounter" | "isTapped" | "isAttacking" | "isBlocking" | "isToken"
    )
}

pub(super) fn departure_record_atom_supported(filter: &Value) -> bool {
    match value_kind(filter) {
        Some("hasCounter") => {
            filter.get("counter").is_none_or(Value::is_string)
                && filter
                    .get("minimum")
                    .is_none_or(|value| integer_value(value).is_some())
                && filter
                    .get("maximum")
                    .is_none_or(|value| integer_value(value).is_some())
        }
        Some(kind) => departure_record_atom(kind),
        None => false,
    }
}

/// A predicate over a permanent that left the battlefield, read from its
/// departure record: the recorded type line for type-line atoms, and the
/// recorded last known state for counters, tapped and combat status and
/// token status. A record without that state answers nothing (`None`), so
/// the condition is false rather than read on the card in its new zone.
pub(super) fn departure_record_matches(
    detail: &Value,
    filter: &Value,
    depth: usize,
) -> Option<bool> {
    if depth >= 32 {
        return None;
    }
    let state = &detail["lastKnownState"];
    match value_kind(filter) {
        Some("not") => {
            departure_record_matches(detail, &filter["operand"], depth + 1).map(|value| !value)
        }
        Some("and") => {
            let mut all = true;
            for operand in filter["operands"].as_array()? {
                all &= departure_record_matches(detail, operand, depth + 1)?;
            }
            Some(all)
        }
        Some("or") => {
            let mut any = false;
            for operand in filter["operands"].as_array()? {
                any |= departure_record_matches(detail, operand, depth + 1)?;
            }
            Some(any)
        }
        Some("hasCounter") => {
            let counters: BTreeMap<String, i32> =
                serde_json::from_value(state.get("counters")?.clone()).ok()?;
            Some(counter_presence_matches(&counters, filter))
        }
        Some("isTapped") => state.get("tapped")?.as_bool(),
        Some("isAttacking") => state.get("attacking")?.as_bool(),
        Some("isBlocking") => state.get("blocking")?.as_bool(),
        _ => event_history::type_line_filter(
            detail["typeLine"].as_str()?,
            detail["wasCreature"].as_bool(),
            state.get("token").and_then(Value::as_bool),
            filter,
        ),
    }
}

impl GameEngine {
    /// The zone an object is in now, by its instance: the battlefield, the
    /// stack, or the card zone that holds it.
    fn current_zone_of_instance(&self, instance_id: &str) -> Option<&'static str> {
        if self.permanent_position(instance_id).is_some() {
            return Some("battlefield");
        }
        if self
            .state
            .stack
            .iter()
            .any(|object| object.card.instance_id == instance_id)
        {
            return Some("stack");
        }
        self.card_outside_battlefield_zone(instance_id)
    }

    fn instance_in_zones(&self, instance_id: &str, zones: &Value) -> bool {
        self.current_zone_of_instance(instance_id)
            .is_some_and(|zone| zones_contain(zones, zone))
    }

    fn player_state_atom_holds(&self, kind: &str, player_id: &str) -> bool {
        match kind {
            "playerIsMonarch" => self.state.rule_modifiers.iter().any(|modifier| {
                value_kind(modifier) == Some("monarch")
                    && modifier["playerId"].as_str() == Some(player_id)
            }),
            "playerWasStartingPlayer" => self
                .state
                .players
                .get(self.starting_player)
                .is_some_and(|player| player.id == player_id),
            "isPlayersTurn" => self
                .state
                .players
                .get(self.state.active_player)
                .is_some_and(|player| player.id == player_id),
            _ => false,
        }
    }

    /// The continuous evaluation: the reference player and the source.
    pub(super) fn state_condition_atom_with_source(
        &self,
        condition: &Value,
        controller_id: &str,
        source: Option<&CardInstance>,
    ) -> bool {
        match value_kind(condition) {
            Some(OBJECT_IN_ZONE) => {
                matches!(
                    value_kind(&condition["object"]),
                    Some("self" | "abilitySource")
                ) && source.is_some_and(|source| {
                    self.instance_in_zones(&source.instance_id, &condition["zones"])
                })
            }
            Some(kind) => {
                let player = &condition["player"];
                let reference_player = value_kind(player) == Some("abilityController")
                    || (value_kind(player) == Some("controllerOf")
                        && matches!(
                            value_kind(&player["object"]),
                            Some("self" | "abilitySource")
                        ));
                reference_player && self.player_state_atom_holds(kind, controller_id)
            }
            None => false,
        }
    }

    /// The resolution evaluation: every player and object reference of the
    /// shared resolvers.
    pub(super) fn state_condition_atom_at_resolution(
        &self,
        condition: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> bool {
        match value_kind(condition) {
            Some(OBJECT_IN_ZONE) => self
                .condition_object_instance(&condition["object"], stack_object, bindings, decisions)
                .is_some_and(|instance_id| {
                    self.instance_in_zones(&instance_id, &condition["zones"])
                }),
            Some(kind) => self
                .resolve_player(&condition["player"], stack_object, bindings)
                .is_some_and(|player_id| self.player_state_atom_holds(kind, &player_id)),
            None => false,
        }
    }

    /// The one object an `objectInZone` atom names at resolution, if any.
    fn condition_object_instance(
        &self,
        reference: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Option<String> {
        match value_kind(reference) {
            Some("self" | "abilitySource") => Some(stack_object.card.instance_id.clone()),
            Some("observedEventObject") => self
                .resolve_observed_event_permanent(reference, stack_object)
                .ok()
                .flatten(),
            Some("boundObjects" | "decisionResult") => {
                let objects = self
                    .resolve_runtime_object_ids_checked(
                        reference,
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .ok()?;
                match objects.as_slice() {
                    [instance_id] => Some(instance_id.clone()),
                    _ => None,
                }
            }
            _ => match self.resolve_target(reference, stack_object, bindings)? {
                TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id } => {
                    Some(instance_id)
                }
                TargetRef::StackObject { stack_id } => self
                    .state
                    .stack
                    .iter()
                    .find(|object| object.id == stack_id)
                    .map(|object| object.card.instance_id.clone()),
                TargetRef::Player { .. } => None,
            },
        }
    }
}
