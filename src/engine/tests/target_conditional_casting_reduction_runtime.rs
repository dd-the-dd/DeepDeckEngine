use super::*;

// Real Oracle text: parser -> IR -> engine proof that an own reduction keyed to
// the spell's chosen targets reads those targets, not unrelated permanents.
const ORACLE_TEXT: &str = "This spell costs {2} less to cast if it targets an attacking creature.";

fn spell_definition() -> CardDefinition {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Targeted Reducer".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{3}{B}".to_string()),
        oracle_text: Some(ORACLE_TEXT.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let mut definition = test_definition("targeted-reducer", "Instant");
    definition.mana_cost = "{3}{B}".to_string();
    definition.rules = parsed
        .abilities
        .into_iter()
        .map(|ability| ability.rule.expect("canonical reduction"))
        .collect();
    assert!(definition.rules.iter().all(rule_is_executable));
    definition
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn reduction_reads_the_chosen_targets_of_the_spell() {
    let definition = spell_definition();
    for (attacking, creature_type, expected) in [
        (true, "Creature — Elf", 2),
        (false, "Creature — Elf", 0),
        (true, "Artifact", 0),
    ] {
        let mut engine = test_engine(2);
        let mut victim = test_instance(
            "victim",
            test_definition("victim", creature_type),
            "player-1",
        );
        victim.flags.insert("attacking".to_string(), attacking);
        let bystander = {
            let mut other = test_instance(
                "other",
                test_definition("other", "Creature — Elf"),
                "player-1",
            );
            other.flags.insert("attacking".to_string(), true);
            other
        };
        engine.state.players[1].battlefield = vec![victim, bystander];
        let targets = BTreeMap::from([(
            "target-0".to_string(),
            TargetRef::Permanent {
                instance_id: "victim".to_string(),
            },
        )]);
        assert_eq!(
            engine.own_targeting_casting_cost_reduction(&definition, "player-0", &targets),
            expected,
            "attacking={attacking} type={creature_type}"
        );
        assert_eq!(
            engine.own_targeting_casting_cost_reduction(&definition, "player-0", &BTreeMap::new()),
            0
        );
    }
}
