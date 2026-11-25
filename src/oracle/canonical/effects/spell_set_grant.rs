//! Lowers a static keyword grant to a spell set (`automata/spell_set_grant.rs`)
//! to the engine's spell-grant modifiers. The spell set is the shared spell
//! predicate cast by the ability controller; each keyword names the one
//! engine modifier that evaluates it while that spell is cast (CR 601.2,
//! 702.51 convoke, 702.126 improvise, 702.78 conspire, 702.56 replicate,
//! 702.61 split second, 702.85 cascade). A keyword without such a modifier
//! fails explicitly.
use super::super::*;
use crate::oracle::{
    characteristic_vocabulary::type_line_has_permanent_card_type, syntax::AbilityInput,
};

pub(in crate::oracle::canonical) enum SpellSetGrantParseAttempt {
    NotClaimed,
    Parsed(CanonicalRuleDraft),
    Failed(MachineFailure),
}

pub(in crate::oracle::canonical) fn try_parse_spell_set_grant(
    input: &AbilityInput<'_>,
) -> SpellSetGrantParseAttempt {
    if !type_line_has_permanent_card_type(input.face_type_line) {
        return SpellSetGrantParseAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("static-spell-grant", input.tokens);
    match try_recognize_spell_set_grant(&mut cursor) {
        SpellSetGrantAttempt::NotClaimed => SpellSetGrantParseAttempt::NotClaimed,
        SpellSetGrantAttempt::Failed(failure) => SpellSetGrantParseAttempt::Failed(failure),
        SpellSetGrantAttempt::Parsed(node) => match lower_spell_set_grant(&mut cursor, &node) {
            Ok(draft) => SpellSetGrantParseAttempt::Parsed(draft),
            Err(failure) => SpellSetGrantParseAttempt::Failed(failure),
        },
    }
}

/// The spells a grant applies to: the controller's spells matching the
/// shared predicate (`{"kind":"spells"}`, the engine's spell-grant subject).
fn granted_spells(filter: &Value) -> Value {
    json!({ "kind": "spells", "controller": controller(), "where": filter })
}

pub(in crate::oracle::canonical) fn lower_spell_set_grant(
    cursor: &mut OracleTokenCursor<'_>,
    node: &SpellSetGrantNode,
) -> MachineResult<CanonicalRuleDraft> {
    cursor.enter("static.spell-grant.caster");
    // The engine grants to the spells its source's controller casts.
    cursor.require(
        node.spells.caster == Some(SpellCasterScope::AbilityController),
        "spells cast by the ability controller (`you cast`)",
    )?;
    cursor.enter("static.spell-grant.filter");
    cursor.require(
        domain_filter_is_representable(&node.spells.predicate),
        "a spell filter preserving Boolean domain identities",
    )?;
    let filter = lower_spell_object_predicate(&node.spells.predicate).unwrap_or(Value::Null);
    cursor.enter("static.spell-grant.keyword-modifier");
    let modifier = match &node.keyword {
        SpellGrantedKeyword::Parameterless(GrantedKeyword::Convoke) => {
            json!({ "kind": "grantConvoke", "spells": granted_spells(&filter) })
        }
        SpellGrantedKeyword::Parameterless(GrantedKeyword::Improvise) => {
            json!({ "kind": "grantImprovise", "spells": granted_spells(&filter) })
        }
        SpellGrantedKeyword::Conspire => {
            json!({ "kind": "grantConspire", "spells": granted_spells(&filter) })
        }
        SpellGrantedKeyword::Replicate(cost) => {
            cost.fixed_runtime_mana_value()
                .map_err(|error| cursor.fail(error.expected()))?;
            json!({
                "kind": "grantReplicate",
                "spells": granted_spells(&filter),
                "cost": { "kind": "payMana", "manaCost": cost.as_oracle_cost() },
            })
        }
        // CR 702.61a: while such a spell is on the stack.
        SpellGrantedKeyword::Parameterless(GrantedKeyword::SplitSecond) => json!({
            "kind": "grantKeywordToSpells",
            "player": controller(),
            "where": filter,
            "keyword": "splitSecond",
        }),
        // CR 702.85a: a cascade trigger of each matching spell cast.
        SpellGrantedKeyword::Parameterless(GrantedKeyword::Cascade) => json!({
            "kind": "grantCascadeToSpells",
            "player": controller(),
            "minimumManaValue": 0,
            "where": filter,
        }),
        SpellGrantedKeyword::Parameterless(_) => {
            return Err(cursor.fail(
                "a keyword the engine grants to spells (convoke, improvise, conspire, replicate, split second, cascade)",
            ));
        }
    };
    let transition_trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [modifier],
        }),
        &[
            "Read the granted spells with the shared spell-cast collection and its caster",
            "Read the granted keyword as one typed atom",
            "Lower the keyword to the one spell-grant modifier the engine evaluates",
            &transition_trace,
        ],
    ))
}

#[cfg(test)]
#[path = "spell_set_grant_tests.rs"]
mod tests;
