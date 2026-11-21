use super::*;

#[test]
fn projection_budget_counts_all_branches_and_accepts_its_last_payable_state() {
    for payable_projection in [4096, 4097] {
        let mut calls = 0;
        let mut project = |prefix: &[ManaActivation]| {
            calls += 1;
            if prefix.is_empty() {
                return Ok(world(
                    &[],
                    (0..4096)
                        .map(|i| option(&format!("branch-{i}"), &[]))
                        .collect(),
                ));
            }
            if calls == payable_projection {
                Ok(world(&[(0, "G", false)], vec![]))
            } else {
                Ok(world(&[], vec![]))
            }
        };
        let result =
            GameEngine::try_plan_ordered_mana_requirement(&requirement("{G}"), &mut project);
        assert_eq!(calls, 4096);
        if payable_projection == 4096 {
            assert!(result.unwrap().is_some());
        } else {
            assert!(result.is_err());
        }
    }
}

#[test]
fn duplicate_physical_pool_indices_are_not_double_counted() {
    let mut project = |_: &[ManaActivation]| Ok(world(&[(3, "G", false), (3, "G", false)], vec![]));
    assert!(
        GameEngine::try_plan_ordered_mana_requirement(&requirement("{G}{G}"), &mut project,)
            .is_err()
    );
}

fn requirement(text: &str) -> ManaRequirement {
    parse_mana_requirement(text).expect("test mana requirement")
}

fn option(source: &str, declared_output: &[&str]) -> OrderedManaChoice {
    OrderedManaChoice {
        source_id: source.into(),
        choice: ManaChoice {
            intrinsic_symbols: None,
            origin: Some(ManaProgramOrigin::Native { rule_index: 0 }),
            symbols: declared_output.iter().map(|s| (*s).into()).collect(),
            snow: false,
            life_cost: 0,
            spend_restriction: None,
            activation_requirement: ManaRequirement::default(),
            sacrifices_source: false,
        },
        declaration: serde_json::from_value(json!({
            "id": source, "kind": "activateAbility", "playerId": "player-0",
            "label": source, "cardInstanceId": source,
        }))
        .expect("declared test activation"),
    }
}

fn world(pool: &[(usize, &str, bool)], choices: Vec<OrderedManaChoice>) -> OrderedManaState {
    OrderedManaState {
        available_mana: pool.iter().map(|(i, s, n)| (*i, (*s).into(), *n)).collect(),
        available_mana_cant_be_countered_indices: Vec::new(),
        available_life: 20,
        choices,
        progress_key: String::new(),
        deferred_branch_error: None,
    }
}

#[test]
fn empty_requirement_does_not_project_or_inspect_sources() {
    let mut project = |_: &[ManaActivation]| -> Result<OrderedManaState, EngineError> {
        panic!("zero payment must not inspect unsupported producers");
    };
    let payment =
        GameEngine::try_plan_ordered_mana_requirement(&ManaRequirement::default(), &mut project)
            .unwrap()
            .unwrap();
    assert!(payment.activations.is_empty());
    assert!(payment.pool_indices.is_empty());
}

#[test]
fn gross_preview_mana_is_not_spendable_after_a_sacrifice() {
    let mut project = |prefix: &[ManaActivation]| {
        Ok(world(
            &[],
            if prefix.is_empty() {
                vec![option("sacrifice", &["G"])]
            } else {
                vec![]
            },
        ))
    };
    assert!(
        GameEngine::try_plan_ordered_mana_requirement(&requirement("{G}"), &mut project,)
            .unwrap()
            .is_none()
    );
}

#[test]
fn same_source_can_activate_twice_when_projected_world_allows_it() {
    let mut project = |prefix: &[ManaActivation]| {
        let pool = (0..prefix.len())
            .map(|i| (i, "G", false))
            .collect::<Vec<_>>();
        let mut state = world(
            &pool,
            if prefix.len() < 2 {
                vec![option("reusable", &["G"])]
            } else {
                vec![]
            },
        );
        state.available_life = 20 - prefix.len() as i32;
        Ok(state)
    };
    let payment =
        GameEngine::try_plan_ordered_mana_requirement(&requirement("{G}{G}"), &mut project)
            .unwrap()
            .unwrap();
    assert_eq!(
        payment
            .activations
            .iter()
            .map(|a| a.source_id.as_str())
            .collect::<Vec<_>>(),
        ["reusable", "reusable"]
    );
    assert_eq!(payment.pool_indices, [0, 1]);
    assert!(payment.ordered);
}

#[test]
fn zero_output_cost_can_unlock_a_new_source_in_the_next_world() {
    let mut project = |prefix: &[ManaActivation]| {
        Ok(match prefix.len() {
            0 => world(&[], vec![option("unlock", &[])]),
            1 => world(&[], vec![option("new-source", &["G"])]),
            _ => world(&[(9, "G", false)], vec![]),
        })
    };
    let payment = GameEngine::try_plan_ordered_mana_requirement(&requirement("{G}"), &mut project)
        .unwrap()
        .unwrap();
    assert_eq!(
        payment
            .activations
            .iter()
            .map(|a| a.source_id.as_str())
            .collect::<Vec<_>>(),
        ["unlock", "new-source"]
    );
    assert_eq!(payment.pool_indices, [9]);
}

