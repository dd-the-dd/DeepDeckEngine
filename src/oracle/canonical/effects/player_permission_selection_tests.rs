//! Two shared frames restored for the league cohort: the static player
//! permission (actor + play/cast/look action + origin + base cost + timing
//! manner) and the choose-then-move selection (`return N PERMANENTS to its
//! owner's hand`). Each has real cards, an unused sibling combination and
//! near misses that must stay explicit failures.
use serde_json::{Value, json};

use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(name: &str, type_line: &str, mana_cost: &str, text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes")
}

/// Every ability of the complete card is canonical; returns their rules.
fn canonical_rules(name: &str, type_line: &str, mana_cost: &str, text: &str) -> Vec<Value> {
    let parsed = parse(name, type_line, mana_cost, text);
    assert_eq!(parsed.status, "canonical", "{text}: {parsed:#?}");
    parsed
        .abilities
        .iter()
        .filter(|ability| ability.status != "empty")
        .map(|ability| {
            assert_eq!(ability.status, "canonical", "{text}: {ability:#?}");
            ability.rule.clone().expect("canonical rule")
        })
        .collect()
}

/// The single ability of a one-line text is not canonical.
fn rejected(type_line: &str, text: &str) {
    let parsed = parse("Probe", type_line, "{2}", text);
    let ability = parsed
        .abilities
        .iter()
        .find(|ability| ability.status != "empty")
        .expect("one ability");
    assert_ne!(ability.status, "canonical", "{text}: {:#?}", ability.rule);
}

fn controller() -> Value {
    json!({"kind":"controllerOf","object":{"kind":"self"}})
}

fn static_rule(modifier: Value) -> Value {
    json!({
        "kind":"staticAbility", "source":{"kind":"self"},
        "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
        "modifiers":[modifier]
    })
}

fn card_type(value: &str) -> Value {
    json!({"kind":"cardTypeContains","value":value})
}

fn nonland_and(spell: Value) -> Value {
    json!({"kind":"and","operands":[{"kind":"not","operand":card_type("Land")}, spell]})
}

#[test]
fn graveyard_land_permission_is_a_play_action_with_a_graveyard_origin() {
    let rules = canonical_rules(
        "Icetill Explorer",
        "Creature — Insect Scout",
        "{2}{G}{G}",
        "You may play an additional land on each of your turns.\nYou may play lands from your graveyard.\nLandfall — Whenever a land you control enters, mill a card.",
    );
    assert_eq!(
        rules[1],
        static_rule(json!({"kind":"playLandsFromGraveyard","player":controller()}))
    );
    // The additional-land allowance keeps its own reader.
    assert_eq!(rules[0]["modifiers"][0]["kind"], "additionalLandPlay");
}

#[test]
fn flash_manner_composes_with_any_spell_query_and_the_first_spell_ordinal() {
    let blue_spirit = canonical_rules(
        "The Blue Spirit",
        "Legendary Creature — Human Rogue Ally",
        "{2}{U}",
        "You may cast the first creature spell you cast each turn as though it had flash.",
    );
    assert_eq!(
        blue_spirit[0],
        static_rule(json!({
            "kind":"grantFlashCasting","player":controller(),
            "where":card_type("Creature"),"firstEachTurn":true
        }))
    );
    let leyline = canonical_rules(
        "Leyline of Anticipation",
        "Enchantment",
        "{2}{U}{U}",
        "If this card is in your opening hand, you may begin the game with it on the battlefield.\nYou may cast spells as though they had flash.",
    );
    assert_eq!(
        leyline[1],
        static_rule(json!({
            "kind":"grantFlashCasting","player":controller(),"where":null,"firstEachTurn":false
        }))
    );
    // Unused combination: graveyard origin + flash manner without `and`.
    let sibling = canonical_rules(
        "Probe",
        "Enchantment",
        "{2}",
        "You may cast instant spells from your graveyard as though they had flash.",
    );
    assert_eq!(
        sibling[0],
        static_rule(json!({
            "kind":"castingPermission","players":controller(),"sourceZone":"graveyard",
            "where":card_type("Instant"),"withoutPayingManaCost":false,"asThoughFlash":true
        }))
    );
}

