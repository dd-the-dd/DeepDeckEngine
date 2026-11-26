//! The one condition language (CR 603.4, 608.2c, 611.3a, 118.12).
//!
//! Every condition parent (`as long as`, a sentence `If`, an intervening
//! `if`, `unless`, and the `if ..., ` envelopes of family parents) reads its
//! Boolean program through the same atom reader
//! (`recognize_condition_atom`), subject first. Atoms belong to a small set
//! of families:
//!
//! ```text
//! Compare(Num, op, Num, player subject)   counts, life, X, power, ...
//! ObjectSatisfies(BindingRef, Pred)       the source, a target, a bound card
//!   ObjectZone(BindingRef, zones)         `is still on the battlefield`
//!   ObjectControl(BindingRef)             `you controlled that permanent`
//! EventHistory(Pattern, scope)            this-turn / last-turn event counts
//! CastFact                                kicked, cast, cast from a zone
//! ActionOutcome                           `if you can't`, `if you don't`
//! TurnOwnership                           `it's [not] your|their turn`
//! PlayerDesignation(holder)               enduring story, completed dungeon,
//!                                         the monarch, the starting player
//! ```
//!
//! A parent declares which families its evaluation moment can resolve
//! (`ConditionScope`); an atom outside that allow-list is an explicit
//! failure, never an approximation.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    BooleanConditionNode, ChoiceBinding, ConditionContext, ConditionObjectReference,
    ConditionPlayerSubject, ControlledCountDirection, ControlledPermanentCountConditionNode,
    CountThreshold, LocalObjectReference, LocalPlayerReference, NumericAtom,
    NumericComparisonOperator, NumericExpression, ObjectKind, ObjectPredicate, PlayerStateMeasure,
    ThresholdOperator, recognize_card_type_union_selector, recognize_condition_atom,
    recognize_numeric_atom, recognize_object_query,
};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

/// A Boolean program over the one atom grammar.
pub(in crate::oracle::canonical) type ConditionNode = BooleanConditionNode<ConditionAtom>;

/// The families of the condition language. A parent's scope is an
/// allow-list over these families.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ConditionFamily {
    Compare,
    ObjectSatisfies,
    EventHistory,
    CastFact,
    ActionOutcome,
    TurnOwnership,
    PlayerDesignation,
}

/// The evaluation moment of a condition parent, which decides the atom
/// families it can resolve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ConditionScope {
    /// `as long as`, activation and entry restrictions (CR 611.3a): a state
    /// evaluated continuously, without a cast record or a previous action.
    Static,
    /// A sentence `If` of an effect program (CR 608.2c): every family.
    Program,
    /// The intervening `if` of a triggered ability (CR 603.4): no earlier
    /// action of the program exists yet.
    TriggerGuard,
    /// `unless CONDITION`: a state condition (a payment is another branch).
    Unless,
    /// A family parent's `if ..., ` envelope that composes exactly one family.
    Only(ConditionFamily),
}

impl ConditionScope {
    /// Whether the scope resolves `atom`: its family, or a cast record the
    /// source permanent keeps (the mana spent to cast it, CR 601.2h), which
    /// a continuously evaluated or `unless` condition can still read.
    pub(in crate::oracle::canonical) fn allows_atom(self, atom: &ConditionAtom) -> bool {
        // The objects an earlier action affected exist only in a program.
        if atom.reads_action_result() {
            return self == Self::Program;
        }
        // The resolving ability's own record exists only while it resolves.
        if atom.reads_ability_history() {
            return self == Self::Program;
        }
        // A spell's turn ordinal is read on the spell a cast event observed;
        // only a parent that binds that event can resolve it.
        if matches!(atom, ConditionAtom::SpellCastOrdinal(_)) {
            return matches!(self, Self::Program | Self::TriggerGuard);
        }
        self.allows(atom.family())
            || (matches!(self, Self::Static | Self::Unless) && atom.is_source_cast_record())
    }

    pub(in crate::oracle::canonical) fn allows(self, family: ConditionFamily) -> bool {
        match self {
            Self::Program => true,
            Self::TriggerGuard => family != ConditionFamily::ActionOutcome,
            Self::Static | Self::Unless => !matches!(
                family,
                ConditionFamily::ActionOutcome | ConditionFamily::CastFact
            ),
            Self::Only(only) => family == only,
        }
    }

    pub(in crate::oracle::canonical) fn refusal(self) -> &'static str {
        match self {
            Self::Program => "a condition of the effect program",
            Self::TriggerGuard => {
                "an intervening-if condition (no earlier action of the program exists yet)"
            }
            Self::Static => "a continuously evaluated state condition",
            Self::Unless => "an `unless` state condition",
            Self::Only(_) => "the one condition family this parent composes",
        }
    }
}

/// One atom of the condition language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ConditionAtom {
    /// `Compare(Num, op, Num)` read for one player subject.
    Compare(ConditionComparison),
    /// `ObjectSatisfies(BindingRef, Pred)`: a referenced object tested with
    /// the shared object predicate.
    ObjectSatisfies(ReferencedObjectConditionNode),
    /// `EventHistory(Pattern, scope)` compared with a count.
    EventHistory(EventHistoryNode),
    /// A fact recorded when the source spell was cast.
    CastFact(CastFactNode),
    /// The outcome of an earlier action or optional payment of the program.
    ActionOutcome(ActionOutcomeNode),
    /// `it's [not] your turn`.
    TurnOwnership(ControllerTurnPropositionNode),
    /// `it's [not] their turn`, `it's an opponent's turn`: the turn of a
    /// player other than the controller (CR 102.1, 500.1).
    PlayerTurn(PlayerTurnNode),
    /// A designation a player holds (CR 309.7, enduring story, CR 724 the
    /// monarch, CR 103.1 the starting player).
    PlayerDesignation(PlayerDesignationNode),
    /// The zone a referenced object is in (`this creature is on the
    /// battlefield`, CR 400.1): typed zones, never an object predicate.
    ObjectZone(ObjectZoneConditionNode),
    /// `you control|controlled it`, `you didn't control that permanent`:
    /// the referenced object's controller is the ability's controller (CR
    /// 108.4), read as last known information once it left (CR 608.2h).
    ObjectControl(ObjectControlConditionNode),
    /// A choice the caster made for the ability's own source as it was cast
    /// (`the gift was promised`, `its surge cost was paid`, `this spell was
    /// cast using teamwork`), kept by the permanent it became.
    CastChoice(CastChoiceNode),
    /// `it's the second creature spell you cast this turn`: the position of
    /// the spell a cast event observed among its caster's spells of a query
    /// this turn (CR 601.2i), fixed when it was cast.
    SpellCastOrdinal(SpellCastOrdinalNode),
    /// `the top card of your library is a Goblin card` (CR 401.1, 404.1):
    /// the shared card predicate over the top card of one player's ordered
    /// zone. An empty zone has no top card, so the atom is false.
    TopCard(TopCardConditionNode),
    /// `red is the most common color among all permanents or is tied for
    /// most common`: a color's rank among the colors of a permanent query
    /// (CR 105.1), compared with every other color.
    MostCommonColor(MostCommonColorNode),
}

impl ConditionAtom {
    pub(in crate::oracle::canonical) fn family(&self) -> ConditionFamily {
        match self {
            Self::Compare(_) => ConditionFamily::Compare,
            Self::ObjectSatisfies(_) | Self::ObjectZone(_) | Self::ObjectControl(_) => {
                ConditionFamily::ObjectSatisfies
            }
            Self::EventHistory(_) | Self::SpellCastOrdinal(_) => ConditionFamily::EventHistory,
            Self::CastFact(_) | Self::CastChoice(_) => ConditionFamily::CastFact,
            Self::TopCard(_) => ConditionFamily::ObjectSatisfies,
            Self::MostCommonColor(_) => ConditionFamily::Compare,
            Self::ActionOutcome(_) => ConditionFamily::ActionOutcome,
            Self::TurnOwnership(_) | Self::PlayerTurn(_) => ConditionFamily::TurnOwnership,
            Self::PlayerDesignation(_) => ConditionFamily::PlayerDesignation,
        }
    }

