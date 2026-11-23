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

fn request(type_line: &str) -> OracleCardParseRequest {
    OracleCardParseRequest {
        card_name: "Band Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: None,
        layout: None,
        faces: Vec::new(),
    }
}

fn statuses(abilities: &[ParsedOracleAbility]) -> Vec<&str> {
    abilities
        .iter()
        .map(|ability| ability.status.as_str())
        .collect()
}

#[test]
fn level_bands_refuse_their_canonical_bodies() {
    // Halimar Wavewatch: `Islandwalk` sits in the LEVEL 5+ band.
    let mut abilities = vec![
        ability(None, "Level up {2}", "unsupported"),
        ability(None, "LEVEL 1-4", "unsupported"),
        ability(None, "0/6", "unsupported"),
        ability(None, "LEVEL 5+", "unsupported"),
        ability(None, "6/6", "unsupported"),
        ability(None, "Islandwalk", "canonical"),
    ];
    reject_ungated_band_bodies(
        &request("Creature \u{2014} Merfolk Soldier"),
        &mut abilities,
    );
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
}

#[test]
fn a_station_band_spans_the_following_lines_of_a_spacecraft() {
    // Hearthhull: the 8+ band includes the line printed after it.
    let mut abilities = vec![
        ability(
            None,
            "Station (Tap another creature you control: ...)",
            "unsupported",
        ),
        ability(
            None,
            "2+ | {1}, {T}, Sacrifice a land: Draw two cards.",
            "canonical",
        ),
        ability(None, "8+ | Flying, vigilance, haste", "canonical"),
        ability(
            None,
            "Whenever you sacrifice a land, each opponent loses 2 life.",
            "canonical",
        ),
    ];
    reject_ungated_band_bodies(
        &request("Legendary Artifact \u{2014} Spacecraft"),
        &mut abilities,
    );
    assert_eq!(
        statuses(&abilities),
        ["unsupported", "unsupported", "unsupported", "unsupported"]
    );
    // The Spacecraft subtype alone identifies station thresholds.
    let mut abilities = vec![
        ability(None, "10+ | Flying", "canonical"),
        ability(
            None,
            "This creature gets +1/+1 for each basic land you control.",
            "canonical",
        ),
    ];
    reject_ungated_band_bodies(&request("Artifact \u{2014} Spacecraft"), &mut abilities);
    assert_eq!(statuses(&abilities), ["unsupported", "unsupported"]);
}

#[test]
fn a_solved_line_is_its_own_band() {
    let mut abilities = vec![
        ability(
            None,
            "Solved \u{2014} Creatures you control get +1/+0.",
            "canonical",
        ),
        ability(None, "Creatures you control have vigilance.", "canonical"),
    ];
    reject_ungated_band_bodies(&request("Enchantment \u{2014} Case"), &mut abilities);
    assert_eq!(statuses(&abilities), ["unsupported", "canonical"]);
}

#[test]
fn near_misses_are_not_band_introducers() {
    let mut abilities = vec![
        // Loyalty costs, counters and level-up keyword lines are not bands.
        ability(None, "+2: Draw a card.", "canonical"),
        ability(None, "Level up {1}{W}", "canonical"),
        ability(None, "Put a +1/+1 counter on target creature.", "canonical"),
        // Die-roll result rows outside a Spacecraft or Station face.
        ability(None, "15+ | Scry 2.", "canonical"),
        ability(None, "2+ players | Draw a card.", "canonical"),
    ];
    reject_ungated_band_bodies(&request("Sorcery"), &mut abilities);
    assert!(
        abilities
            .iter()
            .all(|ability| ability.status == "canonical")
    );

    // A level band ends with its face.
    let mut abilities = vec![
        ability(Some("front"), "LEVEL 2+", "unsupported"),
        ability(Some("back"), "Flying", "canonical"),
    ];
    reject_ungated_band_bodies(&request("Creature \u{2014} Human"), &mut abilities);
    assert_eq!(abilities[1].status, "canonical");
}
