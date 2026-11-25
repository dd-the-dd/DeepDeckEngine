//! The performer of a clause (CR 608.2c, 109.5): the player its subject
//! names. The clause reads the subject and checks the verb's agreement; the
//! ordinary action readers then read the predicate as the performer's own
//! instruction, in the second person (`that player creates a token` is the
//! instruction `create a token` performed by that player). The lowering puts
//! the performer on every primitive of the action; the engine runs them as
//! that player's.
use super::super::{MachineResult, OracleTokenCursor};
use super::source_sacrifice::atomic_action::player_subject::{
    PlayerAnaphor, player_anaphor_at, recognize_defending_player_subject,
    recognize_object_related_player_subject, recognize_player_effect_subject,
};
use super::{
    ObjectPlayerRelation, ObjectSubjectAntecedent, PlayerBinding, PlayerEffectSubject,
    PlayerSetReferenceNode, VerbAgreement, clause_verb_unsupported, defending_player_starts,
    lookup_verb, object_related_player_head_length,
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
    /// `each player who controls the most lands`, `each opponent who
    /// doesn't control an Elf`, `each player other than its controller`:
    /// every member of the restricted population performs the action in turn
    /// order (CR 101.4, 102.1-102.3). The members are the population's
    /// players satisfying the relative predicate when the action begins.
    FilteredPopulation(Box<super::PlayerPopulationQueryNode>),
    /// `the player to your left|right` (CR 101.4, 800.4j): the nearest
    /// player still in the game in that direction of the seating order.
    SeatNeighbor(SeatDirection),
    /// `an opponent`: one player of the population, chosen by the ability's
    /// controller on resolution (CR 608.2d). `binding` names the choice: every
    /// later verb the same subject continues reads the same chosen player.
    Chosen {
        players: PlayerSetReferenceNode,
        binding: PlayerBinding,
    },
    /// `two|up to N|any number of target players|opponents each VERB`: the
    /// players one target decision declares under `binding` (CR 115.1,
    /// 601.2c); each of them performs the action in turn order.
    DeclaredPlayers {
        target: super::TargetListNode,
        binding: PlayerBinding,
    },
    /// The same declared players, already declared by an earlier verb of
    /// the clause (`each lose 2 life and sacrifice a creature`).
    BoundPlayers(PlayerBinding),
    /// `For each OBJECT VERBed this way, its controller|owner ACTION`: the
    /// action is performed once per object the earlier action at
    /// `effect_index` affected, each time by that object's player, read by
    /// its last known information once it left the battlefield (CR 608.2c,
    /// 608.2h, 108.3, 109.4).
    EachAffectedObject {
        relation: ObjectPlayerRelation,
        effect_index: usize,
        filter: super::ActionResultFilter,
    },
}

