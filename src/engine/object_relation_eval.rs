//! Relation atoms of the shared object query: a candidate related to another
//! object, its anchor (`attachedTo`, `blocking`, `blockedBy`,
//! `sharesCharacteristic`).
//!
//! The parser lowers each relation with its anchor as an object expression
//! (`self`, `triggeringPermanent`, `chosenTarget`, an `eachPermanent`
//! query). A relation is evaluated in two steps:
//!
//! 1. Preparation, once before the candidates are enumerated, resolves the
//!    anchor against the resolving ability (its source, its targets, its
//!    bindings) into `anchorIds` and, for a share, the anchor's values of the
//!    characteristic (`anchorValues`), read from the anchor itself or its last
//!    known information (CR 608.2h). An anchor a share cannot read is an
//!    error, never an empty set.
//! 2. Evaluation tests one candidate against the prepared atom from game
//!    state only: the attachment (CR 301.5, 303.4), the block pairs of the
//!    current combat (CR 509.1) or the candidate's own characteristics.
//!
//! Only consumers that prepare their filter accept a relation in their
//! validator (`filter_supported_with_relations`); an unprepared atom never
//! matches. Counts of related permanents are evaluated here too, for the
//! resolving ability (`runtime_related_permanent_count`) and for a static
//! source (`related_permanent_count_with_source`).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::{
    CardDefinition, CardInstance, EngineError, GameEngine, RuntimeBinding, StackObject, TargetRef,
    card_color_symbols, has_keyword, object_expression_supported, permanent_filter_supported,
    player_reference_supported, runtime_object_filter::ObjectFilterView, type_line_subtypes,
    value_kind,
};

const PERMANENT_CARD_TYPES: [&str; 6] = [
    "Artifact",
    "Battle",
    "Creature",
    "Enchantment",
    "Land",
    "Planeswalker",
];
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

/// Whether `kind` is a relation atom this module prepares and evaluates.
pub(super) fn is_relation_kind(kind: &str) -> bool {
    matches!(
        kind,
        "attachedTo" | "blocking" | "blockedBy" | "convoked" | "sharesCharacteristic"
    )
}

/// The anchor object expressions a relation can resolve at resolution.
fn anchor_supported(anchor: &Value) -> bool {
    match value_kind(anchor) {
        Some("self" | "abilitySource" | "triggeringPermanent" | "combatCounterpart") => true,
        Some("chosenTarget" | "chosenTargets") => anchor["id"].is_string(),
        Some("singleBoundObject" | "boundObjects") => anchor["binding"].is_string(),
        Some("observedEventObject") => anchor["event"].is_string(),
        Some("attachedPermanent") => value_kind(&anchor["attachment"]) == Some("self"),
        Some("eachPermanent") => {
            anchor.get("player").is_none_or(player_reference_supported)
                && anchor.get("where").is_none_or(|filter| {
                    filter_supported_with_relations(filter, permanent_filter_supported)
                })
        }
        _ => false,
    }
}

/// One unprepared relation atom: a known relation and characteristic, and
/// an anchor this module resolves.
pub(super) fn relation_atom_supported(atom: &Value) -> bool {
    let kind = match value_kind(atom) {
        Some("attachedTo" | "blocking" | "blockedBy" | "convoked") => true,
        Some("sharesCharacteristic") => matches!(
            atom["characteristic"].as_str(),
            Some("creatureType" | "cardType" | "permanentType" | "landType" | "color")
        ),
        _ => false,
    };
    kind && anchor_supported(&atom["object"])
}

/// A filter whose leaves are relation atoms or atoms `ordinary` accepts. A
/// consumer that prepares its filter before matching uses this in place of
/// `ordinary` alone.
pub(super) fn filter_supported_with_relations(
    filter: &Value,
    ordinary: fn(&Value) -> bool,
) -> bool {
    match value_kind(filter) {
        Some(kind) if is_relation_kind(kind) => relation_atom_supported(filter),
        Some("and" | "or") => filter["operands"].as_array().is_some_and(|operands| {
            !operands.is_empty()
                && operands
                    .iter()
                    .all(|operand| filter_supported_with_relations(operand, ordinary))
        }),
        Some("not") => filter_supported_with_relations(&filter["operand"], ordinary),
        _ => ordinary(filter),
    }
}

