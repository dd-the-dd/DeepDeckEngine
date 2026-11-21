//! Shared type-change characteristics (CR 205.1a/b, 205.4, 305.7; layer 4 per
//! CR 613.1d):
//!
//! ```text
//! TypeChange := Supertype
//!             | "every" "basic" "land" "type" Retention?
//!             | ["a" | "an"] Subtype* CardType* Retention?
//! Retention  := "in" "addition" "to" ("its" | "their") "other" ["land" | "creature"] "types"
//! ```
//!
//! The same child serves every copula or verb parent (`is`, `are`, `becomes`).
//! The parent owns the subject, its grammatical number, the duration and the
//! lowering; this child only returns typed characteristics. A bare change
//! without the retention clause replaces the subtypes of its class (CR
//! 205.1b, 305.7); that meaning is carried as data, never re-inferred.
use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, BasicLandSubtype, lookup_basic_land_subtype, lookup_creature_subtype_tokens,
    lookup_permanent_card_type, lookup_supertype,
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{CardSupertype, CreatureSubtype, ObjectCardType, TypeChangeRetention};

/// Grammatical number of the subject that owns the copula. A singular
/// subject takes an article and singular nouns; a plural subject takes bare
/// plural nouns (prenominal subtypes such as `Forest lands` stay singular).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TypeChangeNumber {
    Singular,
    Plural,
}

/// One subtype named by a type change, typed by its subtype class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TypeChangeSubtype {
    Creature(CreatureSubtype),
    BasicLand(BasicLandSubtype),
}

impl TypeChangeSubtype {
    pub(in crate::oracle::canonical) const fn ir_name(self) -> &'static str {
        match self {
            Self::Creature(subtype) => subtype.ir_name(),
            Self::BasicLand(subtype) => subtype.ir_name(),
        }
    }
}

/// The subtype class named by `its other land types` (CR 205.3i/m).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum RetainedTypeClass {
    AllTypes,
    LandTypes,
    CreatureTypes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TypeChangeCharacteristicsNode {
    pub supertypes: Vec<CardSupertype>,
    pub subtypes: Vec<TypeChangeSubtype>,
    pub card_types: Vec<ObjectCardType>,
    /// `every basic land type` (CR 305.6 lists the five basic land types).
    pub all_basic_land_types: bool,
    pub retention: TypeChangeRetention,
}

impl TypeChangeCharacteristicsNode {
    /// A nominal change names at least one type noun, not only an adjective.
    pub(in crate::oracle::canonical) fn is_nominal(&self) -> bool {
        !self.subtypes.is_empty() || !self.card_types.is_empty() || self.all_basic_land_types
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TypeChangeState {
    Start,
    EveryBasicLandType,
    Supertype,
    Types,
    Retention,
    Complete,
}

pub(in crate::oracle::canonical) fn type_change_characteristics_start(
    cursor: &OracleTokenCursor<'_>,
    number: TypeChangeNumber,
) -> bool {
    let Some(first) = cursor.peek() else {
        return false;
    };
    let lowered = first.to_ascii_lowercase();
    match lowered.as_str() {
        "every" => cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("basic")),
        "a" | "an" => number == TypeChangeNumber::Singular,
        _ => {
            lookup_supertype(first, AcceptedForms::SingularOnly).is_some()
                || type_word_at(cursor, 0, forms_for(number)).is_some()
        }
    }
}

const fn forms_for(number: TypeChangeNumber) -> AcceptedForms {
    match number {
        TypeChangeNumber::Singular => AcceptedForms::SingularOnly,
        TypeChangeNumber::Plural => AcceptedForms::SingularOrPlural,
    }
}

enum TypeWord {
    Subtype(TypeChangeSubtype, usize),
    CardType(ObjectCardType),
}

fn type_word_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
    forms: AcceptedForms,
) -> Option<TypeWord> {
    let first = cursor.peek_ahead(offset)?;
    if let Some(subtype) = lookup_basic_land_subtype(first, forms) {
        return Some(TypeWord::Subtype(TypeChangeSubtype::BasicLand(subtype), 1));
    }
    if let Some(card_type) = lookup_permanent_card_type(first, forms) {
        return Some(TypeWord::CardType(card_type));
    }
    lookup_creature_subtype_tokens(first, cursor.peek_ahead(offset + 1), forms)
        .map(|(subtype, width)| TypeWord::Subtype(TypeChangeSubtype::Creature(subtype), width))
}

