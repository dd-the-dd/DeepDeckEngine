//! Random and contested outcomes: flipping coins (CR 705), rolling dice
//! (CR 706) and clashing (CR 701.30). Each is one primitive of the acting
//! player. It records its outcome under the instruction's `bind` for the
//! rest of the resolving program, where `the flip`, `the result` and a bare
//! `you win` read the latest outcome of their kind, and it publishes one
//! event per flip, roll or clash for observers (`whenever you win a coin
//! flip`, `whenever you roll a die`).
//!
//! Randomness comes from the engine's seeded generator, the same source as
//! shuffles and random selections: a replay from the same seed and the same
//! decisions reproduces every outcome. A clash has no random part; its
//! top-or-bottom choices are ordinary resolution decisions.
//!
//! Recorded bindings (the IR contract of `keyword_action_programs.rs`):
//!
//! ```text
//! flipCoins  BIND = flips won, BIND:lost, BIND:heads, BIND:tails,
//!            BIND:count, BIND:player
//! rollDice   BIND = total of the results, BIND:1..BIND:N each result,
//!            BIND:count, BIND:player
//! clash      BIND = 1 when the acting player won, else 0,
//!            BIND:opponent, BIND:player
//! ```
use rand::Rng;

use super::*;

/// The faces of the largest die an instruction may name (a d1000 exists
/// nowhere; a larger value is refused, not rolled).
const MAXIMUM_DIE_SIDES: u64 = 1000;

/// Coins or dice one instruction may produce at once: a bound on `X`.
const MAXIMUM_OUTCOMES: i32 = 1000;

fn bind_supported(effect: &Value) -> bool {
    effect["bind"].as_str().is_some_and(|id| !id.is_empty())
}

/// `Some(...)` for a random or contested outcome primitive.
pub(super) fn random_outcome_effect_supported(effect: &Value) -> Option<bool> {
    let acting =
        |effect: &Value| player_reference_supported(&effect["player"]) && bind_supported(effect);
    match value_kind(effect) {
        Some("flipCoins") => Some(acting(effect) && numeric_expression_supported(&effect["count"])),
        Some("rollDice") => Some(
            acting(effect)
                && numeric_expression_supported(&effect["count"])
                && effect["sides"]
                    .as_u64()
                    .is_some_and(|sides| (2..=MAXIMUM_DIE_SIDES).contains(&sides)),
        ),
        Some("clash") => Some(acting(effect) && player_reference_supported(&effect["opponent"])),
        _ => None,
    }
}

/// `Some(...)` for the reader of an outcome quantity.
pub(super) fn random_outcome_value_supported(expression: &Value) -> Option<bool> {
    (value_kind(expression) == Some("randomOutcomeValue")).then(|| {
        expression["binding"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    })
}

/// The value of an outcome quantity: the latest outcome its binding
/// recorded, or 0 when no outcome of that kind happened while the program
/// resolves (a declined `you may flip a coin` won and lost no flip, CR
/// 705.2; no die was rolled, and every face of a die is at least 1, CR
/// 706.1). `None` for any other expression.
pub(super) fn random_outcome_value(
    expression: &Value,
    bindings: &BTreeMap<String, RuntimeBinding>,
) -> Option<i32> {
    if value_kind(expression) != Some("randomOutcomeValue") {
        return None;
    }
    let id = expression["binding"].as_str()?;
    Some(match bindings.get(id) {
        Some(RuntimeBinding::Integer(value)) => *value,
        _ => 0,
    })
}

/// The outcome of one coin flip (CR 705.2-705.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CoinFlip {
    pub heads: bool,
    pub won: bool,
}

/// The totals of one `flip N coins` instruction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CoinFlipTotals {
    pub count: i32,
    pub won: i32,
    pub heads: i32,
}

impl CoinFlipTotals {
    fn record(&mut self, flip: CoinFlip) {
        self.count += 1;
        self.won += i32::from(flip.won);
        self.heads += i32::from(flip.heads);
    }

    fn write(self, bindings: &mut BTreeMap<String, RuntimeBinding>, bind: &str) {
        bindings.insert(bind.to_string(), RuntimeBinding::Integer(self.won));
        bindings.insert(
            format!("{bind}:lost"),
            RuntimeBinding::Integer(self.count - self.won),
        );
        bindings.insert(format!("{bind}:heads"), RuntimeBinding::Integer(self.heads));
        bindings.insert(
            format!("{bind}:tails"),
            RuntimeBinding::Integer(self.count - self.heads),
        );
        bindings.insert(format!("{bind}:count"), RuntimeBinding::Integer(self.count));
    }
}

