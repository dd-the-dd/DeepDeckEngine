use crate::oracle::counter_name::CounterName;

use super::{
    CardColor, CardSupertype, CharacteristicBatch, ColorQuantity, GrantedKeyword, ManaCostNode,
    NumericComparisonOperator, NumericExpression, ObjectCardType, ObjectControllerScope,
    ObjectKind, PermanentDesignation, SelectorNumericProperty,
};

#[cfg(test)]
use super::NumericAtom;

/// A battlefield status adjective (`tapped`, `untapped`, `attacking`,
/// `blocking`). `untapped` is the negation of the tapped status (CR 110.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectStatus {
    Tapped,
    Untapped,
    Attacking,
    Blocking,
    /// CR 700.9: it has a counter, an Aura its controller controls or an
    /// Equipment attached.
    Modified,
    /// An Aura is attached to it.
    Enchanted,
    /// An Equipment is attached to it (CR 301.5).
    Equipped,
    /// CR 509.1h: an attacking creature that a creature blocked. It remains
    /// blocked for the rest of combat even if its blockers leave combat.
    Blocked,
    /// CR 509.1h: an attacking creature no creature blocked.
    Unblocked,
    /// This-turn history of the object, read by a past-tense relative
    /// clause (`that entered this turn`, `that was dealt damage this turn`).
    /// They have no adjective surface.
    EnteredThisTurn,
    AttackedThisTurn,
    BlockedThisTurn,
    /// `that was blocked this turn` (CR 509.1h): a blocker was declared for
    /// the attacking object this turn.
    WasBlockedThisTurn,
    WasDealtDamageThisTurn,
    DealtDamageThisTurn,
    /// `[cards in a zone] that were put there from the battlefield this
    /// turn`: the card's latest arrival in the zone it is in was its
    /// departure from the battlefield during this turn (CR 400.7: the card
    /// is a new object that remembers this zone change, CR 603.10a). A fact
    /// of a card in a zone, never of a permanent.
    PutThereFromBattlefieldThisTurn,
    /// `[cards in a graveyard] that were put there this turn`: the card's
    /// latest arrival in the zone it is in happened during this turn,
    /// whatever zone it came from (CR 400.7). A fact of a card in a zone.
    PutThereThisTurn,
    /// `[cards in a graveyard] that were milled this turn` (CR 701.13b):
    /// the card's latest arrival in its zone this turn was a mill.
    MilledThisTurn,
}

impl ObjectStatus {
    pub(in crate::oracle::canonical) fn from_adjective(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "tapped" => Some(Self::Tapped),
            "untapped" => Some(Self::Untapped),
            "attacking" => Some(Self::Attacking),
            "blocking" => Some(Self::Blocking),
            "modified" => Some(Self::Modified),
            "enchanted" => Some(Self::Enchanted),
            "equipped" => Some(Self::Equipped),
            "blocked" => Some(Self::Blocked),
            "unblocked" => Some(Self::Unblocked),
            _ => None,
        }
    }

    /// The runtime status atom and whether this adjective negates it.
    pub(in crate::oracle::canonical) const fn ir_atom(self) -> (&'static str, bool) {
        match self {
            Self::Tapped => ("isTapped", false),
            Self::Untapped => ("isTapped", true),
            Self::Attacking => ("isAttacking", false),
            Self::Blocking => ("isBlocking", false),
            Self::Modified => ("isModified", false),
            Self::Enchanted => ("hasAttachedAura", false),
            Self::Equipped => ("isEquipped", false),
            Self::Blocked => ("isBlocked", false),
            Self::Unblocked => ("isUnblockedAttacker", false),
            Self::EnteredThisTurn => ("enteredThisTurn", false),
            Self::AttackedThisTurn => ("attackedThisTurn", false),
            Self::BlockedThisTurn => ("blockedThisTurn", false),
            Self::WasBlockedThisTurn => ("wasBlockedThisTurn", false),
            Self::WasDealtDamageThisTurn => ("wasDealtDamageThisTurn", false),
            Self::DealtDamageThisTurn => ("dealtDamageThisTurn", false),
            Self::PutThereFromBattlefieldThisTurn => ("putThereFromBattlefieldThisTurn", false),
            Self::PutThereThisTurn => ("putThereThisTurn", false),
            Self::MilledThisTurn => ("milledThisTurn", false),
        }
    }

    /// The domain a status is a fact of: the zone history of a card in a
    /// zone is read on cards; every other status is a battlefield state or
    /// history of a permanent.
    pub(in crate::oracle::canonical) fn supports_domain(self, domain: ObjectKind) -> bool {
        match self {
            Self::PutThereFromBattlefieldThisTurn
            | Self::PutThereThisTurn
            | Self::MilledThisTurn => domain == ObjectKind::Card,
            _ => domain == ObjectKind::Permanent,
        }
    }
}

/// How many counters (of one kind, or of any kind) an object has: at least
/// `minimum` and, when bounded, at most `maximum`. `a counter` is
/// `minimum: 1`; `no counters` is `minimum: 0, maximum: Some(0)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CounterPresence {
    pub counter: Option<CounterName>,
    pub minimum: i64,
    pub maximum: Option<i64>,
}

impl CounterPresence {
    /// At least one counter (of the kind when named).
    pub(in crate::oracle::canonical) fn any(counter: Option<CounterName>) -> Self {
        Self {
            counter,
            minimum: 1,
            maximum: None,
        }
    }
}

