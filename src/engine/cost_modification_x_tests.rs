//! Runtime proofs of the cost-modification frame (CR 601.2f, 118.7) and of
//! its next-spell and own-target-count consumers. Each test names the
//! semantic risk it guards; parser shapes are proved by the
//! `cost-modification-x-batch9` fixtures.
use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::super::*;
use super::{
    ColoredCastingReduction, apply_colored_casting_reductions, colored_casting_reductions,
    colored_reduction_supported, finalize_casting_mana_components, own_casting_increase_supported,
    own_target_count_increases, remove_generic_mana_from_components,
};

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    CardDefinition {
        id: id.into(),
        name: id.into(),
        type_line: type_line.into(),
        mana_cost: String::new(),
        power: Some("1".into()),
        toughness: Some("1".into()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules,
    }
}

fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant", Vec::new()); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        9109,
    )
    .unwrap()
}

fn colored(symbol: &str, amount: i32, excess_reduces_generic: bool) -> ColoredCastingReduction {
    ColoredCastingReduction {
        symbol: symbol.into(),
        amount,
        excess_reduces_generic,
    }
}

/// CR 118.7c: a colored reduction removes that color first; its excess
/// reduces generic mana. Without the excess, a `{W}{B}` reduction of a
/// `{1}{W}` spell would charge `{1}` instead of `{0}`.
#[test]
fn colored_reduction_excess_reduces_generic_by_default() {
    let mut costs = vec!["{1}{W}".to_string()];
    let excess = apply_colored_casting_reductions(
        &mut costs,
        &[colored("W", 1, true), colored("B", 1, true)],
    )
    .unwrap();
    assert_eq!(costs, vec!["{1}".to_string()]);
    assert_eq!(excess, 1);
    assert_eq!(
        finalize_casting_mana_components(costs, 0, excess, 0),
        vec!["".to_string()]
    );
}

/// `This effect reduces only the amount of colored mana you pay`: the
/// excess is discarded (Edgewalker's own example: `{1}{W}` costs `{1}`).
#[test]
fn colored_only_reduction_keeps_the_generic_component() {
    let mut costs = vec!["{1}{W}".to_string()];
    let excess = apply_colored_casting_reductions(
        &mut costs,
        &[colored("W", 1, false), colored("B", 1, false)],
    )
    .unwrap();
    assert_eq!(costs, vec!["{1}".to_string()]);
    assert_eq!(excess, 0);
}

/// A hybrid or Phyrexian symbol of the reduced color has no proved rule
/// here; the reduction is refused instead of guessed.
#[test]
fn colored_reduction_refuses_a_hybrid_symbol_of_its_color() {
    let mut costs = vec!["{G/W}{G}".to_string()];
    assert!(apply_colored_casting_reductions(&mut costs, &[colored("G", 1, true)]).is_err());
    let mut unrelated = vec!["{U/B}{G}".to_string()];
    assert_eq!(
        apply_colored_casting_reductions(&mut unrelated, &[colored("G", 1, true)]).unwrap(),
        0
    );
    assert_eq!(unrelated, vec!["{U/B}".to_string()]);
}

/// CR 601.2f: increases and reductions (colored, then generic), then the
/// floor; Delve substitutes payment only after the total is locked.
#[test]
fn colored_and_generic_reductions_precede_the_floor_and_delve() {
    // Base {3}{G}{G} reduced by {G} three times (excess 1 to generic) and
    // {1} generic, with a floor of three mana.
    let mut costs = vec!["{3}{G}{G}".to_string()];
    let reductions = colored_casting_reductions(
        &json!({ "symbols": ["G"], "times": { "kind": "integer", "value": 3 }, "excessReducesGeneric": true }),
        3,
    )
    .unwrap();
    let excess = apply_colored_casting_reductions(&mut costs, &reductions).unwrap();
    assert_eq!(excess, 1);
    let mut finalized = finalize_casting_mana_components(costs, 0, 1 + excess, 3);
    // {3} - 2 = {1}; one mana is below the floor of three.
    assert_eq!(finalized, vec!["{1}".to_string(), "{2}".to_string()]);
    remove_generic_mana_from_components(&mut finalized, 2);
    assert_eq!(finalized, vec!["".to_string(), "{1}".to_string()]);
}

/// The `coloredMana` field is a closed shape; an unknown symbol, field or
/// excess policy is refused so no reduction is silently dropped.
#[test]
fn colored_reduction_shape_is_closed() {
    let valid = json!({ "symbols": ["W", "B"], "times": { "kind": "integer", "value": 1 }, "excessReducesGeneric": false });
    assert!(colored_reduction_supported(&valid));
    let mut unknown_symbol = valid.clone();
    unknown_symbol["symbols"] = json!(["W/B"]);
    assert!(!colored_reduction_supported(&unknown_symbol));
    let mut extra = valid.clone();
    extra["hybrid"] = json!(true);
    assert!(!colored_reduction_supported(&extra));
    let mut no_policy = valid;
    no_policy
        .as_object_mut()
        .unwrap()
        .remove("excessReducesGeneric");
    assert!(!colored_reduction_supported(&no_policy));
}

