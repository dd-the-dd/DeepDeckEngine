use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, PlayerRole, lookup_card_type, lookup_player_role_any_number, lookup_supertype,
};

use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    CardSupertype, NumericComparisonOperator, ObjectCardType, ObjectControllerScope, ObjectKind,
    ObjectPredicate, PlayerPredicate, SelectorNumericProperty, SelectorPredicate, SpellCasterScope,
    StackAbilityKind, StackItemPredicate, TargetSelector, object_predicate_atom_starts,
    parameterless_keyword_from_oracle_word, recognize_mana_cost, recognize_object_predicate_atom,
    recognize_parameterless_keyword,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObjectQueryState {
    Start,
    ReadObjectIntersection,
    ReadUnion,
    ReadAfterComma,
    ReadPostnominalQualifier,
    Complete,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ObjectQueryFrame {
    union_branches: Vec<SelectorPredicate>,
    /// Per union branch: some characteristic word appeared in plural form.
    branch_has_plural_surface: Vec<bool>,
    /// Per union branch: the grammatical number of its noun head.
    branch_number: Vec<super::GrammaticalNumber>,
    has_explicit_union: bool,
    has_repeated_head_article: bool,
    last_connector_is_exclusive: bool,
    has_inclusive_connector: bool,
    postnominal_qualifiers: Vec<PostnominalQualifier>,
    /// The player relations read from their own verbs (`control`, `cast`,
    /// `own`), never chosen by the parent.
    relations: QueryRelations,
    /// Where the ownership relation stood among the postnominal qualifiers,
    /// so a parent reading it as the `ownedBy` characteristic keeps its
    /// position in the composed predicate.
    owner_qualifier_index: Option<usize>,
    logical_connectors: Vec<super::ContextualListConnector>,
    pending_logical_connector: Option<super::ContextualListConnector>,
    logical_comma: bool,
    /// The open comma run: the cursor before its first comma and the frame
    /// as it was there. It commits when `or`/`and/or` closes it.
    comma_run: Option<(
        super::super::machine::MachineCheckpoint,
        Box<ObjectQueryFrame>,
    )>,
    /// A run closed by `or`/`and/or` whose member after the connector must
    /// still agree in number before the list commits.
    closing_run: Option<(
        super::super::machine::MachineCheckpoint,
        Box<ObjectQueryFrame>,
    )>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PostnominalQualifierScope {
    LastCoordinatedHead,
    WholeQuery,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PostnominalQualifier {
    predicate: SelectorPredicate,
    scope: PostnominalQualifierScope,
}

/// The relation lexicon of an object query (CR 108.3 owner, 108.4
/// controller, 601.2 caster). The verb names the relation; the subject
/// names the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RelationVerb {
    Control,
    Cast,
    Own,
}

impl RelationVerb {
    /// The verb form a reader of this relation expects after its subject.
    const fn surface(self, third_person: bool) -> &'static str {
        match (self, third_person) {
            (Self::Control, false) => "control",
            (Self::Control, true) => "controls",
            (Self::Cast, false) => "cast",
            (Self::Cast, true) => "casts",
            (Self::Own, false) => "own",
            (Self::Own, true) => "owns",
        }
    }

    /// The verb in its agreement with the subject: base form after `you`
    /// and plural subjects, third person singular after `an opponent` and
    /// a bound player.
    fn from_word(raw: &str, third_person: bool) -> Option<Self> {
        match (raw.to_ascii_lowercase().as_str(), third_person) {
            ("control", false) | ("controls", true) => Some(Self::Control),
            ("cast", false) | ("casts", true) => Some(Self::Cast),
            ("own", false) | ("owns", true) => Some(Self::Own),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuerySubjectScope {
    AbilityController,
    NotAbilityController,
    OpponentOfAbilityController,
    /// `defending player` / `that player` bound by the triggering event.
    EventPlayer,
}

/// Where one relation verb was read, so a parent that cannot consume the
/// relation reports the verb itself, as a reader expecting another verb
/// would have.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RelationSite {
    token_index: usize,
    span_start: usize,
    span_end: usize,
    raw: String,
    negated: bool,
    third_person: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReadRelation {
    scope: QuerySubjectScope,
    site: RelationSite,
}

/// Every player relation one noun phrase states, one slot per relation
/// verb. The noun phrase reads them all; each parent type-checks which
/// relations it consumes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct QueryRelations {
    controller: Option<ReadRelation>,
    caster: Option<ReadRelation>,
    owner: Option<ReadRelation>,
}

impl QueryRelations {
    fn slot_mut(&mut self, verb: RelationVerb) -> &mut Option<ReadRelation> {
        match verb {
            RelationVerb::Control => &mut self.controller,
            RelationVerb::Cast => &mut self.caster,
            RelationVerb::Own => &mut self.owner,
        }
    }

    /// Whether the phrase states no player relation at all.
    pub(in crate::oracle::canonical) fn is_empty(&self) -> bool {
        self.controller.is_none() && self.caster.is_none() && self.owner.is_none()
    }

    fn controller_scope(&self) -> Option<QuerySubjectScope> {
        self.controller.as_ref().map(|relation| relation.scope)
    }

    fn owner_scope(&self) -> Option<QuerySubjectScope> {
        self.owner.as_ref().map(|relation| relation.scope)
    }

    /// Whether the phrase relates its objects to a controlling player.
    pub(in crate::oracle::canonical) fn has_controller(&self) -> bool {
        self.controller.is_some()
    }
}

/// A relation the parent cannot consume fails at its verb, in the state a
/// relation reader expecting `expected` reports (`you cast` where a
/// controlled permanent is required fails at `cast`, expecting `control`).
fn refuse_relation(
    cursor: &mut OracleTokenCursor<'_>,
    relation: &ReadRelation,
    expected: RelationVerb,
) -> super::super::MachineFailure {
    let site = &relation.site;
    cursor.enter(if site.negated {
        "selector.controller-qualifier.negated-relation"
    } else {
        "selector.controller-qualifier.relation-or-negation"
    });
    cursor.fail_with_span(
        expected.surface(site.third_person),
        site.token_index,
        site.span_start,
        site.span_end,
        site.raw.clone(),
    )
}

/// Ownership stays a relation to a player, not an object characteristic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectOwnershipScope {
    AbilityController,
    NotAbilityController,
    OpponentOfAbilityController,
    /// The player the triggering event records.
    EventPlayer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct RelatedObjectQueryNode {
    pub objects: super::ObjectQueryNode,
    pub ownership: Option<ObjectOwnershipScope>,
}

/// How many objects the parent's determiner selects with one quota. It is a
/// parent fact handed to the noun phrase, not a separate grammar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum QueryQuota {
    #[default]
    ExistingQuery,
    /// One parent-owned quota applies to the whole candidate predicate. An
    /// explicitly repeated determiner would introduce a separate quota and
    /// must not be erased by Boolean normalization into a single union.
    ParentOwnedShared { exactly_one: bool },
}

/// The facts a parent supplies to the one object noun-phrase recognizer.
/// They are data: there is one recognizer, not one wrapper per policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ObjectQueryContext {
    quota: QueryQuota,
    /// `Some(available)` for a resolution-owned query, whose numeric
    /// comparisons may read pure expressions and, when available, the
    /// ability source (its counters, by last-known information).
    source_available: Option<bool>,
    /// The query describes declared target candidates, which may be objects
    /// on the stack: a `that targets ...` relative clause relates such an
    /// object to its own targets (CR 115.1).
    stack_targets: bool,
}

impl ObjectQueryContext {
    /// An ordinary query: literal or declared-X comparisons, no quota.
    pub(in crate::oracle::canonical) const ORDINARY: Self = Self {
        quota: QueryQuota::ExistingQuery,
        source_available: None,
        stack_targets: false,
    };

    /// The candidates of a declared target slot (CR 115.1): an ordinary
    /// query whose stack objects may be related to their own targets.
    pub(in crate::oracle::canonical) const DECLARED_TARGET: Self = Self {
        quota: QueryQuota::ExistingQuery,
        source_available: None,
        stack_targets: true,
    };

    /// A resolution-owned query. Source proof is distinct from implicit
    /// event subjects and grammatical antecedents.
    pub(in crate::oracle::canonical) const fn resolution(source_available: bool) -> Self {
        Self {
            quota: QueryQuota::ExistingQuery,
            source_available: Some(source_available),
            stack_targets: false,
        }
    }

    /// One parent-owned quota (`two artifacts or creatures`) for the whole
    /// coordinated predicate.
    pub(in crate::oracle::canonical) fn shared_quota(count: &super::NumericExpression) -> Self {
        Self {
            quota: QueryQuota::ParentOwnedShared {
                exactly_one: matches!(
                    count,
                    super::NumericExpression::Atom(super::NumericAtom::Integer(1))
                ),
            },
            source_available: None,
            stack_targets: false,
        }
    }
}

/// One recognized object noun phrase: its composed predicate and every
/// player relation it states. Parents only type-check it through the
/// projections below; none of them re-reads the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct RecognizedObjectQuery {
    predicate: SelectorPredicate,
    /// The same predicate with an ownership relative to the ability
    /// controller read as the `ownedBy` characteristic, at its position.
    owned_predicate: Option<SelectorPredicate>,
    relations: QueryRelations,
}

impl RecognizedObjectQuery {
    /// The composed characteristic predicate, without any player relation.
    pub(in crate::oracle::canonical) fn predicate(&self) -> &SelectorPredicate {
        &self.predicate
    }

    pub(in crate::oracle::canonical) fn relations(&self) -> &QueryRelations {
        &self.relations
    }

    /// Coordinates the noun phrases of one set (`artifacts and creatures you
    /// control`). The set composes the characteristic predicates; its player
    /// relations are the last branch's, since a relation ends a set.
    pub(in crate::oracle::canonical) fn coordinate(
        branches: Vec<RecognizedObjectQuery>,
        compose: impl Fn(Vec<SelectorPredicate>) -> Option<SelectorPredicate>,
    ) -> Option<RecognizedObjectQuery> {
        let last = branches.last()?;
        let relations = last.relations.clone();
        let owned_last = last.owned_predicate.clone();
        let plain: Vec<SelectorPredicate> = branches
            .into_iter()
            .map(|branch| branch.predicate)
            .collect();
        let owned_predicate = match owned_last {
            Some(owned) => {
                let mut predicates = plain.clone();
                if let Some(slot) = predicates.last_mut() {
                    *slot = owned;
                }
                compose(predicates)
            }
            None => None,
        };
        Some(RecognizedObjectQuery {
            predicate: compose(plain)?,
            owned_predicate,
            relations,
        })
    }

    /// The characteristic predicate a control consumer reads: an ownership
    /// relative to the ability controller becomes `ownedBy` (CR 108.3);
    /// any other relation the consumer cannot represent fails at its verb.
    fn controlled_predicate(
        &self,
        cursor: &mut OracleTokenCursor<'_>,
    ) -> MachineResult<SelectorPredicate> {
        if let Some(caster) = &self.relations.caster {
            return Err(refuse_relation(cursor, caster, RelationVerb::Control));
        }
        match &self.relations.owner {
            None => Ok(self.predicate.clone()),
            Some(owner) => match owner.scope {
                QuerySubjectScope::AbilityController | QuerySubjectScope::NotAbilityController => {
                    cursor.require_value(
                        self.owned_predicate.clone(),
                        "an ownership characteristic composed with the query",
                    )
                }
                QuerySubjectScope::OpponentOfAbilityController | QuerySubjectScope::EventPlayer => {
                    Err(refuse_relation(cursor, owner, RelationVerb::Control))
                }
            },
        }
    }

    /// A query whose player relation is control (CR 108.4). An ownership
    /// relative to the ability controller is the `ownedBy` characteristic
    /// of the same objects; the player who cast a spell is not a controller.
    pub(in crate::oracle::canonical) fn into_object_query(
        self,
        cursor: &mut OracleTokenCursor<'_>,
    ) -> MachineResult<super::ObjectQueryNode> {
        let predicate = self.controlled_predicate(cursor)?;
        Ok(super::ObjectQueryNode {
            predicate,
            controller: self
                .relations
                .controller_scope()
                .map(object_controller_scope),
            exclude_source: false,
        })
    }

