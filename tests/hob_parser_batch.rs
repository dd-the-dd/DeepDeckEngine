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
    assert!(
        !parsed.abilities.is_empty(),
        "{name} has no parsed abilities"
    );
    for ability in &parsed.abilities {
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
}

#[test]
fn hob_cards_88_and_90_are_canonical_and_executable() {
    assert_card_is_executable(
        "Bombur, Gentle Dreamer",
        "Legendary Creature - Dwarf Citizen",
        "{2}{R}",
        "Bombur doesn't untap during your untap step unless you have an enduring story.",
    );
    assert_card_is_executable(
        "Burn, Burn, Tree and Fern",
        "Enchantment - Saga",
        "{2}{R}",
        "III, IV — Add {R}.",
    );
}

#[test]
fn hob_card_91_is_canonical_and_executable() {
    assert_card_is_executable(
        "Dáin Ironfoot",
        "Legendary Creature - Dwarf Warrior",
        "{2}{R}{R}",
        "When Dáin enters, create a colorless Equipment artifact token named Axe with \"Equipped creature gets +1/+0\" and equip {2}. When you do, attach it to target creature you control.\nWhenever Dáin attacks, each equipped attacking creature gains double strike until end of turn.",
    );
}

#[test]
fn hob_card_93_is_canonical_and_executable() {
    assert_card_is_executable(
        "Desolation of Smaug",
        "Sorcery",
        "{3}{R}",
        "Add four mana in any combination of colors. Spend this mana only to cast Dragon spells.",
    );
}

#[test]
fn hob_card_95_is_canonical_and_executable() {
    assert_card_is_executable(
        "Dwarven Mauler",
        "Creature - Dwarf Warrior",
        "{2}{R}",
        "Equip abilities you activate that target this creature cost {2} less to activate.",
    );
}

#[test]
fn hob_card_97_is_canonical_and_executable() {
    assert_card_is_executable(
        "Gandalf, Spark Starter",
        "Legendary Creature - Avatar Wizard",
        "{3}{R}",
        "When Gandalf enters, he deals 3 damage divided as you choose among one, two, or three targets.",
    );
}

#[test]
fn hob_card_102_is_canonical_and_executable() {
    assert_card_is_executable(
        "Iron Hills Stalwart",
        "Creature - Dwarf Warrior",
        "{2}{R}",
        "When this creature enters, attach target Equipment you control to up to one target creature you control.",
    );
}

#[test]
fn hob_card_107_is_canonical_and_executable() {
    assert_card_is_executable(
        "Pinecone Strike",
        "Sorcery",
        "{1}{R}",
        "Choose one or both —\n• Pinecone Strike deals 3 damage to target creature. If that creature would die this turn, exile it instead.\n• Destroy target artifact token.",
    );
}

#[test]
fn hob_cards_108_and_112_are_canonical_and_executable() {
    assert_card_is_executable(
        "Ragged Short Spear",
        "Artifact - Equipment",
        "{1}{R}",
        "When this Equipment enters, you may discard a card. If you do, draw two cards.",
    );
    assert_card_is_executable(
        "Snowslope Hunter",
        "Creature - Dwarf Warrior",
        "{3}{R}",
        "Sacrifice another creature or artifact: Exile the top card of your library. You may play it until the end of your next turn. Activate only during your turn and only once each turn.",
    );
}

#[test]
fn hob_cards_119_and_120_are_canonical_and_executable() {
    assert_card_is_executable(
        "Beorn the Fierce",
        "Legendary Creature - Bear Shapeshifter Warrior",
        "{3}{G}{G}",
        "Trample\nOther Bears you control get +2/+2.\nAt the beginning of combat on your turn, put a trample counter on up to one target creature you control. It becomes a Bear in addition to its other types. Then if you control three or more Bears, draw two cards.",
    );
    assert_card_is_executable(
        "Beorn's Hospitality",
        "Enchantment",
        "{1}{G}",
        "Landfall \u{2014} Whenever a land you control enters, put a +1/+1 counter on target creature you control.\n{5}{G}{G}: This enchantment becomes a Bear creature in addition to its other types and gains \"This creature's power and toughness are each equal to the number of lands you control.\" (This effect doesn't end.)",
    );
}

