//! Explicit fixture admission, called by builders before any gameplay/query.
//! Never recaptures an existing designation and never overwrites tracked objects.
use super::object_identity::existing_object_identity_of;
use super::object_identity_initialization::ConstructionAdmission;
use super::{CardInstance, EngineError, GameEngine};
impl GameEngine {
    pub(super) fn admit_explicit_test_fixture_objects(&mut self) -> Result<(), EngineError> {
        if self.quantity_transaction_in_progress() {
            return Err(EngineError::new(
                "test fixture admission requires construction scope",
            ));
        }
        self.admit_construction_objects(ConstructionAdmission::FixtureExtensions)?;
        let mut stack = std::mem::take(&mut self.state.stack);
        let adopted = stack
            .iter_mut()
            .filter(|object| !super::card_zone_transition::stack_requires_checkpoint(object))
            .try_for_each(|object| self.adopt_fixture_ability_source_identity(&mut object.card));
        self.state.stack = stack;
        adopted
    }

    /// An ability put on the stack carries a copy of its source, including the
    /// source's current incarnation marker. A fixture that copied the source
    /// before admission adopts that marker from the live carrier. It never
    /// replaces an existing marker and never mints a new incarnation.
    pub(super) fn adopt_fixture_ability_source_identity(
        &self,
        card: &mut CardInstance,
    ) -> Result<(), EngineError> {
        if existing_object_identity_of(card)?.is_some() {
            return Ok(());
        }
        let mut carriers = self.state.players.iter().flat_map(|player| {
            player
                .library
                .iter()
                .chain(&player.hand)
                .chain(&player.graveyard)
                .chain(&player.battlefield)
                .chain(&player.exile)
                .chain(&player.command_zone)
        });
        let Some(carrier) = carriers.find(|carrier| carrier.instance_id == card.instance_id) else {
            return Ok(());
        };
        if let Some(identity) = existing_object_identity_of(carrier)? {
            Self::assign_tracked_zone_identity(card, &identity.zone_identity);
        }
        Ok(())
    }
}
