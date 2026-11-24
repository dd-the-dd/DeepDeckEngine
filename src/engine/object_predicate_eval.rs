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

use serde_json::Value;

use super::{
    CardInstance, GameEngine, StackObject, TargetRef, matches_stack_filter,
    permanent_filter_supported, stack_filter_supported, value_kind,
};

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
fn targets_atom_supported(atom: &Value) -> bool {
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

impl GameEngine {
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
    fn stack_object_targets_match(
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
    /// creature that is not blocked. A blocked attacker whose blockers left
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
                self.permanent_is_declared_attacker(permanent)
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
