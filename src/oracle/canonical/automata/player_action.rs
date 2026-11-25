//! A singular grammatical player subject followed by one independent action.
//! Sequence owns subject lifetime; target declarations remain separate bindings.
use super::super::{MachineResult, OracleTokenCursor};
use super::recognize_counter_name;
use super::source_sacrifice::atomic_action::ActingSubject;
use super::source_sacrifice::atomic_action::counted_card::recognize_card_count_noun;
use super::source_sacrifice::atomic_action::player_discard::{
    PlayerDiscardEffectNode, recognize_player_discard_action_fragment,
};
use super::source_sacrifice::atomic_action::player_subject::{
    ObjectSubjectAntecedent, defending_player_starts, object_related_player_head_length,
    player_anaphor_at, recognize_defending_player_subject, recognize_object_related_player_subject,
    recognize_player_effect_subject,
};
use super::{
    CardCollectionReferenceNode, CollectionOperandAttempt, EffectRecognitionContext,
    LifeActionNode, NumericExpression, PlayerBinding, PlayerEffectSubject,
    PlayerSacrificeActionNode, SubjectVerbAgreement, VerbAgreement, recognize_draw_action_fragment,
    recognize_numeric_expression, recognize_player_sacrifice_action_with_form,
    recognize_supplied_life_action, supplied_life_action_starts,
    try_recognize_card_collection_reference,
};
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form, lookup_verb};
use crate::oracle::counter_name::CounterName;

/// Lexicon lemmas this child dispatches on (other verbs keep their readers).
const MIGRATED_PLAYER_LEMMAS: &[VerbLemma] = &[
    VerbLemma::Mill,
    VerbLemma::Discard,
    VerbLemma::Sacrifice,
    VerbLemma::Exile,
    VerbLemma::Reveal,
    VerbLemma::Get,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SingularPlayerAction {
    Draw(NumericExpression),
    Life(LifeActionNode),
    /// `mill N cards` (CR 701.17): the player's verb; N is the shared numeric
    /// expression and the counted noun agrees with it.
    Mill(NumericExpression),
    /// `venture into the dungeon` / `venture into NAME` (CR 701.49): the
    /// named form is the restricted procedure of that Dungeon (CR 701.49c).
    Venture(VentureActionNode),
    /// `discards N cards [at random]`: the same action child that quantified
    /// subjects use. The node's own subject repeats the enclosing one.
    Discard(PlayerDiscardEffectNode),
    /// `sacrifices N QUERY [of their choice]` on the acting player's own
    /// permanents (CR 701.21a); the player chooses, then all are sacrificed.
    Sacrifice(PlayerSacrificeActionNode),
    /// `proliferate` (CR 701.34a): the acting player chooses any number of
    /// permanents and/or players that have a counter, then gives each one
    /// additional counter of each kind already there. The expression is the
    /// number of separate proliferations (`proliferate twice`), 1 when bare.
    Proliferate(NumericExpression),
    /// `reveals|discards their hand`: the shared whole-hand verb child.
    Hand(PlayerHandVerb),
    /// `discards|exiles that card|it|them`: the player acts on cards an
    /// earlier action of the same program bound, never on a new choice.
    ReferencedCards {
        verb: ReferencedCardVerb,
        cards: CardCollectionReferenceNode,
    },
    /// `gets N KIND counter(s)` | `gets {E}{E}` (CR 122.1): counters placed
    /// on the acting player.
    Counters(PlayerCountersNode),
    /// `creates TOKEN_DESCRIPTION` (CR 111.2): the acting player creates the
    /// tokens and controls them; the description is the shared token body.
    CreateTokens(Box<super::TokenCreationBodyNode>),
    /// `scries N` (CR 701.22a): the acting player looks at the top N cards
    /// of their library and orders them on the top and bottom.
    Scry(NumericExpression),
    /// `takes an extra turn after this one` (CR 500.7): the acting player
    /// takes the next turn after the current one.
    ExtraTurn,
    /// `becomes the monarch` / `takes the initiative` / `the Ring tempts
    /// you`: the acting player's designation (`player_designation.rs`).
    Designation(super::PlayerDesignationAction),
    /// `skips their next [N] turn(s)|untap step|draw step|combat phase`
    /// (CR 500.11): that part of the acting player's future turns does not
    /// happen.
    Skip(SkipTurnPartNode),
    /// `can't cast spells this turn` (CR 101.2): a prohibition on the acting
    /// player for the rest of the turn.
    Prohibition(PlayerProhibition),
    /// A change of the acting player's own game status: `loses the game`
    /// (CR 104.3a) or `gains hexproof|shroud|protection from everything
    /// DURATION` (CR 702.11d, 702.18d, 702.16k).
    Status(PlayerStatusAction),
}

/// A player's own game-status change (`player_action.rs` cell of the
/// player subject x verb table).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerStatusAction {
    /// `lose(s) the game` (CR 104.3a): the acting player loses the game.
    LoseGame,
    /// `gain(s) KEYWORDS DURATION`: the keywords a player can have, for the
    /// stated duration.
    GainKeywords(PlayerKeywordGrantNode),
}

/// A keyword the engine applies to a player (CR 702.11d hexproof, 702.18d
/// shroud, 702.16k protection from everything).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum PlayerGrantedKeyword {
    Hexproof,
    Shroud,
    ProtectionFromEverything,
}

impl PlayerGrantedKeyword {
    /// The player keyword an object keyword grant names, when a player can
    /// have it (`you and permanents you control gain hexproof`).
    pub(in crate::oracle::canonical) fn of_granted_keyword(
        keyword: super::GrantedKeyword,
    ) -> Option<Self> {
        match keyword {
            super::GrantedKeyword::Hexproof => Some(Self::Hexproof),
            super::GrantedKeyword::Shroud => Some(Self::Shroud),
            _ => None,
        }
    }
}

/// `gain(s) KEYWORDS DURATION` on the acting player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerKeywordGrantNode {
    pub keywords: Vec<PlayerGrantedKeyword>,
    pub duration: super::EffectDurationNode,
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// The player keyword word at `offset` (`hexproof`, `shroud`, `protection`).
fn player_keyword_word_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    ["hexproof", "shroud", "protection"]
        .iter()
        .any(|word| word_at(cursor, offset, word))
}

/// True when the verb at the cursor (any agreement) opens a player status
/// change: `lose(s) the game` or `gain(s) hexproof|shroud|protection`.
/// Other `lose` / `gain` frames (life, control, an object's keyword) keep
/// their own readers.
pub(in crate::oracle::canonical) fn player_status_action_follows(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let verb = |lemma: VerbLemma| {
        cursor
            .peek()
            .is_some_and(|raw| lookup_verb(raw, &[lemma]).is_some())
    };
    (verb(VerbLemma::Lose) && word_at(cursor, 1, "the") && word_at(cursor, 2, "game"))
        || (verb(VerbLemma::Gain) && player_keyword_word_at(cursor, 1))
}

