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
        /// `play a land from exile` (CR 305.1, 601.2a): the land was played
        /// from exile; the land-play record keeps the zone it left.
        from_exile: bool,
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
    /// `a player|an opponent draws their Nth card each turn` (CR 121.2):
    /// the same per-turn draw ordinal, counted for a third-person drawer.
    PlayerOrdinalCardDrawn {
        player: PlayerPredicate,
        ordinal: i64,
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
        /// The permanent of `tap PERMANENT for mana`.
        permanent: Option<super::PlayerEventCardObject>,
        /// `for the first time each turn`: only the first occurrence of
        /// this event in a turn (the shared modifier tail).
        first_time_each_turn: bool,
    },
    /// `equipped|enchanted creature VERB`: the event subject is the object the
    /// ability source is attached to (CR 301.5a, 303.4a), not the source.
    AttachedObjectEvent {
        role: AttachedObjectRole,
        action: AttachedObjectEventAction,
        /// `attacks alone` (CR 506.5); only an attack reads it.
        alone: bool,
        /// The noun the event wrote for the attached object (`enchanted
        /// land becomes tapped`): the nominal a later anaphor must follow.
        nominal: super::TargetNominal,
    },
    /// `equipped|enchanted creature blocks [QUERY]` or `becomes blocked by
    /// QUERY` (CR 509.1a, 509.1h): the block of the attached object, with
    /// the other combatant when written (one occurrence per block pair).
    AttachedObjectBlock {
        role: AttachedObjectRole,
        verb: AttachedBlockVerb,
        counterpart: Option<super::ObjectQueryNode>,
    },
    /// `enchanted PERMANENT leaves the battlefield | is put into a graveyard
    /// | is put into exile [from the battlefield]` (CR 603.6c, 603.10a): the
    /// attached object's departure, observed by its attachment looking back.
    AttachedObjectDeparture {
        role: AttachedObjectRole,
        /// `None` for any zone (`leaves the battlefield`).
        destination: Option<&'static str>,
    },
    /// `enchanted creature becomes the target of STACK-OBJECT` (CR 115.1):
    /// the targeting frame of `become` with the attached object as subject.
    AttachedObjectTargeted {
        role: AttachedObjectRole,
        stack: super::TargetingStackSelectorNode,
    },
    /// `enchanted creature is dealt [combat|noncombat] damage` (CR 120.3):
    /// the passive damage clause with the attached object as recipient.
    AttachedObjectDealtDamage {
        role: AttachedObjectRole,
        damage: DamageKind,
    },
    /// `SOURCE and at least N other ATTACKERS attack`, `ACTOR attack(s) with
    /// N or more ATTACKERS`: one attack declaration (CR 508.1).
    AttackDeclaration(super::AttackDeclarationNode),
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
    /// `is tapped for mana` (CR 106.11a): a mana ability of the attached
    /// permanent with {T} in its cost was activated.
    TappedForMana,
}

/// Which combatant of a block the attached object is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AttachedBlockVerb {
    /// `blocks`: the attached object is the blocker.
    Blocks,
    /// `becomes blocked by`: the attached object is the attacker.
    BecomesBlocked,
}

fn attached_word_at(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// Whether an attached-object verb frame starts `offset` tokens after the
/// cursor (a lexical probe for the occurrence union).
fn attached_predicate_starts_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    [
        "attacks", "blocks", "becomes", "dies", "deals", "is", "leaves",
    ]
    .iter()
    .any(|word| attached_word_at(cursor, offset, word))
}

/// `equipped|enchanted NOUN` (CR 301.5a, 303.4a). The attachment fixes the
/// object; the noun restates its card type.
fn recognize_attached_object_subject(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(AttachedObjectRole, String)> {
    cursor.enter("event.attached-object.role");
    let role = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("equipped") => AttachedObjectRole::Equipped,
        Some("enchanted") => AttachedObjectRole::Enchanted,
        _ => return Err(cursor.fail("`equipped` or `enchanted`")),
    };
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("event.attached-object.noun");
    let noun = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .filter(|raw| {
            ["creature", "land", "artifact", "enchantment", "permanent"].contains(&raw.as_str())
        })
        .ok_or_else(|| cursor.fail("an attached permanent noun"))?;
    cursor.consume(&noun)?;
    Ok((role, noun))
}

