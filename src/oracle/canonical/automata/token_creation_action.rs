use super::super::{MachineFailure, OracleTokenCursor};
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ExplicitTokenCreationNode {
    pub player: LocalPlayerReference,
    pub quantity: NumericExpression,
    pub template: ExplicitTokenTemplateNode,
    pub keywords: Vec<GrantedKeyword>,
    /// Complete quoted abilities of the created tokens (CR 113.1).
    pub abilities: Vec<QuotedAbilityNode>,
    pub tapped: bool,
}

#[cfg(test)]
#[path = "token_creation_action_tests.rs"]
mod tests;

pub(in crate::oracle::canonical) enum TokenCreationAttempt {
    NotClaimed,
    Recognized(LocalEffectNode),
    FailedCommitted(MachineFailure),
}

/// A trial owns transient actor/quantity/template slots. Named and explicit
/// definitions are different typed outcomes; only a completed child commits.
pub(in crate::oracle::canonical) fn try_recognize_token_creation_action(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> TokenCreationAttempt {
    let mut trial = cursor.clone();
    let player = if trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("create"))
    {
        LocalPlayerReference::Controller
    } else {
        match recognize_player_reference(&mut trial, antecedent) {
            Ok(player) => player,
            Err(_) => return TokenCreationAttempt::NotClaimed,
        }
    };
    let verb = match player {
        LocalPlayerReference::Controller => "create",
        _ => "creates",
    };
    if !trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case(verb))
    {
        return TokenCreationAttempt::NotClaimed;
    }
    trial.enter("token.creation.action");
    if let Err(f) = trial.consume(verb) {
        return TokenCreationAttempt::FailedCommitted(f);
    }
    trial.enter("token.creation.quantity");
    let quantity = if trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
    {
        trial
            .advance_to(trial.position() + 1)
            .expect("one present article");
        NumericExpression::from(1)
    } else {
        match recognize_numeric_expression(&mut trial, antecedent) {
            Ok(quantity) => quantity,
            Err(f) => return TokenCreationAttempt::FailedCommitted(f),
        }
    };
    let tapped = match recognize_token_entry_state(&mut trial) {
        Ok(tapped) => tapped,
        Err(f) => return TokenCreationAttempt::FailedCommitted(f),
    };
    let template = match try_recognize_token_descriptor(&mut trial) {
        TokenDescriptorAttempt::Recognized(node) => node,
        TokenDescriptorAttempt::FailedCommitted(f) => {
            return TokenCreationAttempt::FailedCommitted(f);
        }
        TokenDescriptorAttempt::NotClaimed => {
            // Reuse the complete catalog reader and all of its existing
            // quantity/list/reminder rules. This is an explicit alternative,
            // not recovery after an explicit descriptor has committed.
            return match recognize_create_named_tokens_effect(cursor, antecedent) {
                Ok(node) => {
                    TokenCreationAttempt::Recognized(LocalEffectNode::CreateNamedTokens(node))
                }
                Err(f) => TokenCreationAttempt::FailedCommitted(f),
            };
        }
    };
    trial.enter("token.creation.noun");
    let singular = matches!(quantity, NumericExpression::Atom(NumericAtom::Integer(1)));
    let noun = if singular { "token" } else { "tokens" };
    if let Err(f) = trial.consume(noun) {
        return TokenCreationAttempt::FailedCommitted(f);
    }
    if player != LocalPlayerReference::Controller {
        // This old typed body is the input of parents that capture the
        // referenced controller BEFORE destroying/countering the object.
        // Pass the parsed node and ending cursor, never recompute its player
        // through the ordinary controller-only lowerer after that departure.
        trial.enter("token.creation.referenced-controller-body");
        if tapped {
            return TokenCreationAttempt::FailedCommitted(
                trial.fail("an untapped token for the existing referenced-controller parent"),
            );
        }
        let atom = match quantity {
            NumericExpression::Atom(NumericAtom::Integer(n))
                if n > 0 && i32::try_from(n).is_ok() => NumericAtom::Integer(n),
            NumericExpression::Atom(NumericAtom::VariableX) => NumericAtom::VariableX,
            _ => return TokenCreationAttempt::FailedCommitted(trial.fail(
                "an existing referenced-controller token quantity: positive i32 literal or declared X")),
        };
        if template.types.as_slice() != [ObjectCardType::Creature] {
            return TokenCreationAttempt::FailedCommitted(trial.fail(
                "a Creature-only template representable by the existing controller-snapshot parent",
            ));
        }
        *cursor = trial;
        return TokenCreationAttempt::Recognized(LocalEffectNode::CreateTokens(
            CreateTokensEffectNode {
                player,
                quantity: atom,
                token: template.creature,
            },
        ));
    }
    let granted = if trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("with"))
    {
        if let Err(f) = trial.consume("with") {
            return TokenCreationAttempt::FailedCommitted(f);
        }
        match recognize_effect_owned_granted_ability_list(&mut trial, GRANTED_OBJECT_TYPE_LINE) {
            Ok(granted) => granted,
            Err(f) => return TokenCreationAttempt::FailedCommitted(f),
        }
    } else {
        GrantedAbilityListNode {
            keywords: Vec::new(),
            abilities: Vec::new(),
        }
    };
    trial.enter("token.creation.complete");
    *cursor = trial;
    TokenCreationAttempt::Recognized(LocalEffectNode::CreateExplicitTokens(
        ExplicitTokenCreationNode {
            player,
            quantity,
            template,
            keywords: granted.keywords,
            abilities: granted.abilities,
            tapped,
        },
    ))
}
