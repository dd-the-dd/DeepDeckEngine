use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims tap-led any-color mana with a following spend restriction tied to a
/// chosen characteristic. The production and restriction children own all
/// subsequent semantic validation and committed failures.
pub(in crate::oracle::canonical) fn claims_promoted_restricted_mana_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() >= 11
        && tokens
            .first()
            .is_some_and(|token| token.token_type == "tapSymbol")
        && tokens.get(1).is_some_and(|token| token.raw == ":")
        && tokens
            .get(2)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("add"))
        && tokens.iter().any(|token| {
            token.token_type == "selectionReferenceWord" && token.raw.eq_ignore_ascii_case("chosen")
        })
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("can't"))
        && tokens
            .windows(2)
            .any(|window| window[0].raw == "." && window[1].raw.eq_ignore_ascii_case("spend"))
}

pub(in crate::oracle::canonical) fn parse_promoted_restricted_mana(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("restricted-mana-ability", input.tokens);
    let ability = recognize_restricted_mana_ability(&mut cursor)?;
    let TapSelfCostNode = ability.cost;
    cursor.enter("activation.restricted-mana.binding-validation");
    let (usage, alternatives) = match &ability.restriction.constraint {
        ManaSpendConstraintNode::Only {
            usage,
            alternatives,
        } => (usage, alternatives),
    };
    cursor.require(alternatives.is_empty(), "one cast-spell spend restriction")?;
    let ManaSpendUsageNode::CastSpell { spell } = usage;
    cursor.require(
        spell.binding == Some(CharacteristicChoiceBinding::CreatureType),
        "a creature-type choice binding",
    )?;
    let where_predicate = lower_spell_object_predicate(&spell.predicate)
        .ok_or_else(|| cursor.fail("a nonempty spell predicate"))?;
    let cant_be_countered = ability
        .restriction
        .consequences
        .contains(&ManaSpendConsequenceNode::SpellCantBeCountered);
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "manaAbility",
            "source": self_ref(),
            "costs": [{
                "kind": "tap",
                "object": self_ref(),
            }],
            "effects": [{
                "kind": "addMana",
                "player": {
                    "kind": "controllerOf",
                    "object": self_ref(),
                },
                "mana": {
                    "kind": "chooseColor",
                    "amount": ability.effect.amount,
                    "spendRestriction": {
                        "kind": "castSpell",
                        "where": where_predicate,
                        "chosenCreatureTypeFromSource": true,
                        "cantBeCountered": cant_be_countered,
                    },
                },
            }],
        }),
        &[
            "Recognize one tap-cost activation envelope",
            "Delegate unrestricted color production to the shared mana child",
            "Bind a cast-spell predicate to the stored creature-type choice",
            "Retain the independent uncounterability consequence",
            "Lower to tap and addMana primitives with typed spend metadata",
            &trace,
        ],
    ))
}
