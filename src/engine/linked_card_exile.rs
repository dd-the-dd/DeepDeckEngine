//! Linked card exiles (CR 607.2a): a card an ability exiles from a zone
//! other than the battlefield (a hand, a library, a graveyard, the stack) is
//! linked to the ability's source object when the card composition pass
//! pairs that exile with a reader of the same face (`linkToSource`). The
//! link is the same `exiledWithSource` record a linked permanent exile
//! writes, so every reader (`cardsExiledWithSource`, `cardExiledWithSource`)
//! answers both kinds. A card exile never returns when the source leaves
//! (that duration is a permanent exile's `until`).

use super::*;

impl GameEngine {
    /// Whether the exile primitive asks for its cards to be linked.
    pub(super) fn exile_links_to_source(effect: &Value) -> bool {
        effect["linkToSource"].as_bool() == Some(true)
    }

    /// Records the link of one card the resolving ability just exiled. The
    /// card must be in exile; its current definition is the last known
    /// information a reader of the link uses once the card left exile.
    pub(super) fn register_linked_exiled_card(
        &mut self,
        source: &CardInstance,
        exiled_id: &str,
    ) -> Result<(), EngineError> {
        let definition = self
            .state
            .players
            .iter()
            .flat_map(|player| player.exile.iter())
            .find(|card| card.instance_id == exiled_id)
            .map(|card| card.definition.clone())
            .ok_or_else(|| EngineError::new("a linked card exile requires the card in exile"))?;
        self.register_linked_exiled_permanent(source, exiled_id, false, &definition)
    }
}