/// The player a relation names (`to you`, `to an opponent`, `to a player`;
/// `that targets you`), relative to the ability controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DamagedPlayer {
    AbilityController,
    OpponentOfAbilityController,
    AnyPlayer,
}

impl DamagedPlayer {
    pub(in crate::oracle::canonical) const fn ir_name(self) -> &'static str {
        match self {
            Self::AbilityController => "abilityController",
            Self::OpponentOfAbilityController => "opponentOfAbilityController",
            Self::AnyPlayer => "anyPlayer",
        }
    }
}

/// The permanents a spell's targets may be (`that targets a creature you
/// control`): the shared permanent predicate and its controller relative to
/// the ability controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetedPermanents {
    pub predicate: Box<ObjectPredicate>,
    pub controller: Option<ObjectControllerScope>,
}

/// What a spell targets (CR 115.1, 115.10): one of its targets is one of the
/// listed players or a permanent of the listed query (`that targets you or a
/// permanent you control`). At least one alternative is present.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetedObjects {
    pub players: Option<DamagedPlayer>,
    pub permanents: Option<TargetedPermanents>,
}

/// A relation of the object to a name, a player or the turn's history,
/// read by a postnominal or relative clause of the shared object query
/// (`named Fenric`, `that dealt combat damage to you this turn`). Each
/// relation states the object domains it can be evaluated in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectRelation {
    /// `named NAME` (CR 201.2): the object's name is NAME.
    Named(String),
    /// `that dealt [combat] damage to PLAYER this turn` (CR 120.1, 510.2):
    /// a damage event of this turn whose source is this permanent and whose
    /// recipient is that player.
    DealtDamageToPlayerThisTurn {
        recipient: DamagedPlayer,
        combat_only: bool,
    },
    /// `that targets OBJECT|PLAYER` (CR 115.1): a spell one of whose
    /// targets is a listed player or a permanent of the listed query.
    Targets(TargetedObjects),
    /// `with the chosen name` / `with that name` (CR 201.2, 608.2d): the
    /// object's name is the card name an earlier `choose a card name` of the
    /// same effect program recorded. Symbolic; the rule validator proves
    /// the producer.
    ChosenName,
    /// A relation to another object (`attached to that creature`,
    /// `blocking it`, `that shares a creature type with it`), read by the
    /// relative slot (`relative_clause.rs`).
    Related(super::RelatedObject),
    /// `spells you cast from your graveyard` (CR 601.2a): the zone the spell
    /// was cast from. A fact of the spell's cast, never a characteristic;
    /// only a consumer that records the cast zone evaluates it.
    CastFrom(CastOrigin),
}

/// A zone a spell can be cast from (CR 601.2a), as a cast origin names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CastOriginZone {
    Hand,
    Graveyard,
    Library,
    Exile,
}

impl CastOriginZone {
    pub(in crate::oracle::canonical) const fn ir_name(self) -> &'static str {
        match self {
            Self::Hand => "hand",
            Self::Graveyard => "graveyard",
            Self::Library => "library",
            Self::Exile => "exile",
        }
    }

    /// A zone word after `your` (`your graveyard`) or alone (`exile`).
    pub(in crate::oracle::canonical) fn from_word(raw: &str) -> Option<Self> {
        Some(match raw.to_ascii_lowercase().as_str() {
            "hand" => Self::Hand,
            "graveyard" => Self::Graveyard,
            "library" => Self::Library,
            "exile" => Self::Exile,
            _ => return None,
        })
    }
}

/// `from your graveyard`, `from exile` after a cast relation: the zone and
/// whether the text names the caster's own zone (`your graveyard`, CR
/// 400.3: a card is in its owner's graveyard) rather than the shared exile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CastOrigin {
    pub zone: CastOriginZone,
    pub caster_owned: bool,
}

impl ObjectRelation {
    /// A name is a characteristic of every object (CR 109.3); a damage
    /// history is read from a battlefield permanent's own events.
    pub(in crate::oracle::canonical) fn supports_domain(&self, domain: ObjectKind) -> bool {
        match self {
            Self::Named(_) | Self::ChosenName => true,
            Self::DealtDamageToPlayerThisTurn { .. } => domain == ObjectKind::Permanent,
            // The targets of an object on the stack (CR 115.1).
            Self::Targets(_) => domain == ObjectKind::Spell,
            // The cast record of a spell (CR 601.2a).
            Self::CastFrom(_) => domain == ObjectKind::Spell,
            Self::Related(related) => related.supports_domain(domain),
        }
    }

    /// Whether the relation reads the permanent's state or history rather
    /// than a characteristic of its definition.
    pub(in crate::oracle::canonical) fn reads_object_state(&self) -> bool {
        matches!(
            self,
            Self::DealtDamageToPlayerThisTurn { .. }
                | Self::Targets(_)
                | Self::Related(_)
                | Self::CastFrom(_)
        )
    }
}

/// A keyword ability an object has whatever its parameters (CR 702): the
/// keywords whose printed form carries a cost or a number, read by a `with`
/// qualifier (`cards with cycling`, `a card with a kicker ability`). It is a
/// fact about the object's abilities, never a grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum KeywordAbilityName {
    /// CR 702.29: cycling; typecycling is a cycling ability (CR 702.29e-f).
    Cycling,
    /// CR 702.34: flashback.
    Flashback,
    /// CR 702.33: kicker (and multikicker, CR 702.33c).
    Kicker,
    /// CR 702.113: awaken.
    Awaken,
    /// CR 702.164: toxic, whatever its count (`creatures with toxic`).
    Toxic,
}

