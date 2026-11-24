//! Runtime proofs of the shared entry-replacement frame (CR 614.1c, 614.12,
//! 707.9), the draw replacement (CR 614.1a, 121.6) and a spending union
//! (CR 106.6), on the IR the Oracle compiler emits for them.
use super::*;

/// Answers each replacement choice with the option whose label is listed
/// next, in order.
struct LabelProvider {
    labels: Vec<&'static str>,
}

impl DecisionProvider for LabelProvider {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        assert_eq!(request.kind, DecisionKind::ReplacementChoice);
        let wanted = self.labels.remove(0);
        request
            .options
            .iter()
            .position(|option| option.label == wanted)
            .ok_or_else(|| EngineError::new(format!("no option labeled `{wanted}`")))
    }
}

fn self_ref() -> Value {
    json!({ "kind": "self" })
}

fn on_battlefield() -> Value {
    json!({ "kind": "inZone", "object": self_ref(), "zone": { "kind": "battlefield" } })
}

/// `As this artifact enters, choose a color.` as the entry frame lowers it.
fn color_choice_rule() -> Value {
    json!({
        "kind": "replacementEffect",
        "source": self_ref(),
        "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
        "decisions": [{
            "id": "chosenColor",
            "kind": "chooseColor",
            "player": { "kind": "abilityController" },
            "options": ["W", "U", "B", "R", "G"],
        }],
        "replacement": [{ "kind": "storeDecision", "decisionId": "chosenColor" }],
    })
}

/// `Creatures you control of the chosen color get +1/+1.` after the
/// card-level binding to the stored choice.
fn chosen_color_anthem_rule() -> Value {
    json!({
        "kind": "staticAbility",
        "source": self_ref(),
        "activeWhile": on_battlefield(),
        "modifiers": [{
            "kind": "modifyPowerToughness",
            "objects": {
                "kind": "permanents",
                "controller": { "kind": "controllerOf", "object": self_ref() },
                "where": {
                    "kind": "and",
                    "operands": [
                        { "kind": "cardTypeContains", "value": "Creature" },
                        { "kind": "chosenColor", "decisionId": "chosenColor" },
                    ],
                },
            },
            "power": { "kind": "integer", "value": 1 },
            "toughness": { "kind": "integer", "value": 1 },
        }],
    })
}

fn colored_creature(id: &str, mana_cost: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature - Elf");
    definition.mana_cost = mana_cost.to_string();
    test_instance(id, definition, "player-0")
}

#[test]
fn stored_color_choice_is_read_by_a_static_predicate_and_a_mana_value() {
    let mut engine = test_engine(2);
    let mut definition = test_definition("caged-sun", "Artifact");
    definition.rules = vec![color_choice_rule(), chosen_color_anthem_rule()];
    let mut source = test_instance("caged-sun", definition, "player-0");
    let mut provider = LabelProvider {
        labels: vec!["Choose G"],
    };
    engine
        .apply_enter_replacements(&mut source, &mut provider)
        .expect("the color choice applies as the artifact enters");

    // The stored value feeds `storedColor` mana; the static reference is
    // bound to the concrete color predicate of the permanent's own rules.
    assert_eq!(
        stored_decision_value(&source.definition, "chosenColor"),
        Some("G")
    );
    assert_eq!(
        source.definition.rules[1]["modifiers"][0]["objects"]["where"]["operands"][1],
        json!({ "kind": "colorContains", "value": "green" })
    );
    // The producing replacement keeps its symbolic decision.
    assert_eq!(
        source.definition.rules[0]["decisions"][0]["kind"],
        "chooseColor"
    );

    let green = colored_creature("green-elf", "{G}");
    let red = colored_creature("red-elf", "{R}");
    engine.state.players[0]
        .battlefield
        .extend([source, green.clone(), red.clone()]);
    assert_eq!(engine.static_power_toughness_bonus(&green), (1, 1));
    assert_eq!(engine.static_power_toughness_bonus(&red), (0, 0));
}

#[test]
fn leaving_the_battlefield_restores_the_unbound_rules() {
    let mut definition = test_definition("caged-sun", "Artifact");
    definition.rules = vec![color_choice_rule(), chosen_color_anthem_rule()];
    let mut card = test_instance("caged-sun", definition, "player-0");
    super::super::stored_choice_binding::bind_stored_choice(
        &mut card.definition,
        "chosenColor",
        "W",
    );
    reset_after_leaving_battlefield(&mut card);
    assert_eq!(
        card.definition.rules[1]["modifiers"][0]["objects"]["where"]["operands"][1]["kind"],
        "chosenColor"
    );
}

fn entry_copy_rule(optional: bool) -> Value {
    json!({
        "kind": "replacementEffect",
        "source": self_ref(),
        "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
        "decisions": [{
            "id": "entryCopy",
            "kind": "chooseBattlefieldPermanent",
            "optional": optional,
            "where": { "kind": "cardTypeContains", "value": "Creature" },
        }],
        "replacement": [{
            "kind": "copyEnteringPermanent",
            "decisionId": "entryCopy",
            "tapped": false,
            "grantKeywords": ["haste"],
        }],
    })
}

