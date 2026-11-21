//! Compatibility for existing private tests of supported boards only.
//! Production callers use the fallible enumerator; errors never become vectors.
use super::*;

impl GameEngine {
    pub(super) fn legal_priority_actions(&self, index: usize) -> Vec<LegalAction> {
        self.try_legal_priority_actions(index)
            .expect("existing test fixture must have supported legal-action discovery")
    }
}