#[test]
fn hob_cards_123_and_124_are_canonical_and_executable() {
    assert_card_is_executable(
        "Dancing from Dark to Dawn",
        "Enchantment",
        "{3}{G}{G}",
        "Whenever you cast a creature spell, put X +1/+1 counters on target creature you control, where X is that spell's mana value.\nLandfall \u{2014} Whenever a land you control enters, create a 2/2 green Bear creature token.",
    );
    assert_card_is_executable(
        "Down in the Valley",
        "Enchantment - Saga",
        "{2}{G}",
        "(As this Saga enters and after your draw step, add a lore counter. Sacrifice after IV.)\nI \u{2014} Search your library for a basic land card, reveal it, put it into your hand, then shuffle.\nII \u{2014} This Saga gains \"Landfall \u{2014} Whenever a land you control enters, create a 1/1 green Elf creature token.\"\nIII, IV \u{2014} Elves you control get +1/+0 and gain vigilance until end of turn.",
    );
}

#[test]
fn hob_cards_125_and_128_are_canonical_and_executable() {
    assert_card_is_executable(
        "Galion, Elvenking's Butler",
        "Legendary Creature - Elf Advisor",
        "{2}{G}{G}",
        "Whenever Galion attacks, choose up to one other target creature you control. Its base power and toughness become equal to Galion's power and toughness until end of turn.",
    );
    assert_card_is_executable(
        "Little Bear",
        "Creature - Bear",
        "{2}{G}",
        "Flash\nWhen this creature enters, untap another target creature you control. If that creature is a Bear, put a +1/+1 counter on it.",
    );
}

#[test]
fn hob_cards_137_and_138_are_canonical_and_executable() {
    assert_card_is_executable(
        "Through the Forest Gate",
        "Sorcery",
        "{6}{G}{G}",
        "Look at the top twenty cards of your library, put any number of land cards from among them onto the battlefield tapped, then shuffle. You gain 8 life.",
    );
    assert_card_is_executable(
        "Troll Negotiations",
        "Sorcery",
        "{2}{G}{G}",
        "Put two +1/+1 counters on target creature you control. Then it fights target creature an opponent controls. (Each deals damage equal to its power to the other.)",
    );
}

#[test]
fn hob_cards_140_and_143_are_canonical_and_executable() {
    assert_card_is_executable(
        "Wargling",
        "Creature - Wolf",
        "{1}{G}",
        "Ferocious \u{2014} Whenever this creature attacks while you control a creature with power 4 or greater, until end of turn, this creature gets +1/+0 and creatures you control gain trample.",
    );
    assert_card_is_executable(
        "Woodland Weavemaster",
        "Creature - Elf Druid",
        "{1}{G}",
        "Vigilance\nWhenever another Elf you control enters, this creature gets +1/+1 until end of turn.\n{T}: Add X mana of any one color, where X is this creature's power. Spend this mana only to cast Elf spells and activate abilities of Elf sources.",
    );
}

#[test]
fn hob_cards_147_and_148_are_canonical_and_executable() {
    assert_card_is_executable(
        "Bifur, Melodic Rider",
        "Legendary Creature - Dwarf Bard",
        "{4}{R/W}{R/W}",
        "Storied (If you control three or more artifacts, legendaries, and/or Sagas, you have an enduring story for the rest of the game.)\nWhenever Bifur enters or attacks, put a +1/+1 counter on target creature.\nAs long as you have an enduring story, if a triggered ability of a Dwarf you control triggers, that ability triggers an additional time.",
    );
    assert_card_is_executable(
        "Bolg of the North",
        "Legendary Creature - Goblin Soldier",
        "{3}{B}{R}",
        "When Bolg enters, you may sacrifice another creature. When you do, Bolg deals damage equal to that creature's power to another target creature. If excess damage was dealt this way, amass Goblins X, where X is that excess damage. (Put X +1/+1 counters on an Army you control. It's also a Goblin. If you don't control an Army, create a 0/0 black Goblin Army creature token first.)",
    );
}