#[test]
fn actual_order_is_preserved_and_a_failed_order_does_not_hide_a_good_one() {
    let mut project = |prefix: &[ManaActivation]| {
        Ok(match prefix.len() {
            0 => world(
                &[],
                vec![
                    option("sacrifice-red", &["R"]),
                    option("query-colors", &["G", "R"]),
                ],
            ),
            1 if prefix[0].source_id == "sacrifice-red" => {
                world(&[(0, "R", false)], vec![option("query-colors", &["G"])])
            }
            1 => world(
                &[(0, "G", false), (1, "R", false)],
                vec![option("sacrifice-red", &["R"])],
            ),
            _ if prefix[0].source_id == "sacrifice-red" => {
                world(&[(0, "R", false), (1, "G", false)], vec![])
            }
            _ => world(&[(0, "G", false), (1, "R", false), (2, "R", false)], vec![]),
        })
    };
    let payment =
        GameEngine::try_plan_ordered_mana_requirement(&requirement("{R}{R}{G}"), &mut project)
            .unwrap()
            .unwrap();
    assert_eq!(
        payment
            .activations
            .iter()
            .map(|a| a.source_id.as_str())
            .collect::<Vec<_>>(),
        ["query-colors", "sacrifice-red"]
    );
}

#[test]
fn final_pool_indices_and_only_spent_counter_protection_are_preserved() {
    for protected in [5, 7] {
        let mut project = |_: &[ManaActivation]| {
            let mut state = world(&[(5, "G", false), (7, "R", false)], vec![]);
            state.available_mana_cant_be_countered_indices = vec![protected];
            Ok(state)
        };
        let payment =
            GameEngine::try_plan_ordered_mana_requirement(&requirement("{G}"), &mut project)
                .unwrap()
                .unwrap();
        assert_eq!(payment.pool_indices, [5]);
        assert_eq!(payment.pool_symbols, ["G"]);
        assert_eq!(payment.cant_be_countered, protected == 5);
    }
}

#[test]
fn final_pool_reuses_snow_hybrid_and_phyrexian_allocation() {
    let mut project = |_: &[ManaActivation]| {
        let mut state = world(&[(4, "G", true), (8, "U", false)], vec![]);
        state.available_life = 2;
        Ok(state)
    };
    let payment =
        GameEngine::try_plan_ordered_mana_requirement(&requirement("{S}{G/U}{W/P}"), &mut project)
            .unwrap()
            .unwrap();
    assert_eq!(
        payment
            .pool_indices
            .iter()
            .copied()
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([4, 8])
    );
    assert_eq!(payment.phyrexian_life_paid, 2);
}

#[test]
fn incomplete_branch_is_error_unless_a_complete_sibling_proves_payment() {
    for has_success in [false, true] {
        let mut project = |prefix: &[ManaActivation]| {
            if let Some(first) = prefix.first() {
                if first.source_id == "unsupported" {
                    return Err(EngineError::new("unsupported branch"));
                }
                return Ok(world(&[(0, "G", false)], vec![]));
            }
            let mut choices = vec![option("unsupported", &[])];
            if has_success {
                choices.push(option("supported", &["G"]));
            }
            Ok(world(&[], choices))
        };
        let result =
            GameEngine::try_plan_ordered_mana_requirement(&requirement("{G}"), &mut project);
        if has_success {
            assert!(result.unwrap().is_some());
        } else {
            assert!(result.is_err());
        }
    }
}

#[test]
fn depth_exhaustion_is_explicit_but_a_payable_boundary_is_accepted() {
    for payable_at_boundary in [false, true] {
        let mut project = |prefix: &[ManaActivation]| {
            if payable_at_boundary && prefix.len() == 128 {
                return Ok(world(&[(0, "G", false)], vec![]));
            }
            Ok(world(&[], vec![option("unbounded", &[])]))
        };
        let result =
            GameEngine::try_plan_ordered_mana_requirement(&requirement("{G}"), &mut project);
        if payable_at_boundary {
            assert!(result.unwrap().is_some());
        } else {
            assert!(result.is_err());
        }
    }
}

#[test]
fn deferred_branch_does_not_hide_available_pool_or_a_complete_sibling() {
    for scenario in 0..3 {
        let mut project = |prefix: &[ManaActivation]| {
            if !prefix.is_empty() {
                return Ok(world(&[(0, "G", false)], vec![]));
            }
            let pool = if scenario == 0 {
                vec![(0, "G", false)]
            } else {
                vec![]
            };
            let choices = if scenario == 1 {
                vec![option("complete", &["G"])]
            } else {
                vec![]
            };
            let mut state = world(&pool, choices);
            state.deferred_branch_error = Some(EngineError::new("unresolved cost choice"));
            Ok(state)
        };
        let result =
            GameEngine::try_plan_ordered_mana_requirement(&requirement("{G}"), &mut project);
        if scenario == 2 {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().is_some());
        }
    }
}
