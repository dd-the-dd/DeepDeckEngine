use super::super::{MachineFailure, OracleTokenCursor};
use super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ExplicitTokenCreationNode {
    pub player: LocalPlayerReference,
    pub body: TokenCreationBodyNode,
}

/// What a `create` verb creates, independent of who performs it: the
/// controller's imperative and a player subject's `creates` share it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TokenCreationBodyNode {
    pub quantity: NumericExpression,
    pub template: ExplicitTokenTemplateNode,
    pub keywords: Vec<GrantedKeyword>,
    /// Complete quoted abilities of the created tokens (CR 113.1).
    pub abilities: Vec<QuotedAbilityNode>,
    pub tapped: bool,
    /// The token's name when the effect gives one (CR 111.4): `NAME, a ...`.
    pub name: Option<String>,
    /// The `legendary` supertype of the created token (CR 205.4d).
    pub legendary: bool,
}

/// `NAME, a|an`: a proper name given by the effect before the article of a
/// single token description (CR 111.4). The name is read up to its comma.
fn recognize_token_proper_name(
    cursor: &mut OracleTokenCursor<'_>,
) -> super::super::MachineResult<Option<String>> {
    cursor.enter("token.creation.proper-name");
    let tokens = cursor.token_slice(cursor.position(), usize::MAX);
    let Some(comma) = tokens.iter().position(|token| token.raw == ",") else {
        return Ok(None);
    };
    let words = &tokens[..comma];
    let is_name = !words.is_empty()
        && words[0]
            .raw
            .starts_with(|character: char| character.is_ascii_uppercase())
        && words.iter().all(|token| {
            token.raw.chars().any(char::is_alphabetic)
                && token.raw.chars().all(|character| {
                    character.is_alphanumeric() || "'\u{2019}-".contains(character)
                })
        })
        && tokens.get(comma + 1).is_some_and(|token| {
            token.raw.eq_ignore_ascii_case("a") || token.raw.eq_ignore_ascii_case("an")
        });
    if !is_name {
        return Ok(None);
    }
    let name = words
        .iter()
        .map(|token| token.raw.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    cursor.advance_to(cursor.position() + comma + 2)?;
    Ok(Some(name))
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
    let player = if crate::oracle::canonical::automata::lemma_at(
        &trial,
        VerbLemma::Create,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    ) {
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
    let head = match recognize_token_body_head(&mut trial, antecedent) {
        Ok(Some(head)) => head,
        Ok(None) => {
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
        Err(f) => return TokenCreationAttempt::FailedCommitted(f),
    };
    if player != LocalPlayerReference::Controller {
        // This old typed body is the input of parents that capture the
        // referenced controller BEFORE destroying/countering the object.
        // Pass the parsed node and ending cursor, never recompute its player
        // through the ordinary controller-only lowerer after that departure.
        trial.enter("token.creation.referenced-controller-body");
        if head.tapped || head.legendary || head.name.is_some() {
            return TokenCreationAttempt::FailedCommitted(
                trial.fail("an untapped token for the existing referenced-controller parent"),
            );
        }
        let atom = match head.quantity {
            NumericExpression::Atom(NumericAtom::Integer(n))
                if n > 0 && i32::try_from(n).is_ok() => NumericAtom::Integer(n),
            NumericExpression::Atom(NumericAtom::VariableX) => NumericAtom::VariableX,
            _ => return TokenCreationAttempt::FailedCommitted(trial.fail(
                "an existing referenced-controller token quantity: positive i32 literal or declared X")),
        };
        if head.template.types.as_slice() != [ObjectCardType::Creature] {
            return TokenCreationAttempt::FailedCommitted(trial.fail(
                "a Creature-only template representable by the existing controller-snapshot parent",
            ));
        }
        *cursor = trial;
        return TokenCreationAttempt::Recognized(LocalEffectNode::CreateTokens(
            CreateTokensEffectNode {
                player,
                quantity: atom,
                token: head.template.creature,
            },
        ));
    }
    match recognize_token_body_tail(&mut trial, head) {
        Ok(body) => {
            *cursor = trial;
            TokenCreationAttempt::Recognized(LocalEffectNode::CreateExplicitTokens(
                ExplicitTokenCreationNode { player, body },
            ))
        }
        Err(f) => TokenCreationAttempt::FailedCommitted(f),
    }
}

/// The written token description after a performer's `create(s)` verb
/// (CR 111.1): quantity, characteristics and granted abilities. A catalog
/// token (`a Treasure token`) is not an explicit description and is refused
/// at the description state.
pub(in crate::oracle::canonical) fn recognize_performed_token_creation_body(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> super::super::MachineResult<TokenCreationBodyNode> {
    let Some(head) = recognize_token_body_head(cursor, antecedent)? else {
        cursor.enter("token.creation.performed.description");
        return Err(cursor.fail(
            "an explicit token description after a player subject's verb (a catalog token is not read for a player subject)",
        ));
    };
    recognize_token_body_tail(cursor, head)
}

/// The description up to its `token(s)` noun; `Ok(None)` when no explicit
/// descriptor starts (the catalog reader's input).
fn recognize_token_body_head(
    trial: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> super::super::MachineResult<Option<TokenCreationBodyNode>> {
    let name = recognize_token_proper_name(trial)?;
    trial.enter("token.creation.quantity");
    let quantity = if name.is_some() {
        NumericExpression::from(1)
    } else if trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
    {
        trial
            .advance_to(trial.position() + 1)
            .expect("one present article");
        NumericExpression::from(1)
    } else {
        recognize_numeric_expression(trial, antecedent)?
    };
    let tapped = recognize_token_entry_state(trial)?;
    trial.enter("token.creation.supertype");
    let legendary = trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("legendary"))
        && trial
            .token_slice(trial.position() + 1, trial.position() + 2)
            .first()
            .is_some_and(|token| token.token_type == "counterValue");
    if legendary {
        trial
            .advance_to(trial.position() + 1)
            .expect("one present supertype");
    }
    let template = match try_recognize_token_descriptor(trial) {
        TokenDescriptorAttempt::Recognized(node) => node,
        TokenDescriptorAttempt::FailedCommitted(f) => return Err(f),
        TokenDescriptorAttempt::NotClaimed => return Ok(None),
    };
    trial.enter("token.creation.noun");
    let singular = matches!(quantity, NumericExpression::Atom(NumericAtom::Integer(1)));
    let noun = if singular { "token" } else { "tokens" };
    trial.consume(noun)?;
    Ok(Some(TokenCreationBodyNode {
        quantity,
        template,
        keywords: Vec::new(),
        abilities: Vec::new(),
        tapped,
        name,
        legendary,
    }))
}

/// The granted abilities after the noun: a `with` list and a following
/// `It has|They have` characteristic sentence (CR 111.4).
fn recognize_token_body_tail(
    trial: &mut OracleTokenCursor<'_>,
    head: TokenCreationBodyNode,
) -> super::super::MachineResult<TokenCreationBodyNode> {
    let singular = matches!(
        head.quantity,
        NumericExpression::Atom(NumericAtom::Integer(1))
    );
    let mut granted = if trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("with"))
    {
        trial.consume("with")?;
        recognize_effect_owned_granted_ability_list(trial, GRANTED_OBJECT_TYPE_LINE)?
    } else {
        GrantedAbilityListNode {
            keywords: Vec::new(),
            abilities: Vec::new(),
        }
    };
    // `... tokens. They have ABILITIES` / `... token. It has ABILITIES`: the
    // next sentence states abilities the created tokens have, the same
    // characteristics as a `with` list (CR 111.4). A temporary `gain` grant
    // stays a separate action owned by the sequence.
    let (pronoun, verb) = if singular {
        ("it", "has")
    } else {
        ("they", "have")
    };
    if trial.peek() == Some(".")
        && trial
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(pronoun))
        && trial
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(verb))
    {
        trial.enter("token.creation.characteristic-sentence");
        trial.consume_phrase(&[".", pronoun, verb])?;
        let more = recognize_effect_owned_granted_ability_list(trial, GRANTED_OBJECT_TYPE_LINE)?;
        granted.keywords.extend(more.keywords);
        granted.abilities.extend(more.abilities);
    }
    trial.enter("token.creation.complete");
    Ok(TokenCreationBodyNode {
        keywords: granted.keywords,
        abilities: granted.abilities,
        ..head
    })
}
