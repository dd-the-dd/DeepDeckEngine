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
    /// A token, keyword action or granted keyword of the program,
    /// validated by that effect's typed reminder grammar.
    Mechanic,
    /// The glossary of a defined term the ability uses (`modified`).
    DefinedTerm,
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
    // A token, keyword action or granted keyword of the program validates the
    // reminder with its own typed grammar, instead of it being skipped.
    let checkpoint = cursor.checkpoint();
    let mechanic = recognize_program_mechanic_reminder(cursor, effects);
    if let Some(Ok(owner)) = mechanic {
        return Ok(owner);
    }
    cursor.rollback(checkpoint);
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
    if let Some((_, glossary)) = DEFINED_TERM_GLOSSARIES.iter().find(|(term, glossary)| {
        cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case(glossary[0]))
            && before
                .iter()
                .any(|token| token.raw.eq_ignore_ascii_case(term))
    }) {
        cursor.enter("activation.reminder.defined-term");
        cursor.consume_phrase(glossary)?;
        cursor.enter("activation.reminder.close");
        cursor.consume(")")?;
        return Ok(AbilityReminderOwner::DefinedTerm);
    }
    if let Some(Err(failure)) = mechanic {
        return Err(failure);
    }
    // Any other parenthetical would have to be read by the reminder grammar
    // of a mechanic the program performs or grants. Without that typed
    // grammar the text is refused, never skipped (CR 207.2 does not make an
    // unread parenthetical safe: it may carry rules text).
    cursor.enter("activation.reminder.mechanic");
    Err(cursor.fail(
        "a reminder read by the typed reminder grammar of a symbol, token or keyword action this ability uses",
    ))
}

/// Glossaries of defined terms an ability may use, each restating its rule:
/// `modified` (CR 700.9). A glossary is read only when the ability's own
/// tokens before it use the term.
const DEFINED_TERM_GLOSSARIES: &[(&str, &[&str])] = &[(
    "modified",
    &[
        "Equipment",
        ",",
        "Auras",
        "you",
        "control",
        ",",
        "and",
        "counters",
        "are",
        "modifications",
        ".",
    ],
)];

/// The program effects whose typed reminder grammar can read a trailing
/// parenthetical, last written first.
fn reminded_program_effects(effects: &super::EffectSequenceNode) -> Vec<&super::LocalEffectNode> {
    let mut found = Vec::new();
    for effect in effects.effects.iter().rev() {
        if let super::LocalEffectNode::OptionalSequence(inner) = effect {
            found.extend(reminded_program_effects(inner));
        } else {
            found.push(effect);
        }
    }
    found
}

fn token_body_keywords(body: &super::TokenCreationBodyNode) -> (&[super::GrantedKeyword], bool) {
    let plural = !matches!(
        body.quantity,
        super::NumericExpression::Atom(super::NumericAtom::Integer(1))
    );
    (&body.keywords, plural)
}