    /// A spell collection whose player relation is the player who cast the
    /// spell, not the spell's current controller on the stack.
    pub(in crate::oracle::canonical) fn into_spell_cast_query(
        self,
        cursor: &mut OracleTokenCursor<'_>,
    ) -> MachineResult<super::SpellCastQueryNode> {
        if let Some(relation) = self
            .relations
            .controller
            .as_ref()
            .or(self.relations.owner.as_ref())
        {
            return Err(refuse_relation(cursor, relation, RelationVerb::Cast));
        }
        Ok(super::SpellCastQueryNode {
            predicate: self.predicate,
            caster: self
                .relations
                .caster
                .as_ref()
                .map(|relation| spell_caster_scope(relation.scope)),
        })
    }

    /// Object relations kept as relations: control and ownership.
    pub(in crate::oracle::canonical) fn into_related_query(
        self,
        cursor: &mut OracleTokenCursor<'_>,
    ) -> MachineResult<RelatedObjectQueryNode> {
        if let Some(caster) = &self.relations.caster {
            return Err(refuse_relation(cursor, caster, RelationVerb::Control));
        }
        Ok(RelatedObjectQueryNode {
            objects: super::ObjectQueryNode {
                predicate: self.predicate,
                controller: self
                    .relations
                    .controller_scope()
                    .map(object_controller_scope),
                exclude_source: false,
            },
            ownership: self.relations.owner_scope().map(object_ownership_scope),
        })
    }
}

/// Recognizes an object predicate plus every player relation it states,
/// independently of whether a parent uses it as a target or as a set query.
///
/// Coordinators are transactional: a comma, `or` or `and/or` is consumed only
/// when another object phrase follows, and a comma-separated run commits only
/// once `or`/`and/or` closes it (or when it lists negative modifiers of one
/// head). Otherwise the connector stays with the parent, whatever the parent
/// is (an `if` separator, a cost list, an effect sequence).
pub(in crate::oracle::canonical) fn recognize_object_noun_query(
    cursor: &mut OracleTokenCursor<'_>,
    context: ObjectQueryContext,
) -> MachineResult<RecognizedObjectQuery> {
    recognize_object_query_frame(cursor, context, false)
}

/// The ordinary object query: the noun phrase read as controlled objects.
pub(in crate::oracle::canonical) fn recognize_object_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::ObjectQueryNode> {
    recognize_object_noun_query(cursor, ObjectQueryContext::ORDINARY)?.into_object_query(cursor)
}

fn recognize_object_query_frame(
    cursor: &mut OracleTokenCursor<'_>,
    context: ObjectQueryContext,
    logical_properties: bool,
) -> MachineResult<RecognizedObjectQuery> {
    let query_source_available = context.source_available;
    let mut state = ObjectQueryState::Start;
    let mut frame = ObjectQueryFrame::default();

    loop {
        state = match state {
            ObjectQueryState::Start => {
                cursor.enter("selector.predicate.start");
                if object_selector_phrase_starts(cursor) {
                    ObjectQueryState::ReadObjectIntersection
                } else {
                    return Err(cursor.fail("an object predicate"));
                }
            }
            ObjectQueryState::ReadObjectIntersection => {
                cursor.enter("selector.predicate.intersection");
                if !frame.union_branches.is_empty() {
                    if logical_properties {
                        frame.logical_connectors.push(
                            frame
                                .pending_logical_connector
                                .take()
                                .unwrap_or(super::ContextualListConnector::Or),
                        );
                    }
                    if cursor.peek().is_some_and(|word| {
                        word.eq_ignore_ascii_case("a") || word.eq_ignore_ascii_case("an")
                    }) {
                        cursor.enter("selector.coordination.cardinality");
                        frame.has_repeated_head_article = true;
                        frame.has_explicit_union = true;
                    }
                    if let Err(failure) = consume_optional_selector_article(cursor) {
                        // A failed member after a tentative comma ends the
                        // list there: the comma is the parent's, and the
                        // member's failure explains a parent failing at it.
                        if abandon_comma_run(cursor, &mut frame) {
                            cursor.defer_failure(failure);
                            state = ObjectQueryState::ReadPostnominalQualifier;
                            continue;
                        }
                        return Err(failure);
                    }
                }
                let branch_start = cursor.position();
                let branch = match recognize_object_intersection(cursor) {
                    Ok(branch) => branch,
                    Err(failure) => {
                        if abandon_comma_run(cursor, &mut frame) {
                            cursor.defer_failure(failure);
                            state = ObjectQueryState::ReadPostnominalQualifier;
                            continue;
                        }
                        return Err(failure);
                    }
                };
                frame.union_branches.push(branch);
                let branch = cursor.token_slice(branch_start, cursor.position());
                let numbers = (0..branch.len())
                    .filter_map(|index| {
                        super::atom_surface_number(
                            &branch[index].raw,
                            branch.get(index + 1).map(|token| token.raw.as_str()),
                        )
                    })
                    .collect::<Vec<_>>();
                frame
                    .branch_has_plural_surface
                    .push(numbers.contains(&super::GrammaticalNumber::Plural));
                // A member's own number: a trailing domain noun belongs to
                // the whole list (`creature, land, or planeswalker cards`).
                let numbers = (0..branch.len())
                    .filter(|index| !super::atom_surface_is_domain_noun(&branch[*index].raw))
                    .filter_map(|index| {
                        super::atom_surface_number(
                            &branch[index].raw,
                            branch.get(index + 1).map(|token| token.raw.as_str()),
                        )
                    })
                    .collect::<Vec<_>>();
                frame
                    .branch_number
                    .push(if numbers.contains(&super::GrammaticalNumber::Plural) {
                        super::GrammaticalNumber::Plural
                    } else if numbers.contains(&super::GrammaticalNumber::Singular) {
                        super::GrammaticalNumber::Singular
                    } else {
                        super::GrammaticalNumber::Unknown
                    });
                // The member after a closing `or` agrees with the run too.
                if let Some((checkpoint, snapshot)) = frame.closing_run.take() {
                    let run_start = snapshot.union_branches.len().saturating_sub(1);
                    if !numbers_agree(&frame.branch_number[run_start..]) {
                        let failure = cursor
                            .fail("members of one object list sharing their grammatical number");
                        cursor.rollback(checkpoint);
                        frame = *snapshot;
                        cursor.defer_failure(failure);
                        state = ObjectQueryState::ReadPostnominalQualifier;
                        continue;
                    }
                }
                ObjectQueryState::ReadUnion
            }
            ObjectQueryState::ReadUnion => {
                cursor.enter("selector.predicate.union");
                if cursor.peek().is_some_and(|token| token == ",") {
                    if comma_continues_object_list(cursor) {
                        // A comma run is tentative until `or`/`and/or` closes it.
                        if frame.comma_run.is_none() {
                            let snapshot = Box::new(frame.clone());
                            frame.comma_run = Some((cursor.checkpoint(), snapshot));
                        }
                        frame.logical_comma = logical_properties;
                        cursor.consume(",")?;
                        frame.last_connector_is_exclusive = false;
                        ObjectQueryState::ReadAfterComma
                    } else {
                        // `, or an opponent controls`: a dangling connector
                        // is the reason the list stops here.
                        let reason = dangling_list_connector_failure(cursor);
                        settle_comma_run_with(cursor, &mut frame, reason);
                        ObjectQueryState::ReadPostnominalQualifier
                    }
                } else if cursor.peek().is_some_and(|token| {
                    token.eq_ignore_ascii_case("or")
                        || token.eq_ignore_ascii_case("and/or")
                        || (logical_properties && token.eq_ignore_ascii_case("and"))
                }) || (!logical_properties
                    && coordinated_type_adjective_and(cursor, &frame))
                {
                    let checkpoint = cursor.checkpoint();
                    let connector = cursor.peek().expect("checked connector").to_string();
                    cursor.consume(&connector)?;
                    if !coordinated_object_selector_phrase_starts(cursor) {
                        // The connector joins whatever the parent reads next.
                        cursor.rollback(checkpoint);
                        settle_comma_run(cursor, &mut frame);
                        ObjectQueryState::ReadPostnominalQualifier
                    } else {
                        if logical_properties {
                            cursor.require(!(connector.eq_ignore_ascii_case("and") && frame.logical_comma),
                                "an explicitly supported logical conjunction without ambiguous serial-comma scope")?;
                            frame.pending_logical_connector =
                                Some(if connector.eq_ignore_ascii_case("and") {
                                    super::ContextualListConnector::And
                                } else {
                                    super::ContextualListConnector::Or
                                });
                        }
                        frame.last_connector_is_exclusive = connector.eq_ignore_ascii_case("or");
                        if !frame.last_connector_is_exclusive {
                            frame.has_inclusive_connector = true;
                        }
                        frame.has_explicit_union = true;
                        // The connector closes any open comma run, when the
                        // run is one list.
                        if close_comma_run(cursor, &mut frame)? {
                            ObjectQueryState::ReadObjectIntersection
                        } else {
                            ObjectQueryState::ReadPostnominalQualifier
                        }
                    }
                } else {
                    settle_comma_run(cursor, &mut frame);
                    ObjectQueryState::ReadPostnominalQualifier
                }
            }
            ObjectQueryState::ReadAfterComma => {
                cursor.enter("selector.predicate.after-comma");
                if cursor.peek().is_some_and(|token| {
                    token.eq_ignore_ascii_case("or") || token.eq_ignore_ascii_case("and/or")
                }) {
                    let connector = cursor.peek().expect("checked connector").to_string();
                    cursor.consume(&connector)?;
                    frame.last_connector_is_exclusive = connector.eq_ignore_ascii_case("or");
                    if !frame.last_connector_is_exclusive {
                        frame.has_inclusive_connector = true;
                    }
                    frame.has_explicit_union = true;
                    // `, or` closes the comma run: the list is committed.
                    if !close_comma_run(cursor, &mut frame)? {
                        state = ObjectQueryState::ReadPostnominalQualifier;
                        continue;
                    }
                }
                cursor.require(
                    coordinated_object_selector_phrase_starts(cursor),
                    "an object predicate after the selector-list comma",
                )?;
                ObjectQueryState::ReadObjectIntersection
            }
            ObjectQueryState::ReadPostnominalQualifier => {
                cursor.enter("selector.predicate.postnominal-qualifier");
                if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("with"))
                {
                    let (predicate, scope) = if cursor.peek_ahead(1).is_some_and(|token| {
                        token.eq_ignore_ascii_case("mana")
                            || token.eq_ignore_ascii_case("power")
                            || token.eq_ignore_ascii_case("toughness")
                    }) {
                        (
                            recognize_numeric_comparison_qualifier(cursor, query_source_available)?,
                            PostnominalQualifierScope::WholeQuery,
                        )
                    } else if counter_qualifier_starts(cursor, 1) {
                        cursor.consume("with")?;
                        (
                            recognize_counter_qualifier(cursor)?,
                            PostnominalQualifierScope::LastCoordinatedHead,
                        )
                    } else {
                        (
                            recognize_keyword_qualifier(cursor)?,
                            PostnominalQualifierScope::LastCoordinatedHead,
                        )
                    };
                    frame
                        .postnominal_qualifiers
                        .push(PostnominalQualifier { predicate, scope });
                    ObjectQueryState::ReadPostnominalQualifier
                } else if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("without"))
                    && (counter_qualifier_starts(cursor, 1)
                        || cursor.peek_ahead(1).is_some_and(keyword_qualifier_word))
                {
                    // Other `without` phrases (`without paying its mana cost`)
                    // belong to the enclosing action, not to this query.
                    // `without flying`, `without a counter on it`: the
                    // negation of the same qualifier read after `with`.
                    cursor.enter("selector.without-qualifier");
                    let predicate = if counter_qualifier_starts(cursor, 1) {
                        cursor.consume("without")?;
                        recognize_counter_qualifier(cursor)?
                    } else {
                        recognize_keyword_qualifier_after(cursor, "without")?
                    };
                    frame.postnominal_qualifiers.push(PostnominalQualifier {
                        predicate: SelectorPredicate::Not(Box::new(predicate)),
                        scope: PostnominalQualifierScope::LastCoordinatedHead,
                    });
                    ObjectQueryState::ReadPostnominalQualifier
                } else if player_relation_subject_starts(cursor) {
                    // `you control`, `an opponent owns`, `you cast`, `you
                    // don't own`, `defending player controls`: the verb
                    // names the relation (CR 108.3, 108.4, 601.2).
                    cursor.enter("selector.player-relation");
                    let (verb, relation) = recognize_player_relation(cursor, None)?;
                    retain_player_relation(cursor, &mut frame, verb, relation)?;
                    ObjectQueryState::ReadPostnominalQualifier
                } else if let Some(inherited) = inherited_relation_after_but(cursor, &frame) {
                    // `you control but don't own`: a second relation of the
                    // same subject. A `but` followed by anything else is
                    // the parent's.
                    cursor.enter("selector.player-relation.but");
                    cursor.consume("but")?;
                    if inherited.is_some() {
                        let (verb, relation) = recognize_player_relation(cursor, inherited)?;
                        retain_player_relation(cursor, &mut frame, verb, relation)?;
                    }
                    ObjectQueryState::ReadPostnominalQualifier
                } else if let Some(length) = chosen_creature_type_qualifier_length(cursor) {
                    // `of the chosen type` / `of that [creature] type`: the
                    // creature type chosen earlier in the program (CR 608.2d).
                    cursor.enter("selector.chosen-creature-type-qualifier");
                    cursor.advance_to(cursor.position() + length)?;
                    frame.postnominal_qualifiers.push(PostnominalQualifier {
                        predicate: SelectorPredicate::ChosenCreatureType,
                        scope: PostnominalQualifierScope::WholeQuery,
                    });
                    ObjectQueryState::ReadPostnominalQualifier
                } else if let Some(length) = super::chosen_color_qualifier_length(cursor) {
                    // `of the chosen color` / `of that color`: the color
                    // chosen earlier in the program (CR 608.2d).
                    cursor.enter("selector.chosen-color-qualifier");
                    cursor.advance_to(cursor.position() + length)?;
                    frame.postnominal_qualifiers.push(PostnominalQualifier {
                        predicate: SelectorPredicate::ChosenColor,
                        scope: PostnominalQualifierScope::WholeQuery,
                    });
                    ObjectQueryState::ReadPostnominalQualifier
                } else if named_qualifier_width(cursor).is_some() {
                    // `creature named Fenric` (CR 201.2): the object's name.
                    let predicate = recognize_named_qualifier(cursor)?;
                    frame.postnominal_qualifiers.push(PostnominalQualifier {
                        predicate,
                        scope: PostnominalQualifierScope::WholeQuery,
                    });
                    ObjectQueryState::ReadPostnominalQualifier
                } else if super::relative_clause_starts_in(cursor, context.stack_targets) {
                    // `that was dealt damage this turn`, `that has a counter
                    // on it`, `that aren't enchanted`: the relative slot.
                    let predicate = super::recognize_relative_clause(cursor)?;
                    frame.postnominal_qualifiers.push(PostnominalQualifier {
                        predicate,
                        scope: PostnominalQualifierScope::WholeQuery,
                    });
                    ObjectQueryState::ReadPostnominalQualifier
                } else if cursor.peek().is_some_and(|token| {
                    token.eq_ignore_ascii_case("that's") || token.eq_ignore_ascii_case("that’s")
                }) {
                    let relative_word = cursor.peek().expect("checked relative word").to_string();
                    cursor.consume(&relative_word)?;
                    let relative = recognize_object_query_frame(
                        cursor,
                        ObjectQueryContext {
                            quota: context.quota,
                            source_available: query_source_available,
                            stack_targets: context.stack_targets,
                        },
                        true,
                    )?
                    .into_object_query(cursor)?;
                    cursor.require(
                        relative.controller.is_none(),
                        "a relative characteristic without another controller qualifier",
                    )?;
                    frame.postnominal_qualifiers.push(PostnominalQualifier {
                        predicate: relative.predicate,
                        scope: PostnominalQualifierScope::WholeQuery,
                    });
                    ObjectQueryState::ReadPostnominalQualifier
                } else {
                    ObjectQueryState::Complete
                }
            }
            ObjectQueryState::Complete => {
                cursor.enter("selector.predicate.complete");
                if let QueryQuota::ParentOwnedShared { exactly_one } = context.quota {
                    cursor.require(!frame.has_repeated_head_article || (exactly_one
                        && frame.last_connector_is_exclusive && !frame.has_inclusive_connector),
                        "coordinated alternatives with one shared quota, not independent headed quantities")?;
                }
                if !frame.has_explicit_union {
                    compose_comma_separated_negative_modifiers(&mut frame.union_branches);
                }
                // Coordinated modifiers sharing one head form a single headed
                // branch before postnominal qualifiers choose their scope.
                if !logical_properties
                    && !frame.has_repeated_head_article
                    && let Some(shared) = share_coordinated_trailing_head(
                        &frame.union_branches,
                        &frame.branch_has_plural_surface,
                    )
                {
                    frame.union_branches = vec![shared];
                }
                // CR 108.3: an ownership relative to the ability controller
                // is also the `ownedBy` characteristic of the same objects.
                // Both readings compose here, once; the parent's projection
                // chooses the one its consumer represents.
                let owned_predicate = match frame.relations.owner_scope() {
                    Some(
                        scope @ (QuerySubjectScope::AbilityController
                        | QuerySubjectScope::NotAbilityController),
                    ) => {
                        let mut qualifiers = frame.postnominal_qualifiers.clone();
                        let index = frame
                            .owner_qualifier_index
                            .unwrap_or(qualifiers.len())
                            .min(qualifiers.len());
                        qualifiers.insert(
                            index,
                            PostnominalQualifier {
                                predicate: SelectorPredicate::OwnedByAbilityController {
                                    negated: scope == QuerySubjectScope::NotAbilityController,
                                },
                                scope: PostnominalQualifierScope::WholeQuery,
                            },
                        );
                        Some(compose_query_predicate(
                            cursor,
                            frame.union_branches.clone(),
                            qualifiers,
                            frame.logical_connectors.clone(),
                            logical_properties,
                        )?)
                    }
                    _ => None,
                };
                let predicate = compose_query_predicate(
                    cursor,
                    frame.union_branches,
                    frame.postnominal_qualifiers,
                    frame.logical_connectors,
                    logical_properties,
                )?;
                return Ok(RecognizedObjectQuery {
                    predicate,
                    owned_predicate,
                    relations: frame.relations,
                });
            }
        };
    }
}

