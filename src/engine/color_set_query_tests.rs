use super::*;
use crate::engine::mana_projection_context::ExecutableManaOutputs;
use crate::engine::mana_projection_contract::NoManaReason;

fn engine() -> GameEngine {
    let filler: CardDefinition =
        serde_json::from_value(json!({"id":"filler","name":"filler","typeLine":"Land","rules":[]}))
            .unwrap();
    GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player{i}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 4],
                })
                .collect(),
        },
        632,
    )
    .unwrap()
}

fn permanent(
    e: &mut GameEngine,
    owner: usize,
    id: &str,
    mana: &str,
    rules: Vec<Value>,
) -> CardInstance {
    let definition: CardDefinition = serde_json::from_value(json!({
        "id":id,"name":id,"typeLine":"Creature","manaCost":mana,
        "power":"1","toughness":"1","rules":rules,
    }))
    .unwrap();
    let mut card = card_instance(&format!("player-{owner}"), &definition, 0);
    card.instance_id = id.to_owned();
    e.state.players[owner].battlefield.push(card.clone());
    card
}

fn query() -> Value {
    json!({"kind":"eachColorAmongPermanents","player":{"kind":"controllerOf","object":{"kind":"self"}}})
}

fn static_rule(modifier: Value) -> Value {
    json!({"kind":"staticAbility","source":{"kind":"self"},
        "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
        "modifiers":[modifier]})
}

#[test]
fn colors_union_indicators_and_cost_but_exclude_identity_and_colorless() {
    let mut e = engine();
    permanent(&mut e, 0, "whiteblue", "{W}{U}", vec![]);
    permanent(&mut e, 0, "duplicate", "{W}", vec![]);
    permanent(
        &mut e,
        0,
        "indicator",
        "",
        vec![json!({"kind":"rulesMarker","colorIndicator":["G"]})],
    );
    permanent(
        &mut e,
        0,
        "identity-only",
        "{C}",
        vec![json!({"kind":"rulesMarker","colorIdentity":["R","B"]})],
    );
    let colors = e.checked_color_set_query(&query(), "player-0").unwrap();
    assert_eq!(colors.symbols(), ["W", "U", "G"]);
    assert_eq!(colors.cardinality(), 3);
}

#[test]
fn phased_objects_and_opponents_are_not_counted() {
    let mut e = engine();
    permanent(&mut e, 0, "own", "{G}", vec![]);
    permanent(&mut e, 0, "phased", "{R}", vec![]);
    e.state.players[0].battlefield[1]
        .flags
        .insert("phasedOut".into(), true);
    permanent(&mut e, 1, "opponent", "{U}", vec![]);
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap()
            .symbols(),
        ["G"]
    );
}

#[test]
fn current_control_changes_membership_without_rebinding_ability_actor() {
    let mut e = engine();
    let source = permanent(&mut e, 0, "source", "{W}", vec![]);
    let blue = permanent(&mut e, 1, "blue", "{U}", vec![]);
    e.change_permanent_controller(&blue.instance_id, "player-0")
        .unwrap();
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap()
            .symbols(),
        ["W", "U"]
    );
    e.change_permanent_controller(&source.instance_id, "player-1")
        .unwrap();
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap()
            .symbols(),
        ["U"]
    );
}

#[test]
fn replacement_colors_and_devoid_observe_five_color_domain() {
    let mut e = engine();
    permanent(
        &mut e,
        0,
        "recolored",
        "{R}",
        vec![static_rule(
            json!({"kind":"setColor","objects":{"kind":"self"},"colors":["White"]}),
        )],
    );
    permanent(
        &mut e,
        0,
        "devoid",
        "{U}",
        vec![json!({"kind":"keywordAbility","ability":{"kind":"devoid"}})],
    );
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap()
            .symbols(),
        ["W"]
    );
}

#[test]
fn temporary_colorless_and_all_colors_are_not_identity_or_sixth_color() {
    let mut e = engine();
    permanent(
        &mut e,
        0,
        "all",
        "",
        vec![static_rule(
            json!({"kind":"setAllColors","objects":{"kind":"self"}}),
        )],
    );
    let red = permanent(&mut e, 0, "temporary", "{R}", vec![]);
    e.state.rule_modifiers.push(json!({"kind":"temporaryColors","cardInstanceId":red.instance_id,"colors":[],"expiresAfterTurn":e.state.turn_number}));
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap()
            .symbols(),
        ["W", "U", "B", "R", "G"]
    );
    assert!(
        e.checked_permanent_five_colors(&red)
            .unwrap()
            .symbols()
            .is_empty()
    );
}

