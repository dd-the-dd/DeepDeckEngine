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
/// subject whose binding fails is the clause's failure.
pub(in crate::oracle::canonical) fn recognize_performer(
    cursor: &mut OracleTokenCursor<'_>,
    context: &PerformerContext,
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
    for token in tokens.iter_mut().skip(verb + 1) {
        match token.raw.as_str() {
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
        PerformerNode::Player(_) => "a player other than the controller",
    }
}
