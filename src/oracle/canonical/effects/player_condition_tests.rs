//! Player-state conditions through real cards: one player subject and one
//! measured proposition, read by every condition parent (static `as long
//! as`, intervening `if`, sentence `if`, activation restriction, cost
//! reduction, entry exception) and lowered to the same condition IR.
use serde_json::{Value, json};

use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(type_line: &str, mana_cost: &str, text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: "Test Card".to_string(),
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

/// The canonical rule of the ability at `index`.
fn rule(type_line: &str, mana_cost: &str, text: &str, index: usize) -> Value {
    let parsed = parse(type_line, mana_cost, text);
    let ability = &parsed.abilities[index];
    assert_eq!(
        ability.status,
        "canonical",
        "{text}: {:?}",
        ability
            .diagnostics
            .first()
            .map(|d| (&d.state, &d.expected, &d.found))
    );
    ability.rule.clone().expect("canonical rule")
}

fn failure_state(type_line: &str, text: &str, index: usize) -> String {
    let parsed = parse(type_line, "{1}", text);
    let ability = &parsed.abilities[index];
    assert_ne!(ability.status, "canonical", "{text}");
    ability
        .diagnostics
        .first()
        .and_then(|diagnostic| diagnostic.state.clone())
        .unwrap_or_default()
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn quantified_player() -> Value {
    json!({ "kind": "quantifiedPlayer" })
}

fn opponents() -> Value {
    json!({ "kind": "opponentsOf", "player": controller() })
}

fn compare(operator: &str, left: Value, right: i64) -> Value {
    json!({ "kind": "compare", "operator": operator, "left": left,
        "right": { "kind": "integer", "value": right } })
}

fn some(players: Value, condition: Value) -> Value {
    json!({ "kind": "playerQuantifier", "quantifier": "some", "players": players,
        "condition": condition })
}

fn quantified_life_at_most(value: i64) -> Value {
    compare(
        "<=",
        json!({ "kind": "lifeTotal", "player": quantified_player() }),
        value,
    )
}

#[test]
fn static_conditions_quantify_each_player_measure() {
    // Ruthless Cullblade: life total.
    let static_rule = rule(
        "Creature - Vampire Warrior",
        "{1}{B}",
        "This creature gets +2/+1 as long as an opponent has 10 or less life.",
        0,
    );
    assert_eq!(
        static_rule["condition"],
        some(opponents(), quantified_life_at_most(10))
    );
    // Wu Admiral: controlled permanents through the shared object query.
    let static_rule = rule(
        "Creature - Human Soldier",
        "{4}{U}",
        "This creature gets +1/+1 as long as an opponent controls an Island.",
        0,
    );
    assert_eq!(
        static_rule["condition"],
        some(
            opponents(),
            compare(
                ">=",
                json!({ "kind": "countPermanents", "player": quantified_player(),
                    "where": { "kind": "subtypeContains", "value": "Island" },
                    "excludeSource": false }),
                1
            )
        )
    );
    // Kavu Runner: `no opponent` negates the existential.
    let static_rule = rule(
        "Creature - Kavu",
        "{3}{R}",
        "This creature has haste as long as no opponent controls a white or blue creature.",
        0,
    );
    assert_eq!(static_rule["condition"]["kind"], "not");
    assert_eq!(
        static_rule["condition"]["operand"]["quantifier"],
        json!("some")
    );
    // Guul Draz Specter: cards in a zone, with the bare `in hand` zone.
    let static_rule = rule(
        "Creature - Specter",
        "{2}{B}{B}",
        "This creature gets +3/+3 as long as an opponent has no cards in hand.",
        0,
    );
    assert_eq!(
        static_rule["condition"]["condition"],
        compare(
            "<=",
            json!({ "kind": "countCards",
                "zone": { "kind": "hand", "player": quantified_player() },
                "where": Value::Null }),
            0
        )
    );
}

#[test]
fn every_condition_parent_reaches_the_same_player_atoms() {
    // Vivisection Evangelist: intervening `if` over player counters (CR 603.4).
    let trigger = rule(
        "Creature - Phyrexian Cleric",
        "{3}{W}{B}",
        "When this creature enters, if an opponent has three or more poison counters, destroy target creature or planeswalker an opponent controls.",
        0,
    );
    assert_eq!(
        trigger["condition"],
        some(
            opponents(),
            compare(
                ">=",
                json!({ "kind": "countPlayerCounters", "player": quantified_player(),
                    "counter": "poison" }),
                3
            )
        )
    );
    // Into the Story: the cost-reduction parent, with `their` graveyard.
    let reduction = rule(
        "Instant",
        "{5}{U}{U}",
        "This spell costs {3} less to cast if an opponent has seven or more cards in their graveyard.",
        0,
    );
    assert_eq!(
        reduction["modifiers"][0]["condition"]["condition"]["left"]["zone"],
        json!({ "kind": "graveyard", "player": quantified_player() })
    );
    // Strangled Cemetery: an entry exception over every player.
    let entry = rule(
        "Land",
        "",
        "This land enters tapped unless a player has 13 or less life.",
        0,
    );
    assert_eq!(
        entry["condition"],
        json!({ "kind": "not",
            "operand": some(json!({ "kind": "eachPlayer" }), quantified_life_at_most(13)) })
    );
    assert_eq!(entry["replacement"][0]["kind"], "setEnteringState");
}

#[test]
fn the_controller_subject_shares_measures_without_a_quantifier() {
    // Cutthroat il-Dal and Leapfrog.
    let hellbent = rule(
        "Creature - Human Rogue",
        "{3}{B}",
        "This creature has shadow as long as you have no cards in hand.",
        0,
    );
    assert_eq!(
        hellbent["condition"],
        compare(
            "<=",
            json!({ "kind": "countCards", "zone": { "kind": "hand", "player": controller() },
                "where": Value::Null }),
            0
        )
    );
    let history = rule(
        "Creature - Frog",
        "{2}{U}",
        "This creature has flying as long as you've cast an instant or sorcery spell this turn.",
        0,
    );
    assert_eq!(history["condition"]["kind"], "hasCastSpellThisTurn");
    assert_eq!(history["condition"]["player"], controller());
}

#[test]
fn an_unused_combination_needs_no_new_production() {
    // Each player, cards drawn this turn, intervening `if` of a step trigger.
    let trigger = rule(
        "Enchantment",
        "{2}{U}",
        "At the beginning of each end step, if each player has drawn two or more cards this turn, you gain 2 life.",
        0,
    );
    assert_eq!(
        trigger["condition"],
        json!({ "kind": "playerQuantifier", "quantifier": "every",
            "players": { "kind": "eachPlayer" },
            "condition": compare(">=", json!({ "kind": "countEventsThisTurn",
                "event": "cardDrawn", "player": quantified_player(), "where": Value::Null }), 2) })
    );
    // An opponent's filtered spell history gating a static grant.
    let static_rule = rule(
        "Creature - Test",
        "{1}{R}",
        "This creature gets +2/+0 as long as an opponent has cast a noncreature spell this turn.",
        0,
    );
    assert_eq!(
        static_rule["condition"]["condition"]["kind"],
        "hasCastSpellThisTurn"
    );
}

#[test]
fn numeric_comparison_reads_the_spell_x() {
    // Martial Coup's condition with an unused effect.
    let effects = rule(
        "Sorcery",
        "{X}{U}",
        "Draw X cards. If X is 5 or more, you gain 5 life.",
        0,
    )["effects"]
        .clone();
    let conditional = effects
        .as_array()
        .and_then(|effects| {
            effects
                .iter()
                .find(|effect| effect.get("condition").is_some())
        })
        .expect("a conditional effect");
    assert_eq!(conditional["condition"]["operator"], ">=");
    assert_eq!(conditional["condition"]["right"]["value"], 5);
}

#[test]
fn near_misses_fail_explicitly() {
    assert_eq!(
        failure_state(
            "Creature - Test",
            "This creature gets +1/+1 as long as an opponent has eight or more cards in your graveyard.",
            0
        ),
        "condition.player.cards.possessive"
    );
    assert_eq!(
        failure_state(
            "Creature - Test",
            "This creature gets +1/+1 as long as an opponent has cast two or more noncreature spells this turn.",
            0
        ),
        "condition.player.cast.filtered-count"
    );
}

#[test]
fn collective_opponents_and_turn_ownership_join_the_shared_static_atoms() {
    // Turbulent Crater-style exception: `your opponents` counted together.
    let entry = rule(
        "Land",
        "",
        "This land enters tapped unless your opponents control eight or more lands.",
        0,
    );
    assert_eq!(
        entry["condition"],
        json!({ "kind": "not", "operand": compare(
            ">=",
            json!({ "kind": "countPermanentsControlledByOpponents", "player": controller(),
                "where": { "kind": "cardTypeContains", "value": "Land" } }),
            8
        ) })
    );
    // Sardian Cliffstomper: turn ownership and a controlled count in one
    // static program.
    let static_rule = rule(
        "Creature - Human Warrior",
        "{1}{R}",
        "As long as it's your turn and you control four or more Mountains, this creature gets +3/+0.",
        0,
    );
    assert_eq!(static_rule["condition"]["kind"], "and");
    assert_eq!(
        static_rule["condition"]["operands"][0],
        json!({ "kind": "duringControllerTurn", "player": controller() })
    );
    // `your opponents` takes a plural verb; a possession has no collective reading.
    assert_eq!(
        failure_state(
            "Creature - Test",
            "This creature gets +1/+1 as long as your opponents have ten or more cards in hand.",
            0
        ),
        "condition.static.atom-kind"
    );
}