#[test]
fn hob_cards_150_and_153_are_canonical_and_executable() {
    assert_card_is_executable(
        "The Chief Warg",
        "Legendary Creature - Wolf",
        "{2}{B}{G}",
        "Menace (This creature can't be blocked except by two or more creatures.)\nFerocious \u{2014} Whenever you attack while you control a creature with power 4 or greater, you draw a card and lose 1 life.",
    );
    assert_card_is_executable(
        "Duskwatch Hunter",
        "Creature - Wolf",
        "{2}{B/G}",
        "This creature can't be blocked by tokens.\nWhen this creature enters, put a +1/+1 counter on target creature.",
    );
}

#[test]
fn hob_cards_155_and_157_are_canonical_and_executable() {
    assert_card_is_executable(
        "Eagle's Rescue",
        "Enchantment - Aura",
        "{2}{W/U}{W/U}",
        "Enchant creature\nEnchanted creature gets +2/+2 and has flying.\n{2}{W/U}{W/U}: Return this card from your graveyard to the battlefield attached to target creature you control with power 1 or less. Activate only as a sorcery.",
    );
    assert_card_is_executable(
        "Goblin Plate Mail",
        "Artifact - Equipment",
        "{1}{B/R}",
        "When this Equipment enters, amass Goblins 1, then attach this Equipment to the amassed Army. (To amass Goblins 1, put a +1/+1 counter on an Army you control. It's also a Goblin. If you don't control an Army, create a 0/0 black Goblin Army creature token first.)\nEquipped creature gets +1/+0 and has menace.\nEquip {4}",
    );
}

#[test]
fn hob_cards_163_and_168_are_canonical_and_executable() {
    assert_card_is_executable(
        "Silvan Reveler",
        "Creature - Elf Citizen",
        "{2}{G}{U}",
        "When this creature enters, draw a card, then discard a card. If you discard a land card this way, put it from your graveyard onto the battlefield tapped.\nLandfall \u{2014} Whenever a land you control enters, you may pay {1}{G}{U}. If you do, return this card from your graveyard to your hand.",
    );
    assert_card_is_executable(
        "Thranduil's Company",
        "Creature - Elf Soldier",
        "{2}{G}{U}",
        "As long as you control another Elf, you may play an additional land on each of your turns.\nLandfall \u{2014} Whenever a land you control enters, put two +1/+1 counters on target creature you control. It gains vigilance until end of turn.",
    );
}

#[test]
fn hob_cards_169_and_171_are_canonical_and_executable() {
    assert_card_is_executable(
        "Tom, Bert, and William",
        "Legendary Creature - Troll",
        "{3}{B}{G}",
        "{1}, Sacrifice another creature: Draw cards equal to the sacrificed creature's power, then discard a card.\nWhen Tom, Bert, and William die, if they were a creature, return them to the battlefield. They're an artifact. (They're no longer a creature.)",
    );
    assert_card_is_executable(
        "The Black Arrow",
        "Legendary Artifact - Equipment",
        "{3}",
        "Flash\nWhen The Black Arrow enters, it deals 1 damage to any target. If a Dragon is dealt damage this way, destroy it.\nEquipped creature gets +1/+1 and has reach.\nEquip {1} ({1}: Attach to target creature you control. Equip only as a sorcery.)",
    );
}

