//! Revalidate generated automatic production before public action mutation.
//! Payment execution consumes this same private plan; it does not query legacy.
use super::mana_discovery_session::ManaDiscoverySession;
use super::*;

impl GameEngine {
    pub(super) fn apply_checked_priority_action<P: DecisionProvider>(
        &mut self,
        action: &LegalAction,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        self.validate_generated_mana_payment(action)?;
        self.apply_priority_action(action, provider)
    }

    pub(super) fn validate_generated_mana_payment(
        &self,
        action: &LegalAction,
    ) -> Result<(), EngineError> {
        let Some(entries) = action.decisions.get("manaPayment") else {
            return Ok(());
        };
        let entries = entries
            .as_array()
            .ok_or_else(|| EngineError::new("invalid generated mana payment array"))?;
        if mana_postcost_ordered::is_ordered_payment(&action.decisions) {
            // Prefix costs can make later programs available or unavailable.
            // Validate structure now, then let each normal activation executor
            // authenticate availability in its committed prefix world.
            for receipt in entries {
                match value_kind(receipt) {
                    Some("manaPaymentMetadata" | "phyrexianManaPayment") => continue,
                    Some(_) => {
                        return Err(EngineError::new("unsupported generated mana payment entry"));
                    }
                    None => {}
                }
                granted_mana_program::ordered_receipt_declaration(receipt, &action.player_id)?;
                // A non-tap producer may legally activate again. Availability,
                // use limits and costs belong to each actual prefix execution.
            }
            return Ok(());
        }
        let session = ManaDiscoverySession::new(self);
        let mut seen_sources = BTreeSet::new();
        for activation in entries {
            match value_kind(activation) {
                Some("manaPaymentMetadata" | "phyrexianManaPayment") => continue,
                Some(_) => {
                    return Err(EngineError::new("unsupported generated mana payment entry"));
                }
                None => {}
            }
            let source_id = activation["sourceId"]
                .as_str()
                .ok_or_else(|| EngineError::new("generated mana payment lacks source identity"))?;
            if !seen_sources.insert(source_id) {
                return Err(EngineError::new(
                    "generated mana payment repeats one source",
                ));
            }
            let (owner, index) = self
                .permanent_position(source_id)
                .ok_or_else(|| EngineError::new("generated mana source left battlefield"))?;
            let source = &self.state.players[owner].battlefield[index];
            let assist = action
                .decisions
                .get("assistPlayerId")
                .and_then(Value::as_str);
            if source.controller != action.player_id && assist != Some(source.controller.as_str()) {
                return Err(EngineError::new(
                    "generated mana source is not controlled by payer or assisting player",
                ));
            }
            let mana = activation["mana"]
                .as_array()
                .ok_or_else(|| EngineError::new("generated mana output is not an array"))?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| EngineError::new("generated mana output is not a symbol"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if mana.is_empty() {
                return Err(EngineError::new(
                    "NoMana activation is not a useful payment source",
                ));
            }
            let origin = granted_mana_program::receipt_origin(activation)?;
            let rule = self.resolve_mana_program(source, origin.as_ref())?;
            let has_tap_cost = rule.as_ref().map_or(true, |rule| {
                rule["costs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|cost| value_kind(cost) == Some("tap"))
            });
            if has_tap_cost && self.permanent_cannot_pay_tap_symbol(source) {
                return Err(EngineError::new(
                    "generated mana source cannot pay tap cost",
                ));
            }
            let life_cost = integer_value(&activation["lifePaid"])
                .ok_or_else(|| EngineError::new("invalid generated mana life cost"))?;
            let sacrifices = activation["sacrificesSource"]
                .as_bool()
                .ok_or_else(|| EngineError::new("invalid generated mana sacrifice cost"))?;
            // Spending restrictions were checked against the resulting spell/
            // ability during generation. This validates actual production without
            // rebuilding a different face or weakening those restrictions.
            let choices = session.mana_choices_for_permanent(source, None, true, false)?;
            if !choices.iter().any(|choice| {
                choice.origin == origin
                    && choice.symbols == mana
                    && choice.life_cost == life_cost
                    && choice.sacrifices_source == sacrifices
            }) {
                return Err(EngineError::new(
                    "generated mana production plan is no longer valid",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "mana_payment_validation_tests.rs"]
mod tests;
