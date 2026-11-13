use std::cell::Cell;

use super::{
    recognize_entities, resolve_unknown_names, ExactNameTokenIndex, NameLinkKind,
    NameResolutionContext, NamedCardIndex,
};

#[derive(Default)]
struct ExactNames(ExactNameTokenIndex);

impl ExactNames {
    fn with(name: &str) -> Self {
        Self(ExactNameTokenIndex::from_names([name]))
    }
}

impl NamedCardIndex for ExactNames {
    fn exact_name_ends(
        &self,
        tokens: &[super::super::model::RecognizedEntity],
        start: usize,
        maximum_end: usize,
    ) -> Vec<usize> {
        self.0.exact_name_ends(tokens, start, maximum_end)
    }
}

fn context(card: &str) -> NameResolutionContext {
    NameResolutionContext::new(card, std::iter::empty())
}

#[test]
fn longest_unknown_run_resolves_to_the_current_card() {
    let text = "Lightning Helix deals 3 damage to any target.";
    let tokens = recognize_entities(text);
    let result = resolve_unknown_names(
        text,
        &tokens,
        &context("Lightning Helix"),
        &ExactNames::default(),
    )
    .expect("the two unknown words link as one exact current-card name");
    let links = result.resolved;

    assert_eq!(result.state, super::NameResolverState::Complete);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].kind, NameLinkKind::CurrentCard);
    assert_eq!(links[0].canonical_name, "Lightning Helix");
    assert_eq!(links[0].raw, "Lightning Helix");
    assert_eq!((links[0].token_start, links[0].token_end), (0, 2));
    assert_eq!((links[0].span_start, links[0].span_end), (0, 15));
    let alias_attempt = result
        .trace
        .iter()
        .find(|transition| transition.state == super::NameResolverState::TryContextAlias)
        .expect("the trace exposes the contextual alias attempt");
    assert_eq!(alias_attempt.memory.checkpoint, 0);
    assert_eq!(alias_attempt.memory.position, 2);
    assert_eq!(
        alias_attempt.memory.candidate.as_deref(),
        Some("Lightning Helix")
    );
    assert_eq!(
        alias_attempt.memory.alias_tried.as_deref(),
        Some("Lightning Helix")
    );
    assert!(result
        .trace
        .iter()
        .any(|transition| transition.state == super::NameResolverState::CommitLongest));
    assert_eq!(
        result.trace.last().map(|transition| transition.state),
        Some(super::NameResolverState::Complete)
    );
}

#[test]
fn ascii_possessive_multi_token_name_resolves_to_the_current_card() {
    let text = "Test Goyf's unmappedlexeme is equal to 1.";
    let tokens = recognize_entities(text);
    let failure =
        resolve_unknown_names(text, &tokens, &context("Test Goyf"), &ExactNames::default())
            .expect_err("unrelated semantic vocabulary remains explicitly unresolved");

    assert_eq!(failure.resolved.len(), 1);
    assert_eq!(failure.unresolved[0].raw, "unmappedlexeme");
    let link = &failure.resolved[0];
    assert_eq!(link.kind, NameLinkKind::CurrentCard);
    assert_eq!(link.canonical_name, "Test Goyf");
    assert_eq!(link.raw, "Test Goyf");
    assert_eq!((link.token_start, link.token_end), (0, 2));
    assert_eq!((link.span_start, link.span_end), (0, "Test Goyf".len()));
}

#[test]
fn typographic_possessive_name_resolves_without_including_the_suffix_span() {
    let text = "Barrowgoyf’s unmappedlexeme is equal to 1.";
    let tokens = recognize_entities(text);
    let failure = resolve_unknown_names(
        text,
        &tokens,
        &context("Barrowgoyf"),
        &ExactNames::default(),
    )
    .expect_err("unrelated semantic vocabulary remains explicitly unresolved");

    assert_eq!(failure.resolved.len(), 1);
    assert_eq!(failure.unresolved[0].raw, "unmappedlexeme");
    let link = &failure.resolved[0];
    assert_eq!(link.kind, NameLinkKind::CurrentCard);
    assert_eq!(link.raw, "Barrowgoyf");
    assert_eq!((link.token_start, link.token_end), (0, 1));
    assert_eq!((link.span_start, link.span_end), (0, "Barrowgoyf".len()));
}