impl KeywordAbilityName {
    /// The Oracle word naming the keyword.
    pub(in crate::oracle::canonical) fn from_oracle_word(raw: &str) -> Option<Self> {
        Some(match raw.to_ascii_lowercase().as_str() {
            "cycling" => Self::Cycling,
            "flashback" => Self::Flashback,
            "kicker" => Self::Kicker,
            "awaken" => Self::Awaken,
            "toxic" => Self::Toxic,
            _ => return None,
        })
    }

    /// The keyword-ability rule kinds the runtime records for it: every
    /// kind is the same keyword (a typecycling ability is a cycling one).
    pub(in crate::oracle::canonical) const fn ir_kinds(self) -> &'static [&'static str] {
        match self {
            Self::Cycling => &["cycling", "typecycling", "basicLandcycling"],
            Self::Flashback => &["flashback"],
            Self::Kicker => &["kicker"],
            Self::Awaken => &["awaken"],
            Self::Toxic => &["toxic"],
        }
    }
}

/// A class of symbols in an object's mana cost (CR 107.4, 202.1), read from
/// the printed cost the runtime keeps typed, never from an Oracle string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ManaCostSymbolClass {
    /// `{X} in its mana cost` (CR 107.3).
    VariableX,
}

/// The value a set constraint compares between chosen objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DistinctProperty {
    /// CR 201.2: two objects have different names.
    Name,
    Power,
    ManaValue,
}

impl DistinctProperty {
    pub(in crate::oracle::canonical) const fn ir_name(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Power => "power",
            Self::ManaValue => "manaValue",
        }
    }
}

/// Which end of an ordering a superlative names (`the greatest`, `the
/// least`). Ties are members: every object whose value equals the extreme
/// value has the greatest value (CR 107.1 compares numbers; an object tied
/// for the greatest is among those with the greatest).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum Extreme {
    Greatest,
    Least,
}

impl Extreme {
    pub(in crate::oracle::canonical) const fn ir_name(self) -> &'static str {
        match self {
            Self::Greatest => "greatest",
            Self::Least => "least",
        }
    }
}

/// The population a superlative ranks the candidate within, relative to the
/// candidate itself or to the whole battlefield.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ExtremumPopulation {
    /// `among creatures on the battlefield`: every permanent of the query.
    Battlefield,
    /// `among creatures they control` / `that player controls`, where that
    /// player is the one choosing among their own permanents: the query's
    /// permanents controlled by the candidate's controller.
    CandidateController,
    /// `among creatures you control`.
    AbilityController,
}

impl ExtremumPopulation {
    pub(in crate::oracle::canonical) const fn ir_name(self) -> &'static str {
        match self {
            Self::Battlefield => "battlefield",
            Self::CandidateController => "candidateController",
            Self::AbilityController => "abilityController",
        }
    }
}

/// The qualifier atoms of the shared object query that are not printed
/// characteristics of one object read alone: a keyword ability with
/// parameters, a symbol class of the mana cost, the turn's history of the
/// object relative to the ability source, a constraint over the whole set
/// being chosen, and a superlative ranking among a population.
///
/// A set constraint (`DistinctValues`, `TotalAtMost`) is not a candidate
/// filter: only a consumer selecting or counting a set evaluates it, and it
/// must sit at the top level of the query's conjunction. Every other
/// consumer refuses it (see [`ObjectPredicate::split_set_constraints`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectQualifier {
    KeywordAbility(KeywordAbilityName),
    ManaCostSymbol(ManaCostSymbolClass),
    /// `dealt damage by this creature this turn` (CR 120.1): a damage event
    /// of this turn whose source is the ability source and whose recipient
    /// is this permanent.
    DamagedBySourceThisTurn,
    /// `with different names|powers|mana values`: no two chosen objects
    /// share the value.
    DistinctValues(DistinctProperty),
    /// `with total mana value N or less`: the sum over the chosen objects.
    TotalAtMost {
        property: SelectorNumericProperty,
        maximum: Box<NumericExpression>,
    },
    /// `that share a land type` after a plural chosen set (CR 205.3i): the
    /// chosen objects have one value of the characteristic in common.
    SharedValue(super::SharedCharacteristic),
    /// `controlled by different players` after a plural chosen set: no two
    /// chosen permanents have the same controller (CR 108.4).
    DistinctControllers,
    /// `controlled by the same player` after a plural chosen set: every
    /// chosen permanent has one controller (CR 108.4).
    SameController,
    /// `with the greatest power among creatures they control`.
    Extremum {
        property: SelectorNumericProperty,
        extreme: Extreme,
        among: Box<ObjectPredicate>,
        population: ExtremumPopulation,
    },
    /// `that entered the battlefield under your control this turn` (CR
    /// 110.2a): the permanent entered this turn, and the player relative to
    /// the ability controller controlled it as it entered. A later control
    /// change does not alter that fact.
    EnteredUnderControlThisTurn(DamagedPlayer),
    /// `attacking you [or a planeswalker you control]` (CR 508.1b, 506.3):
    /// the permanent is an attacking creature whose defender is the player
    /// relative to the ability controller or, when `planeswalkers`, a
    /// planeswalker that player controls.
    AttackingPlayer {
        player: DamagedPlayer,
        planeswalkers: bool,
    },
    /// `with toughness greater than its power`: one property of the
    /// candidate compared with another property of the same candidate (CR
    /// 208.1). It depends on nothing but the candidate.
    CandidatePropertyComparison {
        left: SelectorNumericProperty,
        operator: NumericComparisonOperator,
        right: SelectorNumericProperty,
    },
}

