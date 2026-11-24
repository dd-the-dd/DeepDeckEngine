//! The performer of a clause (CR 608.2c, 109.5): the player its subject
//! names. The clause reads the subject and checks the verb's agreement; the
//! ordinary action readers then read the predicate as the performer's own
//! instruction, in the second person (`that player creates a token` is the
//! instruction `create a token` performed by that player). The lowering puts
//! the performer on every primitive of the action; the engine runs them as
//! that player's.
use super::super::{MachineResult, OracleTokenCursor};
use super::source_sacrifice::atomic_action::player_subject::{
    recognize_defending_player_subject, recognize_object_related_player_subject,
    recognize_player_effect_subject,
};
use super::{
    ObjectSubjectAntecedent, PlayerBinding, PlayerEffectSubject, PlayerSetReferenceNode,
    VerbAgreement, clause_verb_unsupported, defending_player_starts, lookup_verb,
    object_related_player_head_length,
};
use crate::oracle::model::RecognizedEntity;
use crate::oracle::syntax::recognize_entities;

/// Who performs an action other than the ability's controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PerformerNode {
    /// One player the binding environment names.
    Player(PlayerEffectSubject),
    /// `each player|opponent|other player`: every player of the population
    /// performs the action in turn order (CR 101.4).
    Population {
        players: PlayerSetReferenceNode,
        excluding_controller: bool,
    },
    /// `an opponent`: one player of the population, chosen by the ability's
    /// controller on resolution (CR 608.2d). `binding` names the choice: every
    /// later verb the same subject continues reads the same chosen player.
    Chosen {
        players: PlayerSetReferenceNode,
        binding: PlayerBinding,
    },
}

impl PerformerNode {
    /// The performer a subjectless later verb of the same sentence continues
    /// (`target opponent reveals their hand and discards a card`): a declared
    /// target is declared once, then bound; every other performer is itself.
    pub(in crate::oracle::canonical) fn continued(&self) -> Self {
        match self {
            Self::Player(PlayerEffectSubject::DeclaredTarget { binding, .. }) => {
                Self::Player(PlayerEffectSubject::Bound(*binding))
            }
            other => other.clone(),
        }
    }

    /// The player binding a later `that player` names after this performer:
    /// only a declared target is a program-level player antecedent.
    pub(in crate::oracle::canonical) fn declared_player(&self) -> Option<PlayerBinding> {
        match self {
            Self::Player(PlayerEffectSubject::DeclaredTarget { binding, .. }) => Some(*binding),
            _ => None,
        }
    }
}

/// The shared clause modality at the cursor after a performer subject
/// (CR 608.2c, 608.2d). Only the indicative and `may` are a performer's
/// instruction; another modal (`can't`, `would`) is no performed action and
/// is refused by the caller with the clause's own diagnostic.
pub(in crate::oracle::canonical) fn performer_modality(
    cursor: &OracleTokenCursor<'_>,
) -> super::ClauseModality {
    match cursor.peek().and_then(super::lookup_auxiliary) {
        Some((super::Auxiliary::May, _)) => super::ClauseModality::May,
        Some((super::Auxiliary::Would, _)) => super::ClauseModality::Would,
        Some((super::Auxiliary::Cannot, _)) => super::ClauseModality::Cannot,
        _ => super::ClauseModality::Indicative,
    }
}

/// `PLAYER may VERB ...` read as the performer's own permission: the
/// predicate after `may` in the performer's person, and the subject's last
/// token read as `you`, so the shared permission reader (`you may ...`)
/// owns the optional sentence. Returns the tokens and the position of that
/// `you`. Spans and indices stay the original ones. `cursor` is at `may`.
pub(in crate::oracle::canonical) fn person_shifted_permission_tokens(
    cursor: &OracleTokenCursor<'_>,
    subject: &str,
) -> MachineResult<(Vec<RecognizedEntity>, usize)> {
    let mut verb = cursor.clone();
    verb.enter("clause.performer.may");
    let may = verb.position();
    verb.require(may > 0, "a performer subject before `may`")?;
    verb.consume("may")?;
    let mut tokens = person_shifted_tokens(&verb, VerbAgreement::Base, subject)?;
    replace_word(&mut tokens[may - 1], "you");
    Ok((tokens, may - 1))
}

