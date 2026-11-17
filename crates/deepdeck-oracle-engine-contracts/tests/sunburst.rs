use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, parse_oracle_card},
};

fn parse(
    name: &str,
    type_line: &str,
    mana_cost: &str,
    text: &str,
) -> deepdeck_oracle_engine_contracts::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

/// CR 702.44: Sunburst compiles with or without its reminder to the keyword
/// primitive that the runtime executes on entry.
#[test]
fn sunburst_compiles_with_or_without_reminder_text() {
    for (name, type_line, text) in [
        (
            "Engineered Explosives",
            "Artifact",
            "Sunburst (This artifact enters with a charge counter on it for each color of mana spent to cast it.)\n{2}, Sacrifice this artifact: Destroy each nonland permanent with mana value equal to the number of charge counters on this artifact.",
        ),
        (
            "Skyreach Manta",
            "Artifact Creature — Fish",
            "Sunburst (This creature enters with a +1/+1 counter on it for each color of mana spent to cast it.)\nFlying",
        ),
        ("Test", "Artifact", "Sunburst"),
    ] {
        let result = parse(name, type_line, "{X}", text);
        assert_eq!(
            result.status,
            "canonical",
            "{name}: {:?}",
            result
                .abilities
                .iter()
                .flat_map(|ability| ability.diagnostics.iter())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            result.abilities[0].rule.as_ref().unwrap()["ability"]["kind"],
            "sunburst"
        );
        assert!(
            result
                .abilities
                .iter()
                .filter_map(|ability| ability.rule.as_ref())
                .all(rule_is_executable),
            "{name}"
        );
    }
}
