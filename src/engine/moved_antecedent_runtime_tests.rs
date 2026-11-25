//! Runtime proofs of the moved-permanent antecedent the binding environment
//! publishes (CR 400.7, 608.2c): a battlefield entry binds the permanent its
//! card became for the whole resolution, so a later independent program
//! step (`That creature gains haste.` after `You may put a creature card
//! ... onto the battlefield.`) reads the same binding; when the optional
//! entry was declined the binding names no object and the reading action
//! does nothing instead of falling back to another object.
use super::super::*;

fn definition(id: &str, type_line: &str) -> CardDefinition {
    serde_json::from_value(json!({
        "id": id,
        "name": id,
        "typeLine": type_line,
        "manaCost": "{0}",
        "power": "2",
        "toughness": "2",
        "rules": [],
    }))
    .unwrap()
}

/// A source on the battlefield and one creature card in its controller's
/// graveyard.
fn game() -> (GameEngine, StackObject) {
    let filler = definition("filler", "Instant");
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("p{index}"),
                    name: format!("P{index}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        8811,
    )
    .unwrap();
    let mut source = card_instance("p0", &definition("source", "Artifact"), 401);
    engine.prepare_battlefield_entry_identity(&mut source);
    engine.state.players[0].battlefield.push(source.clone());
    engine.state.players[0].graveyard.push(card_instance(
        "p0",
        &definition("returned", "Creature — Zombie"),
        402,
    ));
    let stack = StackObject {
        id: "moved-antecedent-stack".into(),
        controller: "p0".into(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (engine, stack)
}

fn moved() -> Value {
    json!({"kind": "singleBoundObject", "binding": "movedOntoBattlefield"})
}

/// `put a creature card from your graveyard onto the battlefield`: the
/// ordinary card move, which always binds the permanent it became.
fn graveyard_entry() -> Value {
    json!({"kind": "moveCards",
        "cards": {"kind": "cardsInZone",
            "zone": {"kind": "graveyard", "player": {"kind": "abilityController"}},
            "where": {"kind": "cardTypeContains", "value": "Creature"}},
        "to": {"kind": "battlefield", "player": {"kind": "abilityController"}, "tapped": false},
        "bind": "movedOntoBattlefield"})
}

fn rule(effects: Vec<Value>) -> Value {
    json!({"kind": "activatedAbility", "source": {"kind": "self"}, "costs": [],
        "effects": effects})
}

/// Answers every resolution choice with the same option index (0 declines
/// an optional effect, 1 performs it).
struct Choose(usize);

impl DecisionProvider for Choose {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Ok(self.0)
    }
}

fn returned_permanent(engine: &GameEngine) -> Option<&CardInstance> {
    engine.state.players[0]
        .battlefield
        .iter()
        .find(|permanent| permanent.definition.id == "returned")
}

fn resolve(effects: Vec<Value>, choice: usize) -> GameEngine {
    let (mut engine, mut stack) = game();
    stack.ability_rule = Some(rule(effects));
    engine.state.stack.push(stack);
    engine.resolve_top_stack(&mut Choose(choice)).unwrap();
    engine
}

#[test]
fn a_following_step_acts_on_the_permanent_an_earlier_step_put_onto_the_battlefield() {
    // The two steps are concatenated into one resolution; the second reads
    // the first step's binding, never the source.
    let engine = resolve(
        vec![
            graveyard_entry(),
            json!({"kind": "tapPermanent", "permanent": moved()}),
        ],
        0,
    );
    let returned = returned_permanent(&engine).expect("the card entered the battlefield");
    assert!(returned.tapped, "the moved permanent is the one tapped");
    assert!(
        !engine.state.players[0]
            .battlefield
            .iter()
            .any(|permanent| permanent.definition.id == "source" && permanent.tapped),
        "the ability source is never the fallback antecedent"
    );
}

#[test]
fn an_optional_entry_binds_the_permanent_only_when_it_was_performed() {
    let effects = || {
        vec![
            json!({"kind": "optionalEffects", "player": {"kind": "abilityController"},
                "effects": [graveyard_entry()]}),
            json!({"kind": "tapPermanent", "permanent": moved()}),
        ]
    };
    // Performed: the following step taps the permanent the card became.
    let performed = resolve(effects(), 1);
    assert!(returned_permanent(&performed).is_some_and(|permanent| permanent.tapped));
    // Declined: nothing entered, the binding names no object and the
    // following action does nothing (no error, no other object tapped).
    let declined = resolve(effects(), 0);
    assert!(returned_permanent(&declined).is_none());
    assert!(
        declined.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.definition.id == "returned")
    );
    assert!(
        declined.state.players[0]
            .battlefield
            .iter()
            .all(|permanent| !permanent.tapped)
    );
}