/// Subject-free `lose(s) the game` | `gain(s) KEYWORDS DURATION`; the caller
/// resolves the acting player and its agreement. A preposed duration of the
/// sentence (`Until your next turn, ... you gain protection from
/// everything`) is the grant's own (CR 611.2a). A keyword or duration the
/// engine does not apply to players fails here explicitly.
pub(in crate::oracle::canonical) fn recognize_player_status_fragment(
    cursor: &mut OracleTokenCursor<'_>,
    form: VerbAgreement,
    supplied: Option<&super::EffectDurationNode>,
) -> MachineResult<PlayerStatusAction> {
    cursor.enter("player-action.status.verb");
    if cursor
        .peek()
        .is_some_and(|raw| lookup_verb(raw, &[VerbLemma::Lose]).is_some())
    {
        cursor.consume(super::verb_entry(VerbLemma::Lose).surface(form.form()))?;
        cursor.enter("player-action.lose-game.object");
        cursor.consume_phrase(&["the", "game"])?;
        return Ok(PlayerStatusAction::LoseGame);
    }
    cursor.consume(super::verb_entry(VerbLemma::Gain).surface(form.form()))?;
    let mut keywords = Vec::new();
    loop {
        cursor.enter("player-action.keyword-grant.keyword");
        let keyword = if word_at(cursor, 0, "hexproof") {
            cursor.consume("hexproof")?;
            PlayerGrantedKeyword::Hexproof
        } else if word_at(cursor, 0, "shroud") {
            cursor.consume("shroud")?;
            PlayerGrantedKeyword::Shroud
        } else if word_at(cursor, 0, "protection") {
            cursor.enter("player-action.keyword-grant.protection");
            cursor.require(
                word_at(cursor, 1, "from") && word_at(cursor, 2, "everything"),
                "`protection from everything`, the player protection the engine applies",
            )?;
            cursor.consume_phrase(&["protection", "from", "everything"])?;
            PlayerGrantedKeyword::ProtectionFromEverything
        } else {
            return Err(cursor.fail(
                "a keyword the engine grants to a player (hexproof, shroud, protection from everything)",
            ));
        };
        cursor.require(
            !keywords.contains(&keyword),
            "distinct keywords granted to the player",
        )?;
        keywords.push(keyword);
        // `hexproof and shroud`, `hexproof, shroud, and ...`.
        let width = if word_at(cursor, 0, "and") && player_keyword_word_at(cursor, 1) {
            1
        } else if cursor.peek() == Some(",") && player_keyword_word_at(cursor, 1) {
            1
        } else if cursor.peek() == Some(",")
            && word_at(cursor, 1, "and")
            && player_keyword_word_at(cursor, 2)
        {
            2
        } else {
            break;
        };
        cursor.advance_to(cursor.position() + width)?;
    }
    cursor.enter("player-action.keyword-grant.duration");
    let duration = super::recognize_trailing_or_supplied_duration(cursor, supplied)?;
    cursor.require(
        matches!(
            duration,
            super::EffectDurationNode::UntilEndOfCurrentTurn
                | super::EffectDurationNode::UntilNextTurn {
                    player: super::LocalPlayerReference::Controller,
                }
        ),
        "a player keyword duration: `until end of turn` or `until your next turn`",
    )?;
    if keywords.contains(&PlayerGrantedKeyword::ProtectionFromEverything) {
        cursor.enter("player-action.keyword-grant.protection-duration");
        cursor.require(
            matches!(duration, super::EffectDurationNode::UntilNextTurn { .. }),
            "protection from everything until your next turn, the player protection the engine applies",
        )?;
    }
    Ok(PlayerStatusAction::GainKeywords(PlayerKeywordGrantNode {
        keywords,
        duration,
    }))
}

/// The part of a player's turn a `skip` instruction names (CR 500.11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum SkippedTurnPart {
    /// A whole turn (CR 500.11).
    Turn,
    /// The untap step (CR 502).
    UntapStep,
    /// The draw step (CR 504).
    DrawStep,
    /// The combat phase (CR 506).
    CombatPhase,
    /// Every combat phase of the player's next turn (CR 500.11, 506.1):
    /// `skips all combat phases of their next turn`.
    AllCombatPhasesOfNextTurn,
}

impl SkippedTurnPart {
    /// The engine name of the skipped part.
    pub(in crate::oracle::canonical) fn name(self) -> &'static str {
        match self {
            Self::Turn => "turn",
            Self::UntapStep => "untapStep",
            Self::DrawStep => "drawStep",
            Self::CombatPhase => "combatPhase",
            Self::AllCombatPhasesOfNextTurn => "allCombatPhasesOfNextTurn",
        }
    }
}

/// `skip your next [N] PART [this turn]`: the next `count` occurrences of
/// the acting player's own `part` are skipped. `this_turn` limits the skip
/// to an occurrence in the current turn (`skips their next combat phase
/// this turn`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) struct SkipTurnPartNode {
    pub part: SkippedTurnPart,
    pub count: u32,
    pub this_turn: bool,
}

/// A rule-changing prohibition on the acting player until end of turn
/// (CR 101.2, 611.2a). Only prohibitions the engine runs are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum PlayerProhibition {
    /// `can't cast spells this turn`.
    CastSpells,
}

/// Counters a player gets: the canonical counter name and the amount.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerCountersNode {
    pub counter: CounterName,
    pub count: NumericExpression,
}

/// True when the `get` at the cursor takes player counters: a counter
/// symbol, or a counted counter noun before any clause boundary that is not
/// placed `on` or taken `from` an object.
pub(in crate::oracle::canonical) fn player_counters_follow(cursor: &OracleTokenCursor<'_>) -> bool {
    if cursor
        .peek()
        .is_none_or(|raw| lookup_verb(raw, &[VerbLemma::Get]).is_none())
    {
        return false;
    }
    if cursor
        .token_slice(cursor.position() + 1, cursor.position() + 2)
        .first()
        .is_some_and(|token| token.token_type == "counterSymbol")
    {
        return true;
    }
    for offset in 1..6 {
        let Some(raw) = cursor.peek_ahead(offset) else {
            return false;
        };
        if matches!(raw, "." | "," | ";" | ":" | "(") {
            return false;
        }
        // `get an amount of {E}`, `get that many {E}`: the symbol is the
        // counter after a written amount (CR 107.14, 122.1).
        if counter_symbol_at(cursor, offset) {
            return true;
        }
        if raw.eq_ignore_ascii_case("counter") || raw.eq_ignore_ascii_case("counters") {
            return !cursor.peek_ahead(offset + 1).is_some_and(|next| {
                next.eq_ignore_ascii_case("on") || next.eq_ignore_ascii_case("from")
            });
        }
    }
    false
}

