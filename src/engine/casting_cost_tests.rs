use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::super::*;
use super::{finalize_casting_mana_components, remove_generic_mana_from_components};

fn card_with_alternative(alternative_mana_cost: Option<&str>) -> CardDefinition {
    let mut costs = Vec::new();
    if let Some(mana_cost) = alternative_mana_cost {
        costs.push(json!({ "kind": "payMana", "manaCost": mana_cost }));
    }
    CardDefinition {
        id: "x-spell".to_string(),
        name: "X Spell".to_string(),
        type_line: "Sorcery".to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{X}{U}".to_string(),
        power: None,
        toughness: None,
        rules: vec![json!({
            "kind": "keywordAbility",
            "source": { "kind": "self" },
            "ability": { "kind": "alternativeCost", "costs": costs },
        })],
    }
}

fn engine_with_card(card: CardDefinition) -> GameEngine {
    let mut engine = GameEngine::new(
        GameSetup {
            players: vec![
                PlayerDeck {
                    id: "player-0".to_string(),
                    name: "Player 0".to_string(),
                    starting_life: 20,
                    cards: vec![card.clone()],
                },
                PlayerDeck {
                    id: "player-1".to_string(),
                    name: "Player 1".to_string(),
                    starting_life: 20,
                    cards: vec![card],
                },
            ],
            opening_hand_size: 0,
            starting_player: 0,
        },
        1073,
    )
    .expect("test setup is valid");
    engine.state.players[0].mana_pool = vec![
        FloatingMana {
            symbol: "C".to_string(),
            spend_restriction: None,
        };
        4
    ];
    engine
}

fn decisions(use_alternative: bool, x_value: i64) -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "useAlternativeCost".to_string(),
            Value::Bool(use_alternative),
        ),
        ("xValue".to_string(), Value::from(x_value)),
    ])
}

fn select_alternative(
    card: &CardDefinition,
    use_alternative: bool,
    x_value: i64,
) -> BTreeMap<String, Value> {
    let mut selected = decisions(use_alternative, x_value);
    if use_alternative {
        selected.insert(
            "effectiveAlternativeCost".to_string(),
            card.rules[0]["ability"].clone(),
        );
    }
    selected
}

#[test]
fn free_alternative_cost_allows_only_x_zero() {
    let card = card_with_alternative(None);
    let engine = engine_with_card(card.clone());

    assert!(engine.declared_x_is_legal_for_selected_casting_cost(
        &card,
        "player-0",
        &select_alternative(&card, true, 0),
    ));
    assert!(!engine.declared_x_is_legal_for_selected_casting_cost(
        &card,
        "player-0",
        &select_alternative(&card, true, 3),
    ));
    let declarations = engine
        .declarations_for_card(&card, "player-0")
        .expect("the generic spell has declarations");
    assert!(declarations.iter().any(|declaration| {
        declaration.decisions.get("useAlternativeCost") == Some(&Value::Bool(true))
            && declaration.decisions.get("xValue") == Some(&Value::from(0))
    }));
    assert!(!declarations.iter().any(|declaration| {
        declaration.decisions.get("useAlternativeCost") == Some(&Value::Bool(true))
            && declaration
                .decisions
                .get("xValue")
                .and_then(Value::as_i64)
                .is_some_and(|value| value > 0)
    }));
}

#[test]
fn normal_mana_cost_preserves_nonzero_x() {
    let card = card_with_alternative(None);
    let engine = engine_with_card(card.clone());

    assert!(engine.declared_x_is_legal_for_selected_casting_cost(
        &card,
        "player-0",
        &decisions(false, 3),
    ));
    let declarations = engine
        .declarations_for_card(&card, "player-0")
        .expect("the normal X cost has declarations");
    assert!(declarations.iter().any(|declaration| {
        declaration.decisions.get("useAlternativeCost") == Some(&Value::Bool(false))
            && declaration.decisions.get("xValue") == Some(&Value::from(3))
    }));
}

#[test]
fn alternative_mana_cost_containing_x_preserves_and_resolves_x() {
    let card = card_with_alternative(Some("{X}{B}"));
    let engine = engine_with_card(card.clone());
    let decisions = select_alternative(&card, true, 3);

    assert!(engine.declared_x_is_legal_for_selected_casting_cost(&card, "player-0", &decisions,));
    let mana_costs = engine.casting_mana_costs(&card, "player-0", &decisions);
    assert!(mana_costs.iter().any(|cost| cost == "{3}{B}"));
    assert!(mana_costs.iter().all(|cost| !cost.contains("{X}")));
    let declarations = engine
        .declarations_for_card(&card, "player-0")
        .expect("the alternative X cost has declarations");
    assert!(declarations.iter().any(|declaration| {
        declaration.decisions.get("useAlternativeCost") == Some(&Value::Bool(true))
            && declaration.decisions.get("xValue") == Some(&Value::from(3))
    }));
}

