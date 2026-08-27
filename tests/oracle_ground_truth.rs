use mtg_engine::engine::rule_is_executable;
use mtg_engine::oracle::{OracleCardFace, OracleCardParseRequest, parse_oracle_card};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn count_nodes_with_kind(value: &Value, expected_kind: &str) -> usize {
    match value {
        Value::Array(values) => values
            .iter()
            .map(|value| count_nodes_with_kind(value, expected_kind))
            .sum(),
        Value::Object(object) => {
            usize::from(value["kind"] == expected_kind)
                + object
                    .values()
                    .map(|value| count_nodes_with_kind(value, expected_kind))
                    .sum::<usize>()
        }
        _ => 0,
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read_json(path: &Path) -> Value {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_str(&source)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn optional_string(value: &Value) -> Option<String> {
    value.as_str().map(ToOwned::to_owned)
}

fn assert_single_ability_executable(
    card_name: &str,
    type_line: &str,
    mana_cost: Option<&str>,
    oracle_text: &str,
) {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        faces: Vec::new(),
        layout: Some("normal".to_string()),
        mana_cost: mana_cost.map(ToOwned::to_owned),
        oracle_text: Some(oracle_text.to_string()),
        type_line: type_line.to_string(),
    });
    assert_eq!(
        result.abilities.len(),
        1,
        "{card_name} should expose one ability: {oracle_text}"
    );
    let ability = &result.abilities[0];
    assert_eq!(
        ability.status, "canonical",
        "{card_name} parser gap (classified {}): {:?}",
        ability.ability_type, ability.source.text,
    );
    let rule = ability.rule.as_ref().expect("canonical rule");
    assert!(rule_is_executable(rule), "{card_name} engine gap: {rule}");
}

fn assert_all_abilities_executable(
    card_name: &str,
    type_line: &str,
    mana_cost: Option<&str>,
    oracle_text: &str,
    expected_count: usize,
) {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        faces: Vec::new(),
        layout: Some("normal".to_string()),
        mana_cost: mana_cost.map(ToOwned::to_owned),
        oracle_text: Some(oracle_text.to_string()),
        type_line: type_line.to_string(),
    });
    assert_eq!(result.abilities.len(), expected_count, "{card_name}");
    for ability in &result.abilities {
        assert_eq!(
            ability.status, "canonical",
            "{card_name} parser gap: {}",
            ability.source.text
        );
        let rule = ability.rule.as_ref().expect("canonical rule");
        assert!(rule_is_executable(rule), "{card_name} engine gap: {rule}");
    }
}

#[test]
fn graveyard_copy_and_conditional_payment_rules_are_executable() {
    let single_abilities = [
        (
            "Seance",
            "Enchantment",
            None,
            "At the beginning of each upkeep, you may exile target creature card from your graveyard. If you do, create a token that's a copy of that card, except it's a Spirit in addition to its other types. Exile it at the beginning of the next end step.",
        ),
        (
            "Foster",
            "Enchantment",
            None,
            "Whenever a creature you control dies, you may pay {1}. If you do, reveal cards from the top of your library until you reveal a creature card. Put that card into your hand and the rest into your graveyard.",
        ),
        (
            "Mortal Obstinacy",
            "Enchantment - Aura",
            None,
            "Whenever enchanted creature deals combat damage to a player, you may sacrifice this Aura. If you do, destroy target enchantment.",
        ),
        (
            "Soul Tithe",
            "Enchantment - Aura",
            None,
            "At the beginning of the upkeep of enchanted permanent's controller, that player sacrifices it unless they pay {X}, where X is its mana value.",
        ),
        (
            "Spear of Heliod",
            "Legendary Enchantment Artifact",
            None,
            "{1}{W}{W}, {T}: Destroy target creature that dealt damage to you this turn.",
        ),
        (
            "Timely Ward",
            "Enchantment - Aura",
            None,
            "You may cast this spell as though it had flash if it targets a commander.",
        ),
    ];
    for (name, type_line, mana_cost, text) in single_abilities {
        assert_single_ability_executable(name, type_line, mana_cost, text);
    }

    assert_all_abilities_executable(
        "Mimic Vat",
        "Artifact",
        None,
        "Imprint — Whenever a nontoken creature dies, you may exile that card. If you do, return each other card exiled with this artifact to its owner's graveyard.\n{3}, {T}: Create a token that's a copy of a card exiled with this artifact. It gains haste. Exile it at the beginning of the next end step.",
        2,
    );
    assert_all_abilities_executable(
        "Caravan Vigil",
        "Sorcery",
        Some("{G}"),
        "Search your library for a basic land card, reveal it, put it into your hand, then shuffle.\nMorbid — You may put that card onto the battlefield instead of putting it into your hand if a creature died this turn.",
        2,
    );
}

#[test]
fn alternate_casting_keywords_are_canonical_and_executable() {
    assert_all_abilities_executable(
        "Selfless Safewright",
        "Creature — Human Cleric",
        Some("{2}{W}"),
        "Flash\nConvoke (Your creatures can help cast this spell. Each creature you tap while casting this spell pays for {1} or one mana of that creature's color.)\nWhen this creature enters, choose a creature type. Other permanents you control of that type gain hexproof and indestructible until end of turn.",
        3,
    );
    assert_all_abilities_executable(
        "Terminus",
        "Sorcery",
        Some("{4}{W}{W}"),
        "Put all creatures on the bottom of their owners' libraries.\nMiracle {W} (You may cast this card for its miracle cost when you draw it if it's the first card you drew this turn.)",
        2,
    );
    assert_all_abilities_executable(
        "Planar Outburst",
        "Sorcery",
        Some("{3}{W}{W}"),
        "Destroy all nonland creatures.\nAwaken 4â€”{5}{W}{W}{W} (If you cast this spell for {5}{W}{W}{W}, also put four +1/+1 counters on target land you control and it becomes a 0/0 Elemental creature with haste. It's still a land.)",
        2,
    );
    let give_take = parse_oracle_card(OracleCardParseRequest {
        card_name: "Give // Take".to_string(),
        type_line: "Sorcery // Sorcery".to_string(),
        mana_cost: Some("{2}{G} // {2}{U}".to_string()),
        oracle_text: None,
        layout: Some("split".to_string()),
        faces: vec![
            OracleCardFace {
                id: "give".to_string(),
                name: "Give".to_string(),
                type_line: "Sorcery".to_string(),
                mana_cost: Some("{2}{G}".to_string()),
                oracle_text: "Put three +1/+1 counters on target creature.\nFuse (You may cast one or both halves of this card from your hand.)".to_string(),
                power: None,
                toughness: None,
            },
            OracleCardFace {
                id: "take".to_string(),
                name: "Take".to_string(),
                type_line: "Sorcery".to_string(),
                mana_cost: Some("{2}{U}".to_string()),
                oracle_text: "Remove all +1/+1 counters from target creature you control. Draw that many cards.\nFuse (You may cast one or both halves of this card from your hand.)".to_string(),
                power: None,
                toughness: None,
            },
        ],
    });
    assert_eq!(give_take.abilities.len(), 4);
    assert!(give_take.abilities.iter().all(|ability| {
        ability.status == "canonical" && ability.rule.as_ref().is_some_and(rule_is_executable)
    }));
    assert_all_abilities_executable(
        "Stratus Dancer",
        "Creature â€” Djinn Monk",
        Some("{1}{U}"),
        "Flying\nMegamorph {1}{U} (You may cast this card face down as a 2/2 creature for {3}. Turn it face up any time for its megamorph cost and put a +1/+1 counter on it.)\nWhen this creature is turned face up, counter target instant or sorcery spell.",
        3,
    );
    assert_all_abilities_executable(
        "Kheru Spellsnatcher",
        "Creature â€” Snake Wizard",
        Some("{3}{U}"),
        "Morph {4}{U}{U} (You may cast this card face down as a 2/2 creature for {3}. Turn it face up any time for its morph cost.)\nWhen this creature is turned face up, counter target spell. If that spell is countered this way, exile it instead of putting it into its owner's graveyard. You may cast that card without paying its mana cost for as long as it remains exiled.",
        2,
    );
}

#[test]
fn delayed_mana_and_damage_history_triggers_are_executable() {
    assert_single_ability_executable(
        "Predator Ooze",
        "Creature — Ooze",
        Some("{G}{G}{G}"),
        "Whenever a creature dealt damage by this creature this turn dies, put a +1/+1 counter on this creature.",
    );
    assert_single_ability_executable(
        "Skyrider Patrol",
        "Creature — Elf Scout",
        Some("{2}{G}{U}"),
        "At the beginning of combat on your turn, you may pay {G}{U}. When you do, put a +1/+1 counter on another target creature you control, and that creature gains flying until end of turn.",
    );
    assert_single_ability_executable(
        "Plasm Capture",
        "Instant",
        Some("{G}{G}{U}{U}"),
        "Counter target spell. At the beginning of your next first main phase, add X mana in any combination of colors, where X is that spell's mana value.",
    );
}

#[test]
fn stored_creature_copy_aura_is_canonical_and_executable() {
    assert_all_abilities_executable(
        "Metamorphic Alteration",
        "Enchantment — Aura",
        Some("{1}{U}"),
        "Enchant creature\nAs this Aura enters, choose a creature.\nEnchanted creature is a copy of the chosen creature.",
        3,
    );
}

#[test]
fn remaining_sokrate_spell_and_static_families_are_executable() {
    assert_all_abilities_executable(
        "Depth Charge Colossus",
        "Artifact Creature â€” Dreadnought",
        Some("{9}"),
        "Prototype {4}{U}{U} â€” 6/6 (You may cast this spell with different mana cost, color, and size. It keeps its abilities and types.)\nThis creature doesn't untap during your untap step.\n{3}: Untap this creature.",
        3,
    );
    assert_all_abilities_executable(
        "Hindering Light",
        "Instant",
        Some("{W}{U}"),
        "Counter target spell that targets you or a permanent you control.\nDraw a card.",
        2,
    );
    assert_single_ability_executable(
        "Azorius Charm",
        "Instant",
        Some("{W}{U}"),
        "Choose one â€”\nâ€¢ Creatures you control gain lifelink until end of turn.\nâ€¢ Draw a card.\nâ€¢ Put target attacking or blocking creature on top of its owner's library.",
    );
    assert_all_abilities_executable(
        "Damping Sphere",
        "Artifact",
        Some("{2}"),
        "If a land is tapped for two or more mana, it produces {C} instead of any other type and amount.\nEach spell a player casts costs {1} more to cast for each other spell that player has cast this turn.",
        2,
    );
    assert_all_abilities_executable(
        "Arcade Gannon",
        "Legendary Creature â€” Human Doctor",
        Some("{2}{W}{U}"),
        "{T}: Draw a card, then discard a card. Put a quest counter on Arcade Gannon.\nFor Auld Lang Syne â€” Once during each of your turns, you may cast an artifact or Human spell from your graveyard with mana value less than or equal to the number of quest counters on Arcade Gannon.",
        2,
    );
    assert_all_abilities_executable(
        "Daxos of Meletis",
        "Legendary Creature â€” Human Soldier",
        Some("{1}{W}{U}"),
        "Daxos can't be blocked by creatures with power 3 or greater.\nWhenever Daxos deals combat damage to a player, exile the top card of that player's library. You gain life equal to that card's mana value. Until end of turn, you may cast that card and you may spend mana as though it were mana of any color to cast that spell.",
        2,
    );
    assert_all_abilities_executable(
        "Ao, the Dawn Sky",
        "Legendary Creature â€” Dragon Spirit",
        Some("{3}{W}{W}"),
        "Flying, vigilance\nWhen Ao dies, choose one â€”\nâ€¢ Look at the top seven cards of your library. Put any number of nonland permanent cards with total mana value 4 or less from among them onto the battlefield. Put the rest on the bottom of your library in a random order.\nâ€¢ Put two +1/+1 counters on each permanent you control that's a creature or Vehicle.",
        2,
    );
    assert_all_abilities_executable(
        "Cataclysmic Gearhulk",
        "Artifact Creature â€” Construct",
        Some("{3}{W}{W}"),
        "Vigilance\nWhen this creature enters, each player chooses an artifact, a creature, an enchantment, and a planeswalker from among the nonland permanents they control, then sacrifices the rest.",
        2,
    );
    assert_all_abilities_executable(
        "Dragon Throne of Tarkir",
        "Legendary Artifact — Equipment",
        Some("{4}"),
        "Equipped creature has defender and \"{2}, {T}: Other creatures you control gain trample and get +X/+X until end of turn, where X is this creature's power.\"\nEquip {3}",
        2,
    );
    assert_all_abilities_executable(
        "The Capitoline Triad",
        "Legendary Artifact Creature — Golem",
        Some("{10}"),
        "Those Who Came Before — This spell costs {1} less to cast for each historic card in your graveyard. (Artifacts, legendaries, and Sagas are historic.)\nExile any number of historic cards from your graveyard with total mana value 30 or greater: You get an emblem with \"Creatures you control have base power and toughness 9/9.\"",
        2,
    );
    assert_all_abilities_executable(
        "Odric, Master Tactician",
        "Legendary Creature — Human Soldier",
        Some("{2}{W}{W}"),
        "First strike (This creature deals combat damage before creatures without first strike.)\nWhenever Odric and at least three other creatures attack, you choose which creatures block this combat and how those creatures block.",
        2,
    );
}

#[test]
fn fixed_mana_spells_share_one_generic_parser_rule() {
    for (name, cost, text, expected_mana) in [
        ("Dark Ritual", "{B}", "Add {B}{B}{B}.", "{B}{B}{B}"),
        (
            "Seething Song",
            "{2}{R}",
            "Add {R}{R}{R}{R}{R}.",
            "{R}{R}{R}{R}{R}",
        ),
    ] {
        let result = parse_oracle_card(OracleCardParseRequest {
            card_name: name.to_string(),
            faces: Vec::new(),
            layout: Some("normal".to_string()),
            mana_cost: Some(cost.to_string()),
            oracle_text: Some(text.to_string()),
            type_line: "Instant".to_string(),
        });
        let ability = &result.abilities[0];
        assert_eq!(ability.status, "canonical", "{name}");
        let rule = ability.rule.as_ref().expect("fixed mana spell rule");
        assert_eq!(rule["effects"][0]["kind"], "addMana");
        assert_eq!(rule["effects"][0]["mana"], expected_mana);
        assert!(rule_is_executable(rule), "{name} engine gap: {rule}");
    }
}