#[test]
fn layer_six_ability_loss_does_not_erase_started_layer_five_color() {
    let mut e = engine();
    let card = permanent(
        &mut e,
        0,
        "color-source",
        "{R}",
        vec![static_rule(
            json!({"kind":"setColor","objects":{"kind":"self"},"colors":["G"]}),
        )],
    );
    e.state
        .rule_modifiers
        .push(json!({"kind":"loseAllAbilities","cardInstanceId":card.instance_id}));
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap()
            .symbols(),
        ["G"]
    );
}

#[test]
fn unrelated_pt_condition_is_never_evaluated_by_color_reader() {
    let mut e = engine();
    let mut pt = static_rule(
        json!({"kind":"modifyPowerToughness","objects":{"kind":"self"},"power":query(),"toughness":query()}),
    );
    pt["condition"] = json!({"kind":"compare","left":{"kind":"powerOf","object":{"kind":"self"}},"operator":">","right":{"kind":"integer","value":0}});
    assert!(battlefield_static_rule_supported(&pt));
    permanent(&mut e, 0, "pt-source", "{G}", vec![pt]);
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap()
            .symbols(),
        ["G"]
    );
}

#[test]
fn genuine_color_pt_dependency_is_explicit_error_not_zero_or_printed_fallback() {
    let mut e = engine();
    let mut color =
        static_rule(json!({"kind":"setColor","objects":{"kind":"self"},"colors":["U"]}));
    color["condition"] = json!({"kind":"compare","left":{"kind":"powerOf","object":{"kind":"self"}},"operator":">","right":{"kind":"integer","value":0}});
    assert!(battlefield_static_rule_supported(&color));
    permanent(&mut e, 0, "dependent", "{R}", vec![color]);
    let error = e.checked_color_set_query(&query(), "player-0").unwrap_err();
    assert_eq!(error.state, "color.query.characteristic-dependency");
}

#[test]
fn unresolved_color_layer_order_is_reported() {
    let mut e = engine();
    permanent(
        &mut e,
        0,
        "conflict",
        "{R}",
        vec![
            static_rule(json!({"kind":"setColor","objects":{"kind":"self"},"colors":["U"]})),
            static_rule(json!({"kind":"setAllColors","objects":{"kind":"self"}})),
        ],
    );
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap_err()
            .state,
        "color.query.layer-order"
    );
}

#[test]
fn closed_source_flag_condition_uses_shared_condition_evaluator() {
    let mut e = engine();
    let mut color =
        static_rule(json!({"kind":"setColor","objects":{"kind":"self"},"colors":["U"]}));
    color["condition"] = json!({"kind":"sourceFlagSet","flag":"colorEnabled"});
    permanent(&mut e, 0, "conditional", "{R}", vec![color]);
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap()
            .symbols(),
        ["R"]
    );
    e.state.players[0].battlefield[0]
        .flags
        .insert("colorEnabled".into(), true);
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap()
            .symbols(),
        ["U"]
    );
}

#[test]
fn query_player_and_filter_context_are_checked() {
    let e = engine();
    assert_eq!(
        e.checked_color_set_query(&query(), "missing")
            .unwrap_err()
            .state,
        "color.query.player-context"
    );
    let mut bad = query();
    bad["player"] = json!({"kind":"eachPlayer"});
    assert_eq!(
        e.checked_color_set_query(&bad, "player-0")
            .unwrap_err()
            .state,
        "color.query.player-context"
    );
    bad = query();
    bad["where"] = json!({"kind":"cardTypeContains","value":"Creature"});
    assert_eq!(
        e.checked_color_set_query(&bad, "player-0")
            .unwrap_err()
            .state,
        "color.query.filter-domain"
    );
}

#[test]
fn checked_mana_produces_one_mixed_combination_with_literal_quantity() {
    let mut e = engine();
    let source = permanent(&mut e, 0, "producer", "{G}", vec![]);
    permanent(&mut e, 0, "white", "{W}", vec![]);
    let mut value = query();
    value["amount"] = json!({"kind":"integer","value":2});
    assert_eq!(
        e.try_mana_options_for_value(&value, "player-0", Some(&source))
            .unwrap(),
        ExecutableManaOutputs::Choices(vec![vec!["W".into(), "W".into(), "G".into(), "G".into()]])
    );
}

