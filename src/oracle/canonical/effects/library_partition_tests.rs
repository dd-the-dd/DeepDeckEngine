use super::*;

fn expansion(
    action: LibraryPartitionAction,
    expression: NumericExpression,
    scope: Option<&str>,
) -> Vec<Value> {
    let count = lower_numeric_expression(&expression).unwrap();
    lower_library_partition_effects_for_player(action, &expression, count, controller(), scope)
        .unwrap()
}

#[test]
fn destinations_differ_but_both_procedures_restore_the_exact_partition() {
    for (action, destination, prefix) in [
        (LibraryPartitionAction::Scry, "library", "scry"),
        (LibraryPartitionAction::Surveil, "graveyard", "surveil"),
    ] {
        let effects = expansion(
            action,
            NumericExpression::Atom(NumericAtom::Integer(2)),
            None,
        );
        assert_eq!(effects.len(), 7);
        assert_eq!(effects[0]["kind"], "lookAtTopCards");
        assert_eq!(effects[1]["kind"], "chooseCards");
        assert_eq!(effects[1]["minimum"], integer(0));
        assert_eq!(effects[3]["to"]["kind"], destination);
        assert_eq!(effects[4]["objects"]["kind"], "setDifference");
        assert_eq!(effects[5]["to"]["position"], "top");
        assert_eq!(effects[6]["kind"], "emitAbilitySignal");
        assert_eq!(effects[6]["abilityId"], prefix);
        assert!(
            effects
                .iter()
                .all(|effect| effect["kind"] != "scry" && effect["kind"] != "surveil")
        );
    }
}

#[test]
fn zero_quantity_never_emits_a_completion_and_dynamic_zero_is_guarded() {
    let effects = expansion(
        LibraryPartitionAction::Surveil,
        NumericExpression::Atom(NumericAtom::Integer(0)),
        None,
    );
    assert_eq!(effects.len(), 6);
    assert!(
        effects
            .iter()
            .all(|effect| effect["kind"] != "emitAbilitySignal")
    );
    let effects = expansion(
        LibraryPartitionAction::Surveil,
        NumericExpression::Atom(NumericAtom::VariableX),
        None,
    );
    assert_eq!(effects.last().unwrap()["kind"], "conditional");
    assert_eq!(effects.last().unwrap()["condition"]["right"], integer(1));
    assert_eq!(effects.last().unwrap()["then"][0]["abilityId"], "surveil");
}

#[test]
fn sibling_actions_cannot_overwrite_each_others_transient_bindings() {
    let count = NumericExpression::Atom(NumericAtom::Integer(2));
    let first = expansion(
        LibraryPartitionAction::Surveil,
        count.clone(),
        Some("effect0"),
    );
    let second = expansion(LibraryPartitionAction::Surveil, count, Some("effect1"));
    assert_eq!(first[0]["bind"], "effect0:surveilLookedCards");
    assert_eq!(second[0]["bind"], "effect1:surveilLookedCards");
    assert_ne!(first[1]["id"], second[1]["id"]);
    assert_eq!(first[5]["cards"]["decisionId"], first[4]["id"]);
    assert_eq!(second[5]["cards"]["decisionId"], second[4]["id"]);
}