fn counter_symbol_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    let at = cursor.position() + offset;
    cursor
        .token_slice(at, at + 1)
        .first()
        .is_some_and(|token| token.token_type == "counterSymbol")
}

/// One registered player-counter symbol after a written amount: only the
/// energy symbol `{E}` names a player counter (CR 107.14).
fn recognize_written_amount_symbol(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CounterName> {
    cursor.enter("player-action.counters.amount-symbol");
    let symbol = cursor.consume_type("counterSymbol")?;
    cursor.require(
        symbol.raw.eq_ignore_ascii_case("{E}"),
        "the registered energy-counter symbol `{E}`",
    )?;
    cursor.require(
        !counter_symbol_at(cursor, 0),
        "one counter symbol after a written amount",
    )?;
    Ok(CounterName::from_parts(["energy"]).expect("a non-empty counter name"))
}

/// `(energy counters)` after a written amount of `{E}`: the reminder names
/// the counter kind without restating the amount (CR 207.2).
fn recognize_uncounted_energy_reminder(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    if cursor.peek() != Some("(") {
        return Ok(());
    }
    cursor.enter("player-action.counters.amount-reminder");
    cursor.consume("(")?;
    let name = cursor.consume_type("counterTypeWord")?;
    cursor.require(
        name.raw.eq_ignore_ascii_case("energy"),
        "the energy counter the symbol names",
    )?;
    cursor.consume("counters")?;
    cursor.consume(")")
}

/// `get(s) {E}{E} [(two energy counters)]` | `get(s) N KIND counter(s)` |
/// `get(s) an amount of {E} [(energy counters)] equal to N` | `get(s) N
/// {E} [(energy counters)]` with a written amount (`that many {E}`).
pub(in crate::oracle::canonical) fn recognize_player_counters_fragment(
    cursor: &mut OracleTokenCursor<'_>,
    form: VerbAgreement,
) -> MachineResult<PlayerCountersNode> {
    cursor.enter("player-action.counters.verb");
    cursor.consume(super::verb_entry(VerbLemma::Get).surface(form.form()))?;
    // `an amount of {E} (energy counters) equal to EXPR` (CR 122.1): the
    // amount is the shared `equal to` quantity.
    if word_at(cursor, 0, "an") && word_at(cursor, 1, "amount") && word_at(cursor, 2, "of") {
        cursor.enter("player-action.counters.amount-of");
        cursor.consume_phrase(&["an", "amount", "of"])?;
        let counter = recognize_written_amount_symbol(cursor)?;
        recognize_uncounted_energy_reminder(cursor)?;
        cursor.enter("player-action.counters.amount-of.equal");
        cursor.consume_phrase(&["equal", "to"])?;
        let count = super::recognize_equal_to_amount_expression(cursor, None)?;
        return Ok(PlayerCountersNode { counter, count });
    }
    cursor.enter("player-action.counters.symbol");
    let mut symbols = 0_i64;
    while cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "counterSymbol")
    {
        let symbol = cursor.consume_type("counterSymbol")?;
        cursor.require(
            symbol.raw.eq_ignore_ascii_case("{E}"),
            "the registered energy-counter symbol `{E}`",
        )?;
        symbols += 1;
    }
    if symbols > 0 {
        let counter = CounterName::from_parts(["energy"]).expect("a non-empty counter name");
        // CR 207.2: the reminder restates the symbols; it must agree.
        if cursor.peek() == Some("(") {
            cursor.enter("player-action.counters.reminder");
            cursor.consume("(")?;
            let count = recognize_numeric_expression(cursor, None)?;
            cursor.require(
                count == NumericExpression::Atom(super::NumericAtom::Integer(symbols)),
                "a reminder that counts the energy symbols",
            )?;
            let name = cursor.consume_type("counterTypeWord")?;
            cursor.require(
                name.raw.eq_ignore_ascii_case("energy"),
                "the energy counter the symbols name",
            )?;
            recognize_counter_noun(cursor, &count)?;
            cursor.consume(")")?;
        }
        return Ok(PlayerCountersNode {
            counter,
            count: NumericExpression::Atom(super::NumericAtom::Integer(symbols)),
        });
    }
    cursor.enter("player-action.counters.count");
    let count = recognize_numeric_expression(cursor, None)?;
    // `that many {E} (energy counters)`: a written amount of one symbol.
    if counter_symbol_at(cursor, 0) {
        let counter = recognize_written_amount_symbol(cursor)?;
        recognize_uncounted_energy_reminder(cursor)?;
        return Ok(PlayerCountersNode { counter, count });
    }
    cursor.enter("player-action.counters.name");
    let counter = recognize_counter_name(cursor)?;
    recognize_counter_noun(cursor, &count)?;
    Ok(PlayerCountersNode { counter, count })
}

/// `counter` after one, `counters` otherwise.
fn recognize_counter_noun(
    cursor: &mut OracleTokenCursor<'_>,
    count: &NumericExpression,
) -> MachineResult<()> {
    cursor.enter("player-action.counters.noun");
    let singular = *count == NumericExpression::Atom(super::NumericAtom::Integer(1));
    cursor.consume(if singular { "counter" } else { "counters" })?;
    Ok(())
}

/// A verb whose object is the acting player's whole hand (`their hand`).
/// Singular and quantified subjects share this child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerHandVerb {
    /// CR 701.20a: the player shows every card in their hand. After a
    /// singular subject the hand becomes the observed collection `from it`.
    Reveal,
    /// CR 701.9a: every card in the hand is discarded.
    Discard,
}

/// The whole-hand verb at the cursor, in the subject's agreement, when it is
/// followed by the acting player's `their hand`. `None` makes no claim.
pub(in crate::oracle::canonical) fn player_hand_verb_at(
    cursor: &OracleTokenCursor<'_>,
    form: VerbAgreement,
) -> Option<PlayerHandVerb> {
    let action = if crate::oracle::canonical::automata::lemma_at(cursor, VerbLemma::Reveal, form) {
        PlayerHandVerb::Reveal
    } else if crate::oracle::canonical::automata::lemma_at(cursor, VerbLemma::Discard, form) {
        PlayerHandVerb::Discard
    } else {
        return None;
    };
    (cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("their"))
        && cursor
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("hand")))
    .then_some(action)
}

/// `reveals|discards their hand`, after `player_hand_verb_at` proved the claim.
pub(in crate::oracle::canonical) fn recognize_player_hand_action(
    cursor: &mut OracleTokenCursor<'_>,
    form: VerbAgreement,
) -> MachineResult<PlayerHandVerb> {
    cursor.enter("player-action.hand.verb");
    let action = player_hand_verb_at(cursor, form)
        .ok_or_else(|| cursor.fail("`reveals` or `discards` followed by `their hand`"))?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("player-action.hand.zone");
    cursor.consume_phrase(&["their", "hand"])?;
    Ok(action)
}