/// Whether a relation atom occurs anywhere in `filter`.
pub(super) fn filter_has_relation(filter: &Value) -> bool {
    match value_kind(filter) {
        Some(kind) if is_relation_kind(kind) => true,
        Some("and" | "or") => filter["operands"]
            .as_array()
            .is_some_and(|operands| operands.iter().any(filter_has_relation)),
        Some("not") => filter_has_relation(&filter["operand"]),
        _ => false,
    }
}

/// `countPermanents` over a filter with a relation: the validation of the
/// count's own fields, with relation leaves admitted. `None` for any other
/// expression, which keeps its own validator.
pub(super) fn related_count_supported(expression: &Value) -> Option<bool> {
    if value_kind(expression) != Some("countPermanents")
        || !filter_has_relation(&expression["where"])
    {
        return None;
    }
    Some(
        filter_supported_with_relations(&expression["where"], permanent_filter_supported)
            && expression
                .get("player")
                .is_none_or(player_reference_supported)
            && expression.get("allPlayers").is_none_or(Value::is_boolean)
            && expression
                .get("excludeSource")
                .is_none_or(Value::is_boolean)
            && expression
                .get("ownership")
                .is_none_or(|value| matches!(value.as_str(), Some("owned" | "notOwned"))),
    )
}

/// An `eachPermanent` collection whose filter has a relation, for a consumer
/// that resolves it through the shared runtime collection (which prepares
/// the filter): the relation-aware filter, then every other field of the
/// shared object-expression validator.
pub(super) fn related_collection_supported(expression: &Value) -> bool {
    if value_kind(expression) != Some("eachPermanent") || !filter_has_relation(&expression["where"])
    {
        return false;
    }
    let mut rest = expression.clone();
    if let Some(fields) = rest.as_object_mut() {
        fields.remove("where");
    }
    filter_supported_with_relations(&expression["where"], permanent_filter_supported)
        && object_expression_supported(&rest)
}

/// The values of one characteristic an object has (CR 105.1, 205.2, 205.3).
/// `all_creature_types` is set for an object with every creature type
/// (changeling, CR 702.73a).
#[derive(Default)]
struct CharacteristicValues {
    values: BTreeSet<String>,
    all_creature_types: bool,
}

fn has_type_word(type_line: &str, word: &str) -> bool {
    let types = type_line.split('\u{2014}').next().unwrap_or(type_line);
    let types = types.split(" - ").next().unwrap_or(types);
    types
        .split(|character: char| !character.is_alphanumeric())
        .any(|part| part.eq_ignore_ascii_case(word))
}

/// One characteristic of a definition: its card types, its creature types
/// (the subtypes of a creature or Kindred object, CR 205.3m), its land types
/// (the subtypes of a land, CR 205.3i) or its colors.
fn definition_values(
    definition: &CardDefinition,
    colors: BTreeSet<String>,
    changeling: bool,
    characteristic: &str,
) -> CharacteristicValues {
    let type_line = definition.type_line.as_str();
    let subtypes = || {
        type_line_subtypes(type_line)
            .split_whitespace()
            .map(|word| word.to_ascii_lowercase())
            .collect::<BTreeSet<_>>()
    };
    match characteristic {
        "cardType" | "permanentType" => {
            let types: &[&str] = if characteristic == "cardType" {
                &CARD_TYPES
            } else {
                &PERMANENT_CARD_TYPES
            };
            CharacteristicValues {
                values: types
                    .iter()
                    .filter(|word| has_type_word(type_line, word))
                    .map(|word| word.to_ascii_lowercase())
                    .collect(),
                all_creature_types: false,
            }
        }
        "creatureType" => {
            let creature_like =
                has_type_word(type_line, "Creature") || has_type_word(type_line, "Kindred");
            CharacteristicValues {
                values: if creature_like {
                    subtypes()
                } else {
                    BTreeSet::new()
                },
                all_creature_types: changeling,
            }
        }
        "landType" => CharacteristicValues {
            values: if has_type_word(type_line, "Land") {
                subtypes()
            } else {
                BTreeSet::new()
            },
            all_creature_types: false,
        },
        "color" => CharacteristicValues {
            values: colors,
            all_creature_types: false,
        },
        _ => CharacteristicValues::default(),
    }
}