    /// Kicker status is chosen as the spell is cast (CR 702.33d); targets in
    /// a branch it governs are required only if the spell was kicked.
    pub(in crate::oracle::canonical) fn depends_on_cast_choice(&self) -> bool {
        // A promised gift, a paid teamwork or alternative cost is chosen as
        // the spell is cast (CR 601.2b-f) like kicker.
        matches!(
            self,
            Self::CastFact(CastFactNode {
                fact: CastFact::Kicked,
                ..
            }) | Self::CastChoice(_)
        )
    }

    /// Whether the atom counts the controller's permanents other than the
    /// ability's own source (`another`, `other`).
    pub(in crate::oracle::canonical) fn excludes_source(&self) -> bool {
        let excludes = |number: &ConditionNumber| {
            matches!(
                number,
                ConditionNumber::Player(PlayerStateMeasure::ControlledPermanents {
                    exclude_source: true,
                    ..
                })
            )
        };
        match self {
            Self::Compare(comparison) => excludes(&comparison.left) || excludes(&comparison.right),
            _ => false,
        }
    }

    /// A cast fact the source keeps after it resolved: the mana spent to
    /// cast the ability's own source.
    pub(in crate::oracle::canonical) fn is_source_cast_record(&self) -> bool {
        // The permanent a spell became keeps whether it was cast, whether
        // from a hand, the mana spent and the choices made as it was cast
        // (CR 601.2a, 601.2h): an entry replacement (`enters with a counter
        // if you cast it from your hand`) and a continuous condition read
        // that record of the source itself.
        matches!(
            self,
            Self::CastFact(CastFactNode {
                spell: CastSpellReference::Source(_),
                fact: CastFact::ManaSpent { .. }
                    | CastFact::Cast {
                        zone: None | Some(SourceCastZone::Hand),
                        ..
                    },
            }) | Self::CastChoice(_)
        )
    }

    /// Whether the atom reads the result of the instruction before it.
    pub(in crate::oracle::canonical) fn reads_previous_action(&self) -> bool {
        matches!(
            self,
            Self::ActionOutcome(ActionOutcomeNode::PreviousActionImpossible)
        )
    }

    /// Whether the atom reads the resolving ability's own record (its
    /// resolutions or activations this turn).
    pub(in crate::oracle::canonical) fn reads_ability_history(&self) -> bool {
        matches!(
            self,
            Self::Compare(ConditionComparison { left, right, .. })
                if [left, right].into_iter().any(|number| matches!(
                    number,
                    ConditionNumber::Expression(expression)
                        if super::reads_ability_history(expression)
                ))
        )
    }

    /// Whether the atom counts the objects an earlier action affected
    /// (`a land card was milled this way`), which only the program lowering
    /// binds.
    pub(in crate::oracle::canonical) fn reads_action_result(&self) -> bool {
        matches!(
            self,
            Self::Compare(ConditionComparison { left, right, .. })
                if matches!(left, ConditionNumber::ActionResultObjects(_))
                    || matches!(right, ConditionNumber::ActionResultObjects(_))
        )
    }
}

/// Whether any atom of the program satisfies `test`.
pub(in crate::oracle::canonical) fn condition_any(
    condition: &ConditionNode,
    test: &impl Fn(&ConditionAtom) -> bool,
) -> bool {
    match condition {
        BooleanConditionNode::Atom(atom) => test(atom),
        BooleanConditionNode::And(operands) | BooleanConditionNode::Or(operands) => {
            operands.iter().any(|operand| condition_any(operand, test))
        }
        BooleanConditionNode::Not(operand) => condition_any(operand, test),
    }
}

/// `Compare(left, operator, right)` where every player measure is read for
/// `subject`: the controller, or each member of a quantified population.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ConditionComparison {
    pub subject: ConditionPlayerSubject,
    pub left: ConditionNumber,
    pub operator: NumericComparisonOperator,
    pub right: ConditionNumber,
}

impl ConditionComparison {
    /// `subject` has `measure` within `threshold`.
    pub(in crate::oracle::canonical) fn measured(
        subject: ConditionPlayerSubject,
        measure: PlayerStateMeasure,
        threshold: CountThreshold,
    ) -> Self {
        let (operator, value) = threshold_comparison(threshold);
        Self {
            subject,
            left: ConditionNumber::Player(measure),
            operator,
            right: ConditionNumber::Integer(value),
        }
    }
}

/// The comparison a fixed threshold states.
pub(in crate::oracle::canonical) fn threshold_comparison(
    threshold: CountThreshold,
) -> (NumericComparisonOperator, i64) {
    match threshold.operator {
        ThresholdOperator::AtLeast => (
            NumericComparisonOperator::GreaterThanOrEqual,
            threshold.value,
        ),
        ThresholdOperator::AtMost => (NumericComparisonOperator::LessThanOrEqual, threshold.value),
        ThresholdOperator::Exactly => (NumericComparisonOperator::Equal, threshold.value),
    }
}

/// One operand of a comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ConditionNumber {
    Integer(i64),
    /// The shared numeric expression grammar (`X`, `that creature's power`).
    Expression(NumericExpression),
    /// A measure of the comparison's player subject.
    Player(PlayerStateMeasure),
    /// The greatest value of a measure among the opponents of the subject.
    GreatestOpponent(PlayerStateMeasure),
    /// The least value of a measure among the opponents of the subject.
    LeastOpponent(PlayerStateMeasure),
    /// A measure of the ability's controller whatever the comparison's
    /// subject (`that player controls more lands than you`).
    ControllerMeasure(PlayerStateMeasure),
    /// The objects the action before the condition affected, restated by
    /// verb and noun (`a land card was milled this way`, CR 608.2c). Only a
    /// program position binds it; elsewhere it is an explicit failure.
    ActionResultObjects(super::ActionResultReferenceNode),
}

/// The object an object condition tests, with its predicate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ReferencedObjectConditionNode {
    pub object: ConditionObjectReference,
    pub predicate: ObjectPredicate,
}

/// The typed event of a history pattern. Each kind is one recorded game
/// event; the pattern's fields say whose and which object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum HistoryEvent {
    /// CR 601.2i: a spell became cast.
    SpellCast,
    /// CR 121.1.
    CardDrawn,
    /// CR 119.3.
    LifeGained,
    LifeLost,
    /// CR 120.3: damage dealt to the player (the event's player).
    DamageDealtToPlayer,
    /// CR 508.1a: a creature was declared as an attacker.
    AttackerDeclared,
    /// CR 700.4.
    PermanentDied,
    /// CR 603.6c.
    PermanentLeftBattlefield,
    /// CR 603.6a.
    PermanentEntered,
    /// CR 701.23.
    LibrarySearched,
    /// CR 119.3: a life gain or a life loss (`gained or lost life`).
    LifeChanged,
    /// CR 701.21: the sacrifice record keeps the permanent's type line.
    PermanentSacrificed,
    /// CR 701.9.
    CardDiscarded,
    /// CR 702.29: cycling a card.
    CardCycled,
    /// CR 111.1: the creation record keeps the token's type line.
    TokenCreated,
    /// CR 122.1: counters put on a permanent; the event's player is the
    /// permanent's controller, the actor a related player.
    CountersPlaced,
    /// CR 305.1.
    LandPlayed,
    /// Cards put into a graveyard from anywhere (one arrival record per
    /// move, with each card's type line). Descending reads this record.
    CardsPutIntoGraveyard,
    /// CR 506.1: a combat phase began (`the first combat phase of the turn`
    /// is the turn's first such record).
    CombatPhaseStarted,
    /// CR 700.13: a player committed a crime.
    CrimeCommitted,
    /// A card left a graveyard (CR 400.7): one record per card, whose
    /// player is the graveyard's owner and whose object is the card.
    CardLeftGraveyard,
}

/// The casting mode a cast record keeps (`a spell was warped`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum HistoryCastMode {
    Warp,
}

/// The zone a departing permanent was put into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum HistoryDestination {
    Hand,
}