/// A player's verb whose object is a bound card reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ReferencedCardVerb {
    /// CR 701.9a: the cards move from their owner's hand to the graveyard.
    Discard,
    /// CR 701.13a: the cards move to exile.
    Exile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerActionNode {
    pub player: PlayerEffectSubject,
    pub action: SingularPlayerAction,
}

/// A nonmatching action restores the cursor and leaves established readers in
/// charge. An action headed by a supported verb owns its complete child tail.
pub(in crate::oracle::canonical) fn try_recognize_player_action(
    cursor: &mut OracleTokenCursor<'_>,
    binding: PlayerBinding,
    antecedent: Option<PlayerBinding>,
    inherited: Option<&PlayerEffectSubject>,
    context: &EffectRecognitionContext,
) -> MachineResult<Option<PlayerActionNode>> {
    try_recognize_player_action_with_object_antecedent(
        cursor, binding, antecedent, inherited, None, context,
    )
}

/// The same child with the singular object antecedent the sequence proved,
/// so `its controller` / `that NOUN's owner` can act as the player subject.
pub(in crate::oracle::canonical) fn try_recognize_player_action_with_object_antecedent(
    cursor: &mut OracleTokenCursor<'_>,
    binding: PlayerBinding,
    antecedent: Option<PlayerBinding>,
    inherited: Option<&PlayerEffectSubject>,
    object_antecedent: Option<ObjectSubjectAntecedent>,
    context: &EffectRecognitionContext,
) -> MachineResult<Option<PlayerActionNode>> {
    let checkpoint = cursor.checkpoint();
    // `the Ring tempts you` (CR 701.54a): the Ring is the grammatical
    // subject and the tempted player its object. The clause owns its
    // failures once `the Ring tempts` is read.
    if super::ring_temptation_starts(cursor) {
        let player = super::recognize_ring_temptation(cursor)?;
        return Ok(Some(PlayerActionNode {
            player,
            action: SingularPlayerAction::Designation(
                super::PlayerDesignationAction::RingTemptation,
            ),
        }));
    }
    // After a causative `have`, the explicit subject takes a bare verb and
    // keeps every other argument of the same action (`have target player
    // lose 3 life`, `have that player discard a card`).
    let bare = context.subject_verb_agreement == SubjectVerbAgreement::BareInfinitive;
    let head = cursor.peek().unwrap_or("");
    let explicit_target = head.eq_ignore_ascii_case("target")
        && cursor.peek_ahead(1).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("player") || raw.eq_ignore_ascii_case("opponent")
        });
    let explicit_bound = player_anaphor_at(cursor).is_some();
    let explicit_you = inherited.is_some() && head.eq_ignore_ascii_case("you");
    // `they` (CR 109.5: singular they) names the same player `that player`
    // would: the program's player binding, else the event's player. It takes
    // the plural verb form; without such a player it is not claimed here.
    let explicit_they = !bare
        && head.eq_ignore_ascii_case("they")
        && (antecedent.is_some() || context.frame.event_player);
    let plural = bare || explicit_they;
    let object_relation = object_related_player_head_length(cursor).is_some();
    let defending = defending_player_starts(cursor);
    let player = if explicit_target || explicit_bound || explicit_you {
        // Binding validation is tentative until this child owns the action.
        // Another parent may supply a different typed player relationship.
        recognize_player_effect_subject(cursor, binding, antecedent, context.frame.event_player)
    } else if explicit_they {
        cursor.enter("player-subject.pronoun");
        cursor.consume("they")?;
        Ok(antecedent.map_or(PlayerEffectSubject::EventPlayer, PlayerEffectSubject::Bound))
    } else if defending {
        match recognize_defending_player_subject(cursor, context.frame.defending_player) {
            Ok(player) => Ok(player),
            Err(_) => {
                cursor.rollback(checkpoint);
                return Ok(None);
            }
        }
    } else if object_relation {
        // Tentative like the explicit subjects above: without a proven object
        // antecedent the established readers keep their own diagnostics.
        match recognize_object_related_player_subject(cursor, object_antecedent) {
            Ok(player) => Ok(player),
            Err(_) => {
                cursor.rollback(checkpoint);
                return Ok(None);
            }
        }
    } else if bare {
        // A causative names its performer; it never inherits one.
        return Ok(None);
    } else if let Some(player) = inherited {
        Ok(player.clone())
    } else if is_verb_form(&head, VerbLemma::Mill, VerbForm::Base)
        || (is_verb_form(&head, VerbLemma::Get, VerbForm::Base) && player_counters_follow(cursor))
        || (is_verb_form(&head, VerbLemma::Take, VerbForm::Base) && extra_turn_follows(cursor))
        || (is_verb_form(&head, VerbLemma::Skip, VerbForm::Base) && skip_turn_part_follows(cursor))
        || ((is_verb_form(&head, VerbLemma::Become, VerbForm::Base)
            || is_verb_form(&head, VerbLemma::Take, VerbForm::Base))
            && super::player_designation_follows(cursor).is_some())
        || head.eq_ignore_ascii_case("venture")
        || head.eq_ignore_ascii_case("proliferate")
    {
        // Imperative mood: an instruction without a subject is performed by
        // the ability's controller (`Mill two cards.`). Draw keeps its
        // established imperative reader and exact historical IR; life is the
        // shared life clause's (`life_clause.rs`).
        Ok(PlayerEffectSubject::Controller)
    } else {
        return Ok(None);
    };
    let verb = cursor.peek().unwrap_or("").to_ascii_lowercase();
    let is_draw = matches!(verb.as_str(), "draw" | "draws");
    // Lexicon lemmas at the head, in any agreement (the reader checks it).
    let lemma = cursor
        .peek()
        .and_then(|raw| {
            crate::oracle::canonical::automata::lookup_verb(raw, MIGRATED_PLAYER_LEMMAS)
        })
        .map(|(entry, _)| entry.lemma);
    let is_mill = lemma == Some(VerbLemma::Mill);
    // `reveals|discards their hand`: the possessive must name the acting
    // player's own hand; other objects stay with their own readers.
    let hand_form = if plural {
        VerbAgreement::Base
    } else {
        VerbAgreement::ThirdPersonSingular
    };
    let is_reveal_hand = player_hand_verb_at(cursor, hand_form).is_some();
    let is_venture = matches!(verb.as_str(), "venture" | "ventures")
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("into"));
    // Imperative `discard`/`sacrifice` of the controller keeps its established
    // readers; these third-person actions belong to a non-controller subject.
    let agreement = if plural {
        VerbAgreement::Base
    } else {
        VerbAgreement::ThirdPersonSingular
    };
    let is_third_person_other =
        crate::oracle::canonical::automata::lemma_at(cursor, VerbLemma::Discard, agreement)
            || crate::oracle::canonical::automata::lemma_at(
                cursor,
                VerbLemma::Sacrifice,
                agreement,
            );
    // `exiles that card|it`: only a bound card reference is claimed; a
    // counted exile (`exiles a card from their hand`) has no reader yet.
    let is_referenced_exile =
        crate::oracle::canonical::automata::lemma_at(cursor, VerbLemma::Exile, agreement)
            && referenced_card_operand_follows(cursor);
    let is_proliferate = verb == "proliferate" || (!plural && verb == "proliferates");
    let is_counters = lemma == Some(VerbLemma::Get) && player_counters_follow(cursor);
    // `target player creates TOKENS` (CR 111.2): the declared player is the
    // creator. Other creating subjects keep their established readers.
    let is_create =
        crate::oracle::canonical::automata::lemma_at(cursor, VerbLemma::Create, agreement);
    // `target player scries N`: the imperative `scry` keeps its own reader.
    let is_scry = crate::oracle::canonical::automata::lemma_at(cursor, VerbLemma::Scry, agreement);
    // `take(s) an extra turn after this one`: every player subject, and the
    // imperative of the controller (CR 500.7). The fragment checks the
    // subject's agreement.
    let is_extra_turn = extra_turn_follows(cursor);
    // `becomes the monarch` / `takes the initiative`, `skips their next
    // ...`, `can't cast spells this turn`: every player subject.
    let is_designation = super::player_designation_follows(cursor).is_some();
    let is_skip = skip_turn_part_follows(cursor);
    let is_prohibition = player_prohibition_follows(cursor);
    // `loses the game`, `gains hexproof until end of turn`: the acting
    // player's own status, for every player subject.
    let is_status = player_status_action_follows(cursor);
    let claimed = is_counters
        || is_status
        || is_extra_turn
        || is_designation
        || is_skip
        || is_prohibition
        || is_create
        || is_scry
        || is_draw
        || is_mill
        || is_venture
        || is_proliferate
        || is_third_person_other
        || is_reveal_hand
        || is_referenced_exile;
    // The shared sequence can still inherit a declared player for the next verb.
    if !claimed
        && !explicit_target
        && !explicit_bound
        && !explicit_you
        && !object_relation
        && !defending
        && matches!(inherited, Some(PlayerEffectSubject::Bound(_)))
        && cursor
            .peek_token()
            .is_some_and(|token| token.token_type == "actionWord")
    {
        cursor.enter("player-subject.action.scope");
        return Err(cursor.fail(
            "a represented action for the inherited player subject, an explicit actor, or a new sentence",
        ));
    }
    if !claimed
        // An unresolved subject (`that player` without antecedent) leaves
        // the other established parents in charge instead of committing.
        || ((is_third_person_other || is_reveal_hand || is_referenced_exile)
            && !matches!(&player, Ok(subject) if *subject != PlayerEffectSubject::Controller))
        || (is_create && !matches!(&player, Ok(PlayerEffectSubject::DeclaredTarget { .. })))
        || (is_scry && !matches!(&player, Ok(subject) if *subject != PlayerEffectSubject::Controller))
        // An unresolved subject leaves the performer reader in charge.
        || ((is_extra_turn || is_designation || is_skip || is_prohibition || is_status)
            && player.is_err())
    {
        cursor.rollback(checkpoint);
        return Ok(None);
    }
    let player = player?;
    let form = match player {
        _ if plural => VerbAgreement::Base,
        PlayerEffectSubject::Controller => VerbAgreement::Base,
        PlayerEffectSubject::DeclaredTarget { .. }
        | PlayerEffectSubject::Bound(_)
        | PlayerEffectSubject::EventPlayer
        | PlayerEffectSubject::ObjectRelation { .. } => VerbAgreement::ThirdPersonSingular,
    };
    // `ActingSubject` is the shared conjugation of the selecting verbs: the
    // bare surface keeps the third-person chooser (`of their choice`).
    let other_form = if plural {
        ActingSubject::THIRD_PLURAL
    } else {
        ActingSubject::THIRD_SINGULAR
    };
    // The acting player's own turn parts: `your` for the controller, else
    // `their` (CR 109.5).
    let possessive = if player == PlayerEffectSubject::Controller {
        "your"
    } else {
        "their"
    };
    let action = if is_designation {
        SingularPlayerAction::Designation(super::PlayerDesignationAction::Acquire(
            super::recognize_player_designation_fragment(cursor, form)?,
        ))
    } else if is_status {
        SingularPlayerAction::Status(recognize_player_status_fragment(
            cursor,
            form,
            context.preposed_duration.as_ref(),
        )?)
    } else if is_skip {
        SingularPlayerAction::Skip(recognize_skip_turn_part_fragment(cursor, form, possessive)?)
    } else if is_prohibition {
        SingularPlayerAction::Prohibition(recognize_player_prohibition_fragment(cursor)?)
    } else if is_create {
        cursor.enter("player-action.create-tokens.verb");
        cursor.consume(
            crate::oracle::canonical::automata::verb_entry(VerbLemma::Create).surface(form.form()),
        )?;
        SingularPlayerAction::CreateTokens(Box::new(
            super::recognize_performed_token_creation_body(cursor, None)?,
        ))
    } else if is_extra_turn {
        recognize_extra_turn_fragment(cursor, form)?;
        SingularPlayerAction::ExtraTurn
    } else if is_scry {
        cursor.enter("player-action.scry.verb");
        SingularPlayerAction::Scry(super::recognize_player_scry_fragment(
            cursor,
            crate::oracle::canonical::automata::verb_entry(VerbLemma::Scry).surface(form.form()),
        )?)
    } else if is_reveal_hand {
        SingularPlayerAction::Hand(recognize_player_hand_action(cursor, form)?)
    } else if is_referenced_exile {
        cursor.enter("player-action.referenced-cards.exile");
        cursor.consume(
            crate::oracle::canonical::automata::verb_entry(VerbLemma::Exile)
                .surface(agreement.form()),
        )?;
        let cards = recognize_referenced_card_operand(cursor)?;
        SingularPlayerAction::ReferencedCards {
            verb: ReferencedCardVerb::Exile,
            cards,
        }
    } else if lemma == Some(VerbLemma::Discard) && referenced_card_operand_follows(cursor) {
        cursor.enter("player-action.referenced-cards.discard");
        cursor.consume(
            super::verb_entry(super::VerbLemma::Discard).surface(other_form.agreement.form()),
        )?;
        let cards = recognize_referenced_card_operand(cursor)?;
        SingularPlayerAction::ReferencedCards {
            verb: ReferencedCardVerb::Discard,
            cards,
        }
    } else if is_draw {
        SingularPlayerAction::Draw(recognize_draw_action_fragment(cursor, form, context)?)
    } else if is_mill {
        SingularPlayerAction::Mill(recognize_mill_action_fragment(cursor, form)?)
    } else if is_proliferate {
        cursor.enter("player-action.proliferate.verb");
        cursor.consume(match form {
            VerbAgreement::Base => "proliferate",
            VerbAgreement::ThirdPersonSingular => "proliferates",
        })?;
        // `proliferate twice` / `proliferate X times`: the shared repetition
        // adjunct; each proliferation is its own keyword action.
        SingularPlayerAction::Proliferate(if super::repetition_adjunct_starts(cursor) {
            super::recognize_repetition_adjunct(cursor)?
        } else {
            NumericExpression::from(1_i64)
        })
    } else if is_venture {
        SingularPlayerAction::Venture(recognize_venture_action_fragment(cursor, form)?)
    } else if is_counters {
        SingularPlayerAction::Counters(recognize_player_counters_fragment(cursor, form)?)
    } else if lemma == Some(VerbLemma::Discard) {
        SingularPlayerAction::Discard(recognize_player_discard_action_fragment(
            cursor,
            player.clone(),
            other_form,
        )?)
    } else {
        SingularPlayerAction::Sacrifice(recognize_player_sacrifice_action_with_form(
            cursor, other_form,
        )?)
    };
    Ok(Some(PlayerActionNode { player, action }))
}

