use mtg_engine::engine::rule_is_executable;
use mtg_engine::oracle::{OracleCardParseRequest, parse_oracle_card};

fn assert_card_is_executable(name: &str, type_line: &str, mana_cost: &str, oracle_text: &str) {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(
        parsed.status, "canonical",
        "{name}: {:#?}",
        parsed.abilities
    );
    assert_eq!(
        parsed.abilities.len(),
        1,
        "{name} should expose one tested ability"
    );
    let ability = &parsed.abilities[0];
    let rule = ability
        .rule
        .as_ref()
        .unwrap_or_else(|| panic!("{name}: unsupported ability {}", ability.source.text));
    assert!(
        rule_is_executable(rule),
        "{name}: unexecutable rule for {}: {rule}",
        ability.source.text
    );
}

#[test]
fn hoc_cards_2_through_12_selected_batch_is_canonical_and_executable() {
    for (name, type_line, mana_cost, oracle_text) in [
        (
            "Gandalf, Party Guest",
            "Legendary Creature - Avatar Wizard",
            "{3}{W}{U}",
            "At the beginning of combat on your turn, you may cast an instant or sorcery spell with mana value X or less from your hand without paying its mana cost, where X is twice the number of legendary Wizards you control.",
        ),
        (
            "Thorin, King of Durin's Folk",
            "Legendary Creature - Dwarf Noble",
            "{2}{R}{W}",
            "Other Dwarves you control get +1/+0 for each artifact token you control.",
        ),
        (
            "Bilbo, Fellow Conspirator",
            "Legendary Creature - Halfling Rogue",
            "{1}{B}{R}",
            "If you would create a Food token, instead create a Food token and a Treasure token.",
        ),
        (
            "Ori, Plate Stacker",
            "Legendary Creature - Dwarf Artificer",
            "{2}{W}{B}",
            "When Ori enters, destroy all artifacts and enchantments your opponents control. You gain 1 life for each permanent destroyed this way.",
        ),
        (
            "Long-Lost Lances",
            "Artifact",
            "{2}{W}",
            "During your turn, creatures you control that are equipped have first strike and vigilance.",
        ),
        (
            "Dragon-Cursed Halls",
            "Artifact",
            "{3}",
            "{1}, {T}: Until end of turn, target creature gains \"Whenever this creature deals combat damage to a player, create a Treasure token.\"",
        ),
        (
            "Smaug the Impenetrable",
            "Legendary Creature - Dragon",
            "{5}{R}",
            "Whenever Smaug is dealt noncombat damage, create that many Treasure tokens.",
        ),
        (
            "Bilbo's Burglaring",
            "Sorcery",
            "{4}{U}{U}",
            "For each opponent, gain control of up to one target artifact that player controls.",
        ),
        (
            "Dragon's Desire",
            "Sorcery",
            "{2}{R}",
            "Add {R} for each artifact your opponents control.",
        ),
        (
            "Necklace of Girion",
            "Artifact",
            "{2}",
            "Whenever you cast a green spell and whenever a Forest you control enters, put a +1/+1 counter on target creature you control.",
        ),
    ] {
        assert_card_is_executable(name, type_line, mana_cost, oracle_text);
    }
}