/// Composes the coordinated branches, their postnominal qualifiers and,
/// for a logical property list, its Boolean precedence into one predicate.
fn compose_query_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    mut branches: Vec<SelectorPredicate>,
    qualifiers: Vec<PostnominalQualifier>,
    logical_connectors: Vec<super::ContextualListConnector>,
    logical_properties: bool,
) -> MachineResult<SelectorPredicate> {
    let shared_qualifiers = compose_postnominal_scopes(cursor, &mut branches, qualifiers)?;
    let base = match branches.len() {
        0 => return Err(cursor.fail("a populated selector predicate")),
        1 => branches.pop().expect("one branch"),
        _ if logical_properties => {
            let atoms = branches
                .into_iter()
                .map(super::BooleanConditionNode::Atom)
                .collect();
            let composed = super::lower_boolean_precedence(cursor, atoms, logical_connectors)?;
            object_predicate_from_boolean(composed)
        }
        _ => lift_shared_trailing_domain(branches),
    };
    Ok(if shared_qualifiers.is_empty() {
        base
    } else {
        let mut operands = Vec::with_capacity(1 + shared_qualifiers.len());
        operands.push(base);
        operands.extend(shared_qualifiers);
        SelectorPredicate::Intersection(operands)
    })
}

/// Stores one read relation in its verb's slot. A second relation of the
/// same verb is refused; ownership remembers its qualifier position.
fn retain_player_relation(
    cursor: &mut OracleTokenCursor<'_>,
    frame: &mut ObjectQueryFrame,
    verb: RelationVerb,
    relation: ReadRelation,
) -> MachineResult<()> {
    let slot = frame.relations.slot_mut(verb);
    cursor.require(slot.is_none(), "at most one relation for each player role")?;
    *slot = Some(relation);
    if verb == RelationVerb::Own {
        frame.owner_qualifier_index = Some(frame.postnominal_qualifiers.len());
    }
    Ok(())
}

fn is_negation_word(raw: &str) -> bool {
    raw.eq_ignore_ascii_case("don't") || raw.eq_ignore_ascii_case("don\u{2019}t")
}

/// After a relation, `but` opens a second relation only when one follows:
/// `but don't own` / `but own` inherit the controller subject (`Some`),
/// `but an opponent owns` names its own subject (`None`). Pure lookahead.
fn inherited_relation_after_but(
    cursor: &OracleTokenCursor<'_>,
    frame: &ObjectQueryFrame,
) -> Option<Option<QuerySubjectScope>> {
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("but"))
        || frame.relations.is_empty()
    {
        return None;
    }
    let mut lookahead = cursor.clone();
    lookahead.consume("but").ok()?;
    if player_relation_subject_starts(&lookahead) {
        return Some(None);
    }
    let negated = lookahead.peek().is_some_and(is_negation_word);
    let verb = lookahead.peek_ahead(usize::from(negated))?;
    if !verb.eq_ignore_ascii_case("own") {
        return None;
    }
    // Only the ability controller has a written `don't` form; the inherited
    // subject is the controller relation's (`you control but don't own`).
    match frame.relations.controller_scope() {
        Some(QuerySubjectScope::AbilityController) if frame.relations.owner.is_none() => {
            Some(Some(QuerySubjectScope::AbilityController))
        }
        _ => None,
    }
}

/// `instant and sorcery card(s)`, `artifact and creature spells`: `and`
/// between bare card-type adjectives that share one following domain noun
/// names the objects having either type (CR 205.2a lists a card's types; no
/// object is both an instant and a sorcery). Only a singular, unheaded type
/// adjective before `and` and `TYPE NOUN` after it take this reading, so
/// separately headed branches (`instant cards and sorcery cards`, `creatures
/// and lands you control`) keep their `and` for the parent. Pure lookahead.
fn coordinated_type_adjective_and(
    cursor: &OracleTokenCursor<'_>,
    frame: &ObjectQueryFrame,
) -> bool {
    cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("and"))
        && matches!(
            frame.union_branches.last(),
            Some(SelectorPredicate::CardType(_))
        )
        && frame.branch_has_plural_surface.last() == Some(&false)
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| lookup_card_type(raw, AcceptedForms::SingularOnly).is_some())
        && cursor.peek_ahead(2).is_some_and(|raw| {
            crate::oracle::characteristic_vocabulary::lookup_object_domain(
                raw,
                AcceptedForms::SingularOrPlural,
            )
            .is_some()
        })
}

/// Whether a comma at the cursor continues the object list: another object
/// phrase follows, possibly after `or`/`and/or`. Pure lookahead.
fn comma_continues_object_list(cursor: &OracleTokenCursor<'_>) -> bool {
    let mut lookahead = cursor.clone();
    if lookahead.consume(",").is_err() {
        return false;
    }
    if let Some(connector) = lookahead.peek().map(str::to_string)
        && (connector.eq_ignore_ascii_case("or") || connector.eq_ignore_ascii_case("and/or"))
        && lookahead.consume(&connector).is_err()
    {
        return false;
    }
    coordinated_object_selector_phrase_starts(&lookahead)
}

/// Ends the coordination phase. An unclosed comma run is kept only when it
/// lists negative modifiers of one following head (`nonartifact, nonblack
/// creature`); any other run gives its commas back to the parent.
fn settle_comma_run(cursor: &mut OracleTokenCursor<'_>, frame: &mut ObjectQueryFrame) {
    settle_comma_run_with(cursor, frame, None);
}

/// `settle_comma_run` with the reason the list stopped, deferred to the
/// parent that reads the returned comma.
fn settle_comma_run_with(
    cursor: &mut OracleTokenCursor<'_>,
    frame: &mut ObjectQueryFrame,
    reason: Option<MachineFailure>,
) {
    let Some((checkpoint, snapshot)) = frame.comma_run.take() else {
        return;
    };
    let run_start = snapshot.union_branches.len().saturating_sub(1);
    if !modifiers_of_one_head(&frame.union_branches[run_start..]) {
        let reason = reason.unwrap_or_else(|| {
            cursor.fail("`or`/`and/or` closing the comma-separated object list")
        });
        cursor.rollback(checkpoint);
        *frame = *snapshot;
        cursor.defer_failure(reason);
    }
}

/// The failure of a list connector that no object phrase follows (`, or an
/// opponent controls`), read on a lookahead copy; None for any other stop.
fn dangling_list_connector_failure(cursor: &OracleTokenCursor<'_>) -> Option<MachineFailure> {
    let mut lookahead = cursor.clone();
    lookahead.consume(",").ok()?;
    let connector = lookahead.peek()?.to_string();
    if !(connector.eq_ignore_ascii_case("or") || connector.eq_ignore_ascii_case("and/or")) {
        return None;
    }
    lookahead.consume(&connector).ok()?;
    lookahead.enter("selector.predicate.after-comma");
    Some(lookahead.fail("an object predicate after the selector-list comma"))
}

