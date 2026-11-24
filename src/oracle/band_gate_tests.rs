use serde_json::json;

use super::*;
use crate::oracle::model::AbilitySource;

fn lines(texts: &[&str]) -> Vec<AbilitySource> {
    texts
        .iter()
        .enumerate()
        .map(|(index, text)| AbilitySource {
            face_id: None,
            text: (*text).to_string(),
            line_start: index,
            line_end: index,
        })
        .collect()
}

fn gate(slot: BandSlot) -> Option<BandGate> {
    match slot {
        BandSlot::Body(body) => Some(body.gate),
        BandSlot::Header | BandSlot::Outside => None,
    }
}

const fn level(minimum: u32, maximum: Option<u32>) -> BandGate {
    BandGate::CounterRange {
        counter: "level",
        minimum,
        maximum,
    }
}

#[test]
fn level_bands_gate_every_following_body_line() {
    // Halimar Wavewatch: `Islandwalk` sits in the LEVEL 5+ band.
    let sources = lines(&[
        "Level up {2}",
        "LEVEL 1-4",
        "0/6",
        "LEVEL 5+",
        "6/6",
        "Islandwalk",
    ]);
    let slots = plan_band_slots("Creature \u{2014} Merfolk Soldier", &sources);
    assert_eq!(slots[0], BandSlot::Outside);
    assert_eq!(slots[1], BandSlot::Header);
    assert_eq!(gate(slots[2]), Some(level(1, Some(4))));
    assert_eq!(slots[3], BandSlot::Header);
    assert_eq!(gate(slots[4]), Some(level(5, None)));
    assert_eq!(gate(slots[5]), Some(level(5, None)));
}

#[test]
fn a_level_power_toughness_box_is_a_gated_base_statistics_static() {
    let BandSlot::Body(body) = plan_band_slots("Creature", &lines(&["LEVEL 1-4", "0/6"]))[1] else {
        panic!("the P/T box is a band body");
    };
    let BandPowerToughness::Rule(rule) = band_power_toughness("0/6", &body) else {
        panic!("a level P/T box is lowered");
    };
    assert_eq!(rule["modifiers"][0]["kind"], "setBasePowerToughness");
    assert_eq!(rule["modifiers"][0]["toughness"]["value"], 6);
    assert_eq!(
        rule["bandGate"],
        json!({ "kind": "counterRange", "counter": "level", "minimum": 1, "maximum": 4 })
    );
    // An ordinary ability line is left to the shared parser.
    assert!(matches!(
        band_power_toughness("Islandwalk", &body),
        BandPowerToughness::NotPowerToughness
    ));
}

#[test]
fn a_station_band_strips_its_introducer_and_spans_the_following_lines() {
    // Hearthhull: the 8+ band includes the line printed after it.
    let sources = lines(&[
        "Station (Tap another creature you control: ...)",
        "2+ | {1}, {T}, Sacrifice a land: Draw two cards.",
        "8+ | Flying, vigilance, haste",
        "Whenever you sacrifice a land, each opponent loses 2 life.",
    ]);
    let slots = plan_band_slots("Legendary Artifact \u{2014} Spacecraft", &sources);
    assert_eq!(slots[0], BandSlot::Outside);
    let BandSlot::Body(first) = slots[1] else {
        panic!("a station header carries its first ability");
    };
    assert_eq!(
        band_body_source(sources[1].clone(), &first).text,
        "{1}, {T}, Sacrifice a land: Draw two cards."
    );
    let charge = |minimum| BandGate::CounterRange {
        counter: "charge",
        minimum,
        maximum: None,
    };
    assert_eq!(first.gate, charge(2));
    assert_eq!(gate(slots[2]), Some(charge(8)));
    let BandSlot::Body(continued) = slots[3] else {
        panic!("the band continues on the next line");
    };
    assert_eq!(continued.gate, charge(8));
    // A continued line keeps its whole text.
    assert_eq!(
        band_body_source(sources[3].clone(), &continued).text,
        sources[3].text
    );
    // A station P/T box also makes the permanent a creature: explicit gap.
    assert!(matches!(
        band_power_toughness("5/5", &continued),
        BandPowerToughness::Unsupported(_)
    ));
}

#[test]
fn a_solved_line_is_its_own_band() {
    let sources = lines(&[
        "Solved \u{2014} Creatures you control get +1/+0.",
        "Creatures you control have vigilance.",
    ]);
    let slots = plan_band_slots("Enchantment \u{2014} Case", &sources);
    let BandSlot::Body(body) = slots[0] else {
        panic!("a solved line is a band body");
    };
    assert_eq!(body.gate, BandGate::Solved);
    assert_eq!(
        band_body_source(sources[0].clone(), &body).text,
        "Creatures you control get +1/+0."
    );
    assert_eq!(slots[1], BandSlot::Outside);
}

#[test]
fn near_misses_are_not_band_introducers() {
    let sources = lines(&[
        // Loyalty costs, counters and level-up keyword lines are not bands.
        "+2: Draw a card.",
        "Level up {1}{W}",
        "Put a +1/+1 counter on target creature.",
        // Die-roll result rows outside a Spacecraft or Station face.
        "15+ | Scry 2.",
        "2+ players | Draw a card.",
        // A reversed level range is not a header.
        "LEVEL 4-1",
    ]);
    assert!(
        plan_band_slots("Sorcery", &sources)
            .into_iter()
            .all(|slot| slot == BandSlot::Outside)
    );
}

#[test]
fn a_solved_line_is_a_band_only_on_a_case_face() {
    // CR 702.169: the solved designation belongs to Cases. On another face
    // `Solved —` opens no gated body, so its text is never compiled as an
    // ungated ability under a band it does not have.
    let sources = lines(&["Solved \u{2014} Creatures you control get +1/+0."]);
    assert!(matches!(
        plan_band_slots("Enchantment \u{2014} Case", &sources)[0],
        BandSlot::Body(BandBody {
            gate: BandGate::Solved,
            introducer: BandIntroducer::CaseSolved,
            ..
        })
    ));
    for type_line in [
        "Enchantment",
        "Enchantment \u{2014} Aura",
        "Legendary Enchantment",
    ] {
        assert_eq!(
            plan_band_slots(type_line, &sources)[0],
            BandSlot::Outside,
            "{type_line}"
        );
    }
    // The gate is lowered as the solved designation the runtime reads.
    assert_eq!(
        BandGate::Solved.lower(),
        json!({ "kind": "designation", "designation": "solved" })
    );
}
