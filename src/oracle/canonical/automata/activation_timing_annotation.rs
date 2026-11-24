//! Parenthesized activation explanations are typed metadata, never effects.
use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActivationTimingAnnotation {
    InstantTimingExplanation,
}

#[derive(Clone, Copy)]
enum State {
    Open,
    Action,
    Restriction,
    Relation,
    Article,
    Timing,
    Period,
    Close,
    Complete,
}

/// The parent must prove that the resulting ability uses the stack. A mana
/// ability cannot acquire instant timing merely by discarding this annotation.
pub(in crate::oracle::canonical) fn recognize_activation_timing_annotation(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActivationTimingAnnotation> {
    let mut state = State::Open;
    loop {
        state = match state {
            State::Open => {
                cursor.enter("activation.annotation.open");
                cursor.consume("(")?;
                State::Action
            }
            State::Action => {
                cursor.enter("activation.annotation.action");
                cursor.consume("activate")?;
                State::Restriction
            }
            State::Restriction => {
                cursor.enter("activation.annotation.restriction");
                cursor.consume("only")?;
                State::Relation
            }
            State::Relation => {
                cursor.enter("activation.annotation.relation");
                cursor.consume("as")?;
                State::Article
            }
            State::Article => {
                cursor.enter("activation.annotation.article");
                cursor.consume("an")?;
                State::Timing
            }
            State::Timing => {
                cursor.enter("activation.annotation.timing");
                cursor.consume("instant")?;
                State::Period
            }
            State::Period => {
                cursor.enter("activation.annotation.period");
                cursor.consume(".")?;
                State::Close
            }
            State::Close => {
                cursor.enter("activation.annotation.close");
                cursor.consume(")")?;
                State::Complete
            }
            State::Complete => return Ok(ActivationTimingAnnotation::InstantTimingExplanation),
        }
    }
}

/// What a parenthetical after an activated ability's restrictions explains.
/// Reminder text has no rules meaning (CR 207.2), but it must belong to a
/// symbol or mechanic the ability actually uses; any other parenthetical
/// keeps its diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AbilityReminderOwner {
    /// A glossary of a mana symbol printed in the ability (`{C}`, `{S}`,
    /// Phyrexian mana), validated against that symbol.
    ManaSymbol,
    /// The named token the ability creates, validated by its reminder child.
    NamedToken,
    /// A keyword action the program performs, or a catalog keyword whose
    /// reminder grammar is registered and which the ability names.
    Mechanic,
}

fn is_word(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// Reads one reminder parenthetical whose owner is proved by the ability
/// tokens before it (`0..cursor.position()`) or by its last effect.
pub(in crate::oracle::canonical) fn recognize_owned_ability_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    effects: &super::EffectSequenceNode,
) -> MachineResult<AbilityReminderOwner> {
    let start = cursor.position();
    let before = cursor.token_slice(0, start);
    if let Some(super::LocalEffectNode::CreateNamedTokens(creation)) = effects.effects.last() {
        let checkpoint = cursor.checkpoint();
        match super::recognize_named_token_creation_reminder(cursor, &creation.creation) {
            Ok(()) => return Ok(AbilityReminderOwner::NamedToken),
            Err(_) => cursor.rollback(checkpoint),
        }
    }
    // A keyword action with a typed reminder grammar validates its reminder
    // against the action it explains, instead of being skipped as text.
    if let Some(super::LocalEffectNode::Earthbend(action)) = effects.effects.last() {
        return super::recognize_earthbend_action_reminder(cursor, &action.quantity)
            .map(|_| AbilityReminderOwner::Mechanic);
    }
    cursor.enter("activation.reminder.open");
    cursor.consume("(")?;
    if let Some(symbol) = cursor
        .peek_token()
        .filter(|token| token.token_type == "manaSymbol")
        .map(|token| token.raw.clone())
    {
        cursor.enter("activation.reminder.symbol");
        cursor.require(
            before.iter().any(|token| {
                token.token_type == "manaSymbol" && token.raw.eq_ignore_ascii_case(&symbol)
            }),
            "a reminder for a mana symbol this ability uses",
        )?;
        recognize_mana_symbol_glossary(cursor, &symbol)?;
        cursor.enter("activation.reminder.close");
        cursor.consume(")")?;
        return Ok(AbilityReminderOwner::ManaSymbol);
    }
    cursor.enter("activation.reminder.mechanic");
    cursor.require(
        effects.effects.iter().any(performs_reminded_keyword_action)
            || names_reminded_catalog_mechanic(before),
        "a reminder explaining a symbol, token or keyword this ability uses",
    )?;
    let mut depth = 1usize;
    while depth > 0 {
        match cursor.peek() {
            None => return Err(cursor.fail("a closing parenthesis for the reminder text")),
            Some("(") => depth += 1,
            Some(")") => depth -= 1,
            Some(_) => {}
        }
        cursor.advance_to(cursor.position() + 1)?;
    }
    Ok(AbilityReminderOwner::Mechanic)
}