#[test]
fn azula_reusable_mana_combat_and_zone_patterns_are_executable() {
    let cases = [
        (
            "Ashling, Flame Dancer",
            "Legendary Creature — Elemental Shaman",
            "You don't lose unspent red mana as steps and phases end.",
        ),
        (
            "Electro, Assaulting Battery",
            "Legendary Creature — Human Villain",
            "Whenever you cast an instant or sorcery spell, add {R}.",
        ),
        (
            "Longshot, Rebel Bowman",
            "Legendary Creature — Human Rebel Archer",
            "Noncreature spells you cast cost {1} less to cast.",
        ),
        (
            "Longshot, Rebel Bowman",
            "Legendary Creature — Human Rebel Archer",
            "Whenever you cast a noncreature spell, Longshot deals 2 damage to each opponent.",
        ),
        (
            "Storm-Kiln Artist",
            "Creature — Dwarf Shaman",
            "This creature gets +1/+0 for each artifact you control.",
        ),
        (
            "Firebending Student",
            "Creature — Human Monk",
            "Firebending X, where X is this creature's power. (Whenever this creature attacks, add X {R}. This mana lasts until end of combat.)",
        ),
        (
            "Ozai, the Phoenix King",
            "Legendary Creature — Human Noble",
            "Trample, firebending 4, haste",
        ),
        (
            "Borne Upon a Wind",
            "Instant",
            "You may cast spells this turn as though they had flash.",
        ),
        (
            "Deadly Precision",
            "Sorcery",
            "As an additional cost to cast this spell, pay {4} or sacrifice an artifact or creature.",
        ),
        ("Deadly Precision", "Sorcery", "Destroy target creature."),
        ("Deadly Rollick", "Instant", "Exile target creature."),
        (
            "Epic Downfall",
            "Sorcery",
            "Exile target creature with mana value 3 or greater.",
        ),
        (
            "Fire Nation Attacks",
            "Sorcery",
            "Create two 2/2 red Soldier creature tokens with firebending 1. (Whenever a creature with firebending 1 attacks, add {R}. This mana lasts until end of combat.)",
        ),
        (
            "Rending Volley",
            "Instant",
            "Rending Volley deals 4 damage to target white or blue creature.",
        ),
        (
            "Run Amok",
            "Instant",
            "Target attacking creature gets +3/+3 and gains trample until end of turn.",
        ),
        (
            "How to Start a Riot",
            "Sorcery",
            "Target creature gains menace until end of turn.",
        ),
        (
            "How to Start a Riot",
            "Sorcery",
            "Creatures target player controls get +2/+0 until end of turn.",
        ),
        (
            "Azula, on the Hunt",
            "Legendary Creature — Human Noble",
            "Whenever Azula attacks, you lose 1 life and create a Clue token. (It's an artifact with \"{2}, Sacrifice this token: Draw a card.\")",
        ),
        (
            "Callous Inspector",
            "Creature — Human Detective",
            "When this creature dies, it deals 1 damage to you. Create a Clue token. (It's an artifact with \"{2}, Sacrifice this token: Draw a card.\")",
        ),
        (
            "Faerie Mastermind",
            "Creature — Faerie Rogue",
            "Whenever an opponent draws their second card each turn, you draw a card.",
        ),
        (
            "The Unagi of Kyoshi Island",
            "Legendary Creature — Serpent",
            "Whenever an opponent draws their second card each turn, you draw two cards.",
        ),
        (
            "Fire Nation Sentinels",
            "Creature — Human Soldier",
            "Whenever a nontoken creature an opponent controls dies, put a +1/+1 counter on each creature you control.",
        ),
        (
            "Mai, Jaded Edge",
            "Legendary Creature — Human Assassin",
            "Exhaust — {3}: Put a double strike counter on Mai. (Activate each exhaust ability only once.)",
        ),
        (
            "Vindictive Warden",
            "Creature — Human Soldier",
            "{3}: This creature deals 1 damage to each opponent.",
        ),
        (
            "Fire Nation Archers",
            "Creature — Human Soldier Archer",
            "{5}: This creature deals 2 damage to each opponent. Create a 2/2 red Soldier creature token.",
        ),
        (
            "Fire Nation Turret",
            "Artifact",
            "Remove fifty charge counters from this artifact: It deals 50 damage to any target.",
        ),
        (
            "Zhao, the Moon Slayer",
            "Legendary Creature — Human Soldier",
            "{7}: Put a conqueror counter on Zhao.",
        ),
        (
            "Sozin's Comet",
            "Sorcery",
            "Each creature you control gains firebending 5 until end of turn. (Whenever it attacks, add {R}{R}{R}{R}{R}. This mana lasts until end of combat.)",
        ),
        (
            "The Last Agni Kai",
            "Sorcery",
            "Until end of turn, you don't lose unspent red mana as steps and phases end.",
        ),
    ];
    for (name, type_line, oracle_text) in cases {
        assert_single_ability_executable(name, type_line, None, oracle_text);
    }
}

#[test]
fn remaining_azula_rules_are_canonical_and_executable() {
    let cases = [
        (
            "Azula, Cunning Usurper",
            "Legendary Creature â€” Human Noble",
            "During your turn, you may cast cards exiled with Azula and you may cast them as though they had flash. Mana of any type can be spent to cast those spells.",
        ),
        (
            "Lost in Memories",
            "Enchantment â€” Aura",
            "Enchanted creature gets +1/+1 and has \"Whenever this creature deals combat damage to a player, target instant or sorcery card in your graveyard gains flashback until end of turn. The flashback cost is equal to its mana cost.\" (You may cast that card from your graveyard for its flashback cost. Then exile it.)",
        ),
        (
            "Ozai, the Phoenix King",
            "Legendary Creature â€” Human Noble",
            "If you would lose unspent mana, that mana becomes red instead.",
        ),
        (
            "Ozai, the Phoenix King",
            "Legendary Creature â€” Human Noble",
            "Ozai has flying and indestructible as long as you have six or more unspent mana.",
        ),
        (
            "Scarring Memories",
            "Sorcery",
            "You may cast this spell as though it had flash if you control an attacking legendary creature.",
        ),
        (
            "Smellerbee, Rebel Fighter",
            "Legendary Creature â€” Human Rebel",
            "Whenever Smellerbee attacks, you may discard your hand. If you do, draw cards equal to the number of attacking creatures.",
        ),
        (
            "The Rise of Sozin",
            "Enchantment â€” Saga",
            "I â€” Destroy all creatures.",
        ),
        (
            "The Rise of Sozin",
            "Enchantment â€” Saga",
            "II â€” Choose a card name. Search target opponent's graveyard, hand, and library for up to four cards with that name and exile them. Then that player shuffles.",
        ),
        (
            "The Rise of Sozin",
            "Enchantment â€” Saga",
            "Whenever Fire Lord Sozin deals combat damage to a player, you may pay {X}. When you do, put any number of target creature cards with total mana value X or less from that player's graveyard onto the battlefield under your control.",
        ),
        (
            "The Unagi of Kyoshi Island",
            "Legendary Creature â€” Serpent",
            "Wardâ€”Waterbend {4}. (Whenever this creature becomes the target of a spell or ability an opponent controls, counter it unless that player pays {4}. They can tap their artifacts and creatures to help. Each one pays for {1}.)",
        ),
        (
            "Twin Blades",
            "Artifact â€” Equipment",
            "When this Equipment enters, attach it to target creature you control. That creature gains double strike until end of turn.",
        ),
        (
            "Ragavan, Nimble Pilferer",
            "Legendary Creature â€” Monkey Pirate",
            "Dash {1}{R} (You may cast this spell for its dash cost. If you do, it gains haste, and it's returned from the battlefield to its owner's hand at the beginning of the next end step.)",
        ),
        (
            "How to Start a Riot",
            "Sorcery",
            "Target creature gains menace until end of turn. (It can't be blocked except by two or more creatures.)",
        ),
        (
            "Zhao, the Moon Slayer",
            "Legendary Creature â€” Human Soldier",
            "Nonbasic lands enter tapped.",
        ),
        (
            "Zhao, the Moon Slayer",
            "Legendary Creature â€” Human Soldier",
            "As long as Zhao has a conqueror counter on him, nonbasic lands are Mountains. (They lose all other land types and abilities and have \"{T}: Add {R}.\")",
        ),
        (
            "Spirit",
            "Token Creature â€” Spirit",
            "This token can't block or be blocked by non-Spirit creatures.",
        ),
    ];
    for (name, type_line, oracle_text) in cases {
        assert_single_ability_executable(name, type_line, None, oracle_text);
    }
}

#[test]
fn remaining_kellan_rules_are_canonical_and_executable() {
    let cards = [
        (
            "Eladamri, Korvecdal",
            "Legendary Creature — Elf Warrior",
            "{G}, {T}, Tap two untapped creatures you control: Reveal a card from your hand or the top card of your library. If you reveal a creature card this way, put it onto the battlefield. Activate only during your turn.",
        ),
        (
            "Aligned Heart",
            "Enchantment",
            "Flurry — Whenever you cast your second spell each turn, put a rally counter on this enchantment. Then create a 1/1 white Monk creature token with prowess for each rally counter on it. (Whenever you cast a noncreature spell, the token gets +1/+1 until end of turn.)",
        ),
        (
            "Clive's Hideaway",
            "Land",
            "Hideaway 4 (When this land enters, look at the top four cards of your library, exile one face down, then put the rest on the bottom in a random order.)",
        ),
        (
            "Jace Reawakened",
            "Legendary Planeswalker — Jace",
            "+1: Draw a card, then discard a card.",
        ),
        (
            "On an Adventure",
            "Card",
            "After an Adventure resolves, you can place the exiled card here. You may cast the creature from exile.",
        ),
    ];
    for (name, type_line, text) in cards {
        assert_single_ability_executable(name, type_line, None, text);
    }
}

#[test]
fn composed_entry_families_are_canonical_and_executable() {
    let abilities = [
        (
            "Variable Counter Creature",
            "Creature - Shapeshifter",
            Some("{X}{G}"),
            "This creature enters with X +1/+1 counters on it.",
        ),
        (
            "Variable Counter Enchantment",
            "Enchantment",
            Some("{X}{G}"),
            "This enchantment enters with X charge counters on it.",
        ),
        (
            "Kicked Counter Creature",
            "Creature - Ape Warrior",
            Some("{3}{G}"),
            "If Kicked Counter Creature was kicked, it enters with five +1/+1 counters on it.",
        ),
        (
            "Variable Stun Creature",
            "Creature - Fungus",
            Some("{X}{G}{U}"),
            "This creature enters with a number of stun counters on it equal to three minus X. If X is 2 or less, it enters tapped. (If a permanent with a stun counter would become untapped, remove one from it instead.)",
        ),
        (
            "Large Creature Tutor",
            "Creature - Elf",
            Some("{2}{G}"),
            "When this creature enters, you may search your library for a creature card with mana value 6 or greater, reveal it, put it into your hand, then shuffle.",
        ),
        (
            "Plains Or Creature Tutor",
            "Creature - Human Scout",
            Some("{1}{W}"),
            "When this creature enters, search your library for a basic Plains card or a creature card with mana value 1 or less, reveal it, put it into your hand, then shuffle.",
        ),
        (
            "Temporary Prison",
            "Enchantment",
            Some("{2}{W}"),
            "When this enchantment enters, exile target nonland permanent an opponent controls until this enchantment leaves the battlefield.",
        ),
        (
            "Small Creature Prison",
            "Enchantment",
            Some("{1}{W}"),
            "When this enchantment enters, exile target creature with mana value 3 or less an opponent controls until this enchantment leaves the battlefield. (That creature returns under its owner's control.)",
        ),
        (
            "Mass Counter Engine",
            "Artifact",
            Some("{6}"),
            "When this artifact enters, put a -1/-1 counter on each creature target player controls.",
        ),
        (
            "Creature Type Guardian",
            "Creature - Human Citizen",
            Some("{2}{W}"),
            "When this creature enters, choose a creature type. Other permanents you control of that type gain hexproof and indestructible until end of turn.",
        ),
        (
            "Color Choosing Aura",
            "Enchantment - Aura",
            Some("{1}{G}"),
            "As this Aura enters, choose a color.",
        ),
        (
            "Creature Choosing Aura",
            "Enchantment - Aura",
            Some("{1}{U}"),
            "As this Aura enters, choose a creature.",
        ),
        (
            "Filtered Top Five Creature",
            "Creature - Vedalken Scout",
            Some("{2}{U}"),
            "When this creature enters, look at the top five cards of your library. You may reveal a land card or a card with {X} in its mana cost from among them and put it into your hand. Put the rest on the bottom of your library in a random order.",
        ),
        (
            "Optional Top Five Land",
            "Land",
            None,
            "When this land enters, you may look at the top five cards of your library. If you do, reveal up to one basic land card from among them, then put that card on top of your library and the rest on the bottom in any order.",
        ),
        (
            "Counter Transfer Creature",
            "Creature - Plant",
            Some("{3}{G}"),
            "When this creature leaves the battlefield, put its counters on target creature you control.",
        ),
        (
            "Kicked Mass Bounce Creature",
            "Legendary Creature - Merfolk",
            Some("{6}{U}{U}"),
            "When Slinn Voda enters, if it was kicked, return all creatures to their owners' hands except for Merfolk, Krakens, Leviathans, Octopuses, and Serpents.",
        ),
        (
            "Historic Flicker Creature",
            "Creature - Merfolk Soldier",
            Some("{4}{U}"),
            "When this creature enters, you may exile target historic permanent you control. If you do, return that card to the battlefield under its owner's control at the beginning of the next end step. (Artifacts, legendaries, and Sagas are historic.)",
        ),
        (
            "Evolving Creature",
            "Creature - Human Ooze",
            Some("{G}"),
            "Evolve (Whenever a creature you control enters, if that creature has greater power or toughness than this creature, put a +1/+1 counter on this creature.)",
        ),
        (
            "Graft Land",
            "Land",
            None,
            "Graft 1 (This land enters with a +1/+1 counter on it. Whenever a creature enters, you may move a +1/+1 counter from this land onto it.)",
        ),
        (
            "Exploit Creature",
            "Creature - Zombie Wizard",
            Some("{3}{U}"),
            "Exploit (When this creature enters, you may sacrifice a creature.)",
        ),
        (
            "Exploit Payoff",
            "Creature - Zombie Wizard",
            Some("{3}{U}"),
            "When this creature exploits a creature, return to their owners' hands all creatures your opponents control with toughness less than the exploited creature's toughness.",
        ),
        (
            "Saga Reminder",
            "Enchantment - Saga",
            Some("{2}{W}"),
            "(As this Saga enters and after your draw step, add a lore counter. Sacrifice after III.)",
        ),
        (
            "Temporary Copy Creature",
            "Creature - Shapeshifter",
            Some("{1}{U}"),
            "Whenever another creature you control enters, you may have this creature become a copy of that creature until end of turn. (If it does, it loses this ability for the rest of the turn.)",
        ),
        (
            "Next Creature Enhancement",
            "Instant",
            Some("{G}"),
            "The next creature spell you cast this turn can be cast as though it had flash. That spell can't be countered. That creature enters with an additional +1/+1 counter on it.",
        ),
    ];
    for (name, type_line, mana_cost, text) in abilities {
        assert_single_ability_executable(name, type_line, mana_cost, text);
    }
}