/// One list's members share their number; an invariant noun agrees with
/// either.
fn numbers_agree(numbers: &[super::GrammaticalNumber]) -> bool {
    !(numbers.contains(&super::GrammaticalNumber::Singular)
        && numbers.contains(&super::GrammaticalNumber::Plural))
}

/// Negative modifiers sharing the run's last, headed branch (`nonartifact,
/// nonblack creature`). A run of modifiers alone (`noncreature, nonland`)
/// has no head to modify and is not this form.
fn modifiers_of_one_head(run: &[SelectorPredicate]) -> bool {
    let Some((last, preceding)) = run.split_last() else {
        return false;
    };
    !preceding.is_empty() && preceding.iter().all(negative_modifier) && !negative_modifier(last)
}

/// Gives an open comma run's commas back to the parent: the cursor and the
/// frame return to the first comma. False when no run is open.
fn abandon_comma_run(cursor: &mut OracleTokenCursor<'_>, frame: &mut ObjectQueryFrame) -> bool {
    let Some((checkpoint, snapshot)) = frame.comma_run.take().or_else(|| frame.closing_run.take())
    else {
        return false;
    };
    cursor.rollback(checkpoint);
    *frame = *snapshot;
    true
}

/// Closes an open comma run at `or`/`and/or`. The members of one list share
/// their grammatical number (`an artifact, creatures or planeswalkers you
/// control get` is a parent clause, not a list): otherwise the run is
/// abandoned and false is returned. `nonartifact, nonblack creature or
/// planeswalker` has no proved scope for its modifiers and is refused.
fn close_comma_run(
    cursor: &mut OracleTokenCursor<'_>,
    frame: &mut ObjectQueryFrame,
) -> MachineResult<bool> {
    let Some((_, snapshot)) = frame.comma_run.as_ref() else {
        return Ok(true);
    };
    let run_start = snapshot.union_branches.len().saturating_sub(1);
    // The explicit scope refusal comes first: a modifier list must not fall
    // back to its first bare modifier through the number check.
    cursor.require(
        !modifiers_of_one_head(&frame.union_branches[run_start..]),
        "a negative-modifier list whose head is not coordinated with another branch",
    )?;
    if !numbers_agree(&frame.branch_number[run_start..]) {
        let failure = cursor.fail("members of one object list sharing their grammatical number");
        abandon_comma_run(cursor, frame);
        cursor.defer_failure(failure);
        return Ok(false);
    }
    // The member after the connector is checked once it is read.
    frame.closing_run = frame.comma_run.take();
    Ok(true)
}

fn negative_modifier(predicate: &SelectorPredicate) -> bool {
    match predicate {
        // `nonattacking, nonblocking creature`: negated statuses too.
        SelectorPredicate::Not(operand)
            if matches!(operand.as_ref(), SelectorPredicate::Status(_)) =>
        {
            true
        }
        SelectorPredicate::Not(operand) => matches!(
            operand.as_ref(),
            SelectorPredicate::CardType(_)
                | SelectorPredicate::Color(_)
                | SelectorPredicate::ColorQuantity(_)
                | SelectorPredicate::Supertype(_)
                | SelectorPredicate::Subtype(_)
                | SelectorPredicate::CharacteristicBatch(_)
                | SelectorPredicate::IsToken
                | SelectorPredicate::IsCommander
        ),
        SelectorPredicate::Intersection(operands) => {
            !operands.is_empty() && operands.iter().all(negative_modifier)
        }
        _ => false,
    }
}

/// Commas between negative adjectives constrain one following nominal head.
/// Explicit `or`/`and/or` remain unions; a list without a positive head remains
/// unresolved here and is rejected by the caller's existing domain validation.
fn compose_comma_separated_negative_modifiers(branches: &mut Vec<SelectorPredicate>) {
    if branches.len() < 2 {
        return;
    }
    let (last, preceding) = branches.split_last().expect("multiple branches");
    if !preceding.iter().all(negative_modifier) {
        return;
    }
    let SelectorPredicate::Intersection(tail) = last else {
        return;
    };
    let Some((head, modifiers)) = tail.split_last() else {
        return;
    };
    if modifiers.is_empty()
        || !modifiers.iter().all(negative_modifier)
        || !matches!(
            head,
            SelectorPredicate::CardType(_)
                | SelectorPredicate::Subtype(_)
                | SelectorPredicate::ObjectKind(_)
        )
    {
        return;
    }
    let mut operands = Vec::new();
    for branch in std::mem::take(branches) {
        match branch {
            SelectorPredicate::Intersection(parts) => operands.extend(parts),
            atom => operands.push(atom),
        }
    }
    branches.push(SelectorPredicate::Intersection(operands));
}

/// AST construction only; Boolean precedence has one shared implementation.
fn object_predicate_from_boolean(
    node: super::BooleanConditionNode<ObjectPredicate>,
) -> ObjectPredicate {
    match node {
        super::BooleanConditionNode::Atom(atom) => atom,
        super::BooleanConditionNode::And(children) => ObjectPredicate::Intersection(
            children
                .into_iter()
                .map(object_predicate_from_boolean)
                .collect(),
        ),
        super::BooleanConditionNode::Or(children) => ObjectPredicate::Union(
            children
                .into_iter()
                .map(object_predicate_from_boolean)
                .collect(),
        ),
        super::BooleanConditionNode::Not(child) => {
            ObjectPredicate::Not(Box::new(object_predicate_from_boolean(*child)))
        }
    }
}

/// Numeric and relative qualifications retain their shared query scope.
/// A keyword after coordinated object heads qualifies the nearest head:
/// `artifact or creature with flying` keeps an unrestricted artifact branch.
/// A single head preserves the existing AST shape and qualifier order.
fn compose_postnominal_scopes(
    cursor: &mut OracleTokenCursor<'_>,
    branches: &mut [SelectorPredicate],
    qualifiers: Vec<PostnominalQualifier>,
) -> MachineResult<Vec<SelectorPredicate>> {
    if branches.len() <= 1 {
        return Ok(qualifiers.into_iter().map(|node| node.predicate).collect());
    }
    let (local, shared): (Vec<_>, Vec<_>) = qualifiers
        .into_iter()
        .partition(|node| node.scope == PostnominalQualifierScope::LastCoordinatedHead);
    if !local.is_empty() {
        cursor.enter("selector.predicate.qualifier-scope");
        cursor.require(
            branches.iter().all(SelectorPredicate::has_nominal_head),
            "explicit object heads in every keyword-qualified alternative",
        )?;
        let last = branches.last_mut().expect("multiple selector branches");
        let predicates = local.into_iter().map(|node| node.predicate);
        // Keep a postfix card/spell domain at the tail so the existing domain
        // composition can share that noun independently of the local keyword.
        if let SelectorPredicate::Intersection(operands) = last {
            if matches!(operands.last(), Some(SelectorPredicate::ObjectKind(_))) {
                let domain = operands.pop().expect("checked trailing domain");
                operands.extend(predicates);
                operands.push(domain);
                return Ok(shared.into_iter().map(|node| node.predicate).collect());
            }
        }
        let mut operands = vec![last.clone()];
        operands.extend(predicates);
        *last = SelectorPredicate::Intersection(operands);
    }
    Ok(shared.into_iter().map(|node| node.predicate).collect())
}

