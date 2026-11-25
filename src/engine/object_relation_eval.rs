//! Relation atoms of the shared object query: a candidate related to another
//! object, its anchor (`attachedTo`, `blocking`, `blockedBy`, `convoked`,
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
//! validator (`filter_supported_with_relations`): collection destruction,
//! damage to each permanent, counters on each permanent, permanent counts,
//! and declared-target filters whose anchor the source binds
//! (`bind_source_relations`). An unprepared atom never matches. Counts of related permanents are evaluated here too, for the
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
        "attachedTo"
            | "blocking"
            | "blockedBy"
            | "convoked"
            | "sharesCharacteristic"
            | ENTERED_UNDER_CONTROL
            | ATTACKING_PLAYER
            | BLOCKED_ANCHOR_THIS_TURN
            | BLOCKED_BY_ANCHOR_THIS_TURN
            | DEALT_DAMAGE_TO_ANCHOR_THIS_TURN
            | DEALT_COMBAT_DAMAGE_TO_ANCHOR_THIS_TURN
    )
}

/// `that dealt damage to that creature this turn` (CR 120.1): one of this
/// turn's damage records names the candidate as the source and an anchor
/// as the damaged permanent, with a positive amount (CR 120.8: zero damage
/// is not dealt).
const DEALT_DAMAGE_TO_ANCHOR_THIS_TURN: &str = "dealtDamageToAnchorThisTurn";

/// The same record restricted to combat damage (CR 510.2).
const DEALT_COMBAT_DAMAGE_TO_ANCHOR_THIS_TURN: &str = "dealtCombatDamageToAnchorThisTurn";

/// `that blocked it this turn` (CR 509.1): one of this turn's block
/// declarations pairs this incarnation of the candidate, as the blocker,
/// with an anchor as the attacker. Read from the declarations, so a pair
/// whose creatures left combat afterwards still counts.
const BLOCKED_ANCHOR_THIS_TURN: &str = "blockedAnchorThisTurn";

/// `that was blocked by it this turn` (CR 509.1, 509.1h): the same record
/// with the candidate as the attacker and an anchor as the blocker.
const BLOCKED_BY_ANCHOR_THIS_TURN: &str = "blockedByAnchorThisTurn";

/// `that entered the battlefield under your control this turn` (CR
/// 110.2a): a relation of the permanent to the player who controlled it as
/// it entered. Its `player` is a role relative to the ability controller,
/// resolved into `playerIds` at preparation; it has no object anchor.
const ENTERED_UNDER_CONTROL: &str = "enteredUnderControlThisTurn";

/// `attacking you [or a planeswalker you control]` (CR 508.1b, 506.3): a
/// relation of an attacking permanent to the player it attacks, or, with
/// `orPlaneswalkers`, to a planeswalker that player controls. Its `player`
/// is a role relative to the ability controller, resolved into `playerIds`
/// at preparation; it has no object anchor.
const ATTACKING_PLAYER: &str = "attackingPlayer";

/// The relations whose second argument is a player role, not an object.
fn player_role_relation(kind: Option<&str>) -> bool {
    matches!(kind, Some(ENTERED_UNDER_CONTROL | ATTACKING_PLAYER))
}

/// The player roles an entry-controller relation names.
fn entry_player_role_supported(role: &Value) -> bool {
    matches!(
        role.as_str(),
        Some("abilityController" | "opponentOfAbilityController")
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
        _ => super::described_object_anchor::described_anchor_supported(anchor),
    }
}