/// Whether two value sets share a value. An object with every creature type
/// shares one with any object that has a creature type (CR 702.73a).
fn values_shared(candidate: &CharacteristicValues, anchor: &CharacteristicValues) -> bool {
    (candidate.all_creature_types && (anchor.all_creature_types || !anchor.values.is_empty()))
        || (anchor.all_creature_types && !candidate.values.is_empty())
        || candidate
            .values
            .iter()
            .any(|value| anchor.values.contains(value))
}

impl GameEngine {
    fn permanent_values(
        &self,
        permanent: &CardInstance,
        characteristic: &str,
    ) -> CharacteristicValues {
        definition_values(
            &permanent.definition,
            self.permanent_color_symbols(permanent),
            self.permanent_has_keyword(permanent, "changeling"),
            characteristic,
        )
    }

    fn card_values(card: &CardInstance, characteristic: &str) -> CharacteristicValues {
        definition_values(
            &card.definition,
            card_color_symbols(&card.definition),
            has_keyword(&card.definition, "changeling"),
            characteristic,
        )
    }

    /// The values of an anchor object: on the battlefield, else the card it
    /// is in another zone. `None` when no such object exists.
    fn object_values(
        &self,
        instance_id: &str,
        characteristic: &str,
    ) -> Option<CharacteristicValues> {
        if let Some((player, index)) = self.permanent_position(instance_id) {
            return Some(self.permanent_values(
                &self.state.players[player].battlefield[index],
                characteristic,
            ));
        }
        self.card_outside_battlefield(instance_id)
            .map(|card| Self::card_values(card, characteristic))
    }

    /// The objects an anchor names at resolution. A bound object that is no
    /// longer anywhere names no object; a share then reads its last known
    /// information or fails (see `prepare_object_relation`).
    fn relation_anchor_ids(
        &self,
        anchor: &Value,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<Vec<String>, EngineError> {
        Ok(match value_kind(anchor) {
            Some("self" | "abilitySource") => self
                .permanent_position(&source.card.instance_id)
                .map(|_| vec![source.card.instance_id.clone()])
                .unwrap_or_default(),
            Some("chosenTargets") => {
                let id = anchor["id"]
                    .as_str()
                    .ok_or_else(|| EngineError::new("relation anchor target set has no id"))?;
                let prefix = format!("{id}:");
                source
                    .targets
                    .iter()
                    .filter(|(target_id, target)| {
                        (target_id.as_str() == id || target_id.starts_with(&prefix))
                            && self.target_is_legal_for_stack_object(target_id, target, source)
                    })
                    .filter_map(|(_, target)| match target {
                        TargetRef::Permanent { instance_id } => Some(instance_id.clone()),
                        _ => None,
                    })
                    .collect()
            }
            Some("boundObjects") => anchor["binding"]
                .as_str()
                .and_then(|binding| bindings.get(binding))
                .and_then(|binding| match binding {
                    RuntimeBinding::Objects(objects) => Some(objects.clone()),
                    _ => None,
                })
                .ok_or_else(|| EngineError::new("relation anchor binding is not bound"))?,
            Some("eachPermanent") => {
                let filter = match anchor.get("where") {
                    Some(filter) if !filter.is_null() => {
                        self.prepare_runtime_object_filter(filter, source, bindings, decisions)?
                    }
                    _ => Value::Null,
                };
                let players = anchor
                    .get("player")
                    .map(|player| self.resolve_players(player, source, bindings));
                self.state
                    .players
                    .iter()
                    .flat_map(|player| player.battlefield.iter())
                    .filter(|permanent| !permanent.flags.get("phasedOut").copied().unwrap_or(false))
                    .filter(|permanent| {
                        players
                            .as_ref()
                            .is_none_or(|players| players.contains(&permanent.controller))
                    })
                    .filter(|permanent| {
                        filter.is_null()
                            || self.runtime_object_filter_matches(
                                ObjectFilterView::Permanent(permanent),
                                &filter,
                                source,
                                bindings,
                                decisions,
                            )
                    })
                    .map(|permanent| permanent.instance_id.clone())
                    .collect()
            }
            _ => match self.resolve_target(anchor, source, bindings) {
                Some(TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id }) => {
                    vec![instance_id]
                }
                _ => Vec::new(),
            },
        })
    }