/// A keyword action of the program whose definition the printed reminder
/// restates (CR 701.49 venture, 701.57 discover, 701.44 explore, 701.50
/// connive, 701.25 surveil, ...): the typed action itself is the evidence.
fn performs_reminded_keyword_action(effect: &super::LocalEffectNode) -> bool {
    use super::LocalEffectNode as Effect;
    match effect {
        Effect::Surveil(_)
        | Effect::ControllerKeywordAction(_)
        | Effect::PermanentKeywordAction(_) => true,
        Effect::PlayerAction(action) => {
            matches!(action.action, super::SingularPlayerAction::Venture(_))
        }
        _ => false,
    }
}

/// A keyword ability or keyword action named in the ability whose catalog
/// entry registers a reminder grammar (`fear`, `menace`, `vigilance`,
/// `indestructible`, `exalted`, `investigate`, ...). Ordinary actions such as
/// `destroy` or `exile` have no reminder and are never evidence.
fn names_reminded_catalog_mechanic(tokens: &[crate::oracle::model::RecognizedEntity]) -> bool {
    use crate::oracle::ability_catalog::{
        AbilityGrammarRole, lookup_registry, match_catalog_mechanic,
    };
    (0..tokens.len()).any(|position| {
        match_catalog_mechanic(tokens, position)
            .and_then(|matched| lookup_registry(matched.canonical_id))
            .is_some_and(|entry| {
                entry
                    .grammar
                    .iter()
                    .any(|grammar| grammar.role == AbilityGrammarRole::Reminder)
            })
    })
}

/// `{C} represents colorless mana.` (CR 107.4c), `{S} can be paid with one
/// mana from a snow source.` (CR 107.4h) or a Phyrexian symbol's payment
/// alternatives (CR 107.4f). The closing parenthesis stays with the parent.
fn recognize_mana_symbol_glossary(
    cursor: &mut OracleTokenCursor<'_>,
    symbol: &str,
) -> MachineResult<()> {
    let inner = symbol
        .trim_start_matches('{')
        .trim_end_matches('}')
        .to_ascii_uppercase();
    if inner.ends_with("/P") {
        let mut depth = 0usize;
        let close = (0..)
            .find(|offset| match cursor.peek_ahead(*offset) {
                None => true,
                Some("(") => {
                    depth += 1;
                    false
                }
                Some(")") if depth == 0 => true,
                Some(")") => {
                    depth -= 1;
                    false
                }
                Some(_) => false,
            })
            .unwrap_or(0);
        let end = cursor.position() + close;
        return cursor.bounded(end, |cursor| {
            super::recognize_phyrexian_mana_reminder(cursor).map(|_| ())
        });
    }
    let glossary: &[&str] = match inner.as_str() {
        "C" => &["represents", "colorless", "mana", "."],
        "S" => &[
            "can", "be", "paid", "with", "one", "mana", "from", "a", "snow", "source", ".",
        ],
        _ => return Err(cursor.fail("a registered mana-symbol glossary ({C}, {S}, Phyrexian)")),
    };
    cursor.advance_to(cursor.position() + 1)?;
    cursor.consume_phrase(glossary)
}

/// The reminder of an activation keyword restates the restrictions of its
/// definition: `(Activate only if this creature attacked this turn and only
/// once each turn.)`, `(Activate each exhaust ability only once.)`. It is
/// read by the restriction grammar and must equal the definition; the
/// definition, not the reminder, supplies the semantics.
pub(in crate::oracle::canonical) fn recognize_activation_keyword_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    keyword: super::ActivationKeyword,
) -> MachineResult<()> {
    cursor.enter("activation.keyword-reminder.open");
    cursor.consume("(")?;
    let restated = super::recognize_activation_restriction_sentence(cursor, Some(keyword))?;
    if keyword == super::ActivationKeyword::PowerUp {
        // CR 702.193a: the power-up cost reduction, restated.
        cursor.enter("activation.keyword-reminder.power-up-reduction");
        cursor.consume_phrase(&[
            "reduce", "the", "cost", "by", "its", "mana", "cost", "if", "it", "entered", "this",
            "turn", ".",
        ])?;
    }
    cursor.enter("activation.keyword-reminder.close");
    cursor.consume(")")?;
    let mut defined = keyword.defined_restrictions();
    if matches!(
        keyword,
        super::ActivationKeyword::Exhaust | super::ActivationKeyword::PowerUp
    ) {
        defined.class_scope = Some(keyword);
    }
    cursor.enter("activation.keyword-reminder.definition");
    cursor.require(
        restated == defined,
        "a reminder that restates the keyword's defined activation restrictions",
    )
}

/// Whether `(Activate ...)` or `(Any player may activate ...)` starts here.
pub(in crate::oracle::canonical) fn parenthesized_restriction_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor.peek() == Some("(") && (is_word(cursor, 1, "activate") || is_word(cursor, 1, "any"))
}

/// Whether the exact instant-timing annotation `(Activate only as an
/// instant.)` starts here.
pub(in crate::oracle::canonical) fn instant_timing_annotation_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor.peek() == Some("(")
        && ["activate", "only", "as", "an", "instant"]
            .iter()
            .enumerate()
            .all(|(offset, word)| is_word(cursor, offset + 1, word))
}