/// One effect's typed reminder child, or `None` when the effect has no
/// reminder grammar. Each child owns both parentheses.
fn recognize_effect_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    effect: &super::LocalEffectNode,
) -> Option<MachineResult<AbilityReminderOwner>> {
    use super::LocalEffectNode as Effect;
    let mechanic =
        |result: MachineResult<()>| Some(result.map(|()| AbilityReminderOwner::Mechanic));
    match effect {
        Effect::Earthbend(action) => Some(
            super::recognize_earthbend_action_reminder(cursor, &action.quantity)
                .map(|_| AbilityReminderOwner::Mechanic),
        ),
        Effect::Scry(action) => mechanic(super::recognize_scry_reminder(cursor, &action.count)),
        Effect::Surveil(action) => {
            mechanic(super::recognize_surveil_reminder(cursor, &action.count))
        }
        Effect::PlayerAction(action) => match &action.action {
            super::SingularPlayerAction::Venture(venture) => Some(
                super::recognize_venture_reminder(cursor, venture)
                    .map(|_| AbilityReminderOwner::Mechanic),
            ),
            _ => None,
        },
        Effect::Investigate(_) => mechanic(super::recognize_investigate_reminder(cursor)),
        Effect::ControllerKeywordAction(action)
            if action.action == super::ControllerKeywordAction::Discover =>
        {
            mechanic(super::recognize_discover_reminder(
                cursor,
                action.quantity.as_ref(),
            ))
        }
        Effect::PermanentKeywordAction(action) => match action.action {
            super::PermanentKeywordAction::Explore => {
                mechanic(super::recognize_explore_reminder(cursor, &action.targets))
            }
            super::PermanentKeywordAction::Connive => mechanic(super::recognize_connive_reminder(
                cursor,
                &action.targets,
                action.quantity.as_ref(),
            )),
            _ => None,
        },
        Effect::TemporaryKeywordGrant(grant) if !grant.keywords.is_empty() => {
            mechanic(super::recognize_granted_keyword_reminder(
                cursor,
                &grant.keywords,
                super::permanent_operand_is_plural(&grant.objects),
            ))
        }
        Effect::ModifyStats(stats) if !stats.keywords.is_empty() => {
            mechanic(super::recognize_granted_keyword_reminder(
                cursor,
                &stats.keywords,
                super::permanent_operand_is_plural(&stats.targets),
            ))
        }
        Effect::CreateExplicitTokens(creation)
            if creation.additional.is_empty() && !creation.body.keywords.is_empty() =>
        {
            let (keywords, plural) = token_body_keywords(&creation.body);
            mechanic(super::recognize_granted_keyword_reminder(
                cursor, keywords, plural,
            ))
        }
        _ => None,
    }
}

/// Reads a reminder parenthetical owned by an effect of the program: a token
/// it creates, a keyword action it performs, or a keyword it grants. Each
/// candidate reads the same tokens from a checkpoint; the first that reads
/// the whole parenthetical owns it. `None` when no effect of the program has
/// a typed reminder grammar (the cursor is unchanged); otherwise the furthest
/// committed failure is reported, never a skip (CR 207.2).
pub(in crate::oracle::canonical) fn recognize_program_mechanic_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    effects: &super::EffectSequenceNode,
) -> Option<MachineResult<AbilityReminderOwner>> {
    let checkpoint = cursor.checkpoint();
    let mut best_failure: Option<super::super::MachineFailure> = None;
    let mut tried = false;
    for effect in reminded_program_effects(effects) {
        if let Some(creation) = super::reminded_token_creation(effect) {
            tried = true;
            cursor.rollback(checkpoint);
            match super::recognize_token_creation_reminder(cursor, &creation) {
                Ok(()) => return Some(Ok(AbilityReminderOwner::NamedToken)),
                Err(failure) => keep_furthest(&mut best_failure, failure),
            }
        }
        cursor.rollback(checkpoint);
        match recognize_effect_reminder(cursor, effect) {
            Some(Ok(owner)) => return Some(Ok(owner)),
            Some(Err(failure)) => {
                tried = true;
                keep_furthest(&mut best_failure, failure);
            }
            None => {}
        }
    }
    cursor.rollback(checkpoint);
    tried.then(|| Err(best_failure.expect("a tried reminder candidate failed")))
}

fn keep_furthest(
    best: &mut Option<super::super::MachineFailure>,
    failure: super::super::MachineFailure,
) {
    if best
        .as_ref()
        .is_none_or(|current| failure.token_index > current.token_index)
    {
        *best = Some(failure);
    }
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
        // The source is named by one third-person singular pronoun pair
        // (`its ... it`, `his ... he`, `her ... she`); the possessive and
        // the subject must agree.
        cursor.enter("activation.keyword-reminder.power-up-reduction");
        cursor.consume_phrase(&["reduce", "the", "cost", "by"])?;
        let subject = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
            Some("its") => "it",
            Some("his") => "he",
            Some("her") => "she",
            _ => return Err(cursor.fail("its, his or her")),
        };
        cursor.advance_to(cursor.position() + 1)?;
        cursor.consume_phrase(&[
            "mana", "cost", "if", subject, "entered", "this", "turn", ".",
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