/// Qualifiers of a history pattern beyond its player and object: each one
/// is an exact recorded field of the event, never inferred.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct HistoryQualifiers {
    /// `another`: the event object is not the ability's source.
    pub exclude_source: bool,
    /// The counter kind of a counter placement.
    pub counter: Option<crate::oracle::counter_name::CounterName>,
    /// The control relation of the permanent that received counters (the
    /// pattern's player is then the player who put them).
    pub recipient_controller: Option<HistoryPlayer>,
    /// The casting mode of a cast record.
    pub cast_mode: Option<HistoryCastMode>,
    /// The pattern's player reads a departing card's owner (`put into your
    /// graveyard`, CR 404.2), not the permanent's controller.
    pub zone_owner: bool,
    /// The recorded destination of a departure.
    pub destination: Option<HistoryDestination>,
    /// The zone a recorded spell was cast from (`cast a spell from your
    /// hand`, CR 601.2a).
    pub cast_zone: Option<SourceCastZone>,
    /// Whether recorded damage was combat damage (`dealt noncombat damage`,
    /// CR 510.2 combat damage).
    pub combat_damage: Option<bool>,
}

/// The player relation of the event's player (the actor, the recipient of
/// damage, or the controller of a permanent that moved).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum HistoryPlayer {
    Any,
    Controller,
    Opponent,
}

/// What a history pattern counts per matching event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum HistoryMeasure {
    /// One per event.
    Occurrences,
    /// The event's amount (life, damage).
    Amount,
    /// The cards of one arrival record that satisfy the object predicate.
    Cards,
    /// The power an attacker had as it was declared (`attacked with
    /// creatures with total power 6 or greater`, CR 508.1).
    Power,
}

/// The turn a history pattern reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum HistoryScope {
    ThisTurn,
    /// The turn immediately before the current one (`last turn`).
    LastTurn,
    /// The current combat phase (`this combat`, CR 506.1): the records since
    /// the latest combat phase of this turn began.
    ThisCombat,
}

/// How the per-event measure is aggregated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum HistoryAggregate {
    /// Summed over every matching event.
    Total,
    /// Summed per player, then the greatest per-player sum (`a player cast
    /// two or more spells`: one player did).
    GreatestPerPlayer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EventHistoryPattern {
    pub event: HistoryEvent,
    pub player: HistoryPlayer,
    /// The event object's predicate; `None` for every object of the event.
    pub object: Option<ObjectPredicate>,
    pub measure: HistoryMeasure,
    pub qualifiers: HistoryQualifiers,
}

/// `EventHistory(Pattern, scope)` compared with a fixed count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EventHistoryNode {
    pub pattern: EventHistoryPattern,
    pub scope: HistoryScope,
    pub aggregate: HistoryAggregate,
    pub operator: NumericComparisonOperator,
    pub threshold: i64,
}

/// `EventHistory(Pattern, scope)` counted instead of compared: a numeric
/// expression (`for each spell you've cast this turn`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EventHistoryCountNode {
    pub pattern: EventHistoryPattern,
    pub scope: HistoryScope,
    pub aggregate: HistoryAggregate,
}

/// The zone a spell was cast from (CR 601.2a).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SourceCastZone {
    Hand,
    Graveyard,
    Exile,
}

/// How much mana was spent to cast a spell (CR 601.2h).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ManaSpentAmount {
    /// Each listed mana was spent at least its count of times (`{R}{R}`,
    /// `{R}{G}`, `at least three red mana`), in `ManaSpentAmount::SYMBOLS`
    /// order.
    Symbols([u8; 6]),
    /// The total mana spent compared with a fixed value (`five or more
    /// mana`, `no mana`).
    Total {
        operator: NumericComparisonOperator,
        value: i64,
    },
}

impl ManaSpentAmount {
    /// The mana a cast record distinguishes: the five colors and colorless.
    pub(in crate::oracle::canonical) const SYMBOLS: [&'static str; 6] =
        ["W", "U", "B", "R", "G", "C"];
}

/// The spell a cast fact is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CastSpellReference {
    /// The ability's own source (`this spell`, `it` denoting the source).
    Source(LocalObjectReference),
    /// The spell a cast event observed (`that spell`, `it` after `Whenever
    /// you cast a spell`).
    EventSpell,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CastFact {
    /// CR 702.33d.
    Kicked,
    /// CR 601.2h: the mana spent to cast the spell; `negated` reads
    /// `wasn't spent`.
    ManaSpent {
        amount: ManaSpentAmount,
        negated: bool,
    },
    /// The source was cast; from `zone` when present. `negated` reads
    /// `anywhere other than ZONE`.
    Cast {
        zone: Option<SourceCastZone>,
        negated: bool,
    },
    /// `you cast this spell during your main phase` (Addendum): the spell
    /// was cast while its caster was the active player in a main phase.
    CastDuringMainPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CastFactNode {
    pub spell: CastSpellReference,
    pub fact: CastFact,
}

/// The casting mode of an alternative cost a casting keyword grants (CR
/// 118.9): the runtime records the mode it was cast with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AlternativeCastMode {
    /// CR 702.117a.
    Surge,
    /// CR 702.137a.
    Spectacle,
    /// CR 702.119a.
    Emerge,
    /// Sneak.
    Sneak,
    /// CR 702.188a.
    WebSlinging,
}

impl AlternativeCastMode {
    /// The keyword word (`surge`, `web-slinging`) that names the mode.
    pub(in crate::oracle::canonical) fn from_word(raw: &str) -> Option<Self> {
        let raw = raw.to_ascii_lowercase();
        Some(match raw.as_str() {
            "surge" => Self::Surge,
            "spectacle" => Self::Spectacle,
            "emerge" => Self::Emerge,
            "sneak" => Self::Sneak,
            "web-slinging" => Self::WebSlinging,
            _ => return None,
        })
    }

    /// The `alternativeCostMode` the casting keyword lowering declares.
    pub(in crate::oracle::canonical) const fn ir_name(self) -> &'static str {
        match self {
            Self::Surge => "surge",
            Self::Spectacle => "spectacle",
            Self::Emerge => "emerge",
            Self::Sneak => "sneak",
            Self::WebSlinging => "webSlinging",
        }
    }
}

/// One choice made for a spell as it was cast (CR 601.2b-f).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CastChoice {
    /// CR 702.174a: an opponent was promised the gift.
    GiftPromised,
    /// CR 702.194: the teamwork cost was paid.
    Teamwork,
    /// The alternative cost of a casting keyword was paid.
    AlternativeMode(AlternativeCastMode),
    /// A printed alternative mana cost was paid (`the {1}{B} cost was
    /// paid`): the exact symbols the shared mana-cost reader preserves.
    AlternativeManaCost(String),
    /// CR 702.138c: the spell was cast with its escape permission, and the
    /// permanent it became escaped.
    Escaped,
}

/// `CastChoice(choice)` of the ability's own source; `negated` reads
/// `wasn't promised`, `wasn't paid`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CastChoiceNode {
    pub choice: CastChoice,
    pub negated: bool,
}

/// The spell's position among the spells of one query its caster cast this
/// turn (`the second creature spell you cast this turn`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SpellCastOrdinalNode {
    pub spell: CastSpellReference,
    pub ordinal: i64,
    /// The spell query; `None` counts every spell.
    pub predicate: Option<ObjectPredicate>,
}

/// An ordered zone whose top card a condition reads (CR 401.1, 404.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TopCardZone {
    Library,
    Graveyard,
}

