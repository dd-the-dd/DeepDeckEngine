use std::collections::BTreeSet;

use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};

use super::shuffle_values_excluding_by;

fn shuffle(values: &mut [String], excluded: &[&str], seed: u64) {
    let excluded = excluded.iter().copied().collect::<BTreeSet<_>>();
    shuffle_values_excluding_by(
        values,
        &excluded,
        &mut StdRng::seed_from_u64(seed),
        String::as_str,
    );
}

#[test]
fn excluded_objects_remain_at_their_exact_library_indices() {
    let mut library = ["a", "found", "b", "also-found", "c"].map(str::to_string);

    shuffle(&mut library, &["found", "also-found"], 143);

    assert_eq!(library[1], "found");
    assert_eq!(library[3], "also-found");
}

#[test]
fn nonexcluded_objects_are_still_shuffled() {
    let original = ["a", "found", "b", "c", "d"].map(str::to_string);
    let library = (0..64)
        .find_map(|seed| {
            let mut candidate = original.clone();
            shuffle(&mut candidate, &["found"], seed);
            (candidate != original).then_some(candidate)
        })
        .expect("at least one deterministic seed permutes the nonexcluded objects");

    assert_eq!(library[1], "found");
    let mut actual = library.into_iter().collect::<Vec<_>>();
    let mut expected = original.into_iter().collect::<Vec<_>>();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
}

#[test]
fn empty_exclusion_is_identical_to_the_ordinary_shuffle() {
    let original = ["a", "b", "c", "d"].map(str::to_string);
    let mut with_empty_exclusion = original.clone();
    let mut ordinary = original;

    shuffle(&mut with_empty_exclusion, &[], 144);
    ordinary.shuffle(&mut StdRng::seed_from_u64(144));

    assert_eq!(with_empty_exclusion, ordinary);
}

#[test]
fn excluded_objects_outside_this_library_are_ignored() {
    let original = ["a", "b", "c", "d"].map(str::to_string);
    let mut with_unrelated_exclusion = original.clone();
    let mut ordinary = original;

    shuffle(
        &mut with_unrelated_exclusion,
        &["card-in-another-zone"],
        145,
    );
    ordinary.shuffle(&mut StdRng::seed_from_u64(145));

    assert_eq!(with_unrelated_exclusion, ordinary);
}
