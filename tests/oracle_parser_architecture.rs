use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

const LEGACY_LONG_EXACT_COMPARISON_LIMIT: usize = 312;
const LEGACY_TEXT_MATCH_LIMIT: usize = 35;
// This ceiling dropped from 301 when the graveyard-exile/prepare card-shaped
// regex was replaced with move, quantity, criteria, zone, and state primitives.
const LEGACY_LONG_ANCHORED_REGEX_LIMIT: usize = 300;

fn rust_sources(directory: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    for entry in fs::read_dir(directory).expect("canonical parser directory must be readable") {
        let path = entry
            .expect("canonical parser entry must be readable")
            .path();
        if path.is_dir() {
            sources.extend(rust_sources(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(path);
        }
    }
    sources
}

fn long_exact_text_comparisons(source: &str) -> usize {
    let comparison = Regex::new(r#"(?s)\btext\s*==\s*"((?:\\.|[^"\\])*)""#)
        .expect("exact Oracle text comparison regex must compile");
    comparison
        .captures_iter(source)
        .filter(|captures| captures[1].len() >= 40)
        .count()
}

fn text_match_blocks(source: &str) -> usize {
    Regex::new(r"(?s)\bmatch\s+text\s*\{")
        .expect("Oracle text match regex must compile")
        .find_iter(source)
        .count()
}

fn long_anchored_regexes(source: &str) -> usize {
    let plain_raw = Regex::new(r#"(?s)Regex::new\(\s*r"([^"]*)""#)
        .expect("plain raw Rust regex finder must compile");
    let hashed_raw = Regex::new(r##"(?s)Regex::new\(\s*r#"(.*?)"#"##)
        .expect("hashed raw Rust regex finder must compile");
    let inline_flags = Regex::new(r"^\(\?[a-z]+\)\^").expect("inline flag finder must compile");

    plain_raw
        .captures_iter(source)
        .chain(hashed_raw.captures_iter(source))
        .filter(|captures| {
            let pattern = &captures[1];
            pattern.len() >= 80
                && (pattern.starts_with('^') || inline_flags.is_match(pattern))
                && pattern.ends_with('$')
        })
        .count()
}

#[test]
fn complete_oracle_text_matching_can_only_shrink_across_the_parser() {
    let canonical = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/oracle/canonical");
    let test_corpus = canonical.join("tests.rs");
    let production_sources = rust_sources(&canonical)
        .into_iter()
        .filter(|path| path != &test_corpus)
        .map(|path| fs::read_to_string(path).expect("canonical source must be readable"))
        .collect::<Vec<_>>();
    let comparisons = production_sources
        .iter()
        .map(|source| long_exact_text_comparisons(source))
        .sum::<usize>();
    let matches = production_sources
        .iter()
        .map(|source| text_match_blocks(source))
        .sum::<usize>();
    let anchored_regexes = production_sources
        .iter()
        .map(|source| long_anchored_regexes(source))
        .sum::<usize>();

    assert!(
        comparisons <= LEGACY_LONG_EXACT_COMPARISON_LIMIT,
        "complete Oracle text comparisons increased from {LEGACY_LONG_EXACT_COMPARISON_LIMIT} to {comparisons}; compose grammar primitives instead"
    );
    assert!(
        matches <= LEGACY_TEXT_MATCH_LIMIT,
        "match text blocks increased from {LEGACY_TEXT_MATCH_LIMIT} to {matches}; dispatch through grammar primitives instead"
    );
    assert!(
        anchored_regexes <= LEGACY_LONG_ANCHORED_REGEX_LIMIT,
        "whole-ability-shaped regexes increased from {LEGACY_LONG_ANCHORED_REGEX_LIMIT} to {anchored_regexes}; decompose the ability into grammar primitives instead"
    );
}

#[test]
fn canonical_root_only_composes_parser_domains() {
    let source = include_str!("../src/oracle/canonical/mod.rs");
    assert_eq!(long_exact_text_comparisons(source), 0);
    assert_eq!(text_match_blocks(source), 0);
    assert_eq!(long_anchored_regexes(source), 0);
    assert!(
        source.lines().count() <= 60,
        "canonical/mod.rs is growing back into a monolith"
    );
}