impl TopCardZone {
    pub(in crate::oracle::canonical) const fn ir_name(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::Graveyard => "graveyard",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TopCardConditionNode {
    /// The player whose zone it is.
    pub player: ConditionPlayerSubject,
    pub zone: TopCardZone,
    /// The shared card predicate the top card satisfies.
    pub predicate: ObjectPredicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct MostCommonColorNode {
    pub color: super::CardColor,
    /// The permanents whose colors are counted.
    pub predicate: ObjectPredicate,
    /// `or is tied for most common`: a tie with another color still holds.
    pub tie_allowed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActionOutcomeNode {
    /// `If you can't` (CR 609.3): the instruction just before the condition
    /// could not be performed.
    PreviousActionImpossible,
    /// `if you don't`: the optional payment the parent bound was not paid.
    CostNotPaid { payment: ChoiceBinding },
    /// `If you do` / `If you don't` (CR 608.2c) after an optional
    /// instruction of the program (`you may reveal it`): whether it was
    /// actually performed. Only the program lowering resolves it; a guard
    /// that kept the instruction from being offered makes `you do` false.
    OptionalInstruction { performed: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerDesignation {
    EnduringStory,
    /// CR 702.131c: the city's blessing.
    CityBlessing,
    /// CR 309.7: `dungeon` names one Dungeon, or any Dungeon when absent.
    CompletedDungeon {
        dungeon: Option<String>,
    },
    /// CR 724.1: `the monarch`, a designation one player at most holds.
    Monarch,
    /// CR 103.1: the player who took the game's first turn. It is a fact
    /// of the game, so its present and past forms state the same thing.
    StartingPlayer,
}

impl PlayerDesignation {
    /// Whether the designation is recorded for one player of the game
    /// (any player can be asked), rather than for the controller only.
    pub(in crate::oracle::canonical) const fn is_per_player(&self) -> bool {
        matches!(self, Self::Monarch | Self::StartingPlayer)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerDesignationNode {
    pub designation: PlayerDesignation,
    /// False for `you haven't completed ...`.
    pub held: bool,
    /// The player asked: the controller, a quantified population or the
    /// player the triggering event records.
    pub subject: ConditionPlayerSubject,
}

/// The turn of a player other than the controller (`it's not their turn`,
/// `it's an opponent's turn`). `is_turn` is false for the negated form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerTurnNode {
    pub player: ConditionPlayerSubject,
    pub is_turn: bool,
}

/// A zone of the game (CR 400.1) as an object condition names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ConditionZone {
    Battlefield,
    Graveyard,
    Exile,
    Hand,
    Library,
    Stack,
    Command,
}

impl ConditionZone {
    /// The zone a prepositional phrase names at `offset` (`on the
    /// battlefield`, `in your graveyard`, `in exile`, `in the command
    /// zone`), with its width. A personal zone is the referenced card's own
    /// (its owner's, CR 400.3): only `your` and `a` name it.
    pub(in crate::oracle::canonical) fn at(
        cursor: &OracleTokenCursor<'_>,
        offset: usize,
    ) -> Option<(Self, usize)> {
        let word = |at: usize, expected: &str| {
            cursor
                .peek_ahead(offset + at)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
        };
        if word(0, "on") && word(1, "the") && word(2, "battlefield") {
            return Some((Self::Battlefield, 3));
        }
        if word(0, "on") && word(1, "the") && word(2, "stack") {
            return Some((Self::Stack, 3));
        }
        if !word(0, "in") {
            return None;
        }
        if word(1, "exile") {
            return Some((Self::Exile, 2));
        }
        if word(1, "the") && word(2, "command") && word(3, "zone") {
            return Some((Self::Command, 4));
        }
        if word(1, "your") || word(1, "a") {
            let zone = if word(2, "graveyard") {
                Self::Graveyard
            } else if word(2, "hand") {
                Self::Hand
            } else if word(2, "library") {
                Self::Library
            } else {
                return None;
            };
            return Some((zone, 3));
        }
        None
    }

    pub(in crate::oracle::canonical) const fn ir_name(self) -> &'static str {
        match self {
            Self::Battlefield => "battlefield",
            Self::Graveyard => "graveyard",
            Self::Exile => "exile",
            Self::Hand => "hand",
            Self::Library => "library",
            Self::Stack => "stack",
            Self::Command => "commandZone",
        }
    }
}

/// `OBJECT is [not] [still] in ZONE [or ZONE]`: the referenced object is in
/// one of the zones (CR 400.1). The object is resolved like every object
/// condition subject; the zones are a typed disjunction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ObjectZoneConditionNode {
    pub object: ConditionObjectReference,
    pub zones: Vec<ConditionZone>,
    pub negated: bool,
}

/// `you [didn't] control[led] OBJECT`: whether the ability's controller is
/// the referenced object's controller (its last known controller once it
/// left the battlefield).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ObjectControlConditionNode {
    pub object: ConditionObjectReference,
    pub negated: bool,
}

/// Typed turn-ownership proposition `it's [not] your turn`. The enclosing
/// `if` / `unless` introducer owns polarity; this child owns only the clause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ControllerTurnPropositionNode {
    /// True when the proposition asserts that it is the controller's turn.
    pub is_controller_turn: bool,
}

impl ControlledPermanentCountConditionNode {
    /// The count query as a comparison atom of the controller.
    pub(in crate::oracle::canonical) fn into_atom(self) -> ConditionAtom {
        debug_assert_eq!(self.player, LocalPlayerReference::Controller);
        ConditionAtom::Compare(ConditionComparison {
            subject: ConditionPlayerSubject::Controller,
            left: ConditionNumber::Player(PlayerStateMeasure::ControlledPermanents {
                predicate: self.predicate,
                exclude_source: self.exclude_source,
            }),
            operator: match self.direction {
                ControlledCountDirection::OrMore => NumericComparisonOperator::GreaterThanOrEqual,
                ControlledCountDirection::OrFewer => NumericComparisonOperator::LessThanOrEqual,
            },
            right: ConditionNumber::Integer(self.threshold),
        })
    }
}

/// Width of the existential introducer `there is|are` / `there's`.
fn existential_introducer_width(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let contracted = cursor
        .peek()
        .is_some_and(|raw| raw.replace('\u{2019}', "'").eq_ignore_ascii_case("there's"));
    if contracted {
        Some(1)
    } else if word(0, "there") && (word(1, "is") || word(1, "are")) {
        Some(2)
    } else {
        None
    }
}

/// Lexical lookahead for `there are THRESHOLD MEASURE among COLLECTION`: the
/// threshold is at most four words (`at least four`, `four or more`) and is
/// followed by a distinct-value measure. A plain object count keeps its own
/// existential reader.
pub(in crate::oracle::canonical) fn distinct_measure_atom_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let Some(width) = existential_introducer_width(cursor) else {
        return false;
    };
    (width + 1..width + 5).any(|offset| {
        let mut trial = cursor.clone();
        trial.advance_to(trial.position() + offset).is_ok()
            && super::distinct_measure_among_starts(&trial)
    })
}

/// `there are N or more card types among cards in your graveyard` (CR
/// 205.2a, Delirium), `there are five colors among permanents you control`:
/// `Compare(CountDistinct(measure, collection), op, N)` over the shared
/// counting child, read by every condition parent through the one atom
/// reader. The collection names its own player relations.
pub(in crate::oracle::canonical) fn recognize_distinct_measure_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.distinct-measure.existential");
    let width = existential_introducer_width(cursor)
        .ok_or_else(|| cursor.fail("`there is`, `there are` or `there's`"))?;
    for _ in 0..width {
        let raw = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&raw)?;
    }
    cursor.enter("condition.distinct-measure.threshold");
    let threshold = super::recognize_count_threshold(cursor, false)?;
    cursor.enter("condition.distinct-measure.measure");
    let measure = super::recognize_distinct_measure_among(cursor)?;
    let (operator, value) = threshold_comparison(threshold);
    Ok(ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left: ConditionNumber::Expression(measure),
        operator,
        right: ConditionNumber::Integer(value),
    }))
}

/// Offset of `have total` after a collection subject, before the clause ends.
fn collection_total_offset(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    (1..10)
        .take_while(|offset| {
            cursor
                .peek_ahead(offset - 1)
                .is_some_and(|raw| !matches!(raw, "." | "," | ";" | ":"))
        })
        .find(|offset| {
            cursor
                .peek_ahead(*offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("have"))
                && cursor
                    .peek_ahead(offset + 1)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("total"))
        })
}

/// Lexical lookahead for `COLLECTION have total PROPERTY THRESHOLD`.
pub(in crate::oracle::canonical) fn collection_total_atom_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    collection_total_offset(cursor).is_some()
}

/// `creatures you control have total power 8 or greater` (Formidable):
/// `Compare(Sum(property, COLLECTION), op, N)`. The collection is the shared
/// battlefield collection bounded before `have`; the aggregate is the same
/// typed property aggregate `the greatest power among` reads, so the lowering
/// decides which (property, relation) pairs the engine can sum.
pub(in crate::oracle::canonical) fn recognize_collection_total_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.collection-total.collection");
    let offset = collection_total_offset(cursor)
        .ok_or_else(|| cursor.fail("a collection followed by `have total`"))?;
    let end = cursor.position() + offset;
    let query = cursor.bounded(end, super::recognize_object_collection)?;
    cursor.require(
        !query.exclude_source && query.denotes_battlefield_permanents(),
        "a summed collection of battlefield permanents without a source exclusion",
    )?;
    cursor.require(
        query.controller == Some(super::ObjectControllerScope::AbilityController),
        "a summed collection controlled by you",
    )?;
    cursor.enter("condition.collection-total.property");
    cursor.consume_phrase(&["have", "total"])?;
    let property = super::recognize_selector_numeric_property(cursor)?;
    cursor.enter("condition.collection-total.threshold");
    let threshold = super::recognize_count_threshold(cursor, false)?;
    let (operator, value) = threshold_comparison(threshold);
    Ok(ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left: ConditionNumber::Expression(NumericExpression::GreatestPropertyAmong {
            property,
            predicate: query.predicate,
            aggregate: super::PropertyAggregate::Sum,
        }),
        operator,
        right: ConditionNumber::Integer(value),
    }))
}

