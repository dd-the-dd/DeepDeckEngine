use deepdeck_oracle::oracle::{OracleCardParseRequest, parse_oracle_card};

#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Namor the Sub-Mariner; see docs/coverage-targets.md"
)]
#[test]
fn mana_symbol_cast_count_parses_without_compiling_the_engine_runtime() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Namor the Sub-Mariner".to_string(),
        type_line: "Legendary Creature - Merfolk Noble Hero".to_string(),
        mana_cost: Some("{2}{U}{U}".to_string()),
        oracle_text: Some(
            "Whenever you cast a noncreature spell with one or more blue mana symbols in its mana cost, create that many 1/1 blue Merfolk creature tokens."
                .to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("Namor's ability should produce primitive IR");
    assert_eq!(rule["event"]["kind"], "spellCast");
    assert_eq!(rule["event"]["manaSymbol"], "U");
    assert_eq!(
        rule["effects"][0]["quantity"]["kind"],
        "countManaSymbolsInTriggeringSpell"
    );
    assert_eq!(rule["effects"][0]["quantity"]["symbol"], "U");
}
