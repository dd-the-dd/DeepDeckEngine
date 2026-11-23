//! Real Oracle text -> band-gated IR -> runtime for a leveler (CR 711.2a-b):
//! the abilities and P/T box printed inside a level band are the permanent's
//! only while its level counters lie in that band. Every permanent-level
//! keyword reader (effective keyword, effective and printed predicates,
//! static selectors) observes the same gate; a card off the battlefield has
//! no counters and reads every band closed.
use super::*;

const SKYWATCHER_ADEPT: &str = "Level up {3} ({3}: Put a level counter on this. Level up only as a sorcery.)\nLEVEL 1-2\n2/2\nFlying\nLEVEL 3+\n4/2\nFlying";

fn skywatcher_adept() -> CardDefinition {
    let request = OracleCardParseRequest {
        card_name: "Skywatcher Adept".to_string(),
        type_line: "Creature \u{2014} Merfolk Wizard".to_string(),
        mana_cost: Some("{U}".to_string()),
        oracle_text: Some(SKYWATCHER_ADEPT.to_string()),
        layout: Some("leveler".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let mut definition = test_definition("skywatcher-adept", "Creature \u{2014} Merfolk Wizard");
    definition.name = "Skywatcher Adept".to_string();
    definition.mana_cost = "{U}".to_string();
    definition.power = Some("1".to_string());
    definition.toughness = Some("1".to_string());
    definition.rules = parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        // `Level up` is a template keyword: its playable rule is its
        // expansion (an activated ability putting a level counter).
        .flat_map(|rule| crate::oracle::keyword_expansion_rules(&rule))
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect();
    assert_eq!(
        definition
            .rules
            .iter()
            .filter(|rule| rule.get("bandGate").is_some())
            .count(),
        4,
        "both P/T boxes and both flying lines carry their band gate: {:#?}",
        definition.rules
    );
    definition
}

fn adept_at_level(level: i32) -> (GameEngine, CardInstance) {
    let mut engine = test_engine(2);
    let mut adept = test_instance("adept", skywatcher_adept(), "player-0");
    if level > 0 {
        adept.counters.insert("level".to_string(), level);
    }
    engine.state.players[0].battlefield.push(adept.clone());
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    (engine, adept)
}

fn with_flying() -> Value {
    json!({ "kind": "hasKeyword", "value": "flying" })
}

fn flying_readers(engine: &GameEngine, adept: &CardInstance) -> [bool; 5] {
    let creatures_with_flying = json!({
        "kind": "permanents",
        "where": { "kind": "and", "operands": [
            { "kind": "cardTypeContains", "value": "Creature" },
            with_flying(),
        ] },
    });
    [
        engine.permanent_has_effective_keyword(adept, "flying"),
        engine.permanent_matches_effective_filter(adept, &with_flying()),
        matches_permanent_filter(adept, &with_flying()),
        matches_card_filter(
            object_predicate::DefinitionView::for_permanent(adept),
            &with_flying(),
        ),
        GameEngine::static_selector_matches(adept, &creatures_with_flying, adept),
    ]
}

fn power_toughness(engine: &GameEngine) -> (i32, i32) {
    let stats = GameEngine::effective_power_toughness(&engine.state);
    let adept = &stats["adept"];
    (adept.current_power, adept.current_toughness)
}

#[test]
fn a_level_band_keyword_is_read_only_while_its_band_is_open() {
    for (level, flying, stats) in [
        (0, false, (1, 1)),
        (1, true, (2, 2)),
        (2, true, (2, 2)),
        (3, true, (4, 2)),
        (7, true, (4, 2)),
    ] {
        let (engine, adept) = adept_at_level(level);
        assert_eq!(
            flying_readers(&engine, &adept),
            [flying; 5],
            "level {level}: every permanent keyword reader observes the band gate"
        );
        assert_eq!(power_toughness(&engine), stats, "level {level}");
    }
}

#[test]
fn a_leveler_off_the_battlefield_reads_its_bands_closed() {
    // CR 122.2: a card in another zone has no counters, so no band is open;
    // its printed definition still has no flying.
    let adept = skywatcher_adept();
    assert!(!has_keyword(&adept, "flying"));
    assert!(!matches_card_filter(&adept, &with_flying()));
}
