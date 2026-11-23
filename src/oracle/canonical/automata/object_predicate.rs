use crate::oracle::counter_name::CounterName;

use super::{
    CardColor, CardSupertype, CharacteristicBatch, ColorQuantity, GrantedKeyword, ManaCostNode,
    NumericComparisonOperator, NumericExpression, ObjectCardType, ObjectKind, PermanentDesignation,
    SelectorNumericProperty,
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
    /// This-turn history of the object, read by a past-tense relative
    /// clause (`that entered this turn`, `that was dealt damage this turn`).
    /// They have no adjective surface.
    EnteredThisTurn,
    AttackedThisTurn,
    BlockedThisTurn,
    WasDealtDamageThisTurn,
    DealtDamageThisTurn,
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
            Self::EnteredThisTurn => ("enteredThisTurn", false),
            Self::AttackedThisTurn => ("attackedThisTurn", false),
            Self::BlockedThisTurn => ("blockedThisTurn", false),
            Self::WasDealtDamageThisTurn => ("wasDealtDamageThisTurn", false),
            Self::DealtDamageThisTurn => ("dealtDamageThisTurn", false),
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
    /// Lets a consumer enforce its proved numeric-context boundary without
    /// duplicating the walk over shared Boolean predicate composition.
    pub(in crate::oracle::canonical) fn contains_numeric_comparison(&self) -> bool {
        match self {
            Self::NumericComparison { .. } => true,
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
            | Self::NumericComparison { .. } => true,
            Self::ObjectKind(actual) => *actual == domain,
            Self::Status(_)
            | Self::Designation(_)
            | Self::OwnedByAbilityController { .. }
            | Self::HasCounter(_) => domain == ObjectKind::Permanent,
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

    /// The first of a parent's admissible domains this predicate is in.
    pub(in crate::oracle::canonical) fn inferred_domain(
        &self,
        admissible: &[ObjectKind],
    ) -> Option<ObjectKind> {
        admissible
            .iter()
            .copied()
            .find(|domain| self.is_in_domain(*domain))
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
            Self::ChosenCreatureType => true,
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
            | Self::NumericComparison { .. } => false,
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