/// A direction of the seating order (CR 101.4: turn order passes to the
/// left).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SeatDirection {
    Left,
    Right,
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
            Self::DeclaredPlayers { binding, .. } => Self::BoundPlayers(*binding),
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
    /// The one player an earlier action named only through its singular
    /// object (`return target permanent to its owner's hand`, `target
    /// creature an opponent controls`): the antecedent of `that player` when
    /// no player was declared or bound (CR 108.3, 109.4).
    pub object_player_antecedent: Option<(ObjectPlayerRelation, super::PermanentEffectReference)>,
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
        return Some(
            cursor
                .advance_to(cursor.position() + 2)
                .and_then(|()| restricted_population(cursor, context, players, false)),
        );
    }
    if is(0, "each") && is(1, "other") && is(2, "player") {
        return Some(cursor.advance_to(cursor.position() + 3).and_then(|()| {
            restricted_population(cursor, context, PlayerSetReferenceNode::EachPlayer, true)
        }));
    }
    if let Some(direction) = seat_neighbor_at(cursor) {
        cursor.enter("clause.performer.seat-neighbor");
        return Some(cursor.advance_to(cursor.position() + 5).map(|()| {
            (
                PerformerNode::SeatNeighbor(direction),
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
    if let Some(declaration) = declaration
        && declared_players_each_starts(cursor)
    {
        return Some(recognize_declared_players(cursor, declaration));
    }
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
    if let Some(anaphor) = player_anaphor_at(cursor) {
        // Without a player binding, `that player` is the player the previous
        // action named through its object; it is resolved from that object
        // when the action resolves (last known information, CR 608.2h).
        if context.player_antecedent.is_none()
            && !context.event_player
            && let Some((relation, object)) = context.object_player_antecedent
        {
            cursor.enter("clause.performer.object-player-anaphor");
            let written = match anaphor {
                PlayerAnaphor::ThatPlayer => ["that", "player"],
                PlayerAnaphor::ThePlayer => ["the", "player"],
                PlayerAnaphor::ThatOpponent => ["that", "opponent"],
            };
            return Some(cursor.consume_phrase(&written).map(|()| {
                (
                    PerformerNode::Player(PlayerEffectSubject::ObjectRelation { relation, object }),
                    VerbAgreement::ThirdPersonSingular,
                )
            }));
        }
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

/// The population read so far, restricted by a relative clause when one
/// follows (`each player who controls the most lands`, `each player other
/// than its controller`). The restriction is the shared player-set grammar
/// (`player_set.rs`); a restriction it does not type is the clause's failure.
fn restricted_population(
    cursor: &mut OracleTokenCursor<'_>,
    context: &PerformerContext,
    players: PlayerSetReferenceNode,
    excluding_controller: bool,
) -> MachineResult<(PerformerNode, VerbAgreement)> {
    if !super::player_relative_clause_starts(cursor) {
        return Ok((
            PerformerNode::Population {
                players,
                excluding_controller,
            },
            VerbAgreement::ThirdPersonSingular,
        ));
    }
    cursor.enter("clause.performer.restricted-population");
    let relative = super::PlayerRelativeContext {
        player_antecedent: context.player_antecedent,
        event_player: context.event_player,
        object_antecedent: context.object_antecedent.clone(),
    };
    let query = super::recognize_player_population_restriction(
        cursor,
        players,
        excluding_controller,
        &relative,
    )?;
    Ok((
        PerformerNode::FilteredPopulation(Box::new(query)),
        VerbAgreement::ThirdPersonSingular,
    ))
}

/// `the player to your left|right` at the cursor (five tokens). Only the
/// ability controller's neighbor is a performer here; `to their left` names
/// a member of an enclosing iteration and is not read.
pub(in crate::oracle::canonical) fn seat_neighbor_at(
    cursor: &OracleTokenCursor<'_>,
) -> Option<SeatDirection> {
    let is = |offset: usize, word: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    };
    if !(is(0, "the") && is(1, "player") && is(2, "to") && is(3, "your")) {
        return None;
    }
    if is(4, "left") {
        Some(SeatDirection::Left)
    } else if is(4, "right") {
        Some(SeatDirection::Right)
    } else {
        None
    }
}

/// True when the cursor starts a counted player target list followed by
/// the distributive `each` (`two target players each`, `any number of
/// target opponents each`). A single declared player is not this subject.
fn declared_players_each_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let at = |offset: usize, words: &[&str]| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| words.iter().any(|word| raw.eq_ignore_ascii_case(word)))
    };
    !at(0, &["target"])
        && (1..6).any(|offset| {
            at(offset, &["target"])
                && at(offset + 1, &["players", "opponents"])
                && at(offset + 2, &["each"])
        })
}

/// `COUNT target players|opponents each`: one declaration of several target
/// players (CR 115.1, 601.2c) whose members each perform the clause; the
/// verb after `each` takes the base form. The count is the shared target
/// count (a literal, `up to N`, `X` or `any number of`); one target player
/// is the singular performer's, never this distributive subject.
fn recognize_declared_players(
    cursor: &mut OracleTokenCursor<'_>,
    declaration: PlayerBinding,
) -> MachineResult<(PerformerNode, VerbAgreement)> {
    cursor.enter("clause.performer.declared-players");
    let target =
        super::recognize_target_list_with_counts(cursor, super::TargetCountSupport::Expressed)?;
    cursor.enter("clause.performer.declared-players.kind");
    cursor.require(
        matches!(
            target.slots.as_slice(),
            [slot] if matches!(slot.selector, super::TargetSelector::Player(_))
                && !slot.exclude_source
                && (slot.count.is_some() || slot.cardinality.maximum > 1)
        ),
        "one declaration of several target players",
    )?;
    cursor.enter("clause.performer.declared-players.each");
    cursor.consume("each")?;
    Ok((
        PerformerNode::DeclaredPlayers {
            target,
            binding: declaration,
        },
        VerbAgreement::Base,
    ))
}

/// `its controller` / `its owner` at the cursor: the player of the object a
/// preposed `for each OBJECT VERBed this way` iterates. Lexical lookahead.
pub(in crate::oracle::canonical) fn affected_object_player_at(
    cursor: &OracleTokenCursor<'_>,
) -> Option<ObjectPlayerRelation> {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("its"))
        .then(|| cursor.peek_ahead(1).and_then(written_player_relation))
        .flatten()
}

/// The relation an object-related player subject writes before `of`.
fn written_player_relation(raw: &str) -> Option<ObjectPlayerRelation> {
    if raw.eq_ignore_ascii_case("owner") {
        Some(ObjectPlayerRelation::Owner)
    } else if raw.eq_ignore_ascii_case("controller") {
        Some(ObjectPlayerRelation::Controller)
    } else {
        None
    }
}

/// True when the cursor starts `the owner|controller of target ...`: a
/// player subject that declares the object it is related to.
pub(in crate::oracle::canonical) fn object_relation_declaration_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| written_player_relation(raw).is_some())
        && cursor
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
        && cursor
            .peek_ahead(3)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("target"))
}

