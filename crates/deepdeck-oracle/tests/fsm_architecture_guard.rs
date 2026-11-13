use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const PROTECTED_FILES: &[&str] = &[
    "src/oracle/canonical/abilities/additional_cost_native.rs",
    "src/oracle/canonical/abilities/alternative_cost_native.rs",
    "src/oracle/canonical/abilities/activated_library_search_native.rs",
    "src/oracle/canonical/abilities/activated_native.rs",
    "src/oracle/canonical/abilities/keyword_native.rs",
    "src/oracle/canonical/abilities/mana_native.rs",
    "src/oracle/canonical/abilities/replicate_native.rs",
    "src/oracle/canonical/abilities/scoped_mana_native.rs",
    "src/oracle/canonical/effects/counter.rs",
    "src/oracle/canonical/effects/casting_reduction.rs",
    "src/oracle/canonical/effects/exile.rs",
    "src/oracle/canonical/effects/activated_zone_change.rs",
    "src/oracle/canonical/effects/hand_disruption.rs",
    "src/oracle/canonical/effects/zone_change_search.rs",
    "src/oracle/canonical/effects/top_card_partition.rs",
    "src/oracle/canonical/machine.rs",
    "src/oracle/syntax/name_resolver.rs",
    "src/oracle/syntax/tokenizer.rs",
];

const PROTECTED_AUTOMATA_ROOT: &str = "src/oracle/canonical/automata";

const FORBIDDEN_REGEX_MARKERS: &[&str] = &["Regex::new", "regex::Regex", "regex!", "regex_cache"];

// Representative card names that have driven the migration. Production FSM
// modules describe reusable grammar concepts; card fixtures belong in tests.
const FORBIDDEN_CARD_NAMES: &[&str] = &[
    "Lightning Helix",
    "Burst Lightning",
    "Day of Black Sun",
    "Secret Invasion",
    "Wan Shi Tong",
    "Reality Fracture",
    "Swords to Plowshares",
    "Erode",
    "Path to Exile",
    "Negate",
    "Llanowar Elves",
    "Stock Up",
    "Force of Will",
    "Duress",
    "Wasteland",
    "Arcane Signet",
    "Consign to Memory",
    "Flooded Strand",
    "Blasphemous Act",
    "Bitter Triumph",
    "Faerie Macabre",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root resolves")
}

