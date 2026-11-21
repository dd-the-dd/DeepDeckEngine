//! Bounded semantic domain projection of a shared ObjectQuery predicate.
//! Characteristic predicates never establish a domain through negation.
use super::{ObjectKind, ObjectPredicate, TargetSelector};

const MAX_DOMAIN_ALTERNATIVES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TargetDomainProjectionError {
    EmptyBoolean,
    TooManyAlternatives,
    UnsupportedDomain,
    MissingPositiveNominal,
    ConflictingDomains,
    UnprovedNumericStackFilter,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ObjectDomainProjection {
    pub domain: ObjectKind,
    pub predicate: ObjectPredicate,
}

/// Distribute positive conjunctions/disjunctions without throwing away a branch.
/// Every complete alternative must establish one of the caller's domains.
pub(in crate::oracle::canonical) fn project_object_query_domains(
    predicate: &ObjectPredicate,
    allowed: &[ObjectKind],
) -> Result<Vec<ObjectDomainProjection>, TargetDomainProjectionError> {
    validate_domain_atoms(predicate, allowed)?;
    // Preserve same-domain Boolean characteristics as one AST. In particular,
    // an ordinary spell with many color/type alternatives needs no DNF work.
    for domain in allowed {
        if predicate.supports_domain(*domain) && positive_nominal(predicate, *domain) {
            return Ok(vec![ObjectDomainProjection {
                domain: *domain,
                predicate: predicate.clone(),
            }]);
        }
    }
    let branches = predicate_branches(predicate)?;
    let mut groups: Vec<(ObjectKind, Vec<ObjectPredicate>)> = Vec::new();
    for branch in branches {
        let mut explicit = None;
        let mut permanent_nominal = false;
        for atom in &branch {
            match atom {
                ObjectPredicate::ObjectKind(domain) => {
                    if !allowed.contains(domain) {
                        return Err(TargetDomainProjectionError::UnsupportedDomain);
                    }
                    if explicit.is_some_and(|previous| previous != *domain) {
                        return Err(TargetDomainProjectionError::ConflictingDomains);
                    }
                    explicit = Some(*domain);
                }
                ObjectPredicate::CardType(kind) if kind.is_permanent_type() => {
                    permanent_nominal = true
                }
                ObjectPredicate::Not(child) if has_explicit_domain(child) => {
                    return Err(TargetDomainProjectionError::UnsupportedDomain);
                }
                _ => {}
            }
        }
        let domain = explicit
            .or(permanent_nominal.then_some(ObjectKind::Permanent))
            .ok_or(TargetDomainProjectionError::MissingPositiveNominal)?;
        if !allowed.contains(&domain) {
            return Err(TargetDomainProjectionError::UnsupportedDomain);
        }
        if domain == ObjectKind::Permanent
            && branch.iter().any(
                |atom| matches!(atom, ObjectPredicate::CardType(kind) if !kind.is_permanent_type()),
            )
        {
            return Err(TargetDomainProjectionError::UnsupportedDomain);
        }
        let predicate = if branch.len() == 1 {
            branch.into_iter().next().unwrap()
        } else {
            ObjectPredicate::Intersection(branch)
        };
        if let Some((_, predicates)) = groups
            .iter_mut()
            .find(|(candidate, _)| *candidate == domain)
        {
            predicates.push(predicate);
        } else {
            groups.push((domain, vec![predicate]));
        }
    }
    Ok(groups
        .into_iter()
        .map(|(domain, mut predicates)| ObjectDomainProjection {
            domain,
            predicate: if predicates.len() == 1 {
                predicates.pop().unwrap()
            } else {
                ObjectPredicate::Union(predicates)
            },
        })
        .collect())
}

fn validate_domain_atoms(
    predicate: &ObjectPredicate,
    allowed: &[ObjectKind],
) -> Result<(), TargetDomainProjectionError> {
    match predicate {
        ObjectPredicate::ObjectKind(domain) if !allowed.contains(domain) => {
            Err(TargetDomainProjectionError::UnsupportedDomain)
        }
        ObjectPredicate::Not(child) if has_explicit_domain(child) => {
            Err(TargetDomainProjectionError::UnsupportedDomain)
        }
        ObjectPredicate::Not(child) => validate_domain_atoms(child, allowed),
        ObjectPredicate::Union(children) | ObjectPredicate::Intersection(children) => {
            if children.is_empty() {
                return Err(TargetDomainProjectionError::EmptyBoolean);
            }
            for child in children {
                validate_domain_atoms(child, allowed)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn positive_nominal(predicate: &ObjectPredicate, domain: ObjectKind) -> bool {
    match predicate {
        ObjectPredicate::ObjectKind(actual) => *actual == domain,
        ObjectPredicate::CardType(kind) => {
            domain == ObjectKind::Permanent && kind.is_permanent_type()
        }
        ObjectPredicate::Intersection(children) => {
            children.iter().any(|child| positive_nominal(child, domain))
        }
        ObjectPredicate::Union(children) => {
            !children.is_empty() && children.iter().all(|child| positive_nominal(child, domain))
        }
        _ => false,
    }
}

fn predicate_branches(
    predicate: &ObjectPredicate,
) -> Result<Vec<Vec<ObjectPredicate>>, TargetDomainProjectionError> {
    match predicate {
        ObjectPredicate::Union(operands) => {
            if operands.is_empty() {
                return Err(TargetDomainProjectionError::EmptyBoolean);
            }
            let mut result = Vec::new();
            for operand in operands {
                result.extend(predicate_branches(operand)?);
                if result.len() > MAX_DOMAIN_ALTERNATIVES {
                    return Err(TargetDomainProjectionError::TooManyAlternatives);
                }
            }
            Ok(result)
        }
        ObjectPredicate::Intersection(operands) => {
            if operands.is_empty() {
                return Err(TargetDomainProjectionError::EmptyBoolean);
            }
            let mut result = vec![Vec::new()];
            for operand in operands {
                let child = predicate_branches(operand)?;
                if result.len().saturating_mul(child.len()) > MAX_DOMAIN_ALTERNATIVES {
                    return Err(TargetDomainProjectionError::TooManyAlternatives);
                }
                result = result
                    .into_iter()
                    .flat_map(|prefix| {
                        child.iter().map(move |suffix| {
                            let mut branch = prefix.clone();
                            branch.extend(suffix.iter().cloned());
                            branch
                        })
                    })
                    .collect();
            }
            Ok(result)
        }
        atom => Ok(vec![vec![atom.clone()]]),
    }
}

fn has_explicit_domain(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::ObjectKind(_) => true,
        ObjectPredicate::Not(child) => has_explicit_domain(child),
        ObjectPredicate::Union(children) | ObjectPredicate::Intersection(children) => {
            children.iter().any(has_explicit_domain)
        }
        _ => false,
    }
}

pub(in crate::oracle::canonical) fn validate_spell_or_permanent_target(
    selector: &TargetSelector,
) -> Result<(), TargetDomainProjectionError> {
    match selector {
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            for projection in project_object_query_domains(
                predicate,
                &[ObjectKind::Spell, ObjectKind::Permanent],
            )? {
                if projection.domain == ObjectKind::Spell
                    && projection.predicate.contains_numeric_comparison()
                {
                    return Err(TargetDomainProjectionError::UnprovedNumericStackFilter);
                }
            }
            Ok(())
        }
        TargetSelector::Union(branches) => {
            if branches.is_empty() {
                return Err(TargetDomainProjectionError::EmptyBoolean);
            }
            for branch in branches {
                validate_spell_or_permanent_target(branch)?;
            }
            Ok(())
        }
        TargetSelector::Any | TargetSelector::Player(_) | TargetSelector::StackItem(_) => {
            Err(TargetDomainProjectionError::UnsupportedDomain)
        }
    }
}

#[cfg(test)]
#[path = "target_domain_tests.rs"]
mod tests;