/// Offset of the passive `is|are|was|were VERBed this way` after a counted
/// noun phrase, before the clause ends.
fn action_result_copula_offset(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    (1..10)
        .take_while(|offset| {
            cursor
                .peek_ahead(offset - 1)
                .is_some_and(|raw| !matches!(raw, "." | "," | ";" | ":"))
        })
        .find(|offset| {
            let word = |at: usize, expected: &str| {
                cursor
                    .peek_ahead(at)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
            };
            ["is", "are", "was", "were"]
                .iter()
                .any(|copula| word(*offset, copula))
                && super::action_result_participle_at(cursor, offset + 1).is_some_and(
                    |(_, width)| {
                        word(offset + 1 + width, "this") && word(offset + 2 + width, "way")
                    },
                )
        })
}

/// Lexical lookahead for `QUANTITY NOUN is|was VERBed this way`.
pub(in crate::oracle::canonical) fn action_result_condition_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    action_result_copula_offset(cursor).is_some()
}

/// `if a land card was milled this way`, `if no cards were discarded this
/// way`, `if at least one Zombie card is milled this way` (CR 608.2c):
/// `Compare(count(OBJECTS affected by the preceding action), op, N)`. The
/// atom names only the restated verb and noun; the program lowering binds
/// the preceding action of that verb, or fails when it is not one.
pub(in crate::oracle::canonical) fn recognize_action_result_condition_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.action-result.quantity");
    let offset = action_result_copula_offset(cursor)
        .ok_or_else(|| cursor.fail("an object phrase followed by `is|was VERBed this way`"))?;
    let copula = cursor.position() + offset;
    let threshold = super::recognize_count_threshold(cursor, true)?;
    cursor.enter("condition.action-result.objects");
    let query = cursor.bounded(copula, super::recognize_object_collection)?;
    cursor.require(
        !query.exclude_source && query.controller.is_none() && query.predicate.has_nominal_head(),
        "a nominal object phrase without a controller relation",
    )?;
    cursor.enter("condition.action-result.verb");
    let raw = cursor.peek().unwrap_or_default().to_owned();
    cursor.consume(&raw)?;
    let (verb, width) = super::action_result_participle_at(cursor, 0)
        .ok_or_else(|| cursor.fail("`destroyed`, `milled` or `discarded`"))?;
    for _ in 0..width {
        let raw = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&raw)?;
    }
    cursor.consume_phrase(&["this", "way"])?;
    let (operator, value) = threshold_comparison(threshold);
    Ok(ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left: ConditionNumber::ActionResultObjects(super::ActionResultReferenceNode {
            predicate: query.predicate,
            verb,
        }),
        operator,
        right: ConditionNumber::Integer(value),
    }))
}

/// Where a counted object phrase is stated to be, after its copula.
#[derive(Clone, Debug, PartialEq, Eq)]
enum LocatedQuantityTail {
    /// `on the battlefield`: permanents of every player (CR 110.1).
    Battlefield,
    /// `in your graveyard`, `in your hand`: the controller's card zone.
    ControllerZone(super::PlayerCardZone),
    /// A registered permanent status (`attacking`, `tapped`): battlefield
    /// permanents in that state.
    Status(ObjectPredicate),
}

/// The tail of a located quantity at `offset`, with its width. A status is
/// claimed only when the clause ends after it (`attacking you` names a
/// relation this tail does not read).
fn located_quantity_tail_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(LocatedQuantityTail, usize)> {
    let word = |at: usize, expected: &str| {
        cursor
            .peek_ahead(offset + at)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if word(0, "on") && word(1, "the") && word(2, "battlefield") {
        return Some((LocatedQuantityTail::Battlefield, 3));
    }
    if word(0, "in") && word(1, "your") {
        if word(2, "graveyard") {
            return Some((
                LocatedQuantityTail::ControllerZone(super::PlayerCardZone::Graveyard),
                3,
            ));
        }
        if word(2, "hand") {
            return Some((
                LocatedQuantityTail::ControllerZone(super::PlayerCardZone::Hand),
                3,
            ));
        }
        return None;
    }
    let status = cursor
        .peek_ahead(offset)
        .and_then(super::lookup_status_predicate)?;
    let ends = cursor.peek_ahead(offset + 1).is_none_or(|raw| {
        matches!(
            raw.to_ascii_lowercase().as_str(),
            "," | "." | ";" | "and" | "or"
        )
    });
    ends.then_some((LocatedQuantityTail::Status(status), 1))
}

/// The offset of the copula of `QUANTITY OBJECTS is|are TAIL` before the end
/// of the clause. Lexical lookahead only.
fn located_quantity_copula_offset(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let quantity = cursor.peek().is_some_and(|raw| {
        ["a", "an", "another", "no", "at", "exactly", "one"]
            .iter()
            .any(|word| raw.eq_ignore_ascii_case(word))
            || raw.parse::<i64>().is_ok()
            || super::super::parse_number_word(raw).is_some()
    });
    if !quantity {
        return None;
    }
    (1..12)
        .take_while(|offset| {
            cursor
                .peek_ahead(offset - 1)
                .is_some_and(|raw| !matches!(raw, "." | "," | ";" | ":"))
        })
        .find(|offset| {
            cursor.peek_ahead(*offset).is_some_and(|raw| {
                raw.eq_ignore_ascii_case("is") || raw.eq_ignore_ascii_case("are")
            }) && located_quantity_tail_at(cursor, offset + 1).is_some()
        })
}

/// Lexical lookahead for `QUANTITY OBJECTS is|are on the battlefield|in your
/// ZONE|STATUS`.
pub(in crate::oracle::canonical) fn located_quantity_atom_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    located_quantity_copula_offset(cursor).is_some()
}

/// `no creatures are on the battlefield`, `another creature is on the
/// battlefield`, `seven or more land cards are in your graveyard`, `a
/// Warrior card is in your graveyard`, `three or more creatures are
/// attacking`: `Compare(count(QUERY located there), op, N)`. The object
/// phrase is the shared object query (permanents of every player unless it
/// names a controller) or the shared card selector of the controller's
/// zone; the quantity is the shared threshold (`another` excludes the
/// source).
pub(in crate::oracle::canonical) fn recognize_located_quantity_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.located-quantity.quantity");
    let offset = located_quantity_copula_offset(cursor).ok_or_else(|| {
        cursor.fail("a counted object phrase followed by `is|are` and a location")
    })?;
    let copula = cursor.position() + offset;
    let plural = cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("are"));
    let (tail, tail_width) = located_quantity_tail_at(cursor, offset + 1)
        .ok_or_else(|| cursor.fail("`on the battlefield`, `in your ZONE` or a status"))?;
    let another = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("another"));
    let threshold = if another {
        let raw = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&raw)?;
        CountThreshold::at_least(1)
    } else {
        super::recognize_count_threshold(cursor, true)?
    };
    let atom = match &tail {
        LocatedQuantityTail::ControllerZone(zone) => {
            let zone = *zone;
            cursor.enter("condition.located-quantity.cards");
            cursor.require(!another, "counted cards without `another`")?;
            let predicate = cursor.bounded(copula, |cursor| {
                super::recognize_search_card_selector(cursor, plural)
            })?;
            ConditionAtom::Compare(ConditionComparison::measured(
                ConditionPlayerSubject::Controller,
                PlayerStateMeasure::CardsInZone { zone, predicate },
                threshold,
            ))
        }
        LocatedQuantityTail::Battlefield | LocatedQuantityTail::Status(_) => {
            cursor.enter("condition.located-quantity.permanents");
            let mut query = cursor.bounded(copula, |cursor| {
                super::recognize_query_excluding_source(cursor, recognize_object_query)
            })?;
            cursor.require(
                !(another && query.exclude_source),
                "one source exclusion for one object query",
            )?;
            query.exclude_source |= another;
            cursor.require(
                matches!(
                    query.controller,
                    None | Some(super::ObjectControllerScope::AbilityController)
                        | Some(super::ObjectControllerScope::OpponentOfAbilityController)
                ),
                "permanents of every player, of the controller or of the opponents",
            )?;
            cursor.require(
                query.denotes_battlefield_permanents(),
                "a selector that establishes the permanent domain",
            )?;
            let predicate = match &tail {
                LocatedQuantityTail::Status(status) => {
                    ObjectPredicate::Intersection(vec![query.predicate, status.clone()])
                }
                _ => query.predicate,
            };
            let (operator, value) = threshold_comparison(threshold);
            ConditionAtom::Compare(ConditionComparison {
                subject: ConditionPlayerSubject::Controller,
                left: ConditionNumber::Expression(NumericExpression::PermanentCount(Box::new(
                    super::PermanentCountNode {
                        predicate,
                        controller: query.controller,
                        exclude_source: query.exclude_source,
                        measure: super::PermanentCountMeasure::Objects,
                    },
                ))),
                operator,
                right: ConditionNumber::Integer(value),
            })
        }
    };
    cursor.enter("condition.located-quantity.location");
    for _ in 0..1 + tail_width {
        let raw = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&raw)?;
    }
    Ok(atom)
}

