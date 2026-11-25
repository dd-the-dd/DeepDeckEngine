//! Lower only the cast-history contract actually implemented by the runtime.
use super::super::*;
use crate::oracle::{
    characteristic_vocabulary::type_line_has_permanent_card_type, syntax::AbilityInput,
};

pub(in crate::oracle::canonical) enum CastQuotaProgramAttempt {
    Parsed(CanonicalRuleDraft),
    NotClaimed,
    Failed(MachineFailure),
}

/// The event-history matcher is narrower than card_filter_supported. Validate
/// every child, including under negation; an unsupported child must not become
/// false and then accidentally true under NOT. Null is the universal filter;
/// the shared spellCast-event matcher must support that same contract.
pub(in crate::oracle::canonical) fn quota_history_filter_supported(filter: &Value) -> bool {
    if filter.is_null() {
        return true;
    }
    match filter.get("kind").and_then(Value::as_str) {
        Some("and" | "or") => filter["operands"].as_array().is_some_and(|children| {
            !children.is_empty() && children.iter().all(quota_history_filter_supported)
        }),
        Some("not") => quota_history_filter_supported(&filter["operand"]),
        Some("cardTypeContains" | "subtypeContains" | "typeLineContains" | "nameEquals") => {
            filter["value"].is_string()
        }
        Some("isLegendary") => true,
        _ => false,
    }
}

pub(in crate::oracle::canonical) fn try_parse_promoted_cast_quota(
    input: &AbilityInput<'_>,
) -> CastQuotaProgramAttempt {
    let mut cursor = OracleTokenCursor::new("static-cast-quota", input.tokens);
    let node = match try_recognize_cast_quota(&mut cursor) {
        CastQuotaRecognition::Parsed(node) => node,
        CastQuotaRecognition::NotClaimed | CastQuotaRecognition::UnsupportedEnvelope(_) => {
            return CastQuotaProgramAttempt::NotClaimed;
        }
        CastQuotaRecognition::Failed(failure) => return CastQuotaProgramAttempt::Failed(failure),
    };
    match lower_cast_quota(&mut cursor, node, input.face_type_line) {
        Ok(draft) => CastQuotaProgramAttempt::Parsed(draft),
        Err(failure) => CastQuotaProgramAttempt::Failed(failure),
    }
}

fn lower_cast_quota(
    cursor: &mut OracleTokenCursor<'_>,
    node: CastQuotaNode,
    face_type_line: &str,
) -> MachineResult<CanonicalRuleDraft> {
    cursor.enter("static.cast-quota.source-zone");
    cursor.require(
        type_line_has_permanent_card_type(face_type_line),
        "a permanent source face for a battlefield-active quota",
    )?;
    cursor.enter("static.cast-quota.lower.players");
    let players = match node.players {
        PlayerSetReferenceNode::EachPlayer => json!({"kind":"eachPlayer"}),
        PlayerSetReferenceNode::OpponentsOfAbilityController
        | PlayerSetReferenceNode::OpponentsOtherThanEventPlayer => {
            return Err(cursor
                .fail("each player; the runtime quota currently ignores narrower player scopes"));
        }
    };
    cursor.enter("static.cast-quota.lower.maximum");
    let NumericExpression::Atom(NumericAtom::Integer(maximum)) = node.maximum else {
        return Err(cursor.fail("a positive i32 literal cast maximum"));
    };
    cursor.require(
        maximum > 0 && i32::try_from(maximum).is_ok(),
        "a positive i32 literal cast maximum",
    )?;
    cursor.enter("static.cast-quota.lower.caster");
    cursor.require(
        node.spells.caster.is_none(),
        "an unqualified spell collection; the quota already owns its player population",
    )?;
    let filter = lower_spell_object_predicate(&node.spells.predicate).unwrap_or(Value::Null);
    cursor.enter("static.cast-quota.history-filter-support");
    cursor.require(quota_history_filter_supported(&filter),"a universal cast-history filter or Boolean filter over recorded types, subtypes, supertypes or names; no color, numeric or keyword approximation")?;
    Ok(draft(
        json!({"kind":"staticAbility","source":self_ref(),"activeWhile":active_while_battlefield(),"modifiers":[{"kind":"castLimitPerTurn","players":players,"where":filter,"maximum":integer(maximum)}]}),
        &[
            "Compose a shared player population, numeric expression and spell query",
            "Preserve the maximum and each-turn period in one atomic casting quota",
            "Bound lowering to the runtime's each-player and cast-history filter contract",
        ],
    ))
}

#[cfg(test)]
#[path = "cast_quota_tests.rs"]
mod tests;