#[test]
fn hob_cards_172_and_175_are_canonical_and_executable() {
    assert_card_is_executable(
        "Dwarven Mattock",
        "Artifact - Equipment",
        "{2}",
        "When this Equipment enters, attach it to target Dwarf you control.\nEquipped creature gets +2/+2 and has ward {1}. (Whenever equipped creature becomes the target of a spell or ability an opponent controls, counter it unless that player pays {1}.)\nEquip {3} ({3}: Attach to target creature you control. Equip only as a sorcery.)",
    );
    assert_card_is_executable(
        "Key to the Side-Door",
        "Artifact",
        "{1}",
        "{2}, {T}: Target creature can't be blocked this turn.\n{1}, {T}, Discard a legendary card with the same name as a legendary permanent you control: Draw two cards.",
    );
}

#[test]
fn hob_cards_176_and_184_are_canonical_and_executable() {
    assert_card_is_executable(
        "My Precious",
        "Legendary Artifact - Equipment",
        "{3}",
        "Equipped creature has hexproof and can't be blocked.\nEquip-{2}, Pay 2 life.",
    );
    assert_card_is_executable(
        "Allure of Power",
        "Instant - Adventure",
        "{1}{B}",
        "As an additional cost to cast this spell, sacrifice a creature.\nDraw two cards.",
    );
    assert_card_is_executable(
        "Hobbit Hole",
        "Land",
        "",
        "{T}, Sacrifice this land: Search your library for a basic land card, put it onto the battlefield tapped, then shuffle.\nHalflingcycling {4} ({4}, Discard this card: Search your library for a Halfling card, reveal it, put it into your hand, then shuffle.)",
    );
}

#[test]
fn hob_cards_201_and_202_are_canonical_and_executable() {
    assert_card_is_executable(
        "The Great Goblin",
        "Legendary Creature - Goblin Noble",
        "{1}{B/R}{B/R}",
        "Whenever you put one or more counters on a Goblin, Orc, or Army you control, The Great Goblin deals 2 damage to target opponent.\nWhenever another Goblin, Orc, or Army you control dies, exile the top card of your library. You may play it until the end of your next turn.",
    );
    assert_card_is_executable(
        "Thorin Oakenshield",
        "Legendary Creature - Dwarf Noble",
        "{R}{W}",
        "Trample\nStoried (If you control three or more artifacts, legendaries, and/or Sagas, you have an enduring story for the rest of the game.)\nAs long as you have an enduring story, artifacts and creatures you control have ward {1}.",
    );
}

#[test]
fn hob_cards_204_and_207_are_canonical_and_executable() {
    assert_card_is_executable(
        "Glamdring, Foe-hammer",
        "Legendary Artifact - Equipment",
        "{2}",
        "Instant and sorcery spells you cast cost {X} less to cast, where X is equipped creature's power.\nEquip {2}",
    );
    assert_card_is_executable(
        "Gleam of Death",
        "Sorcery - Adventure",
        "{3}{U}",
        "Mill six cards, then put all instant and sorcery cards from among them into your hand. (Then exile this card. You may cast the artifact later from exile.)",
    );
    assert_card_is_executable(
        "The Lonely Mountain",
        "Land - Mountain",
        "",
        "({T}: Add {R}.)\nThis land enters tapped unless you control an Equipment.\n{4}{R}, {T}: Create a 2/2 red Dwarf creature token. This ability costs {1} less to activate for each Equipment you control. Activate only as a sorcery.",
    );
}

#[test]
fn hob_cards_208_and_210_are_canonical_and_executable() {
    assert_card_is_executable(
        "Chief Warg's Company",
        "Creature - Wolf",
        "{1}{B}{G}",
        "Trample\nThis creature can't attack unless you control two or more other Wolves.\nAt the beginning of your upkeep, create a 2/2 green Wolf creature token.",
    );
    assert_card_is_executable(
        "Bard's Company",
        "Creature - Human Citizen",
        "{2}{W}{U}",
        "You may cast this spell as though it had flash if you control a Human.\nOther creatures you control get +1/+1.\nWhenever this creature enters or attacks, recruit. (Draw a card, then discard a card. If you discarded a nonland card, create a 1/1 white Human Soldier creature token.)",
    );
}