/// One unprepared relation atom: a known relation and characteristic, and
/// an anchor this module resolves.
pub(super) fn relation_atom_supported(atom: &Value) -> bool {
    if value_kind(atom) == Some(ENTERED_UNDER_CONTROL) {
        return entry_player_role_supported(&atom["player"]);
    }
    if value_kind(atom) == Some(ATTACKING_PLAYER) {
        return entry_player_role_supported(&atom["player"])
            && atom.get("orPlaneswalkers").is_none_or(Value::is_boolean);
    }
    let kind = match value_kind(atom) {
        Some(
            "attachedTo"
            | "blocking"
            | "blockedBy"
            | "convoked"
            | BLOCKED_ANCHOR_THIS_TURN
            | BLOCKED_BY_ANCHOR_THIS_TURN
            | DEALT_DAMAGE_TO_ANCHOR_THIS_TURN
            | DEALT_COMBAT_DAMAGE_TO_ANCHOR_THIS_TURN,
        ) => true,
        Some("sharesCharacteristic") => matches!(
            atom["characteristic"].as_str(),
            Some(
                "creatureType"
                    | "cardType"
                    | "permanentType"
                    | "landType"
                    | "color"
                    | "name"
                    | "manaValue"
            )
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

/// An anchor a declared target's filter can bind from the ability source
/// alone, when targets are chosen and again on resolution (CR 601.2c,
/// 608.2b): the source, the permanent it is attached to, or a query whose
/// filter and controllers need no binding.
fn source_bindable_anchor(anchor: &Value) -> bool {
    let source_controller = |reference: &Value| {
        value_kind(reference) == Some("controllerOf")
            && matches!(
                value_kind(&reference["object"]),
                Some("self" | "abilitySource")
            )
    };
    match value_kind(anchor) {
        Some("self" | "abilitySource") => true,
        Some("attachedPermanent") => value_kind(&anchor["attachment"]) == Some("self"),
        Some("eachPermanent") => {
            anchor.get("where").is_none_or(|filter| {
                !filter_has_relation(filter) && permanent_filter_supported(filter)
            }) && anchor.get("player").is_none_or(|player| {
                source_controller(player)
                    || (value_kind(player) == Some("opponentsOf")
                        && source_controller(&player["player"]))
            })
        }
        _ => false,
    }
}

/// A declared target's candidate filter: relation leaves whose anchor the
/// source binds (`bind_source_relations`), every other leaf the shared
/// permanent filter.
pub(super) fn target_filter_supported_with_relations(filter: &Value) -> bool {
    match value_kind(filter) {
        Some(kind) if is_relation_kind(kind) => {
            relation_atom_supported(filter)
                && (player_role_relation(Some(kind)) || source_bindable_anchor(&filter["object"]))
        }
        Some("and" | "or") => filter["operands"].as_array().is_some_and(|operands| {
            !operands.is_empty() && operands.iter().all(target_filter_supported_with_relations)
        }),
        Some("not") => target_filter_supported_with_relations(&filter["operand"]),
        _ => permanent_filter_supported(filter),
    }
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
        // CR 201.2: two objects have the same name when a name of one is a
        // name of the other; an object without a name has none in common
        // (CR 708.2a face-down). A split card has both its names.
        "name" => CharacteristicValues {
            values: definition
                .name
                .split(" // ")
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
            all_creature_types: false,
        },
        // CR 202.3: the mana value of the object's mana cost.
        "manaValue" => CharacteristicValues {
            values: BTreeSet::from([super::mana_value(definition).to_string()]),
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

/// Whether every member of a set has one value of the characteristic in
/// common (CR 105.1, 205.2-205.3): an object with every creature type has
/// each creature type a member names (CR 702.73a). A set whose members all
/// have every creature type shares one; an empty set shares nothing.
fn values_common(members: &[CharacteristicValues]) -> bool {
    let specific = members
        .iter()
        .filter(|member| !member.all_creature_types)
        .collect::<Vec<_>>();
    match specific.split_first() {
        None => !members.is_empty(),
        Some((first, rest)) => first
            .values
            .iter()
            .any(|value| rest.iter().all(|other| other.values.contains(value))),
    }
}

/// The characteristics a set share can compare (the relation's own list).
pub(super) fn shared_set_characteristic_supported(characteristic: Option<&str>) -> bool {
    matches!(
        characteristic,
        Some(
            "creatureType"
                | "cardType"
                | "permanentType"
                | "landType"
                | "color"
                | "name"
                | "manaValue"
        )
    )
}

impl GameEngine {
    /// `two target creature cards that share a creature type`: the members
    /// of a chosen set, each read where it is now (a permanent's effective
    /// colors, a card's printed characteristics), have one value of the
    /// characteristic in common. A member that no longer exists fails it.
    pub(super) fn selection_shares_characteristic(
        &self,
        selection: &[String],
        characteristic: &str,
    ) -> bool {
        selection
            .iter()
            .map(|id| self.object_values(id, characteristic))
            .collect::<Option<Vec<_>>>()
            .is_some_and(|members| values_common(&members))
    }

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

    /// A relation anchored on recorded or linked cards
    /// (`described_object_anchor.rs`): their ids and, for a share, the
    /// union of their values of the characteristic, read from the cards
    /// themselves (a cost record is the object as it was paid, CR 608.2h).
    fn prepare_relation_on_cards(atom: &Value, cards: &[CardInstance]) -> Value {
        let mut prepared = atom.clone();
        if value_kind(atom) == Some("sharesCharacteristic") {
            let characteristic = atom["characteristic"].as_str().unwrap_or_default();
            let mut shared = CharacteristicValues::default();
            for card in cards {
                let values = Self::card_values(card, characteristic);
                shared.values.extend(values.values);
                shared.all_creature_types |= values.all_creature_types;
            }
            prepared["anchorValues"] = json!(shared.values);
            prepared["anchorAllCreatureTypes"] = json!(shared.all_creature_types);
        }
        prepared["anchorIds"] = json!(
            cards
                .iter()
                .map(|card| card.instance_id.clone())
                .collect::<Vec<_>>()
        );
        prepared
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
            // The source wherever it is: a permanent's attachments and
            // blocks name it on the battlefield; a resolving spell is the
            // object its own convoke record names (CR 702.51c).
            Some("self" | "abilitySource") => vec![source.card.instance_id.clone()],
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
        if player_role_relation(value_kind(atom)) {
            return self
                .prepare_entry_controller_relation(atom, &source.controller)
                .ok_or_else(|| EngineError::new("unsupported relation player role"));
        }
        let anchor = &atom["object"];
        if let Some(cards) = self.described_anchor_cards_for_stack(anchor, source) {
            return Ok(Self::prepare_relation_on_cards(atom, &cards?));
        }
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
        if player_role_relation(value_kind(atom)) {
            return self.prepare_entry_controller_relation(atom, &source.controller);
        }
        let anchor = &atom["object"];
        if let Some(cards) = self.described_anchor_cards_for_source(anchor, source) {
            return Some(Self::prepare_relation_on_cards(atom, &cards));
        }
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

    /// The block pairs of an anchor that left the battlefield (CR 506.4: it
    /// was removed from combat), read from this turn's block declarations as
    /// its last known information (CR 608.2h). The candidate must still be
    /// the incarnation that blocked (`candidate_blocks`) or was blocked, and
    /// still be in combat.
    fn blocked_departed_anchor(
        &self,
        candidate: &CardInstance,
        anchor_ids: &[Value],
        candidate_blocks: bool,
    ) -> bool {
        let in_combat = if candidate_blocks {
            candidate.flags.get("blocking").copied().unwrap_or(false)
        } else {
            candidate.flags.get("attacking").copied().unwrap_or(false)
        };
        if !in_combat {
            return false;
        }
        let candidate_identity = self.object_identity_value(&candidate.instance_id);
        anchor_ids
            .iter()
            .filter_map(Value::as_str)
            .filter(|anchor| self.permanent_position(anchor).is_none())
            .any(|anchor| {
                self.state.events.iter().any(|event| {
                    if event.turn_number != self.state.turn_number
                        || event.kind != "blockerDeclared"
                    {
                        return false;
                    }
                    let attacker = event.detail["attackerId"].as_str();
                    let blocker = event.card_instance_id.as_deref();
                    let (pair_candidate, pair_anchor, identity_field) = if candidate_blocks {
                        (blocker, attacker, "objectIdentity")
                    } else {
                        (attacker, blocker, "attackerIdentity")
                    };
                    pair_candidate == Some(candidate.instance_id.as_str())
                        && pair_anchor == Some(anchor)
                        && event.detail.get(identity_field).is_none_or(|recorded| {
                            recorded.is_null() || *recorded == candidate_identity
                        })
                })
            })
    }

    /// Whether this turn's block declarations pair this incarnation of
    /// `candidate` with one of `anchor_ids` (CR 509.1): as the blocker when
    /// `candidate_blocked`, else as the blocked attacker. Every declaration
    /// of the turn counts, including blocks whose creatures left combat.
    fn block_history_with_anchor(
        &self,
        candidate: &CardInstance,
        anchor_ids: &[Value],
        candidate_blocked: bool,
    ) -> bool {
        let candidate_identity = self.object_identity_value(&candidate.instance_id);
        self.state.events.iter().any(|event| {
            if event.turn_number != self.state.turn_number || event.kind != "blockerDeclared" {
                return false;
            }
            let attacker = event.detail["attackerId"].as_str();
            let blocker = event.card_instance_id.as_deref();
            let (pair_candidate, pair_anchor, identity_field) = if candidate_blocked {
                (blocker, attacker, "objectIdentity")
            } else {
                (attacker, blocker, "attackerIdentity")
            };
            pair_candidate == Some(candidate.instance_id.as_str())
                && pair_anchor
                    .is_some_and(|anchor| anchor_ids.iter().any(|id| id.as_str() == Some(anchor)))
                && event
                    .detail
                    .get(identity_field)
                    .is_none_or(|recorded| recorded.is_null() || *recorded == candidate_identity)
        })
    }

    /// Binds the relation leaves of a candidate domain's filter (each set of
    /// a `union`) to `source`. `None` when an anchor cannot be bound; the
    /// leaf then stays unprepared and matches no candidate.
    pub(super) fn bind_source_relations(
        &self,
        candidates: &mut Value,
        source: &CardInstance,
    ) -> Option<()> {
        fn bind(engine: &GameEngine, filter: &mut Value, source: &CardInstance) -> Option<()> {
            match value_kind(filter) {
                Some(kind) if is_relation_kind(kind) => {
                    *filter = engine.prepare_object_relation_for_source(filter, Some(source))?;
                }
                Some("and" | "or") => {
                    for operand in filter["operands"].as_array_mut()? {
                        bind(engine, operand, source)?;
                    }
                }
                Some("not") => bind(engine, &mut filter["operand"], source)?,
                _ => {}
            }
            Some(())
        }
        if value_kind(candidates) == Some("union") {
            for set in candidates["sets"].as_array_mut()? {
                self.bind_source_relations(set, source)?;
            }
            return Some(());
        }
        match candidates.get_mut("where") {
            Some(filter) if filter_has_relation(filter) => bind(self, filter, source),
            _ => Some(()),
        }
    }

    /// The entry-controller relation with its role resolved relative to
    /// `controller` (the ability controller): `playerIds` lists the players
    /// it names. A role this relation does not name is `None`.
    fn prepare_entry_controller_relation(&self, atom: &Value, controller: &str) -> Option<Value> {
        let players = self.state.players.iter();
        let ids: Vec<String> = match atom["player"].as_str()? {
            "abilityController" => vec![controller.to_string()],
            "opponentOfAbilityController" => players
                .filter(|player| player.id != controller)
                .map(|player| player.id.clone())
                .collect(),
            _ => return None,
        };
        let mut prepared = atom.clone();
        prepared["playerIds"] = json!(ids);
        prepared["anchorIds"] = json!([]);
        Some(prepared)
    }

    /// CR 110.2a, 400.7: this incarnation of the permanent entered the
    /// battlefield this turn, and its controller as it entered (the player
    /// its latest entry record of this turn names) is one of `player_ids`.
    fn permanent_entered_under_control_this_turn(
        &self,
        permanent: &CardInstance,
        player_ids: &[Value],
    ) -> bool {
        if !permanent
            .flags
            .get("enteredThisTurn")
            .copied()
            .unwrap_or(false)
        {
            return false;
        }
        self.state
            .events
            .iter()
            .rev()
            .take_while(|event| event.turn_number == self.state.turn_number)
            .find(|event| {
                event.kind == "permanentEnteredBattlefield"
                    && event.card_instance_id.as_deref() == Some(permanent.instance_id.as_str())
            })
            .and_then(|event| event.player_id.as_deref())
            .is_some_and(|entry_controller| {
                player_ids
                    .iter()
                    .any(|player| player.as_str() == Some(entry_controller))
            })
    }

    /// CR 508.1b, 506.3: the permanent is an attacking creature of the
    /// current combat whose defender is one of `player_ids` or, when
    /// `planeswalkers`, a planeswalker one of them controls. A creature
    /// attacking a planeswalker is not attacking that planeswalker's
    /// controller.
    fn permanent_attacks_player(
        &self,
        permanent: &CardInstance,
        player_ids: &[Value],
        planeswalkers: bool,
    ) -> bool {
        let named = |id: &str| player_ids.iter().any(|player| player.as_str() == Some(id));
        self.state.combat.attackers.iter().any(|attacker| {
            attacker.attacker_id == permanent.instance_id
                && match &attacker.defender {
                    TargetRef::Player { player_id } => named(player_id.as_str()),
                    TargetRef::Permanent { instance_id } if planeswalkers => self
                        .permanent_position(instance_id)
                        .is_some_and(|(player_index, card_index)| {
                            let defender =
                                &self.state.players[player_index].battlefield[card_index];
                            named(defender.controller.as_str())
                                && self.permanent_has_card_type(defender, "Planeswalker")
                        }),
                    _ => false,
                }
        })
    }

    /// Tests one candidate against a prepared relation atom. An unprepared
    /// atom (no `anchorIds`) never matches; its consumers refuse it first.
    pub(super) fn eval_object_relation(
        &self,
        relation: &Value,
        candidate: ObjectFilterView<'_>,
    ) -> bool {
        if value_kind(relation) == Some(ENTERED_UNDER_CONTROL) {
            return match (relation["playerIds"].as_array(), candidate) {
                (Some(player_ids), ObjectFilterView::Permanent(permanent)) => {
                    self.permanent_entered_under_control_this_turn(permanent, player_ids)
                }
                _ => false,
            };
        }
        if value_kind(relation) == Some(ATTACKING_PLAYER) {
            return match (relation["playerIds"].as_array(), candidate) {
                (Some(player_ids), ObjectFilterView::Permanent(permanent)) => self
                    .permanent_attacks_player(
                        permanent,
                        player_ids,
                        relation["orPlaneswalkers"].as_bool() == Some(true),
                    ),
                _ => false,
            };
        }
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
                }) || self.blocked_departed_anchor(permanent, anchor_ids, true)
            }
            (Some("blockedBy"), ObjectFilterView::Permanent(permanent)) => {
                self.state.combat.blockers.iter().any(|block| {
                    block.attacker_id == permanent.instance_id && anchored(&block.blocker_id)
                }) || self.blocked_departed_anchor(permanent, anchor_ids, false)
            }
            (Some(BLOCKED_ANCHOR_THIS_TURN), ObjectFilterView::Permanent(permanent)) => {
                self.block_history_with_anchor(permanent, anchor_ids, true)
            }
            (Some(BLOCKED_BY_ANCHOR_THIS_TURN), ObjectFilterView::Permanent(permanent)) => {
                self.block_history_with_anchor(permanent, anchor_ids, false)
            }
            (
                Some(
                    kind @ (DEALT_DAMAGE_TO_ANCHOR_THIS_TURN
                    | DEALT_COMBAT_DAMAGE_TO_ANCHOR_THIS_TURN),
                ),
                ObjectFilterView::Permanent(permanent),
            ) => self.state.events.iter().any(|event| {
                event.turn_number == self.state.turn_number
                    && event.kind == "damageDealt"
                    && event.detail["source"].as_str() == Some(permanent.instance_id.as_str())
                    && event
                        .card_instance_id
                        .as_deref()
                        .is_some_and(|damaged| anchored(damaged))
                    && event.detail["amount"]
                        .as_i64()
                        .is_some_and(|amount| amount > 0)
                    && (kind == DEALT_DAMAGE_TO_ANCHOR_THIS_TURN
                        || event.detail["combat"].as_bool() == Some(true))
            }),
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
    pub(super) fn prepare_relations_for_source(
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
