use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, parse_oracle_card_native},
};

// Executability belongs at the public parser/runtime boundary. Keeping these
// checks here lets private parser tests stay independent of the engine crate.
#[test]
fn shared_effect_programs_keep_their_executable_runtime_contracts() {
    let cases: &[(&str, &str, &str, &[&str])] = &[
        (
            "Generic Destruction",
            "Sorcery",
            "Destroy all nonland permanents your opponents control.",
            &["destroyPermanent"],
        ),
        (
            "Generic Destruction",
            "Sorcery",
            "Destroy all creatures. Target creature gets +1/+1 until end of turn.",
            &["destroyPermanent", "modifyPowerToughness"],
        ),
        (
            "Generic Destruction",
            "Sorcery",
            "Destroy all creatures, then draw a card.",
            &["destroyPermanent", "drawCards"],
        ),
        (
            "Generic Modifier",
            "Instant",
            "Target creature gets -2/-2 until end of turn.",
            &["modifyPowerToughness"],
        ),
        (
            "Generic Modifier",
            "Instant",
            "Target creature gets +2/+2 and gains reach until end of turn. Untap it.",
            &["modifyPowerToughness", "grantKeyword", "untapPermanent"],
        ),
        (
            "Generic Modifier",
            "Instant",
            "Target creature gets +3/+3 until end of turn. Draw a card.",
            &["modifyPowerToughness", "drawCards"],
        ),
        (
            "Generic Modifier",
            "Instant",
            "Target creature and target land get +1/+1 until end of turn.",
            &["modifyPowerToughness", "modifyPowerToughness"],
        ),
        (
            "Generic Explorer",
            "Enchantment",
            "You may play two additional lands on each of your turns.",
            &["additionalLandPlay"],
        ),
    ];

    for &(card_name, type_line, text, expected_primitives) in cases {
        let parsed = parse_oracle_card_native(OracleCardParseRequest {
            card_name: card_name.to_string(),
            type_line: type_line.to_string(),
            mana_cost: None,
            oracle_text: Some(text.to_string()),
            layout: None,
            faces: Vec::new(),
        });
        assert_eq!(
            parsed.status, "canonical",
            "{text}: {:?}",
            parsed.diagnostics
        );
        assert_eq!(parsed.abilities.len(), 1, "{text}");
        let rule = parsed.abilities[0]
            .rule
            .as_ref()
            .expect("the complete ability must emit canonical IR");
        assert!(rule_is_executable(rule), "{text}: {rule}");
        let actions = if type_line == "Enchantment" {
            &rule["modifiers"]
        } else {
            &rule["effects"]
        };
        let primitives: Vec<_> = actions
            .as_array()
            .expect("ordered primitive actions")
            .iter()
            .map(|action| action["kind"].as_str().expect("a primitive kind"))
            .collect();
        assert_eq!(primitives, expected_primitives, "{text}");
    }
}
