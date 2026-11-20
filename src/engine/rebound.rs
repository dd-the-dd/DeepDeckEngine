//! Rebound (CR 702.88a) as a typed keyword expansion. A hand-cast spell with
//! rebound is exiled instead of put into its owner's graveyard as it resolves
//! (the resolution owner applies that replacement), and this module installs
//! the shared delayed step trigger: at the beginning of the controller's next
//! upkeep, they may cast the card from exile without paying its mana cost.
use super::*;

/// The keyword's printed form; casting from hand marks the stack object.
pub(super) fn rebound_ability_supported(ability: &Value) -> bool {
    value_kind(ability) == Some("rebound")
}

/// The delayed program: an optional cast of this card from exile, through the
/// same casting primitive Suspend uses (CR 702.88b: alternative cost rules).
fn rebound_cast_program() -> Value {
    json!([{
        "kind": "castAnyNumber",
        "player": { "kind": "abilityController" },
        "cards": { "kind": "self" },
        "where": { "kind": "canBeCastAsSpell" },
        "timing": { "kind": "duringResolution" },
        "withoutPayingManaCost": true,
        "alternativeCostsAllowed": false,
        "additionalCostsApply": true,
        "variableManaValue": { "kind": "integer", "value": 0 },
        "sourceZone": "exile",
        "maximum": { "kind": "integer", "value": 1 },
    }])
}

impl GameEngine {
    /// Installs the next-upkeep delayed trigger for a card rebound exiled.
    /// Its source is that exiled card; its controller is the spell's
    /// controller as it resolved (CR 603.7d).
    pub(super) fn install_rebound_upkeep_trigger(&mut self, card: &CardInstance, controller: &str) {
        let modifier = json!({
            "kind": "delayedStepTrigger",
            "step": "upkeep",
            "turnPlayerId": controller,
            "controllerId": controller,
            "effects": rebound_cast_program(),
            "sourceCard": card,
            "trackedObjectId": null,
            "capturedBindings": {},
        });
        self.state.rule_modifiers.push(modifier.clone());
        self.record_event(
            "delayedTriggerInstalled",
            Some(controller.to_string()),
            Some(card.instance_id.clone()),
            modifier,
        );
    }
}