#[test]
fn checked_mana_colorless_is_no_mana_and_dependencies_still_error() {
    let mut e = engine();
    let source = permanent(&mut e, 0, "colorless", "{C}", vec![]);
    assert_eq!(
        e.try_mana_options_for_value(&query(), "player-0", Some(&source))
            .unwrap(),
        ExecutableManaOutputs::NoMana(NoManaReason::UndefinedType)
    );
    let mut color =
        static_rule(json!({"kind":"setColor","objects":{"kind":"self"},"colors":["U"]}));
    color["condition"] = json!({"kind":"compare","left":{"kind":"powerOf","object":{"kind":"self"}},"operator":">","right":{"kind":"integer","value":0}});
    e.state.players[0].battlefield[0]
        .definition
        .rules
        .push(color);
    assert!(
        e.try_mana_options_for_value(&query(), "player-0", Some(&source))
            .is_err()
    );
}

#[test]
fn quantity_outside_closed_literal_domain_is_error() {
    let mut e = engine();
    let source = permanent(&mut e, 0, "producer", "{G}", vec![]);
    for amount in [
        json!(-1),
        json!(0),
        json!(11),
        json!({"kind":"decisionResult","decisionId":"missing"}),
    ] {
        let mut value = query();
        value["amount"] = amount;
        assert!(
            e.try_mana_options_for_value(&value, "player-0", Some(&source))
                .is_err()
        );
    }
}

#[test]
fn counterfactual_type_support_reuses_color_query_without_quantity_choices() {
    use crate::engine::mana_projection_context::ManaProjectionContext;
    use crate::engine::mana_projection_contract::{ManaType, ManaTypeProjection};
    let mut e = engine();
    let mut value = query();
    value["amount"] = json!(2);
    let mana = json!({"kind":"manaAbility","source":{"kind":"self"},
        "costs":[{"kind":"tap","object":{"kind":"self"}}],
        "effects":[{"kind":"addMana","player":{"kind":"controllerOf","object":{"kind":"self"}},"mana":value}]});
    assert!(rule_is_executable(&mana));
    let source = permanent(&mut e, 0, "color-land", "", vec![mana]);
    e.state.players[0].battlefield[0].definition.type_line = "Land".into();
    permanent(&mut e, 0, "white", "{W}", vec![]);
    permanent(&mut e, 0, "green", "{G}", vec![]);
    let context = ManaProjectionContext::new(&e).unwrap();
    assert_eq!(
        *context.types_for_source(&source.instance_id).unwrap(),
        ManaTypeProjection::Types(BTreeSet::from([ManaType::White, ManaType::Green]))
    );
}

#[test]
fn selector_controller_bindings_must_be_closed_before_matching() {
    let mut e = engine();
    let source = permanent(&mut e, 0, "source", "{R}", vec![]);
    for controller in [
        json!({"kind":"chosenTarget","id":"chosenPlayer"}),
        json!({"kind":"boundValue","id":"player"}),
        json!({"kind":"controllerOf","object":{"kind":"chosenTarget","id":"permanent"}}),
        json!({"kind":"abilityController"}),
    ] {
        let selector = json!({"kind":"permanents","controller":controller});
        assert_eq!(
            GameEngine::checked_color_selector_matches(&source, &selector, &source)
                .unwrap_err()
                .state,
            "color.query.selector-domain"
        );
        e.state.players[0].battlefield[0].definition.rules = vec![static_rule(
            json!({"kind":"setColor","objects":selector,"colors":["U"]}),
        )];
        assert_eq!(
            e.checked_color_set_query(&query(), "player-0")
                .unwrap_err()
                .state,
            "color.query.selector-domain"
        );
    }
}

