use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};

fn parse_card(
    card_name: &str,
    type_line: &str,
    mana_cost: &str,
    oracle_text: &str,
) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_card(
        "Generic Keyword Creature",
        "Creature — Shapeshifter",
        "{3}",
        oracle_text,
    )
}

#[test]
fn exact_reality_fracture_cards_are_not_stolen_by_the_bounded_claim() {
    for (card_name, type_line, mana_cost, oracle_text, legacy_ability_index) in [
        (
            "Unflinching Hortimancer",
            "Creature — Human Cleric",
            "{1}{W}",
            "Ward {1} (Whenever this creature becomes the target of a spell or ability an opponent controls, counter it unless that player pays {1}.)\nWhenever you gain life, put a +1/+1 counter on this creature.",
            0,
        ),
        (
            "Wrecking Gecko",
            "Artifact Creature — Lizard Construct",
            "{4}{G}",
            "Ward {2} (Whenever this creature becomes the target of a spell or ability an opponent controls, counter it unless that player pays {2}.)\n{6}{G}{G}: This creature gets +4/+4 and gains trample until end of turn.",
            0,
        ),
        (
            "Ruric Thar, Biomagus",
            "Legendary Creature — Ogre Crab Wizard",
            "{4}{U}{U}",
            "Flying\nProwess, prowess (Whenever you cast a noncreature spell, this creature gets +1/+1 until end of turn twice.)\nWhenever Ruric Thar becomes the target of a spell or ability an opponent controls, draw a card.",
            1,
        ),
    ] {
        let parsed = parse_card(card_name, type_line, mana_cost, oracle_text);

        assert_eq!(parsed.status, "canonical", "{card_name}");
        assert_eq!(
            parsed.abilities[legacy_ability_index].recognizer,
            RecognizerProvenance::Legacy,
            "{card_name} reminder grammar must remain at the explicit legacy boundary"
        );
    }
}

#[test]
fn ward_and_repeated_keyword_reminders_remain_generic_legacy_siblings() {
    for oracle_text in [
        "Ward {3} (Whenever this creature becomes the target of a spell or ability an opponent controls, counter it unless that player pays {3}.)",
        "Prowess, prowess (Whenever you cast a noncreature spell, this creature gets +1/+1 until end of turn twice.)",
    ] {
        let parsed = parse(oracle_text);
        let ability = &parsed.abilities[0];

        assert_eq!(parsed.status, "canonical", "{oracle_text}");
        assert_eq!(ability.recognizer, RecognizerProvenance::Legacy);
        assert!(ability.rule.is_some());
    }
}

#[test]
fn complete_native_keyword_lists_still_commit() {
    for oracle_text in [
        "Flying, trample",
        "Deathtouch, first strike, menace",
        "Flying, vigilance, lifelink, ward {2}",
    ] {
        let parsed = parse(oracle_text);
        assert_eq!(parsed.status, "canonical", "{oracle_text}");
        assert_eq!(
            parsed.abilities[0].recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
    }
}

#[test]
fn dangling_connector_and_bare_ward_remain_committed_failures() {
    for (oracle_text, expected_state) in [
        ("Flying,", "keyword-list.item"),
        ("Flying, ward", "keyword.ward.cost"),
        ("Ward", "keyword.ward.cost"),
    ] {
        let parsed = parse(oracle_text);
        let ability = &parsed.abilities[0];

        assert_eq!(ability.status, "unsupported", "{oracle_text}");
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert!(ability.rule.is_none());
        assert_eq!(
            ability.diagnostics[0].automaton.as_deref(),
            Some("simple-keyword-list")
        );
        assert_eq!(
            ability.diagnostics[0].state.as_deref(),
            Some(expected_state)
        );
    }
}

#[test]
fn a_keyword_adjective_sentence_remains_outside_the_native_claim() {
    let parsed = parse("Flying creatures you control have vigilance.");

    assert_ne!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
}
