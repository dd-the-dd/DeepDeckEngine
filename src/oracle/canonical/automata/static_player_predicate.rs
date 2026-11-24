//! Player subjects and player predicates of the static frame (CR 604.1,
//! 611.3): the continuous effects a static ability applies to players.
//!
//! ```text
//! PlayerSubject := "you" | PlayerSet | ATTACHED NOUN "'s" "controller"
//! PossessiveSubject := ("your" | PlayerSet "'s") -- `each opponent's`
//! PlayerSet     := shared player population (`players`, `each player`,
//!                  `your opponents`, `each opponent`, `each of your opponents`)
//! Predicate     := HAVE PLAYER-KEYWORD-LIST
//!                | PROHIBITION Action ("or" Action)*
//!                | PLAY "with" POSSESSIVE ("hand" | "hands") "revealed"
//!                | "may play an additional land on each of" POSSESSIVE "turns"
//!                | SKIP POSSESSIVE "draw" ("step" | "steps")
//! Possessed     := "maximum hand size is" (N | ("increased" | "reduced") "by" N)
//! Action        := "gain life" | "lose the game" | "play lands"
//!                | "cast" SPELL-COLLECTION [CastWindow]
//!                | "cast more than" N SPELL-COLLECTION "each turn"
//!                | "activate abilities" ["of" OBJECT-COLLECTION]
//!                | "draw more than" N ("card" | "cards") "each turn"
//!                | "untap more than" N OBJECT-COLLECTION "during" POSSESSIVE
//!                  "untap" ("step" | "steps")
//! CastWindow    := "during combat" | "during your turn"
//! ```
//!
//! The subject is the shared player population, read once; every predicate
//! of the list applies to that population. Each action is an independent
//! typed child under one prohibition word: `can't gain life or pay life`
//! reads both actions or fails on the second, it never drops one. Spell and
//! object qualifiers are the shared object-collection grammar.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    NumericAtom, ObjectQueryNode, PlayerSetReferenceNode, SpellCastQueryNode,
    recognize_numeric_atom, recognize_object_collection, recognize_player_set_reference,
    recognize_spell_cast_collection,
};

/// The players a static player predicate applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum StaticPlayerSubjectNode {
    /// `you`: the ability's controller.
    You,
    /// A shared player population.
    Players(PlayerSetReferenceNode),
    /// `enchanted creature's controller` (CR 303.4, 301.5): the controller of
    /// the permanent the source is attached to.
    AttachedPermanentController,
}

/// Grammatical number of a player subject, for verb agreement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerSubjectNumber {
    /// `you` and plural populations take the base form (`you have`).
    Base,
    /// `each opponent`, `each player`, `enchanted creature's controller`.
    ThirdPersonSingular,
}

/// A read player subject with the number its verbs agree with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct StaticPlayerSubjectReading {
    pub subject: StaticPlayerSubjectNode,
    pub number: PlayerSubjectNumber,
    /// The subject was possessive (`your`, `each opponent's`): a possessed
    /// player characteristic follows.
    pub possessive: bool,
}

/// A keyword a player can have (CR 702.11d hexproof, 702.18d shroud).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerKeywordNode {
    Hexproof,
    Shroud,
}

/// When a spell-casting prohibition applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CastWindowNode {
    /// `during combat` (CR 506.1).
    Combat,
    /// `during your turn`: the ability controller's turn.
    ControllerTurn,
}

/// One player action a static prohibition forbids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ProhibitedPlayerActionNode {
    /// CR 119.8.
    GainLife,
    /// CR 104.3.
    LoseGame,
    /// CR 305.1.
    PlayLands,
    /// `cast SPELLS [during …]`; a bare `spells` has no qualifier.
    CastSpells {
        spells: SpellCastQueryNode,
        window: Option<CastWindowNode>,
    },
    /// `cast more than N SPELLS each turn`.
    CastMoreThan {
        maximum: i64,
        spells: SpellCastQueryNode,
    },
    /// `activate abilities [of OBJECTS]` (CR 602): activated abilities that
    /// aren't mana abilities unless the text says otherwise.
    ActivateAbilities { sources: Option<ObjectQueryNode> },
    /// `draw more than N cards each turn` (CR 121.2).
    DrawMoreThan { maximum: i64 },
    /// `untap more than N OBJECTS during POSSESSIVE untap step` (CR 502.3).
    UntapMoreThan {
        maximum: i64,
        objects: ObjectQueryNode,
    },
}