/// Recognizes `equipped|enchanted NOUN VERB [or VERB]...` after the trigger
/// word: one attached subject and one or more verb frames, each an event of
/// that subject. A union selects one occurrence (`attacks or blocks`); a
/// trailing block counterpart is shared by a bare preceding `blocks`
/// (`blocks or becomes blocked by a creature`). The enclosing clause owns
/// the comma and the body.
pub(in crate::oracle::canonical) fn recognize_attached_object_occurrence(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::GenericTriggerEventNode> {
    let (role, noun) = recognize_attached_object_subject(cursor)?;
    let mut events = vec![recognize_attached_object_predicate(cursor, role, &noun)?];
    while attached_word_at(cursor, 0, "or") && attached_predicate_starts_at(cursor, 1) {
        cursor.enter("event.attached-object.union");
        cursor.consume("or")?;
        let event = recognize_attached_object_predicate(cursor, role, &noun)?;
        cursor.enter("event.attached-object.union-distinct");
        cursor.require(!events.contains(&event), "a distinct event in the union")?;
        events.push(event);
    }
    if let Some(LocalTriggerEventNode::AttachedObjectBlock {
        counterpart: Some(shared),
        ..
    }) = events.last().cloned()
    {
        let last = events.len() - 1;
        for event in &mut events[..last] {
            if let LocalTriggerEventNode::AttachedObjectBlock {
                verb: AttachedBlockVerb::Blocks,
                counterpart,
                ..
            } = event
                && counterpart.is_none()
            {
                *counterpart = Some(shared.clone());
            }
        }
    }
    cursor.enter("event.attached-object.complete");
    Ok(if events.len() == 1 {
        super::GenericTriggerEventNode::Local(events.pop().expect("one attached event"))
    } else {
        super::GenericTriggerEventNode::Compound(
            events
                .into_iter()
                .map(super::GenericTriggerEventNode::Local)
                .collect(),
        )
    })
}

/// `blocks [a|an|one or more QUERY]` / `becomes blocked by QUERY`: the other
/// combatant read by the shared object query (`one or more` is not typed for
/// an attachment and fails explicitly).
fn recognize_attached_block_counterpart(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::ObjectQueryNode> {
    cursor.enter("event.attached-object.counterpart");
    let article = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .filter(|raw| raw == "a" || raw == "an")
        .ok_or_else(|| cursor.fail("`a` or `an` before the other combatant"))?;
    cursor.consume(&article)?;
    super::recognize_object_query(cursor)
}

/// The departure frames of an attached permanent: `leaves the battlefield`,
/// `is put into a graveyard|exile [from the battlefield]`. `None` when no
/// departure verb is at the cursor (nothing consumed).
fn recognize_attached_departure(
    cursor: &mut OracleTokenCursor<'_>,
    role: AttachedObjectRole,
) -> MachineResult<Option<LocalTriggerEventNode>> {
    if cursor.peek().is_some_and(|raw| {
        super::is_verb_form(
            raw,
            super::VerbLemma::Leave,
            super::VerbForm::ThirdPersonSingular,
        )
    }) {
        cursor.enter("event.attached-object.leaves");
        let verb = cursor.peek().map(str::to_string).unwrap_or_default();
        cursor.consume(&verb)?;
        cursor.consume_phrase(&["the", "battlefield"])?;
        return Ok(Some(LocalTriggerEventNode::AttachedObjectDeparture {
            role,
            destination: None,
        }));
    }
    if !(attached_word_at(cursor, 0, "is")
        && cursor.peek_ahead(1).is_some_and(|raw| {
            super::is_verb_form(raw, super::VerbLemma::Put, super::VerbForm::PastParticiple)
        }))
    {
        return Ok(None);
    }
    cursor.enter("event.attached-object.put-into");
    cursor.consume("is")?;
    let participle = cursor.peek().map(str::to_string).unwrap_or_default();
    cursor.consume(&participle)?;
    cursor.consume("into")?;
    cursor.enter("event.attached-object.put-into.zone");
    let destination = if attached_word_at(cursor, 0, "exile") {
        cursor.consume("exile")?;
        "exile"
    } else {
        // The departing permanent goes to its owner's graveyard (CR 400.3);
        // an owner relation on it has no matcher.
        cursor.consume_phrase(&["a", "graveyard"])?;
        "graveyard"
    };
    if attached_word_at(cursor, 0, "from") {
        cursor.consume_phrase(&["from", "the", "battlefield"])?;
    }
    Ok(Some(LocalTriggerEventNode::AttachedObjectDeparture {
        role,
        destination: Some(destination),
    }))
}

/// The registered nominal of an attached permanent noun (`creature`,
/// `land`, `permanent`).
fn attached_nominal(
    cursor: &OracleTokenCursor<'_>,
    noun: &str,
) -> MachineResult<super::TargetNominal> {
    use crate::oracle::characteristic_vocabulary::{
        CharacteristicLexeme, lookup_object_selector_lexeme,
    };
    match lookup_object_selector_lexeme(noun) {
        Some(CharacteristicLexeme::CardType(value)) => Ok(super::TargetNominal::CardType(value)),
        Some(CharacteristicLexeme::ObjectDomain(value)) => Ok(super::TargetNominal::Domain(value)),
        _ => Err(cursor.fail("a registered attached permanent nominal")),
    }
}

/// One verb frame of an attached-object event.
fn recognize_attached_object_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    role: AttachedObjectRole,
    noun: &str,
) -> MachineResult<LocalTriggerEventNode> {
    let nominal = attached_nominal(cursor, noun)?;
    let event = |action: AttachedObjectEventAction| LocalTriggerEventNode::AttachedObjectEvent {
        role,
        action,
        alone: false,
        nominal,
    };
    cursor.enter("event.attached-object.action");
    // `is tapped for mana` (CR 106.11a): the passive of the `tap` lemma with
    // the mana purpose, for any attached permanent.
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("is"))
        && cursor.peek_ahead(1).is_some_and(|raw| {
            super::is_verb_form(raw, super::VerbLemma::Tap, super::VerbForm::PastParticiple)
        })
        && cursor
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("for"))
    {
        cursor.enter("event.attached-object.tapped-for-mana");
        cursor.consume("is")?;
        let participle = cursor.peek().map(str::to_string).unwrap_or_default();
        cursor.consume(&participle)?;
        cursor.consume_phrase(&["for", "mana"])?;
        return Ok(event(AttachedObjectEventAction::TappedForMana));
    }
    // Departures and death apply to any attached permanent.
    if let Some(departure) = recognize_attached_departure(cursor, role)? {
        return Ok(departure);
    }
    if attached_word_at(cursor, 0, "dies") {
        cursor.consume("dies")?;
        return Ok(event(AttachedObjectEventAction::Die));
    }
    // Only a creature takes part in combat, so other nouns admit tapping.
    if noun != "creature" {
        cursor.enter("event.attached-object.action");
        cursor.consume_phrase(&["becomes", "tapped"])?;
        return Ok(event(AttachedObjectEventAction::BecomeTapped));
    }
    // The targeting frame of `become` (CR 115.1) with the attached object as
    // the targeted subject; the stack object is the shared selector.
    if cursor.peek().is_some_and(|raw| {
        super::is_verb_form(
            raw,
            super::VerbLemma::Become,
            super::VerbForm::ThirdPersonSingular,
        )
    }) && super::targeting_frame_starts(cursor, 1)
    {
        cursor.enter("event.attached-object.became-target");
        cursor.consume("becomes")?;
        super::consume_targeting_relation(cursor)?;
        let stack = super::recognize_targeting_stack_selector(cursor)?;
        return Ok(LocalTriggerEventNode::AttachedObjectTargeted { role, stack });
    }
    // `is dealt [combat|noncombat] damage`: the shared passive damage
    // clause with the attached object as recipient.
    let passive = super::supplied_damage_frame(
        super::VerbAgreement::ThirdPersonSingular,
        super::Voice::Passive,
        super::ClauseModality::Indicative,
        super::DamageArguments::PassiveKind,
    );
    if let Some(arguments) = super::recognize_supplied_damage_clause(cursor, &passive)? {
        let super::DamageClauseArguments::Kind(damage) = arguments else {
            return Err(cursor.fail("the kind of damage the attached object is dealt"));
        };
        return Ok(LocalTriggerEventNode::AttachedObjectDealtDamage { role, damage });
    }
    cursor.enter("event.attached-object.action");
    if let Some(complement) = recognize_dealt_event_complement(cursor)? {
        if !complement.is_combat_damage_to_any_player() {
            return Ok(LocalTriggerEventNode::DamageDealt {
                subject: DamageDealer::Attached(role),
                complement,
            });
        }
        return Ok(event(AttachedObjectEventAction::DealCombatDamageToPlayer));
    }
    if attached_word_at(cursor, 0, "attacks") {
        cursor.consume("attacks")?;
        return Ok(LocalTriggerEventNode::AttachedObjectEvent {
            role,
            action: AttachedObjectEventAction::Attack,
            alone: super::consume_alone_modifier(cursor)?,
            nominal,
        });
    }
    if attached_word_at(cursor, 0, "blocks") {
        cursor.consume("blocks")?;
        let counterpart = if attached_word_at(cursor, 0, "a") || attached_word_at(cursor, 0, "an") {
            Some(recognize_attached_block_counterpart(cursor)?)
        } else {
            None
        };
        return Ok(LocalTriggerEventNode::AttachedObjectBlock {
            role,
            verb: AttachedBlockVerb::Blocks,
            counterpart,
        });
    }
    if attached_word_at(cursor, 0, "becomes") && attached_word_at(cursor, 1, "tapped") {
        cursor.consume_phrase(&["becomes", "tapped"])?;
        return Ok(event(AttachedObjectEventAction::BecomeTapped));
    }
    cursor.consume_phrase(&["becomes", "blocked"])?;
    if attached_word_at(cursor, 0, "by") {
        cursor.consume("by")?;
        return Ok(LocalTriggerEventNode::AttachedObjectBlock {
            role,
            verb: AttachedBlockVerb::BecomesBlocked,
            counterpart: Some(recognize_attached_block_counterpart(cursor)?),
        });
    }
    Ok(event(AttachedObjectEventAction::BecomeBlocked))
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

