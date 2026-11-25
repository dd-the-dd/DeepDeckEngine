//! Numeric records read while an ability or a spell resolves, and totals of
//! one numeric property over a collection (`NumericQuantity::AbilityHistory`
//! and `NumericQuantity::PropertyTotal` of the Oracle numeric language):
//!
//! ```text
//! { kind: abilityResolutionsThisTurn }      CR 113.3, 603.1 the resolving
//! { kind: abilityActivationsThisTurn }      CR 602.1 ability's own record
//! { kind: sumBoundObjectProperty, binding, property, where? }  CR 608.2c,h
//! { kind: sumPermanentProperty, player, where, property }      CR 107.1
//!   property: power | toughness | manaValue
//! ```
//!
//! The ability records count the `stackObjectStartedResolving` (resolutions)
//! or `activatedAbilityPutOnStack` (activations) events of this turn whose
//! source is the same object and whose `abilityDigest` names the same rule.
//! The current resolution or activation is already recorded, so it counts.
//! A permanent that left and returned is a new object (CR 400.7): only the
//! events after its latest entry count. An ability without a recorded rule,
//! or a resolution that is not an ability's, has no record: the value is
//! `None`, never zero.
//!
//! A bound-object total reads each object as it last existed on the
//! battlefield once it left (the producing action's LKI record), as the
//! permanent it still is, or as the card it is in another zone; an object a
//! filter cannot be evaluated for (a departed object without a recorded
//! characteristic) makes the value `None`.
use super::*;

pub(super) const ABILITY_RESOLUTIONS: &str = "abilityResolutionsThisTurn";
pub(super) const ABILITY_ACTIVATIONS: &str = "abilityActivationsThisTurn";
const SUM_BOUND_OBJECTS: &str = "sumBoundObjectProperty";
const COUNT_LAST_KNOWN: &str = "countBoundObjectsLastKnown";
/// The event that records a permanent's departure and its last known type
/// line and state (CR 608.2h).
const DEPARTURE_EVENT: &str = "permanentLeftBattlefield";
const SUM_PERMANENTS: &str = "sumPermanentProperty";
const COUNT_COMPARED: &str = "countPlayersComparedWithController";
const COUNT_AT_EXTREMUM: &str = "countPlayersAtExtremum";
const QUANTIFIED_PLAYER: &str = "quantifiedPlayer";
const MAX_DEPTH: usize = 64;

/// Player references other than the quantified member: inside a member
/// measure they would be read against that member.
const OTHER_PLAYER_REFERENCES: &[&str] = &[
    "controller",
    "abilityController",
    "controllerOf",
    "controllerOfObject",
    "controllerOfAttachedPermanent",
    "ownerOf",
    "triggeringPlayer",
    "opponentsOf",
    "eachPlayer",
    "playersExcept",
    "filterPlayers",
];

fn names_only_the_member(value: &Value, depth: usize) -> bool {
    if depth >= MAX_DEPTH {
        return false;
    }
    match value {
        Value::Array(values) => values
            .iter()
            .all(|value| names_only_the_member(value, depth + 1)),
        Value::Object(object) => {
            !value_kind(value).is_some_and(|kind| OTHER_PLAYER_REFERENCES.contains(&kind))
                && object
                    .values()
                    .all(|value| names_only_the_member(value, depth + 1))
        }
        _ => true,
    }
}

/// A measure of one population member written with `quantifiedPlayer`.
fn member_measure_supported(value: &Value) -> bool {
    value.is_object()
        && value_kind(value) != Some(QUANTIFIED_PLAYER)
        && names_only_the_member(value, 0)
        && numeric_expression_supported(&player_quantifier::bind_quantified_player(value))
}

/// The base population a comparison counts: every player, or the opponents
/// of the ability's controller.
fn compared_population_supported(players: &Value) -> bool {
    match value_kind(players) {
        Some("eachPlayer") => true,
        Some("opponentsOf") => {
            value_kind(&players["player"]) == Some("controllerOf")
                && matches!(
                    value_kind(&players["player"]["object"]),
                    Some("self" | "abilitySource")
                )
        }
        _ => false,
    }
}

/// The event that records one resolution of a stack object.
const RESOLUTION_EVENT: &str = "stackObjectStartedResolving";
/// The event that records one activation of a non-mana ability.
const ACTIVATION_EVENT: &str = "activatedAbilityPutOnStack";
/// The event that starts a new battlefield object (CR 400.7).
const ENTRY_EVENT: &str = "permanentEnteredBattlefield";
/// The event detail naming the rule of the ability an event records.
pub(super) const ABILITY_DIGEST_FIELD: &str = "abilityDigest";