impl ObjectQualifier {
    /// The object domains the atom can be evaluated in.
    pub(in crate::oracle::canonical) fn supports_domain(&self, domain: ObjectKind) -> bool {
        match self {
            Self::KeywordAbility(_)
            | Self::ManaCostSymbol(_)
            | Self::DistinctValues(_)
            | Self::TotalAtMost { .. }
            | Self::SharedValue(_)
            | Self::CandidatePropertyComparison { .. } => true,
            Self::DamagedBySourceThisTurn
            | Self::Extremum { .. }
            | Self::DistinctControllers
            | Self::SameController
            | Self::EnteredUnderControlThisTurn(_)
            | Self::AttackingPlayer { .. } => domain == ObjectKind::Permanent,
        }
    }

    /// A constraint over the chosen set rather than over one candidate.
    pub(in crate::oracle::canonical) const fn is_set_constraint(&self) -> bool {
        matches!(
            self,
            Self::DistinctValues(_)
                | Self::TotalAtMost { .. }
                | Self::SharedValue(_)
                | Self::DistinctControllers
                | Self::SameController
        )
    }

    /// Whether the atom reads the declared `X` (`with total mana value X or
    /// less`), which a consumer without that declaration cannot bind.
    pub(in crate::oracle::canonical) fn uses_x(&self) -> bool {
        match self {
            Self::TotalAtMost { maximum, .. } => maximum.uses_x(),
            _ => false,
        }
    }
}

/// A property predicate over an object selected by a surrounding query.
///
/// The surrounding grammar still owns the object domain, zone, controller,
/// cardinality, and any binding. This node only composes reusable object
/// characteristics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectPredicate {
    IsCommander,
    IsToken,
    CardType(ObjectCardType),
    Subtype(String),
    CharacteristicBatch(CharacteristicBatch),
    Color(CardColor),
    /// The number of colors of the object (CR 105.2a-c), read from the same
    /// color characteristic as `Color`.
    ColorQuantity(ColorQuantity),
    Supertype(CardSupertype),
    ExactManaCost(ManaCostNode),
    HasKeyword(GrantedKeyword),
    ObjectKind(ObjectKind),
    Status(ObjectStatus),
    /// `renowned`, `monstrous`, `suspected` (CR 701.37b, 702.112b, 701.60b):
    /// a designation of the permanent, from the shared registry.
    Designation(PermanentDesignation),
    /// `you own` / `you don't own` (CR 108.3): the object's owner relative
    /// to the ability controller. A collection carries it as its ownership
    /// relation, never as a characteristic.
    OwnedByAbilityController {
        negated: bool,
    },
    /// `permanent card`: a card with a permanent card type (CR 110.4b). The
    /// following `card` noun still supplies the object domain.
    PermanentCard,
    /// `with a [KIND] counter on it`, `has three or more KIND counters on it`,
    /// `had no counters on it`: the number of counters on the object, of the
    /// named kind when present, within bounds (CR 122.1).
    HasCounter(CounterPresence),
    /// Has the creature type chosen by an earlier `choose a creature type`
    /// instruction of the same effect program (CR 608.2d). The binding is
    /// symbolic; the program lowering proves its producer.
    ChosenCreatureType,
    /// Has the color chosen by an earlier `choose a color` instruction of the
    /// same effect program (CR 608.2d, 105.1). Symbolic like the chosen type.
    ChosenColor,
    /// A relation to a name, a player or the turn's history.
    Relation(ObjectRelation),
    /// A keyword ability with parameters, a mana-cost symbol class, a
    /// history relative to the source, a set constraint or a superlative.
    Qualifier(ObjectQualifier),
    NumericComparison {
        left: SelectorNumericProperty,
        operator: NumericComparisonOperator,
        right: Box<NumericExpression>,
    },
    Not(Box<ObjectPredicate>),
    Intersection(Vec<ObjectPredicate>),
    Union(Vec<ObjectPredicate>),
}

impl ObjectPredicate {
    /// Whether some atom satisfies `test`, through Boolean composition.
    pub(in crate::oracle::canonical) fn contains_qualifier(
        &self,
        test: &dyn Fn(&ObjectQualifier) -> bool,
    ) -> bool {
        match self {
            Self::Qualifier(qualifier) => test(qualifier),
            Self::Not(operand) => operand.contains_qualifier(test),
            Self::Intersection(operands) | Self::Union(operands) => operands
                .iter()
                .any(|operand| operand.contains_qualifier(test)),
            _ => false,
        }
    }

    /// Whether the predicate states a constraint over the chosen set.
    pub(in crate::oracle::canonical) fn contains_set_constraint(&self) -> bool {
        self.contains_qualifier(&ObjectQualifier::is_set_constraint)
    }

