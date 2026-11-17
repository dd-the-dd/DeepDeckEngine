use super::{
    CardColor, CardSupertype, CharacteristicBatch, GrantedKeyword, ManaCostNode,
    NumericComparisonOperator, NumericExpression, ObjectCardType, ObjectKind,
    SelectorNumericProperty,
};

#[cfg(test)]
use super::NumericAtom;

/// A battlefield status adjective (`tapped`, `attacking`, `blocking`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectStatus {
    Tapped,
    Attacking,
    Blocking,
}

impl ObjectStatus {
    pub(in crate::oracle::canonical) fn from_adjective(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "tapped" => Some(Self::Tapped),
            "attacking" => Some(Self::Attacking),
            "blocking" => Some(Self::Blocking),
            _ => None,
        }
    }

    pub(in crate::oracle::canonical) const fn ir_kind(self) -> &'static str {
        match self {
            Self::Tapped => "isTapped",
            Self::Attacking => "isAttacking",
            Self::Blocking => "isBlocking",
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
    Supertype(CardSupertype),
    ExactManaCost(ManaCostNode),
    HasKeyword(GrantedKeyword),
    ObjectKind(ObjectKind),
    Status(ObjectStatus),
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

    /// Returns whether every value accepted by this predicate has the requested
    /// card type. Every union branch must preserve the type, while one
    /// intersection member is enough to constrain the whole intersection.
    pub(in crate::oracle::canonical) fn guarantees_card_type(
        &self,
        expected: ObjectCardType,
    ) -> bool {
        match self {
            Self::CardType(actual) => *actual == expected,
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
            | Self::Subtype(_)
            | Self::CharacteristicBatch(_)
            | Self::Color(_)
            | Self::Supertype(_)
            | Self::ExactManaCost(_)
            | Self::HasKeyword(_)
            | Self::ObjectKind(_)
            | Self::Status(_)
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
            | Self::Supertype(_)
            | Self::ExactManaCost(_)
            | Self::HasKeyword(_)
            | Self::ChosenCreatureType
            | Self::NumericComparison { .. } => true,
            Self::ObjectKind(actual) => *actual == domain,
            Self::Status(_) => domain == ObjectKind::Permanent,
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
    pub(in crate::oracle::canonical) fn has_domain(&self, domain: ObjectKind) -> bool {
        match self {
            Self::ObjectKind(actual) => *actual == domain,
            Self::CardType(card_type) => {
                domain == ObjectKind::Permanent && card_type.is_permanent_type()
            }
            Self::Not(operand) => operand.has_domain(domain),
            Self::Intersection(operands) => {
                operands.iter().any(|operand| operand.has_domain(domain))
            }
            Self::Union(operands) => {
                !operands.is_empty() && operands.iter().all(|operand| operand.has_domain(domain))
            }
            Self::IsCommander
            | Self::IsToken
            | Self::Subtype(_)
            | Self::CharacteristicBatch(_)
            | Self::Color(_)
            | Self::Supertype(_)
            | Self::ExactManaCost(_)
            | Self::HasKeyword(_)
            | Self::Status(_)
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
            | Self::Supertype(_)
            | Self::ExactManaCost(_)
            | Self::HasKeyword(_)
            | Self::Status(_)
            | Self::ChosenCreatureType
            | Self::NumericComparison { .. } => false,
        }
    }
}

/// Transitional name retained while the lexical selector automata migrate in
/// a separate change. Both names denote the same semantic AST.
pub(in crate::oracle::canonical) use ObjectPredicate as SelectorPredicate;

#[cfg(test)]
#[path = "object_predicate_tests.rs"]
mod tests;
