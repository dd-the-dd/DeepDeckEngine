//! Mana leaving the pools as steps and phases end (CR 500.4, 106.4).
//!
//! Each pool empties at the end of every step and phase, except the mana an
//! effect retains: a per-unit retention the producing effect stamped on the
//! mana (`you don't lose this mana as steps and phases end`, CR 106.4), a
//! firebending unit, a static or temporary retention of the player's unspent
//! mana of some types, or a conversion of the unspent mana (CR 106.4a).

use super::*;

/// The per-unit attribute of mana its producing effect retains until end of
/// turn (`Until end of turn, you don't lose this mana as steps and phases
/// end`). It restricts no use of the mana.
pub(super) const RETAINED_UNTIL_END_OF_TURN: &str = "retainedUntilEndOfTurn";

/// Whether a spend restriction carries only per-unit attributes and no use
/// restriction (no `kind`): such mana may pay for anything.
pub(super) fn unit_attributes_only(restriction: &Value) -> bool {
    restriction.get("kind").is_none()
}

/// Whether a produced mana unit's retention attribute is well formed, and
/// a restriction without a use carries only known unit attributes.
pub(super) fn unit_attributes_supported(restriction: &Value) -> bool {
    restriction
        .get(RETAINED_UNTIL_END_OF_TURN)
        .is_none_or(|retained| retained.as_bool() == Some(true))
        && (!unit_attributes_only(restriction)
            || restriction.as_object().is_some_and(|fields| {
                !fields.is_empty()
                    && fields.keys().all(|field| {
                        matches!(
                            field.as_str(),
                            RETAINED_UNTIL_END_OF_TURN | "producedBySnowSource" | "cantBeCountered"
                        )
                    })
            }))
}

/// Whether the unit survives the end of `ending_step`: an `until end of turn`
/// retention lasts through every step of the turn and ends with the cleanup
/// step, when that effect ends (CR 514.2).
fn unit_retained_past(mana: &FloatingMana, ending_step: GameStep) -> bool {
    ending_step != GameStep::Cleanup
        && mana.spend_restriction.as_ref().is_some_and(|restriction| {
            restriction[RETAINED_UNTIL_END_OF_TURN].as_bool() == Some(true)
        })
}

impl GameEngine {
    /// Empties every pool as the current step or phase ends, keeping the
    /// retained mana and converting the mana a static ability converts.
    pub(super) fn empty_mana_pools_as_step_ends(&mut self) {
        let ending_step = self.state.step;
        let mana_conversions = self
            .state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .flat_map(|source| {
                source
                    .definition
                    .printed_rules()
                    .iter()
                    .flat_map(move |rule| {
                        rule["modifiers"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|modifier| {
                                value_kind(rule) == Some("staticAbility")
                                    && value_kind(modifier) == Some("convertUnspentMana")
                                    && value_kind(&modifier["player"]) == Some("controllerOf")
                            })
                            .filter_map(move |modifier| {
                                modifier["to"]
                                    .as_str()
                                    .map(|symbol| (source.controller.clone(), symbol.to_string()))
                            })
                    })
            })
            .collect::<BTreeMap<_, _>>();
        let mut mana_retentions = self
            .state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .flat_map(|source| {
                source
                    .definition
                    .printed_rules()
                    .iter()
                    .flat_map(move |rule| {
                        rule["modifiers"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|modifier| {
                                value_kind(rule) == Some("staticAbility")
                                    && value_kind(modifier) == Some("retainUnspentMana")
                                    && value_kind(&modifier["player"]) == Some("controllerOf")
                            })
                            .flat_map(move |modifier| {
                                modifier["symbols"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .filter_map(Value::as_str)
                                    .map(move |symbol| {
                                        (source.controller.clone(), symbol.to_string())
                                    })
                            })
                    })
            })
            .fold(
                BTreeMap::<String, BTreeSet<String>>::new(),
                |mut map, (player, symbol)| {
                    map.entry(player).or_default().insert(symbol);
                    map
                },
            );
        for modifier in &self.state.rule_modifiers {
            if value_kind(modifier) != Some("retainUnspentMana")
                || modifier["expiresAfterTurn"]
                    .as_u64()
                    .is_some_and(|turn| turn < u64::from(self.state.turn_number))
            {
                continue;
            }
            let Some(player_id) = modifier["playerId"].as_str() else {
                continue;
            };
            for symbol in modifier["symbols"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                mana_retentions
                    .entry(player_id.to_string())
                    .or_default()
                    .insert(symbol.to_string());
            }
        }
        for player in &mut self.state.players {
            let conversion = mana_conversions.get(&player.id);
            let retained_symbols = mana_retentions.get(&player.id);
            let mut retained_mana = Vec::new();
            for mut mana in std::mem::take(&mut player.mana_pool) {
                let independently_retained =
                    mana.spend_restriction.as_ref().is_some_and(|restriction| {
                        value_kind(restriction) == Some("firebendingMana")
                            && restriction["expiresAfterTurn"]
                                .as_u64()
                                .is_some_and(|turn| turn >= u64::from(self.state.turn_number))
                    }) || unit_retained_past(&mana, ending_step);
                if independently_retained {
                    retained_mana.push(mana);
                } else if retained_symbols.is_some_and(|symbols| symbols.contains(&mana.symbol)) {
                    retained_mana.push(mana);
                } else if let Some(symbol) = conversion {
                    mana.symbol = symbol.clone();
                    mana.spend_restriction = None;
                    retained_mana.push(mana);
                }
            }
            player.mana_pool = retained_mana;
        }
    }
}