    /// The candidate filter and the set constraints of a query, for a
    /// consumer that selects or counts a set. A set constraint must be a
    /// member of the top-level conjunction: under a union or a negation it
    /// has no set to constrain, and `None` is returned. The remaining
    /// candidate filter keeps its order; it is the whole domain when only
    /// constraints were stated.
    pub(in crate::oracle::canonical) fn split_set_constraints(
        &self,
    ) -> Option<(ObjectPredicate, Vec<ObjectQualifier>)> {
        // The conjunction's members, nested conjunctions flattened: a
        // coordinated `with different names and total mana value 6 or
        // less` is one more conjunction of the same selection.
        fn conjuncts<'a>(predicate: &'a ObjectPredicate, members: &mut Vec<&'a ObjectPredicate>) {
            match predicate {
                ObjectPredicate::Intersection(operands) => {
                    operands
                        .iter()
                        .for_each(|operand| conjuncts(operand, members));
                }
                other => members.push(other),
            }
        }
        let mut members = Vec::new();
        conjuncts(self, &mut members);
        let mut rest = Vec::new();
        let mut constraints = Vec::new();
        for member in members {
            match member {
                Self::Qualifier(qualifier) if qualifier.is_set_constraint() => {
                    constraints.push(qualifier.clone());
                }
                other if other.contains_set_constraint() => return None,
                other => rest.push(other.clone()),
            }
        }
        let rest = match rest.len() {
            0 => return None,
            1 => rest.remove(0),
            _ => Self::Intersection(rest),
        };
        Some((rest, constraints))
    }

    /// Lets a consumer enforce its proved numeric-context boundary without
    /// duplicating the walk over shared Boolean predicate composition.
    pub(in crate::oracle::canonical) fn contains_numeric_comparison(&self) -> bool {
        match self {
            // A comparison of two candidate properties is a numeric
            // comparison too: a consumer without that contract refuses it.
            Self::NumericComparison { .. }
            | Self::Qualifier(ObjectQualifier::CandidatePropertyComparison { .. }) => true,
            Self::Not(operand) => operand.contains_numeric_comparison(),
            Self::Intersection(operands) | Self::Union(operands) => {
                operands.iter().any(Self::contains_numeric_comparison)
            }
            _ => false,
        }
    }

    /// Whether a batch word (`outlaw`, `historic`) occurs anywhere in this
    /// predicate, so that a batch-definition reminder describes it.
    pub(in crate::oracle::canonical) fn contains_characteristic_batch(
        &self,
        expected: CharacteristicBatch,
    ) -> bool {
        match self {
            Self::CharacteristicBatch(batch) => *batch == expected,
            Self::Not(operand) => operand.contains_characteristic_batch(expected),
            Self::Intersection(operands) | Self::Union(operands) => operands
                .iter()
                .any(|operand| operand.contains_characteristic_batch(expected)),
            _ => false,
        }
    }

    /// Returns whether every value accepted by this predicate has the requested
    /// card type. Every union branch must preserve the type, while one
    /// intersection member is enough to constrain the whole intersection.
    pub(in crate::oracle::canonical) fn guarantees_card_type(
        &self,
        expected: ObjectCardType,
    ) -> bool {
        match self {
            Self::CardType(actual) => *actual == expected,
            // CR 205.3d: an object can't have a subtype that corresponds to
            // none of its types. A subtype of exactly one card type (`Forest`,
            // `Equipment`, `Aura`) therefore guarantees that type. Creature
            // subtypes do not: Kindred spells carry them too.
            Self::Subtype(name) => {
                crate::oracle::characteristic_vocabulary::noncreature_subtype_card_type(name)
                    == Some(expected)
            }
            Self::Intersection(operands) => operands
                .iter()
                .any(|operand| operand.guarantees_card_type(expected)),
            Self::Union(operands) => {
                !operands.is_empty()
                    && operands
                        .iter()
                        .all(|operand| operand.guarantees_card_type(expected))
            }
            Self::IsCommander
            | Self::IsToken
            | Self::CharacteristicBatch(_)
            | Self::Color(_)
            | Self::ColorQuantity(_)
            | Self::Supertype(_)
            | Self::ExactManaCost(_)
            | Self::HasKeyword(_)
            | Self::ObjectKind(_)
            | Self::Status(_)
            | Self::Designation(_)
            | Self::OwnedByAbilityController { .. }
            | Self::PermanentCard
            | Self::HasCounter(_)
            | Self::ChosenCreatureType
            | Self::ChosenColor
            | Self::Relation(_)
            | Self::Qualifier(_)
            | Self::NumericComparison { .. }
            | Self::Not(_) => false,
        }
    }

    /// Returns whether every branch can be evaluated over `domain` without
    /// crossing into a different explicit object domain.
    ///
    /// Card types are valid characteristics of cards and spells, while only
    /// permanent card types are valid in the battlefield-permanent domain.
    /// Empty Boolean nodes are rejected because they do not form predicates.
    pub(in crate::oracle::canonical) fn supports_domain(&self, domain: ObjectKind) -> bool {
        match self {
            Self::CardType(card_type) => {
                domain != ObjectKind::Permanent || card_type.is_permanent_type()
            }
            Self::IsCommander
            | Self::IsToken
            | Self::Subtype(_)
            | Self::CharacteristicBatch(_)
            | Self::Color(_)
            | Self::ColorQuantity(_)
            | Self::Supertype(_)
            | Self::ExactManaCost(_)
            | Self::HasKeyword(_)
            | Self::ChosenCreatureType
            | Self::ChosenColor
            | Self::NumericComparison { .. } => true,
            Self::ObjectKind(actual) => *actual == domain,
            Self::Status(status) => status.supports_domain(domain),
            Self::Designation(_) | Self::OwnedByAbilityController { .. } | Self::HasCounter(_) => {
                domain == ObjectKind::Permanent
            }
            Self::Relation(relation) => relation.supports_domain(domain),
            Self::Qualifier(qualifier) => qualifier.supports_domain(domain),
            Self::PermanentCard => domain != ObjectKind::Permanent,
            Self::Not(operand) => operand.supports_domain(domain),
            Self::Intersection(operands) | Self::Union(operands) => {
                !operands.is_empty()
                    && operands
                        .iter()
                        .all(|operand| operand.supports_domain(domain))
            }
        }
    }

    /// Returns whether this predicate establishes `domain`, rather than only
    /// adding a characteristic that could apply within several domains.
    ///
    /// One intersection member may establish the domain. Every union branch
    /// must establish it so no alternative can escape the requested domain.
    /// Permanent card types establish the permanent domain directly; card
    /// types alone do not establish the broader card or spell domains.
    ///
    /// CR 109.2: a subtype head used without `card`, `spell`, `source` or
    /// `scheme` names a permanent (`Goblins`, `target Wall`). An explicit
    /// card or spell noun in the same description (`Elf card`, `Goblin
    /// spells`, `Goblin permanent card`) keeps its own domain. A negated
    /// creature subtype (`non-Elf`) is never a head; a negated permanent card
    /// type or noncreature permanent subtype (`noncreature`, `non-Equipment`)
    /// keeps the anchor it had before this inference, through `Not`.
    pub(in crate::oracle::canonical) fn has_domain(&self, domain: ObjectKind) -> bool {
        self.establishes_domain(domain, DomainReading::INFERRED)
    }

    /// The domain this predicate names without the CR 109.2 subtype-head
    /// inference: for a parent whose own noun (`source`) or coordination
    /// (`Elf or Goblin permanent`) decides how a bare subtype is read.
    pub(in crate::oracle::canonical) fn has_explicit_domain(&self, domain: ObjectKind) -> bool {
        self.establishes_domain(domain, DomainReading::EXPLICIT)
    }

    /// The domain a positive noun of this predicate names (CR 109.2a): the
    /// subtype-head inference applies, but a negated noun (`noncreature`)
    /// never anchors a domain by itself. Target declarations and domain
    /// projections use this reading.
    pub(in crate::oracle::canonical) fn has_positive_domain(&self, domain: ObjectKind) -> bool {
        self.establishes_domain(domain, DomainReading::POSITIVE)
    }

    /// The one object-domain authority (CR 109.2a): the predicate names
    /// `domain` and every branch is evaluable in it. Parents type-check a
    /// recognized noun phrase with this, never with their own domain walk.
    pub(in crate::oracle::canonical) fn is_in_domain(&self, domain: ObjectKind) -> bool {
        self.has_domain(domain) && self.supports_domain(domain)
    }

    /// The domain authority for a noun phrase that may state a control
    /// relation: a nominal head the permanent domain admits (`tokens`,
    /// `commanders`) related by control names permanents (CR 108.4, 110.2).
    pub(in crate::oracle::canonical) fn is_in_domain_when_controlled(
        &self,
        domain: ObjectKind,
        controlled: bool,
    ) -> bool {
        self.is_in_domain(domain)
            || (controlled
                && domain == ObjectKind::Permanent
                && self.supports_domain(domain)
                && self.has_nominal_head())
    }

    /// A declared target in `domain` (CR 115.1, 601.2c): a positive noun
    /// names the domain, every branch is evaluable there, no atom reads a
    /// choice made later during resolution, and no object noun is negated
    /// (a negated object noun has no filter to lower).
    pub(in crate::oracle::canonical) fn is_declarable_target_in(&self, domain: ObjectKind) -> bool {
        self.has_positive_domain(domain)
            && self.supports_domain(domain)
            && !self.reads_resolution_choice()
            && !self.negates_object_kind()
    }

    /// Whether an object-kind noun (`card`, `spell`, `permanent`) occurs
    /// anywhere in this predicate.
    pub(in crate::oracle::canonical) fn names_object_kind(&self) -> bool {
        match self {
            Self::ObjectKind(_) => true,
            Self::Not(operand) => operand.names_object_kind(),
            Self::Intersection(operands) | Self::Union(operands) => {
                operands.iter().any(Self::names_object_kind)
            }
            _ => false,
        }
    }

    /// Whether an object-kind noun occurs under a negation.
    pub(in crate::oracle::canonical) fn negates_object_kind(&self) -> bool {
        match self {
            Self::Not(operand) => operand.names_object_kind(),
            Self::Intersection(operands) | Self::Union(operands) => {
                operands.iter().any(Self::negates_object_kind)
            }
            _ => false,
        }
    }

    /// Whether an atom reads a choice made during resolution (`of the chosen
    /// type`, CR 608.2d), which a declaration-time filter cannot evaluate.
    pub(in crate::oracle::canonical) fn reads_resolution_choice(&self) -> bool {
        match self {
            Self::ChosenCreatureType
            | Self::ChosenColor
            | Self::Relation(ObjectRelation::ChosenName) => true,
            Self::Not(operand) => operand.reads_resolution_choice(),
            Self::Intersection(operands) | Self::Union(operands) => {
                operands.iter().any(Self::reads_resolution_choice)
            }
            _ => false,
        }
    }

    /// The atom-level CR 109.2/205.3d inference shared by every domain
    /// projection: a permanent card type, the `permanent` noun, or a subtype
    /// that only permanents carry when no card/spell noun is present.
    pub(in crate::oracle::canonical) fn denotes_permanent_head(&self, subtype_heads: bool) -> bool {
        match self {
            Self::ObjectKind(kind) => *kind == ObjectKind::Permanent,
            Self::CardType(card_type) => card_type.is_permanent_type(),
            // `seven or more Plains`: a land, artifact, enchantment,
            // planeswalker or battle subtype only exists on that permanent
            // type, whatever surrounds it.
            Self::Subtype(name) => {
                crate::oracle::characteristic_vocabulary::noncreature_subtype_implies_permanent(
                    name,
                ) || (subtype_heads
                    && crate::oracle::characteristic_vocabulary::subtype_denotes_permanent(name))
            }
            _ => false,
        }
    }

    /// An explicit non-permanent object noun (`card`, `spell`, or the
    /// `permanent card` category) inside this conjunction: CR 109.2 then no
    /// longer reads a subtype as a permanent. `source` and `scheme` are
    /// nouns their parents read; they call `has_explicit_domain`.
    fn names_nonpermanent_domain(&self) -> bool {
        match self {
            Self::ObjectKind(kind) => *kind != ObjectKind::Permanent,
            Self::PermanentCard => true,
            Self::Intersection(operands) => operands.iter().any(Self::names_nonpermanent_domain),
            Self::Union(operands) => operands.iter().any(Self::names_nonpermanent_domain),
            _ => false,
        }
    }

    /// The single domain walk shared by every projection above.
    fn establishes_domain(&self, domain: ObjectKind, reading: DomainReading) -> bool {
        match self {
            Self::ObjectKind(actual) => *actual == domain,
            Self::CardType(_) | Self::Subtype(_) => {
                domain == ObjectKind::Permanent
                    && self.denotes_permanent_head(reading.subtype_heads)
            }
            Self::Not(operand) => {
                reading.through_negation
                    && operand.establishes_domain(domain, reading.without_subtype_heads())
            }
            Self::Intersection(operands) => {
                let reading = if operands.iter().any(Self::names_nonpermanent_domain) {
                    reading.without_subtype_heads()
                } else {
                    reading
                };
                operands
                    .iter()
                    .any(|operand| operand.establishes_domain(domain, reading))
            }
            Self::Union(operands) => {
                !operands.is_empty()
                    && operands
                        .iter()
                        .all(|operand| operand.establishes_domain(domain, reading))
            }
            Self::IsCommander
            | Self::IsToken
            | Self::CharacteristicBatch(_)
            | Self::Color(_)
            | Self::ColorQuantity(_)
            | Self::Supertype(_)
            | Self::ExactManaCost(_)
            | Self::HasKeyword(_)
            | Self::Status(_)
            | Self::Designation(_)
            | Self::OwnedByAbilityController { .. }
            | Self::PermanentCard
            | Self::HasCounter(_)
            | Self::ChosenCreatureType
            | Self::ChosenColor
            | Self::Relation(_)
            | Self::Qualifier(_)
            | Self::NumericComparison { .. } => false,
        }
    }

    /// Returns whether the predicate contains a noun that can identify an
    /// object, independently of the domain supplied by its parent grammar.
    ///
    /// Entry events supply the battlefield-permanent domain themselves, so a
    /// real subtype such as `Beast` is a sufficient noun head. Adjectival
    /// characteristics such as `green` or `legendary` are not. Intersections
    /// need one noun-bearing operand, while every union branch needs its own
    /// noun so no alternative degenerates into a bare adjective.
    pub(in crate::oracle::canonical) fn has_nominal_head(&self) -> bool {
        match self {
            Self::IsCommander
            | Self::IsToken
            | Self::CardType(_)
            | Self::Subtype(_)
            | Self::ObjectKind(_) => true,
            Self::Not(operand) => operand.has_nominal_head(),
            Self::Intersection(operands) => {
                !operands.is_empty() && operands.iter().any(Self::has_nominal_head)
            }
            Self::Union(operands) => {
                !operands.is_empty() && operands.iter().all(Self::has_nominal_head)
            }
            Self::CharacteristicBatch(_)
            | Self::Color(_)
            | Self::ColorQuantity(_)
            | Self::Supertype(_)
            | Self::ExactManaCost(_)
            | Self::HasKeyword(_)
            | Self::Status(_)
            | Self::Designation(_)
            | Self::OwnedByAbilityController { .. }
            | Self::PermanentCard
            | Self::HasCounter(_)
            | Self::ChosenCreatureType
            | Self::ChosenColor
            | Self::Relation(_)
            | Self::Qualifier(_)
            | Self::NumericComparison { .. } => false,
        }
    }
}