/// `with K [or K]*` holds when the object has any listed keyword;
/// `without K [or K]*` when it has none of them (`creatures without flying
/// or islandwalk`).
fn recognize_keyword_qualifier(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SelectorPredicate> {
    recognize_keyword_qualifier_after(cursor, "with")
}

/// The quantity phrase of a counter qualifier, `offset` tokens ahead:
/// its token length, minimum, maximum and whether the noun is singular.
fn counter_quantity_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(usize, i64, Option<i64>, bool)> {
    let raw = |at: usize| cursor.peek_ahead(offset + at).map(str::to_ascii_lowercase);
    let word = |at: usize, expected: &str| raw(at).as_deref() == Some(expected);
    let number = |at: usize| raw(at).as_deref().and_then(parse_counter_number);
    if word(0, "a") || word(0, "an") {
        return Some((1, 1, None, true));
    }
    if word(0, "no") {
        return Some((1, 0, Some(0), false));
    }
    if word(0, "fewer") && word(1, "than") {
        let value = number(2).filter(|value| *value >= 1)?;
        return Some((3, 0, Some(value - 1), false));
    }
    if let Some(value) = number(0) {
        if word(1, "or") && word(2, "more") && value >= 1 {
            return Some((3, value, None, false));
        }
        if word(1, "or") && (word(2, "fewer") || word(2, "less")) {
            return Some((3, 0, Some(value), false));
        }
        return None;
    }
    // A bare plural (`has counters on it`): one or more.
    (word(0, "counters") || raw(1).as_deref() == Some("counters")).then_some((0, 1, None, false))
}

fn parse_counter_number(raw: &str) -> Option<i64> {
    raw.parse::<i64>()
        .ok()
        .or_else(|| super::super::parse_number_word(raw))
}

/// True when a counter qualifier (`a +1/+1 counter on it`, `one or more
/// counters on them`, `three or more quest counters on it`, `no counters on
/// it`) begins `offset` tokens ahead. Lookahead only: the counter-name child
/// still owns the name and its diagnostics.
pub(in crate::oracle::canonical) fn counter_qualifier_starts(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> bool {
    let Some((length, _, _, _)) = counter_quantity_at(cursor, offset) else {
        return false;
    };
    let word = |at: usize, expected: &str| {
        cursor
            .peek_ahead(offset + at)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    (length..length + 4).any(|at| word(at, "counter") || word(at, "counters"))
}

/// `QUANTITY [KIND] counter(s) on it|them` (CR 122.1): the object has a
/// number of counters, of that kind when named, within the quantity's
/// bounds. The relation word (`with`/`without`/`has`) belongs to the caller.
pub(in crate::oracle::canonical) fn recognize_counter_qualifier(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SelectorPredicate> {
    cursor.enter("selector.counter-qualifier.determiner");
    let (length, minimum, maximum, singular) = counter_quantity_at(cursor, 0).ok_or_else(|| {
        cursor.fail("a counter quantity (`a`, `no`, `N or more`, `fewer than N`)")
    })?;
    for _ in 0..length {
        let raw = cursor.peek().unwrap_or_default().to_string();
        cursor.consume(&raw)?;
    }
    cursor.enter("selector.counter-qualifier.kind");
    let counter = if cursor.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case("counter") || raw.eq_ignore_ascii_case("counters")
    }) {
        None
    } else {
        Some(super::recognize_counter_name(cursor)?)
    };
    cursor.enter("selector.counter-qualifier.noun");
    cursor.consume(if singular { "counter" } else { "counters" })?;
    cursor.enter("selector.counter-qualifier.location");
    cursor.consume("on")?;
    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("it") => cursor.consume("it")?,
        Some("them") => cursor.consume("them")?,
        _ => return Err(cursor.fail("`it` or `them` after the counter location")),
    }
    Ok(SelectorPredicate::HasCounter(super::CounterPresence {
        counter,
        minimum,
        maximum,
    }))
}

/// A word that can begin a keyword qualifier (`flying`, `first strike`).
fn keyword_qualifier_word(raw: &str) -> bool {
    parameterless_keyword_from_oracle_word(raw).is_some()
        || raw.eq_ignore_ascii_case("double")
        || raw.eq_ignore_ascii_case("first")
}

fn recognize_keyword_qualifier_after(
    cursor: &mut OracleTokenCursor<'_>,
    relation: &str,
) -> MachineResult<SelectorPredicate> {
    cursor.enter("selector.keyword-qualifier.relation");
    cursor.consume(relation)?;
    cursor.enter("selector.keyword-qualifier.keyword");
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a supported keyword after `with`"))?;
    cursor.require(
        keyword_qualifier_word(raw),
        "a supported keyword after `with`",
    )?;
    let first = recognize_parameterless_keyword(cursor).map(SelectorPredicate::HasKeyword)?;
    // `with flying or reach`: a disjunction of keyword qualifiers on the same
    // head. `or` continues the list only before another keyword word, so a
    // following branch with its own head (`or creatures with ...`) is untouched.
    let mut alternatives = vec![first];
    while cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
        && cursor.peek_ahead(1).is_some_and(keyword_qualifier_word)
    {
        cursor.consume("or")?;
        cursor.enter("selector.keyword-qualifier.alternative");
        alternatives
            .push(recognize_parameterless_keyword(cursor).map(SelectorPredicate::HasKeyword)?);
    }
    let keywords = match alternatives.len() {
        1 => alternatives.remove(0),
        _ => SelectorPredicate::Union(alternatives),
    };
    Ok(keywords)
}

/// In Oracle phrases such as `artifact or enchantment spell`, the final
/// explicit domain scopes the complete union rather than only its nearest
/// branch. Keep the union of characteristics intact and lift that trailing
/// domain into a surrounding intersection.
/// Nominal heads that name objects by themselves: a branch containing one is
/// a complete coordinated noun phrase, not a prenominal modifier.
fn is_head_noun(predicate: &SelectorPredicate) -> bool {
    matches!(
        predicate,
        SelectorPredicate::CardType(_)
            | SelectorPredicate::ObjectKind(_)
            | SelectorPredicate::IsToken
            | SelectorPredicate::IsCommander
    )
}

/// `Ninja or Rogue creatures`, `white or blue creature`: coordinated
/// prenominal modifiers share the final branch's trailing head and its
/// postnominal qualifiers, so `and(or(Ninja, Rogue), Creature)`. A preceding
/// branch with its own head (`artifacts or creatures`, `creature or Vehicle`,
/// or a plural subtype noun such as `Rogues`) stays an independent branch.
fn share_coordinated_trailing_head(
    branches: &[SelectorPredicate],
    plural_surfaces: &[bool],
) -> Option<SelectorPredicate> {
    if branches.len() < 2 || branches.len() != plural_surfaces.len() {
        return None;
    }
    let (last, preceding) = branches.split_last()?;
    let SelectorPredicate::Intersection(last_operands) = last else {
        return None;
    };
    let head_start = last_operands.iter().position(is_head_noun)?;
    let (last_modifiers, shared_tail) = last_operands.split_at(head_start);
    if last_modifiers.is_empty() {
        return None;
    }
    let modifier_only = |branch: &SelectorPredicate| match branch {
        SelectorPredicate::Intersection(operands) => !operands.iter().any(is_head_noun),
        atom => !is_head_noun(atom),
    };
    if !preceding
        .iter()
        .zip(plural_surfaces)
        .all(|(branch, plural)| !plural && modifier_only(branch))
    {
        return None;
    }
    let last_branch = match last_modifiers {
        [only] => only.clone(),
        several => SelectorPredicate::Intersection(several.to_vec()),
    };
    let mut alternatives = preceding.to_vec();
    alternatives.push(last_branch);
    let mut operands = vec![SelectorPredicate::Union(alternatives)];
    operands.extend(shared_tail.iter().cloned());
    Some(SelectorPredicate::Intersection(operands))
}

pub(in crate::oracle::canonical) fn lift_shared_trailing_domain(
    branches: Vec<SelectorPredicate>,
) -> SelectorPredicate {
    let fallback = SelectorPredicate::Union(branches.clone());
    let Some((last, preceding)) = branches.split_last() else {
        return fallback;
    };
    let SelectorPredicate::Intersection(last_operands) = last else {
        return fallback;
    };
    let Some((SelectorPredicate::ObjectKind(domain), last_characteristics)) =
        last_operands.split_last()
    else {
        return fallback;
    };
    if last_characteristics.is_empty()
        || !preceding
            .iter()
            .chain(last_characteristics.iter())
            .all(|predicate| {
                // The trailing noun owns the domain of the coordination
                // (`Elf or Goblin permanent`): read the branches without the
                // CR 109.2 subtype-head inference.
                predicate.supports_domain(*domain) && !predicate.has_explicit_domain(*domain)
            })
    {
        return fallback;
    }

    let last_branch = match last_characteristics {
        [only] => only.clone(),
        several => SelectorPredicate::Intersection(several.to_vec()),
    };
    let mut union_branches = preceding.to_vec();
    union_branches.push(last_branch);
    SelectorPredicate::Intersection(vec![
        SelectorPredicate::Union(union_branches),
        SelectorPredicate::ObjectKind(*domain),
    ])
}

/// Recognizes the predicate body following a target head. Object grammar is
/// delegated to the same query child used by non-targeting set effects.
pub(in crate::oracle::canonical) fn recognize_target_slot_selector(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetSelector> {
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("any"))
    {
        cursor.enter("selector.predicate.start");
        cursor.consume_phrase(&["any", "target"])?;
        cursor.enter("selector.predicate.complete");
        return Ok(TargetSelector::Any);
    }
    if let Some(player) = cursor.peek().and_then(player_predicate) {
        cursor.enter("selector.predicate.start");
        cursor.advance_to(cursor.position() + 1)?;
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
        {
            cursor.enter("selector.predicate.cross-domain-union");
            cursor.consume("or")?;
            let second = if let Some(second_player) = cursor.peek().and_then(player_predicate) {
                cursor.advance_to(cursor.position() + 1)?;
                TargetSelector::Player(second_player)
            } else {
                let query = recognize_object_query(cursor)?;
                match query.controller {
                    Some(controller) => TargetSelector::ControlledObject {
                        predicate: query.predicate,
                        controller,
                    },
                    None => TargetSelector::Object(query.predicate),
                }
            };
            cursor.enter("selector.predicate.complete");
            return Ok(TargetSelector::Union(vec![
                TargetSelector::Player(player),
                second,
            ]));
        }
        cursor.enter("selector.predicate.complete");
        return Ok(TargetSelector::Player(player));
    }
    if stack_item_selector_starts(cursor) {
        cursor.enter("selector.predicate.start");
        let stack_item = recognize_stack_item_selector(cursor)?;
        cursor.enter("selector.predicate.complete");
        return Ok(TargetSelector::StackItem(stack_item));
    }

    let query = recognize_object_noun_query(cursor, ObjectQueryContext::DECLARED_TARGET)?
        .into_object_query(cursor)?;
    Ok(match query.controller {
        Some(controller) => TargetSelector::ControlledObject {
            predicate: query.predicate,
            controller,
        },
        None => TargetSelector::Object(query.predicate),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlayerRelationState {
    Subject,
    Opponent,
    Opponents,
    NegationOrVerb,
    Verb,
    Complete,
}

/// A player subject of a relation clause at the cursor: `you`, `an
/// opponent`, `your opponents`, or a bound player (`defending player
/// controls`). Dispatch hint only; the relation reader validates the clause.
fn player_relation_subject_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().is_some_and(|token| {
        token.eq_ignore_ascii_case("you")
            || token.eq_ignore_ascii_case("an")
            || token.eq_ignore_ascii_case("your")
    }) || bound_player_relation_starts(cursor)
}

/// Recognizes one relation clause after an object noun: a player subject
/// and the verb naming the relation (`you control`, `an opponent owns`,
/// `you cast`, `you don't own`). The relation is retained outside the
/// characteristic predicate so lowering can populate the generic controller,
/// caster or owner field.
fn recognize_player_relation(
    cursor: &mut OracleTokenCursor<'_>,
    inherited: Option<QuerySubjectScope>,
) -> MachineResult<(RelationVerb, ReadRelation)> {
    // An inherited subject (`you control but don't own`) starts at its
    // optional negation; only `you` is ever inherited.
    let mut state = if inherited.is_some() {
        PlayerRelationState::NegationOrVerb
    } else {
        PlayerRelationState::Subject
    };
    let mut scope = inherited;
    let mut third_person = false;
    let mut negated = false;
    let mut verb = None;
    let mut site = None;
    loop {
        state = match state {
            PlayerRelationState::Subject => {
                cursor.enter("selector.controller-qualifier.subject");
                if bound_player_relation_starts(cursor) {
                    // `defending player controls`, `that player owns`: the
                    // player is resolved by the binding environment (C2),
                    // never guessed by this query.
                    cursor.enter("selector.controller-qualifier.bound-player");
                    let environment = super::installed_binding_environment(cursor);
                    let player = super::recognize_bound_player_reference(cursor, &environment)?;
                    cursor.require(
                        player == super::BoundPlayerReference::EventPlayer,
                        "a player the triggering event records",
                    )?;
                    scope = Some(QuerySubjectScope::EventPlayer);
                    third_person = true;
                    PlayerRelationState::Verb
                } else if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
                {
                    cursor.consume("you")?;
                    scope = Some(QuerySubjectScope::AbilityController);
                    PlayerRelationState::NegationOrVerb
                } else if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
                {
                    cursor.consume("your")?;
                    PlayerRelationState::Opponents
                } else {
                    cursor.consume("an")?;
                    PlayerRelationState::Opponent
                }
            }
            PlayerRelationState::Opponent => {
                cursor.enter("selector.controller-qualifier.opponent");
                cursor.consume("opponent")?;
                scope = Some(QuerySubjectScope::OpponentOfAbilityController);
                third_person = true;
                PlayerRelationState::Verb
            }
            PlayerRelationState::Opponents => {
                cursor.enter("selector.controller-qualifier.opponents");
                cursor.consume("opponents")?;
                scope = Some(QuerySubjectScope::OpponentOfAbilityController);
                PlayerRelationState::Verb
            }
            PlayerRelationState::NegationOrVerb => {
                cursor.enter("selector.controller-qualifier.relation-or-negation");
                if cursor.peek().is_some_and(is_negation_word) {
                    let word = cursor.peek().expect("checked negation").to_string();
                    cursor.consume(&word)?;
                    scope = Some(QuerySubjectScope::NotAbilityController);
                    negated = true;
                }
                PlayerRelationState::Verb
            }
            PlayerRelationState::Verb => {
                // The states keep the names of the former controller reader
                // so a malformed verb reports where it always did.
                cursor.enter(if negated {
                    "selector.controller-qualifier.negated-relation"
                } else {
                    "selector.controller-qualifier.relation-or-negation"
                });
                let token_index = cursor.position();
                let token = cursor
                    .peek_token()
                    .cloned()
                    .ok_or_else(|| cursor.fail("a control, cast or own relation verb"))?;
                let read = RelationVerb::from_word(&token.raw, third_person)
                    .ok_or_else(|| cursor.fail(RelationVerb::Control.surface(third_person)))?;
                cursor.consume(&token.raw)?;
                site = Some(RelationSite {
                    token_index,
                    span_start: token.span_start,
                    span_end: token.span_end,
                    raw: token.raw,
                    negated,
                    third_person,
                });
                verb = Some(read);
                PlayerRelationState::Complete
            }
            PlayerRelationState::Complete => {
                cursor.enter("selector.controller-qualifier.complete");
                let scope = cursor.require_value(scope, "an object relation subject")?;
                let site = cursor.require_value(site, "a read relation verb")?;
                return Ok((
                    cursor.require_value(verb, "a relation verb")?,
                    ReadRelation { scope, site },
                ));
            }
        };
    }
}

fn object_controller_scope(scope: QuerySubjectScope) -> ObjectControllerScope {
    match scope {
        QuerySubjectScope::AbilityController => ObjectControllerScope::AbilityController,
        QuerySubjectScope::NotAbilityController => ObjectControllerScope::NotAbilityController,
        QuerySubjectScope::OpponentOfAbilityController => {
            ObjectControllerScope::OpponentOfAbilityController
        }
        QuerySubjectScope::EventPlayer => ObjectControllerScope::EventPlayer,
    }
}

fn object_ownership_scope(scope: QuerySubjectScope) -> ObjectOwnershipScope {
    match scope {
        QuerySubjectScope::AbilityController => ObjectOwnershipScope::AbilityController,
        QuerySubjectScope::NotAbilityController => ObjectOwnershipScope::NotAbilityController,
        QuerySubjectScope::OpponentOfAbilityController => {
            ObjectOwnershipScope::OpponentOfAbilityController
        }
        QuerySubjectScope::EventPlayer => ObjectOwnershipScope::EventPlayer,
    }
}

/// `defending player controls|owns` / `that player controls|owns`.
fn bound_player_relation_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    (word(0, "defending") || word(0, "that"))
        && word(1, "player")
        && (word(2, "controls") || word(2, "owns") || word(2, "casts"))
}

fn spell_caster_scope(scope: QuerySubjectScope) -> SpellCasterScope {
    match scope {
        QuerySubjectScope::AbilityController => SpellCasterScope::AbilityController,
        QuerySubjectScope::NotAbilityController => SpellCasterScope::NotAbilityController,
        QuerySubjectScope::OpponentOfAbilityController => {
            SpellCasterScope::OpponentOfAbilityController
        }
        QuerySubjectScope::EventPlayer => SpellCasterScope::EventPlayer,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NumericComparisonState {
    Relation,
    Property,
    Right,
    DirectionOrComplete,
    Direction,
    Complete,
}

/// Recognizes a reusable numeric property constraint attached to one selector.
/// The selector owns the predicate; effect machines only validate their domain.
fn recognize_numeric_comparison_qualifier(
    cursor: &mut OracleTokenCursor<'_>,
    query_source_available: Option<bool>,
) -> MachineResult<SelectorPredicate> {
    recognize_numeric_comparison_from(
        cursor,
        query_source_available,
        NumericComparisonState::Relation,
        None,
    )
}

/// One numeric object property (`mana value`, `power`, `toughness`). The
/// caller owns the relation that introduces it (`with`, `has`, `its`).
pub(in crate::oracle::canonical) fn recognize_selector_numeric_property(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SelectorNumericProperty> {
    Ok(
        match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
            Some("mana") => {
                cursor.consume_phrase(&["mana", "value"])?;
                SelectorNumericProperty::ManaValue
            }
            Some("power") => {
                cursor.consume("power")?;
                SelectorNumericProperty::Power
            }
            Some("toughness") => {
                cursor.consume("toughness")?;
                SelectorNumericProperty::Toughness
            }
            _ => return Err(cursor.fail("`mana value`, `power`, or `toughness`")),
        },
    )
}

/// The comparison after an already read property: `[equal to] N [or less |
/// or greater]`. It shares the selector qualifier's threshold grammar and its
/// literal/declared-X right-hand-side policy.
pub(in crate::oracle::canonical) fn recognize_numeric_property_comparison_tail(
    cursor: &mut OracleTokenCursor<'_>,
    property: SelectorNumericProperty,
) -> MachineResult<SelectorPredicate> {
    recognize_numeric_comparison_from(cursor, None, NumericComparisonState::Right, Some(property))
}

fn recognize_numeric_comparison_from(
    cursor: &mut OracleTokenCursor<'_>,
    query_source_available: Option<bool>,
    start: NumericComparisonState,
    initial_property: Option<SelectorNumericProperty>,
) -> MachineResult<SelectorPredicate> {
    let mut state = start;
    let mut property = initial_property;
    let mut right = None;
    let mut operator = NumericComparisonOperator::Equal;
    let mut strict = false;
    loop {
        state = match state {
            NumericComparisonState::Relation => {
                cursor.enter("selector.numeric-comparison.relation");
                cursor.consume("with")?;
                NumericComparisonState::Property
            }
            NumericComparisonState::Property => {
                cursor.enter("selector.numeric-comparison.property");
                property = Some(recognize_selector_numeric_property(cursor)?);
                NumericComparisonState::Right
            }
            NumericComparisonState::Right => {
                cursor.enter("selector.numeric-comparison.right");
                if cursor
                    .peek()
                    .is_some_and(|word| word.eq_ignore_ascii_case("equal"))
                {
                    cursor.consume_phrase(&["equal", "to"])?;
                } else if let Some(prefix_operator) = prefix_comparison_operator(cursor) {
                    // `less than EXPR` / `greater than EXPR` and their
                    // inclusive forms `less than or equal to EXPR`: the
                    // relation precedes its operand, so no postfix
                    // direction follows it.
                    operator = prefix_operator;
                    strict = true;
                }
                let expression = super::recognize_query_numeric_expression(cursor)?;
                // Syntax is shared. Card P/T has no checked AnyZone
                // characteristic evaluator in this new resolution consumer.
                // Do not route it to the historical printed-stat fallback.
                if query_source_available.is_some() {
                    cursor.enter("selector.numeric-comparison.property-context");
                    cursor.require(
                        property == Some(SelectorNumericProperty::ManaValue),
                        "mana value as the proved candidate property for this resolution-card query",
                    )?;
                }
                cursor.enter("selector.numeric-comparison.rhs-context");
                fn supported(
                    expression: &super::NumericExpression,
                    source_available: bool,
                ) -> bool {
                    use super::NumericExpression;
                    match expression {
                        NumericExpression::Atom(_) => true,
                        NumericExpression::CountCounters { .. } => source_available,
                        NumericExpression::ObjectProperty {
                            object: super::LocalObjectReference::SourceSelf,
                            ..
                        } => source_available,
                        NumericExpression::CardsInZone {
                            player: super::LocalPlayerReference::Controller,
                            zone: super::LocalZone::Library,
                        } => true,
                        // `the number of QUERY` over a zone-holder domain:
                        // admitted when its typed count has a runtime amount.
                        NumericExpression::CountQuery(query) => {
                            crate::oracle::canonical::effects::lower_scoped_object_query_amount(
                                query,
                            )
                            .is_some()
                        }
                        NumericExpression::Add { left, right }
                        | NumericExpression::Subtract { left, right }
                        | NumericExpression::Multiply { left, right }
                        | NumericExpression::Divide { left, right, .. } => {
                            supported(left, source_available) && supported(right, source_available)
                        }
                        _ => false,
                    }
                }
                let admitted = match query_source_available {
                    // Compatibility consumers have not proved contextual runtime
                    // expressions. Syntax is shared; their semantic slice stays
                    // literal/declared-X until their capability is migrated.
                    // The source's characteristic is bound by the target
                    // binder when targets are chosen and rechecked.
                    None => matches!(
                        expression,
                        super::NumericExpression::Atom(_)
                            | super::NumericExpression::ObjectProperty {
                                object: super::LocalObjectReference::SourceSelf,
                                ..
                            }
                    ),
                    Some(source_available) => supported(&expression, source_available),
                };
                cursor.require(admitted, "a query expression supported by this consumer's explicit numeric/source context")?;
                right = Some(Box::new(expression));
                if strict {
                    NumericComparisonState::Complete
                } else {
                    NumericComparisonState::DirectionOrComplete
                }
            }
            NumericComparisonState::DirectionOrComplete => {
                cursor.enter("selector.numeric-comparison.direction-or-complete");
                if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("or"))
                {
                    cursor.consume("or")?;
                    NumericComparisonState::Direction
                } else {
                    NumericComparisonState::Complete
                }
            }
            NumericComparisonState::Direction => {
                cursor.enter("selector.numeric-comparison.direction");
                operator = match cursor.peek() {
                    Some(direction) if direction.eq_ignore_ascii_case("less") => {
                        cursor.consume("less")?;
                        NumericComparisonOperator::LessThanOrEqual
                    }
                    Some(direction) if direction.eq_ignore_ascii_case("greater") => {
                        cursor.consume("greater")?;
                        NumericComparisonOperator::GreaterThanOrEqual
                    }
                    _ => return Err(cursor.fail("`less` or `greater`")),
                };
                NumericComparisonState::Complete
            }
            NumericComparisonState::Complete => {
                cursor.enter("selector.numeric-comparison.complete");
                return Ok(SelectorPredicate::NumericComparison {
                    left: cursor.require_value(property, "a populated numeric property")?,
                    operator,
                    right: cursor.require_value(right, "a populated numeric threshold")?,
                });
            }
        };
    }
}

/// `less than` / `greater than` before a comparison's right operand, and
/// the inclusive `less than or equal to` / `greater than or equal to`
/// (CR 107.1: the comparison of two numbers). The whole prefix is read or
/// nothing is consumed.
fn prefix_comparison_operator(
    cursor: &mut OracleTokenCursor<'_>,
) -> Option<NumericComparisonOperator> {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let less = word(0, "less");
    if !(less || word(0, "greater")) || !word(1, "than") {
        return None;
    }
    let inclusive = word(2, "or") && word(3, "equal") && word(4, "to");
    let operator = match (less, inclusive) {
        (true, false) => NumericComparisonOperator::LessThan,
        (false, false) => NumericComparisonOperator::GreaterThan,
        (true, true) => NumericComparisonOperator::LessThanOrEqual,
        (false, true) => NumericComparisonOperator::GreaterThanOrEqual,
    };
    let direction = cursor.peek()?.to_string();
    let mut phrase = vec![direction.as_str(), "than"];
    if inclusive {
        phrase.extend(["or", "equal", "to"]);
    }
    cursor.consume_phrase(&phrase).ok()?;
    Some(operator)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StackItemSelectorState {
    Branch,
    ConnectorOrComplete,
    Complete,
}

#[derive(Default)]
struct StackItemSelectorFrame {
    branches: Vec<StackItemPredicate>,
}

fn stack_item_selector_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case("activated")
            || raw.eq_ignore_ascii_case("triggered")
            || raw.eq_ignore_ascii_case("colorless")
            || raw.eq_ignore_ascii_case("ability")
            || (raw.eq_ignore_ascii_case("spell")
                && cursor
                    .peek_ahead(1)
                    .is_some_and(|s| s.eq_ignore_ascii_case("or"))
                && cursor.peek_ahead(2).is_some_and(|s| {
                    matches!(
                        s.to_ascii_lowercase().as_str(),
                        "ability" | "activated" | "triggered" | "colorless"
                    )
                }))
    })
}

/// Recognizes Boolean composition inside one heterogeneous stack target. The
/// target-list parent continues to own target cardinality and multiplicity.
fn recognize_stack_item_selector(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StackItemPredicate> {
    let mut state = StackItemSelectorState::Branch;
    let mut frame = StackItemSelectorFrame::default();
    loop {
        state = match state {
            StackItemSelectorState::Branch => {
                cursor.enter("selector.stack-item.branch");
                frame.branches.push(recognize_stack_item_branch(cursor)?);
                StackItemSelectorState::ConnectorOrComplete
            }
            StackItemSelectorState::ConnectorOrComplete => {
                cursor.enter("selector.stack-item.connector-or-complete");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
                {
                    cursor.consume("or")?;
                    StackItemSelectorState::Branch
                } else {
                    StackItemSelectorState::Complete
                }
            }
            StackItemSelectorState::Complete => {
                cursor.enter("selector.stack-item.complete");
                return match frame.branches.len() {
                    0 => Err(cursor.fail("a populated stack-item selector")),
                    1 => Ok(frame.branches.pop().expect("one stack-item branch")),
                    _ => Ok(StackItemPredicate::Union(frame.branches)),
                };
            }
        };
    }
}

fn recognize_stack_item_branch(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StackItemPredicate> {
    cursor.enter("selector.stack-item.branch-start");
    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("spell") => {
            cursor.consume("spell")?;
            Ok(StackItemPredicate::Spell)
        }
        Some("ability") => {
            cursor.consume("ability")?;
            Ok(StackItemPredicate::Union(vec![
                StackItemPredicate::Ability(StackAbilityKind::Activated),
                StackItemPredicate::Ability(StackAbilityKind::Triggered),
            ]))
        }
        Some("activated") | Some("triggered") => {
            let kind = if cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("activated"))
            {
                cursor.consume("activated")?;
                StackAbilityKind::Activated
            } else {
                cursor.consume("triggered")?;
                StackAbilityKind::Triggered
            };
            // `activated or triggered ability`: coordinated modifiers share
            // the final branch's `ability` head noun.
            let shares_following_head = cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
                && cursor.peek_ahead(1).is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("activated") || raw.eq_ignore_ascii_case("triggered")
                });
            if !shares_following_head {
                cursor.enter("selector.stack-item.ability-noun");
                cursor.consume("ability")?;
            }
            Ok(StackItemPredicate::Ability(kind))
        }
        Some("colorless") => {
            cursor.enter("selector.stack-item.colorless");
            cursor.consume("colorless")?;
            cursor.enter("selector.stack-item.spell-noun");
            cursor.consume("spell")?;
            Ok(StackItemPredicate::Intersection(vec![
                StackItemPredicate::Colorless,
                StackItemPredicate::Spell,
            ]))
        }
        _ => Err(cursor.fail("an activated/triggered ability or a colorless spell")),
    }
}

