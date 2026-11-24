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
            Self::Related(related) => related.supports_domain(domain),
        }
    }

    /// Whether the relation reads the permanent's state or history rather
    /// than a characteristic of its definition.
    pub(in crate::oracle::canonical) fn reads_object_state(&self) -> bool {
        matches!(
            self,
            Self::DealtDamageToPlayerThisTurn { .. } | Self::Targets(_) | Self::Related(_)
        )
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
            | Self::ChosenColor
            | Self::Relation(_)
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
            Self::Status(_)
            | Self::Designation(_)
            | Self::OwnedByAbilityController { .. }
            | Self::HasCounter(_) => domain == ObjectKind::Permanent,
            Self::Relation(relation) => relation.supports_domain(domain),
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
