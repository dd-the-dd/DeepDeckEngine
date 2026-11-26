//! Object-predicate atoms of the shared object query that read game state
//! rather than an object's definition: combat block status and the turn's
//! event log, and the relation of a stack object to its own targets.
//!
//! The parser owns their grammar (`blocked creature`, `unblocked creature`,
//! `spell that targets a creature you control`) and lowers each one to a
//! typed atom; every engine consumer of such a filter evaluates it here
//! through one implementation. A consumer without that state (a static
//! selector matched without the engine, a spell filter read from a card
//! definition) refuses these atoms in its validator rather than reading
//! them as false.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::{
    CardInstance, GameEngine, GameStep, StackObject, TargetRef, integer_value, mana_value,
    matches_stack_filter, permanent_filter_supported, stack_filter_supported, value_kind,
};

/// `owned by PLAYER` (CR 108.3): a relation of a permanent to the player
/// who owns it, as in `each player gains control of all creatures they
/// own`. Its `player` is one singular player reference, resolved against
/// the resolving ability (so a performer's `you` is that performer) into
/// `playerIds` when the collection is prepared; it has no object anchor and
/// a static selector, which has no resolving ability, cannot prepare it.
pub(super) const OWNED_BY: &str = "ownedBy";

pub(super) fn owned_by_relation_supported(atom: &Value) -> bool {
    value_kind(atom) == Some(OWNED_BY)
        && super::singular_player_reference_supported(&atom["player"])
        && atom.as_object().is_some_and(|object| {
            object
                .keys()
                .all(|key| matches!(key.as_str(), "kind" | "player" | "playerIds" | "anchorIds"))
        })
}

/// `with the greatest|least PROPERTY among QUERY` (CR 107.1): a superlative
/// ranking of the candidate within a population of permanents. It reads the
/// whole battlefield, so it is prepared once before the candidates are
/// enumerated: `extremeIds` lists every member of the population holding the
/// extreme value (ties are all members, as the reminder text of Topple says).
/// The population is the query's permanents on the battlefield, those the
/// ability's controller controls, or, per candidate, those its own
/// controller controls. A member without the property (a noncreature's
/// power) is not ranked; an empty population has no member.
pub(super) const EXTREMUM_AMONG: &str = "extremumAmong";

pub(super) fn extremum_among_supported(atom: &Value) -> bool {
    value_kind(atom) == Some(EXTREMUM_AMONG)
        && matches!(
            atom["property"].as_str(),
            Some("power" | "toughness" | "manaValue")
        )
        && matches!(atom["extreme"].as_str(), Some("greatest" | "least"))
        && matches!(
            atom["population"].as_str(),
            Some("battlefield" | "abilityController" | "candidateController")
        )
        && (atom["where"].is_null() || permanent_filter_supported(&atom["where"]))
        && atom.as_object().is_some_and(|object| {
            object.keys().all(|key| {
                matches!(
                    key.as_str(),
                    "kind"
                        | "property"
                        | "extreme"
                        | "population"
                        | "where"
                        | "extremeIds"
                        | "anchorIds"
                )
            })
        })
}

/// Whether `kind` is a combat block-status atom this module evaluates.
pub(super) fn combat_block_status_kind(kind: &str) -> bool {
    matches!(kind, "isBlocked" | "isUnblockedAttacker")
}

/// A player role relative to the ability controller choosing candidates.
fn player_role_supported(role: &Value) -> bool {
    matches!(
        role.as_str(),
        Some("abilityController" | "opponentOfAbilityController" | "anyPlayer")
    )
}

/// A permanent controller role relative to the ability controller.
fn controller_role_supported(role: &Value) -> bool {
    matches!(
        role.as_str(),
        Some("abilityController" | "notAbilityController" | "opponentOfAbilityController")
    )
}

fn role_matches(role: Option<&str>, player_id: &str, controller_id: &str) -> bool {
    match role {
        Some("abilityController") => player_id == controller_id,
        // A target-candidate opponent is every other player, as for
        // `isOpponentOf` player candidates.
        Some("opponentOfAbilityController" | "notAbilityController") => player_id != controller_id,
        Some("anyPlayer") => true,
        _ => false,
    }
}

