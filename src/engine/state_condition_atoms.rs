//! Player-designation, turn and zone atoms of the condition language.
//!
//! - `playerIsMonarch { player }` (CR 724.1): the player is the monarch.
//! - `playerWasStartingPlayer { player }` (CR 103.1): the player took the
//!   game's first turn.
//! - `isPlayersTurn { player }` (CR 500.1): the player is the active player.
//! - `objectInZone { object, zones }` (CR 400.1): the referenced object is in
//!   one of the zones.
//! - `topCardMatches { player, zone, where }` (CR 401.1, 404.1): the top card
//!   of the player's library or graveyard satisfies the printed card filter;
//!   an empty zone has no top card.
//! - `colorIsMostCommon { color, where, tieAllowed }` (CR 105.1): among the
//!   permanents of `where`, the color's count is greater than (or, with
//!   `tieAllowed`, at least) every other color's.
//! - `castChoiceMade { spell: self, choice, mode?, manaCost? }` (CR
//!   601.2b-f): a choice made for the source as it was cast (`gift`,
//!   `teamwork`, an `alternativeCost` by casting mode or printed mana cost),
//!   read on the resolving spell's decisions or on the record the permanent
//!   it became keeps (`cast_events::cast_choice_flags`).
//! - `spellCastOrdinal` (`event_history.rs`): the observed spell's position
//!   among its caster's spells this turn.
//!
//! Player quantities of the shared numeric language (`numeric_quantities`):
//!
//! - `partySize { player }` (CR 700.8): up to one each of Cleric, Rogue,
//!   Warrior and Wizard among the creatures the player controls, as one
//!   maximum assignment.
//! - `startingLifeTotal { player }` (CR 103.4).
//!
//! Devotion to one or more colors is the shared `devotion { player, colors }`
//! quantity (CR 700.5a), not a condition-only measure.
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
const TOP_CARD: &str = "topCardMatches";
const MOST_COMMON_COLOR: &str = "colorIsMostCommon";
const CAST_CHOICE: &str = "castChoiceMade";
const SPELL_CAST_ORDINAL: &str = "spellCastOrdinal";

/// A cast choice as the casting declaration records it, before the spell
/// keeps it as a flag (CR 601.2b-c: a target a branch of the chosen gift or
/// cost acts on is declared only when that choice was made). Only choices a
/// declaration decision names; any other choice is not made yet.
fn declared_cast_choice(condition: &Value, decisions: &BTreeMap<String, Value>) -> bool {
    match condition["choice"].as_str() {
        Some("gift") => decisions.get("giftRecipient").is_some_and(Value::is_string),
        Some("teamwork") => decisions.get("teamworkPaid").and_then(Value::as_bool) == Some(true),
        Some("escape") => decisions.get("useEscape").and_then(Value::as_bool) == Some(true),
        _ => false,
    }
}

/// The colors a color rank compares (CR 105.1).
const COLORS: [&str; 5] = ["White", "Blue", "Black", "Red", "Green"];

/// The creature types that fill a party (CR 700.8).
const PARTY_ROLES: [&str; 4] = ["Cleric", "Rogue", "Warrior", "Wizard"];

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
    matches!(
        kind,
        OBJECT_IN_ZONE | TOP_CARD | MOST_COMMON_COLOR | CAST_CHOICE | SPELL_CAST_ORDINAL
    ) || PLAYER_ATOMS.contains(&kind)
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
        Some(TOP_CARD) => {
            singular_player_reference_supported(&condition["player"])
                && matches!(condition["zone"].as_str(), Some("library" | "graveyard"))
                && !condition["where"].is_null()
                && card_filter_supported(&condition["where"])
        }
        Some(MOST_COMMON_COLOR) => {
            condition["color"]
                .as_str()
                .is_some_and(|color| COLORS.contains(&color))
                && (condition["where"].is_null() || permanent_filter_supported(&condition["where"]))
                && condition["tieAllowed"].is_boolean()
        }
        Some(CAST_CHOICE) => cast_events::cast_choice_supported(condition),
        Some(SPELL_CAST_ORDINAL) => event_history::spell_cast_ordinal_supported(condition),
        _ => false,
    }
}

/// `Some(valid)` for a player quantity this module owns (`partySize`,
/// `startingLifeTotal`), `None` otherwise.
pub(super) fn player_quantity_supported(expression: &Value) -> Option<bool> {
    let supported = match value_kind(expression) {
        Some("partySize" | "startingLifeTotal") => {
            player_reference_supported(&expression["player"])
        }
        _ => return None,
    };
    Some(supported)
}

