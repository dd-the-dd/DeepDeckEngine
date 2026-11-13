use super::super::{MachineResult, OracleTokenCursor};
use super::token_subtype_list::recognize_token_subtype_list;
use super::{
    ActivationCostSequenceNode, AddManaEffectNode, CardColor, CreateTokensEffectNode,
    CreatureSubtype, CreatureTokenSpecNode, LocalPlayerReference, NumericAtom, PlayerPredicate,
    TargetBinding, TargetCardinality, TargetListNode, TargetSelector, card_color_from_oracle_word,
    recognize_activation_cost_sequence, recognize_add_mana_effect,
    recognize_literal_power_toughness, recognize_numeric_atom, recognize_player_reference,
    recognize_target_list,
};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CreatureTokenBodyState {
    Quantity,
    Statistics,
    Color,
    Subtype,
    CreatureNoun,
    TokenNoun,
    Complete,
}

#[derive(Default)]
struct CreatureTokenBodyFrame {
    quantity: Option<NumericAtom>,
    power: Option<i64>,
    toughness: Option<i64>,
    colors: Vec<CardColor>,
    subtypes: Vec<CreatureSubtype>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CreatureTokenBodyNode {
    quantity: NumericAtom,
    token: CreatureTokenSpecNode,
}

fn recognize_create_action(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("effect.create-token.action");
    match cursor.peek() {
        Some(raw) if raw.eq_ignore_ascii_case("create") => cursor.consume("create"),
        Some(raw) if raw.eq_ignore_ascii_case("creates") => cursor.consume("creates"),
        _ => Err(cursor.fail("`create` or `creates`")),
    }
}

fn recognize_creature_token_body(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CreatureTokenBodyNode> {
    let mut state = CreatureTokenBodyState::Quantity;
    let mut frame = CreatureTokenBodyFrame::default();
    loop {
        state = match state {
            CreatureTokenBodyState::Quantity => {
                cursor.enter("effect.create-token.quantity");
                frame.quantity = Some(
                    if cursor.peek().is_some_and(|raw| {
                        raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an")
                    }) {
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
                    },
                );
                CreatureTokenBodyState::Statistics
            }
            CreatureTokenBodyState::Statistics => {
                cursor.enter("effect.create-token.statistics");
                let statistics = recognize_literal_power_toughness(cursor)?;
                frame.power = Some(statistics.power);
                frame.toughness = Some(statistics.toughness);
                CreatureTokenBodyState::Color
            }
            CreatureTokenBodyState::Color => {
                cursor.enter("effect.create-token.color");
                let raw = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("`colorless` or a Magic color word"))?
                    .to_string();
                if raw.eq_ignore_ascii_case("colorless") {
                    cursor.advance_to(cursor.position() + 1)?;
                    CreatureTokenBodyState::Subtype
                } else {
                    let color = card_color_from_oracle_word(&raw)
                        .ok_or_else(|| cursor.fail("`colorless` or a Magic color word"))?;
                    cursor.advance_to(cursor.position() + 1)?;
                    frame.colors.push(color);
                    CreatureTokenBodyState::Subtype
                }
            }
            CreatureTokenBodyState::Subtype => {
                cursor.enter("effect.create-token.subtype");
                frame.subtypes = recognize_token_subtype_list(cursor)?.subtypes;
                CreatureTokenBodyState::CreatureNoun
            }
            CreatureTokenBodyState::CreatureNoun => {
                cursor.enter("effect.create-token.creature-noun");
                cursor.consume("creature")?;
                CreatureTokenBodyState::TokenNoun
            }
            CreatureTokenBodyState::TokenNoun => {
                cursor.enter("effect.create-token.token-noun");
                let noun = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("`token` or `tokens`"))?
                    .to_string();
                cursor.require(
                    noun.eq_ignore_ascii_case("token") || noun.eq_ignore_ascii_case("tokens"),
                    "`token` or `tokens`",
                )?;
                let singular = matches!(frame.quantity, Some(NumericAtom::Integer(1)));
                cursor.require(
                    (singular && noun.eq_ignore_ascii_case("token"))
                        || (!singular && noun.eq_ignore_ascii_case("tokens")),
                    "token noun agreement with quantity",
                )?;
                cursor.advance_to(cursor.position() + 1)?;
                CreatureTokenBodyState::Complete
            }
            CreatureTokenBodyState::Complete => {
                cursor.enter("effect.create-token.complete");
                return Ok(CreatureTokenBodyNode {
                    quantity: cursor.require_value(frame.quantity, "a token quantity")?,
                    token: CreatureTokenSpecNode {
                        power: cursor.require_value(frame.power, "token power")?,
                        toughness: cursor.require_value(frame.toughness, "token toughness")?,
                        colors: frame.colors,
                        subtypes: frame.subtypes,
                    },
                });
            }
        };
    }
}

/// Recognizes creation of a vanilla creature token. The effect owns the
/// recipient and quantity while the shared body child owns printed token
/// characteristics. Ability-bearing tokens use the same body recognizer and
/// then delegate their quoted rule to a typed activated-ability child.
pub(in crate::oracle::canonical) fn recognize_create_creature_tokens_effect(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<CreateTokensEffectNode> {
    cursor.enter("effect.create-token.player");
    let player: LocalPlayerReference = recognize_player_reference(cursor, antecedent)?;
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
