use std::collections::BTreeSet;

use rand::{Rng, seq::SliceRandom};

use super::CardInstance;

/// Shuffles a library while keeping excluded objects at their current indices.
///
/// Search effects use this for the rule that found cards are not part of a
/// subsequent shuffle while they still physically remain in the library. An
/// empty exclusion, or exclusions that do not occur in this library, delegates
/// to the ordinary whole-library shuffle unchanged.
pub(super) fn shuffle_library_excluding<R: Rng + ?Sized>(
    library: &mut [CardInstance],
    excluded_card_ids: &[String],
    rng: &mut R,
) {
    let excluded = excluded_card_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    shuffle_values_excluding_by(library, &excluded, rng, |card| card.instance_id.as_str());
}

fn shuffle_values_excluding_by<T, R, F>(
    values: &mut [T],
    excluded: &BTreeSet<&str>,
    rng: &mut R,
    identity: F,
) where
    R: Rng + ?Sized,
    F: Fn(&T) -> &str,
{
    let movable_indices = values
        .iter()
        .enumerate()
        .filter_map(|(index, value)| (!excluded.contains(identity(value))).then_some(index))
        .collect::<Vec<_>>();

    if movable_indices.len() == values.len() {
        values.shuffle(rng);
        return;
    }

    for index in (1..movable_indices.len()).rev() {
        let swap_with = rng.gen_range(0..=index);
        values.swap(movable_indices[index], movable_indices[swap_with]);
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "library_shuffle_tests.rs"]
mod tests;

impl super::GameEngine {
    /// The players whose libraries hold at least one of `cards`, in seat
    /// order, each once: the libraries a `shuffle X into a library`
    /// instruction shuffles after the move (CR 701.24b). A card no longer in
    /// a library (replaced elsewhere, CR 614.6) names no library.
    pub(super) fn libraries_holding(&self, cards: &[String]) -> Vec<String> {
        self.state
            .players
            .iter()
            .filter(|player| {
                player
                    .library
                    .iter()
                    .any(|card| cards.iter().any(|id| *id == card.instance_id))
            })
            .map(|player| player.id.clone())
            .collect()
    }
}