/// `{"kind":"targets", "players"?, "permanents"?}` (CR 115.1): at least one
/// alternative, each with a role or filter this module evaluates.
pub(super) fn targets_atom_supported(atom: &Value) -> bool {
    let players = atom.get("players");
    let permanents = atom.get("permanents");
    (players.is_some() || permanents.is_some())
        && players.is_none_or(player_role_supported)
        && permanents.is_none_or(|query| {
            query
                .get("controller")
                .is_none_or(controller_role_supported)
                && (query["where"].is_null() || permanent_filter_supported(&query["where"]))
        })
}

/// The `where` filter of a declared spell target: the stack filter of the
/// spell's characteristics, composed with the `targets` relation of the
/// spell to its own targets. Only the target-candidate consumer evaluates
/// the relation; other spell filters keep their stack-filter validator.
pub(super) fn spell_candidate_filter_supported(filter: &Value) -> bool {
    match value_kind(filter) {
        Some("targets") => targets_atom_supported(filter),
        Some("and" | "or") => filter["operands"].as_array().is_some_and(|operands| {
            !operands.is_empty() && operands.iter().all(spell_candidate_filter_supported)
        }),
        Some("not") => spell_candidate_filter_supported(&filter["operand"]),
        _ => stack_filter_supported(filter),
    }
}

/// One set constraint (`distinctValues` of names, powers or mana values;
/// `totalAtMost` of one numeric property with a literal bound).
pub(super) fn set_constraint_supported(constraint: &Value) -> bool {
    match value_kind(constraint) {
        Some("distinctValues") => matches!(
            constraint["property"].as_str(),
            Some("name" | "power" | "manaValue")
        ),
        Some("totalAtMost") => {
            SetProperty::from_ir(constraint["property"].as_str()).is_some()
                && integer_value(&constraint["value"]).is_some()
        }
        // `that share a creature type` among the chosen set.
        Some("sharedValue") => super::object_relation_eval::shared_set_characteristic_supported(
            constraint["characteristic"].as_str(),
        ),
        _ => false,
    }
}

/// `{ kind: allOf, constraints: [...] }`: every listed constraint, each
/// admitted by the consumer's own validator.
pub(super) fn all_of_supported(constraint: &Value, supported: fn(&Value) -> bool) -> bool {
    constraint["constraints"]
        .as_array()
        .is_some_and(|all| !all.is_empty() && all.iter().all(supported))
}

/// One numeric property of a selected object for a set constraint.
#[derive(Clone, Copy)]
enum SetProperty {
    Power,
    Toughness,
    ManaValue,
}

impl SetProperty {
    fn from_ir(raw: Option<&str>) -> Option<Self> {
        match raw? {
            "power" => Some(Self::Power),
            "toughness" => Some(Self::Toughness),
            "manaValue" => Some(Self::ManaValue),
            _ => None,
        }
    }
}

impl GameEngine {
    /// Prepares an `ownedBy` relation for the resolving ability: its player
    /// resolved now. An unresolved player is an error, never an empty owner.
    pub(super) fn prepare_owned_by_relation(
        &self,
        atom: &Value,
        source: &StackObject,
        bindings: &BTreeMap<String, super::RuntimeBinding>,
    ) -> Result<Value, super::EngineError> {
        let player_id = self
            .resolve_player(&atom["player"], source, bindings)
            .ok_or_else(|| {
                super::EngineError::new("the owner of an ownedBy relation is unresolved")
            })?;
        let mut prepared = atom.clone();
        prepared["playerIds"] = json!([player_id]);
        prepared["anchorIds"] = json!([]);
        Ok(prepared)
    }

    /// A prepared `ownedBy` relation: the permanent's owner is one of its
    /// players. An unprepared relation never matches.
    pub(super) fn permanent_owned_by_relation(
        &self,
        relation: &Value,
        permanent: &CardInstance,
    ) -> bool {
        relation["playerIds"].as_array().is_some_and(|players| {
            players
                .iter()
                .any(|player| player.as_str() == Some(permanent.owner.as_str()))
        })
    }