fn recognize_object_intersection(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SelectorPredicate> {
    let checkpoint = cursor.checkpoint();
    let mut predicates = Vec::new();

    while object_predicate_atom_starts(cursor, AcceptedForms::SingularOrPlural) {
        cursor.enter("selector.predicate.characteristic");
        predicates.push(recognize_object_predicate_atom(
            cursor,
            AcceptedForms::SingularOrPlural,
        )?);
    }

    match predicates.len() {
        0 => {
            cursor.rollback(checkpoint);
            Err(cursor.fail("one or more consecutive object types"))
        }
        1 => Ok(predicates.pop().expect("one predicate")),
        _ => Ok(SelectorPredicate::Intersection(predicates)),
    }
}

/// Recognizes a postfix card-domain selector such as `instant or sorcery
/// card`. The final `card` applies to the whole type union, not only its last
/// branch.
pub(in crate::oracle::canonical) fn recognize_card_type_union_selector(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectPredicate> {
    let mut types = Vec::new();
    loop {
        cursor.enter("selector.card-type-union.branch");
        let token = cursor
            .peek_token()
            .cloned()
            .ok_or_else(|| cursor.fail("a registered singular card type"))?;
        let card_type = lookup_card_type(&token.raw, AcceptedForms::SingularOnly)
            .ok_or_else(|| cursor.fail("a registered singular card type"))?;
        cursor.advance_to(cursor.position() + 1)?;
        types.push(ObjectPredicate::CardType(card_type));
        cursor.enter("selector.card-type-union.connector-or-domain");
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
        {
            cursor.consume("or")?;
            continue;
        }
        cursor.consume("card")?;
        break;
    }
    let types = match types.len() {
        0 => return Err(cursor.fail("at least one registered card type")),
        1 => types.pop().expect("one card type"),
        _ => ObjectPredicate::Union(types),
    };
    cursor.enter("selector.card-type-union.complete");
    Ok(ObjectPredicate::Intersection(vec![
        ObjectPredicate::ObjectKind(ObjectKind::Card),
        types,
    ]))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObjectTypeUnionListState {
    Branch,
    ConnectorOrComplete,
    AfterComma,
    FinalBranch,
    Complete,
}

fn recognize_object_type_list_branch(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectPredicate> {
    cursor.enter("selector.type-union-list.branch");
    let token = cursor
        .peek_token()
        .cloned()
        .ok_or_else(|| cursor.fail("a registered singular or plural card type"))?;
    let card_type = lookup_card_type(&token.raw, AcceptedForms::SingularOrPlural)
        .ok_or_else(|| cursor.fail("a registered singular or plural card type"))?;
    cursor.advance_to(cursor.position() + 1)?;
    Ok(ObjectPredicate::CardType(card_type))
}

/// Recognizes a collection type list such as `artifact, creature, and
/// enchantment`. The collection parent explicitly selects this grammar, so
/// `and` denotes union here without changing adjective intersections such as
/// `artifact creature` in the general object-query grammar.
pub(in crate::oracle::canonical) fn recognize_object_type_union_list(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectPredicate> {
    let mut state = ObjectTypeUnionListState::Branch;
    let mut branches = Vec::new();
    loop {
        state = match state {
            ObjectTypeUnionListState::Branch => {
                branches.push(recognize_object_type_list_branch(cursor)?);
                ObjectTypeUnionListState::ConnectorOrComplete
            }
            ObjectTypeUnionListState::ConnectorOrComplete => {
                cursor.enter("selector.type-union-list.connector-or-complete");
                match cursor.peek() {
                    Some(raw) if raw == "," => {
                        cursor.consume(",")?;
                        ObjectTypeUnionListState::AfterComma
                    }
                    Some(raw)
                        if raw.eq_ignore_ascii_case("and") || raw.eq_ignore_ascii_case("or") =>
                    {
                        let connector = raw.to_string();
                        cursor.consume(&connector)?;
                        ObjectTypeUnionListState::FinalBranch
                    }
                    _ => ObjectTypeUnionListState::Complete,
                }
            }
            ObjectTypeUnionListState::AfterComma => {
                cursor.enter("selector.type-union-list.after-comma");
                if cursor.peek().is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("and") || raw.eq_ignore_ascii_case("or")
                }) {
                    let connector = cursor.peek().expect("checked connector").to_string();
                    cursor.consume(&connector)?;
                    ObjectTypeUnionListState::FinalBranch
                } else {
                    ObjectTypeUnionListState::Branch
                }
            }
            ObjectTypeUnionListState::FinalBranch => {
                branches.push(recognize_object_type_list_branch(cursor)?);
                ObjectTypeUnionListState::Complete
            }
            ObjectTypeUnionListState::Complete => {
                cursor.enter("selector.type-union-list.complete");
                return match branches.len() {
                    0 => Err(cursor.fail("one or more object type branches")),
                    1 => Ok(branches.pop().expect("one object type branch")),
                    _ => Ok(ObjectPredicate::Union(branches)),
                };
            }
        };
    }
}

#[cfg(test)]
#[path = "selector_union_tests.rs"]
mod union_tests;

#[cfg(test)]
#[path = "selector_shared_head_tests.rs"]
mod shared_head_tests;

#[cfg(test)]
#[path = "selector_type_union_list_tests.rs"]
mod type_union_list_tests;

#[cfg(test)]
#[path = "selector_noncreature_subtype_tests.rs"]
mod noncreature_subtype_tests;

#[cfg(test)]
#[path = "selector_relative_qualifier_tests.rs"]
mod relative_qualifier_tests;

#[cfg(test)]
#[path = "selector_numeric_qualifier_tests.rs"]
mod numeric_qualifier_tests;

#[cfg(test)]
#[path = "selector_qualifier_scope_tests.rs"]
mod qualifier_scope_tests;

#[cfg(test)]
#[path = "search_card_selector_tests.rs"]
mod search_card_selector_tests;

#[cfg(test)]
#[path = "selector_state_qualifier_tests.rs"]
mod state_qualifier_tests;

fn player_predicate(raw: &str) -> Option<PlayerPredicate> {
    match lookup_player_role_any_number(raw)? {
        PlayerRole::Any => Some(PlayerPredicate::Any),
        PlayerRole::Opponent => Some(PlayerPredicate::Opponent),
    }
}

fn object_selector_phrase_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    object_predicate_atom_starts(cursor, AcceptedForms::SingularOrPlural)
}

/// A coordinated member may repeat its article. The first member's
/// determiner still belongs to its parent, and this lookahead never commits.
fn coordinated_object_selector_phrase_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let mut lookahead = cursor.clone();
    consume_optional_selector_article(&mut lookahead).is_ok()
        && object_selector_phrase_starts(&lookahead)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchCardSelectorState {
    Predicate,
    Subtype,
    CardNoun,
    QualifierOrComplete,
    BasicLandTypeQualifier,
    ExactManaCostQualifier,
    NumericQualifier,
    Complete,
}

/// `permanent card(s)` at the cursor: the shared card-category atom.
fn permanent_card_category_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("permanent"))
        && cursor.peek_ahead(1).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("card") || raw.eq_ignore_ascii_case("cards")
        })
}

