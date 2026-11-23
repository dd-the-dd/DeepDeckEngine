//! Shared target referents: lexical nominal evidence survives until a parent
//! validates it against the original declared target selector.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ObjectCardType, ObjectKind, ObjectPlayerRelation, ObjectPredicate, TargetBinding,
    TargetSelector, object_related_player_head, possessive_nominal, project_object_query_domains,
    recognize_anaphoric_target_reference,
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
    // The genitive nominal is the binding environment's shared reader.
    let nominal = possessive_nominal(&object.raw).ok_or_else(|| {
        cursor.fail("a registered card type or object domain for the target reference")
    })?;
    let binding = antecedent
        .ok_or_else(|| cursor.fail("a prior target binding for the qualified object reference"))?;
    Ok(TypedTargetReference {
        binding,
        required_nominal: Some(nominal),
    })
}

/// A bounded lexical head hint only: the controller form of the binding
/// environment's object-related player head. The actual reference child
/// still resolves the binding; this cannot establish semantics.
pub(in crate::oracle::canonical) fn controller_reference_head_length(
    tokens: &[RecognizedEntity],
) -> Option<usize> {
    object_related_player_head(tokens)
        .filter(|(_, relation)| *relation == ObjectPlayerRelation::Controller)
        .map(|(length, _)| length)
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
