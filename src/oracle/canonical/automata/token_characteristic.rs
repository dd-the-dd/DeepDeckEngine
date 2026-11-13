use crate::oracle::characteristic_vocabulary::{AcceptedForms, lookup_card_type};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ActivationCostSequenceNode, CardColor, NumericAtom, ObjectCardType, ObjectControllerScope,
    ObjectPredicate, PowerToughnessDeltaNode, recognize_activation_cost_sequence,
    recognize_literal_power_toughness, recognize_numeric_atom, recognize_object_query,
    recognize_signed_power_toughness_delta,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CountedCharacteristicAbilityNode {
    pub delta: PowerToughnessDeltaNode,
    pub counted: ObjectPredicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CharacteristicCreatureTokenNode {
    pub power: i64,
    pub toughness: i64,
    pub colors: Vec<CardColor>,
    pub types: Vec<ObjectCardType>,
    pub subtypes: Vec<String>,
    pub ability: CountedCharacteristicAbilityNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CreateCharacteristicTokenEffectNode {
    pub quantity: NumericAtom,
    pub token: CharacteristicCreatureTokenNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CharacteristicTokenActivationNode {
    pub costs: ActivationCostSequenceNode,
    pub effect: CreateCharacteristicTokenEffectNode,
}

fn consume_quote(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<String> {
    let quote = cursor
        .peek()
        .filter(|raw| matches!(*raw, "'" | "\"" | "‘" | "’"))
        .ok_or_else(|| cursor.fail("an opening local ability quote"))?
        .to_string();
    cursor.consume(&quote)?;
    Ok(quote)
}

fn recognize_counted_characteristic_ability(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CountedCharacteristicAbilityNode> {
    cursor.enter("token.characteristic.subject");
    cursor.consume_phrase(&["this", "token", "gets"])?;
    let delta = recognize_signed_power_toughness_delta(cursor)?;
    cursor.enter("token.characteristic.counted-set");
    cursor.consume_phrase(&["for", "each"])?;
    let query = recognize_object_query(cursor)?;
    cursor.require(
        query.controller == Some(ObjectControllerScope::AbilityController),
        "a counted permanent set controlled by the token controller",
    )?;
    cursor.enter("token.characteristic.terminator");
    cursor.consume(".")?;
    Ok(CountedCharacteristicAbilityNode {
        delta,
        counted: query.predicate,
    })
}

/// Recognizes an imperative creature-token creation whose quoted local
/// characteristic ability scales its power and toughness with a reusable
/// controlled-permanent predicate. Both quote levels remain token boundaries;
/// no text is reconstructed or reparsed.
pub(in crate::oracle::canonical) fn recognize_create_characteristic_token_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CreateCharacteristicTokenEffectNode> {
    cursor.enter("effect.characteristic-token.action");
    cursor.consume("create")?;

    cursor.enter("effect.characteristic-token.quantity");
    let quantity = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
    {
        let article = cursor.peek().expect("checked article").to_string();
        cursor.consume(&article)?;
        NumericAtom::Integer(1)
    } else {
        let quantity = recognize_numeric_atom(cursor)?;
        cursor.require(
            !matches!(quantity, NumericAtom::Integer(value) if value <= 0),
            "a positive token quantity or X",
        )?;
        quantity
    };

    cursor.enter("effect.characteristic-token.statistics");
    let statistics = recognize_literal_power_toughness(cursor)?;

    cursor.enter("effect.characteristic-token.color");
    let mut colors = Vec::new();
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("colorless"))
    {
        cursor.consume("colorless")?;
    } else {
        let raw = cursor
            .peek()
            .ok_or_else(|| cursor.fail("`colorless` or a Magic color"))?
            .to_string();
        let color = super::card_color_from_oracle_word(&raw)
            .ok_or_else(|| cursor.fail("`colorless` or a Magic color"))?;
        cursor.consume(&raw)?;
        colors.push(color);
    }

    cursor.enter("effect.characteristic-token.subtypes");
    let mut subtypes = Vec::new();
    while cursor.peek().is_some_and(|raw| {
        lookup_card_type(raw, AcceptedForms::SingularOnly).is_none()
            && raw.chars().next().is_some_and(char::is_uppercase)
            && raw.chars().any(char::is_alphabetic)
    }) {
        let subtype = cursor.peek().expect("checked subtype").to_string();
        cursor.consume(&subtype)?;
        subtypes.push(subtype);
    }

    cursor.enter("effect.characteristic-token.types");
    let mut types = Vec::new();
    while let Some(card_type) = cursor
        .peek()
        .and_then(|raw| lookup_card_type(raw, AcceptedForms::SingularOnly))
    {
        let raw = cursor.peek().expect("checked card type").to_string();
        cursor.consume(&raw)?;
        types.push(card_type);
    }
    cursor.require(
        types.contains(&ObjectCardType::Creature),
        "the Creature card type in a characteristic creature-token specification",
    )?;

    cursor.enter("effect.characteristic-token.noun");
    let noun = cursor
        .peek()
        .ok_or_else(|| cursor.fail("`token` or `tokens`"))?
        .to_string();
    let singular = matches!(quantity, NumericAtom::Integer(1));
    cursor.require(
        (singular && noun.eq_ignore_ascii_case("token"))
            || (!singular && noun.eq_ignore_ascii_case("tokens")),
        "token noun agreement with quantity",
    )?;
    cursor.consume(&noun)?;

    cursor.enter("effect.characteristic-token.ability-link");
    cursor.consume("with")?;
    let quote = consume_quote(cursor)?;
    let ability = recognize_counted_characteristic_ability(cursor)?;
    cursor.enter("effect.characteristic-token.close-quote");
    cursor.consume(&quote)?;

    Ok(CreateCharacteristicTokenEffectNode {
        quantity,
        token: CharacteristicCreatureTokenNode {
            power: statistics.power,
            toughness: statistics.toughness,
            colors,
            types,
            subtypes,
            ability,
        },
    })
}

pub(in crate::oracle::canonical) fn recognize_characteristic_token_activation(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CharacteristicTokenActivationNode> {
    cursor.enter("activation.characteristic-token.costs");
    let costs = recognize_activation_cost_sequence(cursor)?;
    cursor.enter("activation.characteristic-token.separator");
    cursor.consume(":")?;
    let effect = recognize_create_characteristic_token_effect(cursor)?;
    // The quoted characteristic ability ends with the same period that
    // terminates the surrounding token-creation instruction. Oracle does not
    // print a second period after the closing local quote.
    cursor.enter("activation.characteristic-token.complete");
    cursor.finish()?;
    Ok(CharacteristicTokenActivationNode { costs, effect })
}

#[cfg(test)]
#[path = "token_characteristic_tests.rs"]
mod tests;
