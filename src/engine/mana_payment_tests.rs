use super::*;

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":type_line,"rules":rules}))
        .expect("bounded test definition")
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Land", vec![]);
    let mut e = GameEngine::new(
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
        644,
    )
    .expect("bounded engine");
    for id in ["first", "second"] {
        let rule = json!({"kind":"manaAbility","source":{"kind":"self"},
            "costs":[{"kind":"tap","object":{"kind":"self"}}],
            "effects":[{"kind":"addMana","player":{"kind":"abilityController"},"mana":"{G}"}]});
        assert!(rule_is_executable(&rule));
        e.state.players[0].battlefield.push(card_instance(
            "player-0",
            &definition(id, "Land", vec![rule]),
            0,
        ));
    }
    e
}

#[derive(Debug, PartialEq, Eq)]
struct UnsupportedSource(String);

#[test]
fn zero_cost_and_invalid_cost_do_not_query_sources() {
    let e = engine();
    let mut calls = 0;
    let mut resolve = |_: &CardInstance, _: Option<&CardDefinition>| {
        calls += 1;
        Err::<Vec<ManaChoice>, _>(UnsupportedSource("must not be reached".into()))
    };
    let zero = e
        .try_plan_mana_payment(0, &[], &BTreeSet::new(), None, None, None, &mut resolve)
        .expect("zero cost is not unsupported")
        .expect("empty payment exists");
    assert!(zero.activations.is_empty());
    assert!(zero.pool_indices.is_empty());
    assert_eq!(zero.phyrexian_life_paid, 0);
    let invalid = e
        .try_plan_mana_payment(
            0,
            &["{DefinitelyNotMana}".into()],
            &BTreeSet::new(),
            None,
            None,
            None,
            &mut resolve,
        )
        .expect("invalid cost is an impossible payment, not a source error");
    assert!(invalid.is_none());
    assert_eq!(calls, 0);
}

#[test]
fn later_source_error_is_preserved_even_after_an_ordinary_candidate() {
    let e = engine();
    let mut visited = Vec::new();
    let before = serde_json::to_value(&e.state).unwrap();
    let mut resolve = |source: &CardInstance, spending: Option<&CardDefinition>| {
        visited.push(source.definition.id.clone());
        if source.definition.id == "second" {
            return Err(UnsupportedSource(source.instance_id.clone()));
        }
        Ok(e.mana_choices_for_permanent(source, spending, false, false))
    };
    let failure = e
        .try_plan_mana_payment(
            0,
            &["{G}".into()],
            &BTreeSet::new(),
            None,
            None,
            None,
            &mut resolve,
        )
        .unwrap_err();
    assert_eq!(
        failure,
        UnsupportedSource(e.state.players[0].battlefield[1].instance_id.clone())
    );
    assert_eq!(visited, ["first", "second"]);
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}

#[test]
fn excluded_or_unavailable_sources_are_not_queried() {
    let mut e = engine();
    let first = e.state.players[0].battlefield[0].instance_id.clone();
    e.state.players[0].battlefield[1].tapped = true;
    let mut calls = 0;
    let mut resolve = |_: &CardInstance, _: Option<&CardDefinition>| {
        calls += 1;
        Err::<Vec<ManaChoice>, _>(UnsupportedSource("must not be reached".into()))
    };
    let payment = e
        .try_plan_mana_payment(
            0,
            &["{G}".into()],
            &BTreeSet::from([first]),
            None,
            None,
            None,
            &mut resolve,
        )
        .expect("unavailable source is not unsupported");
    assert!(payment.is_none());
    assert_eq!(calls, 0);
}

#[test]
fn supported_payment_keeps_exact_source_symbols_and_impossible_is_not_error() {
    let e = engine();
    let mut resolve = |source: &CardInstance, spending: Option<&CardDefinition>| {
        Ok::<_, UnsupportedSource>(e.mana_choices_for_permanent(source, spending, false, false))
    };
    let payment = e
        .try_plan_mana_payment(
            0,
            &["{G}".into()],
            &BTreeSet::new(),
            None,
            None,
            None,
            &mut resolve,
        )
        .unwrap()
        .expect("green payment exists");
    assert_eq!(payment.activations.len(), 1);
    assert_eq!(
        payment.activations[0].source_id,
        e.state.players[0].battlefield[0].instance_id
    );
    assert_eq!(
        payment.activations[0]
            .origin
            .as_ref()
            .and_then(ManaProgramOrigin::native_index),
        Some(0)
    );
    assert_eq!(payment.activations[0].symbols, ["G"]);
    assert_eq!(payment.phyrexian_life_paid, 0);
    assert!(payment.pool_indices.is_empty());
    assert!(!payment.cant_be_countered);
    let impossible = e
        .try_plan_mana_payment(
            0,
            &["{U}".into()],
            &BTreeSet::new(),
            None,
            None,
            None,
            &mut resolve,
        )
        .expect("ordinary lack of blue is not unsupported");
    assert!(impossible.is_none());
}
