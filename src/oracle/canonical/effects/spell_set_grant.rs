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
        SpellSetGrantAttempt::Parsed(node) => {
            let lowered = match &node.predicate {
                // CR 702.41a: affinity is a cost reduction of each matching
                // spell, counted over the quality's permanents its caster
                // controls: the shared spell-set casting reduction.
                SpellSetPredicate::Has(SpellGrantedKeyword::Affinity(quality)) => {
                    lower_granted_affinity(input, &mut cursor, &node, quality)
                }
                _ => lower_spell_set_grant(&mut cursor, &node),
            };
            match lowered {
                Ok(draft) => SpellSetGrantParseAttempt::Parsed(draft),
                Err(failure) => SpellSetGrantParseAttempt::Failed(failure),
            }
        }
    }
}

/// The spells a grant applies to: the controller's spells matching the
/// shared predicate (`{"kind":"spells"}`, the engine's spell-grant subject).
fn granted_spells(filter: &Value) -> Value {
    json!({ "kind": "spells", "controller": controller(), "where": filter })
}

fn lower_granted_affinity(
    input: &AbilityInput<'_>,
    cursor: &mut OracleTokenCursor<'_>,
    node: &SpellSetGrantNode,
    quality: &ObjectPredicate,
) -> MachineResult<CanonicalRuleDraft> {
    cursor.enter("static.spell-grant.affinity");
    cursor.require(
        node.scope == SpellSetScope::Each,
        "affinity granted to every matching spell (no ordinal or stack-control scope)",
    )?;
    let expansion = affinity_reduction(quality);
    lower_casting_cost_reduction(
        input,
        cursor,
        CastingCostReductionNode {
            subject: CastingReductionSubject::SpellSet(node.spells.clone()),
            amount: expansion.amount,
            condition: None,
            target_query: None,
            colored: None,
        },
    )
}

/// The keywords whose damage results the engine reads from a spell it
/// resolves (CR 702.2b, 702.15b, 702.80, 702.90): a grant to the spells a
/// player controls on the stack.
fn damage_keyword_for_spells(keyword: GrantedKeyword) -> bool {
    matches!(
        keyword,
        GrantedKeyword::Lifelink
            | GrantedKeyword::Deathtouch
            | GrantedKeyword::Wither
            | GrantedKeyword::Infect
    )
}

pub(in crate::oracle::canonical) fn lower_spell_set_grant(
    cursor: &mut OracleTokenCursor<'_>,
    node: &SpellSetGrantNode,
) -> MachineResult<CanonicalRuleDraft> {
    cursor.enter("static.spell-grant.filter");
    cursor.require(
        domain_filter_is_representable(&node.spells.predicate),
        "a spell filter preserving Boolean domain identities",
    )?;
    let filter = lower_spell_object_predicate(&node.spells.predicate).unwrap_or(Value::Null);
    let keyword = match &node.predicate {
        // CR 101.2: no player casts a matching spell; a caster relation
        // would name the prohibited players, which this subject does not.
        SpellSetPredicate::CantBeCast => {
            cursor.enter("static.spell-grant.prohibition");
            cursor.require(
                node.spells.caster.is_none(),
                "spells of every caster before `can't be cast`",
            )?;
            let mut modifier =
                json!({ "kind": "prohibitCast", "players": { "kind": "eachPlayer" } });
            if !filter.is_null() {
                modifier["where"] = filter;
            }
            return Ok(static_spell_rule(cursor, modifier));
        }
        SpellSetPredicate::Has(keyword) => keyword,
    };
    if node.scope == SpellSetScope::Controlled {
        // `SPELLS you control have KEYWORD`: the spells on the stack whose
        // controller is the ability controller.
        cursor.enter("static.spell-grant.controlled-keyword");
        let SpellGrantedKeyword::Parameterless(granted) = keyword else {
            return Err(cursor.fail("a parameterless keyword of the spells you control"));
        };
        cursor.require(
            damage_keyword_for_spells(*granted),
            "a damage keyword the engine reads from a resolving spell (lifelink, deathtouch, wither, infect)",
        )?;
        let mut modifier = json!({
            "kind": "grantKeywordToSpells",
            "player": controller(),
            "keyword": granted.canonical_name(),
        });
        modifier["where"] = filter;
        return Ok(static_spell_rule(cursor, modifier));
    }
    cursor.enter("static.spell-grant.caster");
    // The engine grants to the spells its source's controller casts.
    cursor.require(
        node.spells.caster == Some(SpellCasterScope::AbilityController),
        "spells cast by the ability controller (`you cast`)",
    )?;
    cursor.enter("static.spell-grant.keyword-modifier");
    let first_each_turn = node.scope == SpellSetScope::FirstEachTurn;
    if first_each_turn {
        // The engine tracks the cast ordinal for these grants only.
        cursor.enter("static.spell-grant.ordinal");
        cursor.require(
            matches!(
                keyword,
                SpellGrantedKeyword::Parameterless(
                    GrantedKeyword::Convoke | GrantedKeyword::Improvise | GrantedKeyword::Cascade
                )
            ),
            "a keyword whose first-spell-each-turn grant the engine tracks (convoke, improvise, cascade)",
        )?;
    }
    let mut modifier = match keyword {
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
        SpellGrantedKeyword::Parameterless(_) | SpellGrantedKeyword::Affinity(_) => {
            return Err(cursor.fail(
                "a keyword the engine grants to spells (convoke, improvise, conspire, replicate, split second, cascade)",
            ));
        }
    };
    if first_each_turn {
        // `the first SPELL you cast each turn` (CR 601.2): the grant reads
        // the caster's spells cast earlier this turn.
        modifier["firstEachTurn"] = json!(true);
    }
    Ok(static_spell_rule(cursor, modifier))
}

fn static_spell_rule(cursor: &OracleTokenCursor<'_>, modifier: Value) -> CanonicalRuleDraft {
    let transition_trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [modifier],
        }),
        &[
            "Read the spells with the shared spell-cast collection and its caster",
            "Read the granted keyword or the cast prohibition as one typed predicate",
            "Lower it to the one spell modifier the engine evaluates",
            &transition_trace,
        ],
    )
}

#[cfg(test)]
#[path = "spell_set_grant_tests.rs"]
mod tests;