#[derive(Clone, Copy)]
enum Property {
    Power,
    Toughness,
    ManaValue,
}

fn property(expression: &Value) -> Option<Property> {
    match expression["property"].as_str()? {
        "power" => Some(Property::Power),
        "toughness" => Some(Property::Toughness),
        "manaValue" => Some(Property::ManaValue),
        _ => None,
    }
}

/// A printed power or toughness of a card outside the battlefield (CR
/// 208.1); a card without one reads 0.
fn printed_stat(value: &Option<String>) -> i32 {
    value
        .as_deref()
        .and_then(|stat| stat.trim().parse::<i32>().ok())
        .unwrap_or(0)
}

/// A filter a departure record answers: type-line atoms and the recorded
/// state atoms (counters, tapped, combat, token), under Boolean operators.
fn departure_filter_supported(filter: &Value, depth: usize) -> bool {
    depth < 32
        && match value_kind(filter) {
            Some(
                "cardTypeContains" | "subtypeContains" | "typeLineContains" | "isLegendary"
                | "isBasic",
            ) => filter["value"].is_string() || filter.get("value").is_none(),
            Some(kind) if state_condition_atoms::departure_record_atom(kind) => {
                state_condition_atoms::departure_record_atom_supported(filter)
            }
            Some("not") => departure_filter_supported(&filter["operand"], depth + 1),
            Some("and" | "or") => filter["operands"].as_array().is_some_and(|operands| {
                !operands.is_empty()
                    && operands
                        .iter()
                        .all(|operand| departure_filter_supported(operand, depth + 1))
            }),
            _ => false,
        }
}

/// `Some(valid)` for a kind this module owns, `None` otherwise.
pub(super) fn resolution_record_supported(expression: &Value) -> Option<bool> {
    Some(match value_kind(expression)? {
        ABILITY_RESOLUTIONS | ABILITY_ACTIVATIONS => expression
            .as_object()
            .is_some_and(|fields| fields.len() == 1),
        SUM_BOUND_OBJECTS => {
            property(expression).is_some()
                && expression["binding"]
                    .as_str()
                    .is_some_and(|binding| !binding.is_empty())
                && expression.get("where").is_none_or(card_filter_supported)
        }
        COUNT_LAST_KNOWN => {
            expression["binding"]
                .as_str()
                .is_some_and(|binding| !binding.is_empty())
                && departure_filter_supported(&expression["where"], 0)
        }
        SUM_PERMANENTS => {
            property(expression).is_some()
                && player_reference_supported(&expression["player"])
                && permanent_filter_supported(&expression["where"])
        }
        COUNT_COMPARED => {
            player_reference_supported(&expression["player"])
                && compared_population_supported(&expression["players"])
                && matches!(
                    expression["operator"].as_str(),
                    Some(">" | "<" | ">=" | "<=" | "==")
                )
                && member_measure_supported(&expression["value"])
        }
        COUNT_AT_EXTREMUM => {
            matches!(expression["extremum"].as_str(), Some("greatest" | "least"))
                && member_measure_supported(&expression["value"])
        }
        _ => return None,
    })
}

/// A stable identity of one ability rule (FNV-1a over its canonical JSON):
/// two events record the same ability of the same object when their source
/// and digest are equal.
pub(super) fn ability_digest(rule: &Value) -> Value {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in rule.to_string().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Value::from(format!("{hash:016x}"))
}

/// The digest of the rule a stack object resolves, `null` for a spell.
pub(super) fn stack_ability_digest(stack_object: &StackObject) -> Value {
    stack_object
        .ability_rule
        .as_ref()
        .map_or(Value::Null, ability_digest)
}