#[test]
fn optional_entry_copy_takes_copiable_values_and_its_exception() {
    let mut engine = test_engine(2);
    let mut bear_definition = test_definition("bear", "Creature - Bear");
    bear_definition.power = Some("4".to_string());
    engine.state.players[1]
        .battlefield
        .push(test_instance("bear", bear_definition, "player-1"));

    let mut clone_definition = test_definition("clone", "Creature - Shapeshifter");
    clone_definition.rules = vec![entry_copy_rule(true)];
    let mut clone = test_instance("clone", clone_definition.clone(), "player-0");
    engine
        .apply_enter_replacements(
            &mut clone,
            &mut LabelProvider {
                labels: vec!["Copy bear"],
            },
        )
        .expect("the optional copy applies");
    assert_eq!(clone.definition.name, "bear");
    assert_eq!(clone.definition.power.as_deref(), Some("4"));
    assert!(has_keyword(&clone.definition, "haste"));

    let mut declined = test_instance("clone-2", clone_definition, "player-0");
    engine
        .apply_enter_replacements(
            &mut declined,
            &mut LabelProvider {
                labels: vec!["Do not copy a permanent"],
            },
        )
        .expect("declining the optional copy is legal");
    assert_eq!(declined.definition.name, "clone");
}

fn reveal_or_tapped_rule() -> Value {
    let plains = json!({ "kind": "subtypeContains", "value": "Plains" });
    json!({
        "kind": "replacementEffect",
        "source": self_ref(),
        "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
        "decisions": [{
            "id": "entryReveal",
            "kind": "chooseRevealCard",
            "where": plains.clone(),
            "minimum": { "kind": "integer", "value": 0 },
            "maximum": { "kind": "integer", "value": 1 },
        }],
        "replacement": [{
            "kind": "conditional",
            "condition": {
                "kind": "compare",
                "operator": "==",
                "left": {
                    "kind": "countDecisionCardsMatching",
                    "decisionId": "entryReveal",
                    "where": plains,
                },
                "right": { "kind": "integer", "value": 0 },
            },
            "then": [{ "kind": "setEnteringState", "object": self_ref(), "tapped": true }],
            "else": [],
        }],
    })
}

#[test]
fn reveal_or_tapped_land_enters_tapped_only_without_a_revealed_card() {
    let mut engine = test_engine(2);
    engine.state.players[0].hand.push(test_instance(
        "plains-in-hand",
        test_definition("plains-in-hand", "Basic Land - Plains"),
        "player-0",
    ));
    let mut land_definition = test_definition("port-town", "Land");
    land_definition.rules = vec![reveal_or_tapped_rule()];

    let mut revealed = test_instance("port-town-a", land_definition.clone(), "player-0");
    engine
        .apply_enter_replacements(
            &mut revealed,
            &mut LabelProvider {
                labels: vec!["Reveal plains-in-hand"],
            },
        )
        .expect("revealing applies");
    assert!(!revealed.tapped);

    let mut declined = test_instance("port-town-b", land_definition, "player-0");
    engine
        .apply_enter_replacements(
            &mut declined,
            &mut LabelProvider {
                labels: vec!["Do not reveal a card"],
            },
        )
        .expect("declining to reveal applies");
    assert!(declined.tapped);
}

#[test]
fn draw_replacement_turns_a_draw_outside_the_draw_step_into_two() {
    let mut engine = test_engine(2);
    let mut archive = test_definition("archive", "Artifact");
    archive.rules = vec![json!({
        "kind": "staticAbility",
        "source": self_ref(),
        "activeWhile": on_battlefield(),
        "modifiers": [{
            "kind": "increaseEachDrawCount",
            "player": { "kind": "abilityController" },
            "amount": 1,
            "exceptFirstInDrawStep": true,
        }],
    })];
    engine.state.players[0]
        .battlefield
        .push(test_instance("archive", archive, "player-0"));
    let before = engine.state.players[0].hand.len();
    engine
        .draw_cards("player-0", 1)
        .expect("the draw is replaced");
    assert_eq!(engine.state.players[0].hand.len(), before + 2);
}

#[test]
fn a_spending_union_admits_spells_and_abilities_of_the_same_objects() {
    let restricted = FloatingMana {
        symbol: "C".to_string(),
        spend_restriction: Some(json!({
            "kind": "castSpellOrActivateAbility",
            "where": { "kind": "cardTypeContains", "value": "Artifact" },
        })),
    };
    let artifact = test_definition("relic", "Artifact");
    let creature = test_definition("bear", "Creature - Bear");
    let pay = super::super::mana_postcost_ordered::mana_pool_entry_can_pay;
    assert!(pay(&restricted, Some(&artifact), None, Some("hand")));
    assert!(pay(&restricted, None, Some(&artifact), None));
    assert!(!pay(&restricted, Some(&creature), None, Some("hand")));
    assert!(!pay(&restricted, None, Some(&creature), None));
}
