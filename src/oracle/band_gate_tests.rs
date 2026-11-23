use super::*;
use crate::oracle::model::{AbilitySource, RecognizerProvenance};

fn ability(face: Option<&str>, text: &str, status: &str) -> ParsedOracleAbility {
    ParsedOracleAbility {
        source: AbilitySource {
            face_id: face.map(str::to_string),
            text: text.to_string(),
            line_start: 0,
            line_end: 0,
        },
        ability_type: "staticAbility".to_string(),
        status: status.to_string(),
        recognizer: RecognizerProvenance::Unclaimed,
        rule: (status == "canonical").then(|| serde_json::json!({ "kind": "staticAbility" })),
        entities: Vec::new(),
        iterations: Vec::new(),
        diagnostics: Vec::new(),
    }
}

#[test]
fn level_station_and_solved_bands_refuse_their_canonical_bodies() {
    // Halimar Wavewatch: `Islandwalk` sits in the LEVEL 5+ band.
    let mut abilities = vec![
        ability(None, "Level up {2}", "unsupported"),
        ability(None, "LEVEL 1-4", "unsupported"),
        ability(None, "0/6", "unsupported"),
        ability(None, "LEVEL 5+", "unsupported"),
        ability(None, "6/6", "unsupported"),
        ability(None, "Islandwalk", "canonical"),
    ];
    reject_ungated_band_bodies(&mut abilities);
    let islandwalk = &abilities[5];
    assert_eq!(islandwalk.status, "unsupported");
    assert!(islandwalk.rule.is_none());
    assert_eq!(islandwalk.diagnostics[0].code, "ungated_band_body");
    assert_eq!(
        islandwalk.diagnostics[0].state.as_deref(),
        Some("card-structure.band.ungated-body")
    );
    // A failing body keeps its own grammar diagnostic.
    assert!(abilities[2].diagnostics.is_empty());

    // Station and Case prefixes gate only their own line.
    let mut abilities = vec![
        ability(None, "8+ | Flying, vigilance, haste", "canonical"),
        ability(
            None,
            "Whenever you sacrifice a land, each opponent loses 2 life.",
            "canonical",
        ),
        ability(
            None,
            "Solved \u{2014} Creatures you control get +1/+0.",
            "canonical",
        ),
    ];
    reject_ungated_band_bodies(&mut abilities);
    assert_eq!(abilities[0].status, "unsupported");
    assert_eq!(abilities[1].status, "canonical");
    assert_eq!(abilities[2].status, "unsupported");
}

#[test]
fn near_misses_are_not_band_introducers() {
    let mut abilities = vec![
        // Loyalty costs, counters and level-up keyword lines are not bands.
        ability(None, "+2: Draw a card.", "canonical"),
        ability(None, "Level up {1}{W}", "canonical"),
        ability(None, "Put a +1/+1 counter on target creature.", "canonical"),
        // A level band ends with its face.
        ability(Some("front"), "LEVEL 2+", "unsupported"),
        ability(Some("back"), "Flying", "canonical"),
    ];
    reject_ungated_band_bodies(&mut abilities);
    assert!(
        abilities
            .iter()
            .filter(|ability| ability.source.text != "LEVEL 2+")
            .all(|ability| ability.status == "canonical")
    );
}
