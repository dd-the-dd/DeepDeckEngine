//! Explicit fixture admission, called by builders before any gameplay/query.
//! Never recaptures an existing designation and never overwrites tracked objects.
use super::object_identity_initialization::ConstructionAdmission;
use super::{EngineError, GameEngine};
impl GameEngine {
    pub(super) fn admit_explicit_test_fixture_objects(&mut self) -> Result<(), EngineError> {
        if self.quantity_transaction_in_progress() {
            return Err(EngineError::new(
                "test fixture admission requires construction scope",
            ));
        }
        self.admit_construction_objects(ConstructionAdmission::FixtureExtensions)
    }
}