/// The size of the largest party among `roles_of` each creature (CR
/// 700.8): a maximum assignment of creatures to distinct roles.
fn party_size(roles_of: &[[bool; 4]]) -> i32 {
    fn assign(
        creature: usize,
        roles_of: &[[bool; 4]],
        holder: &mut [Option<usize>; 4],
        visited: &mut [bool; 4],
    ) -> bool {
        for role in 0..4 {
            if !roles_of[creature][role] || visited[role] {
                continue;
            }
            visited[role] = true;
            if holder[role].is_none_or(|other| assign(other, roles_of, holder, visited)) {
                holder[role] = Some(creature);
                return true;
            }
        }
        false
    }
    let mut holder = [None; 4];
    let mut size = 0;
    for creature in 0..roles_of.len() {
        let mut visited = [false; 4];
        if assign(creature, roles_of, &mut holder, &mut visited) {
            size += 1;
        }
        if size == 4 {
            break;
        }
    }
    size
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
        // A card is on the stack as a spell; an ability on the stack whose
        // source is the card leaves the card where it is (CR 113.7a, 405.1).
        if self.state.stack.iter().any(|object| {
            object.card.instance_id == instance_id
                && !matches!(
                    object.ability_kind.as_deref(),
                    Some("triggeredAbility" | "activatedAbility" | "manaAbility")
                )
        }) {
            return Some("stack");
        }
        self.card_outside_battlefield_zone(instance_id)
    }

    fn instance_in_zones(&self, instance_id: &str, zones: &Value) -> bool {
        self.current_zone_of_instance(instance_id)
            .is_some_and(|zone| zones_contain(zones, zone))
    }

    /// `topCardMatches` for one player: the last card of the ordered zone is
    /// its top (CR 401.1, 404.1).
    fn top_card_matches(&self, condition: &Value, player_id: &str) -> bool {
        let Ok(player_index) = self.player_index(player_id) else {
            return false;
        };
        let player = &self.state.players[player_index];
        let top = match condition["zone"].as_str() {
            Some("library") => player.library.last(),
            Some("graveyard") => player.graveyard.last(),
            _ => None,
        };
        top.is_some_and(|card| matches_card_filter(&card.definition, &condition["where"]))
    }

    /// `colorIsMostCommon` over every permanent of a player still in the
    /// game that satisfies `where`, read on its effective colors.
    fn color_is_most_common(&self, condition: &Value) -> bool {
        let Some(color) = condition["color"].as_str() else {
            return false;
        };
        let counted = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| player.battlefield.iter())
            .filter(|card| {
                condition["where"].is_null()
                    || self.permanent_state_matches(card, &condition["where"])
            })
            .collect::<Vec<_>>();
        let count_of = |name: &str| {
            let filter = json!({ "kind": "colorContains", "value": name });
            counted
                .iter()
                .filter(|card| self.permanent_state_matches(card, &filter))
                .count()
        };
        let own = count_of(color);
        let tie_allowed = condition["tieAllowed"].as_bool() == Some(true);
        COLORS.iter().filter(|other| **other != color).all(|other| {
            let theirs = count_of(*other);
            if tie_allowed {
                own >= theirs
            } else {
                own > theirs
            }
        })
    }

    /// The source's own record of a cast choice: its battlefield
    /// incarnation's flags, else the flags the source card carries.
    fn source_cast_choice_recorded(&self, condition: &Value, source: &CardInstance) -> bool {
        let Some(flag) = cast_events::cast_choice_flag(condition) else {
            return false;
        };
        self.permanent_position(&source.instance_id)
            .map(|(player_index, card_index)| {
                &self.state.players[player_index].battlefield[card_index].flags
            })
            .unwrap_or(&source.flags)
            .get(&flag)
            .copied()
            .unwrap_or(false)
    }

    /// `Some(value)` for a player quantity this module owns, for the player
    /// the expression names (already resolved to `player_id`).
    pub(super) fn player_quantity(
        &self,
        expression: &Value,
        player_id: &str,
    ) -> Option<Option<i32>> {
        player_quantity_supported(expression)?;
        let Ok(player_index) = self.player_index(player_id) else {
            return Some(None);
        };
        let player = &self.state.players[player_index];
        let value = match value_kind(expression) {
            Some("startingLifeTotal") => Some(player.starting_life),
            Some("partySize") => {
                let roles_of = player
                    .battlefield
                    .iter()
                    .filter(|card| self.permanent_is_creature(card))
                    .map(|card| {
                        PARTY_ROLES.map(|role| {
                            self.permanent_state_matches(
                                card,
                                &json!({ "kind": "subtypeContains", "value": role }),
                            )
                        })
                    })
                    .filter(|roles| roles.iter().any(|role| *role))
                    .collect::<Vec<_>>();
                Some(party_size(&roles_of))
            }
            _ => return None,
        };
        Some(value)
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
        decisions: &BTreeMap<String, Value>,
    ) -> bool {
        match value_kind(condition) {
            Some(TOP_CARD) => numeric_evaluation::source_relative_numeric_player(
                &condition["player"],
                controller_id,
                source,
            )
            .is_some_and(|player_id| self.top_card_matches(condition, player_id)),
            Some(MOST_COMMON_COLOR) => self.color_is_most_common(condition),
            Some(CAST_CHOICE) => match source {
                Some(source) => self.source_cast_choice_recorded(condition, source),
                // A casting declaration reads the choices it already made.
                None => declared_cast_choice(condition, decisions),
            },
            Some(SPELL_CAST_ORDINAL) => self.spell_cast_ordinal_holds(condition, decisions),
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
            Some(TOP_CARD) => self
                .resolve_player(&condition["player"], stack_object, bindings)
                .is_some_and(|player_id| self.top_card_matches(condition, &player_id)),
            Some(MOST_COMMON_COLOR) => self.color_is_most_common(condition),
            // A resolving spell reads its own cast decisions (a copy carries
            // the original's choices, CR 707.10); an ability reads the
            // record its source permanent keeps.
            Some(CAST_CHOICE) => {
                if stack_object_is_spell(stack_object) {
                    cast_events::cast_choice_flag(condition).is_some_and(|flag| {
                        self.cast_choice_flags(
                            &stack_object.card.definition,
                            &stack_object.decisions,
                        )
                        .contains(&flag)
                    })
                } else {
                    self.source_cast_choice_recorded(condition, &stack_object.card)
                }
            }
            Some(SPELL_CAST_ORDINAL) => self.spell_cast_ordinal_holds(condition, decisions),
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