#[test]
fn possessive_other_name_is_not_guessed_as_the_current_card() {
    let text = "Tarmogoyf's unmappedlexeme is equal to 1.";
    let tokens = recognize_entities(text);
    let failure = resolve_unknown_names(
        text,
        &tokens,
        &context("Barrowgoyf"),
        &ExactNames::with("Tarmogoyf"),
    )
    .expect_err("a non-current catalog name must not gain contextual possessive matching");

    assert!(failure.resolved.is_empty());
    assert_eq!(failure.unresolved[0].raw, "Tarmogoyf's unmappedlexeme");
}

#[test]
fn a_partial_name_is_not_fuzzily_accepted() {
    let text = "Lightning deals 3 damage.";
    let tokens = recognize_entities(text);
    let failure = resolve_unknown_names(
        text,
        &tokens,
        &context("Lightning Helix"),
        &ExactNames::default(),
    )
    .expect_err("a non-declared abbreviation must remain unresolved");

    assert!(failure.resolved.is_empty());
    assert_eq!(failure.unresolved.len(), 1);
    assert_eq!(failure.unresolved[0].raw, "Lightning");
}

#[test]
fn known_vocabulary_inside_a_rejected_context_name_stays_in_the_failure_range() {
    let text = "Other Spark deals 2 damage to any target.";
    let tokens = recognize_entities(text);
    let failure = resolve_unknown_names(
        text,
        &tokens,
        &context("Test Spark"),
        &ExactNames::default(),
    )
    .expect_err("a different two-token name must not be guessed as the current card");

    assert!(failure.resolved.is_empty());
    assert_eq!(failure.unresolved.len(), 1);
    assert_eq!(failure.unresolved[0].raw, "Other Spark");
    assert_eq!(
        (
            failure.unresolved[0].token_start,
            failure.unresolved[0].token_end
        ),
        (0, 2)
    );
    assert_eq!(
        (
            failure.unresolved[0].span_start,
            failure.unresolved[0].span_end
        ),
        (0, "Other Spark".len())
    );
}

#[test]
fn rejected_context_range_does_not_absorb_unrelated_known_vocabulary() {
    let text = "Other Ember deals 2 damage to any target.";
    let tokens = recognize_entities(text);
    let failure = resolve_unknown_names(
        text,
        &tokens,
        &context("Test Spark"),
        &ExactNames::default(),
    )
    .expect_err("an unrelated unknown word must remain locally unresolved");

    assert!(failure.resolved.is_empty());
    assert_eq!(failure.unresolved.len(), 1);
    assert_eq!(failure.unresolved[0].raw, "Ember");
    assert_eq!(
        (
            failure.unresolved[0].token_start,
            failure.unresolved[0].token_end
        ),
        (1, 2)
    );
}

#[test]
fn comma_prefix_is_an_explicit_non_fuzzy_self_alias() {
    let text = "Nicol Bolas deals 3 damage to any target.";
    let tokens = recognize_entities(text);
    let result = resolve_unknown_names(
        text,
        &tokens,
        &context("Nicol Bolas, the Ravager"),
        &ExactNames::default(),
    )
    .expect("the exact prefix before a comma is an allowed alias");
    let links = result.resolved;

    assert_eq!(links[0].kind, NameLinkKind::CurrentCard);
    assert_eq!(links[0].canonical_name, "Nicol Bolas, the Ravager");
    assert_eq!(links[0].raw, "Nicol Bolas");
}

#[test]
fn a_transform_face_name_links_to_the_current_face() {
    let text = "Insectile Aberration attacks.";
    let tokens = recognize_entities(text);
    let faces = ["Delver of Secrets", "Insectile Aberration"];
    let context = NameResolutionContext::new(
        "Delver of Secrets // Insectile Aberration",
        faces.iter().copied(),
    );
    let result = resolve_unknown_names(text, &tokens, &context, &ExactNames::default())
        .expect("the face name resolves and the event word is already known");
    let links = result.resolved;

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].kind, NameLinkKind::CurrentFace);
    assert_eq!(links[0].canonical_name, "Insectile Aberration");
}

#[test]
fn an_exact_injected_catalog_name_can_span_multiple_unknown_tokens() {
    let text = "Create a Black Lotus.";
    let tokens = recognize_entities(text);
    let result = resolve_unknown_names(
        text,
        &tokens,
        &context("Test Factory"),
        &ExactNames::with("Black Lotus"),
    )
    .expect("the injected exact index links another named card");
    let links = result.resolved;

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].kind, NameLinkKind::NamedCard);
    assert_eq!(links[0].canonical_name, "Black Lotus");
    assert_eq!((links[0].token_start, links[0].token_end), (2, 4));
}