/// A change to a player's maximum hand size (CR 402.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum HandSizeChangeNode {
    Set(i64),
    /// A signed adjustment: `increased by N` (+N) or `reduced by N` (-N).
    Adjust(i64),
}

/// One continuous predicate of a player subject.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum StaticPlayerPredicateNode {
    HasKeywords(Vec<PlayerKeywordNode>),
    Prohibited(Vec<ProhibitedPlayerActionNode>),
    /// `play with their hands revealed` (CR 402.3 exception).
    PlayWithHandsRevealed,
    MaximumHandSize(HandSizeChangeNode),
    /// `may play an additional land on each of their turns` (CR 305.2).
    AdditionalLandPlay {
        count: i64,
    },
    /// `skip your draw step` (CR 500.11).
    SkipDrawStep,
}

fn word_is(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor.peek_ahead(offset).is_some_and(|raw| {
        raw.replace('\u{2019}', "'")
            .eq_ignore_ascii_case(&expected.replace('\u{2019}', "'"))
    })
}

fn consume_current(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a token"))?
        .to_string();
    cursor.consume(&raw)
}

/// `can't` / `cannot` at `offset`.
pub(in crate::oracle::canonical) fn player_prohibition_word_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> bool {
    word_is(cursor, offset, "can't") || word_is(cursor, offset, "cannot")
}

/// A player subject at the cursor and its token width, without consuming.
/// `you control` belongs to the control-change frame, `you may` to the
/// optional permission frame; neither is a player-predicate subject here.
pub(in crate::oracle::canonical) fn static_player_subject_width(
    cursor: &OracleTokenCursor<'_>,
) -> Option<(StaticPlayerSubjectNode, usize)> {
    if word_is(cursor, 0, "you") {
        return Some((StaticPlayerSubjectNode::You, 1));
    }
    if word_is(cursor, 0, "each")
        && word_is(cursor, 1, "of")
        && word_is(cursor, 2, "your")
        && word_is(cursor, 3, "opponents")
    {
        return Some((
            StaticPlayerSubjectNode::Players(PlayerSetReferenceNode::OpponentsOfAbilityController),
            4,
        ));
    }
    if (word_is(cursor, 0, "enchanted") || word_is(cursor, 0, "equipped"))
        && cursor.peek_ahead(1).is_some_and(|raw| {
            let raw = raw.replace('\u{2019}', "'").to_ascii_lowercase();
            raw == "creature's" || raw == "permanent's"
        })
        && word_is(cursor, 2, "controller")
    {
        return Some((StaticPlayerSubjectNode::AttachedPermanentController, 3));
    }
    let mut trial = cursor.clone();
    let players = recognize_player_set_reference(&mut trial).ok()?;
    Some((
        StaticPlayerSubjectNode::Players(players),
        trial.position() - cursor.position(),
    ))
}

/// A possessive player subject (`your`, `each opponent's`, `each player's`)
/// followed by a possessed player characteristic this frame reads.
pub(in crate::oracle::canonical) fn static_player_possessive_width(
    cursor: &OracleTokenCursor<'_>,
) -> Option<(StaticPlayerSubjectNode, usize)> {
    let possessed_at = |offset: usize| {
        word_is(cursor, offset, "maximum")
            && word_is(cursor, offset + 1, "hand")
            && word_is(cursor, offset + 2, "size")
    };
    if word_is(cursor, 0, "your") && possessed_at(1) {
        return Some((StaticPlayerSubjectNode::You, 1));
    }
    if word_is(cursor, 0, "each") && possessed_at(2) {
        let population = match cursor
            .peek_ahead(1)
            .map(|raw| raw.replace('\u{2019}', "'").to_ascii_lowercase())
            .as_deref()
        {
            Some("opponent's") => PlayerSetReferenceNode::OpponentsOfAbilityController,
            Some("player's") => PlayerSetReferenceNode::EachPlayer,
            _ => return None,
        };
        return Some((StaticPlayerSubjectNode::Players(population), 2));
    }
    None
}

/// Whether a player predicate this frame owns starts at the cursor: the
/// commit point of the player subject. Other verbs after a player subject
/// (`you may`, `you control`, `each player sacrifices`) stay with their
/// readers.
pub(in crate::oracle::canonical) fn static_player_predicate_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    (word_is(cursor, 0, "have") || word_is(cursor, 0, "has"))
        || player_prohibition_word_at(cursor, 0)
        || ((word_is(cursor, 0, "play") || word_is(cursor, 0, "plays"))
            && word_is(cursor, 1, "with")
            && (word_is(cursor, 2, "their") || word_is(cursor, 2, "your"))
            && (word_is(cursor, 3, "hand") || word_is(cursor, 3, "hands")))
        || (word_is(cursor, 0, "may")
            && word_is(cursor, 1, "play")
            && word_is(cursor, 3, "additional")
            && word_is(cursor, 4, "land"))
        || ((word_is(cursor, 0, "skip") || word_is(cursor, 0, "skips"))
            && word_is(cursor, 2, "draw"))
        || word_is(cursor, 0, "maximum")
}

