//! Zone of function (CR 113.6, 604.3): where a rule of an object functions.
//!
//! A static ability states it with `activeWhile: inZone(self, zone)`; a
//! triggered ability with `triggerZone` (absent means the battlefield,
//! CR 113.6). Every observer that asks "does this rule function from where
//! its object is" reads one typed value instead of re-matching JSON shapes.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ZoneOfFunction {
    Battlefield,
    Graveyard,
    Hand,
    Exile,
    Library,
    Stack,
    /// While the object is a spell, or being cast (CR 601.2f reductions).
    StackOrCast,
    Command,
    CommandZoneOrBattlefield,
}

impl ZoneOfFunction {
    fn from_static_zone(zone: &str) -> Option<Self> {
        Some(match zone {
            "battlefield" => Self::Battlefield,
            "graveyard" => Self::Graveyard,
            "hand" => Self::Hand,
            "exile" => Self::Exile,
            "library" => Self::Library,
            "stack" => Self::Stack,
            "stackOrCast" => Self::StackOrCast,
            "command" => Self::Command,
            "commandZoneOrBattlefield" => Self::CommandZoneOrBattlefield,
            _ => return None,
        })
    }

    /// Whether a rule with this zone of function functions while its object
    /// is in `zone` (engine zone names: battlefield, graveyard, hand, exile,
    /// library, stack, commandZone).
    pub(super) fn functions_in(self, zone: &str) -> bool {
        match self {
            Self::Battlefield => zone == "battlefield",
            Self::Graveyard => zone == "graveyard",
            Self::Hand => zone == "hand",
            Self::Exile => zone == "exile",
            Self::Library => zone == "library",
            Self::Stack | Self::StackOrCast => zone == "stack",
            Self::Command => zone == "commandZone",
            Self::CommandZoneOrBattlefield => matches!(zone, "commandZone" | "battlefield"),
        }
    }

    /// Zones other than the battlefield whose static abilities feed the
    /// continuous-effect collector (CR 604.3: they say where they function).
    pub(super) fn is_card_zone_outside_battlefield(self) -> bool {
        matches!(self, Self::Graveyard | Self::Hand | Self::Exile)
    }
}

/// The zone of function a static rule states, if it is a self `inZone`.
pub(super) fn static_zone_of_function(rule: &Value) -> Option<ZoneOfFunction> {
    let active = &rule["activeWhile"];
    if value_kind(active) != Some("inZone") || value_kind(&active["object"]) != Some("self") {
        return None;
    }
    value_kind(&active["zone"]).and_then(ZoneOfFunction::from_static_zone)
}

/// The zone of function of a triggered rule. Absent means the battlefield.
pub(super) fn trigger_zone_of_function(rule: &Value) -> ZoneOfFunction {
    match rule["triggerZone"].as_str() {
        None => ZoneOfFunction::Battlefield,
        Some(zone) => ZoneOfFunction::from_static_zone(zone).unwrap_or(ZoneOfFunction::Battlefield),
    }
}

/// A static ability that functions from a card zone other than the
/// battlefield ("as long as this card is in your graveyard"). It shares the
/// battlefield modifier vocabulary; only its zone of function differs.
pub(super) fn card_zone_static_rule_supported(rule: &Value) -> bool {
    static_zone_of_function(rule).is_some_and(ZoneOfFunction::is_card_zone_outside_battlefield)
        && rule.get("condition").is_none_or(condition_supported)
        && rule["modifiers"].as_array().is_some_and(|modifiers| {
            !modifiers.is_empty() && modifiers.iter().all(battlefield_static_modifier_supported)
        })
}

/// An activation limit of an ability activated from its owner's hand
/// (forecast, CR 702.57a-b: `only once each turn`). The use is recorded on
/// the card in hand, which stays there when its cost only reveals it; a card
/// that leaves and returns is a new object with no use (CR 400.7). Another
/// limit or another off-battlefield zone keeps no use record and is refused.
pub(super) fn hand_activation_limit_supported(rule: &Value) -> bool {
    rule["activationZone"].as_str() == Some("hand")
        && value_kind(&rule["activationLimit"]) == Some("oncePerTurn")
        && rule["activationLimit"]["id"].is_string()
}

impl GameEngine {
    /// Records one use of a hand-activated ability's limit on the card in
    /// hand (see [`hand_activation_limit_supported`]). A source that left the
    /// hand while its costs were paid keeps no record: it is a new object.
    pub(super) fn record_hand_activation_use(
        &mut self,
        player_index: usize,
        zone_name: &str,
        instance_id: &str,
        rule: &Value,
        rule_index: usize,
    ) {
        if zone_name != "hand" || !hand_activation_limit_supported(rule) {
            return;
        }
        let Some(flag) =
            activation_limit_flag(&rule["activationLimit"], self.state.turn_number, rule_index)
        else {
            return;
        };
        if let Some(card) = self.state.players[player_index]
            .hand
            .iter_mut()
            .find(|card| card.instance_id == instance_id)
        {
            card.flags.insert(flag, true);
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "zone_of_function_tests.rs"]
mod tests;
