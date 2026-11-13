use super::*;
use std::collections::VecDeque;

#[derive(Default)]
struct RiotDecisionProvider {
    choices: VecDeque<usize>,
    requests: usize,
}

impl RiotDecisionProvider {
    fn with_choices(choices: impl IntoIterator<Item = usize>) -> Self {
        Self {
            choices: choices.into_iter().collect(),
            requests: 0,
        }
    }
}

impl DecisionProvider for RiotDecisionProvider {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        assert_eq!(request.kind, DecisionKind::ReplacementChoice);
        assert_eq!(request.options.len(), 2);
        assert!(request.options[0].label.contains("+1/+1 counter"));
        assert!(request.options[1].label.contains("haste"));
        self.requests += 1;
        self.choices
            .pop_front()
            .ok_or_else(|| EngineError::new("missing scripted Riot choice"))
    }
}

fn riot_grant_rule() -> Value {
    json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "modifiers": [{
            "kind": "grantKeyword",
            "objects": {
                "kind": "permanents",
                "controller": {
                    "kind": "controllerOf",
                    "object": { "kind": "self" },
                },
                "where": {
                    "kind": "and",
                    "operands": [
                        { "kind": "not", "operand": { "kind": "isToken" } },
                        { "kind": "cardTypeContains", "value": "Creature" },
                    ],
                },
            },
            "keyword": "riot",
        }],
    })
}

fn riot_source(id: &str) -> CardInstance {
    let mut definition = test_definition(id, "Enchantment");
    definition.rules = vec![riot_grant_rule()];
    test_instance(id, definition, "player-0")
}

fn entering_creature(id: &str) -> CardInstance {
    test_instance(id, test_definition(id, "Creature - Beast"), "player-0")
}

#[test]
fn granted_riot_uses_typed_counter_or_haste_entry_choices() {
    let mut counter_engine = test_engine(2);
    counter_engine.state.players[0]
        .battlefield
        .push(riot_source("riot-source"));
    let mut counter_creature = entering_creature("counter-creature");
    let mut counter_provider = RiotDecisionProvider::with_choices([0]);
    counter_engine
        .apply_enter_replacements(&mut counter_creature, &mut counter_provider)
        .expect("counter Riot choice applies");
    assert_eq!(counter_creature.counters.get("+1/+1"), Some(&1));
    assert!(!has_keyword(&counter_creature.definition, "haste"));

    let mut haste_engine = test_engine(2);
    haste_engine.state.players[0]
        .battlefield
        .push(riot_source("riot-source"));
    let mut haste_creature = entering_creature("haste-creature");
    let mut haste_provider = RiotDecisionProvider::with_choices([1]);
    haste_engine
        .apply_enter_replacements(&mut haste_creature, &mut haste_provider)
        .expect("haste Riot choice applies");
    assert!(haste_creature.counters.is_empty());
    assert!(has_keyword(&haste_creature.definition, "haste"));
}

#[test]
fn multiple_riot_instances_each_create_an_independent_entry_choice() {
    let mut engine = test_engine(2);
    engine.state.players[0]
        .battlefield
        .extend([riot_source("riot-a"), riot_source("riot-b")]);
    let mut creature = entering_creature("multiple-riot");
    let mut provider = RiotDecisionProvider::with_choices([0, 1]);

    engine
        .apply_enter_replacements(&mut creature, &mut provider)
        .expect("both Riot instances apply before entry");

    assert_eq!(provider.requests, 2);
    assert_eq!(creature.counters.get("+1/+1"), Some(&1));
    assert!(has_keyword(&creature.definition, "haste"));
}

#[test]
fn nontoken_selector_does_not_offer_riot_to_tokens_or_noncreatures() {
    let mut engine = test_engine(2);
    engine.state.players[0]
        .battlefield
        .push(riot_source("riot-source"));

    let mut token = entering_creature("creature-token");
    token.definition.is_token = true;
    let mut artifact = test_instance(
        "nontoken-artifact",
        test_definition("nontoken-artifact", "Artifact"),
        "player-0",
    );
    let mut provider = RiotDecisionProvider::default();

    engine
        .apply_enter_replacements(&mut token, &mut provider)
        .expect("token is outside the granted selector");
    engine
        .apply_enter_replacements(&mut artifact, &mut provider)
        .expect("noncreature is outside the granted selector");

    assert_eq!(provider.requests, 0);
    assert!(token.counters.is_empty());
    assert!(!has_keyword(&token.definition, "haste"));
    assert!(artifact.counters.is_empty());
}

#[test]
fn legacy_grant_riot_adapter_delegates_to_the_same_typed_choice() {
    let mut definition = test_definition("legacy-source", "Enchantment");
    definition.rules = vec![json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "modifiers": [{
            "kind": "grantRiot",
            "objects": {
                "kind": "permanents",
                "controller": {
                    "kind": "controllerOf",
                    "object": { "kind": "self" },
                },
                "where": { "kind": "cardTypeContains", "value": "Creature" },
            },
        }],
    })];
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.push(test_instance(
        "legacy-source",
        definition,
        "player-0",
    ));
    let mut creature = entering_creature("legacy-recipient");
    let mut provider = RiotDecisionProvider::with_choices([0]);

    engine
        .apply_enter_replacements(&mut creature, &mut provider)
        .expect("legacy adapter uses typed Riot semantics");

    assert_eq!(provider.requests, 1);
    assert_eq!(creature.counters.get("+1/+1"), Some(&1));
}

#[test]
fn copy_replacement_establishes_riot_eligibility_before_the_granted_choice() {
    struct ChooseArtifactCopy {
        requests: usize,
    }

    impl DecisionProvider for ChooseArtifactCopy {
        fn choose(
            &mut self,
            _state: &GameState,
            request: &EngineDecisionRequest,
        ) -> Result<usize, EngineError> {
            self.requests += 1;
            assert_eq!(request.kind, DecisionKind::ReplacementChoice);
            request
                .options
                .iter()
                .position(|option| option.label.contains("Copy copied-artifact"))
                .ok_or_else(|| EngineError::new("artifact copy option is unavailable"))
        }
    }

    let mut engine = test_engine(2);
    engine.state.players[0]
        .battlefield
        .push(riot_source("riot-source"));
    engine.state.players[0].battlefield.push(test_instance(
        "copied-artifact",
        test_definition("copied-artifact", "Artifact"),
        "player-0",
    ));
    let mut entering = entering_creature("copying-creature");
    entering.definition.rules = vec![json!({
        "kind": "replacementEffect",
        "source": { "kind": "self" },
        "event": { "kind": "wouldEnterBattlefield", "object": { "kind": "self" } },
        "decisions": [{
            "id": "entryCopy",
            "kind": "chooseBattlefieldPermanent",
            "where": { "kind": "cardTypeContains", "value": "Artifact" },
            "optional": true,
        }],
        "replacement": [{
            "kind": "copyEnteringPermanent",
            "decisionId": "entryCopy",
            "tapped": false,
            "grantKeywords": [],
        }],
    })];
    let mut provider = ChooseArtifactCopy { requests: 0 };

    engine
        .apply_enter_replacements(&mut entering, &mut provider)
        .expect("copy replacement resolves before granted Riot eligibility");

    assert_eq!(provider.requests, 1);
    assert_eq!(entering.definition.type_line, "Artifact");
    assert!(entering.counters.is_empty());
    assert!(!has_keyword(&entering.definition, "haste"));
}