fn search_subtype_starts(raw: Option<&str>) -> bool {
    raw.is_some_and(|raw| {
        raw.chars().next().is_some_and(char::is_uppercase) && raw.chars().any(char::is_alphabetic)
    })
}

fn consume_optional_selector_article(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
    {
        let article = cursor.peek().expect("checked article").to_string();
        cursor.consume(&article)?;
    }
    Ok(())
}

/// A prenominal characteristic adjective of a searched card (`colorless`,
/// `legendary`, `nonlegendary`, `historic`, `green`), read by the shared
/// atom machine. Type and subtype nouns keep their search-specific union
/// readers; this commits only for an adjective atom.
fn recognize_search_card_adjective(cursor: &mut OracleTokenCursor<'_>) -> Option<ObjectPredicate> {
    fn adjective(predicate: &ObjectPredicate) -> bool {
        match predicate {
            ObjectPredicate::Not(operand) => adjective(operand),
            ObjectPredicate::Color(_)
            | ObjectPredicate::ColorQuantity(_)
            | ObjectPredicate::CharacteristicBatch(_)
            | ObjectPredicate::Supertype(CardSupertype::Legendary) => true,
            _ => false,
        }
    }
    let mut trial = cursor.clone();
    let atom = recognize_object_predicate_atom(&mut trial, AcceptedForms::SingularOnly).ok()?;
    if !adjective(&atom) {
        return None;
    }
    *cursor = trial;
    Some(atom)
}