fn rust_files_below(root: &Path, files: &mut Vec<PathBuf>) {
    if !root.exists() {
        return;
    }
    for entry in fs::read_dir(root).expect("protected automata directory is readable") {
        let path = entry.expect("directory entry is readable").path();
        if path.is_dir() {
            rust_files_below(&path, files);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

fn protected_sources() -> Vec<PathBuf> {
    let root = workspace_root();
    let mut files = PROTECTED_FILES
        .iter()
        .map(|relative| root.join(relative))
        .collect::<Vec<_>>();
    rust_files_below(&root.join(PROTECTED_AUTOMATA_ROOT), &mut files);
    files.sort();
    files.dedup();
    files
}

fn is_production_source(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_none_or(|name| name != "tests.rs" && !name.ends_with("_tests.rs"))
}

fn contains_bounded_phrase(source: &str, phrase: &str) -> bool {
    source.match_indices(phrase).any(|(start, matched)| {
        let end = start + matched.len();
        let begins_at_boundary = source[..start]
            .chars()
            .next_back()
            .is_none_or(|character| !character.is_alphanumeric());
        let ends_at_boundary = source[end..]
            .chars()
            .next()
            .is_none_or(|character| !character.is_alphanumeric());
        begins_at_boundary && ends_at_boundary
    })
}

#[test]
fn card_name_guard_requires_alphanumeric_phrase_boundaries() {
    assert!(contains_bounded_phrase(
        "a production branch for negate is forbidden",
        "negate"
    ));
    assert!(!contains_bounded_phrase(
        "a value can be negated generically",
        "negate"
    ));
    assert!(!contains_bounded_phrase("renegate", "negate"));
}

#[test]
fn dedicated_test_basenames_are_not_production_sources() {
    for path in [
        "src/oracle/canonical/automata/tests.rs",
        "src/oracle/canonical/machine_tests.rs",
        "src/oracle/canonical/automata/foo_tests.rs",
    ] {
        assert!(
            !is_production_source(Path::new(path)),
            "{path} should be classified as a dedicated test file"
        );
    }
}

#[test]
fn ordinary_rust_basename_is_a_production_source() {
    assert!(is_production_source(Path::new(
        "src/oracle/canonical/automata/foo.rs"
    )));
}

#[test]
fn test_like_directories_do_not_exempt_production_basenames() {
    for path in [
        "src/oracle/canonical/automata/tests/foo.rs",
        "src/oracle/canonical/automata/foo_tests/helpers.rs",
        "src/oracle/canonical/automata/tests.rs/foo.rs",
    ] {
        assert!(
            is_production_source(Path::new(path)),
            "{path} should still be scanned as production"
        );
    }
}

fn relative(path: &Path) -> String {
    path.strip_prefix(workspace_root())
        .expect("protected source stays in the workspace")
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn new_fsm_path_contains_no_regex_or_card_named_production_logic() {
    let mut violations = Vec::new();
    for path in protected_sources()
        .into_iter()
        .filter(|path| is_production_source(path))
    {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
        for marker in FORBIDDEN_REGEX_MARKERS {
            if source.contains(marker) {
                violations.push(format!("{} contains forbidden `{marker}`", relative(&path)));
            }
        }
        let lowercase = source.to_lowercase();
        for card_name in FORBIDDEN_CARD_NAMES {
            if contains_bounded_phrase(&lowercase, &card_name.to_lowercase()) {
                violations.push(format!(
                    "{} contains card-specific name `{card_name}`",
                    relative(&path)
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "the regex-free generic FSM boundary was violated:\n{}",
        violations.join("\n")
    );
}

#[test]
fn protected_production_modules_do_not_embed_tests() {
    let mut violations = Vec::new();
    for path in protected_sources()
        .into_iter()
        .filter(|path| is_production_source(path))
    {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
        if source.contains("#[test]") || source.contains("mod tests {") {
            violations.push(format!(
                "{} embeds tests; move them to a dedicated test module or integration test",
                relative(&path)
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "production/test separation was violated:\n{}",
        violations.join("\n")
    );
}

#[test]
fn legacy_allowlist_cannot_exempt_the_new_fsm_boundary() {
    let root = workspace_root();
    let allowlist_path = root.join("scripts/fsm-legacy-regex-allowlist.json");
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(&allowlist_path)
            .unwrap_or_else(|error| panic!("could not read {}: {error}", allowlist_path.display())),
    )
    .expect("legacy regex allowlist is valid JSON");

    assert_eq!(manifest["schemaVersion"], "fsm-legacy-regex-allowlist/v1");
    let entries = manifest["entries"]
        .as_array()
        .expect("allowlist entries are an array");
    assert!(!entries.is_empty(), "legacy debt must be explicitly listed");

    let protected = PROTECTED_FILES
        .iter()
        .copied()
        .chain(std::iter::once(PROTECTED_AUTOMATA_ROOT))
        .collect::<Vec<_>>();
    for entry in entries {
        let path = entry["path"]
            .as_str()
            .expect("every allowlist entry has a path")
            .trim_end_matches("/**");
        assert!(
            !protected.iter().any(|candidate| {
                // A broad legacy directory may temporarily contain an
                // explicitly protected native file. `protected_sources`
                // still scans that file directly, so the parent allowlist
                // cannot exempt it. What remains forbidden is allowlisting
                // the protected file/root itself or a child below it.
                path == *candidate || path.starts_with(&format!("{candidate}/"))
            }),
            "legacy allowlist must never cover protected FSM path `{path}`"
        );
        assert!(
            entry["reason"]
                .as_str()
                .is_some_and(|reason| !reason.is_empty()),
            "legacy entry `{path}` needs a reason"
        );
        assert!(
            entry["removalCondition"]
                .as_str()
                .is_some_and(|condition| !condition.is_empty()),
            "legacy entry `{path}` needs a removal condition"
        );
    }
}
