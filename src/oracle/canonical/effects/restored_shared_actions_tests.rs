//! Shared actions restored for released cards (dungeon rooms, Channel,
//! Engineered Explosives, Acererak, Aluren). Each case composes existing
//! concepts; near-misses keep an explicit failure.
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::{Value, json};

fn parse(type_line: &str, text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn last_rule(type_line: &str, text: &str) -> Value {
    let parsed = parse(type_line, text);
    assert_eq!(
        parsed.status,
        "canonical",
        "{text}: {:?}",
        parsed
            .abilities
            .iter()
            .flat_map(|ability| ability.diagnostics.iter())
            .map(|diagnostic| (&diagnostic.state, &diagnostic.expected))
            .collect::<Vec<_>>()
    );
    parsed
        .abilities
        .last()
        .and_then(|ability| ability.rule.clone())
        .expect("a rule")
}

fn kinds(rule: &Value) -> Vec<&str> {
    rule["effects"]
        .as_array()
        .expect("effects")
        .iter()
        .map(|effect| effect["kind"].as_str().expect("kind"))
        .collect()
}

#[test]
fn each_player_unless_they_pay_distributes_the_payment() {
    // Veils of Fear, and a sibling over opponents with a sacrifice list.
    let rule = last_rule(
        "Sorcery",
        "Each player loses 2 life unless they discard a card.",
    );
    let iteration = &rule["effects"][0];
    assert_eq!(iteration["kind"], "forEachPlayer");
    assert_eq!(iteration["players"]["kind"], "eachPlayer");
    let player = json!({"kind":"boundValue","id":iteration["bind"]});
    assert_eq!(iteration["effects"][0]["kind"], "tryPayCost");
    assert_eq!(iteration["effects"][0]["player"], player);
    assert_eq!(iteration["effects"][1]["then"][0]["kind"], "loseLife");
    assert_eq!(iteration["effects"][1]["then"][0]["player"], player);
    let sibling = last_rule(
        "Sorcery",
        "Each opponent loses 3 life unless they sacrifice a creature, artifact, or land of their choice.",
    );
    let cost = &sibling["effects"][0]["effects"][0]["cost"];
    assert_eq!(cost["kind"], "sacrificePermanent");
    assert_eq!(cost["where"]["kind"], "or");
    // `they` names a quantified population, never a lone player.
    assert_ne!(
        parse("Sorcery", "You lose 2 life unless they discard a card.").status,
        "canonical"
    );
}

#[test]
fn one_sacrifice_verb_over_independently_headed_objects() {
    // Oubliette: one verb, three independently quantified sacrifices.
    let rule = last_rule(
        "Sorcery",
        "Discard a card and sacrifice a creature, an artifact, and a land.",
    );
    assert_eq!(
        kinds(&rule),
        [
            "discardCards",
            "sacrificePermanents",
            "sacrificePermanents",
            "sacrificePermanents"
        ]
    );
    assert_eq!(rule["effects"][3]["where"]["value"], "Land");
    let sibling = last_rule("Sorcery", "Sacrifice an enchantment and a creature.");
    assert_eq!(
        kinds(&sibling),
        ["sacrificePermanents", "sacrificePermanents"]
    );
}

#[test]
fn a_named_legendary_token_is_one_explicit_description() {
    let rule = last_rule(
        "Sorcery",
        "Create Voja Fenstalker, a legendary 5/5 green and white Elf Wolf creature token with trample.",
    );
    let token = &rule["effects"][0]["token"];
    assert_eq!(token["name"], "Voja Fenstalker");
    assert_eq!(token["supertypes"], json!(["Legendary"]));
    assert_eq!(token["abilities"][0]["kind"], "trample");
}

#[test]
fn drawn_cards_are_the_antecedent_of_a_resolution_free_cast() {
    // Mad Wizard's Lair (CR 608.2g): cast during resolution, from hand.
    let rule = last_rule(
        "Sorcery",
        "Draw three cards and reveal them. You may cast one of them without paying its mana cost.",
    );
    assert_eq!(kinds(&rule), ["drawCards", "revealCards", "castAnyNumber"]);
    assert_eq!(
        rule["effects"][0]["bind"],
        rule["effects"][2]["cards"]["binding"]
    );
    assert_eq!(rule["effects"][2]["sourceZone"], "hand");
    assert_eq!(rule["effects"][2]["maximum"]["value"], 1);
    // Revealed library cards are observed in the caster's own library: the
    // same resolution cast reads them there (`castFromObservedLibrary`).
    let rule = last_rule(
        "Sorcery",
        "Reveal the top two cards of your library. You may cast one of them without paying its mana cost.",
    );
    assert_eq!(kinds(&rule), ["bind", "revealCards", "castAnyNumber"]);
    assert_eq!(rule["effects"][2]["sourceZone"], "library");
    assert_eq!(
        rule["effects"][2]["cards"]["binding"],
        rule["effects"][0]["id"]
    );
    // Near miss: a cast zone contradicting the observed library is refused.
    assert_ne!(
        parse(
            "Sorcery",
            "Reveal the top two cards of your library. You may cast one of them without paying its mana cost from your graveyard."
        )
        .status,
        "canonical"
    );
}

#[test]
fn a_chosen_card_enters_with_counters_and_stays_the_antecedent() {
    // Throne of the Dead Three.
    let rule = last_rule(
        "Sorcery",
        "Reveal the top ten cards of your library. Put a creature card from among them onto the battlefield with three +1/+1 counters on it. It gains hexproof until your next turn. Then shuffle.",
    );
    assert_eq!(
        kinds(&rule),
        [
            "bind",
            "revealCards",
            "chooseCards",
            "moveCards",
            "grantKeyword",
            "shuffleZone"
        ]
    );
    let entry = &rule["effects"][3];
    assert_eq!(entry["to"]["kind"], "battlefield");
    assert_eq!(entry["to"]["enterWithCounters"][0]["count"]["value"], 3);
    assert_eq!(rule["effects"][4]["object"]["binding"], entry["bind"]);
    // Sibling: a different counter, keyword and duration on the same frame.
    let sibling = last_rule(
        "Sorcery",
        "Look at the top five cards of your library. Put an artifact card from among them onto the battlefield with a shield counter on it. It gains haste until end of turn.",
    );
    assert_eq!(
        sibling["effects"][2]["to"]["enterWithCounters"][0]["counter"],
        "shield"
    );
}

#[test]
fn venture_completion_condition_and_owner_hand_compose_in_a_trigger() {
    // Acererak: intervening `if`, a source bounce, and a keyword action.
    let rule = last_rule(
        "Legendary Creature \u{2014} Zombie Wizard",
        "When this creature enters, if you haven't completed Tomb of Annihilation, return this creature to its owner's hand and venture into the dungeon.",
    );
    assert_eq!(rule["condition"]["kind"], "not");
    assert_eq!(rule["condition"]["operand"]["name"], "Tomb of Annihilation");
    assert_eq!(kinds(&rule), ["returnToOwnersHand", "ventureDungeon"]);
    let sibling = last_rule(
        "Creature \u{2014} Human",
        "When this creature enters, if you've completed a dungeon, venture into Undercity.",
    );
    assert_eq!(sibling["condition"]["kind"], "completedDungeon");
    assert!(sibling["condition"].get("name").is_none());
    assert_eq!(sibling["effects"][0]["procedure"], "venture into Undercity");
}

#[test]
fn any_player_free_flash_permission_is_the_shared_static_permission() {
    // Aluren.
    let rule = last_rule(
        "Enchantment",
        "Any player may cast creature spells with mana value 3 or less without paying their mana costs and as though they had flash.",
    );
    let modifier = &rule["modifiers"][0];
    assert_eq!(modifier["players"]["kind"], "eachPlayer");
    assert_eq!(modifier["sourceZone"], "hand");
    assert_eq!(modifier["withoutPayingManaCost"], true);
    assert_eq!(modifier["asThoughFlash"], true);
    assert_ne!(
        parse(
            "Enchantment",
            "Any player may cast creature spells as though they had flash and without paying their mana costs."
        )
        .status,
        "canonical"
    );
}

#[test]
fn a_labeled_hand_activation_reads_it_they_and_its_cost_reduction() {
    let reduction =
        " This ability costs {1} less to activate for each legendary creature you control.";
    for (effect, expected) in [
        (
            "It deals 4 damage to target attacking or blocking creature.",
            vec!["dealDamage"],
        ),
        (
            "Create two 1/1 colorless Spirit creature tokens. They gain haste until end of turn.",
            vec!["createTokens", "grantKeyword"],
        ),
        (
            "Mill three cards, then return a creature or planeswalker card from your graveyard to your hand.",
            vec!["mill", "chooseCards", "moveCards"],
        ),
    ] {
        let rule = last_rule(
            "Legendary Land",
            &format!("Channel \u{2014} {{2}}{{R}}, Discard this card: {effect}{reduction}"),
        );
        assert_eq!(rule["activationZone"], "hand", "{effect}");
        assert_eq!(
            rule["manaCostReduction"]["kind"], "countPermanents",
            "{effect}"
        );
        assert_eq!(kinds(&rule), expected, "{effect}");
    }
    let sniper = last_rule(
        "Artifact Creature \u{2014} Goblin Archer",
        "Channel \u{2014} {1}{R}, Discard this card: It deals 2 damage to any target.",
    );
    assert_eq!(sniper["effects"][0]["source"], json!({"kind":"self"}));
    assert!(sniper.get("manaCostReduction").is_none());
    // `it` is unproved when the source is retained and nothing else is named.
    assert_ne!(
        parse("Artifact", "{T}: It deals 1 damage to any target.").status,
        "canonical"
    );
    // `they` needs created tokens, not any earlier plural.
    assert_ne!(
        parse(
            "Sorcery",
            "Draw two cards. They gain haste until end of turn."
        )
        .status,
        "canonical"
    );
}

#[test]
fn a_collection_compares_with_the_sacrificed_sources_counters() {
    // Engineered Explosives (CR 608.2h: last-known information of the source).
    let rule = last_rule(
        "Artifact",
        "{2}, Sacrifice this artifact: Destroy each nonland permanent with mana value equal to the number of charge counters on this artifact.",
    );
    let filter = &rule["effects"][0]["permanent"]["where"]["operands"][1];
    assert_eq!(filter["kind"], "compare");
    assert_eq!(
        filter["right"],
        json!({"kind":"countCounters","object":{"kind":"self"},"counter":"charge"})
    );
    let sibling = last_rule(
        "Artifact",
        "{1}, {T}: Destroy each artifact with mana value equal to the number of fuse counters on this artifact.",
    );
    assert_eq!(sibling["effects"][0]["kind"], "destroyPermanent");
    // A spell has no permanent source whose counters a query could read.
    assert_ne!(
        parse(
            "Sorcery",
            "Destroy each nonland permanent with mana value equal to the number of charge counters on this artifact."
        )
        .status,
        "canonical"
    );
}