#[test]
fn hob_cards_225_and_233_are_canonical_and_executable() {
    assert_card_is_executable(
        "Desert Were-Worm",
        "Creature - Dragon Wurm",
        "{4}{R}{R}",
        "This creature gets +2/+0 for each Mountain you control.\nWhenever you attack with creatures with total power 12 or greater for the first time each turn, untap all attacking creatures. After this phase, there is an additional combat phase.",
    );
    assert_card_is_executable(
        "Thranduil, Sindarin Liege",
        "Legendary Creature - Elf Noble",
        "{2}{G/U}{G/U}",
        "Other Elves you control get +1/+1.\nLandfall - Whenever a land you control enters, create a 1/1 green Elf creature token.",
    );
    assert_card_is_executable(
        "Silvan Rally",
        "Sorcery - Adventure",
        "{1}{G/U}{G/U}",
        "Mill four cards, then put up to two land cards from among them into your hand. (Then exile this card. You may cast the creature later from exile.)",
    );
}

#[test]
fn hob_cards_236_and_237_are_canonical_and_executable() {
    assert_card_is_executable(
        "Orcrist, Goblin-cleaver",
        "Legendary Artifact - Equipment",
        "{3}",
        "Equipped creature gets +2/+2 and has trample.\nWhenever equipped creature deals combat damage to a player, choose a creature type. Create a Treasure token for each creature you control of that type.\nEquip {3}",
    );
    assert_card_is_executable(
        "Sting, Bilbo's Sword",
        "Legendary Artifact - Equipment",
        "{2}",
        "Flash\nWhen Sting enters, put a hone counter on Sting for each creature target opponent controls. Attach Sting to up to one target creature you control. (Each hone counter on an Equipment grants +1/+0 to equipped creature.)\nEquip {3}",
    );
}

#[test]
fn hob_cards_244_and_245_are_canonical_and_executable() {
    assert_card_is_executable(
        "Bard, King of Dale",
        "Legendary Creature - Human Noble Archer",
        "{4}{W}{U}",
        "Reach, vigilance\nIf you would draw a card except the first one you draw in each of your draw steps, draw two cards instead.\nIf one or more tokens would be created under your control, twice that many of those tokens are created instead.",
    );
    assert_card_is_executable(
        "Smaug, Wicked Worm",
        "Legendary Creature - Dragon",
        "{3}{B}{R}",
        "Flying\nWhen Smaug enters, create X tapped Treasure tokens, where X is the number of artifacts your opponents control.\nWhenever you cast a spell, if mana from a Treasure was spent to cast it, you draw a card and lose 1 life.",
    );
}

#[test]
fn hob_cards_249_and_263_are_canonical_and_executable() {
    assert_card_is_executable(
        "Smaug the Magnificent",
        "Legendary Creature - Dragon",
        "{2}{R}{R}",
        "Flying, haste\nWhenever Smaug attacks, he deals damage equal to the number of Treasures you control to any target.\nAt the beginning of your upkeep, create a Treasure token.",
    );
    assert_card_is_executable(
        "Glóin the Mighty",
        "Legendary Creature - Dwarf Warrior",
        "{3}{R}",
        "At the beginning of your first main phase, add {R}{R}.",
    );
    assert_card_is_executable(
        "Easy Pickings",
        "Sorcery - Adventure",
        "{2}{R}",
        "Easy Pickings deals 1 damage to each creature your opponents control. (Then exile this card. You may cast the creature later from exile.)",
    );
}

#[test]
fn hob_card_268_is_canonical_and_executable() {
    assert_card_is_executable(
        "The Notary Hobbits",
        "Legendary Creature - Halfling Advisor",
        "{3}{G}{G}",
        "When The Notary Hobbits enter, if they're not a token, create two tokens that are copies of them, except the tokens aren't legendary.\n{T}: Add {C} for each Halfling you control.",
    );
}