    /// Resolves the anchor of one relation atom for the resolving ability
    /// `source` (CR 608.2b). The prepared atom carries `anchorIds` and, for
    /// a share, `anchorValues` (and `anchorAllCreatureTypes`).
    pub(super) fn prepare_object_relation(
        &self,
        atom: &Value,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<Value, EngineError> {
        if !relation_atom_supported(atom) {
            return Err(EngineError::new("unsupported object relation atom"));
        }
        let anchor = &atom["object"];
        let anchor_ids = self.relation_anchor_ids(anchor, source, bindings, decisions)?;
        let mut prepared = atom.clone();
        if value_kind(atom) == Some("sharesCharacteristic") {
            let characteristic = atom["characteristic"].as_str().unwrap_or_default();
            let mut shared = CharacteristicValues::default();
            let mut read = false;
            for id in &anchor_ids {
                if let Some(values) = self.object_values(id, characteristic) {
                    read = true;
                    shared.values.extend(values.values);
                    shared.all_creature_types |= values.all_creature_types;
                }
            }
            if !read && matches!(value_kind(anchor), Some("self" | "abilitySource")) {
                // The source's last known information (CR 608.2h): the card
                // the resolving ability carries.
                read = true;
                shared = Self::card_values(&source.card, characteristic);
            }
            // A query anchor may name no permanent: then nothing shares with
            // it. A bound anchor with no object and no last known
            // information is not read as an empty set.
            if !read && value_kind(anchor) != Some("eachPermanent") {
                return Err(EngineError::new(
                    "the shared characteristic's anchor has no object or last known information",
                ));
            }
            prepared["anchorValues"] = json!(shared.values);
            prepared["anchorAllCreatureTypes"] = json!(shared.all_creature_types);
        }
        prepared["anchorIds"] = json!(anchor_ids);
        Ok(prepared)
    }

    /// Prepares only the relation leaves of `filter` for the resolving
    /// ability, for a consumer whose own matcher reads every other leaf
    /// (`permanent_matches_effective_filter` evaluates a prepared relation).
    pub(super) fn prepare_relation_leaves(
        &self,
        filter: &Value,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Result<Value, EngineError> {
        if !filter_has_relation(filter) {
            return Ok(filter.clone());
        }
        let decisions = BTreeMap::new();
        super::object_predicate::try_map_predicate_leaves(filter, 0, &mut |leaf| match value_kind(
            leaf,
        ) {
            Some(kind) if is_relation_kind(kind) => {
                self.prepare_object_relation(leaf, source, bindings, &decisions)
            }
            _ => Ok(leaf.clone()),
        })
    }

    /// The controllers a static anchor query names relative to the source's
    /// controller: every player, the controller, or the controller's
    /// opponents. `None` for any other player reference.
    fn static_anchor_controllers(
        &self,
        player: Option<&Value>,
        controller: &str,
    ) -> Option<Vec<String>> {
        let source_controller = |reference: &Value| {
            value_kind(reference) == Some("controllerOf")
                && matches!(
                    value_kind(&reference["object"]),
                    Some("self" | "abilitySource")
                )
        };
        let players = self.state.players.iter().filter(|player| !player.has_lost);
        Some(match player {
            None => players.map(|player| player.id.clone()).collect(),
            Some(reference) if source_controller(reference) => vec![controller.to_string()],
            Some(reference)
                if value_kind(reference) == Some("opponentsOf")
                    && source_controller(&reference["player"]) =>
            {
                players
                    .filter(|player| player.id != controller)
                    .map(|player| player.id.clone())
                    .collect()
            }
            Some(_) => return None,
        })
    }

    /// Resolves the anchor of a relation for a static source, which has no
    /// resolving ability: the source itself, the permanent it is attached
    /// to, or a query whose filter and controllers need no binding (`attached
    /// to a creature`). Any other anchor is `None` (undefined, never empty).
    fn prepare_object_relation_for_source(
        &self,
        atom: &Value,
        source: Option<&CardInstance>,
    ) -> Option<Value> {
        if !relation_atom_supported(atom) {
            return None;
        }
        let source = source?;
        let anchor = &atom["object"];
        let anchor_ids = match value_kind(anchor) {
            Some("self" | "abilitySource") => vec![source.instance_id.clone()],
            Some("attachedPermanent") => source.attached_to.clone().into_iter().collect(),
            Some("eachPermanent") => {
                let filter = anchor.get("where").cloned().unwrap_or(Value::Null);
                if filter_has_relation(&filter) {
                    return None;
                }
                let controllers =
                    self.static_anchor_controllers(anchor.get("player"), &source.controller)?;
                self.state
                    .players
                    .iter()
                    .flat_map(|player| player.battlefield.iter())
                    .filter(|permanent| !permanent.flags.get("phasedOut").copied().unwrap_or(false))
                    .filter(|permanent| controllers.contains(&permanent.controller))
                    .filter(|permanent| {
                        filter.is_null() || self.permanent_state_matches(permanent, &filter)
                    })
                    .map(|permanent| permanent.instance_id.clone())
                    .collect()
            }
            _ => return None,
        };
        let mut prepared = atom.clone();
        if value_kind(atom) == Some("sharesCharacteristic") {
            let characteristic = atom["characteristic"].as_str().unwrap_or_default();
            let mut shared = CharacteristicValues::default();
            for id in &anchor_ids {
                let values = self.object_values(id, characteristic)?;
                shared.values.extend(values.values);
                shared.all_creature_types |= values.all_creature_types;
            }
            prepared["anchorValues"] = json!(shared.values);
            prepared["anchorAllCreatureTypes"] = json!(shared.all_creature_types);
        }
        prepared["anchorIds"] = json!(anchor_ids);
        Some(prepared)
    }

    /// Tests one candidate against a prepared relation atom. An unprepared
    /// atom (no `anchorIds`) never matches; its consumers refuse it first.
    pub(super) fn eval_object_relation(
        &self,
        relation: &Value,
        candidate: ObjectFilterView<'_>,
    ) -> bool {
        let Some(anchor_ids) = relation["anchorIds"].as_array() else {
            return false;
        };
        let anchored = |id: &str| anchor_ids.iter().any(|anchor| anchor.as_str() == Some(id));
        match (value_kind(relation), candidate) {
            (Some("attachedTo"), ObjectFilterView::Permanent(permanent)) => permanent
                .attached_to
                .as_deref()
                .is_some_and(|attached| anchored(attached)),
            (Some("blocking"), ObjectFilterView::Permanent(permanent)) => {
                self.state.combat.blockers.iter().any(|block| {
                    block.blocker_id == permanent.instance_id && anchored(&block.attacker_id)
                })
            }
            (Some("blockedBy"), ObjectFilterView::Permanent(permanent)) => {
                self.state.combat.blockers.iter().any(|block| {
                    block.attacker_id == permanent.instance_id && anchored(&block.blocker_id)
                })
            }
            // CR 702.51c: the creatures tapped to pay the convoke cost of
            // the anchor, recorded with the anchor's latest cast.
            (Some("convoked"), ObjectFilterView::Permanent(permanent)) => {
                anchor_ids.iter().filter_map(Value::as_str).any(|anchor| {
                    self.state
                        .events
                        .iter()
                        .rev()
                        .find(|event| {
                            event.kind == "spellCast"
                                && event.card_instance_id.as_deref() == Some(anchor)
                        })
                        .is_some_and(|cast| {
                            cast.detail["decisions"]["convokeSources"]
                                .as_array()
                                .is_some_and(|sources| {
                                    sources.iter().any(|source| {
                                        source.as_str() == Some(permanent.instance_id.as_str())
                                    })
                                })
                        })
                })
            }
            (Some("sharesCharacteristic"), view) => {
                let characteristic = relation["characteristic"].as_str().unwrap_or_default();
                let anchor = CharacteristicValues {
                    values: relation["anchorValues"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect(),
                    all_creature_types: relation["anchorAllCreatureTypes"].as_bool() == Some(true),
                };
                let candidate = match view {
                    ObjectFilterView::Permanent(permanent) => {
                        self.permanent_values(permanent, characteristic)
                    }
                    ObjectFilterView::Definition(definition) => definition_values(
                        definition.definition(),
                        card_color_symbols(definition.definition()),
                        has_keyword(definition.definition(), "changeling"),
                        characteristic,
                    ),
                };
                values_shared(&candidate, &anchor)
            }
            _ => false,
        }
    }

    /// Prepares every relation leaf of `filter` for a static source.
    fn prepare_relations_for_source(
        &self,
        filter: &Value,
        source: Option<&CardInstance>,
    ) -> Option<Value> {
        match value_kind(filter) {
            Some(kind) if is_relation_kind(kind) => {
                self.prepare_object_relation_for_source(filter, source)
            }
            Some("and" | "or") => {
                let mut prepared = filter.clone();
                prepared["operands"] = Value::Array(
                    filter["operands"]
                        .as_array()?
                        .iter()
                        .map(|operand| self.prepare_relations_for_source(operand, source))
                        .collect::<Option<Vec<_>>>()?,
                );
                Some(prepared)
            }
            Some("not") => {
                let mut prepared = filter.clone();
                prepared["operand"] =
                    self.prepare_relations_for_source(&filter["operand"], source)?;
                Some(prepared)
            }
            _ => Some(filter.clone()),
        }
    }

    /// Matches a permanent against a filter whose relation leaves are
    /// prepared; every other leaf is the shared permanent-state matcher.
    fn permanent_matches_related_filter(&self, permanent: &CardInstance, filter: &Value) -> bool {
        match value_kind(filter) {
            Some(kind) if is_relation_kind(kind) => {
                self.eval_object_relation(filter, ObjectFilterView::Permanent(permanent))
            }
            Some("and") => filter["operands"].as_array().is_some_and(|operands| {
                operands
                    .iter()
                    .all(|operand| self.permanent_matches_related_filter(permanent, operand))
            }),
            Some("or") => filter["operands"].as_array().is_some_and(|operands| {
                operands
                    .iter()
                    .any(|operand| self.permanent_matches_related_filter(permanent, operand))
            }),
            Some("not") => !self.permanent_matches_related_filter(permanent, &filter["operand"]),
            _ => self.permanent_state_matches(permanent, filter),
        }
    }

    /// `countPermanents` over a prepared filter, with the count's player,
    /// ownership and source exclusion. `player_id` is the count's player.
    fn count_related_permanents(
        &self,
        expression: &Value,
        filter: &Value,
        player_id: &str,
        source_id: Option<&str>,
    ) -> Option<i32> {
        let all_players = expression["allPlayers"].as_bool() == Some(true);
        if !all_players {
            self.player_index(player_id).ok()?;
        }
        let count = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .filter(|player| all_players || player.id == player_id)
            .flat_map(|player| player.battlefield.iter())
            .filter(|permanent| !permanent.flags.get("phasedOut").copied().unwrap_or(false))
            .filter(|permanent| match expression["ownership"].as_str() {
                Some("owned") => permanent.owner == player_id,
                Some("notOwned") => permanent.owner != player_id,
                _ => true,
            })
            .filter(|permanent| {
                !(expression["excludeSource"].as_bool() == Some(true)
                    && source_id == Some(permanent.instance_id.as_str()))
            })
            .filter(|permanent| self.permanent_matches_related_filter(permanent, filter))
            .count();
        i32::try_from(count).ok()
    }

    /// A count of related permanents for the resolving ability. `None` when
    /// the expression is not such a count (the caller keeps its own path);
    /// `Some(None)` when its anchor cannot be resolved.
    pub(super) fn runtime_related_permanent_count(
        &self,
        expression: &Value,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Option<Option<i32>> {
        if value_kind(expression) != Some("countPermanents")
            || !filter_has_relation(&expression["where"])
        {
            return None;
        }
        let Ok(filter) =
            self.prepare_runtime_object_filter(&expression["where"], source, bindings, decisions)
        else {
            return Some(None);
        };
        let player_id = expression
            .get("player")
            .and_then(|player| self.resolve_player(player, source, bindings))
            .unwrap_or_else(|| source.controller.clone());
        Some(self.count_related_permanents(
            expression,
            &filter,
            &player_id,
            Some(source.card.instance_id.as_str()),
        ))
    }

    /// A count of related permanents for a static source (a characteristic-
    /// defining or continuous amount). Same contract as the runtime count.
    pub(super) fn related_permanent_count_with_source(
        &self,
        expression: &Value,
        controller_id: &str,
        source: Option<&CardInstance>,
    ) -> Option<Option<i32>> {
        if value_kind(expression) != Some("countPermanents")
            || !filter_has_relation(&expression["where"])
        {
            return None;
        }
        // Only a count of the source controller's permanents (or of every
        // player's) has a player here.
        if expression.get("player").is_some_and(|player| {
            !(value_kind(player) == Some("controllerOf")
                && matches!(
                    value_kind(&player["object"]),
                    Some("self" | "abilitySource")
                ))
        }) {
            return Some(None);
        }
        let Some(filter) = self.prepare_relations_for_source(&expression["where"], source) else {
            return Some(None);
        };
        Some(self.count_related_permanents(
            expression,
            &filter,
            controller_id,
            source.map(|source| source.instance_id.as_str()),
        ))
    }
}