#[test]
fn remaining_land_mana_and_static_families_are_executable() {
    let abilities = [
        (
            "Jund Panorama",
            "Land",
            "{1}, {T}, Sacrifice this land: Search your library for a basic Swamp, Mountain, or Forest card, put it onto the battlefield tapped, then shuffle.",
        ),
        (
            "Myriad Landscape",
            "Land",
            "{2}, {T}, Sacrifice this land: Search your library for up to two basic land cards that share a land type, put them onto the battlefield tapped, then shuffle.",
        ),
        (
            "Fertile Ground",
            "Enchantment - Aura",
            "Whenever enchanted land is tapped for mana, its controller adds an additional one mana of any color.",
        ),
        (
            "Market Festival",
            "Enchantment - Aura",
            "Whenever enchanted land is tapped for mana, its controller adds an additional two mana in any combination of colors.",
        ),
        (
            "Gift of Paradise",
            "Enchantment - Aura",
            "Enchanted land has \"{T}: Add two mana of any one color.\"",
        ),
        (
            "Holy Mantle",
            "Enchantment - Aura",
            "Enchanted creature gets +2/+2 and has protection from creatures.",
        ),
        (
            "Meltstrider's Resolve",
            "Enchantment - Aura",
            "Enchanted creature gets +0/+2 and can't be blocked by more than one creature.",
        ),
        (
            "Archon of Sun's Grace",
            "Creature - Archon",
            "Pegasus creatures you control have lifelink.",
        ),
        (
            "Emil, Vastlands Roamer",
            "Legendary Creature - Human Ranger",
            "Creatures you control with +1/+1 counters on them have trample.",
        ),
        (
            "Legolas Greenleaf",
            "Legendary Creature - Elf Archer",
            "Legolas can't be blocked by creatures with power 2 or less.",
        ),
        (
            "Loading Zone",
            "Enchantment",
            "Warp {G} (You may cast this card from your hand for its warp cost. Exile this enchantment at the beginning of the next end step, then you may cast it from exile on a later turn.)",
        ),
        (
            "In Bolas's Clutches",
            "Enchantment - Aura",
            "Enchant permanent",
        ),
    ];
    for (name, type_line, text) in abilities {
        assert_single_ability_executable(name, type_line, None, text);
    }
}

#[test]
fn split_linked_exile_rules_are_canonical_and_executable() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Linked Exile Enchantment".to_string(),
        faces: Vec::new(),
        layout: Some("normal".to_string()),
        mana_cost: Some("{2}{W}".to_string()),
        oracle_text: Some(
            "When this enchantment enters, exile another target nonland permanent.\nWhen this enchantment leaves the battlefield, return the exiled card to the battlefield under its owner's control."
                .to_string(),
        ),
        type_line: "Enchantment".to_string(),
    });
    assert_eq!(result.abilities.len(), 2);
    for ability in &result.abilities {
        assert_eq!(
            ability.status, "canonical",
            "parser gap: {}",
            ability.source.text
        );
        assert!(
            rule_is_executable(ability.rule.as_ref().expect("canonical rule")),
            "engine gap: {}",
            ability.source.text
        );
    }
}

#[test]
fn toph_and_omnath_blocking_spells_are_canonical_and_executable() {
    let spells = [
        (
            "Cracked Earth Technique",
            "Earthbend 3, then earthbend 3. You gain 3 life. (To earthbend 3, target land you control becomes a 0/0 creature with haste that's still a land. Put three +1/+1 counters on it. When it dies or is exiled, return it to the battlefield tapped.)",
        ),
        (
            "Earth Rumble",
            "Earthbend 2. When you do, up to one target creature you control fights target creature an opponent controls. (To earthbend 2, target land you control becomes a 0/0 creature with haste that's still a land. Put two +1/+1 counters on it. When it dies or is exiled, return it to the battlefield tapped. Creatures that fight each deal damage equal to their power to the other.)",
        ),
        (
            "Earth Rumble Triumph",
            "Choose one â€”\nâ€¢ Draw cards equal to the greatest power among non-Human creatures you control.\nâ€¢ Non-Human creatures you control get +3/+3 until end of turn.",
        ),
        (
            "Earthshape",
            "Earthbend 3. Then each creature you control with power less than or equal to that land's power gains hexproof and indestructible until end of turn. You gain hexproof until end of turn.",
        ),
        (
            "Gamble",
            "Search your library for a card, put that card into your hand, discard a card at random, then shuffle.",
        ),
        (
            "Inspiring Call",
            "Draw a card for each creature you control with a +1/+1 counter on it. Those creatures gain indestructible until end of turn. (Damage and effects that say \"destroy\" don't destroy them.)",
        ),
        (
            "Insurrection",
            "Untap all creatures and gain control of them until end of turn. They gain haste until end of turn.",
        ),
        (
            "Origin of Metalbending",
            "Choose one â€”\nâ€¢ Destroy target artifact or enchantment.\nâ€¢ Put a +1/+1 counter on target creature you control. It gains indestructible until end of turn. (Damage and effects that say \"destroy\" don't destroy it.)",
        ),
        (
            "Rocky Rebuke",
            "Target creature you control deals damage equal to its power to target creature an opponent controls.",
        ),
        (
            "Animist's Awakening",
            "Reveal the top X cards of your library. Put all land cards from among them onto the battlefield tapped and the rest on the bottom of your library in a random order.",
        ),
        (
            "Animist's Awakening",
            "Spell mastery â€” If there are two or more instant and/or sorcery cards in your graveyard, untap those lands.",
        ),
        (
            "Archdruid's Charm",
            "Choose one â€”\nâ€¢ Search your library for a creature or land card and reveal it. Put it onto the battlefield tapped if it's a land card. Otherwise, put it into your hand. Then shuffle.\nâ€¢ Put a +1/+1 counter on target creature you control. It deals damage equal to its power to target creature you don't control.\nâ€¢ Exile target artifact or enchantment.",
        ),
        (
            "Blue Sun's Zenith",
            "Target player draws X cards. Shuffle Blue Sun's Zenith into its owner's library.",
        ),
        (
            "Boundless Realms",
            "Search your library for up to X basic land cards, where X is the number of lands you control, put them onto the battlefield tapped, then shuffle.",
        ),
        (
            "Debt to the Deathless",
            "Each opponent loses two times X life. You gain life equal to the life lost this way.",
        ),
        (
            "Drown in Dreams",
            "Choose one. If you control a commander as you cast this spell, you may choose both instead.\nâ€¢ Target player draws X cards.\nâ€¢ Target player mills twice X cards.",
        ),
        ("Duneblast", "Choose up to one creature. Destroy the rest."),
        (
            "Entish Restoration",
            "Sacrifice a land. Search your library for up to two basic land cards, put them onto the battlefield tapped, then shuffle. If you control a creature with power 4 or greater, instead search your library for up to three basic land cards, put them onto the battlefield tapped, then shuffle.",
        ),
        (
            "Invoke the Firemind",
            "Choose one â€”\nâ€¢ Draw X cards.\nâ€¢ Invoke the Firemind deals X damage to any target.",
        ),
        (
            "Iridian Maelstrom",
            "Destroy each creature that isn't all colors.",
        ),
        (
            "Klauth's Will",
            "Choose one. If you control a commander as you cast this spell, you may choose both instead.\nâ€¢ Breathe Flame â€” Klauth's Will deals X damage to each creature without flying.\nâ€¢ Smash Relics â€” Destroy up to X target artifacts and/or enchantments.",
        ),
        (
            "Lavalanche",
            "Lavalanche deals X damage to target player or planeswalker and each creature that player or that planeswalker's controller controls.",
        ),
        (
            "Mind Grind",
            "Each opponent reveals cards from the top of their library until they reveal X land cards, then puts all cards revealed this way into their graveyard. X can't be 0.",
        ),
        (
            "Time Wipe",
            "Return a creature you control to its owner's hand, then destroy all creatures.",
        ),
        (
            "To the Crystal Tower",
            "Choose two â€”\nâ€¢ Counter target spell.\nâ€¢ Return target permanent to its owner's hand.\nâ€¢ Tap all creatures your opponents control.\nâ€¢ Draw a card.",
        ),
        (
            "Villainous Wealth",
            "Target opponent exiles the top X cards of their library. You may cast any number of spells with mana value X or less from among them without paying their mana costs.",
        ),
    ];
    for (name, oracle_text) in spells {
        assert_single_ability_executable(name, "Sorcery", Some("{X}"), oracle_text);
    }
}

#[test]
fn toph_and_omnath_core_triggers_are_canonical_and_executable() {
    let triggers = [
        (
            "Amulet of Vigor",
            "Whenever a permanent you control enters tapped, untap it.",
        ),
        (
            "Badgermole Cub",
            "Whenever you tap a creature for mana, add an additional {G}.",
        ),
        (
            "Bitter Work",
            "Whenever you attack a player with one or more creatures with power 4 or greater, draw a card.",
        ),
        (
            "Bumi, Unleashed",
            "Whenever Bumi deals combat damage to a player, untap all lands you control. After this phase, there is an additional combat phase. Only land creatures can attack during that combat phase.",
        ),
        (
            "Lumra, Bellow of the Woods",
            "When Lumra enters, mill four cards. Then return all land cards from your graveyard to the battlefield tapped.",
        ),
        (
            "Portal to Phyrexia",
            "When this artifact enters, each opponent sacrifices three creatures of their choice.",
        ),
        (
            "Portal to Phyrexia",
            "At the beginning of your upkeep, put target creature card from a graveyard onto the battlefield under your control. It's a Phyrexian in addition to its other types.",
        ),
        (
            "Awakening",
            "At the beginning of each upkeep, untap all creatures and lands.",
        ),
        (
            "Dark Depths",
            "When Dark Depths has no ice counters on it, sacrifice it. If you do, create Marit Lage, a legendary 20/20 black Avatar creature token with flying and indestructible.",
        ),
        (
            "Burgeoning",
            "Whenever an opponent plays a land, you may put a land card from your hand onto the battlefield.",
        ),
        (
            "Mirari's Wake",
            "Whenever you tap a land for mana, add one mana of any type that land produced.",
        ),
        (
            "Springleaf Parade",
            "When this enchantment enters, create X 1/1 colorless Shapeshifter creature tokens with changeling. (They're every creature type.)",
        ),
        (
            "Omnath, Locus of All",
            "At the beginning of your first main phase, look at the top card of your library. You may reveal that card if it has three or more colored mana symbols in its mana cost. If you do, add three mana in any combination of its colors and put it into your hand. If you don't reveal it, put it into your hand.",
        ),
    ];
    for (name, oracle_text) in triggers {
        assert_single_ability_executable(name, "Creature", Some("{3}"), oracle_text);
    }
}

#[test]
fn avatar_deck_continuous_effects_are_canonical_and_executable() {
    let abilities = [
        (
            "Elvish Reclaimer",
            "This creature gets +2/+2 as long as there are three or more land cards in your graveyard.",
        ),
        (
            "Mayor Tong of Chin Village",
            "Your opponents can't cast spells from anywhere other than their hands.",
        ),
        (
            "Leyline of the Guildpact",
            "Each nonland permanent you control is all colors.",
        ),
        (
            "The Legend of Kyoshi",
            "Lands you control have trample and hexproof.",
        ),
        (
            "Sokka's Charge",
            "During your turn, Allies you control have double strike and lifelink.",
        ),
        (
            "Freya Crescent",
            "Jump â€” During your turn, Freya Crescent has flying.",
        ),
    ];
    for (name, oracle_text) in abilities {
        assert_single_ability_executable(name, "Enchantment", Some("{2}"), oracle_text);
    }
}

#[test]
fn second_avatar_deck_batch_is_canonical_and_executable() {
    let abilities = [
        (
            "Elemental Teachings",
            "Sorcery",
            "Search your library for up to four land cards with different names and reveal them. An opponent chooses two of those cards. Put the chosen cards into your graveyard and the rest onto the battlefield tapped, then shuffle.",
        ),
        (
            "Doppelgang",
            "Sorcery",
            "For each of X target permanents, create X tokens that are copies of that permanent.",
        ),
        (
            "Expansion",
            "Instant",
            "Copy target instant or sorcery spell with mana value 4 or less. You may choose new targets for the copy.",
        ),
        (
            "Explosion",
            "Instant",
            "Explosion deals X damage to any target. Target player draws X cards.",
        ),
        (
            "Obscuring Haze",
            "Instant",
            "Prevent all damage that would be dealt this turn by creatures your opponents control.",
        ),
        (
            "Bristly Bill, Spine Sower",
            "Creature",
            "Landfall â€” Whenever a land you control enters, put a +1/+1 counter on target creature.",
        ),
        (
            "Experimental Synthesizer",
            "Artifact",
            "When this artifact enters or leaves the battlefield, exile the top card of your library. Until end of turn, you may play that card.",
        ),
        (
            "Portcullis",
            "Artifact",
            "Whenever a creature enters, if there are two or more other creatures on the battlefield, exile that creature. Return that card to the battlefield under its owner's control when this artifact leaves the battlefield.",
        ),
        (
            "Silver Shroud Costume",
            "Artifact - Equipment",
            "When this Equipment enters, attach it to target creature you control. That creature gains shroud until end of turn. (It can't be the target of spells or abilities.)",
        ),
        (
            "Sphinx's Tutelage",
            "Enchantment",
            "Whenever you draw a card, target opponent mills two cards. If two nonland cards that share a color were milled this way, repeat this process.",
        ),
        (
            "Storm King's Thunder",
            "Sorcery",
            "When you next cast an instant or sorcery spell this turn, copy that spell X times. You may choose new targets for the copies.",
        ),
        (
            "Unbound Flourishing",
            "Enchantment",
            "Whenever you cast a permanent spell with a mana cost that contains {X}, double the value of X.",
        ),
        (
            "Unbound Flourishing",
            "Enchantment",
            "Whenever you cast an instant or sorcery spell or activate an ability, if that spell's mana cost or that ability's activation cost contains {X}, copy that spell or ability. You may choose new targets for the copy.",
        ),
        (
            "Toph, Hardheaded Teacher",
            "Creature",
            "Whenever you cast a spell, earthbend 1. If that spell is a Lesson, put an additional +1/+1 counter on that land. (Target land you control becomes a 0/0 creature with haste that's still a land. Put a +1/+1 counter on it. When it dies or is exiled, return it to the battlefield tapped.)",
        ),
        (
            "Experience",
            "Card",
            "(Place your experience counters here.)",
        ),
    ];
    for (name, type_line, oracle_text) in abilities {
        assert_single_ability_executable(name, type_line, Some("{X}"), oracle_text);
    }
}