/// One nominal type head an object query names positively: the `permanent`
/// noun, a card type, or a subtype. Heads are ordered by the type hierarchy
/// (CR 205.3d: a subtype corresponds to its card type; every permanent type
/// is a kind of permanent, CR 110.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectTypeHead {
    Permanent,
    CardType(ObjectCardType),
    Subtype(String),
}

impl ObjectTypeHead {
    /// A head from its IR spelling (`cardTypeContains` / `subtypeContains`
    /// values). An unknown card type spelling names no head.
    pub(in crate::oracle::canonical) fn card_type_from_ir(name: &str) -> Option<Self> {
        crate::oracle::characteristic_vocabulary::ORACLE_CARD_TYPES
            .iter()
            .find(|card_type| card_type.ir_name().eq_ignore_ascii_case(name))
            .map(|card_type| Self::CardType(*card_type))
    }

    /// The heads this head implies, itself included: a land, artifact,
    /// enchantment, planeswalker or battle subtype implies its one card type
    /// (CR 205.3d), a creature subtype names a creature head (CR 205.3m;
    /// a Kindred permanent also carries it, CR 308.2, so it never proves the
    /// creature type for a characteristic read), and a permanent card type
    /// implies `permanent`.
    fn lineage(&self) -> Vec<Self> {
        let mut lineage = vec![self.clone()];
        let card_type = match self {
            Self::Permanent => None,
            Self::CardType(card_type) => Some(*card_type),
            Self::Subtype(name) => {
                crate::oracle::characteristic_vocabulary::noncreature_subtype_card_type(name)
                    .or_else(|| {
                        crate::oracle::characteristic_vocabulary::lookup_creature_subtype(
                            name,
                            crate::oracle::characteristic_vocabulary::AcceptedForms::SingularOnly,
                        )
                        .map(|_| ObjectCardType::Creature)
                    })
            }
        };
        if let Some(card_type) = card_type {
            if !matches!(self, Self::CardType(_)) {
                lineage.push(Self::CardType(card_type));
            }
            if card_type.is_permanent_type() {
                lineage.push(Self::Permanent);
            }
        } else if let Self::Subtype(name) = self
            && crate::oracle::characteristic_vocabulary::noncreature_subtype_implies_permanent(name)
        {
            lineage.push(Self::Permanent);
        }
        lineage
    }

