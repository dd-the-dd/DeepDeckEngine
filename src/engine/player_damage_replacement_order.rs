//! Conservative capability boundary for the NEW simultaneous player batch.
//! CR 616 choices are not implemented by a fixed arithmetic evaluation order.
//! The scalar compatibility path deliberately does not call this boundary.
use super::*;

impl GameEngine {
    pub(super) fn require_player_damage_replacement_order(
        &self,
        player: &str,
        source: &str,
        source_controller: &str,
        source_card: &CardInstance,
        source_is_creature: bool,
    ) -> Result<(), EngineError> {
        let modifiers = self.battlefield_static_modifiers();
        let mut additive = false;
        let mut multiplicative = false;
        let mut fixed_prevention = 0_usize;
        let mut full_prevention = 0_usize;
        let cannot_prevent = self.has_ecc_static_ability("damageCannotBePrevented", None);
        for (owner, modifier) in &modifiers {
            if owner.controller == source_controller {
                match value_kind(modifier) {
                    Some("increaseDamageBySourceCounter") if player != source_controller => {
                        let quantity = modifier["counter"]
                            .as_str()
                            .and_then(|name| owner.counters.get(name))
                            .copied()
                            .unwrap_or(0);
                        if quantity < 0 {
                            return Err(EngineError::new(
                                "damage counter bonus has a negative counter quantity",
                            ));
                        }
                        additive |= quantity > 0;
                    }
                    Some("increaseControlledNoncombatDamage") if player != source_controller => {
                        additive |= integer_value(&modifier["amount"]).is_some_and(|n| n > 0);
                    }
                    Some("doubleDamageFromChosenType") => {
                        multiplicative |= Self::chosen_creature_type_matches(
                            owner,
                            object_predicate::DefinitionView::for_permanent(&source_card),
                            &json!({"decisionId": modifier["decisionId"]}),
                        );
                    }
                    _ => {}
                }
            }
            if !cannot_prevent && owner.controller == player {
                match value_kind(modifier) {
                    Some("preventOpponentDamageAmount") if source_controller != player => {
                        fixed_prevention += 1;
                    }
                    Some("preventPlayerDamage") => full_prevention += 1,
                    _ => {}
                }
            }
        }
        for modifier in &self.state.rule_modifiers {
            if value_kind(modifier) == Some("damageMultiplier")
                && modifier["playerId"].as_str() == Some(source_controller)
            {
                let factor = integer_value(&modifier["factor"])
                    .filter(|factor| *factor >= 1)
                    .ok_or_else(|| {
                        EngineError::new(
                            "player damage replacement requires an integer factor of at least one",
                        )
                    })?;
                multiplicative |= factor > 1;
            }
            if cannot_prevent {
                continue;
            }
            let prevented = match value_kind(modifier) {
                Some("linkedDamagePrevention") => {
                    modifier["damagingCardInstanceId"].as_str() == Some(source)
                        && modifier["sourceCardInstanceId"]
                            .as_str()
                            .is_some_and(|id| self.permanent_position(id).is_some())
                }
                Some("protectionFromEverything") => modifier["playerId"].as_str() == Some(player),
                Some("preventOpponentCreatureDamage") => {
                    source_is_creature
                        && source_controller != player
                        && modifier["playerId"].as_str() == Some(player)
                }
                _ => false,
            };
            full_prevention += usize::from(prevented);
        }
        // Positive additions commute with each other, positive integer factors
        // with each other, and pure fixed preventions with each other. Crossing
        // those classes (or competing full preventions) needs affected-player
        // choices, per-effect identity and re-evaluated applicability.
        if additive && multiplicative
            || (additive || multiplicative) && (fixed_prevention > 0 || full_prevention > 0)
            || full_prevention > 0 && (fixed_prevention > 0 || full_prevention > 1)
        {
            return Err(EngineError::new(
                "simultaneous damage has competing replacements requiring affected-player ordering choices",
            ));
        }
        Ok(())
    }
}
