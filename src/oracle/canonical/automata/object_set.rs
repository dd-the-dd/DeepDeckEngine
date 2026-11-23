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
    ownership: Option<super::ObjectOwnershipScope>,
    exclude_source: bool,
}

/// `[other] QUERY` for a parent that owns its query boundary: the source
/// exclusion is read here, once, and returned as the query's own field, so
/// no parent scans for `other` itself. The collection frame uses the same
/// lexical child for its first branch.
pub(in crate::oracle::canonical) fn recognize_query_excluding_source(
    cursor: &mut OracleTokenCursor<'_>,
    mut recognize_query: impl FnMut(&mut OracleTokenCursor<'_>) -> MachineResult<ObjectQueryNode>,
) -> MachineResult<ObjectQueryNode> {
    let excluded = super::recognize_optional_source_exclusion(cursor)?;
    let mut query = recognize_query(cursor)?;
    cursor.require(
        !(excluded && query.exclude_source),
        "one source exclusion for one object query",
    )?;
    query.exclude_source |= excluded;
    Ok(query)
}

/// The collection reading of one recognized object set: its composed
/// predicate, its player relations and whether `other` excludes the source.
struct RecognizedCollection {
    predicate: SelectorPredicate,
    subject: Option<CollectionSubjectScope>,
    ownership: Option<super::ObjectOwnershipScope>,
    exclude_source: bool,
}

impl ObjectQueryNode {
    /// CR 109.2: whether this query denotes battlefield permanents. A type
    /// or subtype head without `card`/`spell` names a permanent; a noun the
    /// permanent domain admits (`tokens`, `commanders`) qualified by a
    /// control relation names permanents too (CR 108.4, 110.2). This is the
    /// one permanent-domain check every collection parent shares.
    pub(in crate::oracle::canonical) fn denotes_battlefield_permanents(&self) -> bool {
        self.predicate.supports_domain(ObjectKind::Permanent)
            && (self.predicate.has_domain(ObjectKind::Permanent)
                || (self.controller.is_some() && self.predicate.has_nominal_head()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CollectionRelationKind {
    Control,
    Cast,
    Relations,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CollectionSubjectScope {
    Controller(super::ObjectControllerScope),
    Caster(super::SpellCasterScope),
}

enum CollectionQueryNode {
    Control(ObjectQueryNode),
    Cast(SpellCastQueryNode),
    Relations(super::RelatedObjectQueryNode),
}

fn is_shared_numeric_collection_qualifier(predicate: &SelectorPredicate) -> bool {
    matches!(predicate, SelectorPredicate::NumericComparison { .. })
}

fn has_branch_local_postnominal_qualifier(predicate: &SelectorPredicate) -> bool {
    match predicate {
        SelectorPredicate::HasKeyword(_)
        | SelectorPredicate::HasCounter(_)
        | SelectorPredicate::NumericComparison { .. } => true,
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
                return Some(super::lift_shared_trailing_domain(predicates));
            }
            let last = predicates
                .pop()
                .expect("multiple predicates have a last branch");
            let SelectorPredicate::Intersection(last_operands) = last else {
                predicates.push(last);
                return Some(super::lift_shared_trailing_domain(predicates));
            };
            let (qualifiers, base_operands): (Vec<_>, Vec<_>) = last_operands
                .into_iter()
                .partition(is_shared_numeric_collection_qualifier);
            if qualifiers.is_empty() {
                predicates.push(SelectorPredicate::Intersection(base_operands));
                return Some(super::lift_shared_trailing_domain(predicates));
            }
            if base_operands.is_empty() {
                predicates.push(SelectorPredicate::Intersection(qualifiers));
                return Some(super::lift_shared_trailing_domain(predicates));
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
        CollectionQueryNode::Relations(query) => {
            if let Some(ownership) = query.ownership {
                cursor.require(
                    frame.ownership.is_none(),
                    "one explicit owner relation at the end of the object set",
                )?;
                frame.ownership = Some(ownership);
            }
            (
                query.objects.predicate,
                query
                    .objects
                    .controller
                    .map(CollectionSubjectScope::Controller),
            )
        }
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
        CollectionRelationKind::Relations => {
            super::recognize_related_object_query_before_separator(cursor)
                .map(CollectionQueryNode::Relations)
        }
        CollectionRelationKind::Control => {
            recognize_object_query_before_separator(cursor).map(CollectionQueryNode::Control)
        }
        CollectionRelationKind::Cast => {
            recognize_spell_cast_query_before_separator(cursor).map(CollectionQueryNode::Cast)
        }
    }
}

/// Universal quantification is independent of a choice's min/max cardinality.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectCollectionQuantifier {
    Universal,
}

/// Domain evidence supplied by the parent is not an extra predicate. A subtype
/// can name an object in that domain without pretending it is a creature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DomainObjectCollectionNode {
    pub quantifier: ObjectCollectionQuantifier,
    pub domain: ObjectKind,
    pub exclude_source: bool,
    pub objects: ObjectQueryNode,
}

/// Opt-in child for parents with an explicit object domain. Bare plural
/// collections, Each and All all denote universal membership; no choice
/// cardinality or target declaration is introduced. Old wrappers are unchanged.
pub(in crate::oracle::canonical) fn recognize_object_collection_in_domain(
    cursor: &mut OracleTokenCursor<'_>,
    domain: ObjectKind,
) -> MachineResult<DomainObjectCollectionNode> {
    let mut trial = cursor.clone();
    trial.enter("object-collection.quantifier");
    if let Some(word) = trial
        .peek()
        .filter(|word| word.eq_ignore_ascii_case("each") || word.eq_ignore_ascii_case("all"))
    {
        let word = word.to_owned();
        trial.consume(&word)?;
    }
    let objects = recognize_object_collection(&mut trial)?;
    let exclude_source = objects.exclude_source;
    trial.enter("object-collection.context-domain");
    trial.require(
        objects.predicate.supports_domain(domain) && objects.predicate.has_nominal_head(),
        "a nominal object collection compatible with its explicit parent domain",
    )?;
    trial.enter("object-collection.representable-filter");
    trial.require(
        crate::oracle::canonical::effects::domain_filter_is_representable(&objects.predicate),
        "a collection filter whose domain identities preserve Boolean meaning",
    )?;
    *cursor = trial;
    Ok(DomainObjectCollectionNode {
        quantifier: ObjectCollectionQuantifier::Universal,
        domain,
        exclude_source,
        objects,
    })
}

/// Recognizes a non-targeting object collection. The shared child retains an
/// optional controller relation so parents can decide whether one is required.
pub(in crate::oracle::canonical) fn recognize_object_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectQueryNode> {
    let collection = recognize_collection_internal(cursor, CollectionRelationKind::Control)?;
    let controller = match collection.subject {
        None => None,
        Some(CollectionSubjectScope::Controller(controller)) => Some(controller),
        Some(CollectionSubjectScope::Caster(_)) => {
            unreachable!("control wrapper owns relation kind")
        }
    };
    Ok(ObjectQueryNode {
        predicate: collection.predicate,
        controller,
        exclude_source: collection.exclude_source,
    })
}

fn recognize_collection_internal(
    cursor: &mut OracleTokenCursor<'_>,
    relation_kind: CollectionRelationKind,
) -> MachineResult<RecognizedCollection> {
    let collection = recognize_collection_with_relations(cursor, relation_kind)?;
    cursor.require(
        collection.ownership.is_none(),
        "no owner relation discarded by this compatibility collection wrapper",
    )?;
    Ok(collection)
}

fn recognize_collection_with_relations(
    cursor: &mut OracleTokenCursor<'_>,
    relation_kind: CollectionRelationKind,
) -> MachineResult<RecognizedCollection> {
    let mut state = ObjectSetState::Branch;
    let mut frame = ObjectSetFrame::default();

    loop {
        state = match state {
            ObjectSetState::Branch => {
                cursor.enter("object-set.branch");
                // `other Elves you control`: the source exclusion scopes the
                // whole coordinated collection and is read once, here.
                frame.exclude_source = super::recognize_optional_source_exclusion(cursor)?;
                let query = match relation_kind {
                    CollectionRelationKind::Relations => CollectionQueryNode::Relations(
                        super::recognize_related_object_query_before_separator(cursor)?,
                    ),
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
                if frame.subject.is_some() || frame.ownership.is_some() {
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
                return Ok(RecognizedCollection {
                    predicate,
                    subject: frame.subject,
                    ownership: frame.ownership,
                    exclude_source: frame.exclude_source,
                });
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
    let RecognizedCollection {
        predicate,
        subject,
        exclude_source,
        ..
    } = recognize_collection_internal(cursor, CollectionRelationKind::Cast)?;
    cursor.enter("spell-cast-set.domain");
    cursor.require(
        !exclude_source,
        "a spell collection without a source exclusion",
    )?;
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
        !query.exclude_source,
        "an object set without a source exclusion this parent lowers",
    )?;
    cursor.require(
        query.denotes_battlefield_permanents(),
        "an object set explicitly anchored in the battlefield-permanent domain",
    )?;
    Ok(ObjectSetNode {
        predicate: query.predicate,
        controller: cursor
            .require_value(query.controller, "a controller relation for the object set")?,
    })
}

#[cfg(test)]
#[path = "object_collection_domain_tests.rs"]
mod domain_tests;

pub(in crate::oracle::canonical) fn recognize_related_object_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::RelatedObjectQueryNode> {
    let RecognizedCollection {
        predicate,
        subject,
        ownership,
        exclude_source,
    } = recognize_collection_with_relations(cursor, CollectionRelationKind::Relations)?;
    let controller = match subject {
        None => None,
        Some(CollectionSubjectScope::Controller(scope)) => Some(scope),
        Some(CollectionSubjectScope::Caster(_)) => {
            return Err(cursor
                .fail("an object relationship, not a caster relation in a counted object query"));
        }
    };
    Ok(super::RelatedObjectQueryNode {
        objects: ObjectQueryNode {
            predicate,
            controller,
            exclude_source,
        },
        ownership,
    })
}