    /// Whether one head is a kind of the other: `Dinosaur` and `creature`,
    /// `Equipment` and `artifact`, `Aura` and `enchantment`, `permanent` and
    /// any permanent type. Two unrelated types (`artifact`, `creature`) do
    /// not overlap as heads even though one object can have both.
    pub(in crate::oracle::canonical) fn overlaps(&self, other: &Self) -> bool {
        self.is_kind_of(other) || other.is_kind_of(self)
    }

    /// Whether every object with this head has `other` too (`Saga` is a
    /// kind of enchantment and of permanent, not of creature).
    pub(in crate::oracle::canonical) fn is_kind_of(&self, other: &Self) -> bool {
        self.lineage().contains(other)
    }
}

/// Whether an object described by `object_heads` (its printed types and
/// subtypes) is named by a slot whose heads are `slot_heads`: one of its
/// heads is a kind of one of the slot's heads.
pub(in crate::oracle::canonical) fn type_heads_name(
    object_heads: &[ObjectTypeHead],
    slot_heads: &[ObjectTypeHead],
) -> bool {
    slot_heads
        .iter()
        .any(|slot| object_heads.iter().any(|head| head.is_kind_of(slot)))
}

/// Whether two head sets name a common kind: some head of one is a kind of
/// some head of the other. An empty set names no head and overlaps nothing.
pub(in crate::oracle::canonical) fn type_heads_overlap(
    left: &[ObjectTypeHead],
    right: &[ObjectTypeHead],
) -> bool {
    left.iter()
        .any(|head| right.iter().any(|other| head.overlaps(other)))
}

