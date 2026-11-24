//! `can't attack you [or planeswalkers you control]` (CR 508.1c): a static
//! restriction that removes the ability controller, and optionally the
//! planeswalkers that player controls, from the legal defenders of the
//! affected creatures. A battle the player protects stays a legal defender.
use serde_json::Value;

use super::{CardInstance, GameEngine, TargetRef, has_card_type, value_kind};

impl GameEngine {
    /// The objects of a combat restriction are matched at declaration
    /// against effective characteristics (`creatures without flying` sees a
    /// granted flying), with the same controller, ownership and source
    /// exclusion relations as every static selector.
    pub(super) fn combat_restriction_selector_matches(
        &self,
        source: &CardInstance,
        selector: &Value,
        permanent: &CardInstance,
    ) -> bool {
        if value_kind(selector) != Some("permanents")
            || value_kind(&selector["where"]) == Some("chosenCreatureType")
            || selector.get("history").is_some()
        {
            return self.static_keyword_selector_matches(source, selector, permanent);
        }
        Self::static_selector_relations_match(source, selector, permanent)
            && !(selector["excludeSource"].as_bool().unwrap_or(false)
                && source.instance_id == permanent.instance_id)
            && self.permanent_matches_effective_filter(permanent, &selector["where"])
    }

    pub(super) fn attack_defender_protected_by(
        &self,
        source: &CardInstance,
        modifier: &Value,
        defender: &TargetRef,
    ) -> bool {
        match defender {
            TargetRef::Player { player_id } => *player_id == source.controller,
            TargetRef::Permanent { instance_id }
                if modifier["defenders"].as_str() == Some("playerAndPlaneswalkers") =>
            {
                self.permanent_position(instance_id)
                    .is_some_and(|(player_index, card_index)| {
                        let permanent = &self.state.players[player_index].battlefield[card_index];
                        permanent.controller == source.controller
                            && has_card_type(&permanent.definition, "Planeswalker")
                    })
            }
            _ => false,
        }
    }
}

/// Per-object combat restriction and requirement markers (CR 508.1c-d,
/// 509.1b-c) granted by static abilities and temporary effects.
pub(super) fn is_combat_restriction_marker(keyword: &str) -> bool {
    matches!(
        keyword,
        "cantAttack"
            | "cantBlock"
            | "cantBeBlocked"
            | "maxOneBlocker"
            | "mustBeBlockedIfAble"
            | "attackEachCombatIfAble"
            | "blockEachCombatIfAble"
            | "assignsNoCombatDamage"
    )
}