#[test]
fn selected_free_alternative_does_not_borrow_x_from_an_unselected_alternative() {
    let mut card = card_with_alternative(None);
    card.rules.push(json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": {
            "kind": "alternativeCost",
            "costs": [{ "kind": "payMana", "manaCost": "{X}{B}" }],
        },
    }));
    let engine = engine_with_card(card.clone());
    let decisions = select_alternative(&card, true, 3);

    assert!(!engine.declared_x_is_legal_for_selected_casting_cost(&card, "player-0", &decisions,));
}

#[test]
fn actually_paid_additional_mana_cost_containing_x_preserves_x() {
    let mut card = card_with_alternative(None);
    card.rules.push(json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": {
            "kind": "castingDeclaration",
            "additionalCosts": [{ "kind": "payMana", "manaCost": "{X}" }],
        },
        "effects": [],
    }));
    let engine = engine_with_card(card.clone());
    let decisions = select_alternative(&card, true, 3);

    assert!(engine.declared_x_is_legal_for_selected_casting_cost(&card, "player-0", &decisions,));
    assert!(
        engine
            .casting_mana_costs(&card, "player-0", &decisions)
            .iter()
            .any(|cost| cost == "{3}")
    );
}

#[test]
fn total_cost_reduction_carries_from_base_into_increase() {
    assert_eq!(
        finalize_casting_mana_components(vec!["{1}".to_string(), "{1}".to_string()], 0, 2, 0,),
        vec!["".to_string(), "".to_string()],
    );
}

#[test]
fn generic_reduction_preserves_colored_and_colorless_symbols() {
    assert_eq!(
        finalize_casting_mana_components(vec!["{1}{U}{C}".to_string(), "{2}".to_string()], 0, 3, 0,),
        vec!["{U}{C}".to_string(), "".to_string()],
    );
}

#[test]
fn x_is_resolved_before_total_reduction() {
    assert_eq!(
        finalize_casting_mana_components(vec!["{X}{U}".to_string(), "{X}".to_string()], 2, 3, 0,),
        vec!["{U}".to_string(), "{1}".to_string()],
    );
}

#[test]
fn minimum_mana_is_applied_after_increases_and_reductions() {
    assert_eq!(
        finalize_casting_mana_components(vec!["{1}".to_string(), "{1}".to_string()], 0, 2, 3,),
        vec!["".to_string(), "".to_string(), "{3}".to_string()],
    );
}

#[test]
fn delve_substitutes_payment_after_the_minimum_mana_floor() {
    let mut costs = finalize_casting_mana_components(vec!["{1}".to_string()], 0, 0, 3);
    assert_eq!(costs, vec!["{1}".to_string(), "{2}".to_string()]);

    remove_generic_mana_from_components(&mut costs, 2);

    assert_eq!(costs, vec!["".to_string(), "{1}".to_string()]);
    assert_eq!(
        costs.iter().fold(0_i32, |total, cost| {
            total.saturating_add(mana_cost_mana_amount(cost))
        }),
        1,
    );
}

#[test]
fn delve_without_selected_cards_does_not_remove_generic_mana() {
    let mut card = card_with_alternative(None);
    card.mana_cost = "{2}{U}".to_string();
    card.rules.push(json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "delve" },
    }));
    let engine = engine_with_card(card.clone());

    let costs = engine.casting_mana_costs(&card, "player-0", &BTreeMap::new());

    assert!(costs.iter().any(|cost| cost == "{2}{U}"));
    assert_eq!(
        costs.iter().fold(0_i32, |total, cost| {
            total.saturating_add(mana_cost_generic_amount(cost))
        }),
        2,
    );
}

#[test]
fn selected_alternative_cost_receives_static_tax() {
    let card = card_with_alternative(Some("{2}"));
    let mut engine = engine_with_card(card.clone());
    let tax_definition = CardDefinition {
        id: "spell-tax".to_string(),
        name: "Spell Tax".to_string(),
        type_line: "Artifact".to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{1}".to_string(),
        power: None,
        toughness: None,
        rules: vec![json!({
            "kind": "staticAbility",
            "source": { "kind": "self" },
            "activeWhile": {
                "kind": "inZone",
                "object": { "kind": "self" },
                "zone": { "kind": "battlefield" },
            },
            "modifiers": [{
                "kind": "additionalCastingCost",
                "players": { "kind": "eachPlayer" },
                "where": { "kind": "cardTypeContains", "value": "Sorcery" },
                "cost": { "kind": "payMana", "manaCost": "{1}" },
            }],
        })],
    };
    let mut tax = card_instance("player-1", &tax_definition, 0);
    tax.instance_id = "spell-tax-instance".to_string();
    engine.state.players[1].battlefield.push(tax);

    let costs = engine.casting_mana_costs(&card, "player-0", &select_alternative(&card, true, 0));

    assert_eq!(
        costs
            .iter()
            .map(|cost| mana_cost_mana_amount(cost))
            .sum::<i32>(),
        3,
    );
    assert!(costs.iter().any(|cost| cost == "{2}"));
    assert!(costs.iter().any(|cost| cost == "{1}"));
    assert!(costs.iter().all(|cost| !cost.contains("{U}")));
}