/// CR 701.30d: the clashing player who revealed a card with a greater mana
/// value than every other revealed card wins. A player who revealed no card
/// (an empty library) cannot win; a tie has no winner.
pub(super) fn clash_winner(revealed: &[(String, Option<i32>)]) -> Option<String> {
    revealed
        .iter()
        .find(|(player, mana_value)| {
            mana_value.is_some_and(|mana_value| {
                revealed
                    .iter()
                    .filter(|(other, _)| other != player)
                    .all(|(_, other)| other.is_none_or(|other| mana_value > other))
            })
        })
        .map(|(player, _)| player.clone())
}

impl GameEngine {
    /// Runs a random or contested outcome primitive. `Ok(false)` for any
    /// other effect.
    pub(super) fn execute_random_outcome_effect<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        match value_kind(effect) {
            Some("flipCoins") => self.flip_coins(effect, stack_object, bindings, decisions)?,
            Some("rollDice") => self.roll_dice(effect, stack_object, bindings, decisions)?,
            Some("clash") => self.clash(effect, stack_object, bindings, decisions, provider)?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn outcome_bind(effect: &Value) -> Result<String, EngineError> {
        effect["bind"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(ToOwned::to_owned)
            .ok_or_else(|| EngineError::new("a random outcome without its result binding"))
    }

    /// The number of coins or dice, read when the instruction resolves.
    fn outcome_count(
        &self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<i32, EngineError> {
        let count = self.checked_runtime_numeric_expression(
            &effect["count"],
            stack_object,
            bindings,
            decisions,
            "random outcome count",
        )?;
        if !(0..=MAXIMUM_OUTCOMES).contains(&count) {
            return Err(EngineError::new(format!(
                "a number of coins or dice between 0 and {MAXIMUM_OUTCOMES}, got {count}"
            )));
        }
        Ok(count)
    }

    /// CR 705.2: one seeded fair coin. The flipping player calls heads or
    /// tails while the coin is in the air; for a fair coin the call does not
    /// change the outcome's distribution, so the engine records the call as
    /// heads and the player wins exactly when the coin comes up heads.
    pub(super) fn flip_one_coin(&mut self) -> CoinFlip {
        let heads = self.rng.gen_bool(0.5);
        CoinFlip { heads, won: heads }
    }

    /// CR 706.2: one seeded roll of a die with `sides` faces, 1 to `sides`.
    pub(super) fn roll_one_die(&mut self, sides: i32) -> i32 {
        self.rng.gen_range(1..=sides)
    }

    fn flip_coins(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
    ) -> Result<(), EngineError> {
        let bind = Self::outcome_bind(effect)?;
        let mut totals = CoinFlipTotals::default();
        // A flipping player who left the game flips nothing (CR 800.4a).
        let Some(player_id) = self.resolve_player(&effect["player"], stack_object, bindings) else {
            totals.write(bindings, &bind);
            bindings.remove(&format!("{bind}:player"));
            return Ok(());
        };
        let count = self.outcome_count(effect, stack_object, bindings, decisions)?;
        for index in 0..count {
            let flip = self.flip_one_coin();
            totals.record(flip);
            self.record_event(
                "coinFlipped",
                Some(player_id.clone()),
                Some(stack_object.card.instance_id.clone()),
                json!({
                    "stackId": stack_object.id,
                    "index": index,
                    "call": "heads",
                    "result": if flip.heads { "heads" } else { "tails" },
                    "won": flip.won,
                }),
            );
        }
        totals.write(bindings, &bind);
        bindings.insert(format!("{bind}:player"), RuntimeBinding::Player(player_id));
        Ok(())
    }

    fn roll_dice(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
    ) -> Result<(), EngineError> {
        let bind = Self::outcome_bind(effect)?;
        let sides = effect["sides"]
            .as_u64()
            .filter(|sides| (2..=MAXIMUM_DIE_SIDES).contains(sides))
            .and_then(|sides| i32::try_from(sides).ok())
            .ok_or_else(|| EngineError::new("a die with between 2 and 1000 faces"))?;
        let Some(player_id) = self.resolve_player(&effect["player"], stack_object, bindings) else {
            bindings.insert(bind.clone(), RuntimeBinding::Integer(0));
            bindings.insert(format!("{bind}:count"), RuntimeBinding::Integer(0));
            bindings.remove(&format!("{bind}:player"));
            return Ok(());
        };
        let count = self.outcome_count(effect, stack_object, bindings, decisions)?;
        let mut total = 0_i32;
        for index in 0..count {
            let result = self.roll_one_die(sides);
            total = total.saturating_add(result);
            bindings.insert(
                format!("{bind}:{}", index + 1),
                RuntimeBinding::Integer(result),
            );
            // CR 706.2: the natural result is the face rolled; no modifier
            // applies yet, so the result is the natural result.
            self.record_event(
                "dieRolled",
                Some(player_id.clone()),
                Some(stack_object.card.instance_id.clone()),
                json!({
                    "stackId": stack_object.id,
                    "index": index,
                    "sides": sides,
                    "natural": result,
                    "result": result,
                }),
            );
        }
        bindings.insert(bind.clone(), RuntimeBinding::Integer(total));
        bindings.insert(format!("{bind}:count"), RuntimeBinding::Integer(count));
        bindings.insert(format!("{bind}:player"), RuntimeBinding::Player(player_id));
        Ok(())
    }

    /// Seat distance from the active player: the APNAP order (CR 101.4).
    fn apnap_distance(&self, player_id: &str) -> usize {
        let count = self.state.players.len().max(1);
        self.state
            .players
            .iter()
            .position(|player| player.id == player_id)
            .map_or(count, |index| {
                (index + count - self.state.active_player % count) % count
            })
    }

    /// CR 701.30a-d: each clashing player reveals the top card of their
    /// library, then, in APNAP order, puts it on the top or the bottom of
    /// that library; the acting player wins when their card had the greater
    /// mana value.
    fn clash<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let bind = Self::outcome_bind(effect)?;
        let player = self.resolve_player(&effect["player"], stack_object, bindings);
        let opponent = self.resolve_player(&effect["opponent"], stack_object, bindings);
        let (Some(player_id), Some(opponent_id)) = (player, opponent) else {
            // Without both clashing players no clash happens (CR 609.3).
            bindings.insert(bind.clone(), RuntimeBinding::Integer(0));
            bindings.remove(&format!("{bind}:opponent"));
            return Ok(());
        };
        let mut clashers = vec![player_id.clone()];
        if opponent_id != player_id {
            clashers.push(opponent_id.clone());
        }
        clashers.sort_by_key(|id| self.apnap_distance(id));
        let mut revealed = Vec::with_capacity(clashers.len());
        let mut cards = Vec::with_capacity(clashers.len());
        for id in &clashers {
            let index = self.player_index(id)?;
            let top = self.state.players[index]
                .library
                .last()
                .map(|card| (card.instance_id.clone(), mana_value(&card.definition)));
            if let Some((card_id, mana_value)) = &top {
                self.record_event(
                    "clashCardRevealed",
                    Some(id.clone()),
                    Some(card_id.clone()),
                    json!({ "stackId": stack_object.id, "manaValue": mana_value }),
                );
            }
            revealed.push((id.clone(), top.as_ref().map(|(_, mana_value)| *mana_value)));
            cards.push((id.clone(), top.map(|(card_id, _)| card_id)));
        }
        for (id, card_id) in &cards {
            let Some(card_id) = card_id else {
                continue;
            };
            let decision_id = format!("{bind}:{id}:placement");
            let options = vec!["top".to_string(), "bottom".to_string()];
            self.make_resolution_decision(
                &decision_id,
                id,
                stack_object,
                DecisionChoice::OptionSelection {
                    decision_id: decision_id.clone(),
                    options: options.clone(),
                    prompt: "Put the revealed card on the top or the bottom of your library."
                        .to_string(),
                },
                options.into_iter().map(|option| vec![option]).collect(),
                decisions,
                provider,
            )?;
            let bottom = decisions
                .get(&decision_id)
                .and_then(Value::as_array)
                .and_then(|selected| selected.first())
                .and_then(Value::as_str)
                == Some("bottom");
            if bottom {
                let index = self.player_index(id)?;
                let library = &mut self.state.players[index].library;
                if let Some(position) = library
                    .iter()
                    .rposition(|card| card.instance_id == *card_id)
                {
                    // The library's top is its last card, its bottom index 0.
                    let card = library.remove(position);
                    library.insert(0, card);
                }
            }
        }
        let winner = clash_winner(&revealed);
        self.record_event(
            "clash",
            Some(player_id.clone()),
            Some(stack_object.card.instance_id.clone()),
            json!({
                "stackId": stack_object.id,
                "players": clashers,
                "winner": winner,
            }),
        );
        bindings.insert(
            bind.clone(),
            RuntimeBinding::Integer(i32::from(winner.as_deref() == Some(player_id.as_str()))),
        );
        bindings.insert(
            format!("{bind}:opponent"),
            RuntimeBinding::Player(opponent_id),
        );
        bindings.insert(format!("{bind}:player"), RuntimeBinding::Player(player_id));
        Ok(())
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "random_outcome_tests.rs"]
mod tests;
