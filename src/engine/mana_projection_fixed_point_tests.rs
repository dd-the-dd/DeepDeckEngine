use super::super::mana_projection_contract::ManaProjectionUnsupportedState;
use super::*;

fn green() -> ManaTypeProjection {
    ManaTypeProjection::Types(BTreeSet::from([ManaType::Green]))
}

#[test]
fn ungrounded_mutual_cycle_is_semantic_undefined_not_an_error() {
    let table = solve_mana_type_fixed_point(&["a".into(), "b".into()], |id, current| {
        Ok(current[if id == "a" { "b" } else { "a" }].clone())
    })
    .expect("recursive domains have a defined least fixed point");
    assert_eq!(
        table["a"],
        ManaTypeProjection::NoMana(NoManaReason::UndefinedType)
    );
    assert_eq!(
        table["b"],
        ManaTypeProjection::NoMana(NoManaReason::UndefinedType)
    );
}

#[test]
fn cycle_is_revisited_after_an_anchor_becomes_known_regardless_of_order() {
    for ids in [["a", "b", "anchor"], ["anchor", "b", "a"]] {
        let ids = ids.map(String::from);
        let table = solve_mana_type_fixed_point(&ids, |id, current| {
            Ok(match id {
                "anchor" => green(),
                "b" => current["a"].clone(),
                "a" => {
                    if matches!(current["anchor"], ManaTypeProjection::Types(_)) {
                        green()
                    } else {
                        current["b"].clone()
                    }
                }
                _ => unreachable!("closed test producer set"),
            })
        })
        .expect("anchor propagates around the cycle");
        for id in ids {
            assert_eq!(table[&id], green());
        }
    }
}

#[test]
fn unsupported_producer_error_is_not_a_successful_empty_context() {
    let error = solve_mana_type_fixed_point(&["source".into()], |_, _| {
        Err(ManaProjectionUnsupported {
            source_instance_id: "source".into(),
            rule_index: 7,
            state: ManaProjectionUnsupportedState::StatefulPrefix,
        })
    })
    .expect_err("unsupported cannot converge to empty");
    let ManaFixedPointFailure::Producer(error) = error else {
        panic!("producer role preserved")
    };
    assert_eq!(error.source_instance_id, "source");
    assert_eq!(error.rule_index, 7);
}

#[test]
fn missing_and_nonmonotone_sources_are_errors() {
    assert!(
        matches!(require_source(&ManaTypeTable::new(), "missing"), Err(ManaFixedPointFailure::MissingSource(id)) if id == "missing")
    );
    let mut calls = 0;
    let error = solve_mana_type_fixed_point(&["source".into()], |_, _| {
        calls += 1;
        Ok(if calls == 1 {
            green()
        } else {
            ManaTypeProjection::NoMana(NoManaReason::ZeroAmount)
        })
    })
    .expect_err("nonmonotone evaluation cannot be silently accepted");
    assert!(matches!(error, ManaFixedPointFailure::NonMonotoneProducer(id) if id == "source"));
}
