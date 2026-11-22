use crate::oracle::{
    characteristic_vocabulary::{PlayerRole, lookup_player_role},
    syntax::{NameResolutionFailure, ResolvedNameLink},
};

use super::super::{MachineResult, OracleTokenCursor, parse_ordinal_word};
use super::DamageKind;
use super::{
    EventNumericBinding, LocalObjectReference, LocalPlayerReference, ObjectKind, ObjectPredicate,
    PlayerPredicate, SpellCastEventCaster, SpellCastOrdinalSupport, recognize_ordinal_atom,
    recognize_player_reference, recognize_source_reference, recognize_spell_cast_event_atom,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DrawEventException {
    FirstInEachDrawStep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DrawPeriod {
    Turn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum LocalTriggerEventNode {
    SourceEntered {
        source: LocalObjectReference,
    },
    CardDrawn {
        player: PlayerPredicate,
        exception: Option<DrawEventException>,
    },
    SourceAttacked {
        source: LocalObjectReference,
    },
    /// `you cycle SOURCE` (CR 702.29): functions while the card is cycled.
    SourceCycled {
        source: LocalObjectReference,
    },
    LandPlayed {
        player: LocalPlayerReference,
        exclude_source: bool,
    },
    CombatDamageToPlayer {
        source: LocalObjectReference,
        amount: EventNumericBinding,
    },
    OrdinalCardDrawn {
        player: LocalPlayerReference,
        ordinal: i64,
        period: DrawPeriod,
    },
    /// `the last KIND counter is removed from SOURCE` (CR 122.8, 702.63a):
    /// the removal that leaves the source with no counter of that kind.
    LastCounterRemoved {
        source: LocalObjectReference,
        counter: crate::oracle::counter_name::CounterName,
    },
    /// `SOURCE is dealt [combat|noncombat] damage` (passive of CR 120.3). The
    /// dealt amount is exported for later `that much` expressions.
    SourceDealtDamage {
        source: LocalObjectReference,
        damage: DamageKind,
        amount: EventNumericBinding,
    },
    /// `ACTOR VERB [or VERB]... [CARD]` player event: one shared actor reader
    /// composed with player-event verbs (CR 119.9 life gain, 120 draw, 508.1
    /// attack, 701.9 discard, 702.29 cycling, 701.22 scry, ...). A union
    /// selects one occurrence and shares the actor and the card object.
    /// `one_or_more` reads the plural `one or more PLAYERS` actor (CR 603.2c).
    PlayerEvent {
        actor: super::EventActorScope,
        actions: Vec<super::PlayerEventAction>,
        one_or_more: bool,
        card: Option<super::PlayerEventCardObject>,
    },
    /// `equipped|enchanted creature VERB`: the event subject is the object the
    /// ability source is attached to (CR 301.5a, 303.4a), not the source.
    AttachedObjectEvent {
        role: AttachedObjectRole,
        action: AttachedObjectEventAction,
    },
    /// `SUBJECT deals [combat|noncombat] damage [to RECIPIENT]` (CR 120.1)
    /// for the source or its attached object, beyond the established
    /// `combat damage to a player` atoms.
    DamageDealt {
        subject: DamageDealer,
        complement: super::DamageDealtComplement,
    },
}

/// The object that deals the damage of a local damage event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DamageDealer {
    Source(LocalObjectReference),
    Attached(AttachedObjectRole),
}

/// How the ability source is attached to the event subject.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AttachedObjectRole {
    /// `equipped creature` on an Equipment.
    Equipped,
    /// `enchanted creature` on an Aura.
    Enchanted,
}

/// Object-event verbs observed for the attached object. Each is the same
/// event the source-subject atoms read, with a different subject reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AttachedObjectEventAction {
    DealCombatDamageToPlayer,
    Attack,
    Die,
    BecomeTapped,
    BecomeBlocked,
}