/// `SUBJECT and SUBJECT each ACTION [and|, then ACTION]...` (CR 109.5): one
/// sentence whose coordinated player subjects each perform the same ordered
/// actions. The verbs take the plural base form after `each`. The result is
/// the distributed per-player list, action-major: every subject completes one
/// action before the next action begins (CR 608.2c). A declared target subject
/// keeps one declaration; its later uses are bound to it. A subject is any
/// singular player subject of the shared readers: `you`, a declared target
/// player, the bound `that player` (the event's player included) and
/// `defending player`.
///
/// Returns `Ok(None)` (cursor restored) unless the coordinated subject frame
/// `... and ... each` is present; after `each` the frame owns its failures.
pub(in crate::oracle::canonical) fn try_recognize_coordinated_player_actions(
    cursor: &mut OracleTokenCursor<'_>,
    first_effect_index: usize,
    antecedent: Option<PlayerBinding>,
    context: &EffectRecognitionContext,
) -> MachineResult<Option<Vec<PlayerActionNode>>> {
    let checkpoint = cursor.checkpoint();
    let starts_subject = |cursor: &OracleTokenCursor<'_>| {
        defending_player_starts(cursor)
            || player_anaphor_at(cursor).is_some()
            || cursor.peek().is_some_and(|raw| {
                raw.eq_ignore_ascii_case("you") || raw.eq_ignore_ascii_case("target")
            })
    };
    if !starts_subject(cursor) {
        return Ok(None);
    }
    // Subjects are read tentatively: a target slot is bound to the effect
    // index its first distributed action will occupy, fixed below.
    let mut subjects = Vec::new();
    loop {
        let subject = if defending_player_starts(cursor) {
            recognize_defending_player_subject(cursor, context.frame.defending_player)
        } else {
            recognize_player_effect_subject(
                cursor,
                PlayerBinding(usize::MAX),
                antecedent,
                context.frame.event_player,
            )
        };
        let Ok(subject) = subject else {
            cursor.rollback(checkpoint);
            return Ok(None);
        };
        subjects.push(subject);
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
            && subjects.len() == 1
        {
            cursor.consume("and")?;
            if !starts_subject(cursor) {
                cursor.rollback(checkpoint);
                return Ok(None);
            }
            continue;
        }
        break;
    }
    if subjects.len() < 2
        || !cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("each"))
    {
        cursor.rollback(checkpoint);
        return Ok(None);
    }
    cursor.enter("player-subject.coordinated.each");
    cursor.consume("each")?;
    // `each` distributes the actions over the written subjects; the same
    // written subject twice is no coordination.
    cursor.require(
        subjects
            .iter()
            .enumerate()
            .all(|(index, subject)| !subjects[..index].contains(subject)),
        "distinct coordinated player subjects",
    )?;
    let mut actions = Vec::new();
    loop {
        cursor.enter("player-subject.coordinated.action");
        let Some(action) = recognize_coordinated_player_action(cursor, &subjects[0], context)?
        else {
            return Err(cursor.fail(
                "a supported coordinated player action (`draw`, `mill`, `discard`, `sacrifice`, `get` counters, `gain`, or `lose`)",
            ));
        };
        actions.push(action);
        // `and VERB`, `, then VERB`, `then VERB`: the same subjects' next
        // action (CR 608.2c).
        let list_checkpoint = cursor.checkpoint();
        let width = match (cursor.peek(), cursor.peek_ahead(1)) {
            (Some(","), Some(next)) if next.eq_ignore_ascii_case("then") => 2,
            (Some(word), _) if word.eq_ignore_ascii_case("and") => 1,
            (Some(word), _) if word.eq_ignore_ascii_case("then") => 1,
            _ => 0,
        };
        if width > 0 {
            cursor.advance_to(cursor.position() + width)?;
            if coordinated_player_action_starts(cursor) {
                continue;
            }
        }
        cursor.rollback(list_checkpoint);
        break;
    }
    // Distribute action-major; the first use of a declared target declares
    // it at its own effect index, every later use is bound to that index.
    let mut distributed = Vec::new();
    let mut declared: Vec<Option<PlayerBinding>> = vec![None; subjects.len()];
    for action in &actions {
        for (index, subject) in subjects.iter().enumerate() {
            let effect_index = first_effect_index + distributed.len();
            let player = match (subject, declared[index]) {
                (PlayerEffectSubject::DeclaredTarget { target, .. }, None) => {
                    let binding = PlayerBinding(effect_index);
                    declared[index] = Some(binding);
                    PlayerEffectSubject::DeclaredTarget {
                        target: target.clone(),
                        binding,
                    }
                }
                (PlayerEffectSubject::DeclaredTarget { .. }, Some(binding)) => {
                    PlayerEffectSubject::Bound(binding)
                }
                (other, _) => other.clone(),
            };
            let mut action = action.clone();
            // The discard node repeats its acting player.
            if let SingularPlayerAction::Discard(discard) = &mut action {
                discard.player = player.clone();
            }
            distributed.push(PlayerActionNode { player, action });
        }
    }
    Ok(Some(distributed))
}

