use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::{AbilityInput, is_promoted_spell_cost_increase_envelope};

/// Claims only a complete static spell-cost-increase envelope. Subject
/// characteristics remain owned by the shared object-collection automaton;
/// this token-only boundary prevents activation taxes, reductions, and
/// conditional continuations from being intercepted by this parent.
pub(in crate::oracle::canonical) fn claims_promoted_spell_cost_increase_family(
    input: &AbilityInput<'_>,
) -> bool {
    is_promoted_spell_cost_increase_envelope(input)
}

pub(in crate::oracle::canonical) fn parse_promoted_spell_cost_increase(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("static-spell-cost-increase", input.tokens);
    let node = recognize_spell_cost_increase_with_names(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;

    cursor.enter("static.spell-cost-increase.source-zone");
    cursor.require(
        type_line_has_permanent_card_type(input.face_type_line),
        "a permanent card face for a battlefield-active static ability",
    )?;

    cursor.enter("static.spell-cost-increase.lower.players");
    let players = match node.spells.caster {
        None => json!({ "kind": "eachPlayer" }),
        Some(SpellCasterScope::AbilityController) => controller(),
        Some(SpellCasterScope::OpponentOfAbilityController) => json!({
            "kind": "opponentsOf",
            "player": controller(),
        }),
        Some(SpellCasterScope::NotAbilityController | SpellCasterScope::EventPlayer) => {
            return Err(
                cursor.fail("a caster scope representable by the runtime player references")
            );
        }
    };
    let where_filter = lower_spell_object_predicate(&node.spells.predicate).unwrap_or(Value::Null);
    let spell_target = match &node.target {
        None => None,
        Some(target) => {
            cursor.enter("static.spell-cost-increase.lower.target");
            Some(lower_spell_target(&mut cursor, target)?)
        }
    };
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    let mut modifier = json!({
        "kind": "additionalCastingCost",
        "players": players,
        "where": where_filter,
        "cost": {
            "kind": "payMana",
            "manaCost": node.cost.as_oracle_cost(),
        },
    });
    if let Some(target) = spell_target {
        modifier["spellTarget"] = target;
    }
    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [modifier],
        }),
        &[
            "Recognize a typed spell collection through the shared selector automaton",
            "Retain the caster relation independently from the spell predicate",
            "Delegate the fixed additional payment to the shared mana-cost automaton",
            "Lower one atomic additional-casting-cost modifier",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "spell_cost_increase_tests.rs"]
mod tests;
