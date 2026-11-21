//! Landwalk (CR 702.14): `[land subtype]walk` evasion keywords.
//!
//! The keyword vocabulary is the land family of the versioned CR 205.3
//! subtype snapshot shared with the Oracle compiler; no subtype is listed in
//! code. The blocking restriction itself lives in `Engine::can_block`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

const SUBTYPE_SNAPSHOT_JSON: &str =
    include_str!("../../data/oracle-vocabulary/noncreature-subtypes-cr-205.3-2026-11-06.json");

static LANDWALK_KEYWORDS: OnceLock<BTreeMap<String, String>> = OnceLock::new();

/// Lowercase keyword name (`swampwalk`) to its land subtype (`Swamp`).
pub(super) fn landwalk_keywords() -> &'static BTreeMap<String, String> {
    LANDWALK_KEYWORDS.get_or_init(|| {
        let snapshot: serde_json::Value = serde_json::from_str(SUBTYPE_SNAPSHOT_JSON)
            .expect("the versioned CR 205.3 subtype snapshot is valid JSON");
        snapshot["canonical"]
            .as_array()
            .expect("canonical subtypes")
            .iter()
            .filter(|entry| entry["family"] == "land")
            .filter_map(|entry| entry["irName"].as_str())
            .filter(|name| name.chars().all(|c| c.is_ascii_alphabetic()))
            .map(|name| {
                (
                    format!("{}walk", name.to_ascii_lowercase()),
                    name.to_owned(),
                )
            })
            .collect()
    })
}

/// Whether `kind` names a landwalk keyword (`desertwalk`, `swampwalk`, ...).
pub(super) fn is_landwalk_keyword(kind: &str) -> bool {
    landwalk_keywords().contains_key(kind)
}
