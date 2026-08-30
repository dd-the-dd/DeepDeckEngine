use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

const LEGACY_LONG_EXACT_COMPARISON_LIMIT: usize = 312;
const LEGACY_TEXT_MATCH_LIMIT: usize = 35;
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
fn legacy_complete_oracle_text_matching_can_only_shrink() {
    let source = include_str!("../src/oracle/canonical/mod.rs");
    let comparisons = long_exact_text_comparisons(source);
    let matches = text_match_blocks(source);
    let anchored_regexes = long_anchored_regexes(source);

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
fn extracted_canonical_modules_do_not_match_complete_oracle_text() {
    let canonical = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/oracle/canonical");
    let legacy_monolith = canonical.join("mod.rs");
    let test_corpus = canonical.join("tests.rs");

    for path in rust_sources(&canonical) {
        if path == legacy_monolith || path == test_corpus {
            continue;
        }
        let source = fs::read_to_string(&path).expect("canonical source must be readable");
        assert_eq!(
            long_exact_text_comparisons(&source),
            0,
            "{} matches a complete Oracle ability; extracted modules must compose primitives",
            path.display()
        );
        assert_eq!(
            text_match_blocks(&source),
            0,
            "{} dispatches on complete Oracle text; extracted modules must compose primitives",
            path.display()
        );
    }
}
