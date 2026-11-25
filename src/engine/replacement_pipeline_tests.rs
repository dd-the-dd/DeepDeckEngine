//! The shared propose -> match -> order -> apply loop and its graveyard
//! destination family.
use super::*;

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
                    cards: vec![definition("filler", "Instant", vec![]); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        6161,
    )
    .unwrap()
}

/// Picks the last offered option and records the request.
struct PickLast(Vec<String>);
impl DecisionProvider for PickLast {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        self.0.push(request.id.clone());
        Ok(request.options.len() - 1)
    }
}

fn proposal(id: &str, delta: i32) -> ReplacementProposal<i32> {
    ReplacementProposal {
        id: id.into(),
        source_id: format!("source-{id}"),
        source_name: id.into(),
        program: delta,
    }
}

#[test]
fn each_proposal_applies_at_most_once_in_the_chosen_order() {
    let e = engine();
    let proposals = vec![proposal("double", 0), proposal("plus-one", 1)];
    let mut amount = 2;
    let mut provider = PickLast(Vec::new());
    let applied = e
        .run_replacement_pipeline(
            &proposals,
            &mut amount,
            &ReplacementOrder::AffectedChooses {
                player_id: "player-0",
                request_prefix: "test-replacement",
                decision_key: "testReplacementId",
            },
            &mut provider,
            |_, amount| *amount > 0,
            |proposal, amount| {
                *amount = if proposal.program == 0 {
                    *amount * 2
                } else {
                    *amount + proposal.program
                };
                Ok(())
            },
        )
        .unwrap();
    // The chooser picked plus-one first, then the only remaining proposal.
    assert_eq!(applied, vec!["plus-one".to_string(), "double".to_string()]);
    assert_eq!(amount, 6);
    assert_eq!(provider.0.len(), 1);
    assert!(provider.0[0].starts_with("test-replacement:"));
}

#[test]
fn ordering_policies_are_explicit() {
    let e = engine();
    let proposals = vec![proposal("a", 1), proposal("b", 1)];
    let mut provider = PickLast(Vec::new());
    let mut first = 0;
    let applied = e
        .run_replacement_pipeline(
            &proposals,
            &mut first,
            &ReplacementOrder::FirstProposed,
            &mut provider,
            |_, applied| *applied == 0,
            |proposal, applied| {
                *applied += proposal.program;
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(applied, vec!["a".to_string()]);
    // A real choice without a chooser is an explicit refusal, never a pick.
    let mut unchosen = 0;
    let error = e
        .run_replacement_pipeline(
            &proposals,
            &mut unchosen,
            &ReplacementOrder::AffectedChooses {
                player_id: "player-0",
                request_prefix: "test-replacement",
                decision_key: "testReplacementId",
            },
            &mut RequiredReplacementChoiceProvider(None),
            |_, _| true,
            |_, _| Ok(()),
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("requires an explicit decision provider")
    );
    assert_eq!(unchosen, 0);
    assert!(provider.0.is_empty());
}

#[test]
fn graveyard_destination_proposals_share_one_precedence() {
    let mut e = engine();
    let replacer = definition(
        "replacer",
        "Enchantment",
        vec![json!({"kind":"staticAbility","source":{"kind":"self"},
            "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
            "modifiers":[{"kind":"replaceGraveyardDestination",
                "graveyardOwners":{"kind":"opponentsOf","player":{"kind":"abilityController"}},
                "from":{"kind":"anyZone"},"to":{"kind":"exile"}}]})],
    );
    e.state.players[0]
        .battlefield
        .push(card_instance("player-0", &replacer, 700));
    e.admit_explicit_test_fixture_objects().unwrap();
    let opponent_card = card_instance("player-1", &definition("bear", "Creature", vec![]), 701);
    let modifiers = e.battlefield_static_modifiers();
    let proposals =
        e.propose_graveyard_destination_replacements(&opponent_card, "player-1", &modifiers);
    assert_eq!(proposals.len(), 1);
    assert!(matches!(
        proposals[0].program.origin,
        GraveyardReplacementOrigin::Static
    ));
    // Near miss: the controller's own graveyard is not replaced.
    let own_card = card_instance("player-0", &definition("elk", "Creature", vec![]), 702);
    assert!(
        e.propose_graveyard_destination_replacements(&own_card, "player-0", &modifiers)
            .is_empty()
    );
    // A card-specific one-shot modifier is proposed first and is selected.
    e.state.rule_modifiers.push(json!({
        "kind": "exileSpecificCardInsteadOfGraveyard",
        "cardInstanceId": opponent_card.instance_id,
        "sourceCardInstanceId": "spell-source",
    }));
    let selected = e
        .select_graveyard_destination_replacement(&opponent_card, "player-1", &modifiers, None)
        .unwrap()
        .expect("a replacement applies");
    assert!(matches!(
        selected.program.origin,
        GraveyardReplacementOrigin::SpecificCard { .. }
    ));
}
