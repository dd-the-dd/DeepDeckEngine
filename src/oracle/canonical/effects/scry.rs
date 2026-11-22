use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) enum ScryFamilyParseAttempt {
    Parsed(CanonicalRuleDraft),
    DeferredToProgram,
    Failed(MachineFailure),
    NotApplicable,
}

pub(in crate::oracle::canonical) fn try_parse_promoted_scry_family(
    input: &AbilityInput<'_>,
) -> ScryFamilyParseAttempt {
    if !input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("scry"))
    {
        return ScryFamilyParseAttempt::NotApplicable;
    }
    match parse_atomic_scry(input) {
        Ok(Some(draft)) => ScryFamilyParseAttempt::Parsed(draft),
        Ok(None) => ScryFamilyParseAttempt::DeferredToProgram,
        Err(failure) => ScryFamilyParseAttempt::Failed(failure),
    }
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": self_ref() })
}

/// Scry and surveil share the same private library-partition procedure.
/// The typed action chooses only the destination and the registered signal.
pub(in crate::oracle::canonical) fn lower_scry_effects_for_player(
    count_expression: &NumericExpression,
    player: Value,
    scope: Option<&str>,
) -> MachineResult<Vec<Value>> {
    let count = lower_numeric_expression(count_expression).ok_or_else(|| MachineFailure {
        automaton: "effect.scry",
        state: "effect.scry.lower-count",
        token_index: 0,
        span_start: 0,
        span_end: 0,
        expected: "a target-independent numeric expression".to_string(),
        actual: None,
        trace: Vec::new(),
    })?;
    lower_library_partition_effects_for_player(
        LibraryPartitionAction::Scry,
        count_expression,
        count,
        player,
        scope,
    )
}

fn lower_scry_effects(effect: &ScryEffectNode) -> MachineResult<Vec<Value>> {
    let player = match &effect.player {
        LocalPlayerReference::Controller => controller(),
        LocalPlayerReference::ControllerOf(_)
        | LocalPlayerReference::ControllerOfQualified(_)
        | LocalPlayerReference::Bound(_) => {
            unreachable!("standalone scry recognizer binds controller")
        }
    };
    lower_scry_effects_for_player(&effect.count, player, None)
}

fn parse_atomic_scry(input: &AbilityInput<'_>) -> MachineResult<Option<CanonicalRuleDraft>> {
    let mut cursor = OracleTokenCursor::new("scry-action", input.tokens);
    let effect = recognize_scry_effect(&mut cursor)?;
    if cursor.peek().is_none() {
        cursor.enter("effect.scry.period");
        cursor.consume(".")?;
    }
    if cursor.peek() != Some(".") {
        return Ok(None);
    }
    cursor.enter("effect.scry.period");
    cursor.consume(".")?;
    if cursor.peek() == Some("(") {
        recognize_scry_reminder(&mut cursor, &effect.count)?;
    }
    if cursor.peek().is_some() {
        return Ok(None);
    }
    cursor.enter("effect.scry.validation");
    cursor.finish()?;
    let effects = lower_scry_effects(&effect)?;
    let mut rule = json!({
        "kind": "spellAbility",
        "source": self_ref(),
        "effects": effects,
    });
    if matches!(
        &effect.count,
        NumericExpression::Atom(NumericAtom::VariableX)
    ) {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": [x_value()],
        });
    }
    Ok(Some(draft(
        rule,
        &[
            "Recognize scry and its numeric expression from the shared token stream",
            "Partition the looked-at cards into ordered bottom and top collections",
            "Emit the catalog-registered scry signal only for a positive quantity",
        ],
    )))
}

#[cfg(test)]
#[path = "scry_sequence_tests.rs"]
mod sequence_tests;