/// True when a verb of a coordinated player action starts at the cursor in
/// the plural base form. Lexical lookahead only.
fn coordinated_player_action_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().is_some_and(|raw| {
        [
            VerbLemma::Draw,
            VerbLemma::Mill,
            VerbLemma::Discard,
            VerbLemma::Sacrifice,
            VerbLemma::Create,
        ]
        .iter()
        .any(|lemma| is_verb_form(raw, *lemma, VerbForm::Base))
    }) || player_counters_follow(cursor)
        || supplied_life_action_starts(cursor, VerbAgreement::Base)
}

/// One action of coordinated player subjects, in the plural base form, read
/// by the same children as a singular subject's action. `Ok(None)` when no
/// coordinated action starts here. `subject` fills the acting player the
/// discard node repeats; the distribution rewrites it per subject.
fn recognize_coordinated_player_action(
    cursor: &mut OracleTokenCursor<'_>,
    subject: &PlayerEffectSubject,
    context: &EffectRecognitionContext,
) -> MachineResult<Option<SingularPlayerAction>> {
    let head = cursor.peek().unwrap_or("").to_string();
    let base = |lemma: VerbLemma| is_verb_form(&head, lemma, VerbForm::Base);
    Ok(Some(if base(VerbLemma::Draw) {
        SingularPlayerAction::Draw(recognize_draw_action_fragment(
            cursor,
            VerbAgreement::Base,
            context,
        )?)
    } else if base(VerbLemma::Mill) {
        SingularPlayerAction::Mill(recognize_mill_action_fragment(cursor, VerbAgreement::Base)?)
    } else if base(VerbLemma::Discard) {
        SingularPlayerAction::Discard(recognize_player_discard_action_fragment(
            cursor,
            subject.clone(),
            ActingSubject::THIRD_PLURAL,
        )?)
    } else if base(VerbLemma::Sacrifice) {
        SingularPlayerAction::Sacrifice(recognize_player_sacrifice_action_with_form(
            cursor,
            ActingSubject::THIRD_PLURAL,
        )?)
    } else if base(VerbLemma::Create) {
        // CR 111.2: each subject creates and controls its own tokens.
        cursor.enter("player-subject.coordinated.create-tokens");
        cursor.consume(super::verb_entry(VerbLemma::Create).surface(VerbForm::Base))?;
        SingularPlayerAction::CreateTokens(Box::new(
            super::recognize_performed_token_creation_body(cursor, None)?,
        ))
    } else if player_counters_follow(cursor) {
        SingularPlayerAction::Counters(recognize_player_counters_fragment(
            cursor,
            VerbAgreement::Base,
        )?)
    } else {
        match recognize_supplied_life_action(cursor, VerbAgreement::Base, None)? {
            Some(life) => SingularPlayerAction::Life(life),
            None => return Ok(None),
        }
    }))
}