/// Recognizes `equipped|enchanted creature VERB` after the trigger word. The
/// attachment subject is a reference, not a query: the runtime binds it from
/// the source's attachment. The enclosing envelope owns the comma and body.
pub(in crate::oracle::canonical) fn recognize_attached_object_event(
    cursor: &mut OracleTokenCursor<'_>,
    _name_links: &[ResolvedNameLink],
    _name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<LocalTriggerEventNode> {
    cursor.enter("event.attached-object.role");
    let role = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("equipped") => AttachedObjectRole::Equipped,
        Some("enchanted") => AttachedObjectRole::Enchanted,
        _ => return Err(cursor.fail("`equipped` or `enchanted`")),
    };
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("event.attached-object.noun");
    // The attachment fixes the object; the noun restates its card type. Only
    // a creature takes part in combat, so other nouns admit only tapping.
    let noun = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .filter(|raw| {
            ["creature", "land", "artifact", "enchantment", "permanent"].contains(&raw.as_str())
        })
        .ok_or_else(|| cursor.fail("an attached permanent noun"))?;
    cursor.consume(&noun)?;
    cursor.enter("event.attached-object.action");
    if noun != "creature" {
        cursor.consume_phrase(&["becomes", "tapped"])?;
        cursor.enter("event.attached-object.complete");
        return Ok(LocalTriggerEventNode::AttachedObjectEvent {
            role,
            action: AttachedObjectEventAction::BecomeTapped,
        });
    }
    let action = if let Some(complement) = recognize_dealt_event_complement(cursor)? {
        if !complement.is_combat_damage_to_any_player() {
            cursor.enter("event.attached-object.complete");
            return Ok(LocalTriggerEventNode::DamageDealt {
                subject: DamageDealer::Attached(role),
                complement,
            });
        }
        AttachedObjectEventAction::DealCombatDamageToPlayer
    } else if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("attacks"))
    {
        cursor.consume("attacks")?;
        AttachedObjectEventAction::Attack
    } else if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("dies"))
    {
        cursor.consume("dies")?;
        AttachedObjectEventAction::Die
    } else if cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("tapped"))
    {
        cursor.consume_phrase(&["becomes", "tapped"])?;
        AttachedObjectEventAction::BecomeTapped
    } else {
        cursor.consume_phrase(&["becomes", "blocked"])?;
        AttachedObjectEventAction::BecomeBlocked
    };
    cursor.enter("event.attached-object.complete");
    Ok(LocalTriggerEventNode::AttachedObjectEvent { role, action })
}

