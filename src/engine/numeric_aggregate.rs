//! The shared aggregate of the Oracle numeric language over an object set
//! (`NumericQuantity::Aggregate`, CR 107.1): one function of one property
//! over the members of a runtime collection or of one referenced object.
//!
//! ```text
//! { kind: aggregateObjects,
//!   function: greatest | least | total | distinct,
//!   property: power | toughness | manaValue          (one number each)
//!           | colors | cardTypes | creatureTypes     (value sets)
//!           | basicLandTypes | counterKinds | names,
//!   objects: COLLECTION | object: REFERENCE }
//! ```
//!
//! `greatest`, `least` and `total` read a numeric property and are zero over
//! an empty set; `distinct` counts the different values among the members,
//! a member with several values (a multicolored permanent, an artifact
//! creature) contributing each of them. A battlefield member is read with
//! its current characteristics (CR 613): power and toughness after every
//! layer, colors and card types as modified; a card in another zone, or a
//! spell, is read on its card (CR 109.3). The collection is the shared
//! runtime collection algebra; an unresolvable collection or reference is
//! `None`, never zero. Subtypes are not card types (CR 205.2a): an
//! `Artifact Creature — Golem` has two card types.
use super::*;

/// The numeric properties every function reads (one value per member).
const NUMERIC_PROPERTIES: [&str; 3] = ["power", "toughness", "manaValue"];

/// The value-set properties only `distinct` reads.
const SET_PROPERTIES: [&str; 6] = [
    "colors",
    "cardTypes",
    "creatureTypes",
    "basicLandTypes",
    "counterKinds",
    "names",
];

/// CR 205.2a: the card types.
const CARD_TYPES: [&str; 9] = [
    "Artifact",
    "Battle",
    "Creature",
    "Enchantment",
    "Instant",
    "Kindred",
    "Land",
    "Planeswalker",
    "Sorcery",
];

/// CR 205.3i: the basic land types.
const BASIC_LAND_TYPES: [&str; 5] = ["Plains", "Island", "Swamp", "Mountain", "Forest"];

/// Every creature type of the canonical registry (CR 205.3m), which a
/// changeling has (CR 702.73a). The same versioned registry the shared
/// predicate reads; only its members are needed here.
fn every_creature_type() -> &'static [String] {
    static TYPES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    TYPES.get_or_init(|| {
        #[derive(serde::Deserialize)]
        struct Registry {
            canonical: Vec<String>,
        }
        let registry: Registry = serde_json::from_str(include_str!(
            "../../data/oracle-vocabulary/creature-subtypes-cr-205.3m-2026-11-06.json"
        ))
        .expect("embedded creature-subtype registry must deserialize");
        registry
            .canonical
            .into_iter()
            .map(|name| name.to_ascii_lowercase())
            .collect()
    })
}

/// `Some(valid)` for the aggregate kind, `None` for any other kind.
pub(super) fn aggregate_objects_supported(expression: &Value) -> Option<bool> {
    if value_kind(expression) != Some("aggregateObjects") {
        return None;
    }
    let property = expression["property"].as_str().unwrap_or_default();
    let numeric = NUMERIC_PROPERTIES.contains(&property);
    let function = match expression["function"].as_str() {
        Some("greatest" | "least" | "total") => numeric,
        Some("distinct") => numeric || SET_PROPERTIES.contains(&property),
        _ => false,
    };
    let members = expression
        .get("requireMembers")
        .is_none_or(Value::is_boolean);
    let operand = match (expression.get("objects"), expression.get("object")) {
        (Some(objects), None) => object_expression_supported(objects),
        (None, Some(object)) => aggregate_reference_supported(object),
        _ => false,
    };
    Some(function && members && operand)
}

/// The single objects an aggregate reads by reference.
fn aggregate_reference_supported(reference: &Value) -> bool {
    match value_kind(reference) {
        Some(
            "self"
            | "abilitySource"
            | "triggeringPermanent"
            | "triggeringStackObject"
            | "observedEventObject"
            | "combatCounterpart",
        ) => true,
        Some("attachedPermanent") => value_kind(&reference["attachment"]) == Some("self"),
        Some("singleBoundObject" | "boundObject") => reference["binding"].is_string(),
        _ => target_reference_supported(reference),
    }
}