/// The drawing player of an ordinal draw: `you` (second person, `draw
/// your`), or `a player` / `an opponent` (third person, `draws their`).
/// Reads the article and role through the shared player-role vocabulary.
fn recognize_ordinal_drawer(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<PlayerPredicate>> {
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
    {
        cursor.consume("you")?;
        return Ok(None);
    }
    let article = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .filter(|raw| raw == "a" || raw == "an")
        .ok_or_else(|| cursor.fail("`you`, `a player` or `an opponent`"))?;
    cursor.consume(&article)?;
    let token = cursor
        .peek_token()
        .ok_or_else(|| cursor.fail("a supported player role"))?;
    let role =
        lookup_player_role(&token.raw).ok_or_else(|| cursor.fail("a supported player role"))?;
    cursor.require(
        matches!(
            (article.as_str(), role),
            ("a", PlayerRole::Any) | ("an", PlayerRole::Opponent)
        ),
        "the normalized article for the player role",
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    Ok(Some(player_predicate(role)))
}

/// Recognizes `you draw your Nth card in a turn|each turn`, or the third
/// person `a player|an opponent draws their Nth card ...`, as one reusable
/// event atom (CR 121.2: the ordinal counts the draws in the current turn).
/// Trigger words and the trailing comma remain parent-owned.
pub(in crate::oracle::canonical) fn recognize_ordinal_card_drawn_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalTriggerEventNode> {
    let mut state = OrdinalDrawState::Player;
    let mut ordinal = None;
    // `None`: the second-person `you`.
    let mut drawer = None;
    loop {
        state = match state {
            OrdinalDrawState::Player => {
                cursor.enter("event.ordinal-card-drawn.player");
                drawer = recognize_ordinal_drawer(cursor)?;
                OrdinalDrawState::Action
            }
            OrdinalDrawState::Action => {
                cursor.enter("event.ordinal-card-drawn.action");
                cursor.consume(if drawer.is_some() { "draws" } else { "draw" })?;
                OrdinalDrawState::Possessive
            }
            OrdinalDrawState::Possessive => {
                cursor.enter("event.ordinal-card-drawn.possessive");
                cursor.consume(if drawer.is_some() { "their" } else { "your" })?;
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
                if let Some(player) = drawer {
                    return Ok(LocalTriggerEventNode::PlayerOrdinalCardDrawn {
                        player,
                        ordinal: cursor.require_value(ordinal, "a draw ordinal")?,
                    });
                }
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
                // `from ZONE` (CR 305.1): the zone the land was played from,
                // a slot of the same event. Only exile has a matcher; another
                // zone fails here, never dropped.
                let from_exile = cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("from"));
                if from_exile {
                    cursor.enter("event.land-played.origin");
                    cursor.consume_phrase(&["from", "exile"])?;
                }
                cursor.enter("event.land-played.complete");
                return Ok(LocalTriggerEventNode::LandPlayed {
                    player: cursor.require_value(player, "a populated event player")?,
                    exclude_source,
                    from_exile,
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