/// True when the verb at the cursor is followed by a bound card reference
/// (`that card`, `it`, `them`, `those cards`) rather than a counted operand.
fn referenced_card_operand_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    let mut trial = cursor.clone();
    if trial.advance_to(trial.position() + 1).is_err() {
        return false;
    }
    recognize_referenced_card_operand(&mut trial).is_ok()
}

fn recognize_referenced_card_operand(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardCollectionReferenceNode> {
    cursor.enter("player-action.referenced-cards.operand");
    match try_recognize_card_collection_reference(cursor) {
        CollectionOperandAttempt::Recognized(fragment)
            if matches!(
                fragment.node,
                CardCollectionReferenceNode::Selected | CardCollectionReferenceNode::Antecedent
            ) =>
        {
            Ok(fragment.node)
        }
        CollectionOperandAttempt::FailedCommitted(failure)
        | CollectionOperandAttempt::RejectedCandidate(failure) => Err(failure),
        _ => Err(cursor.fail("a bound card reference (`that card`, `it`, `them`)")),
    }
}

/// Subject-free `mill N cards` child; the caller resolves actor and agreement.
pub(in crate::oracle::canonical) fn recognize_mill_action_fragment(
    cursor: &mut OracleTokenCursor<'_>,
    form: VerbAgreement,
) -> MachineResult<NumericExpression> {
    cursor.enter("player-action.mill.verb");
    cursor.consume(
        crate::oracle::canonical::automata::verb_entry(VerbLemma::Mill).surface(form.form()),
    )?;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("cards"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("equal"))
    {
        // `mills cards equal to EXPR`: the shared `equal to` quantity.
        cursor.enter("player-action.mill.equal-amount");
        cursor.consume_phrase(&["cards", "equal", "to"])?;
        return super::recognize_equal_to_amount_expression(cursor, None);
    }
    cursor.enter("player-action.mill.count");
    let count = recognize_numeric_expression(cursor, None)?;
    cursor.enter("player-action.mill.cards");
    recognize_card_count_noun(cursor, &count)?;
    Ok(count)
}

/// `take(s) an extra turn` at the cursor, in any agreement: the only object
/// the `take` frame reads (CR 500.7). The other `take` readings (`take the
/// initiative`, `take N damage`) are not claimed.
pub(in crate::oracle::canonical) fn extra_turn_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| lookup_verb(raw, &[VerbLemma::Take]).is_some())
        && ["an", "extra", "turn"]
            .iter()
            .enumerate()
            .all(|(index, word)| {
                cursor
                    .peek_ahead(index + 1)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
            })
}

/// Subject-free `take(s) an extra turn after this one` (CR 500.7); the
/// caller resolves the acting player and the verb agreement. The turn order
/// is part of the frame: an extra turn at another point fails explicitly.
pub(in crate::oracle::canonical) fn recognize_extra_turn_fragment(
    cursor: &mut OracleTokenCursor<'_>,
    form: VerbAgreement,
) -> MachineResult<()> {
    cursor.enter("player-action.extra-turn.verb");
    cursor.consume(
        crate::oracle::canonical::automata::verb_entry(VerbLemma::Take).surface(form.form()),
    )?;
    cursor.enter("player-action.extra-turn.object");
    cursor.consume_phrase(&["an", "extra", "turn"])?;
    cursor.enter("player-action.extra-turn.order");
    cursor.consume_phrase(&["after", "this", "one"])?;
    Ok(())
}

/// True when `skip(s) your|their next ...` is at the cursor, in any
/// agreement: the `skip` frame of a player's own future turn part. A skip
/// without `next` (`skip your draw step`, a static rule) is not claimed.
pub(in crate::oracle::canonical) fn skip_turn_part_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| lookup_verb(raw, &[VerbLemma::Skip]).is_some())
        && ((cursor.peek_ahead(1).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("your") || raw.eq_ignore_ascii_case("their")
        }) && cursor
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("next")))
            // `skip(s) all combat phases of POSSESSIVE next turn`.
            || (word_at(cursor, 1, "all") && word_at(cursor, 2, "combat")))
}