fn strive_spell() -> CardDefinition {
    definition(
        "strive-spell",
        "Instant",
        vec![json!({
            "kind": "staticAbility",
            "source": { "kind": "self" },
            "activeWhile": { "kind": "inZone", "object": { "kind": "self" }, "zone": { "kind": "stackOrCast" } },
            "modifiers": [{ "kind": "increaseOwnCastingCost", "manaCost": "{2}{G}", "per": "targetBeyondFirst" }],
        })],
    )
}

/// Strive (CR 702.108): one copy of the cost for each target after the
/// first, counted on the declared targets (CR 601.2c before 601.2f).
#[test]
fn own_increase_counts_targets_beyond_the_first() {
    let card = strive_spell();
    assert!(own_casting_increase_supported(
        &card.rules[0]["modifiers"][0]
    ));
    let target = |id: &str| TargetRef::Permanent {
        instance_id: id.into(),
    };
    assert!(own_target_count_increases(&card, &BTreeMap::new()).is_empty());
    let one = BTreeMap::from([("t1".to_string(), target("a"))]);
    assert!(own_target_count_increases(&card, &one).is_empty());
    let three = BTreeMap::from([
        ("t1".to_string(), target("a")),
        ("t2".to_string(), target("b")),
        ("t3".to_string(), target("c")),
    ]);
    assert_eq!(
        own_target_count_increases(&card, &three),
        vec!["{2}{G}".to_string(), "{2}{G}".to_string()]
    );
    // A unit the runtime does not count is not an executable increase.
    let mut unknown = card.rules[0]["modifiers"][0].clone();
    unknown["per"] = json!("modeBeyondFirst");
    assert!(!own_casting_increase_supported(&unknown));
}

fn next_spell_reduction(amount: Value) -> Value {
    json!({
        "kind": "installOneShotModifier",
        "capture": { "player": { "kind": "abilityController" } },
        "expires": { "kind": "endOfCurrentTurn" },
        "match": { "kind": "spellCast", "player": { "kind": "capturedValue", "name": "player" } },
        "apply": { "kind": "reduceCastingCost", "amount": amount },
        "consume": { "kind": "firstMatchingEvent" }
    })
}

fn stack_object(source: &CardInstance, decisions: BTreeMap<String, Value>) -> StackObject {
    StackObject {
        id: "stack:next-spell".into(),
        controller: "player-0".into(),
        card: source.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".into()),
        ability_rule: None,
        decisions,
        targets: BTreeMap::new(),
    }
}

/// `the next spell you cast this turn costs {N} less`: read while the cost
/// is determined, then consumed once by that cast; a later spell pays full.
#[test]
fn next_spell_reduction_applies_to_one_cast_and_is_consumed() {
    let mut e = engine();
    let effect = next_spell_reduction(json!({ "kind": "integer", "value": 2 }));
    assert!(super::super::one_shot_modifier::install_one_shot_modifier_supported(&effect));
    let source = card_instance(
        "player-0",
        &definition("berserker", "Creature", Vec::new()),
        900,
    );
    e.install_one_shot_modifier(
        &effect,
        &stack_object(&source, BTreeMap::new()),
        &BTreeMap::new(),
    );
    let spell = definition("spell", "Instant", Vec::new());
    assert_eq!(e.one_shot_casting_cost_reduction("player-1", &spell), 0);
    assert_eq!(e.one_shot_casting_cost_reduction("player-0", &spell), 2);
    let operations = e.consume_one_shot_spell_cast_modifier("player-0", &spell);
    assert_eq!(operations.len(), 1);
    assert_eq!(e.one_shot_casting_cost_reduction("player-0", &spell), 0);
}

/// The amount is a snapshot taken when the rule is made (the X bound as
/// the ability resolved), and two pending rules both name the same next
/// spell: one cast consumes both.
#[test]
fn next_spell_reduction_snapshots_x_and_every_pending_rule_names_the_cast() {
    let mut e = engine();
    let source = card_instance("player-0", &definition("muse", "Creature", Vec::new()), 901);
    let decisions = BTreeMap::from([("xValue".to_string(), Value::from(3))]);
    e.install_one_shot_modifier(
        &next_spell_reduction(json!({ "kind": "decisionResult", "decisionId": "xValue" })),
        &stack_object(&source, decisions),
        &BTreeMap::new(),
    );
    e.install_one_shot_modifier(
        &next_spell_reduction(json!({ "kind": "integer", "value": 1 })),
        &stack_object(&source, BTreeMap::new()),
        &BTreeMap::new(),
    );
    let spell = definition("spell", "Sorcery", Vec::new());
    assert_eq!(e.one_shot_casting_cost_reduction("player-0", &spell), 4);
    assert_eq!(
        e.consume_one_shot_spell_cast_modifier("player-0", &spell)
            .len(),
        2
    );
    assert_eq!(e.one_shot_casting_cost_reduction("player-0", &spell), 0);
}