#[test]
fn library_top_origin_composes_look_play_and_cast_actions_into_one_card_filter() {
    let chorus = canonical_rules(
        "Elven Chorus",
        "Enchantment",
        "{3}{G}",
        "You may look at the top card of your library any time.\nYou may cast creature spells from the top of your library.\nCreatures you control have \"{T}: Add one mana of any color.\"",
    );
    assert_eq!(
        chorus[0],
        static_rule(json!({"kind":"lookAtTopLibrary","player":controller()}))
    );
    assert_eq!(
        chorus[1],
        static_rule(json!({
            "kind":"playCardsFromTopLibrary","player":controller(),
            "where":nonland_and(card_type("Creature"))
        }))
    );
    let future_sight = canonical_rules(
        "Future Sight",
        "Enchantment",
        "{2}{U}{U}{U}",
        "You may play lands and cast spells from the top of your library.",
    );
    assert_eq!(
        future_sight[0]["modifiers"][0]["where"],
        json!({"kind":"or","operands":[card_type("Land"),{"kind":"not","operand":card_type("Land")}]})
    );
    // Unused combination: a subtype-filtered spell query after a land play.
    let sibling = canonical_rules(
        "Probe",
        "Creature — Bird",
        "{2}{G}",
        "You may play lands and cast Bird spells from the top of your library.",
    );
    assert_eq!(
        sibling[0]["modifiers"][0]["where"],
        json!({"kind":"or","operands":[
            card_type("Land"),
            nonland_and(json!({"kind":"subtypeContains","value":"Bird"}))
        ]})
    );
}

#[test]
fn permission_near_misses_fail_at_their_own_capability() {
    for (type_line, text) in [
        // The runtime holds these permissions for the source's controller only.
        (
            "Enchantment",
            "Any player may cast spells as though they had flash.",
        ),
        // Only land plays have a graveyard play route.
        ("Enchantment", "You may play cards from your graveyard."),
        // The pronoun agrees with the plural query.
        ("Enchantment", "You may cast spells as though it had flash."),
        // Waived base costs are not proved from a library.
        (
            "Enchantment",
            "You may cast spells from the top of your library without paying their mana costs.",
        ),
        // A static chosen type is not a resolution-time chosen type.
        (
            "Creature — Elf",
            "You may cast creature spells of the chosen type from the top of your library.",
        ),
        (
            "Creature — Elf",
            "You may look at the top two cards of your library any time.",
        ),
        // A later ordinal is not represented by the spell history.
        (
            "Enchantment",
            "You may cast the second creature spell you cast each turn as though it had flash.",
        ),
    ] {
        rejected(type_line, text);
    }
}

fn selected_return(where_filter: Value, minimum: i64, maximum: i64) -> Vec<Value> {
    vec![
        json!({
            "kind":"choosePermanents","id":"effect0:selectedPermanents","player":controller(),
            "candidates":{"kind":"permanents","where":where_filter,"controller":controller()},
            "minimum":{"kind":"integer","value":minimum},"maximum":{"kind":"integer","value":maximum}
        }),
        json!({"kind":"returnToOwnersHand","object":{"kind":"decisionResult","decisionId":"effect0:selectedPermanents"}}),
    ]
}

#[test]
fn return_selection_is_a_resolution_choice_then_an_owner_hand_move() {
    let karoo = canonical_rules(
        "Dimir Aqueduct",
        "Land",
        "",
        "This land enters tapped.\nWhen this land enters, return a land you control to its owner's hand.\n{T}: Add {U}{B}.",
    );
    assert_eq!(karoo[1]["kind"], "triggeredAbility");
    assert_eq!(
        karoo[1]["effects"],
        Value::Array(selected_return(card_type("Land"), 1, 1))
    );
    let time_wipe = canonical_rules(
        "Time Wipe",
        "Sorcery",
        "{2}{W}{W}{U}",
        "Return a creature you control to its owner's hand, then destroy all creatures.",
    );
    let effects = time_wipe[0]["effects"].as_array().expect("effects");
    assert_eq!(
        effects[..2],
        selected_return(card_type("Creature"), 1, 1)[..]
    );
    assert_eq!(effects[2]["kind"], "destroyPermanent");
    // Unused combination: up-to bounds + the plural owner possessive.
    let sibling = canonical_rules(
        "Probe",
        "Sorcery",
        "{1}{U}",
        "Return up to two lands you control to their owner's hand.",
    );
    assert_eq!(
        sibling[0]["effects"],
        Value::Array(selected_return(card_type("Land"), 0, 2))
    );
}

#[test]
fn targeted_and_non_hand_returns_are_not_selection_choices() {
    let targeted = canonical_rules(
        "Probe",
        "Instant",
        "{U}",
        "Return target creature to its owner's hand.",
    );
    assert!(
        !targeted[0].to_string().contains("choosePermanents"),
        "a declared target is not a resolution choice: {:#}",
        targeted[0]
    );
    rejected(
        "Instant",
        "Return a land you control to the top of its owner's library.",
    );
    rejected(
        "Instant",
        "Return a creature card you control to its owner's hand.",
    );
}