#[test]
fn omnath_defensive_package_is_canonical_and_executable() {
    let abilities = [
        (
            "Assault Suit",
            "Artifact - Equipment",
            "Equipped creature gets +2/+2, has haste, can't attack you or planeswalkers you control, and can't be sacrificed.",
        ),
        (
            "Assault Suit",
            "Artifact - Equipment",
            "At the beginning of each opponent's upkeep, you may have that player gain control of equipped creature until end of turn. If you do, untap it.",
        ),
        (
            "Collective Restraint",
            "Enchantment",
            "Domain — Creatures can't attack you unless their controller pays {X} for each creature they control that's attacking you, where X is the number of basic land types among lands you control.",
        ),
        (
            "Fated Firepower",
            "Enchantment",
            "This enchantment enters with X fire counters on it.",
        ),
        (
            "Fated Firepower",
            "Enchantment",
            "If a source you control would deal damage to an opponent or a permanent an opponent controls, it deals that much damage plus an amount of damage equal to the number of fire counters on this enchantment instead.",
        ),
        (
            "Solitary Confinement",
            "Enchantment",
            "At the beginning of your upkeep, sacrifice this enchantment unless you discard a card.",
        ),
        (
            "Solitary Confinement",
            "Enchantment",
            "Skip your draw step.",
        ),
        (
            "Solitary Confinement",
            "Enchantment",
            "You have shroud. (You can't be the target of spells or abilities.)",
        ),
        (
            "Solitary Confinement",
            "Enchantment",
            "Prevent all damage that would be dealt to you.",
        ),
        (
            "Sphere of Safety",
            "Enchantment",
            "Creatures can't attack you or planeswalkers you control unless their controller pays {X} for each of those creatures, where X is the number of enchantments you control.",
        ),
        (
            "Tectonic Split",
            "Sorcery",
            "As an additional cost to cast this spell, sacrifice half the lands you control, rounded up.",
        ),
    ];
    for (name, type_line, oracle_text) in abilities {
        assert_single_ability_executable(name, type_line, Some("{X}"), oracle_text);
    }
}

#[test]
fn aang_package_is_canonical_and_executable() {
    let abilities = [
        (
            "Aang's Iceberg",
            "Enchantment",
            "When this enchantment enters, exile up to one other target nonland permanent until this enchantment leaves the battlefield.",
        ),
        (
            "Aang's Shelter",
            "Instant",
            "Until your next turn, your life total can't change and you gain protection from everything. All permanents you control phase out. (While they're phased out, they're treated as though they don't exist. They phase in before you untap during your untap step.)",
        ),
        (
            "Enter the Avatar State",
            "Instant",
            "Until end of turn, target creature you control becomes an Avatar in addition to its other types and gains flying, first strike, lifelink, and hexproof. (A creature with hexproof can't be the target of spells or abilities your opponents control.)",
        ),
        (
            "Aang, Master of Elements",
            "Creature",
            "At the beginning of each upkeep, you may transform Aang, Master of Elements. If you do, you gain 4 life, draw four cards, put four +1/+1 counters on him, and he deals 4 damage to each opponent.",
        ),
        (
            "Aang, Master of Elements",
            "Creature",
            "Spells you cast cost {W}{U}{B}{R}{G} less to cast. (This can reduce generic costs.)",
        ),
        (
            "Thriving Heath",
            "Land",
            "This land enters tapped. As it enters, choose a color other than white.",
        ),
        (
            "The Watcher in the Water",
            "Creature",
            "The Watcher in the Water enters tapped with nine stun counters on it. (If a permanent with a stun counter would become untapped, remove one from it instead.)",
        ),
        (
            "Sun Warriors",
            "Creature",
            "Firebending X, where X is the number of creatures you control. (Whenever this creature attacks, add X {R}. This mana lasts until end of combat.)",
        ),
    ];
    for (name, type_line, oracle_text) in abilities {
        assert_single_ability_executable(name, type_line, Some("{2}"), oracle_text);
    }
}

#[test]
fn alexios_package_is_canonical_and_executable() {
    let abilities = [
        (
            "Academy Manufactor",
            "Artifact Creature",
            "If you would create a Clue, Food, or Treasure token, instead create one of each.",
        ),
        (
            "Genji Glove",
            "Artifact — Equipment",
            "Whenever equipped creature attacks, if it's the first combat phase of the turn, untap it. After this phase, there is an additional combat phase.",
        ),
        (
            "Bothersome Quasit",
            "Creature",
            "Whenever you cast a noncreature spell, goad target creature an opponent controls. (Until your next turn, that creature attacks each combat if able and attacks a player other than you if able.)",
        ),
        (
            "Komainu Battle Armor",
            "Artifact Creature — Equipment Dog",
            "Reconfigure {4} ({4}: Attach to target creature you control; or unattach from a creature. Reconfigure only as a sorcery. While attached, this isn't a creature.)",
        ),
        (
            "Masterwork of Ingenuity",
            "Artifact — Equipment",
            "You may have this Equipment enter as a copy of any Equipment on the battlefield.",
        ),
        (
            "The Reaver Cleaver",
            "Legendary Artifact — Equipment",
            "Equipped creature gets +1/+1 and has trample and \"Whenever this creature deals combat damage to a player or battle, create that many Treasure tokens.\"",
        ),
        (
            "Chef's Kiss",
            "Instant",
            "Gain control of target spell that targets only a single permanent or player. Copy it, then reselect the targets at random for the spell and the copy. The new targets can't be you or a permanent you control.",
        ),
        (
            "Undercity",
            "Dungeon",
            "Secret Entrance — Search your library for a basic land card, reveal it, put it into your hand, then shuffle.",
        ),
    ];
    for (name, type_line, oracle_text) in abilities {
        assert_single_ability_executable(name, type_line, Some("{2}"), oracle_text);
    }
}

fn request_from_truth(truth: &Value) -> OracleCardParseRequest {
    let context = &truth["context"];
    let faces = context["faces"]
        .as_array()
        .map(|faces| {
            faces
                .iter()
                .map(|face| OracleCardFace {
                    id: face["id"].as_str().expect("face id").to_string(),
                    mana_cost: optional_string(&face["manaCost"]),
                    name: face["name"].as_str().expect("face name").to_string(),
                    oracle_text: face["oracleText"]
                        .as_str()
                        .expect("face Oracle text")
                        .to_string(),
                    power: optional_string(&face["power"]),
                    toughness: optional_string(&face["toughness"]),
                    type_line: face["typeLine"]
                        .as_str()
                        .expect("face type line")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    OracleCardParseRequest {
        card_name: context["name"]
            .as_str()
            .expect("card context name")
            .to_string(),
        faces,
        layout: optional_string(&context["layout"]),
        mana_cost: optional_string(&context["manaCost"]),
        oracle_text: optional_string(&context["oracleText"]),
        type_line: context["typeLine"]
            .as_str()
            .expect("card context type line")
            .to_string(),
    }
}

#[test]
fn firebending_lesson_parses_kicker_target_and_conditional_damage() {
    let truth = read_json(
        &workspace_root()
            .join("fixtures")
            .join("oracle-regressions")
            .join("firebending-lesson.json"),
    );
    let result = parse_oracle_card(request_from_truth(&truth));
    let expected = truth["abilities"].as_array().expect("truth abilities");

    assert_eq!(result.abilities.len(), expected.len());
    for (actual, expected) in result.abilities.iter().zip(expected) {
        assert_eq!(actual.status, "canonical");
        assert_eq!(actual.rule.as_ref(), Some(&expected["expectedRule"]));
    }
}

#[test]
fn day_of_judgment_parses_as_untargeted_global_destruction() {
    let truth = read_json(
        &workspace_root()
            .join("fixtures")
            .join("oracle-regressions")
            .join("day-of-judgment.json"),
    );
    let result = parse_oracle_card(request_from_truth(&truth));
    let expected = truth["abilities"].as_array().expect("truth abilities");

    assert_eq!(result.abilities.len(), 1);
    assert_eq!(result.abilities[0].status, "canonical");
    assert_eq!(
        result.abilities[0].rule.as_ref(),
        Some(&expected[0]["expectedRule"])
    );
}

#[test]
fn multiversal_passage_parses_its_land_type_and_life_choices() {
    let truth = read_json(
        &workspace_root()
            .join("fixtures")
            .join("oracle-regressions")
            .join("multiversal-passage.json"),
    );
    let result = parse_oracle_card(request_from_truth(&truth));
    let expected = truth["abilities"].as_array().expect("truth abilities");

    assert_eq!(result.abilities[0].status, "canonical");
    assert_eq!(
        result.abilities[0].rule.as_ref(),
        Some(&expected[0]["expectedRule"])
    );
}

#[test]
fn city_of_brass_parses_mana_and_self_tap_damage() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "City of Brass".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some(
            "{T}: Add one mana of any color.\nWhenever this land becomes tapped, it deals 1 damage to you."
                .to_string(),
        ),
        type_line: "Land".to_string(),
    });

    assert_eq!(result.abilities.len(), 2);
    assert!(
        result
            .abilities
            .iter()
            .all(|ability| ability.status == "canonical"),
        "City of Brass should parse canonically: {:?}",
        result.diagnostics
    );
    let trigger = result
        .abilities
        .iter()
        .filter_map(|ability| ability.rule.as_ref())
        .find(|rule| rule["kind"] == "triggeredAbility")
        .expect("City of Brass has a canonical tap trigger");
    assert_eq!(trigger["event"]["kind"], "permanentTapped");
    assert_eq!(trigger["effects"][0]["kind"], "dealDamage");
    assert_eq!(trigger["effects"][0]["amount"]["value"], 1);
    assert_eq!(trigger["effects"][0]["recipient"]["kind"], "controllerOf");
}

#[test]
fn reflecting_pool_parses_controlled_land_mana_types() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Reflecting Pool".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some(
            "{T}: Add one mana of any type that a land you control could produce.".to_string(),
        ),
        type_line: "Land".to_string(),
    });

    assert_eq!(result.abilities.len(), 1);
    assert_eq!(result.abilities[0].status, "canonical");
    let rule = result.abilities[0]
        .rule
        .as_ref()
        .expect("Reflecting Pool has a canonical mana rule");
    assert_eq!(rule["kind"], "manaAbility");
    assert_eq!(
        rule["effects"][0]["mana"]["kind"],
        "manaTypesLandsYouControlCouldProduce"
    );
    assert_eq!(rule["effects"][0]["mana"]["amount"], 1);
}

#[test]
fn ally_encampment_parses_ally_only_mana() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Ally Encampment".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some(
            "{T}: Add {C}.\n{T}: Add one mana of any color. Spend this mana only to cast an Ally spell.\n{1}, {T}, Sacrifice this land: Return target Ally you control to its owner's hand."
                .to_string(),
        ),
        type_line: "Land".to_string(),
    });

    let ally_mana = result
        .abilities
        .iter()
        .filter_map(|ability| ability.rule.as_ref())
        .find(|rule| rule["effects"][0]["mana"]["kind"] == "chooseColor")
        .expect("Ally Encampment has its colored mana ability");
    assert_eq!(ally_mana["kind"], "manaAbility");
    assert_eq!(
        ally_mana["effects"][0]["mana"]["spendRestriction"]["where"],
        serde_json::json!({ "kind": "subtypeContains", "value": "Ally" })
    );
}

#[test]
fn great_divide_guide_resolves_land_and_ally_qualifiers() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Great Divide Guide".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{1}{G}".to_string()),
        oracle_text: Some(
            "Each land and Ally you control has \"{T}: Add one mana of any color.\"".to_string(),
        ),
        type_line: "Creature — Human Scout Ally".to_string(),
    });

    let rule = result.abilities[0]
        .rule
        .as_ref()
        .expect("Great Divide Guide has a canonical static ability");
    assert_eq!(result.abilities[0].status, "canonical");
    assert_eq!(
        rule["modifiers"][0]["objects"]["where"],
        serde_json::json!({
            "kind": "or",
            "operands": [
                { "kind": "cardTypeContains", "value": "Land" },
                { "kind": "subtypeContains", "value": "Ally" },
            ],
        })
    );
    assert!(rule_is_executable(rule));
}

#[test]
fn granted_mana_qualifiers_resolve_plurals_and_card_name_prefixes() {
    let plural_result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Alliance Guide".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some(
            "Lands and Allies you control have \"{T}: Add one mana of any color.\"".to_string(),
        ),
        type_line: "Creature — Human Ally".to_string(),
    });
    assert_eq!(
        plural_result.abilities[0].rule.as_ref().unwrap()["modifiers"][0]["objects"]["where"],
        serde_json::json!({
            "kind": "or",
            "operands": [
                { "kind": "cardTypeContains", "value": "Land" },
                { "kind": "subtypeContains", "value": "Ally" },
            ],
        })
    );

    let name_result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Aang, Swift Savior".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some("Aang you control have \"{T}: Add one mana of any color.\"".to_string()),
        type_line: "Legendary Creature — Human Avatar Ally".to_string(),
    });
    let name_rule = name_result.abilities[0]
        .rule
        .as_ref()
        .expect("a leading card-name reference resolves canonically");
    assert_eq!(
        name_rule["modifiers"][0]["objects"]["where"],
        serde_json::json!({ "kind": "nameStartsWith", "value": "Aang" })
    );
    assert!(rule_is_executable(name_rule));
}

