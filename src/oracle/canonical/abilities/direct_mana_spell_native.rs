use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims a spell whose first atomic effect begins with mana production.
/// The claim stops at the stable lexical boundary; the shared effect sequence
/// and mana children own the complete grammar and any committed failure.
pub(in crate::oracle::canonical) fn claims_promoted_direct_mana_spell_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let add_action = tokens.first().is_some_and(|token| {
        token.token_type == "actionWord" && token.raw.eq_ignore_ascii_case("add")
    });
    let fixed_mana = tokens
        .get(1)
        .is_some_and(|token| token.token_type == "manaSymbol");
    let quantified_mana = tokens
        .get(1)
        .is_some_and(|token| token.token_type == "quantity")
        && tokens.get(2).is_some_and(|token| {
            token.token_type == "costWord" && token.raw.eq_ignore_ascii_case("mana")
        });

    add_action && (fixed_mana || quantified_mana)
}

/// Wraps one independently recognized AddMana effect as a spell ability.
/// Unlike an activated mana ability, this rule remains on the stack until the
/// spell resolves; the envelope therefore never emits `manaAbility`.
pub(in crate::oracle::canonical) fn parse_promoted_direct_mana_spell(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("direct-mana-spell", input.tokens);
    let sequence =
        recognize_effect_sequence(&mut cursor, input.name_links, input.name_resolution_failure)?;
    let [LocalEffectNode::AddMana(effect)] = sequence.effects.as_slice() else {
        return Err(cursor.fail("one atomic AddMana spell effect"));
    };
    cursor.require(
        sequence.connectors.is_empty(),
        "a single mana-production effect",
    )?;
    let effect = lower_add_mana_effect(effect);
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": [effect],
        }),
        &[
            "Recognize a direct mana-production spell from one shared token stream",
            "Lower the shared AddMana effect without reclassifying it as a mana ability",
            &trace,
        ],
    ))
}
