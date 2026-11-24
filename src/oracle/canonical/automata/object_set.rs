use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ObjectKind, ObjectQueryContext, ObjectQueryNode, ObjectSetNode, RecognizedObjectQuery,
    SelectorPredicate, SpellCastQueryNode, recognize_object_noun_query,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObjectSetState {
    Branch,
    ConnectorOrComplete,
    Complete,
}

#[derive(Default)]
struct ObjectSetFrame {
    branches: Vec<RecognizedObjectQuery>,
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

/// One recognized object set: the coordinated noun phrases composed into
/// one query, and whether `other` excludes the source. Every player relation
/// is read by the noun phrase itself; a relation ends the set. Parents
/// type-check the query through its projections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ObjectSetQuery {
    pub query: RecognizedObjectQuery,
    pub exclude_source: bool,
}

impl ObjectQueryNode {
    /// CR 109.2: whether this query denotes battlefield permanents. A type
    /// or subtype head without `card`/`spell` names a permanent; a noun the
    /// permanent domain admits (`tokens`, `commanders`) qualified by a
    /// control relation names permanents too (CR 108.4, 110.2). This is the
    /// one permanent-domain check every collection parent shares.
    pub(in crate::oracle::canonical) fn denotes_battlefield_permanents(&self) -> bool {
        self.predicate
            .is_in_domain_when_controlled(ObjectKind::Permanent, self.controller.is_some())
    }
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

fn recognize_collection_continuation(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<RecognizedObjectQuery> {
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
    recognize_object_noun_query(cursor, ObjectQueryContext::ORDINARY)
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

/// A counted collection: present objects, or the departures of this turn
/// (`creatures that died this turn`), which are no longer objects of any
/// present zone and are counted from the turn's records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CountedObjectCollection {
    Present(ObjectQueryNode),
    Departed(super::ScopedObjectQueryNode),
}

/// The collection a counting introducer (`for each`, `the number of`)
/// reads. The shared set reader owns the phrase; this child chooses the
/// projection its source set needs and ends the operand transactionally: an
/// unread relative clause after it fails here with its own diagnostic.
pub(in crate::oracle::canonical) fn recognize_counted_object_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CountedObjectCollection> {
    let set = recognize_object_set_query(cursor)?;
    let counted = if set.query.relations().departure().is_some() {
        let departed = set.query.into_departure_query(cursor, set.exclude_source)?;
        CountedObjectCollection::Departed(cursor.require_value(
            departed,
            "a departure history read by the counted collection",
        )?)
    } else {
        let mut objects = set.query.into_object_query(cursor)?;
        objects.exclude_source = set.exclude_source;
        CountedObjectCollection::Present(objects)
    };
    super::refuse_unread_relative_tail(cursor)?;
    Ok(counted)
}

/// Recognizes a non-targeting object collection. The shared child retains an
/// optional controller relation so parents can decide whether one is required.
pub(in crate::oracle::canonical) fn recognize_object_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectQueryNode> {
    let set = recognize_object_set_query(cursor)?;
    let mut objects = set.query.into_object_query(cursor)?;
    objects.exclude_source = set.exclude_source;
    Ok(objects)
}

/// The one coordinated-set reader. Each branch is the shared object noun
/// phrase; the set reads its branches and its source exclusion only.
pub(in crate::oracle::canonical) fn recognize_object_set_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectSetQuery> {
    let mut state = ObjectSetState::Branch;
    let mut frame = ObjectSetFrame::default();

    loop {
        state = match state {
            ObjectSetState::Branch => {
                cursor.enter("object-set.branch");
                // `other Elves you control`: the source exclusion scopes the
                // whole coordinated collection and is read once, here.
                frame.exclude_source = super::recognize_optional_source_exclusion(cursor)?;
                frame.branches.push(recognize_object_noun_query(
                    cursor,
                    ObjectQueryContext::ORDINARY,
                )?);
                ObjectSetState::ConnectorOrComplete
            }
            ObjectSetState::ConnectorOrComplete => {
                cursor.enter("object-set.connector-or-complete");
                // A player relation closes the coordinated set: it relates
                // the whole set, and no later branch may restate one.
                if frame
                    .branches
                    .last()
                    .is_some_and(|branch| !branch.relations().is_empty())
                {
                    ObjectSetState::Complete
                } else if cursor.peek().is_some_and(|raw| {
                    raw == "," || raw.eq_ignore_ascii_case("and") || raw.eq_ignore_ascii_case("or")
                }) {
                    match cursor.attempt(recognize_collection_continuation) {
                        Ok(branch) => {
                            frame.branches.push(branch);
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
                let mut query = cursor.require_value(
                    RecognizedObjectQuery::coordinate(
                        frame.branches,
                        compose_collection_predicates,
                    ),
                    "one or more permanent-set predicates",
                )?;
                // `other than this creature` after the set: the same source
                // exclusion as a leading `other`, stated once.
                let postnominal = query.take_source_exclusion();
                cursor.require(
                    !(postnominal && frame.exclude_source),
                    "one source exclusion for one object set",
                )?;
                return Ok(ObjectSetQuery {
                    query,
                    exclude_source: frame.exclude_source || postnominal,
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
    let set = recognize_object_set_query(cursor)?;
    let query = set.query.into_spell_cast_query(cursor)?;
    cursor.enter("spell-cast-set.domain");
    cursor.require(
        !set.exclude_source,
        "a spell collection without a source exclusion",
    )?;
    cursor.require(
        query.predicate.is_in_domain(ObjectKind::Spell),
        "a collection explicitly anchored in the spell domain",
    )?;
    Ok(query)
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

/// Lexical head of a bare subtype-headed collection subject (`Wizards you
/// control get`, `Samurai creatures you control`, `other Goblins and Orcs
/// you control`, `Goblin creatures get`): after an optional `other`, a
/// subtype word that is not the card's own name, read by the shared
/// collection as a plural collection of battlefield permanents (CR 109.2).
/// Pure lookahead: the collection child still owns the phrase and its
/// failures; a singular subtype (`Goblin Guide deals`) is never claimed.
pub(in crate::oracle::canonical) fn subtype_collection_head_starts(
    cursor: &OracleTokenCursor<'_>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
) -> bool {
    let head_offset = usize::from(
        cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("other")),
    );
    let head = cursor.position() + head_offset;
    let subtype_head = cursor
        .token_slice(head, head + 1)
        .first()
        .is_some_and(|token| token.token_type == "subtypeWord");
    if !subtype_head
        || name_links.iter().any(|link| {
            (link.token_start..link.token_end.max(link.token_start + 1)).contains(&head)
        })
    {
        return false;
    }
    let mut trial = cursor.clone();
    let start = trial.position();
    let Ok(query) = recognize_object_collection(&mut trial) else {
        return false;
    };
    let read = trial.token_slice(start, trial.position());
    let plural = (0..read.len()).any(|index| {
        super::atom_surface_number(
            &read[index].raw,
            read.get(index + 1).map(|token| token.raw.as_str()),
        ) == Some(super::GrammaticalNumber::Plural)
    });
    plural && query.denotes_battlefield_permanents()
}

#[cfg(test)]
#[path = "object_collection_domain_tests.rs"]
mod domain_tests;

/// A coordinated set whose control and ownership stay relations.
pub(in crate::oracle::canonical) fn recognize_related_object_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::RelatedObjectQueryNode> {
    let set = recognize_object_set_query(cursor)?;
    let mut related = set.query.into_related_query(cursor)?;
    related.objects.exclude_source = set.exclude_source;
    Ok(related)
}