#[test]
fn avatar_aang_parses_bending_trigger_and_transform_condition() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Avatar Aang".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{R}{G}{W}{U}".to_string()),
        oracle_text: Some(
            "Flying, firebending 2\nWhenever you waterbend, earthbend, firebend, or airbend, draw a card. Then if you've done all four this turn, transform Avatar Aang."
                .to_string(),
        ),
        type_line: "Legendary Creature — Human Avatar Ally".to_string(),
    });

    assert!(
        result
            .abilities
            .iter()
            .all(|ability| ability.status == "canonical"),
        "Avatar Aang should parse canonically: {:?}",
        result.diagnostics
    );
    let trigger = result
        .abilities
        .iter()
        .filter_map(|ability| ability.rule.as_ref())
        .find(|rule| rule["event"]["kind"] == "bendingPerformed")
        .expect("Avatar Aang has a canonical bending trigger");
    assert_eq!(trigger["event"]["forms"].as_array().unwrap().len(), 4);
    assert_eq!(trigger["effects"][0]["kind"], "drawCards");
    assert_eq!(
        trigger["effects"][1]["operation"],
        "transformIfAllBendingForms"
    );
}

#[test]
fn exotic_orchard_parses_only_colors_opposing_lands_could_produce() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Exotic Orchard".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some(
            "{T}: Add one mana of any color that a land an opponent controls could produce."
                .to_string(),
        ),
        type_line: "Land".to_string(),
    });

    assert_eq!(result.abilities.len(), 1);
    assert_eq!(result.abilities[0].status, "canonical");
    let rule = result.abilities[0]
        .rule
        .as_ref()
        .expect("Exotic Orchard has a canonical mana rule");
    assert_eq!(rule["kind"], "manaAbility");
    assert_eq!(
        rule["effects"][0]["mana"]["kind"],
        "manaColorsLandsOpponentsControlCouldProduce"
    );
    assert_eq!(rule["effects"][0]["mana"]["amount"], 1);
}

#[test]
fn activate_only_if_uses_the_general_condition_parser() {
    let artifact_or_enchantment = parse_oracle_card(OracleCardParseRequest {
        card_name: "Conditional Mana Rock".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some(
            "{T}: Add one mana of any color. Activate only if you control an artifact or an enchantment."
                .to_string(),
        ),
        type_line: "Artifact".to_string(),
    });

    assert_eq!(artifact_or_enchantment.status, "canonical");
    let rule = artifact_or_enchantment.abilities[0]
        .rule
        .as_ref()
        .expect("conditional mana rule");
    assert_eq!(rule["activationCondition"]["kind"], "controlsPermanent");
    assert_eq!(rule["activationCondition"]["where"]["kind"], "or");
    assert_eq!(
        rule["activationCondition"]["where"]["operands"][0]["value"],
        "Artifact"
    );
    assert_eq!(
        rule["activationCondition"]["where"]["operands"][1]["value"],
        "Enchantment"
    );

    let ferocious = parse_oracle_card(OracleCardParseRequest {
        card_name: "Ferocious Mana Rock".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some(
            "{T}: Add {G}. Activate only if you control a creature with power 4 or greater."
                .to_string(),
        ),
        type_line: "Artifact".to_string(),
    });

    assert_eq!(ferocious.status, "canonical");
    let rule = ferocious.abilities[0]
        .rule
        .as_ref()
        .expect("ferocious mana rule");
    assert_eq!(rule["activationCondition"]["kind"], "controlsPermanent");
    assert_eq!(rule["activationCondition"]["where"]["kind"], "and");
    assert_eq!(
        rule["activationCondition"]["where"]["operands"][1]["operator"],
        ">="
    );
    assert_eq!(
        rule["activationCondition"]["where"]["operands"][1]["right"]["value"],
        4
    );
}

#[test]
fn enter_replacements_are_classified_by_event_shape_and_generic_conditions() {
    let type_choice = parse_oracle_card(OracleCardParseRequest {
        card_name: "Adaptive Automaton".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{3}".to_string()),
        oracle_text: Some("As this artifact enters, choose a creature type.".to_string()),
        type_line: "Artifact Creature - Construct".to_string(),
    });

    assert_eq!(type_choice.status, "canonical");
    assert_eq!(type_choice.abilities[0].ability_type, "replacementEffect");
    let rule = type_choice.abilities[0]
        .rule
        .as_ref()
        .expect("as-enters replacement rule");
    assert_eq!(rule["kind"], "replacementEffect");
    assert_eq!(rule["event"]["kind"], "wouldEnterBattlefield");
    assert_eq!(rule["decisions"][0]["kind"], "chooseCreatureType");
    assert_eq!(rule["replacement"][0]["kind"], "storeDecision");

    let reveal_land = parse_oracle_card(OracleCardParseRequest {
        card_name: "Reveal Land".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some(
            "This land enters tapped unless you control a Plains or an Island.".to_string(),
        ),
        type_line: "Land".to_string(),
    });

    assert_eq!(reveal_land.status, "canonical");
    assert_eq!(reveal_land.abilities[0].ability_type, "replacementEffect");
    let rule = reveal_land.abilities[0]
        .rule
        .as_ref()
        .expect("conditional enter tapped rule");
    assert_eq!(rule["condition"]["kind"], "not");
    assert_eq!(rule["condition"]["operand"]["kind"], "controlsPermanent");
    assert_eq!(rule["condition"]["operand"]["where"]["kind"], "or");
    assert_eq!(
        rule["condition"]["operand"]["where"]["operands"][0]["value"],
        "Plains"
    );
    assert_eq!(
        rule["condition"]["operand"]["where"]["operands"][1]["value"],
        "Island"
    );
}

#[test]
fn criteria_parser_reuses_types_subtypes_counts_and_targets() {
    let enchantment_firebending = parse_oracle_card(OracleCardParseRequest {
        card_name: "Criteria Firebender".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{2}{R}".to_string()),
        oracle_text: Some(
            "Firebending X, where X is the number of enchantments you control".to_string(),
        ),
        type_line: "Creature - Human Shaman".to_string(),
    });
    assert_eq!(enchantment_firebending.status, "canonical");
    let rule = enchantment_firebending.abilities[0]
        .rule
        .as_ref()
        .expect("firebending rule");
    assert_eq!(rule["ability"]["quantity"]["kind"], "countPermanents");
    assert_eq!(rule["ability"]["quantity"]["where"]["value"], "Enchantment");

    let zombie_firebending = parse_oracle_card(OracleCardParseRequest {
        card_name: "Subtype Firebender".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{2}{R}".to_string()),
        oracle_text: Some(
            "Firebending X, where X is the number of Zombies you control".to_string(),
        ),
        type_line: "Creature - Human Shaman".to_string(),
    });
    assert_eq!(zombie_firebending.status, "canonical");
    let rule = zombie_firebending.abilities[0]
        .rule
        .as_ref()
        .expect("subtype firebending rule");
    assert_eq!(rule["ability"]["quantity"]["where"]["value"], "Zombie");

    let criteria_mana = parse_oracle_card(OracleCardParseRequest {
        card_name: "Criteria Mana".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some(
            "{T}: Add X mana of any one color, where X is the number of tapped artifacts and/or creatures you control."
                .to_string(),
        ),
        type_line: "Artifact".to_string(),
    });
    assert_eq!(criteria_mana.status, "canonical");
    let rule = criteria_mana.abilities[0]
        .rule
        .as_ref()
        .expect("criteria mana rule");
    assert_eq!(
        rule["effects"][0]["mana"]["amount"]["kind"],
        "countPermanents"
    );
    assert_eq!(rule["effects"][0]["mana"]["amount"]["where"]["kind"], "and");
    assert_eq!(
        rule["effects"][0]["mana"]["amount"]["where"]["operands"][1]["kind"],
        "or"
    );

    let destroy_zombie = parse_oracle_card(OracleCardParseRequest {
        card_name: "Destroy Zombie".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{1}{B}".to_string()),
        oracle_text: Some("Destroy target Zombie.".to_string()),
        type_line: "Instant".to_string(),
    });
    assert_eq!(destroy_zombie.status, "canonical");
    let rule = destroy_zombie.abilities[0]
        .rule
        .as_ref()
        .expect("destroy zombie rule");
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"]["value"],
        "Zombie"
    );
}

#[test]
fn exhaust_reuses_generic_activation_costs_and_mill_effect_parser() {
    let exhausted_mill = parse_oracle_card(OracleCardParseRequest {
        card_name: "Exhaustive Mill Engine".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{3}".to_string()),
        oracle_text: Some(
            "Exhaust — {2}{U}{U}, {T}: Any number of target players each mill cards equal to the number of cards in their graveyard."
                .to_string(),
        ),
        type_line: "Artifact".to_string(),
    });

    assert_eq!(exhausted_mill.status, "canonical");
    assert_eq!(exhausted_mill.abilities[0].ability_type, "activatedAbility");
    let rule = exhausted_mill.abilities[0]
        .rule
        .as_ref()
        .expect("exhaust mill rule");
    assert_eq!(rule["costs"][0]["kind"], "payMana");
    assert_eq!(rule["costs"][0]["manaCost"], "{2}{U}{U}");
    assert_eq!(rule["costs"][1]["kind"], "tap");
    assert_eq!(rule["activationLimit"]["kind"], "oncePerGameObject");
    assert_eq!(rule["effects"][0]["kind"], "choosePlayers");
    assert_eq!(rule["effects"][1]["kind"], "millEachPlayer");
    assert_eq!(
        rule["effects"][1]["count"]["kind"],
        "thatPlayersGraveyardCount"
    );

    let fixed_mill = parse_oracle_card(OracleCardParseRequest {
        card_name: "Reusable Mill Engine".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(
            "{1}, {T}: Any number of target players each mill two cards.".to_string(),
        ),
        type_line: "Artifact".to_string(),
    });

    assert_eq!(fixed_mill.status, "canonical");
    let rule = fixed_mill.abilities[0]
        .rule
        .as_ref()
        .expect("fixed mill rule");
    assert!(rule.get("activationLimit").is_none());
    assert_eq!(rule["effects"][1]["count"]["value"], 2);
}

#[test]
fn activated_effect_parser_reuses_counter_destroy_target_and_criteria_primitives() {
    let counter_ability = parse_oracle_card(OracleCardParseRequest {
        card_name: "Counter Distributor".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{4}".to_string()),
        oracle_text: Some(
            "{3}{W}, {T}: Put a +1/+1 counter on each creature you control.".to_string(),
        ),
        type_line: "Artifact".to_string(),
    });

    assert_eq!(counter_ability.status, "canonical");
    assert_eq!(
        counter_ability.abilities[0].ability_type,
        "activatedAbility"
    );
    let rule = counter_ability.abilities[0]
        .rule
        .as_ref()
        .expect("counter activated rule");
    assert_eq!(rule["costs"][0]["manaCost"], "{3}{W}");
    assert_eq!(rule["costs"][1]["kind"], "tap");
    assert_eq!(rule["effects"][0]["kind"], "putCounters");
    assert_eq!(rule["effects"][0]["counter"], "+1/+1");
    assert_eq!(rule["effects"][0]["count"]["value"], 1);
    assert_eq!(rule["effects"][0]["permanent"]["kind"], "eachPermanent");
    assert_eq!(
        rule["effects"][0]["permanent"]["where"]["value"],
        "Creature"
    );

    let destroy_ability = parse_oracle_card(OracleCardParseRequest {
        card_name: "Tapped Creature Destroyer".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some("{T}: Destroy target tapped creature.".to_string()),
        type_line: "Artifact".to_string(),
    });

    assert_eq!(destroy_ability.status, "canonical");
    let rule = destroy_ability.abilities[0]
        .rule
        .as_ref()
        .expect("destroy activated rule");
    let decision = &rule["declaration"]["decisions"][0];
    assert_eq!(decision["kind"], "chooseTargets");
    assert_eq!(decision["candidates"]["where"]["kind"], "and");
    assert_eq!(
        decision["candidates"]["where"]["operands"][0]["kind"],
        "isTapped"
    );
    assert_eq!(
        decision["candidates"]["where"]["operands"][1]["value"],
        "Creature"
    );
    assert_eq!(rule["effects"][0]["kind"], "destroyPermanent");
    assert_eq!(rule["effects"][0]["permanent"]["id"], "targetPermanent");

    let sacrifice_destroy = parse_oracle_card(OracleCardParseRequest {
        card_name: "Generic Sacrifice Destroyer".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{2}{B}".to_string()),
        oracle_text: Some(
            "{B}, Sacrifice a creature: Destroy target nonblack creature.".to_string(),
        ),
        type_line: "Creature - Zombie Cleric".to_string(),
    });

    assert_eq!(sacrifice_destroy.status, "canonical");
    let rule = sacrifice_destroy.abilities[0]
        .rule
        .as_ref()
        .expect("sacrifice destroy rule");
    assert_eq!(rule["costs"][0]["manaCost"], "{B}");
    assert_eq!(rule["costs"][1]["kind"], "sacrificePermanent");
    assert_eq!(rule["declaration"]["decisions"][0]["id"], "sacrificeCost1");
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"]["value"],
        "Creature"
    );
    let target_filter = &rule["declaration"]["decisions"][1]["candidates"]["where"];
    assert_eq!(target_filter["kind"], "and");
    assert_eq!(target_filter["operands"][0]["kind"], "not");
    assert_eq!(
        target_filter["operands"][0]["operand"]["kind"],
        "colorContains"
    );
    assert_eq!(target_filter["operands"][0]["operand"]["value"], "black");
    assert_eq!(target_filter["operands"][1]["value"], "Creature");

    let black_destroy = parse_oracle_card(OracleCardParseRequest {
        card_name: "Generic Black Destroyer".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some("{T}: Destroy target black creature.".to_string()),
        type_line: "Artifact".to_string(),
    });

    assert_eq!(black_destroy.status, "canonical");
    let rule = black_destroy.abilities[0]
        .rule
        .as_ref()
        .expect("black destroy rule");
    let target_filter = &rule["declaration"]["decisions"][0]["candidates"]["where"];
    assert_eq!(target_filter["kind"], "and");
    assert_eq!(target_filter["operands"][0]["kind"], "colorContains");
    assert_eq!(target_filter["operands"][0]["value"], "black");
    assert_eq!(target_filter["operands"][1]["value"], "Creature");
}

