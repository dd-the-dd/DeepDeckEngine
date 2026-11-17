//! Shared target referents: lexical nominal evidence survives until a parent
//! validates it against the original declared target selector.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ObjectCardType, ObjectKind, ObjectPredicate, TargetBinding, TargetSelector,
    project_object_query_domains, recognize_anaphoric_target_reference,
};
use crate::oracle::characteristic_vocabulary::{
    CharacteristicLexeme, lookup_object_selector_lexeme,
};
use crate::oracle::model::RecognizedEntity;

/// The registered possessive nominal grammar currently yields exactly these
/// two Copy atoms. Numeric ASTs may convert them into ObjectPredicate without
/// changing their historical representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TargetNominal {
    CardType(ObjectCardType),
    Domain(ObjectKind),
}

#[cfg(test)]
#[path = "typed_target_reference_tests.rs"]
mod tests;

impl TargetNominal {
    pub(in crate::oracle::canonical) fn as_predicate(self) -> ObjectPredicate {
        match self {
            Self::CardType(value) => ObjectPredicate::CardType(value),
            Self::Domain(value) => ObjectPredicate::ObjectKind(value),
        }
    }
}

/// Qualified actor references need a parent that validates their nominal and
/// snapshots the controller at the correct semantic boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TargetReferenceSupport {
    #[default]
    Unqualified,
    TypedNominal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TypedTargetReference {
    pub binding: TargetBinding,
    pub required_nominal: Option<TargetNominal>,
}

pub(in crate::oracle::canonical) fn recognize_typed_target_reference(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
    nominal_state: &'static str,
) -> MachineResult<TypedTargetReference> {
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("its"))
    {
        return recognize_anaphoric_target_reference(cursor, antecedent).map(|binding| {
            TypedTargetReference {
                binding,
                required_nominal: None,
            }
        });
    }
    cursor.consume("that")?;
    cursor.enter(nominal_state);
    let object = cursor.consume_type("possessiveTypeWord")?;
    let normalized = object.raw.to_ascii_lowercase();
    let surface = normalized
        .strip_suffix("'s")
        .or_else(|| normalized.strip_suffix("\u{2019}s"))
        .ok_or_else(|| cursor.fail("a possessive registered object nominal"))?;
    let nominal = match lookup_object_selector_lexeme(surface) {
        Some(CharacteristicLexeme::CardType(value)) => TargetNominal::CardType(value),
        Some(CharacteristicLexeme::ObjectDomain(value)) => TargetNominal::Domain(value),
        _ => {
            return Err(
                cursor.fail("a registered card type or object domain for the target reference")
            );
        }
    };
    let binding = antecedent
        .ok_or_else(|| cursor.fail("a prior target binding for the qualified object reference"))?;
    Ok(TypedTargetReference {
        binding,
        required_nominal: Some(nominal),
    })
}

/// A bounded lexical head hint only. The actual reference child still resolves
/// the binding and preserves the nominal; this cannot establish semantics.
pub(in crate::oracle::canonical) fn controller_reference_head_length(
    tokens: &[RecognizedEntity],
) -> Option<usize> {
    let raw = |index: usize| tokens.get(index).map(|token| token.raw.as_str());
    if raw(0).is_some_and(|value| value.eq_ignore_ascii_case("its"))
        && raw(1).is_some_and(|value| value.eq_ignore_ascii_case("controller"))
    {
        return Some(2);
    }
    if raw(0).is_some_and(|value| value.eq_ignore_ascii_case("that"))
        && tokens
            .get(1)
            .is_some_and(|token| token.token_type == "possessiveTypeWord")
        && raw(2).is_some_and(|value| value.eq_ignore_ascii_case("controller"))
    {
        return Some(3);
    }
    None
}

fn object_implies_nominal(actual: &ObjectPredicate, required: &ObjectPredicate) -> bool {
    if actual == required {
        return true;
    }
    match required {
        ObjectPredicate::ObjectKind(domain) => {
            project_object_query_domains(actual, &[*domain]).is_ok()
        }
        ObjectPredicate::CardType(value) => actual.guarantees_card_type(*value),
        _ => false,
    }
}

pub(in crate::oracle::canonical) fn selector_implies_nominal(
    selector: &TargetSelector,
    required: &ObjectPredicate,
) -> bool {
    match selector {
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            object_implies_nominal(predicate, required)
        }
        TargetSelector::Union(branches) => {
            !branches.is_empty()
                && branches
                    .iter()
                    .all(|branch| selector_implies_nominal(branch, required))
        }
        TargetSelector::Any | TargetSelector::Player(_) | TargetSelector::StackItem(_) => false,
    }
}