pub(in crate::oracle::canonical) fn recognize_type_change_characteristics(
    cursor: &mut OracleTokenCursor<'_>,
    number: TypeChangeNumber,
) -> MachineResult<TypeChangeCharacteristicsNode> {
    let forms = forms_for(number);
    let mut state = TypeChangeState::Start;
    let mut node = TypeChangeCharacteristicsNode {
        supertypes: Vec::new(),
        subtypes: Vec::new(),
        card_types: Vec::new(),
        all_basic_land_types: false,
        retention: TypeChangeRetention::ReplacesSameClass,
    };
    loop {
        state = match state {
            TypeChangeState::Start => {
                cursor.enter("type-change.start");
                let first = cursor
                    .peek()
                    .map(str::to_ascii_lowercase)
                    .ok_or_else(|| cursor.fail("a type-change characteristic"))?;
                match first.as_str() {
                    "every" => TypeChangeState::EveryBasicLandType,
                    "a" | "an" => {
                        cursor.require(
                            number == TypeChangeNumber::Singular,
                            "an article only after a singular subject",
                        )?;
                        cursor.consume(&first)?;
                        TypeChangeState::Types
                    }
                    _ if lookup_supertype(&first, AcceptedForms::SingularOnly).is_some() => {
                        TypeChangeState::Supertype
                    }
                    _ => {
                        cursor.require(
                            number == TypeChangeNumber::Plural,
                            "`a` or `an` before the types of a singular subject",
                        )?;
                        TypeChangeState::Types
                    }
                }
            }
            TypeChangeState::EveryBasicLandType => {
                cursor.enter("type-change.every-basic-land-type");
                cursor.consume_phrase(&["every", "basic", "land", "type"])?;
                node.all_basic_land_types = true;
                TypeChangeState::Retention
            }
            TypeChangeState::Supertype => {
                cursor.enter("type-change.supertype");
                let raw = cursor.peek().expect("checked supertype").to_string();
                let supertype = lookup_supertype(&raw, AcceptedForms::SingularOnly)
                    .ok_or_else(|| cursor.fail("a registered supertype"))?;
                cursor.consume(&raw)?;
                node.supertypes.push(supertype);
                TypeChangeState::Retention
            }
            TypeChangeState::Types => {
                cursor.enter("type-change.types");
                while let Some(word) = type_word_at(cursor, 0, forms) {
                    match word {
                        TypeWord::Subtype(subtype, width) => {
                            // Subtypes precede the card types they modify.
                            cursor.require(
                                node.card_types.is_empty(),
                                "subtypes before the card types they modify",
                            )?;
                            if matches!(subtype, TypeChangeSubtype::Creature(_)) {
                                cursor.require(
                                    cursor
                                        .token_slice(cursor.position(), cursor.position() + width)
                                        .iter()
                                        .all(|token| token.token_type == "subtypeWord"),
                                    "lexical subtypeWord evidence from CR 205.3m",
                                )?;
                            }
                            cursor
                                .require(!node.subtypes.contains(&subtype), "distinct subtypes")?;
                            node.subtypes.push(subtype);
                            cursor.advance_to(cursor.position() + width)?;
                        }
                        TypeWord::CardType(card_type) => {
                            cursor.require(
                                !node.card_types.contains(&card_type),
                                "distinct card types",
                            )?;
                            node.card_types.push(card_type);
                            cursor.advance_to(cursor.position() + 1)?;
                        }
                    }
                }
                cursor.require(
                    node.is_nominal(),
                    "a registered subtype or permanent card type",
                )?;
                TypeChangeState::Retention
            }
            TypeChangeState::Retention => {
                cursor.enter("type-change.retention");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("in"))
                {
                    cursor.consume_phrase(&["in", "addition", "to"])?;
                    cursor.enter("type-change.retention.possessive");
                    let possessive = match number {
                        TypeChangeNumber::Singular => "its",
                        TypeChangeNumber::Plural => "their",
                    };
                    cursor.consume(possessive)?;
                    cursor.consume("other")?;
                    cursor.enter("type-change.retention.class");
                    let class = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                        Some("land") => {
                            cursor.consume("land")?;
                            RetainedTypeClass::LandTypes
                        }
                        Some("creature") => {
                            cursor.consume("creature")?;
                            RetainedTypeClass::CreatureTypes
                        }
                        _ => RetainedTypeClass::AllTypes,
                    };
                    cursor.consume("types")?;
                    cursor.require(
                        retained_class_matches(&node, class),
                        "a retained type class that contains every added type",
                    )?;
                    node.retention = TypeChangeRetention::InAdditionToOtherTypes;
                }
                TypeChangeState::Complete
            }
            TypeChangeState::Complete => {
                cursor.enter("type-change.complete");
                return Ok(node);
            }
        };
    }
}

/// `its other land types` retains only land subtypes; every added type must
/// then be a land subtype of that class (CR 205.3i).
fn retained_class_matches(node: &TypeChangeCharacteristicsNode, class: RetainedTypeClass) -> bool {
    match class {
        RetainedTypeClass::AllTypes => true,
        RetainedTypeClass::LandTypes => {
            node.supertypes.is_empty()
                && node.card_types.is_empty()
                && (node.all_basic_land_types || !node.subtypes.is_empty())
                && node
                    .subtypes
                    .iter()
                    .all(|subtype| matches!(subtype, TypeChangeSubtype::BasicLand(_)))
        }
        RetainedTypeClass::CreatureTypes => {
            node.supertypes.is_empty()
                && node.card_types.is_empty()
                && !node.all_basic_land_types
                && !node.subtypes.is_empty()
                && node
                    .subtypes
                    .iter()
                    .all(|subtype| matches!(subtype, TypeChangeSubtype::Creature(_)))
        }
    }
}

#[cfg(test)]
#[path = "type_change_tests.rs"]
mod tests;