/// One member of the aggregated set: a battlefield permanent read with its
/// current characteristics, or a card (another zone, or a spell) read on
/// its card.
#[derive(Clone, Copy)]
enum Member<'a> {
    Permanent(&'a CardInstance),
    Card(&'a CardInstance),
}

impl<'a> Member<'a> {
    fn card(self) -> &'a CardInstance {
        match self {
            Self::Permanent(card) | Self::Card(card) => card,
        }
    }
}

/// One value of a member for a property: a number, or the values of a set.
enum MemberValue {
    Number(Option<i32>),
    Values(BTreeSet<String>),
}

impl GameEngine {
    /// The member an object id names now: a phased-in permanent, a card of
    /// another zone, or a spell on the stack (by card or stack id).
    fn aggregate_member(&self, id: &str) -> Option<Member<'_>> {
        if let Some((player_index, card_index)) = self.permanent_position(id) {
            return Some(Member::Permanent(
                &self.state.players[player_index].battlefield[card_index],
            ));
        }
        if let Some(card) = self.card_outside_battlefield(id) {
            return Some(Member::Card(card));
        }
        self.state
            .stack
            .iter()
            .find(|object| object.id == id || object.card.instance_id == id)
            .map(|object| Member::Card(&object.card))
    }

    fn aggregate_member_of_target(&self, target: &TargetRef) -> Option<Member<'_>> {
        match target {
            TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id } => {
                self.aggregate_member(instance_id)
            }
            TargetRef::StackObject { stack_id } => self.aggregate_member(stack_id),
            TargetRef::Player { .. } => None,
        }
    }

    /// The value of one member for `property` (CR 613: a permanent's
    /// current characteristics).
    fn aggregate_member_value(&self, member: Member<'_>, property: &str) -> Option<MemberValue> {
        let card = member.card();
        let definition = &card.definition;
        let permanent = match member {
            Member::Permanent(permanent) => Some(permanent),
            Member::Card(_) => None,
        };
        Some(match property {
            // A card without a printed value has no power (CR 208.2); it is
            // no member of a power aggregate.
            "power" => MemberValue::Number(match permanent {
                Some(permanent) => Some(self.current_power(permanent)),
                None => definition
                    .power
                    .as_ref()
                    .map(|_| parse_stat(&definition.power)),
            }),
            "toughness" => MemberValue::Number(match permanent {
                Some(permanent) => Some(self.current_toughness(permanent)),
                None => definition
                    .toughness
                    .as_ref()
                    .map(|_| parse_stat(&definition.toughness)),
            }),
            "manaValue" => MemberValue::Number(Some(mana_value(definition))),
            "colors" => MemberValue::Values(match permanent {
                Some(permanent) => self.permanent_color_symbols(permanent),
                None => card_color_symbols(definition),
            }),
            "cardTypes" => MemberValue::Values(
                CARD_TYPES
                    .iter()
                    .filter(|card_type| match permanent {
                        Some(permanent) => self.permanent_has_card_type(permanent, card_type),
                        None => has_card_type(definition, card_type),
                    })
                    .map(|card_type| card_type.to_string())
                    .collect(),
            ),
            "creatureTypes" => {
                let changeling = match permanent {
                    Some(permanent) => self.permanent_has_keyword(permanent, "changeling"),
                    None => permanent_printed_has_keyword(card, "changeling"),
                };
                MemberValue::Values(if changeling {
                    every_creature_type().iter().cloned().collect()
                } else {
                    type_line_subtypes(&definition.type_line)
                        .split_whitespace()
                        .filter(|subtype| object_predicate::is_creature_subtype(subtype))
                        .map(str::to_ascii_lowercase)
                        .collect()
                })
            }
            "basicLandTypes" => MemberValue::Values(
                BASIC_LAND_TYPES
                    .iter()
                    .filter(|land_type| match permanent {
                        Some(permanent) => self.permanent_matches_effective_filter(
                            permanent,
                            &json!({ "kind": "subtypeContains", "value": land_type }),
                        ),
                        None => has_card_subtype(definition, land_type),
                    })
                    .map(|land_type| land_type.to_string())
                    .collect(),
            ),
            "counterKinds" => MemberValue::Values(
                card.counters
                    .iter()
                    .filter(|(_, count)| **count > 0)
                    .map(|(kind, _)| kind.clone())
                    .collect(),
            ),
            "names" => {
                MemberValue::Values(std::iter::once(definition.name.to_ascii_lowercase()).collect())
            }
            _ => return None,
        })
    }

    /// The function of `expression` over `members`; `None` for an unknown
    /// function or property.
    fn aggregate_over(&self, expression: &Value, members: &[Member<'_>]) -> Option<i32> {
        // `requireMembers`: undefined over an empty set (a comparison that
        // reads it is false), never zero.
        if members.is_empty() && expression["requireMembers"].as_bool() == Some(true) {
            return None;
        }
        let property = expression["property"].as_str()?;
        let values = members
            .iter()
            .map(|member| self.aggregate_member_value(*member, property))
            .collect::<Option<Vec<_>>>()?;
        let numbers = values
            .iter()
            .filter_map(|value| match value {
                MemberValue::Number(number) => *number,
                MemberValue::Values(_) => None,
            })
            .collect::<Vec<i32>>();
        match expression["function"].as_str()? {
            "greatest" => Some(numbers.iter().copied().max().unwrap_or(0)),
            "least" => Some(numbers.iter().copied().min().unwrap_or(0)),
            "total" => Some(numbers.iter().copied().fold(0_i32, i32::saturating_add)),
            "distinct" => {
                let mut distinct = BTreeSet::new();
                for value in &values {
                    match value {
                        MemberValue::Number(Some(number)) => {
                            distinct.insert(number.to_string());
                        }
                        MemberValue::Number(None) => {}
                        MemberValue::Values(set) => distinct.extend(set.iter().cloned()),
                    }
                }
                i32::try_from(distinct.len()).ok()
            }
            _ => None,
        }
    }

    /// The resolution path: the collection is resolved by the shared
    /// collection algebra with the stack object's bindings, the reference
    /// by the shared reference resolver.
    pub(super) fn runtime_numeric_aggregate(
        &self,
        expression: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Option<Option<i32>> {
        if value_kind(expression) != Some("aggregateObjects") {
            return None;
        }
        let members = if let Some(objects) = expression.get("objects") {
            self.resolve_runtime_object_ids_checked(objects, stack_object, bindings, decisions)
                .ok()
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| self.aggregate_member(id))
                        .collect::<Vec<_>>()
                })
        } else {
            let reference = &expression["object"];
            match value_kind(reference) {
                Some("self" | "abilitySource") => self
                    .aggregate_member(&stack_object.card.instance_id)
                    .or(Some(Member::Card(&stack_object.card)))
                    .map(|member| vec![member]),
                _ => self
                    .resolve_target(reference, stack_object, bindings)
                    .and_then(|target| self.aggregate_member_of_target(&target))
                    .map(|member| vec![member]),
            }
        };
        Some(members.and_then(|members| self.aggregate_over(expression, &members)))
    }

    /// The controller-relative path (static abilities, conditions): the
    /// collections whose members need no resolution binding, read for the
    /// ability's controller and source. Any other collection is `None`.
    pub(super) fn source_numeric_aggregate(
        &self,
        expression: &Value,
        controller_id: &str,
        source: Option<&CardInstance>,
    ) -> Option<Option<i32>> {
        if value_kind(expression) != Some("aggregateObjects") {
            return None;
        }
        let members = if let Some(objects) = expression.get("objects") {
            self.source_aggregate_collection(objects, controller_id, source, 0)
        } else {
            let reference = &expression["object"];
            match value_kind(reference) {
                Some("self" | "abilitySource") => source.map(|source| {
                    vec![
                        self.aggregate_member(&source.instance_id)
                            .unwrap_or(Member::Card(source)),
                    ]
                }),
                // `equipped creature`: the permanent the source is attached to.
                Some("attachedPermanent")
                    if value_kind(&reference["attachment"]) == Some("self") =>
                {
                    source
                        .and_then(|source| source.attached_to.as_deref())
                        .and_then(|id| self.aggregate_member(id))
                        .map(|member| vec![member])
                }
                _ => None,
            }
        };
        Some(members.and_then(|members| self.aggregate_over(expression, &members)))
    }

    /// The players a controller-relative population names.
    fn source_aggregate_players(
        &self,
        reference: Option<&Value>,
        controller_id: &str,
        source: Option<&CardInstance>,
    ) -> Option<Vec<String>> {
        let living = |id: &str| {
            self.state
                .players
                .iter()
                .any(|player| player.id == id && !player.has_lost)
        };
        let Some(reference) = reference else {
            return Some(
                self.state
                    .players
                    .iter()
                    .filter(|player| !player.has_lost)
                    .map(|player| player.id.clone())
                    .collect(),
            );
        };
        match value_kind(reference) {
            Some("eachPlayer") => self.source_aggregate_players(None, controller_id, source),
            Some("opponentsOf") => {
                let anchor = numeric_evaluation::source_relative_numeric_player(
                    &reference["player"],
                    controller_id,
                    source,
                )?;
                Some(
                    self.state
                        .players
                        .iter()
                        .filter(|player| !player.has_lost && player.id != anchor)
                        .map(|player| player.id.clone())
                        .collect(),
                )
            }
            _ => {
                let player = numeric_evaluation::source_relative_numeric_player(
                    reference,
                    controller_id,
                    source,
                )?;
                Some(if living(player) {
                    vec![player.to_string()]
                } else {
                    Vec::new()
                })
            }
        }
    }

    fn source_aggregate_collection(
        &self,
        objects: &Value,
        controller_id: &str,
        source: Option<&CardInstance>,
        depth: usize,
    ) -> Option<Vec<Member<'_>>> {
        if depth >= 16 {
            return None;
        }
        match value_kind(objects) {
            Some("eachPermanent") => {
                if objects.get("players").is_some() || objects.get("exclude").is_some() {
                    return None;
                }
                let players =
                    self.source_aggregate_players(objects.get("player"), controller_id, source)?;
                let exclude_source = objects["excludeSource"].as_bool() == Some(true);
                if exclude_source && source.is_none() {
                    return None;
                }
                let filter = &objects["where"];
                Some(
                    self.state
                        .players
                        .iter()
                        .filter(|player| players.contains(&player.id))
                        .flat_map(|player| player.battlefield.iter())
                        .filter(|permanent| {
                            !permanent.flags.get("phasedOut").copied().unwrap_or(false)
                        })
                        .filter(|permanent| {
                            !exclude_source
                                || source.is_some_and(|source| {
                                    source.instance_id != permanent.instance_id
                                })
                        })
                        .filter(|permanent| {
                            filter.is_null() || self.permanent_state_matches(permanent, filter)
                        })
                        .map(Member::Permanent)
                        .collect(),
                )
            }
            Some("cardsInZone") => {
                let zone = &objects["zone"];
                let holders = match (zone.get("player"), zone.get("players")) {
                    (Some(player), None) => {
                        self.source_aggregate_players(Some(player), controller_id, source)?
                    }
                    (None, Some(players)) => {
                        self.source_aggregate_players(Some(players), controller_id, source)?
                    }
                    _ => return None,
                };
                let filter = &objects["where"];
                let mut members = Vec::new();
                for player in &self.state.players {
                    if !holders.contains(&player.id) {
                        continue;
                    }
                    let cards = match value_kind(zone) {
                        Some("graveyard") => &player.graveyard,
                        Some("hand") => &player.hand,
                        Some("library") => &player.library,
                        Some("exile") => &player.exile,
                        _ => return None,
                    };
                    members.extend(
                        cards
                            .iter()
                            .filter(|card| {
                                filter.is_null() || matches_card_filter(&card.definition, filter)
                            })
                            .map(Member::Card),
                    );
                }
                Some(members)
            }
            Some("union") => {
                let mut members: Vec<Member<'_>> = Vec::new();
                for set in objects["sets"].as_array()? {
                    for member in
                        self.source_aggregate_collection(set, controller_id, source, depth + 1)?
                    {
                        if !members
                            .iter()
                            .any(|known| known.card().instance_id == member.card().instance_id)
                        {
                            members.push(member);
                        }
                    }
                }
                Some(members)
            }
            _ => None,
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "numeric_aggregate_tests.rs"]
mod tests;