/// Subject-free `skip(s) POSSESSIVE next [N] PART [this turn]` (CR 500.11);
/// the caller resolves the acting player, the agreement and its possessive.
/// A missing or unknown turn part fails here explicitly.
pub(in crate::oracle::canonical) fn recognize_skip_turn_part_fragment(
    cursor: &mut OracleTokenCursor<'_>,
    form: VerbAgreement,
    possessive: &str,
) -> MachineResult<SkipTurnPartNode> {
    cursor.enter("player-action.skip.verb");
    cursor.consume(super::verb_entry(VerbLemma::Skip).surface(form.form()))?;
    if word_at(cursor, 0, "all") {
        // Every combat phase of the acting player's next turn (CR 506.1:
        // a turn may have more than one combat phase).
        cursor.enter("player-action.skip.all-combat-phases");
        cursor.consume_phrase(&["all", "combat", "phases", "of", possessive, "next", "turn"])?;
        return Ok(SkipTurnPartNode {
            part: SkippedTurnPart::AllCombatPhasesOfNextTurn,
            count: 1,
            this_turn: false,
        });
    }
    cursor.enter("player-action.skip.possessive");

    cursor.consume(possessive)?;
    cursor.consume("next")?;
    cursor.enter("player-action.skip.count");
    let count = if cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "quantity")
    {
        match super::recognize_numeric_atom(cursor)? {
            super::NumericAtom::Integer(value) if value > 0 => u32::try_from(value)
                .map_err(|_| cursor.fail("a literal number of skipped turns"))?,
            _ => return Err(cursor.fail("a positive literal number of skipped turns")),
        }
    } else {
        1
    };
    cursor.enter("player-action.skip.part");
    let word = |cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let part = if word(cursor, 0, "turn") || word(cursor, 0, "turns") {
        cursor.consume(if count == 1 { "turn" } else { "turns" })?;
        SkippedTurnPart::Turn
    } else {
        let (part, phrase): (SkippedTurnPart, &[&str]) = if word(cursor, 0, "untap") {
            (SkippedTurnPart::UntapStep, &["untap", "step"])
        } else if word(cursor, 0, "draw") {
            (SkippedTurnPart::DrawStep, &["draw", "step"])
        } else if word(cursor, 0, "combat") {
            (SkippedTurnPart::CombatPhase, &["combat", "phase"])
        } else {
            return Err(cursor.fail(
                "the skipped turn part (`turn`, `untap step`, `draw step`, `combat phase`)",
            ));
        };
        cursor.require(count == 1, "one skipped step or phase")?;
        cursor.consume_phrase(phrase)?;
        part
    };
    cursor.enter("player-action.skip.this-turn");
    let this_turn = word(cursor, 0, "this") && word(cursor, 1, "turn");
    if this_turn {
        cursor.require(
            part != SkippedTurnPart::Turn,
            "a step or phase of the current turn after `this turn`",
        )?;
        cursor.consume_phrase(&["this", "turn"])?;
    }
    Ok(SkipTurnPartNode {
        part,
        count,
        this_turn,
    })
}

/// True when `can't VERB` at the cursor is a prohibition on the acting
/// player's own action (`can't cast spells`, `can't play lands`).
pub(in crate::oracle::canonical) fn player_prohibition_follows(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .and_then(super::lookup_auxiliary)
        .is_some_and(|(auxiliary, _)| auxiliary == super::Auxiliary::Cannot)
        && cursor.peek_ahead(1).is_some_and(|raw| {
            lookup_verb(raw, &[VerbLemma::Cast, VerbLemma::Play])
                .is_some_and(|(_, forms)| forms.contains(&VerbForm::Base))
        })
}

/// Subject-free `can't cast spells this turn` (CR 101.2): the acting
/// player's prohibition for the rest of the turn. A prohibition the engine
/// does not run (`can't play lands`, a qualified spell) fails explicitly.
pub(in crate::oracle::canonical) fn recognize_player_prohibition_fragment(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerProhibition> {
    cursor.enter("player-action.prohibition.modal");
    cursor.require(
        player_prohibition_follows(cursor),
        "`can't` and the prohibited action",
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("player-action.prohibition.action");
    cursor.require(
        cursor
            .peek()
            .is_some_and(|raw| is_verb_form(raw, VerbLemma::Cast, VerbForm::Base)),
        "`cast spells`, the player prohibition the runtime installs",
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("player-action.prohibition.spells");
    cursor.consume("spells")?;
    cursor.enter("player-action.prohibition.duration");
    cursor.consume_phrase(&["this", "turn"])?;
    Ok(PlayerProhibition::CastSpells)
}

/// `venture into` opens the imperative keyword action (CR 701.49).
pub(in crate::oracle::canonical) fn venture_action_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("venture"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("into"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct VentureActionNode {
    /// `None` for `the dungeon` (any Dungeon without an entry restriction).
    pub dungeon: Option<String>,
}

/// Subject-free `venture into the dungeon|NAME` child (CR 701.49).
pub(in crate::oracle::canonical) fn recognize_venture_action_fragment(
    cursor: &mut OracleTokenCursor<'_>,
    form: VerbAgreement,
) -> MachineResult<VentureActionNode> {
    cursor.enter("player-action.venture.verb");
    cursor.consume(match form {
        VerbAgreement::Base => "venture",
        VerbAgreement::ThirdPersonSingular => "ventures",
    })?;
    cursor.consume("into")?;
    cursor.enter("player-action.venture.dungeon");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("dungeon"))
    {
        cursor.consume_phrase(&["the", "dungeon"])?;
        return Ok(VentureActionNode { dungeon: None });
    }
    let mut words = Vec::new();
    while let Some(raw) = cursor.peek() {
        let name_word = raw.starts_with(|character: char| character.is_ascii_uppercase())
            && raw.chars().all(|character| {
                character.is_alphanumeric() || character == '\'' || character == '\u{2019}'
            });
        if !name_word {
            break;
        }
        words.push(raw.to_string());
        cursor.advance_to(cursor.position() + 1)?;
    }
    cursor.require(!words.is_empty(), "`the dungeon` or a Dungeon name")?;
    Ok(VentureActionNode {
        dungeon: Some(words.join(" ")),
    })
}

/// The printed reminder of `venture into the dungeon` restates CR 701.49a:
/// `(Enter the first room or advance to the next room.)`. A named Dungeon
/// has its own entry rules and no reminder grammar here. Owns both
/// parentheses.
pub(in crate::oracle::canonical) fn recognize_venture_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    venture: &VentureActionNode,
) -> MachineResult<()> {
    cursor.enter("player-action.venture.reminder.open");
    cursor.consume("(")?;
    cursor.enter("player-action.venture.reminder.dungeon");
    cursor.require(
        venture.dungeon.is_none(),
        "the reminder of venturing into any dungeon",
    )?;
    cursor.enter("player-action.venture.reminder.body");
    cursor.consume_phrase(&[
        "Enter", "the", "first", "room", "or", "advance", "to", "the", "next", "room", ".",
    ])?;
    cursor.enter("player-action.venture.reminder.close");
    cursor.consume(")")
}

/// Declarations become bound uses in grammatical memory, never cloned targets.
pub(in crate::oracle::canonical) fn continuing_player_subject(
    player: &PlayerEffectSubject,
) -> PlayerEffectSubject {
    match player {
        PlayerEffectSubject::DeclaredTarget { binding, .. } => PlayerEffectSubject::Bound(*binding),
        other => other.clone(),
    }
}

#[cfg(test)]
#[path = "player_action_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "player_hand_action_tests.rs"]
mod hand_tests;