impl ObjectPredicate {
    /// Whether every value this predicate accepts has a creature subtype
    /// head (`Dinosaur`, `target Dinosaur you control`): such a permanent is
    /// a creature or a Kindred permanent (CR 205.3m, 308.2), so its power
    /// and toughness read the creature's value or the undefined value of a
    /// noncreature permanent (CR 208.3), never another characteristic.
    pub(in crate::oracle::canonical) fn has_creature_subtype_head(&self) -> bool {
        match self {
            Self::Subtype(name) => {
                crate::oracle::characteristic_vocabulary::lookup_creature_subtype(
                    name,
                    crate::oracle::characteristic_vocabulary::AcceptedForms::SingularOnly,
                )
                .is_some()
            }
            Self::Intersection(operands) => operands.iter().any(Self::has_creature_subtype_head),
            Self::Union(operands) => {
                !operands.is_empty() && operands.iter().all(Self::has_creature_subtype_head)
            }
            _ => false,
        }
    }

    /// The type heads this predicate names positively: card types, subtypes
    /// and the `permanent` noun read through Boolean composition, never
    /// under a negation (`nonartifact` names no artifact head) and never
    /// from a numeric comparison's operand.
    pub(in crate::oracle::canonical) fn positive_type_heads(&self) -> Vec<ObjectTypeHead> {
        match self {
            Self::CardType(card_type) => vec![ObjectTypeHead::CardType(*card_type)],
            Self::Subtype(name) => vec![ObjectTypeHead::Subtype(name.clone())],
            Self::ObjectKind(ObjectKind::Permanent) => vec![ObjectTypeHead::Permanent],
            Self::Intersection(operands) | Self::Union(operands) => operands
                .iter()
                .flat_map(Self::positive_type_heads)
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// How the one domain walk reads nouns (CR 109.2a).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DomainReading {
    /// A subtype that only permanents carry names a permanent.
    subtype_heads: bool,
    /// A negated permanent noun keeps the anchor it had before inference.
    through_negation: bool,
}

impl DomainReading {
    const INFERRED: Self = Self {
        subtype_heads: true,
        through_negation: true,
    };
    const EXPLICIT: Self = Self {
        subtype_heads: false,
        through_negation: true,
    };
    const POSITIVE: Self = Self {
        subtype_heads: true,
        through_negation: false,
    };

    const fn without_subtype_heads(self) -> Self {
        Self {
            subtype_heads: false,
            through_negation: self.through_negation,
        }
    }
}

/// Transitional name retained while the lexical selector automata migrate in
/// a separate change. Both names denote the same semantic AST.
pub(in crate::oracle::canonical) use ObjectPredicate as SelectorPredicate;

#[cfg(test)]
#[path = "object_predicate_tests.rs"]
mod tests;