#[test]
fn reminder_text_colons_do_not_reclassify_triggers_as_activated_abilities() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Pitiless Plunderer".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{3}{B}".to_string()),
        oracle_text: Some(
            "Whenever another creature you control dies, create a Treasure token. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")"
                .to_string(),
        ),
        type_line: "Creature - Human Pirate".to_string(),
    });

    assert_eq!(result.status, "canonical");
    assert_eq!(result.abilities[0].ability_type, "triggeredAbility");
    let rule = result.abilities[0]
        .rule
        .as_ref()
        .expect("Pitiless Plunderer trigger");
    assert_eq!(rule["event"]["kind"], "permanentDied");
    assert_eq!(rule["effects"][0]["token"]["kind"], "namedToken");
    assert_eq!(rule["effects"][0]["token"]["name"], "Treasure");
}

#[test]
fn negate_and_enter_surveillance_parse_as_executable_shapes() {
    let negate = parse_oracle_card(OracleCardParseRequest {
        card_name: "Negate".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{1}{U}".to_string()),
        oracle_text: Some("Counter target noncreature spell.".to_string()),
        type_line: "Instant".to_string(),
    });
    let negate_rule = negate.abilities[0].rule.as_ref().expect("Negate rule");
    assert_eq!(negate.status, "canonical");
    assert_eq!(
        negate_rule["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
        "not"
    );
    assert_eq!(
        negate_rule["declaration"]["decisions"][0]["candidates"]["where"]["operand"]["value"],
        "creature"
    );
    assert_eq!(negate_rule["effects"][0]["kind"], "counterSpell");

    let archive = parse_oracle_card(OracleCardParseRequest {
        card_name: "Meticulous Archive".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some(
            "When this land enters, surveil 1. (Look at the top card of your library. You may put it into your graveyard.)"
                .to_string(),
        ),
        type_line: "Land - Plains Island".to_string(),
    });
    let archive_rule = archive.abilities[0]
        .rule
        .as_ref()
        .expect("Meticulous Archive rule");
    assert_eq!(archive.status, "canonical");
    assert_eq!(archive_rule["event"]["kind"], "enterBattlefield");
    assert_eq!(archive_rule["effects"][0]["kind"], "surveil");
    assert_eq!(archive_rule["effects"][0]["count"]["value"], 1);
}

#[test]
fn evergreen_keyword_reminder_text_does_not_hide_menace() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Teacher's Pest".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{B}".to_string()),
        oracle_text: Some(
            "Menace (This creature can't be blocked except by two or more creatures.)".to_string(),
        ),
        type_line: "Creature - Pest".to_string(),
    });

    assert_eq!(result.status, "canonical");
    assert_eq!(result.abilities.len(), 1);
    assert_eq!(result.abilities[0].ability_type, "keywordAbility");
    assert_eq!(
        result.abilities[0].rule.as_ref().expect("menace rule")["ability"]["kind"],
        "menace"
    );
}

#[test]
fn no_more_lies_and_pest_control_compile_to_generic_engine_rules() {
    let counter = parse_oracle_card(OracleCardParseRequest {
        card_name: "No More Lies".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{W}{U}".to_string()),
        oracle_text: Some(
            "Counter target spell unless its controller pays {3}. If that spell is countered this way, exile it instead of putting it into its owner's graveyard."
                .to_string(),
        ),
        type_line: "Instant".to_string(),
    });
    assert_eq!(counter.status, "canonical");
    let counter_rule = counter.abilities[0]
        .rule
        .as_ref()
        .expect("No More Lies rule");
    assert_eq!(
        counter_rule["effects"][0]["kind"],
        "counterStackObjectUnlessPays"
    );
    assert_eq!(counter_rule["effects"][0]["manaCost"], "{3}");
    assert_eq!(counter_rule["effects"][0]["exileInstead"], true);

    let sweeper = parse_oracle_card(OracleCardParseRequest {
        card_name: "Pest Control".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{W}{B}".to_string()),
        oracle_text: Some("Destroy all nonland permanents with mana value 1 or less.".to_string()),
        type_line: "Sorcery".to_string(),
    });
    assert_eq!(sweeper.status, "canonical");
    let sweeper_rule = sweeper.abilities[0]
        .rule
        .as_ref()
        .expect("Pest Control rule");
    assert_eq!(sweeper_rule["effects"][0]["kind"], "destroyPermanent");
    assert_eq!(
        sweeper_rule["effects"][0]["permanent"]["where"]["kind"],
        "and"
    );
    assert_eq!(
        sweeper_rule["effects"][0]["permanent"]["where"]["operands"][1]["left"]["kind"],
        "manaValueOf"
    );
}

#[test]
fn cycling_compiles_as_a_paid_hand_ability() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Sheltered Thicket".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some("Cycling {2} ({2}, Discard this card: Draw a card.)".to_string()),
        type_line: "Land - Mountain Forest".to_string(),
    });

    assert_eq!(result.status, "canonical");
    let rule = result.abilities[0].rule.as_ref().expect("cycling rule");
    assert_eq!(rule["kind"], "keywordAbility");
    assert_eq!(rule["ability"]["kind"], "cycling");
    assert_eq!(rule["ability"]["cost"]["manaCost"], "{2}");
    assert_eq!(rule["ability"]["activationZone"], "hand");
    assert_eq!(rule["ability"]["costs"][0]["kind"], "discardCard");
    assert_eq!(rule["ability"]["effects"][0]["kind"], "drawCards");
}

#[test]
fn talon_gates_is_promoted_after_parsing_and_keeps_every_cost() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Talon Gates of Madara".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some("{1}, {T}: Add one mana of any color.".to_string()),
        type_line: "Land - Gate".to_string(),
    });

    assert_eq!(result.status, "canonical");
    assert_eq!(result.abilities.len(), 1);
    let ability = &result.abilities[0];
    assert_eq!(ability.ability_type, "manaAbility");
    assert_eq!(ability.iterations[0].result["kind"], "activatedAbility");
    assert_eq!(
        ability
            .iterations
            .last()
            .map(|iteration| &iteration.result["kind"]),
        Some(&Value::String("manaAbility".to_string()))
    );
    let rule = ability
        .rule
        .as_ref()
        .expect("Talon has a canonical mana ability");
    assert_eq!(rule["kind"], "manaAbility");
    assert_eq!(rule["costs"][0]["kind"], "payMana");
    assert_eq!(rule["costs"][0]["manaCost"], "{1}");
    assert_eq!(rule["costs"][1]["kind"], "tap");
    assert_eq!(rule["effects"][0]["mana"]["kind"], "chooseColor");
}

#[test]
fn adagia_station_compiles_with_a_creature_tap_cost_and_counter_threshold() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Adagia, Windswept Bastion".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some(
            "This land enters tapped.\n{T}: Add {W}.\nStation (Tap another creature you control: Put charge counters equal to its power on this Planet. Station only as a sorcery.)\n12+ | {3}{W}, {T}: Create a token that's a copy of target artifact or enchantment you control, except it's legendary. Activate only as a sorcery."
                .to_string(),
        ),
        type_line: "Land - Planet".to_string(),
    });

    assert_eq!(result.status, "canonical");
    let station = result
        .abilities
        .iter()
        .filter_map(|ability| ability.rule.as_ref())
        .find(|rule| rule["effects"][0]["operation"].as_str() == Some("stationCreaturePower"))
        .expect("station rule");
    assert_eq!(station["costs"][0]["kind"], "tap");
    assert_eq!(station["costs"][0]["object"]["id"], "stationCreature");
    assert_eq!(station["activationCondition"]["kind"], "sorceryTiming");
    assert_eq!(
        station["declaration"]["decisions"][0]["candidates"]["ignoreTargetingRestrictions"],
        true
    );

    let copy = result
        .abilities
        .iter()
        .filter_map(|ability| ability.rule.as_ref())
        .find(|rule| rule["effects"][0]["operation"].as_str() == Some("adagiaLegendaryCopy"))
        .expect("copy rule");
    assert_eq!(copy["activationCondition"]["left"]["kind"], "countCounters");
    assert_eq!(copy["activationCondition"]["left"]["counter"], "charge");
    assert_eq!(copy["activationCondition"]["right"]["value"], 12);
}

#[test]
fn sacrifice_cost_is_also_promoted_to_a_mana_ability() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Sacrifice mana source".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some("Sacrifice a creature: Add one mana of any color.".to_string()),
        type_line: "Artifact".to_string(),
    });

    assert_eq!(result.status, "canonical");
    let ability = &result.abilities[0];
    assert_eq!(ability.iterations[0].result["kind"], "activatedAbility");
    let rule = ability.rule.as_ref().expect("sacrifice mana rule");
    assert_eq!(rule["kind"], "manaAbility");
    assert_eq!(rule["costs"][0]["kind"], "sacrificePermanent");
    assert_eq!(rule["declaration"]["decisions"][0]["kind"], "chooseTargets");
    assert_eq!(rule["effects"][0]["kind"], "addMana");
}

#[test]
fn smothering_abomination_parses_every_ability_canonically() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Smothering Abomination".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{2}{B}{G}".to_string()),
        oracle_text: Some(
            "Devoid (This card has no color.)\nFlying\nAt the beginning of your upkeep, sacrifice a creature.\nWhenever you sacrifice a creature, draw a card."
                .to_string(),
        ),
        type_line: "Creature - Eldrazi".to_string(),
    });

    assert_eq!(result.status, "canonical");
    assert_eq!(result.abilities.len(), 4);
    assert!(
        result
            .abilities
            .iter()
            .all(|ability| ability.status == "canonical")
    );
    assert_eq!(
        result.abilities[0]
            .rule
            .as_ref()
            .map(|rule| &rule["ability"]["kind"]),
        Some(&Value::String("devoid".to_string()))
    );
    assert!(
        result
            .abilities
            .iter()
            .filter_map(|ability| ability.rule.as_ref())
            .any(|rule| {
                rule["event"]["kind"] == "permanentDied"
                    && rule["event"]["reason"] == "sacrificed"
                    && rule["effects"][0]["kind"] == "drawCards"
            })
    );
}

#[test]
fn endrek_sahr_parses_cast_and_thrull_threshold_triggers() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Endrek Sahr, Master Breeder".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{4}{B}".to_string()),
        oracle_text: Some(
            "Whenever you cast a creature spell, create X 1/1 black Thrull creature tokens, where X is that spell's mana value.\nWhen you control seven or more Thrulls, sacrifice Endrek Sahr."
                .to_string(),
        ),
        type_line: "Legendary Creature - Human Wizard".to_string(),
    });

    assert_eq!(result.status, "canonical");
    assert_eq!(result.abilities.len(), 2);
    assert!(
        result
            .abilities
            .iter()
            .all(|ability| ability.status == "canonical")
    );
    let cast_trigger = result.abilities[0]
        .rule
        .as_ref()
        .expect("Endrek cast trigger is canonical");
    assert_eq!(cast_trigger["event"]["kind"], "spellCast");
    assert_eq!(
        cast_trigger["event"]["where"]["value"],
        Value::String("Creature".to_string())
    );
    assert_eq!(
        cast_trigger["effects"][0]["quantity"]["kind"],
        "triggeringSpellManaValue"
    );
    assert_eq!(cast_trigger["effects"][0]["token"]["subtypes"][0], "Thrull");

    let threshold_trigger = result.abilities[1]
        .rule
        .as_ref()
        .expect("Endrek threshold trigger is canonical");
    assert_eq!(threshold_trigger["event"]["kind"], "stateConditionMet");
    assert_eq!(
        threshold_trigger["event"]["condition"]["left"]["where"]["value"],
        "Thrull"
    );
    assert_eq!(threshold_trigger["event"]["condition"]["right"]["value"], 7);
    assert_eq!(
        threshold_trigger["effects"][0]["kind"],
        "sacrificePermanent"
    );
}

#[test]
fn sekkuar_core_death_token_and_life_land_patterns_are_canonical() {
    let cases = [
        (
            "Sek'Kuar, Deathkeeper",
            "Legendary Creature - Orc Shaman",
            "Whenever another nontoken creature you control dies, create a 3/1 black and red Graveborn creature token with haste.",
            "triggeredAbility",
        ),
        (
            "Pitiless Plunderer",
            "Creature - Human Pirate",
            "Whenever another creature you control dies, create a Treasure token. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")",
            "triggeredAbility",
        ),
        (
            "Infestation Sage",
            "Creature - Human Wizard",
            "When this creature dies, create a 1/1 black and green Insect creature token with flying.",
            "triggeredAbility",
        ),
        (
            "Sprouting Thrinax",
            "Creature - Lizard",
            "When this creature dies, create three 1/1 green Saproling creature tokens.",
            "triggeredAbility",
        ),
        (
            "Young Pyromancer",
            "Creature - Human Shaman",
            "Whenever you cast an instant or sorcery spell, create a 1/1 red Elemental creature token.",
            "triggeredAbility",
        ),
        (
            "Akoum Refuge",
            "Land",
            "This land enters tapped.\nWhen this land enters, you gain 1 life.",
            "replacementEffect",
        ),
    ];

    for (name, type_line, oracle_text, first_rule_kind) in cases {
        let result = parse_oracle_card(OracleCardParseRequest {
            card_name: name.to_string(),
            faces: Vec::new(),
            layout: None,
            mana_cost: None,
            oracle_text: Some(oracle_text.to_string()),
            type_line: type_line.to_string(),
        });

        assert!(
            result
                .abilities
                .iter()
                .all(|ability| ability.status == "canonical"),
            "{name} should parse canonically: {:?}",
            result.diagnostics
        );
        assert_eq!(
            result.abilities[0]
                .rule
                .as_ref()
                .and_then(|rule| rule["kind"].as_str()),
            Some(first_rule_kind),
            "{name} first rule"
        );
    }
}

