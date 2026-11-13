use std::collections::BTreeMap;

use serde_json::Value;

use super::{CardInstance, GameEngine, TargetRef, has_card_type, value_kind};

pub(super) fn protected_player_reference_supported(reference: &Value) -> bool {
    match value_kind(reference) {
        Some("abilityController" | "eachPlayer") => true,
        Some("controllerOf") => matches!(
            value_kind(&reference["object"]),
            Some("self" | "abilitySource")
        ),
        Some("opponentsOf") => protected_player_reference_supported(&reference["player"]),
        Some("controllerOfAttachedPermanent") => {
            value_kind(&reference["attachment"]) == Some("self")
        }
        _ => false,
    }
}

impl GameEngine {
    fn attack_tax_modifier_protects_defender(
        &self,
        source: &CardInstance,
        modifier: &Value,
        defender: &TargetRef,
    ) -> bool {
        let protected_player_id = match defender {
            TargetRef::Player { player_id } => player_id.as_str(),
            TargetRef::Permanent { instance_id }
                if modifier["protectPlaneswalkers"].as_bool() == Some(true) =>
            {
                let Some((player_index, card_index)) = self.permanent_position(instance_id) else {
                    return false;
                };
                let permanent = &self.state.players[player_index].battlefield[card_index];
                if !has_card_type(&permanent.definition, "Planeswalker") {
                    return false;
                }
                permanent.controller.as_str()
            }
            _ => return false,
        };
        self.event_player_reference_matches(
            &modifier["protectedPlayer"],
            source,
            protected_player_id,
        )
    }

    pub(super) fn attack_tax_for(&self, defender: &TargetRef) -> i32 {
        self.battlefield_static_modifiers()
            .iter()
            .filter(|(source, modifier)| {
                value_kind(modifier) == Some("attackTax")
                    && self.attack_tax_modifier_protects_defender(source, modifier, defender)
                    && modifier.get("condition").is_none_or(|condition| {
                        self.condition_value_with_source(
                            condition,
                            &source.controller,
                            &BTreeMap::new(),
                            Some(source),
                        )
                    })
            })
            .filter_map(|(source, modifier)| {
                self.numeric_expression_with_source(
                    &modifier["amount"],
                    &source.controller,
                    Some(source),
                )
            })
            .fold(0_i32, i32::saturating_add)
            .max(0)
    }
}
