//! The departure record of a card that left a graveyard (CR 400.7).
//!
//! Every site that moves cards out of a graveyard already publishes the
//! departure to the `cardsLeftGraveyard` trigger matcher; the same
//! publication records one `cardLeftGraveyard` event per card, so that the
//! one event-history evaluator answers `if a card left your graveyard this
//! turn`. The record's player owns the graveyard (CR 404.2); its object is
//! the card, with the type line it had as it left.
use super::*;

/// The recorded event kind of one card leaving a graveyard.
pub(super) const CARD_LEFT_GRAVEYARD: &str = "cardLeftGraveyard";

impl GameEngine {
    pub(super) fn record_cards_left_graveyard(&mut self, player_id: &str, card_ids: &[String]) {
        for card_id in card_ids {
            let definition = self
                .permanent_position(card_id)
                .map(|(player, index)| &self.state.players[player].battlefield[index].definition)
                .or_else(|| {
                    self.card_outside_battlefield(card_id)
                        .map(|card| &card.definition)
                })
                .or_else(|| {
                    self.state
                        .stack
                        .iter()
                        .find(|object| object.card.instance_id == *card_id)
                        .map(|object| &object.card.definition)
                });
            let detail = definition.map_or_else(
                || json!({}),
                |definition| {
                    json!({
                        "typeLine": definition.type_line,
                        "wasToken": definition.is_token,
                    })
                },
            );
            self.record_event(
                CARD_LEFT_GRAVEYARD,
                Some(player_id.to_string()),
                Some(card_id.clone()),
                detail,
            );
        }
    }
}