impl GameEngine {
    /// `Some(value)` for a resolution record this module owns, read for the
    /// resolving `stack_object` with the resolution's bindings.
    pub(super) fn resolution_record_numeric(
        &self,
        expression: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Option<Option<i32>> {
        match value_kind(expression)? {
            ABILITY_RESOLUTIONS => Some(self.ability_record_count(stack_object, RESOLUTION_EVENT)),
            ABILITY_ACTIVATIONS => Some(self.ability_record_count(stack_object, ACTIVATION_EVENT)),
            SUM_BOUND_OBJECTS => {
                if resolution_record_supported(expression) != Some(true) {
                    return Some(None);
                }
                Some(self.bound_object_property_total(expression, bindings, decisions))
            }
            COUNT_LAST_KNOWN => {
                if resolution_record_supported(expression) != Some(true) {
                    return Some(None);
                }
                Some(self.bound_objects_last_known_count(expression, bindings))
            }
            _ => None,
        }
    }

    /// The events of this turn recording the ability `stack_object` resolves
    /// (its resolutions, or its activations), since its source last entered.
    fn ability_record_count(&self, stack_object: &StackObject, event_kind: &str) -> Option<i32> {
        let ability_kind = stack_object.ability_kind.as_deref()?;
        let admitted = match event_kind {
            ACTIVATION_EVENT => ability_kind == "activatedAbility",
            _ => matches!(ability_kind, "activatedAbility" | "triggeredAbility"),
        };
        if !admitted {
            return None;
        }
        let digest = stack_ability_digest(stack_object);
        if digest.is_null() {
            return None;
        }
        let source = stack_object.card.instance_id.as_str();
        let since = self
            .state
            .events
            .iter()
            .rposition(|event| {
                event.kind == ENTRY_EVENT && event.card_instance_id.as_deref() == Some(source)
            })
            .map_or(0, |index| index + 1);
        let count = self.state.events[since..]
            .iter()
            .filter(|event| {
                event.turn_number == self.state.turn_number
                    && event.kind == event_kind
                    && event.card_instance_id.as_deref() == Some(source)
                    && event.detail[ABILITY_DIGEST_FIELD] == digest
            })
            .count();
        i32::try_from(count).ok()
    }

    /// Whether one bound object satisfied `filter` as it last existed on the
    /// battlefield: the permanent it still is, else its latest departure
    /// record (CR 608.2h). `None` without either, or for a filter the record
    /// does not answer.
    fn bound_object_last_known_matches(&self, instance_id: &str, filter: &Value) -> Option<bool> {
        if let Some((player, index)) = self.permanent_position(instance_id) {
            let permanent = &self.state.players[player].battlefield[index];
            return Some(self.permanent_matches_effective_filter(permanent, filter));
        }
        let departure = self.state.events.iter().rev().find(|event| {
            event.kind == DEPARTURE_EVENT && event.card_instance_id.as_deref() == Some(instance_id)
        })?;
        state_condition_atoms::departure_record_matches(&departure.detail, filter, 0)
    }

    /// The bound objects of a removal (destroy, sacrifice) that satisfied
    /// `where` as they last existed on the battlefield (`if an artifact was
    /// destroyed this way`, CR 608.2c, 608.2h).
    fn bound_objects_last_known_count(
        &self,
        expression: &Value,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Option<i32> {
        let binding = expression["binding"].as_str()?;
        let Some(RuntimeBinding::Objects(objects)) = bindings.get(binding) else {
            return None;
        };
        let mut count = 0_i32;
        for instance_id in objects {
            if self.bound_object_last_known_matches(instance_id, &expression["where"])? {
                count = count.checked_add(1)?;
            }
        }
        Some(count)
    }

    /// The sum of one property over the objects a producing action bound.
    fn bound_object_property_total(
        &self,
        expression: &Value,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Option<i32> {
        let property = property(expression)?;
        let binding = expression["binding"].as_str()?;
        let Some(RuntimeBinding::Objects(objects)) = bindings.get(binding) else {
            return None;
        };
        let filter = expression.get("where").filter(|filter| !filter.is_null());
        let recorded = decisions.get(&effect_domains::bound_object_lki_key(binding));
        let field = match property {
            Property::Power => "power",
            Property::Toughness => "toughness",
            Property::ManaValue => "manaValue",
        };
        let mut total = 0_i32;
        for instance_id in objects {
            let value = if let Some(record) =
                recorded.and_then(|records| records.get(instance_id.as_str()))
            {
                // Its last known information (CR 608.2h): the numbers the
                // producing action recorded, the type line and state its
                // departure recorded.
                if let Some(filter) = filter
                    && !self.bound_object_last_known_matches(instance_id, filter)?
                {
                    continue;
                }
                i32::try_from(record.get(field)?.as_i64()?).ok()?
            } else if let Some((player, index)) = self.permanent_position(instance_id) {
                let permanent = &self.state.players[player].battlefield[index];
                if filter.is_some_and(|filter| {
                    !self.permanent_matches_effective_filter(permanent, filter)
                }) {
                    continue;
                }
                match property {
                    Property::Power => self.current_power(permanent),
                    Property::Toughness => self.current_toughness(permanent),
                    Property::ManaValue => mana_value(&permanent.definition),
                }
            } else {
                let card = self.card_outside_battlefield(instance_id)?;
                if filter.is_some_and(|filter| !matches_card_filter(&card.definition, filter)) {
                    continue;
                }
                match property {
                    Property::Power => printed_stat(&card.definition.power),
                    Property::Toughness => printed_stat(&card.definition.toughness),
                    Property::ManaValue => mana_value(&card.definition),
                }
            };
            total = total.checked_add(value)?;
        }
        Some(total)
    }

    /// The controller-relative quantities of this module, read for the
    /// player the expression names (already resolved to `player_id`).
    pub(super) fn controller_relative_record(
        &self,
        expression: &Value,
        player_id: &str,
    ) -> Option<Option<i32>> {
        let kind = value_kind(expression)?;
        if !matches!(kind, SUM_PERMANENTS | COUNT_COMPARED | COUNT_AT_EXTREMUM) {
            return None;
        }
        if resolution_record_supported(expression) != Some(true) {
            return Some(None);
        }
        Some(match kind {
            SUM_PERMANENTS => self.permanent_property_total(expression, player_id),
            COUNT_COMPARED => self.players_compared_with_controller(expression, player_id),
            _ => self.players_at_extremum(expression),
        })
    }

    /// One member's measure: `quantifiedPlayer` read as `player_id`.
    fn member_measure(&self, measure: &Value, player_id: &str) -> Option<i32> {
        self.numeric_expression_with_source(measure, player_id, None)
    }

    /// The living members of the population whose measure compares with the
    /// controller's (`operator` reads `member OP controller`).
    fn players_compared_with_controller(
        &self,
        expression: &Value,
        controller_id: &str,
    ) -> Option<i32> {
        let measure = player_quantifier::bind_quantified_player(&expression["value"]);
        let anchor = self.member_measure(&measure, controller_id)?;
        let opponents_only = value_kind(&expression["players"]) == Some("opponentsOf");
        let mut count = 0_i32;
        for player in self.state.players.iter().filter(|player| !player.has_lost) {
            if opponents_only && player.id == controller_id {
                continue;
            }
            let value = self.member_measure(&measure, &player.id)?;
            let holds = match expression["operator"].as_str()? {
                ">" => value > anchor,
                "<" => value < anchor,
                ">=" => value >= anchor,
                "<=" => value <= anchor,
                "==" => value == anchor,
                _ => return None,
            };
            if holds {
                count = count.checked_add(1)?;
            }
        }
        Some(count)
    }

    /// The living players whose measure is the greatest (or least) of every
    /// living player's; `None` in a game without a living player.
    fn players_at_extremum(&self, expression: &Value) -> Option<i32> {
        let measure = player_quantifier::bind_quantified_player(&expression["value"]);
        let values = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .map(|player| self.member_measure(&measure, &player.id))
            .collect::<Option<Vec<_>>>()?;
        let extremum = match expression["extremum"].as_str()? {
            "greatest" => values.iter().copied().max()?,
            "least" => values.iter().copied().min()?,
            _ => return None,
        };
        i32::try_from(values.iter().filter(|value| **value == extremum).count()).ok()
    }

    /// `sumPermanentProperty` for the player the expression names (already
    /// resolved to `player_id`): the property summed over that player's
    /// permanents matching `where`, read with every layer applied.
    fn permanent_property_total(&self, expression: &Value, player_id: &str) -> Option<i32> {
        let property = property(expression)?;
        let player_index = self.player_index(player_id).ok()?;
        let mut total = 0_i32;
        for permanent in self.state.players[player_index]
            .battlefield
            .iter()
            .filter(|permanent| {
                self.permanent_matches_effective_filter(permanent, &expression["where"])
            })
        {
            let value = match property {
                Property::Power => self.current_power(permanent),
                Property::Toughness => self.current_toughness(permanent),
                Property::ManaValue => mana_value(&permanent.definition),
            };
            total = total.checked_add(value)?;
        }
        Some(total)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "resolution_records_tests.rs"]
mod tests;