#[test]
fn common_counter_and_damage_triggers_are_canonical_and_executable() {
    let cases = [
        (
            "Beastmaster Ascension",
            "Enchantment",
            "Whenever a creature you control attacks, you may put a quest counter on this enchantment.",
        ),
        (
            "Innkeeper's Talent",
            "Enchantment - Class",
            "At the beginning of combat on your turn, put a +1/+1 counter on target creature you control.",
        ),
        (
            "Earth King's Lieutenant",
            "Creature - Human Ally",
            "When this creature enters, put a +1/+1 counter on each other Ally creature you control.",
        ),
        (
            "Momo, Rambunctious Rascal",
            "Creature - Animal Ally",
            "When Momo enters, he deals 4 damage to target tapped creature an opponent controls.",
        ),
        (
            "Annie Joins Up",
            "Legendary Enchantment",
            "When Annie Joins Up enters, it deals 5 damage to target creature or planeswalker an opponent controls.",
        ),
        (
            "Dragonback Assault",
            "Enchantment",
            "When this enchantment enters, it deals 3 damage to each creature and each planeswalker.",
        ),
    ];

    for (name, type_line, oracle_text) in cases {
        let result = parse_oracle_card(OracleCardParseRequest {
            card_name: name.to_string(),
            faces: Vec::new(),
            layout: None,
            mana_cost: None,
            oracle_text: Some(oracle_text.to_string()),
            type_line: type_line.to_string(),
        });

        assert_eq!(
            result.status, "canonical",
            "{name}: {:?}",
            result.diagnostics
        );
        assert!(
            result
                .abilities
                .iter()
                .filter_map(|ability| ability.rule.as_ref())
                .all(rule_is_executable),
            "{name} should only emit executable rules",
        );
    }
}

#[test]
fn mobilize_sacrifice_damage_and_static_patterns_are_executable() {
    let cases = [
        (
            "Agate Instigator token",
            "Token Creature - Lizard Rogue",
            "(This token's mana cost is {1}{R}.)",
        ),
        (
            "Sword of Hearth and Home",
            "Artifact - Equipment",
            "Equipped creature gets +2/+2 and has protection from green and from white.",
        ),
        (
            "Carrion Feeder",
            "Creature - Zombie",
            "This creature can't block.",
        ),
        (
            "Elturel Survivors",
            "Creature - Tiefling Peasant",
            "Trample, myriad (Whenever this creature attacks, for each opponent other than defending player, you may create a token copy that's tapped and attacking that player or a planeswalker they control. Exile the tokens at end of combat.)",
        ),
        (
            "Essence Channeler",
            "Creature - Bat Cleric",
            "As long as you've lost life this turn, this creature has flying and vigilance.\nWhenever you gain life, put a +1/+1 counter on this creature.\nWhen this creature dies, put its counters on target creature you control.",
        ),
        (
            "Elturel Survivors bonus",
            "Creature - Tiefling Peasant",
            "As long as this creature is attacking, it gets +X/+0, where X is the number of lands defending player controls.",
        ),
        (
            "Isshin, Two Heavens as One",
            "Legendary Creature - Human Samurai",
            "If a creature attacking causes a triggered ability of a permanent you control to trigger, that ability triggers an additional time.",
        ),
        (
            "Jaxis, the Troublemaker",
            "Legendary Creature - Human Warrior",
            "{R}, {T}, Discard a card: Create a token that's a copy of another target creature you control. It gains haste and \"When this token dies, draw a card.\" Sacrifice it at the beginning of the next end step. Activate only as a sorcery.\nBlitz {1}{R} (If you cast this spell for its blitz cost, it gains haste and \"When this creature dies, draw a card.\" Sacrifice it at the beginning of the next end step.)",
        ),
        (
            "Juri, Master of the Revue",
            "Legendary Creature - Human Shaman",
            "Whenever you sacrifice a permanent, put a +1/+1 counter on Juri.\nWhen Juri dies, it deals damage equal to its power to any target.",
        ),
        (
            "Sandbender Scavengers",
            "Creature - Human Citizen",
            "Whenever you sacrifice another permanent, put a +1/+1 counter on this creature.\nWhen this creature dies, you may exile it. When you do, return target creature card with mana value less than or equal to this creature's power from your graveyard to the battlefield.",
        ),
        (
            "Stadium Headliner",
            "Creature - Devil",
            "{1}{R}, Sacrifice this creature: It deals damage equal to the number of creatures you control to target creature.",
        ),
        (
            "Semester's End",
            "Instant",
            "Exile any number of target creatures and/or planeswalkers you control. At the beginning of the next end step, return each of them to the battlefield under its owner's control. Each of them enters with an additional +1/+1 counter on it if it's a creature and an additional loyalty counter on it if it's a planeswalker.",
        ),
        (
            "Throne of the God-Pharaoh",
            "Legendary Artifact",
            "At the beginning of your end step, each opponent loses life equal to the number of tapped creatures you control.",
        ),
    ];

    for (name, type_line, oracle_text) in cases {
        let result = parse_oracle_card(OracleCardParseRequest {
            card_name: name.to_string(),
            faces: Vec::new(),
            layout: None,
            mana_cost: None,
            oracle_text: Some(oracle_text.to_string()),
            type_line: type_line.to_string(),
        });

        assert_eq!(
            result.status, "canonical",
            "{name}: {:?}",
            result.diagnostics
        );
        assert!(
            result
                .abilities
                .iter()
                .filter_map(|ability| ability.rule.as_ref())
                .all(rule_is_executable),
            "{name} should only emit executable rules",
        );
    }
}

#[test]
fn mobilize_keyword_and_evergreen_groups_are_canonical() {
    let cases = [
        (
            "Zurgo Stormrender",
            "Legendary Creature - Orc Warrior",
            "Mobilize 1 (Whenever this creature attacks, create a tapped and attacking 1/1 red Warrior creature token. Sacrifice it at the beginning of the next end step.)",
            "mobilize",
        ),
        (
            "Dalkovan Packbeasts",
            "Creature - Beast",
            "Vigilance\nMobilize 3 (Whenever this creature attacks, create three tapped and attacking 1/1 red Warrior creature tokens. Sacrifice them at the beginning of the next end step.)",
            "vigilance",
        ),
        (
            "Ocelot Pride",
            "Creature - Cat",
            "First strike, lifelink",
            "firstStrike",
        ),
        (
            "Avatar Aang",
            "Legendary Creature - Human Monk",
            "Flying, firebending 2",
            "firebending",
        ),
        (
            "Emeritus of Ideation",
            "Creature - Avatar Wizard",
            "Flying, ward {2}",
            "ward",
        ),
        ("Ward Mana Test", "Creature - Wizard", "Ward {2}", "ward"),
        (
            "Ward Life Test",
            "Creature - Demon",
            "Ward—Pay 2 life.",
            "ward",
        ),
        (
            "Duke Ulder Ravengard",
            "Legendary Creature - Human Noble Soldier",
            "Trample, myriad (Whenever this creature attacks, for each opponent other than defending player, you may create a token copy that's tapped and attacking that player or a planeswalker they control. Exile the tokens at end of combat.)",
            "myriad",
        ),
        (
            "Jaxis, the Troublemaker",
            "Legendary Creature - Human Warrior",
            "Blitz {1}{R} (If you cast this spell for its blitz cost, it gains haste and \"When this creature dies, draw a card.\" Sacrifice it at the beginning of the next end step.)",
            "blitz",
        ),
        (
            "Aerial Blitz Test",
            "Creature - Dragon",
            "Flying, blitz {1}{R} (If you cast this spell for its blitz cost, it gains haste and \"When this creature dies, draw a card.\" Sacrifice it at the beginning of the next end step.)",
            "blitz",
        ),
        ("Monk", "Token Creature - Monk", "Prowess", "prowess"),
    ];

    for (name, type_line, oracle_text, expected_keyword) in cases {
        let result = parse_oracle_card(OracleCardParseRequest {
            card_name: name.to_string(),
            faces: Vec::new(),
            layout: None,
            mana_cost: None,
            oracle_text: Some(oracle_text.to_string()),
            type_line: type_line.to_string(),
        });
        assert!(
            result
                .abilities
                .iter()
                .all(|ability| ability.status == "canonical"),
            "{name} should parse canonically"
        );
        assert!(
            result.abilities.iter().any(|ability| matches!(
                ability.ability_type.as_str(),
                "keywordAbility" | "keywordAbilityGroup"
            )),
            "{name} should classify as keyword ability or group",
        );
        assert!(
            result
                .abilities
                .iter()
                .filter_map(|ability| ability.rule.as_ref())
                .any(|rule| rule.to_string().contains(expected_keyword)),
            "{name} should contain {expected_keyword}"
        );
    }
}

#[test]
fn offspring_parses_its_cost_and_complete_enter_trigger() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Agate Instigator".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{1}{R}".to_string()),
        oracle_text: Some(
            "Offspring {1}{R} (You may pay an additional {1}{R} as you cast this spell. If you do, when this creature enters, create a 1/1 token copy of it.)"
                .to_string(),
        ),
        type_line: "Creature - Lizard Rogue".to_string(),
    });

    assert_eq!(result.status, "canonical");
    assert_eq!(result.abilities.len(), 1);
    assert_eq!(result.abilities[0].ability_type, "keywordAbility");
    let rule = result.abilities[0]
        .rule
        .as_ref()
        .expect("offspring is canonical");
    assert_eq!(rule["ability"]["kind"], "offspring");
    assert_eq!(rule["ability"]["cost"]["kind"], "payMana");
    assert_eq!(rule["ability"]["cost"]["manaCost"], "{1}{R}");
    assert_eq!(
        rule["ability"]["trigger"]["event"]["kind"],
        "enterBattlefield"
    );
    assert_eq!(
        rule["ability"]["trigger"]["condition"]["kind"],
        "offspringCostWasPaid"
    );
    assert_eq!(
        rule["ability"]["trigger"]["effects"][0]["kind"],
        "createTokenCopy"
    );
    assert_eq!(
        rule["ability"]["trigger"]["effects"][0]["basePower"]["value"],
        1
    );
    assert_eq!(
        rule["ability"]["trigger"]["effects"][0]["baseToughness"]["value"],
        1
    );
}

#[test]
fn replicate_parses_as_a_repeatable_additional_cost() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Train of Thought".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{1}{U}".to_string()),
        oracle_text: Some(
            "Replicate {1}{U} (When you cast this spell, copy it for each time you paid its replicate cost.)\nDraw a card."
                .to_string(),
        ),
        type_line: "Sorcery".to_string(),
    });

    assert_eq!(result.status, "canonical");
    let rule = result.abilities[0]
        .rule
        .as_ref()
        .expect("replicate is canonical");
    assert_eq!(rule["ability"]["kind"], "replicate");
    assert_eq!(rule["ability"]["cost"]["manaCost"], "{1}{U}");
    assert_eq!(rule["ability"]["repeatable"], true);
    assert!(rule_is_executable(rule));
}

#[test]
fn kellan_deck_keywords_and_commander_trigger_are_canonical_and_executable() {
    let cases = [
        (
            "Aeve, Progenitor Ooze",
            "Legendary Creature — Ooze",
            Some("{2}{G}{G}{G}"),
            "Storm (When you cast this spell, copy it for each spell cast before it this turn. Copies of permanents enter as tokens.)",
        ),
        (
            "Aloe Alchemist",
            "Creature — Plant Warlock",
            Some("{1}{G}"),
            "Plot {1}{G} (You may pay {1}{G} and exile this card from your hand. Cast it as a sorcery on a later turn without paying its mana cost. Plot only as a sorcery.)",
        ),
        (
            "Rift Sower",
            "Creature — Elf Druid",
            Some("{2}{G}"),
            "Suspend 2—{G} (Rather than cast this card from your hand, you may pay {G} and exile it with two time counters on it. At the beginning of your upkeep, remove a time counter. When the last is removed, you may cast it without paying its mana cost. It has haste.)",
        ),
        (
            "Surge of Brilliance",
            "Instant",
            Some("{3}{U}{U}"),
            "Foretell {1}{U} (During your turn, you may pay {2} and exile this card from your hand face down. Cast it on a later turn for its foretell cost.)",
        ),
        (
            "Imoti, Celebrant of Bounty",
            "Legendary Creature — Naga Druid",
            Some("{3}{G}{U}"),
            "Cascade (When you cast this spell, exile cards from the top of your library until you exile a nonland card that costs less. You may cast it without paying its mana cost. Put the exiled cards on the bottom in a random order.)",
        ),
        (
            "Kellan, the Kid",
            "Legendary Creature — Human Faerie",
            Some("{G}{W}{U}"),
            "Whenever you cast a spell from anywhere other than your hand, you may cast a permanent spell with equal or lesser mana value from your hand without paying its mana cost. If you don't, you may put a land card from your hand onto the battlefield.",
        ),
    ];

    for (name, type_line, mana_cost, oracle_text) in cases {
        assert_single_ability_executable(name, type_line, mana_cost, oracle_text);
    }
}

#[test]
fn kellan_deck_common_cast_and_enter_patterns_are_executable() {
    let cases = [
        (
            "Imoti, Celebrant of Bounty",
            "Legendary Creature — Naga Druid",
            Some("{3}{G}{U}"),
            "Spells you cast with mana value 6 or greater have cascade.",
        ),
        (
            "Outcaster Trailblazer",
            "Creature — Human Druid",
            Some("{2}{G}"),
            "When this creature enters, add one mana of any color.",
        ),
        (
            "Riftwing Cloudskate",
            "Creature — Illusion",
            Some("{3}{U}{U}"),
            "When this creature enters, return target permanent to its owner's hand.",
        ),
        (
            "Chulane, Teller of Tales",
            "Legendary Creature — Human Druid",
            Some("{2}{G}{W}{U}"),
            "{3}, {T}: Return target creature you control to its owner's hand.",
        ),
        (
            "Mind's Desire",
            "Sorcery",
            Some("{4}{U}{U}"),
            "Shuffle your library. Then exile the top card of your library. Until end of turn, you may play that card without paying its mana cost.",
        ),
        (
            "Magus of the Mind",
            "Creature — Human Wizard",
            Some("{4}{U}{U}"),
            "{U}, {T}, Sacrifice this creature: Shuffle your library, then exile the top X cards, where X is one plus the number of spells cast this turn. Until end of turn, you may play lands and cast spells from among cards exiled this way without paying their mana costs.",
        ),
        (
            "Surge of Brilliance",
            "Instant",
            Some("{3}{U}{U}"),
            "Paradox — Draw a card for each spell you've cast this turn from anywhere other than your hand.",
        ),
    ];

    for (name, type_line, mana_cost, oracle_text) in cases {
        assert_single_ability_executable(name, type_line, mana_cost, oracle_text);
    }
}