/// Reads the player subject at the cursor: a possessive subject, `you`,
/// an attached permanent's controller, `each of your opponents`, or the
/// shared player population.
pub(in crate::oracle::canonical) fn recognize_static_player_subject(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StaticPlayerSubjectReading> {
    cursor.enter("static.player.subject");
    let number = if word_is(cursor, 0, "each")
        || word_is(cursor, 0, "enchanted")
        || word_is(cursor, 0, "equipped")
    {
        PlayerSubjectNumber::ThirdPersonSingular
    } else {
        PlayerSubjectNumber::Base
    };
    if let Some((subject, width)) = static_player_possessive_width(cursor) {
        // The possessive token (`your`, `opponent's`) is the subject.
        for _ in 0..width {
            consume_current(cursor)?;
        }
        return Ok(StaticPlayerSubjectReading {
            subject,
            number,
            possessive: true,
        });
    }
    let subject = match static_player_subject_width(cursor) {
        // The shared population reader owns its own forms and states.
        Some((StaticPlayerSubjectNode::Players(_), _)) if !word_is(cursor, 1, "of") => {
            StaticPlayerSubjectNode::Players(recognize_player_set_reference(cursor)?)
        }
        Some((subject, width)) => {
            for _ in 0..width {
                consume_current(cursor)?;
            }
            subject
        }
        None => {
            return Err(cursor.fail(
                "a player subject: `you`, a player population, or an attached permanent's controller",
            ));
        }
    };
    Ok(StaticPlayerSubjectReading {
        subject,
        number,
        possessive: false,
    })
}

fn consume_agreeing_verb(
    cursor: &mut OracleTokenCursor<'_>,
    number: PlayerSubjectNumber,
    base: &str,
    third_person: &str,
) -> MachineResult<()> {
    let expected = match number {
        PlayerSubjectNumber::Base => base,
        PlayerSubjectNumber::ThirdPersonSingular => third_person,
    };
    cursor.require(
        word_is(cursor, 0, expected),
        "a player-predicate verb agreeing in number with its player subject",
    )?;
    consume_current(cursor)
}

fn consume_possessive(
    cursor: &mut OracleTokenCursor<'_>,
    subject: StaticPlayerSubjectNode,
) -> MachineResult<()> {
    let expected = match subject {
        StaticPlayerSubjectNode::You => "your",
        _ => "their",
    };
    cursor.require(
        word_is(cursor, 0, expected),
        "the possessive agreeing with the player subject (`your` or `their`)",
    )?;
    consume_current(cursor)
}

fn recognize_positive_literal(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<i64> {
    match recognize_numeric_atom(cursor)? {
        NumericAtom::Integer(value) if value > 0 => Ok(value),
        _ => Err(cursor.fail("a positive literal quantity")),
    }
}

/// Reads one player predicate. After a possessive subject (`your maximum
/// hand size`) only a possessed characteristic can follow.
pub(in crate::oracle::canonical) fn recognize_static_player_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    reading: StaticPlayerSubjectReading,
) -> MachineResult<StaticPlayerPredicateNode> {
    let StaticPlayerSubjectReading {
        subject,
        number,
        possessive,
    } = reading;
    if possessive {
        cursor.enter("static.player.hand-size");
        cursor.consume_phrase(&["maximum", "hand", "size", "is"])?;
        if word_is(cursor, 0, "increased") || word_is(cursor, 0, "reduced") {
            let increase = word_is(cursor, 0, "increased");
            consume_current(cursor)?;
            cursor.consume("by")?;
            let amount = recognize_positive_literal(cursor)?;
            return Ok(StaticPlayerPredicateNode::MaximumHandSize(
                HandSizeChangeNode::Adjust(if increase { amount } else { -amount }),
            ));
        }
        let value = match recognize_numeric_atom(cursor)? {
            NumericAtom::Integer(value) if value >= 0 => value,
            _ => return Err(cursor.fail("a fixed nonnegative maximum hand size")),
        };
        return Ok(StaticPlayerPredicateNode::MaximumHandSize(
            HandSizeChangeNode::Set(value),
        ));
    }
    cursor.enter("static.player.predicate");
    if word_is(cursor, 0, "have") || word_is(cursor, 0, "has") {
        consume_agreeing_verb(cursor, number, "have", "has")?;
        return recognize_player_keyword_list(cursor);
    }
    if player_prohibition_word_at(cursor, 0) {
        consume_current(cursor)?;
        let mut actions = vec![recognize_prohibited_player_action(cursor, subject)?];
        // `can't A or B`: each coordinated action is read by the same child
        // and must be supported; an unread action is never dropped.
        while word_is(cursor, 0, "or") || (word_is(cursor, 0, ",") && word_is(cursor, 1, "or")) {
            cursor.enter("static.player.prohibition.connector");
            if word_is(cursor, 0, ",") {
                consume_current(cursor)?;
            }
            cursor.consume("or")?;
            actions.push(recognize_prohibited_player_action(cursor, subject)?);
        }
        return Ok(StaticPlayerPredicateNode::Prohibited(actions));
    }
    if word_is(cursor, 0, "play") || word_is(cursor, 0, "plays") {
        cursor.enter("static.player.revealed-hands");
        consume_agreeing_verb(cursor, number, "play", "plays")?;
        cursor.consume("with")?;
        consume_possessive(cursor, subject)?;
        cursor.require(
            word_is(cursor, 0, "hand") || word_is(cursor, 0, "hands"),
            "`hand` or `hands`",
        )?;
        consume_current(cursor)?;
        cursor.consume("revealed")?;
        return Ok(StaticPlayerPredicateNode::PlayWithHandsRevealed);
    }
    if word_is(cursor, 0, "may") {
        cursor.enter("static.player.additional-land");
        cursor.consume_phrase(&["may", "play"])?;
        let count = recognize_positive_literal(cursor)?;
        cursor.consume("additional")?;
        cursor.consume(if count == 1 { "land" } else { "lands" })?;
        cursor.consume_phrase(&["on", "each", "of"])?;
        consume_possessive(cursor, subject)?;
        cursor.consume("turns")?;
        return Ok(StaticPlayerPredicateNode::AdditionalLandPlay { count });
    }
    if word_is(cursor, 0, "skip") || word_is(cursor, 0, "skips") {
        cursor.enter("static.player.skip-step");
        consume_agreeing_verb(cursor, number, "skip", "skips")?;
        consume_possessive(cursor, subject)?;
        cursor.consume("draw")?;
        cursor.require(
            word_is(cursor, 0, "step") || word_is(cursor, 0, "steps"),
            "`step` or `steps`",
        )?;
        consume_current(cursor)?;
        return Ok(StaticPlayerPredicateNode::SkipDrawStep);
    }
    Err(cursor.fail(
        "a player predicate: `have` a keyword, `can't` an action, `play with hands revealed`, an additional land play, or a skipped draw step",
    ))
}

/// `hexproof`, `shroud`, `hexproof and shroud`.
fn recognize_player_keyword_list(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StaticPlayerPredicateNode> {
    cursor.enter("static.player.keywords");
    let mut keywords = Vec::new();
    loop {
        let keyword = if word_is(cursor, 0, "hexproof") {
            PlayerKeywordNode::Hexproof
        } else if word_is(cursor, 0, "shroud") {
            PlayerKeywordNode::Shroud
        } else {
            return Err(cursor.fail(
                "a keyword a player can have that the engine applies to players (hexproof or shroud)",
            ));
        };
        consume_current(cursor)?;
        keywords.push(keyword);
        if word_is(cursor, 0, "and")
            && (word_is(cursor, 1, "hexproof") || word_is(cursor, 1, "shroud"))
        {
            cursor.consume("and")?;
            continue;
        }
        return Ok(StaticPlayerPredicateNode::HasKeywords(keywords));
    }
}

fn recognize_prohibited_player_action(
    cursor: &mut OracleTokenCursor<'_>,
    subject: StaticPlayerSubjectNode,
) -> MachineResult<ProhibitedPlayerActionNode> {
    cursor.enter("static.player.prohibition.action");
    if word_is(cursor, 0, "gain") && word_is(cursor, 1, "life") {
        cursor.consume_phrase(&["gain", "life"])?;
        return Ok(ProhibitedPlayerActionNode::GainLife);
    }
    if word_is(cursor, 0, "lose") && word_is(cursor, 1, "the") && word_is(cursor, 2, "game") {
        cursor.consume_phrase(&["lose", "the", "game"])?;
        return Ok(ProhibitedPlayerActionNode::LoseGame);
    }
    if word_is(cursor, 0, "play") && word_is(cursor, 1, "lands") {
        cursor.consume_phrase(&["play", "lands"])?;
        return Ok(ProhibitedPlayerActionNode::PlayLands);
    }
    if word_is(cursor, 0, "cast") {
        cursor.consume("cast")?;
        if word_is(cursor, 0, "more") && word_is(cursor, 1, "than") {
            cursor.enter("static.player.prohibition.cast-quota");
            cursor.consume_phrase(&["more", "than"])?;
            let maximum = recognize_positive_literal(cursor)?;
            let spells = recognize_spell_cast_collection(cursor)?;
            cursor.require(
                spells.caster.is_none(),
                "an unqualified spell collection; the prohibited players are its casters",
            )?;
            cursor.enter("static.player.prohibition.cast-quota.period");
            cursor.consume_phrase(&["each", "turn"])?;
            return Ok(ProhibitedPlayerActionNode::CastMoreThan { maximum, spells });
        }
        cursor.enter("static.player.prohibition.cast-spells");
        let spells = recognize_spell_cast_collection(cursor)?;
        cursor.require(
            spells.caster.is_none(),
            "an unqualified spell collection; the prohibited players are its casters",
        )?;
        let window = recognize_optional_cast_window(cursor)?;
        return Ok(ProhibitedPlayerActionNode::CastSpells { spells, window });
    }
    if word_is(cursor, 0, "activate") && word_is(cursor, 1, "abilities") {
        cursor.enter("static.player.prohibition.activation");
        cursor.consume_phrase(&["activate", "abilities"])?;
        let sources = if word_is(cursor, 0, "of") {
            cursor.consume("of")?;
            let objects = recognize_object_collection(cursor)?;
            cursor.require(
                objects.controller.is_none() && !objects.exclude_source,
                "an activation source collection without a controller relation or source exclusion",
            )?;
            Some(objects)
        } else {
            None
        };
        return Ok(ProhibitedPlayerActionNode::ActivateAbilities { sources });
    }
    if word_is(cursor, 0, "draw") && word_is(cursor, 1, "more") {
        cursor.enter("static.player.prohibition.draw-quota");
        cursor.consume_phrase(&["draw", "more", "than"])?;
        let maximum = recognize_positive_literal(cursor)?;
        cursor.consume(if maximum == 1 { "card" } else { "cards" })?;
        cursor.consume_phrase(&["each", "turn"])?;
        return Ok(ProhibitedPlayerActionNode::DrawMoreThan { maximum });
    }
    if word_is(cursor, 0, "untap") && word_is(cursor, 1, "more") {
        cursor.enter("static.player.prohibition.untap-quota");
        cursor.consume_phrase(&["untap", "more", "than"])?;
        let maximum = recognize_positive_literal(cursor)?;
        let objects = recognize_object_collection(cursor)?;
        cursor.require(
            objects.controller.is_none() && !objects.exclude_source,
            "an untapped object collection without its own controller relation",
        )?;
        cursor.enter("static.player.prohibition.untap-quota.step");
        cursor.consume("during")?;
        consume_possessive(cursor, subject)?;
        cursor.consume("untap")?;
        cursor.require(
            word_is(cursor, 0, "step") || word_is(cursor, 0, "steps"),
            "`step` or `steps`",
        )?;
        consume_current(cursor)?;
        return Ok(ProhibitedPlayerActionNode::UntapMoreThan { maximum, objects });
    }
    Err(cursor.fail(
        "a prohibited player action: gain life, lose the game, play lands, cast spells, activate abilities, draw or untap more than N",
    ))
}

/// `during combat` / `during your turn` after a cast prohibition. Any other
/// `during` is left to the parent's trailing condition.
fn recognize_optional_cast_window(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<CastWindowNode>> {
    if word_is(cursor, 0, "during") && word_is(cursor, 1, "combat") {
        cursor.enter("static.player.prohibition.cast-window");
        cursor.consume_phrase(&["during", "combat"])?;
        return Ok(Some(CastWindowNode::Combat));
    }
    if word_is(cursor, 0, "during")
        && word_is(cursor, 1, "your")
        && word_is(cursor, 2, "turn")
        && !word_is(cursor, 3, ",")
    {
        cursor.enter("static.player.prohibition.cast-window");
        cursor.consume_phrase(&["during", "your", "turn"])?;
        return Ok(Some(CastWindowNode::ControllerTurn));
    }
    Ok(None)
}

#[cfg(test)]
#[path = "static_player_predicate_tests.rs"]
mod tests;