/// The position of the period that ends the permission's sentence at the
/// cursor; quoted abilities and reminder parentheses are nested scopes (CR
/// 113.1, 207.2a). A permission owns its sentence only.
pub(in crate::oracle::canonical) fn permission_sentence_end(
    cursor: &OracleTokenCursor<'_>,
) -> MachineResult<usize> {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut offset = 0;
    while let Some(raw) = cursor.peek_ahead(offset) {
        match raw {
            "\"" | "\u{201c}" | "\u{201d}" => quoted = !quoted,
            _ if quoted => {}
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            "." if depth == 0 => return Ok(cursor.position() + offset),
            _ => {}
        }
        offset += 1;
    }
    Err(cursor.fail("a complete permission sentence"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PerformedEffectNode {
    pub performer: PerformerNode,
    pub effect: Box<super::LocalEffectNode>,
}

/// The bindings the sequence proved for a performer subject.
#[derive(Clone, Debug, Default)]
pub(in crate::oracle::canonical) struct PerformerContext {
    pub player_antecedent: Option<PlayerBinding>,
    pub event_player: bool,
    pub defending_player: bool,
    pub object_antecedent: Option<ObjectSubjectAntecedent>,
}

/// A performer subject at the cursor and the agreement its verb takes.
/// `None` when no player subject other than the controller starts here; a
/// subject whose binding fails is the clause's failure. Without a
/// declaration slot, `target player` is no performer here.
pub(in crate::oracle::canonical) fn recognize_performer(
    cursor: &mut OracleTokenCursor<'_>,
    context: &PerformerContext,
) -> Option<MachineResult<(PerformerNode, VerbAgreement)>> {
    recognize_declaring_performer(cursor, context, None)
}

/// The performer reader of an effect program that can declare a player
/// slot: `target player|opponent` declares one target player under
/// `declaration` (CR 115.1, 601.2c), and `an opponent` is chosen on
/// resolution under the same binding (CR 608.2d). Every other head is the
/// one [`recognize_performer`] reads.
pub(in crate::oracle::canonical) fn recognize_declaring_performer(
    cursor: &mut OracleTokenCursor<'_>,
    context: &PerformerContext,
    declaration: Option<PlayerBinding>,
) -> Option<MachineResult<(PerformerNode, VerbAgreement)>> {
    let head = (0..3)
        .map(|offset| cursor.peek_ahead(offset).unwrap_or("").to_ascii_lowercase())
        .collect::<Vec<_>>();
    let is = |offset: usize, word: &str| head[offset] == word;
    cursor.enter("clause.performer");
    if is(0, "each") && (is(1, "player") || is(1, "opponent")) {
        let players = if is(1, "player") {
            PlayerSetReferenceNode::EachPlayer
        } else {
            PlayerSetReferenceNode::OpponentsOfAbilityController
        };
        return Some(cursor.advance_to(cursor.position() + 2).map(|()| {
            (
                PerformerNode::Population {
                    players,
                    excluding_controller: false,
                },
                VerbAgreement::ThirdPersonSingular,
            )
        }));
    }
    if is(0, "each") && is(1, "other") && is(2, "player") {
        return Some(cursor.advance_to(cursor.position() + 3).map(|()| {
            (
                PerformerNode::Population {
                    players: PlayerSetReferenceNode::EachPlayer,
                    excluding_controller: true,
                },
                VerbAgreement::ThirdPersonSingular,
            )
        }));
    }
    if is(0, "they") && (context.player_antecedent.is_some() || context.event_player) {
        let player = context
            .player_antecedent
            .map_or(PlayerEffectSubject::EventPlayer, PlayerEffectSubject::Bound);
        return Some(
            cursor
                .consume("they")
                .map(|()| (PerformerNode::Player(player), VerbAgreement::Base)),
        );
    }
    let single = |player: MachineResult<PlayerEffectSubject>| {
        player.map(|player| {
            (
                PerformerNode::Player(player),
                VerbAgreement::ThirdPersonSingular,
            )
        })
    };
    if let Some(declaration) = declaration {
        // `target player|opponent VERBs`: one declared target player. A
        // possessive head (`target player's`) is a zone owner, never here.
        if is(0, "target") && (is(1, "player") || is(1, "opponent")) {
            return Some(single(recognize_player_effect_subject(
                cursor,
                declaration,
                context.player_antecedent,
                context.event_player,
            )));
        }
        // `an opponent VERBs`: the controller chooses one opponent while the
        // effect resolves; no target is declared (CR 608.2d).
        if is(0, "an") && is(1, "opponent") {
            cursor.enter("clause.performer.chosen-opponent");
            return Some(cursor.advance_to(cursor.position() + 2).map(|()| {
                (
                    PerformerNode::Chosen {
                        players: PlayerSetReferenceNode::OpponentsOfAbilityController,
                        binding: declaration,
                    },
                    VerbAgreement::ThirdPersonSingular,
                )
            }));
        }
    }
    if is(0, "that") && is(1, "player") {
        return Some(single(recognize_player_effect_subject(
            cursor,
            PlayerBinding(usize::MAX),
            context.player_antecedent,
            context.event_player,
        )));
    }
    if defending_player_starts(cursor) {
        return Some(single(recognize_defending_player_subject(
            cursor,
            context.defending_player,
        )));
    }
    if object_related_player_head_length(cursor).is_some() {
        return Some(single(recognize_object_related_player_subject(
            cursor,
            context.object_antecedent.clone(),
        )));
    }
    None
}

/// The predicate tokens read as the performer's own instruction: the verb at
/// `verb` in the lexicon base form (after checking the subject's
/// agreement), and the performer's pronouns of its sentence in the second
/// person (`they control`, `their hand`). Spans and indices stay the
/// original ones, so every diagnostic points at the written text.
pub(in crate::oracle::canonical) fn person_shifted_tokens(
    cursor: &OracleTokenCursor<'_>,
    agreement: VerbAgreement,
    subject: &str,
) -> MachineResult<Vec<RecognizedEntity>> {
    let mut cursor = cursor.clone();
    cursor.enter("clause.performer.verb");
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a verb after the performer"))?;
    let Some((entry, forms)) = lookup_verb(raw, &super::ALL_LEMMAS) else {
        return Err(clause_verb_unsupported(&cursor, subject));
    };
    cursor.require(
        forms.contains(&agreement.form()),
        &format!(
            "`{}`, the verb form its subject requires",
            entry.surface(agreement.form())
        ),
    )?;
    let mut tokens = cursor.token_slice(0, usize::MAX).to_vec();
    let verb = cursor.position();
    replace_word(&mut tokens[verb], entry.surface(super::VerbForm::Base));
    let mut depth = 0usize;
    let mut quoted = false;
    for token in tokens.iter_mut().skip(verb + 1) {
        match token.raw.as_str() {
            "\"" | "\u{201c}" | "\u{201d}" => quoted = !quoted,
            _ if quoted => {}
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            "." if depth == 0 => break,
            raw if depth == 0 && raw.eq_ignore_ascii_case("they") => replace_word(token, "you"),
            raw if depth == 0 && raw.eq_ignore_ascii_case("their") => replace_word(token, "your"),
            _ => {}
        }
    }
    Ok(tokens)
}

/// The written token reclassified as `word` by the shared tokenizer.
fn replace_word(token: &mut RecognizedEntity, word: &str) {
    if let Some(class) = recognize_entities(word).into_iter().next() {
        token.category = class.category;
        token.token_type = class.token_type;
    }
    token.raw = word.to_string();
}

/// A short subject description for diagnostics.
pub(in crate::oracle::canonical) fn performer_description(
    performer: &PerformerNode,
) -> &'static str {
    match performer {
        PerformerNode::Population { .. } => "a player population (`each player`)",
        PerformerNode::Player(PlayerEffectSubject::DeclaredTarget { .. }) => {
            "a declared target player"
        }
        PerformerNode::Player(_) => "a player other than the controller",
        PerformerNode::Chosen { .. } => "a player chosen on resolution (`an opponent`)",
    }
}

/// A written `you` in the tokens the performed action consumed
/// (`from..to`) names the ability's controller, which the performer's
/// instruction would read as the performer itself: that reading is
/// ambiguous and fails explicitly. Quoted abilities (CR 113.1) are their own
/// scope: their `you` is the controller of the object that has them.
/// Tokens after the action (`, then you draw a card`) are not its own.
pub(in crate::oracle::canonical) fn controller_reference_failure(
    cursor: &OracleTokenCursor<'_>,
    from: usize,
    to: usize,
) -> Option<super::super::MachineFailure> {
    let mut quoted = false;
    for token in cursor.token_slice(from, to) {
        match token.raw.as_str() {
            "\"" | "\u{201c}" | "\u{201d}" => quoted = !quoted,
            _ if quoted => {}
            raw if ["you", "your", "yours", "you've"]
                .iter()
                .any(|word| raw.eq_ignore_ascii_case(word)) =>
            {
                let mut at = cursor.clone();
                at.enter("clause.performer.controller-reference");
                return Some(at.fail_with_span(
                    "a performer clause without `you`, which would name the performer instead of the ability's controller",
                    token.index,
                    token.span_start,
                    token.span_end,
                    token.raw.clone(),
                ));
            }
            _ => {}
        }
    }
    None
}