/// Reads `if ATOM,` for a family parent that composes exactly one family.
/// The atom is read by the shared reader; the family is the allow-list.
fn recognize_family_envelope(
    cursor: &mut OracleTokenCursor<'_>,
    state: &'static str,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionNode> {
    cursor.enter(state);
    cursor.consume("if")?;
    let atom = recognize_condition_atom(cursor, context)?;
    cursor.enter("condition.envelope.separator");
    cursor.consume(",")?;
    Ok(BooleanConditionNode::Atom(atom))
}

fn family_context(family: ConditionFamily) -> ConditionContext<'static> {
    ConditionContext::standalone(ConditionScope::Only(family), &[], None, false)
}

/// Recognizes `if you control a/an <permanent predicate>,` as the
/// controlled-permanent count comparison shared by every condition parent.
pub(in crate::oracle::canonical) fn recognize_controlled_permanent_presence_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionNode> {
    let node = recognize_family_envelope(
        cursor,
        "condition.controlled-permanent-presence.if",
        &family_context(ConditionFamily::Compare),
    )?;
    cursor.enter("condition.controlled-permanent-presence.measure");
    cursor.require(
        controlled_permanent_count(&node).is_some(),
        "a controlled permanent presence condition",
    )?;
    Ok(node)
}

/// The controlled-permanent count of a single comparison atom, if it is one.
pub(in crate::oracle::canonical) fn controlled_permanent_count(
    condition: &ConditionNode,
) -> Option<(&ObjectPredicate, NumericComparisonOperator, i64, bool)> {
    let BooleanConditionNode::Atom(ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left:
            ConditionNumber::Player(PlayerStateMeasure::ControlledPermanents {
                predicate,
                exclude_source,
            }),
        operator,
        right: ConditionNumber::Integer(threshold),
    })) = condition
    else {
        return None;
    };
    Some((predicate, *operator, *threshold, *exclude_source))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlledPermanentPresenceState {
    Player,
    Control,
    Determiner,
    PermanentPredicate,
    Complete,
}

/// Reads `you control a/an/another OBJECT` as a count query (at least one).
/// This query owns neither `if` nor the trailing clause comma: the shared
/// object query returns a comma that does not continue its own list.
pub(in crate::oracle::canonical) fn recognize_controlled_permanent_presence_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ControlledPermanentCountConditionNode> {
    controlled_permanent_presence_from(cursor, ControlledPermanentPresenceState::Player)
}

/// Words after which a backward scan for an elided `you control` stops: a
/// subject or a finite verb other than `control` opens another clause.
const ELLIPSIS_CLAUSE_BOUNDARIES: &[&str] = &[
    ",", ".", ";", ":", "if", "unless", "as", "controls", "has", "have", "had", "is", "are", "was",
    "were", "it", "it's", "they", "that", "this", "there", "you've", "you're",
];

/// Coordination ellipsis of a shared subject and verb (`if you control an
/// artifact and an enchantment`, `as long as you control a Forest or a
/// Plains`): true at the determiner of a conjunct directly after `and|or`
/// whose preceding conjunct in the same clause is `you control a|an|another
/// PERMANENT`. Lexical lookbehind only; the conjunct is read by the same
/// presence query, so `you control A and B` is `you control A` and `you
/// control B` (their Boolean connector belongs to the list parent).
pub(in crate::oracle::canonical) fn elided_controlled_presence_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let at = |index: usize| {
        cursor
            .token_slice(index, index + 1)
            .first()
            .map(|token| token.raw.replace('\u{2019}', "'").to_ascii_lowercase())
    };
    let determiner = |raw: Option<String>| matches!(raw.as_deref(), Some("a" | "an" | "another"));
    let position = cursor.position();
    if position < 4
        || !determiner(at(position))
        || !matches!(at(position - 1).as_deref(), Some("and" | "or"))
    {
        return false;
    }
    for index in (1..position - 1).rev() {
        let Some(raw) = at(index) else {
            return false;
        };
        if raw == "control" {
            if !(at(index - 1).as_deref() == Some("you") && determiner(at(index + 1))) {
                return false;
            }
            // The conjunct is only the presence query: a conjunct with its
            // own verb (`and a creature died this turn`) is another atom.
            let mut trial = cursor.clone();
            return controlled_permanent_presence_from(
                &mut trial,
                ControlledPermanentPresenceState::Determiner,
            )
            .is_ok()
                && trial.peek().is_none_or(|raw| {
                    matches!(
                        raw.to_ascii_lowercase().as_str(),
                        "," | "." | ";" | "and" | "or"
                    )
                });
        }
        if ELLIPSIS_CLAUSE_BOUNDARIES.contains(&raw.as_str()) {
            return false;
        }
    }
    false
}

/// Reads the conjunct whose `you control` was elided (see
/// `elided_controlled_presence_starts`): the presence query from its
/// determiner, the same controller comparison as the stated conjunct.
pub(in crate::oracle::canonical) fn recognize_elided_controlled_presence_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.controlled-permanent-presence.elided");
    cursor.require(
        elided_controlled_presence_starts(cursor),
        "a conjunct after `you control a PERMANENT and|or`",
    )?;
    controlled_permanent_presence_from(cursor, ControlledPermanentPresenceState::Determiner)
        .map(ControlledPermanentCountConditionNode::into_atom)
}

