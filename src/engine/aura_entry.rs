//! An Aura put onto the battlefield without being cast (CR 303.4f): the
//! player putting it there chooses, as it enters, an object it could enchant.
//! It is attached to that object; with no legal object it stays in the zone
//! it would have moved from. Token creation and card movement share this
//! choice, so no battlefield entry produces an unattached Aura.

use super::*;

/// The attachment an entering card needs.
pub(super) enum EnteringAuraHost {
    /// Not an Aura: it enters unattached.
    NotAnAura,
    /// The chosen object to enchant.
    Host(String),
    /// An Aura with nothing it could enchant: it does not enter.
    NoLegalHost,
}

impl GameEngine {
    /// Chooses the object `definition` will enchant as it enters under
    /// `chooser`'s control. A face-down permanent is not an Aura (CR 708.2).
    pub(super) fn choose_entering_aura_host<P: DecisionProvider>(
        &mut self,
        definition: &CardDefinition,
        chooser: &str,
        stack_object: &StackObject,
        provider: &mut P,
        decision_name: &str,
    ) -> Result<EnteringAuraHost, EngineError> {
        if !type_line_contains(&definition.type_line, "Aura") {
            return Ok(EnteringAuraHost::NotAnAura);
        }
        if entering_aura_enchant_filter(definition).is_none() {
            return Ok(EnteringAuraHost::NoLegalHost);
        }
        let candidates = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| player.battlefield.iter())
            .filter(|permanent| self.aura_could_enchant(definition, permanent))
            .map(|permanent| permanent.instance_id.clone())
            .collect::<Vec<_>>();
        let selected = self.choose_resolution_objects(
            stack_object,
            provider,
            chooser,
            decision_name,
            candidates,
            1,
            1,
            "Choose a permanent for the Aura to enchant.",
        )?;
        Ok(selected
            .into_iter()
            .next()
            .map_or(EnteringAuraHost::NoLegalHost, EnteringAuraHost::Host))
    }

    /// Whether an entering Aura (`definition`) could enchant `permanent`
    /// (CR 303.4a, 303.4f): its Enchant ability allows the object and the
    /// object is not protected from it (CR 702.16c). Shared by the chosen
    /// host of an entering Aura and the stated host of an Aura token created
    /// attached to an object (CR 111.3).
    pub(super) fn aura_could_enchant(
        &self,
        definition: &CardDefinition,
        permanent: &CardInstance,
    ) -> bool {
        entering_aura_enchant_filter(definition).is_some_and(|enchant_filter| {
            self.permanent_matches_effective_filter(permanent, enchant_filter)
                && !self.permanent_is_protected_from_card(permanent, definition)
        })
    }
}

/// The object filter of an Aura's Enchant keyword ability.
fn entering_aura_enchant_filter(definition: &CardDefinition) -> Option<&Value> {
    definition.printed_rules().iter().find_map(|rule| {
        (value_kind(rule) == Some("keywordAbility")
            && value_kind(&rule["ability"]) == Some("enchant"))
        .then(|| &rule["ability"]["where"])
    })
}