    /// One ranked property of a permanent; `None` when it has none (CR
    /// 208.3: a noncreature permanent has no power or toughness).
    fn extremum_property(&self, permanent: &CardInstance, property: &str) -> Option<i32> {
        match property {
            "manaValue" => Some(mana_value(&permanent.definition)),
            "power" if self.permanent_is_creature(permanent) => Some(self.current_power(permanent)),
            "toughness" if self.permanent_is_creature(permanent) => {
                Some(self.current_toughness(permanent))
            }
            _ => None,
        }
    }

    /// Prepares an `extremumAmong` atom relative to `controller_id`, the
    /// controller of the ability reading it. `None` for an unsupported atom.
    pub(super) fn prepare_extremum_among(
        &self,
        atom: &Value,
        controller_id: &str,
    ) -> Option<Value> {
        if !extremum_among_supported(atom) {
            return None;
        }
        let property = atom["property"].as_str()?;
        let greatest = atom["extreme"].as_str()? == "greatest";
        let population = atom["population"].as_str()?;
        let members = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| player.battlefield.iter())
            .filter(|permanent| !permanent.flags.get("phasedOut").copied().unwrap_or(false))
            .filter(|permanent| {
                population != "abilityController" || permanent.controller == controller_id
            })
            .filter(|permanent| {
                atom["where"].is_null()
                    || self.permanent_matches_effective_filter(permanent, &atom["where"])
            })
            .filter_map(|permanent| {
                self.extremum_property(permanent, property).map(|value| {
                    (
                        permanent.instance_id.clone(),
                        permanent.controller.clone(),
                        value,
                    )
                })
            })
            .collect::<Vec<_>>();
        // Each group ranks on its own: the whole population, or each
        // controller's permanents for `candidateController`.
        let group_of = |controller: &str| {
            if population == "candidateController" {
                controller.to_string()
            } else {
                String::new()
            }
        };
        let mut extremes = BTreeMap::<String, i32>::new();
        for (_, controller, value) in &members {
            let entry = extremes.entry(group_of(controller)).or_insert(*value);
            *entry = if greatest {
                (*entry).max(*value)
            } else {
                (*entry).min(*value)
            };
        }
        let extreme_ids = members
            .iter()
            .filter(|(_, controller, value)| extremes.get(&group_of(controller)) == Some(value))
            .map(|(id, ..)| Value::String(id.clone()))
            .collect::<Vec<_>>();
        let mut prepared = atom.clone();
        prepared["extremeIds"] = Value::Array(extreme_ids);
        prepared["anchorIds"] = json!([]);
        Some(prepared)
    }

    /// A prepared `extremumAmong` atom: the permanent is a ranked member
    /// holding the extreme value. An unprepared atom never matches.
    pub(super) fn permanent_holds_extremum(
        &self,
        relation: &Value,
        permanent: &CardInstance,
    ) -> bool {
        relation["extremeIds"].as_array().is_some_and(|ids| {
            ids.iter()
                .any(|id| id.as_str() == Some(permanent.instance_id.as_str()))
        })
    }

    /// The object a selection names: a permanent, or a card in another
    /// zone, with whether it is on the battlefield.
    fn selected_object(&self, instance_id: &str) -> Option<(&CardInstance, bool)> {
        match self.permanent_position(instance_id) {
            Some((player, index)) => Some((&self.state.players[player].battlefield[index], true)),
            None => self
                .card_outside_battlefield(instance_id)
                .map(|card| (card, false)),
        }
    }

    /// A selected object's numeric property: the current value of a
    /// permanent, the printed value of a card elsewhere (CR 208.1, 202.3:
    /// `X` is zero off the stack). A power or toughness with no printed
    /// number (`*`) has no value here (`None`), never zero.
    fn selected_numeric_property(&self, instance_id: &str, property: SetProperty) -> Option<i32> {
        let (card, on_battlefield) = self.selected_object(instance_id)?;
        match property {
            SetProperty::ManaValue => Some(mana_value(&card.definition)),
            SetProperty::Power if on_battlefield => Some(self.current_power(card)),
            SetProperty::Toughness if on_battlefield => Some(self.current_toughness(card)),
            SetProperty::Power => card.definition.power.as_deref()?.parse().ok(),
            SetProperty::Toughness => card.definition.toughness.as_deref()?.parse().ok(),
        }
    }

    /// Whether a complete selection satisfies every set constraint of a
    /// choice (`setConstraints`): no two members share a name, power or mana
    /// value (`distinctValues`, CR 201.2 for names), and the members' total
    /// of one property is at most the bound (`totalAtMost`). The validator
    /// admits only these shapes; anything unreadable fails the selection.
    pub(super) fn selection_satisfies(&self, selection: &[String], constraints: &Value) -> bool {
        let Some(constraints) = constraints.as_array() else {
            return false;
        };
        constraints
            .iter()
            .all(|constraint| match value_kind(constraint) {
                Some("distinctValues") => match constraint["property"].as_str() {
                    Some("name") => {
                        let names = selection
                            .iter()
                            .map(|id| {
                                self.selected_object(id)
                                    .map(|(card, _)| card.definition.name.to_ascii_lowercase())
                            })
                            .collect::<Option<Vec<_>>>();
                        names.is_some_and(|names| {
                            names.iter().collect::<BTreeSet<_>>().len() == names.len()
                        })
                    }
                    property => SetProperty::from_ir(property).is_some_and(|property| {
                        selection
                            .iter()
                            .map(|id| self.selected_numeric_property(id, property))
                            .collect::<Option<Vec<_>>>()
                            .is_some_and(|values| {
                                values.iter().collect::<BTreeSet<_>>().len() == values.len()
                            })
                    }),
                },
                Some("totalAtMost") => {
                    let (Some(property), Some(maximum)) = (
                        SetProperty::from_ir(constraint["property"].as_str()),
                        integer_value(&constraint["value"]),
                    ) else {
                        return false;
                    };
                    selection
                        .iter()
                        .map(|id| self.selected_numeric_property(id, property))
                        .try_fold(0_i64, |total, value| {
                            value.map(|value| total.saturating_add(i64::from(value)))
                        })
                        .is_some_and(|total| total <= i64::from(maximum))
                }
                Some("sharedValue") => {
                    constraint["characteristic"]
                        .as_str()
                        .is_some_and(|characteristic| {
                            self.selection_shares_characteristic(selection, characteristic)
                        })
                }
                _ => false,
            })
    }

    /// A declared target set against one set constraint (CR 601.2c): every
    /// target is an object (a player has no name or mana value), checked
    /// with the same reader as a resolution choice.
    pub(super) fn target_selection_satisfies_set_constraint(
        &self,
        selection: &[TargetRef],
        constraint: &Value,
    ) -> bool {
        let ids = selection
            .iter()
            .map(|target| match target {
                TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id } => {
                    Some(instance_id.clone())
                }
                TargetRef::Player { .. } | TargetRef::StackObject { .. } => None,
            })
            .collect::<Option<Vec<_>>>();
        ids.is_some_and(|ids| {
            self.selection_satisfies(&ids, &Value::Array(vec![constraint.clone()]))
        })
    }

    /// `countDistinctPowers` read outside a resolving stack object (an
    /// intervening `if`, a static or activation condition: `you control
    /// three or more creatures with different powers`): the number of
    /// distinct current powers among the player's permanents matching the
    /// filter, read with the same matcher as the resolving count. A source
    /// exclusion needs the source and is not evaluated here (`None`).
    pub(super) fn distinct_power_count(&self, expression: &Value, player_id: &str) -> Option<i32> {
        if expression["excludeSource"].as_bool() == Some(true) {
            return None;
        }
        let player_index = self.player_index(player_id).ok()?;
        let powers = self.state.players[player_index]
            .battlefield
            .iter()
            .filter(|permanent| self.permanent_state_matches(permanent, &expression["where"]))
            .map(|permanent| self.current_power(permanent))
            .collect::<BTreeSet<_>>();
        i32::try_from(powers.len()).ok()
    }

    /// A stack object as a declared target candidate: its characteristics
    /// through the shared stack filter, and the `targets` relation through
    /// its chosen targets, read relative to `controller_id`.
    pub(super) fn spell_candidate_matches(
        &self,
        object: &StackObject,
        filter: &Value,
        controller_id: &str,
    ) -> bool {
        match value_kind(filter) {
            Some("targets") => self.stack_object_targets_match(object, filter, controller_id),
            Some("and") => filter["operands"].as_array().is_some_and(|operands| {
                operands
                    .iter()
                    .all(|operand| self.spell_candidate_matches(object, operand, controller_id))
            }),
            Some("or") => filter["operands"].as_array().is_some_and(|operands| {
                operands
                    .iter()
                    .any(|operand| self.spell_candidate_matches(object, operand, controller_id))
            }),
            Some("not") => !self.spell_candidate_matches(object, &filter["operand"], controller_id),
            _ => matches_stack_filter(&object.card.definition, filter),
        }
    }

    /// CR 115.1: one of the object's chosen targets is a listed player or a
    /// battlefield permanent matching the listed controller and filter.
    pub(super) fn stack_object_targets_match(
        &self,
        object: &StackObject,
        atom: &Value,
        controller_id: &str,
    ) -> bool {
        object.targets.values().any(|target| match target {
            TargetRef::Player { player_id } => atom
                .get("players")
                .is_some_and(|role| role_matches(role.as_str(), player_id, controller_id)),
            TargetRef::Permanent { instance_id } => atom.get("permanents").is_some_and(|query| {
                self.permanent_position(instance_id)
                    .is_some_and(|(player_index, card_index)| {
                        let permanent = &self.state.players[player_index].battlefield[card_index];
                        query.get("controller").is_none_or(|role| {
                            role_matches(role.as_str(), &permanent.controller, controller_id)
                        }) && (query["where"].is_null()
                            || self.permanent_matches_target_filter(
                                permanent,
                                &query["where"],
                                controller_id,
                            ))
                    })
            }),
            TargetRef::Card { .. } | TargetRef::StackObject { .. } => false,
        })
    }

    /// CR 509.1h: an attacking creature becomes unblocked only once the
    /// declare-blockers step has declared blockers; before then (in the
    /// declare-attackers step) no attacker is blocked or unblocked.
    pub(super) fn blockers_have_been_declared(&self) -> bool {
        matches!(
            self.state.step,
            GameStep::DeclareBlockers | GameStep::CombatDamage
        )
    }

    /// Whether this permanent is an attacking creature in the current combat.
    fn permanent_is_declared_attacker(&self, permanent: &CardInstance) -> bool {
        self.state
            .combat
            .attackers
            .iter()
            .any(|attacker| attacker.attacker_id == permanent.instance_id)
    }

    /// CR 509.1h: an attacking creature becomes blocked once a creature is
    /// declared as its blocker, and it remains blocked for the rest of the
    /// combat even if every creature blocking it leaves combat. The current
    /// block pairs are read first; the turn's `blockerDeclared` events keep
    /// the status of an attacker whose blockers were removed, for the same
    /// incarnation of that attacker only.
    pub(super) fn permanent_is_blocked(&self, permanent: &CardInstance) -> bool {
        if !self.permanent_is_declared_attacker(permanent) {
            return false;
        }
        if self
            .state
            .combat
            .blockers
            .iter()
            .any(|blocker| blocker.attacker_id == permanent.instance_id)
        {
            return true;
        }
        let identity = self.object_identity_value(&permanent.instance_id);
        self.state.events.iter().any(|event| {
            event.turn_number == self.state.turn_number
                && event.kind == "blockerDeclared"
                && event.detail["attackerId"].as_str() == Some(permanent.instance_id.as_str())
                && event
                    .detail
                    .get("attackerIdentity")
                    .is_none_or(|recorded| recorded.is_null() || *recorded == identity)
        })
    }

    /// The block status atoms, read for any consumer of a permanent filter:
    /// `isBlocked` and `isUnblockedAttacker` (CR 509.1h), an attacking
    /// creature that is not blocked once blockers have been declared. A blocked attacker whose blockers left
    /// combat stays blocked, so it never becomes unblocked. `None` for any
    /// other atom.
    pub(super) fn combat_block_status_matches(
        &self,
        permanent: &CardInstance,
        kind: &str,
    ) -> Option<bool> {
        match kind {
            "isBlocked" => Some(self.permanent_is_blocked(permanent)),
            "isUnblockedAttacker" => Some(
                self.blockers_have_been_declared()
                    && self.permanent_is_declared_attacker(permanent)
                    && !self.permanent_is_blocked(permanent),
            ),
            _ => None,
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "object_predicate_eval_tests.rs"]
mod tests;