#[test]
fn threefold_signal_grants_replicate_to_exactly_three_color_spells() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Threefold Signal".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{3}".to_string()),
        oracle_text: Some(
            "When this artifact enters, scry 3.\nEach spell you cast that's exactly three colors has replicate {3}. (When you cast it, copy it for each time you paid its replicate cost. You may choose new targets for the copies. A copy of a permanent spell becomes a token.)"
                .to_string(),
        ),
        type_line: "Artifact".to_string(),
    });

    assert_eq!(result.status, "canonical");
    assert_eq!(result.abilities.len(), 2);
    let grant = result.abilities[1]
        .rule
        .as_ref()
        .expect("granted replicate is canonical");
    assert_eq!(grant["modifiers"][0]["kind"], "grantReplicate");
    assert_eq!(grant["modifiers"][0]["cost"]["manaCost"], "{3}");
    assert_eq!(
        grant["modifiers"][0]["spells"]["where"]["left"]["kind"],
        "colorCountOf"
    );
    assert_eq!(
        grant["modifiers"][0]["spells"]["where"]["right"]["value"],
        3
    );
    assert!(
        result
            .abilities
            .iter()
            .filter_map(|ability| ability.rule.as_ref())
            .all(rule_is_executable)
    );
}

#[test]
fn map_parses_its_paid_sorcery_speed_explore_ability() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Map".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some(
            "{1}, {T}, Sacrifice this artifact: Target creature you control explores. Activate only as a sorcery. (Reveal the top card of your library. Put that card into your hand if it's a land. Otherwise, put a +1/+1 counter on that creature, then put the card back or put it into your graveyard.)"
                .to_string(),
        ),
        type_line: "Token Artifact - Map".to_string(),
    });

    assert_eq!(result.status, "canonical");
    let rule = result.abilities[0].rule.as_ref().expect("Map is canonical");
    assert_eq!(rule["kind"], "activatedAbility");
    assert_eq!(rule["activationCondition"]["kind"], "sorceryTiming");
    assert_eq!(rule["declaration"]["decisions"][0]["id"], "targetCreature");
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["controller"]["kind"],
        "controllerOf"
    );
    assert_eq!(rule["effects"][0]["kind"], "explore");
    assert!(rule_is_executable(rule));
}

#[test]
fn grim_backwoods_preserves_its_sacrifice_cost_declaration() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Grim Backwoods".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some(
            "{T}: Add {C}.\n{2}{B}{G}, {T}, Sacrifice a creature: Draw a card.".to_string(),
        ),
        type_line: "Land".to_string(),
    });

    assert_eq!(result.status, "canonical");
    let rule = result.abilities[1]
        .rule
        .as_ref()
        .expect("Grim Backwoods is canonical");
    let sacrifice = &rule["declaration"]["decisions"][0];
    assert_eq!(sacrifice["id"], "sacrificeCost1");
    assert_eq!(sacrifice["minimum"], 1);
    assert_eq!(sacrifice["maximum"], 1);
    assert_eq!(sacrifice["candidates"]["where"]["value"], "Creature");
    assert!(rule_is_executable(rule));
}

#[test]
fn sephiroth_emblem_parses_its_unrestricted_creature_death_trigger() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Sephiroth, One-Winged Angel Emblem".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: None,
        oracle_text: Some(
            "Whenever a creature dies, target opponent loses 1 life and you gain 1 life."
                .to_string(),
        ),
        type_line: "Emblem".to_string(),
    });

    assert_eq!(result.status, "canonical");
    let rule = result.abilities[0]
        .rule
        .as_ref()
        .expect("Sephiroth emblem is canonical");
    assert_eq!(rule["kind"], "triggeredAbility");
    assert_eq!(rule["event"]["kind"], "permanentDied");
    assert!(rule["event"].get("player").is_none());
    assert_eq!(rule["effects"][0]["operation"], "targetOpponentDrainOne");
    assert!(rule_is_executable(rule));
}

#[test]
fn common_equipment_triggers_parse_against_the_attached_creature() {
    let cases = [
        (
            "Moonsilver Spear",
            "Whenever equipped creature attacks, create a 4/4 white Angel creature token with flying.",
            "attachedPermanentDeclaredAttacker",
        ),
        (
            "Sword of the Animist",
            "Whenever equipped creature attacks, you may search your library for a basic land card, put it onto the battlefield tapped, then shuffle.",
            "attachedPermanentDeclaredAttacker",
        ),
        (
            "Rogue's Gloves",
            "Whenever equipped creature deals combat damage to a player, you may draw a card.",
            "attachedPermanentCombatDamageToPlayer",
        ),
        (
            "Mask of Memory",
            "Whenever equipped creature deals combat damage to a player, you may draw two cards. If you do, discard a card.",
            "attachedPermanentCombatDamageToPlayer",
        ),
    ];

    for (name, oracle_text, event_kind) in cases {
        let result = parse_oracle_card(OracleCardParseRequest {
            card_name: name.to_string(),
            faces: Vec::new(),
            layout: None,
            mana_cost: Some("{2}".to_string()),
            oracle_text: Some(oracle_text.to_string()),
            type_line: "Artifact - Equipment".to_string(),
        });

        assert_eq!(result.status, "canonical", "{name} should be canonical");
        let rule = result.abilities[0]
            .rule
            .as_ref()
            .expect("equipment trigger is canonical");
        assert_eq!(rule["event"]["kind"], event_kind, "{name} event");
        assert!(rule_is_executable(rule), "{name} should be executable");
    }
}

#[test]
fn reusable_cast_and_sacrifice_triggers_parse_canonically() {
    let cases = [
        (
            "Tome of the Guildpact",
            "Whenever you cast a multicolored spell, draw a card.",
            "spellCast",
        ),
        (
            "Blood Aspirant",
            "Whenever you sacrifice a permanent, put a +1/+1 counter on this creature.",
            "permanentDied",
        ),
    ];

    for (name, oracle_text, event_kind) in cases {
        let result = parse_oracle_card(OracleCardParseRequest {
            card_name: name.to_string(),
            faces: Vec::new(),
            layout: None,
            mana_cost: Some("{2}".to_string()),
            oracle_text: Some(oracle_text.to_string()),
            type_line: "Permanent".to_string(),
        });

        assert_eq!(result.status, "canonical", "{name} should be canonical");
        let rule = result.abilities[0]
            .rule
            .as_ref()
            .expect("canonical trigger");
        assert_eq!(rule["event"]["kind"], event_kind, "{name} event");
        assert!(rule_is_executable(rule), "{name} should be executable");
    }
}

#[test]
fn magnetic_theft_parses_as_attachment_without_a_control_change() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Magnetic Theft".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{R}".to_string()),
        oracle_text: Some(
            "Attach target Equipment to target creature. (Control of the Equipment doesn't change.)"
                .to_string(),
        ),
        type_line: "Instant".to_string(),
    });

    assert_eq!(result.status, "canonical");
    let rule = result.abilities[0]
        .rule
        .as_ref()
        .expect("Magnetic Theft is canonical");
    assert_eq!(rule["effects"][0]["kind"], "attachPermanent");
    assert!(
        rule["declaration"]["decisions"][0]["candidates"]
            .get("controller")
            .is_none()
    );
    assert!(rule_is_executable(rule));
}

#[test]
fn graveyard_land_play_and_common_dynamic_effects_parse_canonically() {
    let cases = [
        (
            "Icetill Explorer",
            "You may play lands from your graveyard.",
            "Creature - Human Scout",
            "staticAbility",
            "playLandsFromGraveyard",
        ),
        (
            "Ichor Wellspring",
            "When this artifact enters or is put into a graveyard from the battlefield, draw a card.",
            "Artifact",
            "triggeredAbility",
            "permanentLeftBattlefield",
        ),
        (
            "Crime Novelist",
            "Whenever you sacrifice an artifact, put a +1/+1 counter on this creature and add {R}.",
            "Creature - Human Bard",
            "triggeredAbility",
            "addMana",
        ),
        (
            "Bulk Up",
            "Double target creature's power until end of turn.",
            "Instant",
            "spellAbility",
            "powerOf",
        ),
        (
            "Pillar Launch",
            "Target creature gets +2/+2 and gains reach until end of turn. Untap it.",
            "Instant",
            "spellAbility",
            "untapPermanent",
        ),
    ];

    for (name, oracle_text, type_line, rule_kind, expected_function) in cases {
        let result = parse_oracle_card(OracleCardParseRequest {
            card_name: name.to_string(),
            faces: Vec::new(),
            layout: None,
            mana_cost: Some("{1}".to_string()),
            oracle_text: Some(oracle_text.to_string()),
            type_line: type_line.to_string(),
        });

        assert_eq!(result.status, "canonical", "{name} should be canonical");
        let rule = result.abilities[0].rule.as_ref().expect("canonical rule");
        assert_eq!(rule["kind"], rule_kind, "{name} rule kind");
        assert!(
            rule.to_string().contains(expected_function),
            "{name} should contain {expected_function}"
        );
        assert!(rule_is_executable(rule), "{name} should be executable");
    }
}

/// Feature: The Rust Oracle parser reaches every reviewed Rule IR in the initial deck corpus.
#[test]
fn rust_parser_matches_the_complete_oracle_ground_truth_corpus() {
    let root = workspace_root();
    let manifest = read_json(
        &root
            .join("fixtures")
            .join("oracle-ground-truth")
            .join("decks")
            .join("4c-control.json"),
    );
    let truth_directory = root
        .join("fixtures")
        .join("oracle-ground-truth")
        .join("cards");
    let cards = manifest["cards"].as_array().expect("manifest cards");
    let mut expected_ability_count = 0;
    let mut canonical_ability_count = 0;
    let mut mismatches = Vec::new();

    assert_eq!(cards.len(), 36);
    for card in cards {
        assert_eq!(card["reviewStatus"], "verified");
        let card_id = card["id"].as_str().expect("manifest card id");
        let truth = read_json(&truth_directory.join(format!("{card_id}.json")));
        let expected_abilities = truth["abilities"].as_array().expect("truth abilities");
        let result = parse_oracle_card(request_from_truth(&truth));

        expected_ability_count += expected_abilities.len();
        canonical_ability_count += result
            .abilities
            .iter()
            .filter(|ability| ability.status == "canonical")
            .count();

        if result.abilities.len() != expected_abilities.len() {
            mismatches.push(format!(
                "{card_id}: expected {} abilities, parsed {}",
                expected_abilities.len(),
                result.abilities.len(),
            ));
            continue;
        }

        for (index, (actual, expected)) in
            result.abilities.iter().zip(expected_abilities).enumerate()
        {
            let expected_source = &expected["source"];
            if actual.ability_type != expected["expectedRule"]["kind"] {
                mismatches.push(format!(
                    "{card_id} ability {}: classified {}, expected {}",
                    index + 1,
                    actual.ability_type,
                    expected["expectedRule"]["kind"],
                ));
            }
            if actual.iterations.first().map(|iteration| iteration.depth) != Some(0) {
                mismatches.push(format!(
                    "{card_id} ability {}: simplification does not start at depth 0",
                    index + 1,
                ));
            }
            if actual
                .iterations
                .iter()
                .enumerate()
                .any(|(depth, iteration)| iteration.depth != depth)
            {
                mismatches.push(format!(
                    "{card_id} ability {}: simplification depths are not contiguous",
                    index + 1,
                ));
            }
            let expected_initial_kind = if expected["expectedRule"]["kind"] == "manaAbility" {
                "activatedAbility"
            } else {
                expected["expectedRule"]["kind"]
                    .as_str()
                    .expect("expected initial rule kind")
            };
            if actual.iterations.first().is_none_or(|iteration| {
                iteration.result["kind"] != expected_initial_kind
                    || count_nodes_with_kind(&iteration.result, "unresolvedEntities") == 0
            }) {
                mismatches.push(format!(
                    "{card_id} ability {}: first simplification state is not a partitioned ability",
                    index + 1,
                ));
            }
            let partition_keys = actual
                .iterations
                .first()
                .and_then(|iteration| iteration.result.as_object())
                .map(|object| object.keys().cloned().collect::<Vec<_>>());
            let canonical_keys = expected["expectedRule"]
                .as_object()
                .map(|object| object.keys().cloned().collect::<Vec<_>>());
            if partition_keys != canonical_keys {
                mismatches.push(format!(
                    "{card_id} ability {}: classified sections {:?}, expected {:?}",
                    index + 1,
                    partition_keys,
                    canonical_keys,
                ));
            }
            if actual.iterations.last().is_none_or(|iteration| {
                iteration.result != expected["expectedRule"]
                    || count_nodes_with_kind(&iteration.result, "unresolvedEntities") != 0
            }) {
                mismatches.push(format!(
                    "{card_id} ability {}: final simplification state is not exact canonical Rule IR",
                    index + 1,
                ));
            }
            if actual.source.text != expected_source["text"]
                || actual.source.line_start != expected_source["lineStart"]
                || actual.source.line_end != expected_source["lineEnd"]
                || actual.source.face_id.as_deref() != expected_source["faceId"].as_str()
            {
                mismatches.push(format!(
                    "{card_id} ability {}: source boundary mismatch",
                    index + 1,
                ));
            }

            if actual.rule.as_ref() != Some(&expected["expectedRule"]) {
                mismatches.push(format!(
                    "{card_id} ability {}: expected {}, parsed {}",
                    index + 1,
                    expected["expectedRule"],
                    actual
                        .rule
                        .as_ref()
                        .map(Value::to_string)
                        .unwrap_or_else(|| "<unsupported>".to_string()),
                ));
            }
        }

        let stage_keys = result
            .stages
            .iter()
            .map(|stage| stage.key.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            stage_keys,
            vec![
                "entityRecognition",
                "entitySimplifier",
                "vocabularyExpansion",
                "primitiveExpansion",
                "engineRegistration",
            ],
            "{card_id} audit stages",
        );
    }

    assert_eq!(expected_ability_count, 62);
    assert!(
        mismatches.is_empty(),
        "{} parser mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n"),
    );
    assert_eq!(
        canonical_ability_count, expected_ability_count,
        "all reviewed abilities should reach a canonical terminal state",
    );
}