#[test]
fn unresolved_unknowns_return_a_precise_failure_for_the_llm_queue() {
    let text = "Lightning Helix zaps whichever target.";
    let tokens = recognize_entities(text);
    let failure = resolve_unknown_names(
        text,
        &tokens,
        &context("Lightning Helix"),
        &ExactNames::default(),
    )
    .expect_err("unknown non-name vocabulary must never be swallowed");

    assert_eq!(failure.automaton, "lexical-name-resolver");
    assert_eq!(failure.state, "unresolved-name");
    assert_eq!(
        failure.current_state,
        super::NameResolverState::ReportUnresolved
    );
    assert_eq!(failure.resolved.len(), 1);
    assert_eq!(failure.unresolved.len(), 1);
    assert_eq!(failure.unresolved[0].raw, "zaps whichever");
    assert_eq!(
        (
            failure.unresolved[0].token_start,
            failure.unresolved[0].token_end
        ),
        (2, 4)
    );
    assert_eq!(
        (
            failure.unresolved[0].span_start,
            failure.unresolved[0].span_end
        ),
        (16, 30)
    );
    assert_eq!(failure.memory.position, 2);
    assert_eq!(failure.memory.checkpoint, 2);
    assert_eq!(failure.memory.candidate.as_deref(), Some("zaps whichever"));
    assert!(failure
        .trace
        .iter()
        .any(|transition| transition.state == super::NameResolverState::Rollback));
}

#[test]
fn known_words_inside_a_name_do_not_prevent_exact_linking() {
    let text = "Life from the Loam deals 3 damage.";
    let tokens = recognize_entities(text);
    let result = resolve_unknown_names(
        text,
        &tokens,
        &context("Life from the Loam"),
        &ExactNames::default(),
    )
    .expect("an exact name can contain lexically known words");
    let links = result.resolved;

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].raw, "Life from the Loam");
    assert_eq!((links[0].token_start, links[0].token_end), (0, 4));
}

struct CountingNames {
    index: ExactNameTokenIndex,
    calls: Cell<usize>,
}

impl NamedCardIndex for CountingNames {
    fn exact_name_ends(
        &self,
        tokens: &[super::super::model::RecognizedEntity],
        start: usize,
        maximum_end: usize,
    ) -> Vec<usize> {
        self.calls.set(self.calls.get() + 1);
        self.index.exact_name_ends(tokens, start, maximum_end)
    }
}

#[test]
fn catalog_lookup_is_one_longest_match_query_per_token_not_one_query_per_range() {
    let text = "Create a Black Lotus and Griselbrand.";
    let tokens = recognize_entities(text);
    let names = CountingNames {
        index: ExactNameTokenIndex::from_names(["Black Lotus", "Griselbrand"]),
        calls: Cell::new(0),
    };

    let result = resolve_unknown_names(text, &tokens, &context("Test Factory"), &names)
        .expect("both exact catalog names resolve");

    assert_eq!(
        result
            .resolved
            .iter()
            .map(|link| link.raw.as_str())
            .collect::<Vec<_>>(),
        vec!["Black Lotus", "Griselbrand"]
    );
    let tokens_before_period = tokens.iter().take_while(|token| token.raw != ".").count();
    let prior_range_lookup_count = (0..tokens_before_period)
        .flat_map(|start| (start + 1..=tokens_before_period).map(move |end| (start, end)))
        .filter(|(start, end)| {
            tokens[*start..*end]
                .iter()
                .any(|token| token.token_type == "unknown")
        })
        .count();
    assert_eq!(names.calls.get(), tokens_before_period);
    assert!(
        prior_range_lookup_count > names.calls.get(),
        "the prior range-by-range lookup must remain demonstrably more expensive"
    );
}

#[test]
fn shorter_terminal_from_one_trie_walk_remains_available_after_overlap_selection() {
    let text = "Alpha Beta Gamma Delta Epsilon Zeta Eta.";
    let tokens = recognize_entities(text);
    let index = ExactNameTokenIndex::from_names([
        "Alpha Beta",
        "Alpha Beta Gamma Delta",
        "Gamma Delta Epsilon Zeta Eta",
    ]);

    let result = resolve_unknown_names(text, &tokens, &context("Test Factory"), &index)
        .expect("the disjoint exact names cover every unknown token");

    assert_eq!(
        result
            .resolved
            .iter()
            .map(|link| link.raw.as_str())
            .collect::<Vec<_>>(),
        vec!["Alpha Beta", "Gamma Delta Epsilon Zeta Eta"]
    );
}