/// Recognizes the event atom `the last KIND counter is removed from SOURCE`:
/// the passive of the `remove` lemma with the counter as its theme. The
/// enclosing triggered-ability machine owns `when` and the comma.
pub(in crate::oracle::canonical) fn recognize_last_counter_removed_event(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<LocalTriggerEventNode> {
    cursor.enter("event.last-counter-removed.theme");
    cursor.consume_phrase(&["the", "last"])?;
    let counter = super::recognize_counter_name(cursor)?;
    cursor.consume("counter")?;
    cursor.enter("event.last-counter-removed.verb");
    cursor.consume("is")?;
    let participle = cursor
        .peek()
        .filter(|raw| {
            super::is_verb_form(
                raw,
                super::VerbLemma::Remove,
                super::VerbForm::PastParticiple,
            )
        })
        .map(str::to_string)
        .ok_or_else(|| cursor.fail("the passive participle of `remove`"))?;
    cursor.consume(&participle)?;
    cursor.enter("event.last-counter-removed.source");
    cursor.consume("from")?;
    let source = recognize_source_reference(cursor, name_links, name_failure)?;
    Ok(LocalTriggerEventNode::LastCounterRemoved { source, counter })
}

/// Recognizes the event atom `SOURCE is dealt [combat|noncombat] damage`. The
/// enclosing triggered-ability machine owns `whenever` and the comma.
pub(in crate::oracle::canonical) fn recognize_source_dealt_damage_event(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<LocalTriggerEventNode> {
    cursor.enter("event.source-dealt-damage.source");
    let source = recognize_source_reference(cursor, name_links, name_failure)?;
    cursor.enter("event.source-dealt-damage.passive");
    let frame = super::supplied_damage_frame(
        super::VerbAgreement::ThirdPersonSingular,
        super::Voice::Passive,
        super::ClauseModality::Indicative,
        super::DamageArguments::PassiveKind,
    );
    let Some(super::DamageClauseArguments::Kind(damage)) =
        super::recognize_supplied_damage_clause(cursor, &frame)?
    else {
        return Err(cursor.fail("`is dealt` and the damage it is dealt"));
    };
    cursor.enter("event.source-dealt-damage.complete");
    Ok(LocalTriggerEventNode::SourceDealtDamage {
        source,
        damage,
        amount: EventNumericBinding::DamageAmount,
    })
}

/// Recognizes the event atom `SOURCE deals [combat|noncombat] damage [to
/// RECIPIENT]`. The enclosing triggered-ability machine owns `whenever` and
/// the comma. `combat damage to a player` keeps its established atom; every
/// event exports the dealt amount for later `that many` expressions.
pub(in crate::oracle::canonical) fn recognize_combat_damage_to_player_event(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<LocalTriggerEventNode> {
    cursor.enter("event.combat-damage.source");
    let source = recognize_source_reference(cursor, name_links, name_failure)?;
    cursor.enter("event.combat-damage.action");
    let complement = recognize_dealt_event_complement(cursor)?
        .ok_or_else(|| cursor.fail("`deals` and the damage it deals"))?;
    cursor.enter("event.combat-damage.complete");
    if complement.is_combat_damage_to_any_player() {
        return Ok(LocalTriggerEventNode::CombatDamageToPlayer {
            source,
            amount: EventNumericBinding::DamageAmount,
        });
    }
    Ok(LocalTriggerEventNode::DamageDealt {
        subject: DamageDealer::Source(source),
        complement,
    })
}

/// `deals [KIND] damage [to RECIPIENT]` after a singular object subject:
/// the shared damage clause with the event-complement policy.
fn recognize_dealt_event_complement(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<super::DamageDealtComplement>> {
    let frame = super::supplied_damage_frame(
        super::VerbAgreement::ThirdPersonSingular,
        super::Voice::Active,
        super::ClauseModality::Indicative,
        super::DamageArguments::EventComplement,
    );
    match super::recognize_supplied_damage_clause(cursor, &frame)? {
        Some(super::DamageClauseArguments::Complement(complement)) => Ok(Some(complement)),
        Some(_) => Err(cursor.fail("a damage event complement")),
        None => Ok(None),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TriggeringPlayerBinding;

/// A restricted view for the historical unfiltered tax consumer. It is
/// constructed from the shared event atom only after shape validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerSpellCastEventNode {
    pub caster: PlayerPredicate,
    pub triggering_player: TriggeringPlayerBinding,
}

pub(in crate::oracle::canonical) fn recognize_player_casts_spell_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerSpellCastEventNode> {
    let event =
        recognize_spell_cast_event_atom(cursor, SpellCastOrdinalSupport::UnfilteredOrdinalsOnly)?;
    cursor.enter("event.spell-cast.tax-bare-shape");
    cursor.require(
        event.ordinal.is_none()
            && event.query.predicate == ObjectPredicate::ObjectKind(ObjectKind::Spell),
        "an unfiltered spell-cast event without an ordinal for this tax consumer",
    )?;
    let caster = match event.caster {
        SpellCastEventCaster::AnyPlayer => PlayerPredicate::Any,
        SpellCastEventCaster::Opponent => PlayerPredicate::Opponent,
        SpellCastEventCaster::Controller => {
            return Err(cursor.fail("an explicit player or opponent event for this tax consumer"));
        }
    };
    Ok(PlayerSpellCastEventNode {
        caster,
        triggering_player: event.triggering_player,
    })
}

/// Resolves the event-bound player anaphor. The phrase is a committed error
/// without a successful player-bearing event antecedent.
pub(in crate::oracle::canonical) fn recognize_triggering_player_reference(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TriggeringPlayerBinding>,
) -> MachineResult<TriggeringPlayerBinding> {
    cursor.enter("reference.triggering-player.determiner");
    cursor.consume("that")?;
    cursor.enter("reference.triggering-player.noun");
    cursor.consume("player")?;
    cursor.enter("reference.triggering-player.binding");
    antecedent.ok_or_else(|| cursor.fail("a prior triggering-player binding for `that player`"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OrdinalDrawState {
    Player,
    Action,
    Possessive,
    Ordinal,
    Card,
    PeriodRelation,
    Article,
    Period,
    Complete,
}

/// Recognizes `you draw your Nth card in a turn|each turn` as one reusable
/// event atom (CR 121.2: the ordinal counts the draws in the current turn).
/// Trigger words and the trailing comma remain parent-owned.
pub(in crate::oracle::canonical) fn recognize_ordinal_card_drawn_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalTriggerEventNode> {
    let mut state = OrdinalDrawState::Player;
    let mut ordinal = None;
    loop {
        state = match state {
            OrdinalDrawState::Player => {
                cursor.enter("event.ordinal-card-drawn.player");
                cursor.consume("you")?;
                OrdinalDrawState::Action
            }
            OrdinalDrawState::Action => {
                cursor.enter("event.ordinal-card-drawn.action");
                cursor.consume("draw")?;
                OrdinalDrawState::Possessive
            }
            OrdinalDrawState::Possessive => {
                cursor.enter("event.ordinal-card-drawn.possessive");
                cursor.consume("your")?;
                OrdinalDrawState::Ordinal
            }
            OrdinalDrawState::Ordinal => {
                cursor.enter("event.ordinal-card-drawn.ordinal");
                ordinal = Some(recognize_ordinal_atom(cursor)?);
                OrdinalDrawState::Card
            }
            OrdinalDrawState::Card => {
                cursor.enter("event.ordinal-card-drawn.card");
                cursor.consume("card")?;
                OrdinalDrawState::PeriodRelation
            }
            OrdinalDrawState::PeriodRelation => {
                cursor.enter("event.ordinal-card-drawn.period-relation");
                // `in a turn` and `each turn` name the same counting period.
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("each"))
                {
                    cursor.consume("each")?;
                    OrdinalDrawState::Period
                } else {
                    cursor.consume("in")?;
                    OrdinalDrawState::Article
                }
            }
            OrdinalDrawState::Article => {
                cursor.enter("event.ordinal-card-drawn.article");
                cursor.consume("a")?;
                OrdinalDrawState::Period
            }
            OrdinalDrawState::Period => {
                cursor.enter("event.ordinal-card-drawn.period");
                cursor.consume("turn")?;
                OrdinalDrawState::Complete
            }
            OrdinalDrawState::Complete => {
                cursor.enter("event.ordinal-card-drawn.complete");
                return Ok(LocalTriggerEventNode::OrdinalCardDrawn {
                    player: LocalPlayerReference::Controller,
                    ordinal: cursor.require_value(ordinal, "a draw ordinal")?,
                    period: DrawPeriod::Turn,
                });
            }
        };
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TriggerEventAlternativesNode {
    pub events: Vec<LocalTriggerEventNode>,
}

fn player_predicate(role: PlayerRole) -> PlayerPredicate {
    match role {
        PlayerRole::Any => PlayerPredicate::Any,
        PlayerRole::Opponent => PlayerPredicate::Opponent,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrawExceptionState {
    Exception,
    Determiner,
    Ordinal,
    CardAnaphor,
    PlayerAnaphor,
    Action,
    In,
    Each,
    Of,
    Possessive,
    Step,
    Complete,
}

fn recognize_first_in_each_draw_step_exception(
    cursor: &mut OracleTokenCursor<'_>,
    player: PlayerPredicate,
) -> MachineResult<DrawEventException> {
    let mut state = DrawExceptionState::Exception;
    loop {
        state = match state {
            DrawExceptionState::Exception => {
                cursor.enter("event.card-drawn.exception");
                cursor.consume_type("exceptionWord")?;
                DrawExceptionState::Determiner
            }
            DrawExceptionState::Determiner => {
                cursor.enter("event.card-drawn.exception.determiner");
                cursor.consume("the")?;
                DrawExceptionState::Ordinal
            }
            DrawExceptionState::Ordinal => {
                cursor.enter("event.card-drawn.exception.ordinal");
                let token_index = cursor.position();
                let token = cursor
                    .peek_token()
                    .ok_or_else(|| cursor.fail("the first draw ordinal"))?;
                if parse_ordinal_word(&token.raw) != Some(1) {
                    return Err(cursor.fail_with_span(
                        "the first draw ordinal",
                        token_index,
                        token.span_start,
                        token.span_end,
                        token.raw.clone(),
                    ));
                }
                cursor.advance_to(token_index + 1)?;
                DrawExceptionState::CardAnaphor
            }
            DrawExceptionState::CardAnaphor => {
                cursor.enter("event.card-drawn.exception.card-anaphor");
                cursor.consume("one")?;
                DrawExceptionState::PlayerAnaphor
            }
            DrawExceptionState::PlayerAnaphor => {
                cursor.enter("event.card-drawn.exception.player-anaphor");
                cursor.require(
                    matches!(player, PlayerPredicate::Any | PlayerPredicate::Opponent),
                    "a player binding for `they`",
                )?;
                cursor.consume("they")?;
                DrawExceptionState::Action
            }
            DrawExceptionState::Action => {
                cursor.enter("event.card-drawn.exception.action");
                cursor.consume("draw")?;
                DrawExceptionState::In
            }
            DrawExceptionState::In => {
                cursor.enter("event.card-drawn.exception.in");
                cursor.consume("in")?;
                DrawExceptionState::Each
            }
            DrawExceptionState::Each => {
                cursor.enter("event.card-drawn.exception.each");
                cursor.consume("each")?;
                DrawExceptionState::Of
            }
            DrawExceptionState::Of => {
                cursor.enter("event.card-drawn.exception.of");
                cursor.consume("of")?;
                DrawExceptionState::Possessive
            }
            DrawExceptionState::Possessive => {
                cursor.enter("event.card-drawn.exception.possessive");
                cursor.consume("their")?;
                DrawExceptionState::Step
            }
            DrawExceptionState::Step => {
                cursor.enter("event.card-drawn.exception.step");
                cursor.consume("draw")?;
                let step = cursor.consume_type("turnStepWord")?;
                cursor.require(
                    step.raw.eq_ignore_ascii_case("steps"),
                    "the plural `draw steps` period",
                )?;
                DrawExceptionState::Complete
            }
            DrawExceptionState::Complete => {
                cursor.enter("event.card-drawn.exception.complete");
                return Ok(DrawEventException::FirstInEachDrawStep);
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrawEventState {
    Article,
    Player,
    Action,
    ArticleCard,
    Card,
    OptionalException,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_card_drawn_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalTriggerEventNode> {
    let mut state = DrawEventState::Article;
    let mut article = None;
    let mut player = None;
    let mut exception = None;
    loop {
        state = match state {
            DrawEventState::Article => {
                cursor.enter("event.card-drawn.article");
                let parsed_article = cursor
                    .peek()
                    .filter(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
                    .ok_or_else(|| cursor.fail("`a` or `an` before a player role"))?
                    .to_string();
                cursor.consume(&parsed_article)?;
                article = Some(parsed_article.to_ascii_lowercase());
                DrawEventState::Player
            }
            DrawEventState::Player => {
                cursor.enter("event.card-drawn.player");
                let token = cursor
                    .peek_token()
                    .ok_or_else(|| cursor.fail("a supported player role"))?;
                let role = lookup_player_role(&token.raw)
                    .ok_or_else(|| cursor.fail("a supported player role"))?;
                cursor.require(
                    matches!(
                        (article.as_deref(), role),
                        (Some("a"), PlayerRole::Any) | (Some("an"), PlayerRole::Opponent)
                    ),
                    "the normalized article for the player role",
                )?;
                player = Some(player_predicate(role));
                cursor.advance_to(cursor.position() + 1)?;
                DrawEventState::Action
            }
            DrawEventState::Action => {
                cursor.enter("event.card-drawn.action");
                cursor.consume("draws")?;
                DrawEventState::ArticleCard
            }
            DrawEventState::ArticleCard => {
                cursor.enter("event.card-drawn.card-article");
                cursor.consume("a")?;
                DrawEventState::Card
            }
            DrawEventState::Card => {
                cursor.enter("event.card-drawn.card");
                cursor.consume("card")?;
                DrawEventState::OptionalException
            }
            DrawEventState::OptionalException => {
                cursor.enter("event.card-drawn.optional-exception");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("except"))
                {
                    exception = Some(recognize_first_in_each_draw_step_exception(
                        cursor,
                        cursor.require_value(player, "a drawing player")?,
                    )?);
                }
                DrawEventState::Complete
            }
            DrawEventState::Complete => {
                cursor.enter("event.card-drawn.complete");
                return Ok(LocalTriggerEventNode::CardDrawn {
                    player: cursor.require_value(player, "a drawing player")?,
                    exception,
                });
            }
        };
    }
}

pub(in crate::oracle::canonical) fn recognize_source_entered_event(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<LocalTriggerEventNode> {
    cursor.enter("event.source-entered.source");
    let source = recognize_source_reference(cursor, name_links, name_failure)?;
    cursor.enter("event.source-entered.action");
    let event = cursor.consume_type("eventWord")?;
    cursor.require(
        event.raw.eq_ignore_ascii_case("enters"),
        "the `enters` event",
    )?;
    cursor.enter("event.source-entered.complete");
    Ok(LocalTriggerEventNode::SourceEntered { source })
}

/// Recognizes the event atom `SOURCE attacks`. Trigger words and the following
/// comma remain owned by the surrounding triggered-ability envelope.
pub(in crate::oracle::canonical) fn recognize_source_attacked_event(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<LocalTriggerEventNode> {
    cursor.enter("event.source-attacked.source");
    let source = recognize_source_reference(cursor, name_links, name_failure)?;
    cursor.enter("event.source-attacked.action");
    let event = cursor.consume_type("eventWord")?;
    cursor.require(
        event.raw.eq_ignore_ascii_case("attacks"),
        "the `attacks` event",
    )?;
    cursor.enter("event.source-attacked.complete");
    Ok(LocalTriggerEventNode::SourceAttacked { source })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LandPlayedEventState {
    Player,
    Action,
    Determiner,
    ObjectType,
    Complete,
}

/// Recognizes the event atom `PLAYER_REF play(s) a/another land`. The
/// surrounding triggered-ability envelope owns `when`/`whenever` and the
/// following comma. `another` is retained as a semantic source-exclusion
/// relation rather than discarded as surface vocabulary.
pub(in crate::oracle::canonical) fn recognize_land_played_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalTriggerEventNode> {
    let mut state = LandPlayedEventState::Player;
    let mut player = None;
    let mut exclude_source = false;
    loop {
        state = match state {
            LandPlayedEventState::Player => {
                cursor.enter("event.land-played.player");
                player = Some(recognize_player_reference(cursor, None)?);
                LandPlayedEventState::Action
            }
            LandPlayedEventState::Action => {
                cursor.enter("event.land-played.action");
                let action = cursor
                    .peek_token()
                    .ok_or_else(|| cursor.fail("the land-play action"))?
                    .clone();
                cursor.require(action.token_type == "actionWord", "a land-play action word")?;
                match player
                    .as_ref()
                    .ok_or_else(|| cursor.fail("a populated event player"))?
                {
                    LocalPlayerReference::Controller => cursor.require(
                        action.raw.eq_ignore_ascii_case("play"),
                        "the second-person `play` action",
                    )?,
                    LocalPlayerReference::ControllerOf(_)
                    | LocalPlayerReference::ControllerOfQualified(_)
                    | LocalPlayerReference::Bound(_) => cursor.require(
                        action.raw.eq_ignore_ascii_case("plays"),
                        "the third-person `plays` action",
                    )?,
                }
                cursor.consume(&action.raw)?;
                LandPlayedEventState::Determiner
            }
            LandPlayedEventState::Determiner => {
                cursor.enter("event.land-played.determiner");
                let determiner = cursor.consume_type("quantifierWord")?;
                if determiner.raw.eq_ignore_ascii_case("another") {
                    exclude_source = true;
                } else {
                    cursor.require(
                        determiner.raw.eq_ignore_ascii_case("a"),
                        "the `a` or `another` land determiner",
                    )?;
                }
                LandPlayedEventState::ObjectType
            }
            LandPlayedEventState::ObjectType => {
                cursor.enter("event.land-played.object-type");
                let object_type = cursor
                    .peek_token()
                    .ok_or_else(|| cursor.fail("the `land` object type"))?
                    .clone();
                cursor.require(object_type.token_type == "typeWord", "a card type")?;
                cursor.require(
                    object_type.raw.eq_ignore_ascii_case("land"),
                    "the `land` object type",
                )?;
                cursor.consume(&object_type.raw)?;
                LandPlayedEventState::Complete
            }
            LandPlayedEventState::Complete => {
                cursor.enter("event.land-played.complete");
                return Ok(LocalTriggerEventNode::LandPlayed {
                    player: cursor.require_value(player, "a populated event player")?,
                    exclude_source,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AlternativesState {
    InitialTrigger,
    InitialEvent,
    ConnectorOrSeparator,
    RepeatedTrigger,
    RepeatedEvent,
    Complete,
}

/// Recognizes Oracle's alternative event envelope
/// `When EVENT and whenever EVENT [, and whenever EVENT...],`.
/// The `and` belongs to this event-union parent, never to EffectSequence.
pub(in crate::oracle::canonical) fn recognize_entry_and_repeat_event_alternatives(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<TriggerEventAlternativesNode> {
    let mut state = AlternativesState::InitialTrigger;
    let mut events = Vec::new();
    loop {
        state = match state {
            AlternativesState::InitialTrigger => {
                cursor.enter("event-alternatives.initial-trigger");
                cursor.consume("when")?;
                AlternativesState::InitialEvent
            }
            AlternativesState::InitialEvent => {
                cursor.enter("event-alternatives.initial-event");
                events.push(recognize_source_entered_event(
                    cursor,
                    name_links,
                    name_failure,
                )?);
                AlternativesState::ConnectorOrSeparator
            }
            AlternativesState::ConnectorOrSeparator => {
                cursor.enter("event-alternatives.connector-or-separator");
                if cursor.peek().is_some_and(|raw| raw == ",") {
                    cursor.consume(",")?;
                    AlternativesState::Complete
                } else {
                    cursor.consume("and")?;
                    AlternativesState::RepeatedTrigger
                }
            }
            AlternativesState::RepeatedTrigger => {
                cursor.enter("event-alternatives.repeated-trigger");
                cursor.consume("whenever")?;
                AlternativesState::RepeatedEvent
            }
            AlternativesState::RepeatedEvent => {
                cursor.enter("event-alternatives.repeated-event");
                events.push(recognize_card_drawn_event(cursor)?);
                AlternativesState::ConnectorOrSeparator
            }
            AlternativesState::Complete => {
                cursor.enter("event-alternatives.complete");
                cursor.require(events.len() >= 2, "at least two alternative trigger events")?;
                return Ok(TriggerEventAlternativesNode { events });
            }
        };
    }
}

#[cfg(test)]
#[path = "trigger_event_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "source_attacked_event_tests.rs"]
mod source_attacked_tests;

#[cfg(test)]
#[path = "land_play_event_tests.rs"]
mod land_play_event_tests;