#[test]
fn closed_static_controller_scopes_keep_shared_matcher_semantics() {
    let mut e = engine();
    let source = permanent(&mut e, 0, "source", "{R}", vec![]);
    let opponent = permanent(&mut e, 1, "opponent", "{G}", vec![]);
    let own =
        json!({"kind":"permanents","controller":{"kind":"controllerOf","object":{"kind":"self"}}});
    let others = json!({"kind":"permanents","controller":{"kind":"opponentsOf","player":{"kind":"controllerOf","object":{"kind":"self"}}}});
    let all = json!({"kind":"permanents"});
    assert!(GameEngine::checked_color_selector_matches(&source, &own, &source).unwrap());
    assert!(!GameEngine::checked_color_selector_matches(&source, &own, &opponent).unwrap());
    assert!(GameEngine::checked_color_selector_matches(&source, &others, &opponent).unwrap());
    assert!(!GameEngine::checked_color_selector_matches(&source, &others, &source).unwrap());
    assert!(GameEngine::checked_color_selector_matches(&source, &all, &opponent).unwrap());
}

#[test]
fn modifier_level_characteristic_dependency_is_not_ignored() {
    let mut e = engine();
    let modifier = json!({"kind":"setColor","objects":{"kind":"self"},"colors":["U"],
        "condition":{"kind":"compare","left":{"kind":"powerOf","object":{"kind":"self"}},
        "operator":">","right":{"kind":"integer","value":0}}});
    let rule = static_rule(modifier);
    assert!(battlefield_static_rule_supported(&rule));
    permanent(&mut e, 0, "conditional-color", "{R}", vec![rule]);
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap_err()
            .state,
        "color.query.characteristic-dependency"
    );
}

#[test]
fn modifier_level_closed_false_is_inactive_then_true_applies() {
    let mut e = engine();
    let modifier = json!({"kind":"setColor","objects":{"kind":"self"},"colors":["U"],
        "condition":{"kind":"sourceFlagSet","flag":"colorEnabled"}});
    permanent(
        &mut e,
        0,
        "conditional-color",
        "{R}",
        vec![static_rule(modifier)],
    );
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap()
            .symbols(),
        ["R"]
    );
    e.state.players[0].battlefield[0]
        .flags
        .insert("colorEnabled".into(), true);
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap()
            .symbols(),
        ["U"]
    );
}

#[test]
fn unsupported_active_color_rule_errors_but_proven_inactive_metadata_does_not() {
    let mut e = engine();
    let mut rule = static_rule(json!({"kind":"setColor","objects":{"kind":"self"},"colors":["U"]}));
    rule["modifiers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":"unimplementedStaticModifier"}));
    assert!(!battlefield_static_rule_supported(&rule));
    permanent(&mut e, 0, "active-unsupported", "{R}", vec![rule.clone()]);
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap_err()
            .state,
        "color.query.rule-domain"
    );
    rule["roomDoorIndex"] = json!(0);
    e.state.players[0].battlefield[0].definition.rules = vec![rule];
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap()
            .symbols(),
        ["R"]
    );
}

#[test]
fn wrong_zone_is_inactive_but_unknown_activity_is_error() {
    let mut e = engine();
    let mut rule = static_rule(json!({"kind":"setColor","objects":{"kind":"self"},"colors":["U"]}));
    rule["activeWhile"]["zone"] = json!({"kind":"graveyard"});
    permanent(&mut e, 0, "wrong-zone", "{R}", vec![rule.clone()]);
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap()
            .symbols(),
        ["R"]
    );
    rule["activeWhile"] = json!({"kind":"unimplementedActivity"});
    e.state.players[0].battlefield[0].definition.rules = vec![rule];
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap_err()
            .state,
        "color.query.rule-domain"
    );
}

#[test]
fn controller_membership_is_not_inferred_from_storage_bucket() {
    let mut e = engine();
    let red = permanent(&mut e, 1, "red", "{R}", vec![]);
    e.change_permanent_controller(&red.instance_id, "player-0")
        .unwrap();
    let position = e.permanent_position(&red.instance_id).unwrap();
    assert_eq!(position.0, 0);
    assert_eq!(
        e.state.players[position.0].battlefield[position.1].controller,
        "player-0"
    );
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap()
            .symbols(),
        ["R"]
    );
    // Diagnostic representation probe: the controller field remains authoritative
    // even for a manually imported state whose storage bucket is inconsistent.
    let moved = e.state.players[0].battlefield.remove(position.1);
    e.state.players[1].battlefield.push(moved);
    assert_eq!(
        e.checked_color_set_query(&query(), "player-0")
            .unwrap()
            .symbols(),
        ["R"]
    );
    assert!(
        e.checked_color_set_query(&query(), "player-1")
            .unwrap()
            .symbols()
            .is_empty()
    );
}
