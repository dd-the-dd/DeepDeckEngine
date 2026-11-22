use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ActivationCostSequenceNode, AddManaEffectNode, CreateTokensEffectNode, CreatureTokenSpecNode,
    LocalPlayerReference, NumericAtom, PlayerPredicate, TargetBinding, TargetCardinality,
    TargetListNode, TargetSelector, recognize_activation_cost_sequence, recognize_add_mana_effect,
    recognize_numeric_atom, recognize_target_list,
};
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EmbeddedTokenManaAbilityNode {
    pub costs: ActivationCostSequenceNode,
    pub mana: AddManaEffectNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetedCreateCreatureTokensWithManaAbilityNode {
    pub target: TargetListNode,
    pub quantity: NumericAtom,
    pub token: CreatureTokenSpecNode,
    pub ability: EmbeddedTokenManaAbilityNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CreatureTokenBodyNode {
    quantity: NumericAtom,
    token: CreatureTokenSpecNode,
}

fn recognize_create_action(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("effect.create-token.action");
    match cursor.peek() {
        Some(raw) if is_verb_form(&raw, VerbLemma::Create, VerbForm::Base) => {
            crate::oracle::canonical::automata::consume_lemma(
                cursor,
                crate::oracle::canonical::automata::VerbLemma::Create,
                crate::oracle::canonical::automata::VerbAgreement::Base,
            )
        }
        Some(raw) if is_verb_form(&raw, VerbLemma::Create, VerbForm::ThirdPersonSingular) => {
            crate::oracle::canonical::automata::consume_lemma(
                cursor,
                crate::oracle::canonical::automata::VerbLemma::Create,
                crate::oracle::canonical::automata::VerbAgreement::ThirdPersonSingular,
            )
        }
        _ => Err(cursor.fail("`create` or `creates`")),
    }
}

fn recognize_creature_token_body(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CreatureTokenBodyNode> {
    cursor.enter("effect.create-token.quantity");
    let quantity = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
    {
        cursor.advance_to(cursor.position() + 1)?;
        NumericAtom::Integer(1)
    } else {
        let quantity = recognize_numeric_atom(cursor)?;
        cursor.require(
            !matches!(quantity,NumericAtom::Integer(n) if n<=0),
            "a positive token quantity or X",
        )?;
        quantity
    };
    let descriptor = super::recognize_creature_token_descriptor(cursor)?;
    cursor.enter("effect.create-token.types-supported");
    cursor.require(
        descriptor.types.as_slice() == [super::ObjectCardType::Creature],
        "a Creature-only descriptor in this existing typed token parent",
    )?;
    cursor.enter("effect.create-token.token-noun");
    cursor.consume(if quantity == NumericAtom::Integer(1) {
        "token"
    } else {
        "tokens"
    })?;
    cursor.enter("effect.create-token.complete");
    Ok(CreatureTokenBodyNode {
        quantity,
        token: descriptor.creature,
    })
}

/// Recognizes creation of a vanilla creature token. The effect owns the
/// recipient and quantity while the shared body child owns printed token
/// characteristics. Ability-bearing tokens use the same body recognizer and
/// then delegate their quoted rule to a typed activated-ability child.
pub(in crate::oracle::canonical) fn recognize_create_creature_tokens_effect(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<CreateTokensEffectNode> {
    recognize_create_creature_tokens_effect_with_reference_support(
        cursor,
        antecedent,
        super::TargetReferenceSupport::Unqualified,
    )
}

pub(in crate::oracle::canonical) fn recognize_create_creature_tokens_effect_with_reference_support(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
    support: super::TargetReferenceSupport,
) -> MachineResult<CreateTokensEffectNode> {
    cursor.enter("effect.create-token.player");
    let player = super::recognize_player_reference_with_support(cursor, antecedent, support)?;
    recognize_create_action(cursor)?;
    let body = recognize_creature_token_body(cursor)?;
    Ok(CreateTokensEffectNode {
        player,
        quantity: body.quantity,
        token: body.token,
    })
}

/// Recognizes the quoted activated mana rule carried by a token definition.
/// Quotes and the inner sentence terminator belong to this envelope; cost and
/// mana production remain delegated to their ordinary shared automata.
pub(in crate::oracle::canonical) fn recognize_embedded_token_mana_ability(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EmbeddedTokenManaAbilityNode> {
    cursor.enter("token.ability.quote-open");
    cursor.consume("\"")?;
    cursor.enter("token.ability.costs");
    let costs = recognize_activation_cost_sequence(cursor)?;
    cursor.enter("token.ability.separator");
    cursor.consume(":")?;
    cursor.enter("token.ability.mana");
    let mana = recognize_add_mana_effect(cursor)?;
    cursor.enter("token.ability.terminator");
    cursor.consume(".")?;
    cursor.enter("token.ability.quote-close");
    cursor.consume("\"")?;
    cursor.enter("token.ability.complete");
    Ok(EmbeddedTokenManaAbilityNode { costs, mana })
}

/// Recognizes `target PLAYER creates TOKEN_DESCRIPTION with "ACTIVATED MANA
/// ABILITY"` from one token stream. The target, token body, cost sequence, and
/// mana output stay independent typed children for use by modal and ordinary
/// spell envelopes alike.
pub(in crate::oracle::canonical) fn recognize_targeted_create_creature_tokens_with_mana_ability(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetedCreateCreatureTokensWithManaAbilityNode> {
    cursor.enter("effect.targeted-create-token.target");
    let target = recognize_target_list(cursor)?;
    cursor.require(
        matches!(target.slots.as_slice(), [slot]
            if slot.cardinality == TargetCardinality::EXACTLY_ONE
                && matches!(slot.selector,
                    TargetSelector::Player(PlayerPredicate::Any | PlayerPredicate::Opponent))),
        "exactly one target player or opponent",
    )?;
    recognize_create_action(cursor)?;
    let body = recognize_creature_token_body(cursor)?;
    cursor.enter("effect.targeted-create-token.ability-relation");
    cursor.consume("with")?;
    let ability = recognize_embedded_token_mana_ability(cursor)?;
    cursor.enter("effect.targeted-create-token.complete");
    Ok(TargetedCreateCreatureTokensWithManaAbilityNode {
        target,
        quantity: body.quantity,
        token: body.token,
        ability,
    })
}

#[cfg(test)]
#[path = "token_tests.rs"]
mod tests;
