use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

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

fn is_production_source(path: &Path) -> bool {
    let name = path.file_name().unwrap().to_string_lossy();
    name != "tests.rs" && !name.ends_with("_tests.rs")
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
fn canonical_parser_has_no_whole_text_recognizer() {
    let canonical = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/oracle/canonical");
    let production_sources = rust_sources(&canonical)
        .into_iter()
        .filter(|path| is_production_source(path))
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
        comparisons == 0,
        "found {comparisons} complete Oracle text comparisons; compose token automata instead"
    );
    assert!(
        matches == 0,
        "found {matches} match text blocks; dispatch through token automata instead"
    );
    assert!(
        anchored_regexes == 0,
        "found {anchored_regexes} whole-ability regexes; compose token automata instead"
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

#[test]
fn entire_oracle_parser_has_no_regex_or_opaque_effect_fallback() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/oracle");
    for path in rust_sources(&root)
        .into_iter()
        .filter(|path| is_production_source(path))
    {
        let source = fs::read_to_string(&path).expect("Oracle source must be readable");
        for forbidden in [
            "Regex::new",
            "regex::Regex",
            "regex!",
            "regex_cache",
            "resolveTriggeredInstruction",
        ] {
            assert!(
                !source.contains(forbidden),
                "{} reintroduces `{forbidden}` into the strict parser",
                path.display()
            );
        }
    }
}