/// `the owner|controller of TARGET VERBs ...` (CR 108.3, 109.4, 115.1): the
/// subject declares one target permanent, and the player related to it
/// performs the clause. The declaration is the ordinary `choose target`
/// instruction, so a later `it` names the target; the caller continues the
/// clause with the related player as its performer. The subject must be
/// followed by a finite verb of the lexicon: a modal or a missing verb fails
/// here, never silently.
pub(in crate::oracle::canonical) fn recognize_object_relation_declaration(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(ObjectPlayerRelation, super::ChooseInstructionNode)> {
    cursor.enter("clause.performer.object-relation-declaration");
    cursor.consume("the")?;
    let relation = cursor
        .peek()
        .and_then(written_player_relation)
        .ok_or_else(|| cursor.fail("`owner` or `controller`"))?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.consume("of")?;
    cursor.enter("clause.performer.object-relation-declaration.target");
    let targets = super::recognize_target_list(cursor)?;
    cursor.require(
        matches!(
            targets.slots.as_slice(),
            [slot] if slot.cardinality == super::TargetCardinality::EXACTLY_ONE
                && super::selector_guarantees_permanent(&slot.selector)
        ),
        "exactly one target permanent whose owner or controller performs the clause",
    )?;
    cursor.enter("clause.performer.object-relation-declaration.verb");
    cursor.require(
        cursor.peek().is_some_and(|raw| {
            lookup_verb(raw, &super::ALL_LEMMAS)
                .is_some_and(|(_, forms)| forms.contains(&super::VerbForm::ThirdPersonSingular))
        }),
        "a finite verb performed by the declared object's player",
    )?;
    Ok((
        relation,
        super::ChooseInstructionNode {
            chooser: super::ChooserNode::Controller,
            domain: super::ChoiceDomainNode::PermanentTargets(targets),
        },
    ))
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
    // `PLAYER can't VERB ...` (CR 101.2): the prohibition is the
    // performer's own; the modal keeps its place and takes the bare verb.
    let prohibition = cursor
        .peek()
        .and_then(super::lookup_auxiliary)
        .is_some_and(|(auxiliary, _)| auxiliary == super::Auxiliary::Cannot);
    if prohibition {
        cursor.advance_to(cursor.position() + 1)?;
    }
    let required = if prohibition {
        super::VerbForm::Base
    } else {
        agreement.form()
    };
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a verb after the performer"))?;
    let Some((entry, forms)) = lookup_verb(raw, &super::ALL_LEMMAS) else {
        return Err(clause_verb_unsupported(&cursor, subject));
    };
    cursor.require(
        forms.contains(&required),
        &format!(
            "`{}`, the verb form its subject requires",
            entry.surface(required)
        ),
    )?;
    let mut tokens = cursor.token_slice(0, usize::MAX).to_vec();
    let verb = cursor.position();
    replace_word(&mut tokens[verb], entry.surface(super::VerbForm::Base));
    let mut depth = 0usize;
    let mut quoted = false;
    let mut after_coordinator = false;
    // `of their choice` already names the acting player as the chooser
    // (CR 101.4); the shared choice marker reads it as written. Any other
    // `their choice` (`on their choice of the top or bottom of their
    // library`) is the performer's own possessive and shifts like the rest.
    let chooser_marker = tokens
        .windows(3)
        .enumerate()
        .filter(|(_, triple)| {
            triple[0].raw.eq_ignore_ascii_case("of")
                && triple[1].raw.eq_ignore_ascii_case("their")
                && triple[2].raw.eq_ignore_ascii_case("choice")
        })
        .map(|(index, _)| index + 1)
        .collect::<Vec<_>>();
    for (index, token) in tokens.iter_mut().enumerate().skip(verb + 1) {
        if chooser_marker.contains(&index) {
            continue;
        }
        // A verb coordinated with the performer's verb (`searches ..., then
        // shuffles and puts ...`, `searches ..., puts it ...`) has the same
        // subject and agreement; it is read in the base form like the head
        // verb. Only a verb form of that agreement is rewritten.
        let coordinated = std::mem::take(&mut after_coordinator);
        match token.raw.as_str() {
            "\"" | "\u{201c}" | "\u{201d}" => quoted = !quoted,
            _ if quoted => {}
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            "." if depth == 0 => break,
            raw if depth == 0 && raw.eq_ignore_ascii_case("they") => replace_word(token, "you"),
            raw if depth == 0 && raw.eq_ignore_ascii_case("their") => replace_word(token, "your"),
            raw if depth == 0
                && (raw.eq_ignore_ascii_case("and")
                    || raw.eq_ignore_ascii_case("then")
                    || raw == ",") =>
            {
                after_coordinator = true;
            }
            raw if depth == 0 && coordinated && agreement != VerbAgreement::Base => {
                if let Some((entry, forms)) = lookup_verb(raw, &super::ALL_LEMMAS)
                    && forms.contains(&agreement.form())
                    && !forms.contains(&super::VerbForm::Base)
                {
                    replace_word(token, entry.surface(super::VerbForm::Base));
                }
            }
            _ => {}
        }
    }
    Ok(tokens)
}

/// True when the `you` at `offset` is the player relation of a declared
/// target noun phrase (`target permanent you control`, `another target
/// creature you own`): `you` is followed by the relation verb and the noun
/// phrase it closes began at `target`, with no clause verb between them.
fn declared_target_controller_relation(tokens: &[RecognizedEntity], offset: usize) -> bool {
    let raw = |index: usize| tokens.get(index).map(|token| token.raw.as_str());
    if !raw(offset).is_some_and(|word| word.eq_ignore_ascii_case("you"))
        || !raw(offset + 1).is_some_and(|word| {
            ["control", "controls", "own", "owns"]
                .iter()
                .any(|relation| word.eq_ignore_ascii_case(relation))
        })
    {
        return false;
    }
    for index in (0..offset).rev() {
        let word = raw(index).unwrap_or("");
        if word.eq_ignore_ascii_case("target") {
            return true;
        }
        if matches!(word, "." | ";" | ":" | "\"" | "\u{201c}" | "\u{201d}")
            || lookup_verb(word, &super::ALL_LEMMAS).is_some()
        {
            return false;
        }
    }
    false
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
        PerformerNode::DeclaredPlayers { .. } | PerformerNode::BoundPlayers(_) => {
            "declared target players (`two target players each`)"
        }
        PerformerNode::EachAffectedObject { .. } => {
            "the player of each affected object (`its controller` after `for each`)"
        }
        PerformerNode::FilteredPopulation(_) => {
            "a restricted player population (`each player who ...`)"
        }
        PerformerNode::SeatNeighbor(_) => "a seat neighbor (`the player to your left`)",
    }
}

/// A written `you` in the tokens the performed action consumed
/// (`from..to`) names the ability's controller, which the performer's
/// instruction would read as the performer itself: that reading is
/// ambiguous and fails explicitly. Quoted abilities (CR 113.1) are their own
/// scope: their `you` is the controller of the object that has them.
/// Tokens after the action (`, then you draw a card`) are not its own.
/// The controller relation of a declared target (`gains control of target
/// permanent you control`) is not ambiguous: the ability's controller
/// declares every target while the ability is put on the stack (CR 601.2c,
/// 602.2b), before any player performs the action.
pub(in crate::oracle::canonical) fn controller_reference_failure(
    cursor: &OracleTokenCursor<'_>,
    from: usize,
    to: usize,
) -> Option<super::super::MachineFailure> {
    let mut quoted = false;
    let tokens = cursor.token_slice(from, to);
    for (offset, token) in tokens.iter().enumerate() {
        match token.raw.as_str() {
            "\"" | "\u{201c}" | "\u{201d}" => quoted = !quoted,
            _ if quoted => {}
            _ if declared_target_controller_relation(tokens, offset) => {}

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