fn controlled_permanent_presence_from(
    cursor: &mut OracleTokenCursor<'_>,
    start: ControlledPermanentPresenceState,
) -> MachineResult<ControlledPermanentCountConditionNode> {
    let mut state = start;
    let mut predicate = None;
    let mut exclude_source = false;
    loop {
        state = match state {
            ControlledPermanentPresenceState::Player => {
                cursor.enter("condition.controlled-permanent-presence.player");
                cursor.consume("you")?;
                ControlledPermanentPresenceState::Control
            }
            ControlledPermanentPresenceState::Control => {
                cursor.enter("condition.controlled-permanent-presence.control");
                cursor.consume("control")?;
                ControlledPermanentPresenceState::Determiner
            }
            ControlledPermanentPresenceState::Determiner => {
                cursor.enter("condition.controlled-permanent-presence.determiner");
                let determiner = cursor.consume_type("quantifierWord")?;
                cursor.require(
                    determiner.raw.eq_ignore_ascii_case("a")
                        || determiner.raw.eq_ignore_ascii_case("an")
                        || determiner.raw.eq_ignore_ascii_case("another"),
                    "`a`, `an` or `another` before a controlled permanent predicate",
                )?;
                // `another` is `a` that is not the ability's own source (CR 201.5).
                exclude_source = determiner.raw.eq_ignore_ascii_case("another");
                ControlledPermanentPresenceState::PermanentPredicate
            }
            ControlledPermanentPresenceState::PermanentPredicate => {
                cursor.enter("condition.controlled-permanent-presence.permanent-predicate");
                let query = recognize_object_query(cursor)?;
                cursor.require(
                    query.controller.is_none(),
                    "an unscoped permanent selector after `you control`",
                )?;
                cursor.require(
                    query.predicate.supports_domain(ObjectKind::Permanent),
                    "a predicate valid for a battlefield permanent",
                )?;
                predicate = Some(query.predicate);
                ControlledPermanentPresenceState::Complete
            }
            ControlledPermanentPresenceState::Complete => {
                cursor.enter("condition.controlled-permanent-presence.complete");
                return Ok(ControlledPermanentCountConditionNode {
                    player: LocalPlayerReference::Controller,
                    threshold: 1,
                    direction: ControlledCountDirection::OrMore,
                    predicate: cursor
                        .require_value(predicate, "the controlled permanent predicate")?,
                    exclude_source,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlledPermanentCountState {
    Player,
    Control,
    ComparatorPrefix,
    Threshold,
    ComparatorConnector,
    ComparatorDirection,
    OptionalOther,
    PermanentPredicate,
    Complete,
}

/// Recognizes the reusable count query `you control N or more/fewer [other]
/// <permanents>`. The surrounding condition owns `if`/`unless` and punctuation.
pub(in crate::oracle::canonical) fn recognize_controlled_permanent_count_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ControlledPermanentCountConditionNode> {
    let mut state = ControlledPermanentCountState::Player;
    let mut threshold = None;
    let mut direction = None;
    let mut predicate = None;
    let mut exclude_source = false;
    loop {
        state = match state {
            ControlledPermanentCountState::Player => {
                cursor.enter("condition.controlled-permanent-count.player");
                cursor.consume("you")?;
                ControlledPermanentCountState::Control
            }
            ControlledPermanentCountState::Control => {
                cursor.enter("condition.controlled-permanent-count.control");
                cursor.consume("control")?;
                ControlledPermanentCountState::ComparatorPrefix
            }
            ControlledPermanentCountState::ComparatorPrefix => {
                cursor.enter("condition.controlled-permanent-count.comparator-prefix");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("at"))
                {
                    cursor.consume("at")?;
                    direction = Some(match cursor.peek() {
                        Some(raw) if raw.eq_ignore_ascii_case("least") => {
                            cursor.consume("least")?;
                            ControlledCountDirection::OrMore
                        }
                        Some(raw) if raw.eq_ignore_ascii_case("most") => {
                            cursor.consume("most")?;
                            ControlledCountDirection::OrFewer
                        }
                        _ => return Err(cursor.fail("`least` or `most` after `at`")),
                    });
                }
                ControlledPermanentCountState::Threshold
            }
            ControlledPermanentCountState::Threshold => {
                cursor.enter("condition.controlled-permanent-count.threshold");
                let value = match recognize_numeric_atom(cursor)? {
                    NumericAtom::Integer(value) => value,
                    NumericAtom::VariableX => {
                        return Err(cursor.fail("a fixed nonnegative permanent-count threshold"));
                    }
                };
                cursor.require(value >= 0, "a fixed nonnegative permanent-count threshold")?;
                threshold = Some(value);
                if direction.is_some() {
                    ControlledPermanentCountState::OptionalOther
                } else {
                    ControlledPermanentCountState::ComparatorConnector
                }
            }
            ControlledPermanentCountState::ComparatorConnector => {
                cursor.enter("condition.controlled-permanent-count.comparator-connector");
                cursor.consume("or")?;
                ControlledPermanentCountState::ComparatorDirection
            }
            ControlledPermanentCountState::ComparatorDirection => {
                cursor.enter("condition.controlled-permanent-count.comparator-direction");
                direction = Some(match cursor.peek() {
                    Some(raw) if raw.eq_ignore_ascii_case("more") => {
                        cursor.consume("more")?;
                        ControlledCountDirection::OrMore
                    }
                    Some(raw) if raw.eq_ignore_ascii_case("fewer") => {
                        cursor.consume("fewer")?;
                        ControlledCountDirection::OrFewer
                    }
                    _ => return Err(cursor.fail("`more` or `fewer`")),
                });
                ControlledPermanentCountState::OptionalOther
            }
            ControlledPermanentCountState::OptionalOther => {
                // `other` is read with the query, as its own field.
                cursor.enter("condition.controlled-permanent-count.optional-other");
                ControlledPermanentCountState::PermanentPredicate
            }
            ControlledPermanentCountState::PermanentPredicate => {
                cursor.enter("condition.controlled-permanent-count.permanent-predicate");
                let query =
                    super::recognize_query_excluding_source(cursor, recognize_object_query)?;
                cursor.require(
                    query.controller.is_none(),
                    "an unscoped permanent selector after `you control`",
                )?;
                exclude_source = query.exclude_source;
                // The leading `you control` is this query's control relation.
                let controlled = super::ObjectQueryNode {
                    controller: Some(super::ObjectControllerScope::AbilityController),
                    ..query
                };
                cursor.require(
                    controlled.denotes_battlefield_permanents(),
                    "a selector that establishes the permanent domain",
                )?;
                predicate = Some(controlled.predicate);
                ControlledPermanentCountState::Complete
            }
            ControlledPermanentCountState::Complete => {
                cursor.enter("condition.controlled-permanent-count.complete");
                return Ok(ControlledPermanentCountConditionNode {
                    player: LocalPlayerReference::Controller,
                    threshold: cursor.require_value(threshold, "the count threshold")?,
                    direction: cursor.require_value(direction, "the comparison direction")?,
                    predicate: cursor
                        .require_value(predicate, "the counted permanent predicate")?,
                    exclude_source,
                });
            }
        };
    }
}

/// Player-count query `you have N or more/fewer opponents` (CR 102.3: the
/// opponents of the reference player). The surrounding condition owns
/// `if`/`unless` and punctuation; threshold and direction stay typed values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct OpponentCountConditionNode {
    pub player: LocalPlayerReference,
    pub threshold: i64,
    pub direction: ControlledCountDirection,
}

pub(in crate::oracle::canonical) fn recognize_opponent_count_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<OpponentCountConditionNode> {
    cursor.enter("condition.opponent-count.player");
    cursor.consume("you")?;
    cursor.enter("condition.opponent-count.have");
    cursor.consume("have")?;
    cursor.enter("condition.opponent-count.threshold");
    let threshold = match recognize_numeric_atom(cursor)? {
        NumericAtom::Integer(value) if value >= 0 => value,
        _ => return Err(cursor.fail("a fixed nonnegative opponent-count threshold")),
    };
    cursor.enter("condition.opponent-count.comparator-connector");
    cursor.consume("or")?;
    cursor.enter("condition.opponent-count.comparator-direction");
    let direction = match cursor.peek() {
        Some(raw) if raw.eq_ignore_ascii_case("more") => ControlledCountDirection::OrMore,
        Some(raw) if raw.eq_ignore_ascii_case("fewer") => ControlledCountDirection::OrFewer,
        _ => return Err(cursor.fail("`more` or `fewer`")),
    };
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("condition.opponent-count.noun");
    let noun = cursor.peek().ok_or_else(|| cursor.fail("`opponents`"))?;
    cursor.require(
        noun.eq_ignore_ascii_case("opponents")
            || (threshold == 1 && noun.eq_ignore_ascii_case("opponent")),
        "`opponents`",
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("condition.opponent-count.complete");
    Ok(OpponentCountConditionNode {
        player: LocalPlayerReference::Controller,
        threshold,
        direction,
    })
}

/// Recognizes `if you control N or more/fewer <permanents>,` through the
/// shared atom reader.
pub(in crate::oracle::canonical) fn recognize_controlled_permanent_count_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionNode> {
    let node = recognize_family_envelope(
        cursor,
        "condition.controlled-permanent-count.if",
        &family_context(ConditionFamily::Compare),
    )?;
    cursor.enter("condition.controlled-permanent-count.measure");
    cursor.require(
        controlled_permanent_count(&node).is_some(),
        "a controlled permanent count condition",
    )?;
    Ok(node)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardsExistInZoneConditionState {
    If,
    Existential,
    Copula,
    Determiner,
    Predicate,
    ConnectorOrZone,
    ZoneRelation,
    ZonePossessor,
    Zone,
    Separator,
    Complete,
}

/// Recognizes an existential conjunction over typed card predicates in one
/// player zone, for example `if there is an instant card and a sorcery card
/// in your graveyard,`. Each object phrase is delegated to the shared card
/// selector, while this parent owns Boolean `and` and the zone scope: the
/// result is a conjunction of the shared card-count comparisons.
pub(in crate::oracle::canonical) fn recognize_cards_exist_in_controller_graveyard_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionNode> {
    let mut state = CardsExistInZoneConditionState::If;
    let mut predicates = Vec::new();
    loop {
        state = match state {
            CardsExistInZoneConditionState::If => {
                cursor.enter("condition.zone-card-presence.if");
                cursor.consume("if")?;
                CardsExistInZoneConditionState::Existential
            }
            CardsExistInZoneConditionState::Existential => {
                cursor.enter("condition.zone-card-presence.existential");
                cursor.consume("there")?;
                CardsExistInZoneConditionState::Copula
            }
            CardsExistInZoneConditionState::Copula => {
                cursor.enter("condition.zone-card-presence.copula");
                cursor.consume("is")?;
                CardsExistInZoneConditionState::Determiner
            }
            CardsExistInZoneConditionState::Determiner => {
                cursor.enter("condition.zone-card-presence.determiner");
                let determiner = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("`a` or `an` before a card predicate"))?
                    .to_string();
                cursor.require(
                    determiner.eq_ignore_ascii_case("a") || determiner.eq_ignore_ascii_case("an"),
                    "`a` or `an` before a card predicate",
                )?;
                cursor.consume(&determiner)?;
                CardsExistInZoneConditionState::Predicate
            }
            CardsExistInZoneConditionState::Predicate => {
                cursor.enter("condition.zone-card-presence.predicate");
                let predicate = recognize_card_type_union_selector(cursor)?;
                cursor.require(
                    predicate.is_in_domain(ObjectKind::Card),
                    "a selector in the card domain",
                )?;
                predicates.push(predicate);
                CardsExistInZoneConditionState::ConnectorOrZone
            }
            CardsExistInZoneConditionState::ConnectorOrZone => {
                cursor.enter("condition.zone-card-presence.connector-or-zone");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                {
                    cursor.consume("and")?;
                    CardsExistInZoneConditionState::Determiner
                } else {
                    CardsExistInZoneConditionState::ZoneRelation
                }
            }
            CardsExistInZoneConditionState::ZoneRelation => {
                cursor.enter("condition.zone-card-presence.zone-relation");
                cursor.consume("in")?;
                CardsExistInZoneConditionState::ZonePossessor
            }
            CardsExistInZoneConditionState::ZonePossessor => {
                cursor.enter("condition.zone-card-presence.zone-possessor");
                cursor.consume("your")?;
                CardsExistInZoneConditionState::Zone
            }
            CardsExistInZoneConditionState::Zone => {
                cursor.enter("condition.zone-card-presence.zone");
                cursor.consume("graveyard")?;
                CardsExistInZoneConditionState::Separator
            }
            CardsExistInZoneConditionState::Separator => {
                cursor.enter("condition.zone-card-presence.separator");
                cursor.consume(",")?;
                CardsExistInZoneConditionState::Complete
            }
            CardsExistInZoneConditionState::Complete => {
                cursor.enter("condition.zone-card-presence.complete");
                cursor.require(!predicates.is_empty(), "one or more card predicates")?;
                // One conjunction per existential list, also of one member,
                // so the lowered program keeps a single shape.
                let atoms = predicates
                    .into_iter()
                    .map(|predicate| {
                        BooleanConditionNode::Atom(ConditionAtom::Compare(
                            ConditionComparison::measured(
                                ConditionPlayerSubject::Controller,
                                PlayerStateMeasure::CardsInZone {
                                    zone: super::PlayerCardZone::Graveyard,
                                    predicate,
                                },
                                CountThreshold::at_least(1),
                            ),
                        ))
                    })
                    .collect::<Vec<_>>();
                return Ok(BooleanConditionNode::And(atoms));
            }
        };
    }
}

/// Recognizes the event-history condition shared by revolt-style text
/// (`if a permanent left the battlefield under your control this turn`)
/// through the shared history reader, without assigning any meaning to a
/// decorative ability-word label.
pub(in crate::oracle::canonical) fn recognize_controlled_permanent_left_battlefield_this_turn_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionNode> {
    cursor.enter("condition.permanent-left-battlefield.if");
    cursor.consume("if")?;
    let atom = recognize_condition_atom(cursor, &family_context(ConditionFamily::EventHistory))?;
    cursor.enter("condition.permanent-left-battlefield.event");
    cursor.require(
        matches!(
            &atom,
            ConditionAtom::EventHistory(EventHistoryNode {
                pattern: EventHistoryPattern {
                    event: HistoryEvent::PermanentLeftBattlefield,
                    ..
                },
                ..
            })
        ),
        "a permanent that left the battlefield",
    )?;
    Ok(BooleanConditionNode::Atom(atom))
}

/// Recognizes `if you don't,` while resolving the omitted payment through the
/// typed binding supplied by its parent sequence.
pub(in crate::oracle::canonical) fn recognize_cost_not_paid_condition(
    cursor: &mut OracleTokenCursor<'_>,
    payment: ChoiceBinding,
) -> MachineResult<ConditionNode> {
    let mut context = family_context(ConditionFamily::ActionOutcome);
    context.payment = Some(payment);
    recognize_family_envelope(cursor, "condition.cost-not-paid.if", &context)
}

/// Recognizes the reusable proposition `it's [not] your turn` without an
/// introducer or separator, so entry, cost and effect parents can compose it.
pub(in crate::oracle::canonical) fn recognize_controller_turn_proposition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ControllerTurnPropositionNode> {
    cursor.enter("condition.controller-turn.subject");
    let subject = cursor.consume_type("referenceWord")?;
    cursor.require(
        subject.raw.eq_ignore_ascii_case("it's") || subject.raw.eq_ignore_ascii_case("it’s"),
        "the current-turn reference `it's`",
    )?;
    cursor.enter("condition.controller-turn.negation");
    let negated = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("not"));
    if negated {
        cursor.consume("not")?;
    }
    cursor.enter("condition.controller-turn.possessor");
    cursor.consume("your")?;
    cursor.enter("condition.controller-turn.turn");
    cursor.consume("turn")?;
    Ok(ControllerTurnPropositionNode {
        is_controller_turn: !negated,
    })
}

/// Recognizes the turn condition `if it's not your turn,` through the shared
/// atom reader.
pub(in crate::oracle::canonical) fn recognize_not_your_turn_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionNode> {
    let node = recognize_family_envelope(
        cursor,
        "condition.not-your-turn.if",
        &family_context(ConditionFamily::TurnOwnership),
    )?;
    cursor.enter("condition.not-your-turn.polarity");
    cursor.require(
        node == BooleanConditionNode::Atom(ConditionAtom::TurnOwnership(
            ControllerTurnPropositionNode {
                is_controller_turn: false,
            },
        )),
        "`it's not your turn`",
    )?;
    Ok(node)
}

/// Recognizes the reusable condition envelope `if this spell was kicked,`.
///
/// The spell reference is read by the shared atom reader with the shared
/// reference automaton, so the condition retains the same binding semantics
/// as an effect source.
pub(in crate::oracle::canonical) fn recognize_kicked_condition(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<ConditionNode> {
    let context = ConditionContext::standalone(
        ConditionScope::Only(ConditionFamily::CastFact),
        name_links,
        name_failure,
        false,
    );
    let node = recognize_family_envelope(cursor, "condition.kicked.if", &context)?;
    cursor.enter("condition.kicked.fact");
    cursor.require(
        matches!(
            node,
            BooleanConditionNode::Atom(ConditionAtom::CastFact(CastFactNode {
                fact: CastFact::Kicked,
                ..
            }))
        ),
        "`was kicked`",
    )?;
    Ok(node)
}

#[cfg(test)]
#[path = "condition_controlled_presence_tests.rs"]
mod controlled_presence_tests;

#[cfg(test)]
#[path = "condition_count_comparative_tests.rs"]
mod count_comparative_tests;
