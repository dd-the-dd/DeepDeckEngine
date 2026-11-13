use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ObjectKind, ObjectQueryNode, ObjectSetNode, SelectorPredicate, SpellCastQueryNode,
    recognize_object_query_before_separator, recognize_spell_cast_query_before_separator,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObjectSetState {
    Branch,
    ConnectorOrComplete,
    Complete,
}

#[derive(Default)]
struct ObjectSetFrame {
    predicates: Vec<SelectorPredicate>,
    subject: Option<CollectionSubjectScope>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CollectionRelationKind {
    Control,
    Cast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CollectionSubjectScope {
    Controller(super::ObjectControllerScope),
    Caster(super::SpellCasterScope),
}

enum CollectionQueryNode {
    Control(ObjectQueryNode),
    Cast(SpellCastQueryNode),
}

fn is_shared_numeric_collection_qualifier(predicate: &SelectorPredicate) -> bool {
    matches!(predicate, SelectorPredicate::NumericComparison { .. })
}

fn has_branch_local_postnominal_qualifier(predicate: &SelectorPredicate) -> bool {
    match predicate {
        SelectorPredicate::HasKeyword(_) | SelectorPredicate::NumericComparison { .. } => true,
        SelectorPredicate::Intersection(operands) | SelectorPredicate::Union(operands) => {
            operands.iter().any(has_branch_local_postnominal_qualifier)
        }
        SelectorPredicate::Not(operand) => has_branch_local_postnominal_qualifier(operand),
        _ => false,
    }
}

/// A trailing postnominal qualifier scopes the complete coordinated noun
/// phrase: `artifacts and creatures with mana value X or less` means
/// `(artifact OR creature) AND mana-value<=X`. A qualifier attached to an
/// earlier branch remains local to that branch. Other postnominal predicates,
/// such as keywords, stay branch-local until their Oracle scoping has an
/// independent semantic proof.
fn compose_collection_predicates(
    mut predicates: Vec<SelectorPredicate>,
) -> Option<SelectorPredicate> {
    match predicates.len() {
        0 => None,
        1 => predicates.pop(),
        _ => {
            if predicates
                .iter()
                .take(predicates.len() - 1)
                .any(has_branch_local_postnominal_qualifier)
            {
                return Some(SelectorPredicate::Union(predicates));
            }
            let last = predicates
                .pop()
                .expect("multiple predicates have a last branch");
            let SelectorPredicate::Intersection(last_operands) = last else {
                predicates.push(last);
                return Some(SelectorPredicate::Union(predicates));
            };
            let (qualifiers, base_operands): (Vec<_>, Vec<_>) = last_operands
                .into_iter()
                .partition(is_shared_numeric_collection_qualifier);
            if qualifiers.is_empty() {
                predicates.push(SelectorPredicate::Intersection(base_operands));
                return Some(SelectorPredicate::Union(predicates));
            }
            if base_operands.is_empty() {
                predicates.push(SelectorPredicate::Intersection(qualifiers));
                return Some(SelectorPredicate::Union(predicates));
            }
            let last_base = match base_operands.len() {
                1 => base_operands
                    .into_iter()
                    .next()
                    .expect("one base predicate"),
                _ => SelectorPredicate::Intersection(base_operands),
            };
            predicates.push(last_base);
            let mut shared = Vec::with_capacity(1 + qualifiers.len());
            shared.push(SelectorPredicate::Union(predicates));
            shared.extend(qualifiers);
            Some(SelectorPredicate::Intersection(shared))
        }
    }
}

fn retain_query(
    cursor: &mut OracleTokenCursor<'_>,
    frame: &mut ObjectSetFrame,
    query: CollectionQueryNode,
) -> MachineResult<()> {
    let (predicate, subject) = match query {
        CollectionQueryNode::Control(query) => (
            query.predicate,
            query.controller.map(CollectionSubjectScope::Controller),
        ),
        CollectionQueryNode::Cast(query) => (
            query.predicate,
            query.caster.map(CollectionSubjectScope::Caster),
        ),
    };
    if let Some(subject) = subject {
        cursor.require(
            frame.subject.is_none(),
            "one player relation at the end of the object set",
        )?;
        frame.subject = Some(subject);
    }
    frame.predicates.push(predicate);
    Ok(())
}

fn recognize_collection_continuation(
    cursor: &mut OracleTokenCursor<'_>,
    relation_kind: CollectionRelationKind,
) -> MachineResult<CollectionQueryNode> {
    if cursor.peek().is_some_and(|raw| raw == ",") {
        cursor.consume(",")?;
    }
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("and") || raw.eq_ignore_ascii_case("or"))
    {
        let connector = cursor.peek().expect("checked connector").to_string();
        cursor.consume(&connector)?;
    }
    match relation_kind {
        CollectionRelationKind::Control => {
            recognize_object_query_before_separator(cursor).map(CollectionQueryNode::Control)
        }
        CollectionRelationKind::Cast => {
            recognize_spell_cast_query_before_separator(cursor).map(CollectionQueryNode::Cast)
        }
    }
}

/// Recognizes a non-targeting object collection. The shared child retains an
/// optional controller relation so parents can decide whether one is required.
pub(in crate::oracle::canonical) fn recognize_object_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectQueryNode> {
    let (predicate, subject) =
        recognize_collection_internal(cursor, CollectionRelationKind::Control)?;
    let controller = match subject {
        None => None,
        Some(CollectionSubjectScope::Controller(controller)) => Some(controller),
        Some(CollectionSubjectScope::Caster(_)) => {
            unreachable!("control wrapper owns relation kind")
        }
    };
    Ok(ObjectQueryNode {
        predicate,
        controller,
    })
}

fn recognize_collection_internal(
    cursor: &mut OracleTokenCursor<'_>,
    relation_kind: CollectionRelationKind,
) -> MachineResult<(SelectorPredicate, Option<CollectionSubjectScope>)> {
    let mut state = ObjectSetState::Branch;
    let mut frame = ObjectSetFrame::default();

    loop {
        state = match state {
            ObjectSetState::Branch => {
                cursor.enter("object-set.branch");
                let query = match relation_kind {
                    CollectionRelationKind::Control => CollectionQueryNode::Control(
                        recognize_object_query_before_separator(cursor)?,
                    ),
                    CollectionRelationKind::Cast => CollectionQueryNode::Cast(
                        recognize_spell_cast_query_before_separator(cursor)?,
                    ),
                };
                retain_query(cursor, &mut frame, query)?;
                ObjectSetState::ConnectorOrComplete
            }
            ObjectSetState::ConnectorOrComplete => {
                cursor.enter("object-set.connector-or-complete");
                if frame.subject.is_some() {
                    ObjectSetState::Complete
                } else if cursor.peek().is_some_and(|raw| {
                    raw == "," || raw.eq_ignore_ascii_case("and") || raw.eq_ignore_ascii_case("or")
                }) {
                    match cursor
                        .attempt(|cursor| recognize_collection_continuation(cursor, relation_kind))
                    {
                        Ok(query) => {
                            retain_query(cursor, &mut frame, query)?;
                            ObjectSetState::ConnectorOrComplete
                        }
                        Err(_) => ObjectSetState::Complete,
                    }
                } else {
                    ObjectSetState::Complete
                }
            }
            ObjectSetState::Complete => {
                cursor.enter("object-set.complete");
                let predicate = cursor.require_value(
                    compose_collection_predicates(frame.predicates),
                    "one or more permanent-set predicates",
                )?;
                return Ok((predicate, frame.subject));
            }
        };
    }
}

/// Recognizes a coordinated spell collection whose terminal player relation
/// denotes the caster. The typed caster is kept separate from object control
/// so stack-control changes cannot be misrepresented by later consumers.
pub(in crate::oracle::canonical) fn recognize_spell_cast_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SpellCastQueryNode> {
    let (predicate, subject) = recognize_collection_internal(cursor, CollectionRelationKind::Cast)?;
    cursor.enter("spell-cast-set.domain");
    cursor.require(
        predicate.has_domain(ObjectKind::Spell) && predicate.supports_domain(ObjectKind::Spell),
        "a collection explicitly anchored in the spell domain",
    )?;
    let caster = match subject {
        None => None,
        Some(CollectionSubjectScope::Caster(caster)) => Some(caster),
        Some(CollectionSubjectScope::Controller(_)) => {
            unreachable!("cast wrapper owns relation kind")
        }
    };
    Ok(SpellCastQueryNode { predicate, caster })
}

/// Recognizes a non-targeting set of battlefield permanents whose controller
/// is mandatory. Existing callers keep this stricter contract while newer
/// parents may reuse `recognize_object_collection` without a controller.
pub(in crate::oracle::canonical) fn recognize_controlled_object_set(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectSetNode> {
    let query = recognize_object_collection(cursor)?;
    cursor.enter("object-set.permanent-domain");
    cursor.require(
        query.predicate.supports_domain(ObjectKind::Permanent)
            && query.predicate.has_domain(ObjectKind::Permanent),
        "an object set explicitly anchored in the battlefield-permanent domain",
    )?;
    Ok(ObjectSetNode {
        predicate: query.predicate,
        controller: cursor
            .require_value(query.controller, "a controller relation for the object set")?,
    })
}
