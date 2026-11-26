//! Miracle (CR 702.94).
//!
//! CR 702.94a: `Miracle [cost]` means "You may reveal this card from your
//! hand as you draw it if it's the first card you've drawn this turn. When
//! you reveal this card this way, you may cast it by paying [cost] rather
//! than its mana cost."
//!
//! The keyword is composed from existing primitives, as madness is
//! (CR 702.35): the reveal is recorded as the card is drawn, and the linked
//! trigger is one ordinary resolution-time cast (`castOneCard`) of the card
//! from its owner's hand, optional, paying the miracle cost as the spell's
//! alternative cost (CR 118.9). The cast reads the casting-permission and
//! cost machinery unchanged; nothing here builds a cast action.
//!
//! The first-draw condition reads the action log: the draw just recorded is
//! the drawing player's only `cardDrawn` record this turn. A card that left
//! the hand before the trigger resolves is a new object the cast no longer
//! finds (CR 400.7), so the trigger does nothing.

use super::*;

/// The reveal reason recorded on the miracle card (CR 702.94a).
const MIRACLE_REVEAL_REASON: &str = "miracle";

/// The miracle cost of a card's printed `Miracle` keyword, as the payment
/// it prints (`payMana`), when the runtime supports it.
fn miracle_cost_payment(card: &CardDefinition) -> Option<Value> {
    card.printed_rules()
        .iter()
        .filter(|rule| value_kind(rule) == Some("keywordAbility"))
        .find(|rule| miracle_ability_supported(&rule["ability"]))
        .map(|rule| rule["ability"]["cost"].clone())
}

/// CR 702.94a, second ability: the revealed card's owner may cast it by
/// paying its miracle cost rather than its mana cost. One resolution-time
/// cast of the card in its owner's hand; its other costs (additional costs,
/// increases) still apply (CR 601.2f).
pub(super) fn miracle_trigger_rule(cost: &Value) -> Value {
    let owner = json!({ "kind": "ownerOf", "object": { "kind": "self" } });
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "effects": [{
            "kind": "castOneCard",
            "player": owner,
            "cards": { "kind": "self" },
            "sourceZone": "hand",
            "normalCostsApply": true,
            "optional": true,
            "bind": "miracleCast",
            "alternativeCost": {
                "kind": "alternativeCost",
                "mode": "miracle",
                "costs": [cost.clone()],
            },
        }],
    })
}

impl GameEngine {
    /// After a card is drawn: when it is the first card its player drew
    /// this turn and it has a supported miracle keyword, it is revealed and
    /// its miracle trigger is put on the stack (CR 702.94a). Any later draw
    /// of the turn reveals nothing.
    pub(super) fn enqueue_miracle_trigger(&mut self, drawing_player_id: &str) {
        let turn = self.state.turn_number;
        let mut draws = self.state.events.iter().filter(|event| {
            event.turn_number == turn
                && event.kind == "cardDrawn"
                && event.player_id.as_deref() == Some(drawing_player_id)
        });
        let (Some(first), None) = (draws.next(), draws.next()) else {
            return;
        };
        let Some(card_id) = first.card_instance_id.clone() else {
            return;
        };
        let Ok(player_index) = self.player_index(drawing_player_id) else {
            return;
        };
        let Some(mut source) = self.state.players[player_index]
            .hand
            .iter()
            .find(|card| card.instance_id == card_id)
            .cloned()
        else {
            return;
        };
        let Some(cost) = miracle_cost_payment(&source.definition) else {
            return;
        };
        // "You" of a card in a hand is its owner (CR 108.4a).
        source.controller = source.owner.clone();
        self.record_event(
            "cardRevealed",
            Some(source.owner.clone()),
            Some(source.instance_id.clone()),
            json!({ "reason": MIRACLE_REVEAL_REASON }),
        );
        let _ = self.put_triggered_ability_on_stack(&source, miracle_trigger_rule(&cost));
    }
}