#[test]
fn hob_card_274_is_canonical_and_executable() {
    assert_card_is_executable(
        "Elven Passage",
        "Land",
        "",
        "{T}, Pay 1 life, Sacrifice this land: Search your library for a basic land card, put it onto the battlefield tapped, then shuffle. You may behold an Elf. If you do, untap that land. (To behold an Elf, choose an Elf you control or reveal an Elf card from your hand.)",
    );
}

#[test]
fn hob_card_278_is_canonical_and_executable() {
    assert_card_is_executable(
        "Flameshape",
        "Sorcery - Adventure",
        "{2}{R}",
        "Look at the top two cards of your library and exile them face down. For as long as they remain exiled, you may play them if you control a Wizard.",
    );
}

#[test]
fn hob_card_279_is_canonical_and_executable() {
    assert_card_is_executable(
        "Thorin, Mountain-king",
        "Legendary Creature - Dwarf Noble",
        "{3}{R}{W}",
        "When Thorin enters, attach any number of target Equipment you control to target creature you control. When one or more Equipment become attached to that creature this way, that creature deals damage equal to its power to up to one target creature.",
    );
}

#[test]
fn hob_card_282_is_canonical_and_executable() {
    assert_card_is_executable(
        "Thranduil, the Elvenking",
        "Legendary Creature - Elf Noble",
        "{2}{G}{U}",
        "Thranduil has all activated abilities of all Elf cards in your graveyard.",
    );
}

#[test]
fn hob_card_293_is_canonical_and_executable() {
    assert_card_is_executable(
        "Uncover the Moon-Letters",
        "Enchantment",
        "{2}{U}",
        "Whenever you cast a noncreature spell, you may draw X cards, where X is the amount of mana spent to cast that spell. If you do, discard two cards.",
    );
}

#[test]
fn hob_card_296_is_canonical_and_executable() {
    assert_card_is_executable(
        "Inside Information",
        "Sorcery",
        "{X}{B}{B}",
        "Exile the top X cards of target opponent's library. You may play those cards this turn. If you cast a spell this way, pay life equal to its mana value rather than pay its mana cost.",
    );
}

#[test]
fn hob_card_299_is_canonical_and_executable() {
    assert_card_is_executable(
        "The Sackville-Bagginses",
        "Legendary Creature - Halfling Noble",
        "{2}{B}{G}",
        "Whenever you sacrifice a token, target opponent loses 1 life.\nWhen The Sackville-Bagginses enter, you may sacrifice another creature or artifact. If you do, draw a card and create a Treasure token.",
    );
}

#[test]
fn hob_card_303_is_canonical_and_executable() {
    assert_card_is_executable(
        "Getaway Barrel",
        "Artifact",
        "{3}",
        "When this artifact is put into a graveyard from the battlefield, reveal the top thirteen cards of your library. Put a random creature card from among them onto the battlefield. Put the rest on the bottom of your library in a random order.",
    );
}

#[test]
fn hob_card_309_is_canonical_and_executable() {
    assert_card_is_executable(
        "Radagast of Rhosgobel",
        "Legendary Creature - Avatar Wizard",
        "{2}{G}{U}",
        "The first creature spell you cast each turn costs {2} less to cast and can be cast as though it had flash.",
    );
}

#[test]
fn hob_card_308_is_canonical_and_executable() {
    assert_card_is_executable(
        "Part in Friendship",
        "Enchantment",
        "{3}{G}",
        "Whenever a nontoken creature you control dies, reveal cards from the top of your library until you reveal a creature card. If its mana value is less than or equal to the number of lands you control, put it onto the battlefield. Otherwise, put it into your hand. Put the rest on the bottom of your library in a random order. This ability triggers only once each turn.",
    );
}

#[test]
fn hob_card_321_is_canonical_and_executable() {
    assert_card_is_executable(
        "The Misty Mountains Cold",
        "Enchantment - Saga",
        "{3}{R}",
        "I, II, III, IV — Create a Treasure token. Then if you control four or more Treasures, sacrifice this Saga. If you do, create a 6/6 red Dragon creature token with flying. (A Treasure token is an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")",
    );
}