fn recognize_search_subtype(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<ObjectPredicate> {
    cursor.enter("selector.search-card.subtype");
    let mut words = Vec::new();
    while search_subtype_starts(cursor.peek()) {
        let word = cursor.peek().expect("checked subtype word").to_string();
        cursor.consume(&word)?;
        words.push(word);
    }
    if words.is_empty() {
        return Err(cursor.fail("a capitalized card subtype"));
    }
    Ok(ObjectPredicate::Subtype(words.join(" ")))
}

fn recognize_search_subtype_union(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectPredicate> {
    let mut branches = vec![recognize_search_subtype(cursor)?];
    loop {
        cursor.enter("selector.search-card.union");
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case(","))
        {
            cursor.consume(",")?;
            if cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
            {
                cursor.consume("or")?;
            }
        } else if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
        {
            cursor.consume("or")?;
        } else {
            break;
        }
        consume_optional_selector_article(cursor)?;
        branches.push(recognize_search_subtype(cursor)?);
    }
    Ok(match branches.len() {
        1 => branches.pop().expect("one subtype branch"),
        _ => ObjectPredicate::Union(branches),
    })
}

/// Recognizes Boolean `or` composition between card types while leaving
/// adjacent, unjoined card types to the caller as an intersection. This keeps
/// `artifact or enchantment card` distinct from multi-type requirements and
/// shares the same predicate AST with every search destination.
fn recognize_search_card_type_union(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectPredicate> {
    let mut branches = Vec::new();
    loop {
        cursor.enter("selector.search-card.card-type-branch");
        let card_type = cursor
            .peek()
            .and_then(|raw| lookup_card_type(raw, AcceptedForms::SingularOnly))
            .ok_or_else(|| cursor.fail("a singular card type"))?;
        let raw = cursor.peek().expect("checked card type").to_string();
        cursor.consume(&raw)?;
        branches.push(ObjectPredicate::CardType(card_type));

        cursor.enter("selector.search-card.card-type-connector-or-complete");
        let mut connected = false;
        if cursor.peek().is_some_and(|raw| raw == ",") {
            cursor.consume(",")?;
            if cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
            {
                cursor.consume("or")?;
            }
            connected = true;
        } else if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("or") || raw.eq_ignore_ascii_case("and/or"))
        {
            let connector = cursor.peek().expect("checked connector").to_string();
            cursor.consume(&connector)?;
            connected = true;
        } else if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
            && cursor
                .peek_ahead(1)
                .is_some_and(|raw| lookup_card_type(raw, AcceptedForms::SingularOnly).is_some())
        {
            // `instant and sorcery cards`: type adjectives sharing the one
            // card noun that follows name cards of either type. A headed
            // member (`a creature card and a land card`) ends at its noun
            // before any `and`, so it never reaches this connector.
            cursor.consume("and")?;
            connected = true;
        }
        if !connected {
            break;
        }
        consume_optional_selector_article(cursor)?;
    }
    Ok(match branches.len() {
        1 => branches.pop().expect("one card-type branch"),
        _ => ObjectPredicate::Union(branches),
    })
}

/// Recognizes a card filter used by a library-search grammar. Unlike a target
/// selector, this grammar distinguishes supertypes from card types and owns
/// the terminating `card` noun.
pub(in crate::oracle::canonical) fn recognize_search_card_selector(
    cursor: &mut OracleTokenCursor<'_>,
    plural: bool,
) -> MachineResult<ObjectPredicate> {
    let mut state = SearchCardSelectorState::Predicate;
    let mut predicates = Vec::new();
    loop {
        state = match state {
            SearchCardSelectorState::Predicate => {
                cursor.enter("selector.search-card.predicate");
                if predicates.is_empty()
                    && cursor.peek().is_some_and(|raw| {
                        raw.eq_ignore_ascii_case(if plural { "cards" } else { "card" })
                    })
                {
                    predicates.push(ObjectPredicate::ObjectKind(ObjectKind::Card));
                    SearchCardSelectorState::CardNoun
                } else if predicates.is_empty() && permanent_card_category_starts(cursor) {
                    SearchCardSelectorState::CardNoun
                } else if cursor
                    .peek()
                    .and_then(|raw| lookup_supertype(raw, AcceptedForms::SingularOnly))
                    == Some(CardSupertype::Basic)
                {
                    cursor.consume(CardSupertype::Basic.oracle_singular())?;
                    predicates.push(ObjectPredicate::Supertype(CardSupertype::Basic));
                    SearchCardSelectorState::Predicate
                } else if let Some(adjective) = recognize_search_card_adjective(cursor) {
                    predicates.push(adjective);
                    SearchCardSelectorState::Predicate
                } else if let Some(card_type) = cursor
                    .peek()
                    .and_then(|raw| lookup_card_type(raw, AcceptedForms::SingularOnly))
                {
                    let _ = card_type;
                    predicates.push(recognize_search_card_type_union(cursor)?);
                    SearchCardSelectorState::Predicate
                } else if search_subtype_starts(cursor.peek()) {
                    SearchCardSelectorState::Subtype
                } else if !predicates.is_empty() {
                    SearchCardSelectorState::CardNoun
                } else {
                    return Err(cursor.fail("a supported card supertype, card type, or subtype"));
                }
            }
            SearchCardSelectorState::Subtype => {
                predicates.push(recognize_search_subtype_union(cursor)?);
                SearchCardSelectorState::CardNoun
            }
            SearchCardSelectorState::CardNoun => {
                cursor.enter("selector.search-card.card-noun");
                // CR 110.4b: `permanent card` is the shared card category atom.
                if permanent_card_category_starts(cursor) {
                    predicates.push(super::recognize_object_predicate_atom(
                        cursor,
                        AcceptedForms::SingularOnly,
                    )?);
                }
                cursor.consume(if plural { "cards" } else { "card" })?;
                SearchCardSelectorState::QualifierOrComplete
            }
            SearchCardSelectorState::QualifierOrComplete => {
                cursor.enter("selector.search-card.qualifier-or-complete");
                if named_qualifier_width(cursor).is_some() {
                    // `a card named NAME [or NAME]` (CR 201.2): the shared
                    // name relation of the object query.
                    predicates.push(recognize_named_qualifier(cursor)?);
                    SearchCardSelectorState::QualifierOrComplete
                } else if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("with"))
                    && cursor.peek_ahead(1).is_some_and(keyword_qualifier_word)
                {
                    // `a card with flash`: the shared keyword qualifier.
                    cursor.enter("selector.search-card.keyword-qualifier");
                    predicates.push(recognize_keyword_qualifier(cursor)?);
                    SearchCardSelectorState::QualifierOrComplete
                } else if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("with"))
                {
                    if cursor
                        .peek_ahead(1)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("mana"))
                        && cursor
                            .peek_ahead(2)
                            .is_some_and(|raw| raw.eq_ignore_ascii_case("cost"))
                    {
                        SearchCardSelectorState::ExactManaCostQualifier
                    } else if cursor.peek_ahead(1).is_some_and(|raw| {
                        raw.eq_ignore_ascii_case("mana")
                            || raw.eq_ignore_ascii_case("power")
                            || raw.eq_ignore_ascii_case("toughness")
                    }) {
                        SearchCardSelectorState::NumericQualifier
                    } else {
                        SearchCardSelectorState::BasicLandTypeQualifier
                    }
                } else {
                    SearchCardSelectorState::Complete
                }
            }
            SearchCardSelectorState::BasicLandTypeQualifier => {
                cursor.enter("selector.search-card.basic-land-type");
                cursor.consume_phrase(&["with", "a", "basic", "land", "type"])?;
                cursor.require(
                    predicates.iter().any(|predicate| {
                        predicate == &ObjectPredicate::CardType(ObjectCardType::Land)
                    }),
                    "a land card before the basic-land-type qualifier",
                )?;
                predicates.push(ObjectPredicate::Union(
                    ["Plains", "Island", "Swamp", "Mountain", "Forest"]
                        .into_iter()
                        .map(|subtype| ObjectPredicate::Subtype(subtype.to_string()))
                        .collect(),
                ));
                SearchCardSelectorState::Complete
            }
            SearchCardSelectorState::NumericQualifier => {
                // The same numeric property comparison as every object query;
                // its consumer-independent literal/declared-X policy applies.
                cursor.enter("selector.search-card.numeric-qualifier");
                predicates.push(recognize_numeric_comparison_qualifier(cursor, None)?);
                SearchCardSelectorState::Complete
            }
            SearchCardSelectorState::ExactManaCostQualifier => {
                cursor.enter("selector.search-card.exact-mana-cost");
                cursor.consume_phrase(&["with", "mana", "cost"])?;
                let mut costs = vec![ObjectPredicate::ExactManaCost(recognize_mana_cost(cursor)?)];
                while cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
                {
                    cursor.consume("or")?;
                    costs.push(ObjectPredicate::ExactManaCost(recognize_mana_cost(cursor)?));
                }
                cursor.require(
                    costs.len() >= 2,
                    "two or more exact mana costs joined by `or`",
                )?;
                predicates.push(ObjectPredicate::Union(costs));
                SearchCardSelectorState::Complete
            }
            SearchCardSelectorState::Complete => {
                cursor.enter("selector.search-card.complete");
                let predicate = match predicates.len() {
                    0 => return Err(cursor.fail("a populated library-search card selector")),
                    1 => predicates.pop().expect("one card predicate"),
                    _ => ObjectPredicate::Intersection(predicates),
                };
                return Ok(predicate);
            }
        };
    }
}

/// Separately headed card phrases sharing one parent quantity: `a basic land
/// card or a Gate card`, `an instant card or a card with flash`, `up to two
/// basic land cards and/or Gate cards`. Each alternative is a complete card
/// phrase of the shared card selector, so its own qualifiers stay local to
/// it (`a basic Plains card or a creature card with mana value 1 or less`).
/// `and/or` joins alternatives of one plural quantity only; a singular `a X
/// and/or a Y` names two slots, which this union cannot express, and stays
/// with the parent. An alternative that does not read as a card phrase leaves
/// its connector unconsumed.
pub(in crate::oracle::canonical) fn recognize_search_card_alternatives(
    cursor: &mut OracleTokenCursor<'_>,
    plural: bool,
) -> MachineResult<ObjectPredicate> {
    let mut alternatives = vec![recognize_search_card_selector(cursor, plural)?];
    loop {
        cursor.enter("selector.search-card.alternative");
        let Some(connector) = cursor.peek().map(str::to_string) else {
            break;
        };
        let joins = connector.eq_ignore_ascii_case("or")
            || (plural && connector.eq_ignore_ascii_case("and/or"));
        if !joins {
            break;
        }
        let mut trial = cursor.clone();
        trial.consume(&connector)?;
        if consume_optional_selector_article(&mut trial).is_err() {
            break;
        }
        match recognize_search_card_selector(&mut trial, plural) {
            Ok(alternative) => {
                *cursor = trial;
                alternatives.push(alternative);
            }
            Err(_) => break,
        }
    }
    Ok(match alternatives.len() {
        1 => alternatives.pop().expect("one card phrase"),
        _ => ObjectPredicate::Union(alternatives),
    })
}

#[cfg(test)]
#[path = "selector_transactional_coordinator_tests.rs"]
mod transactional_coordinator_tests;

#[cfg(test)]
#[path = "selector_object_query_atoms_tests.rs"]
mod object_query_atoms_tests;

#[cfg(test)]
#[path = "selector_negative_modifier_tests.rs"]
mod negative_modifier_tests;

#[cfg(test)]
#[path = "selector_coordinated_article_tests.rs"]
mod coordinated_article_tests;

/// Token length of `named NAME` at the cursor: `named` and the name's words.
/// Pure lookahead; see [`card_name_width_at`] for the name's form.
fn named_qualifier_width(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("named"))
    {
        return None;
    }
    card_name_width_at(cursor, 1).map(|width| width + 1)
}

/// Token length of one card name `offset` tokens ahead (CR 201.2): one or
/// more capitalized words, possibly joined by lowercase `of`/`the` before
/// another capitalized word (`Monk of the Open Hand`), and an epithet after
/// a comma (`Jace, Arcane Strategist`, `Gideon, the Oathsworn`) when that
/// epithet ends the name at a list, clause or sentence boundary. Pure
/// lookahead.
fn card_name_width_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<usize> {
    let capitalized = |at: usize| {
        cursor.peek_ahead(at).is_some_and(|raw| {
            raw.chars().next().is_some_and(char::is_uppercase)
                && raw.chars().all(|character| {
                    character.is_alphanumeric()
                        || character == '\''
                        || character == '\u{2019}'
                        || character == '-'
                })
        })
    };
    let joiner = |at: usize| {
        cursor
            .peek_ahead(at)
            .is_some_and(|raw| raw == "of" || raw == "the")
    };
    // The end of a run of capitalized words and joiners starting at `at`.
    let run_end = |mut at: usize| {
        loop {
            if capitalized(at) {
                at += 1;
            } else if joiner(at) && capitalized(at + 1) {
                at += 2;
            } else {
                return at;
            }
        }
    };
    if !capitalized(offset) {
        return None;
    }
    let mut end = run_end(offset);
    // `NAME, EPITHET`: the epithet starts with a capitalized word or `the`
    // and a capitalized word, and is followed by a boundary, never by more
    // name-like words that would make the comma a list separator.
    if cursor.peek_ahead(end) == Some(",") {
        let epithet = end + 1 + usize::from(cursor.peek_ahead(end + 1) == Some("the"));
        if capitalized(epithet) {
            let epithet_end = run_end(epithet);
            let boundary = cursor.peek_ahead(epithet_end).is_none_or(|raw| {
                matches!(raw, "," | "." | ";")
                    || raw.eq_ignore_ascii_case("or")
                    || raw.eq_ignore_ascii_case("and")
                    || raw.eq_ignore_ascii_case("and/or")
            });
            if boundary {
                end = epithet_end;
            }
        }
    }
    Some(end - offset)
}

/// `named NAME [or NAME]*` (CR 201.2): the object's name is one of the
/// listed names. Another name follows `or` only when it does not begin an
/// object phrase of its own (`named Halvar, God of Battle or an Equipment
/// card` leaves `or an Equipment card` to the parent).
pub(in crate::oracle::canonical) fn recognize_named_qualifier(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SelectorPredicate> {
    cursor.enter("selector.named-qualifier");
    cursor.consume("named")?;
    let mut names = Vec::new();
    loop {
        cursor.enter("selector.named-qualifier.name");
        let width =
            card_name_width_at(cursor, 0).ok_or_else(|| cursor.fail("a capitalized card name"))?;
        let mut words = Vec::with_capacity(width);
        for _ in 0..width {
            let raw = cursor.peek().unwrap_or_default().to_string();
            cursor.consume(&raw)?;
            if raw == "," {
                // `Jace, Arcane Strategist`: the comma is part of the name.
                if let Some(last) = words.last_mut() {
                    *last = format!("{last},");
                }
            } else {
                words.push(raw);
            }
        }
        names.push(SelectorPredicate::Relation(super::ObjectRelation::Named(
            words.join(" "),
        )));
        let alternative = cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
            && card_name_width_at(cursor, 1).is_some()
            && {
                let mut lookahead = cursor.clone();
                lookahead.consume("or").is_ok() && !object_selector_phrase_starts(&lookahead)
            };
        if !alternative {
            break;
        }
        cursor.consume("or")?;
    }
    Ok(match names.len() {
        1 => names.pop().expect("one name"),
        _ => SelectorPredicate::Union(names),
    })
}

/// Token length of `of the chosen [creature] type` / `of that [creature] type`
/// at the cursor, the postnominal reference to a creature-type choice.
fn chosen_creature_type_qualifier_length(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if !word(0, "of") {
        return None;
    }
    let determiner = if word(1, "the") && word(2, "chosen") {
        3
    } else if word(1, "that") {
        2
    } else {
        return None;
    };
    let noun = determiner + usize::from(word(determiner, "creature"));
    word(noun, "type").then_some(noun + 1)
}

#[cfg(test)]
#[path = "selector_relation_lexicon_tests.rs"]
mod relation_lexicon_tests;

/// Lexical lookahead for `your commander(s)`.
pub(in crate::oracle::canonical) fn your_commander_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
        && cursor.peek_ahead(1).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("commander") || raw.eq_ignore_ascii_case("commanders")
        })
}

/// `your commander(s)` (CR 903.3): a commander the ability controller owns.
/// The designation and the ownership relation compose the shared predicate;
/// the parent supplies the object domain (a permanent you control, a card in
/// a zone) and its own player relation.
pub(in crate::oracle::canonical) fn recognize_your_commander(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SelectorPredicate> {
    cursor.enter("selector.your-commander");
    cursor.require(your_commander_starts(cursor), "`your commander`")?;
    let noun = cursor.peek_ahead(1).unwrap_or_default().to_string();
    cursor.consume_phrase(&["your", noun.as_str()])?;
    Ok(SelectorPredicate::Intersection(vec![
        SelectorPredicate::IsCommander,
        SelectorPredicate::OwnedByAbilityController { negated: false },
    ]))
}
