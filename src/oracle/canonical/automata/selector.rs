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
    /// Any other single player a controller relation names: a player the
    /// binding environment names or a player target this possessor declares.
    Player(RelatedPlayer),
}

/// The single player a controller relation names when it is neither the
/// ability controller nor a population relative to it (CR 108.4). The query
/// never resolves it: a bound player comes from the binding environment and a
/// declared player is a target of the action that reads the query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum RelatedPlayer {
    /// `that player controls`, `its controller controls`, `that creature's
    /// owner controls`: a player the binding environment names, never a new
    /// declaration (CR 108.3, 109.4).
    Bound(super::BoundPlayerReference),
    /// `target player controls` / `target opponent controls` (CR 115.1,
    /// 601.2c): one player target that the action reading this query
    /// declares. `binding` is that action's program position and `site` the
    /// token of `target`, so two possessors of one action stay distinct.
    DeclaredTarget {
        binding: super::PlayerBinding,
        opponent: bool,
        site: usize,
    },
}

/// Where a departure-history clause was read, with the history it states.
/// The objects it describes have left the battlefield, so only a consumer
/// counting those departures can represent it (CR 400.7, 700.4).
#[derive(Clone, Debug, PartialEq, Eq)]
struct ReadDeparture {
    history: super::DepartureHistory,
    site: RelationSite,
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

/// `except for commanders`, `except for Dragons and Wizards`: the listed
/// objects are not members of the set (CR 608.2b reads the set as stated).
/// The exclusion scopes the whole coordinated set it ends, as a relation
/// does; every projection composes `and(query, not(excluded))`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ReadExclusion {
    excluded: SelectorPredicate,
}

/// Every player relation one noun phrase states, one slot per relation
/// verb. The noun phrase reads them all; each parent type-checks which
/// relations it consumes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct QueryRelations {
    controller: Option<ReadRelation>,
    caster: Option<ReadRelation>,
    owner: Option<ReadRelation>,
    /// `that died this turn`, `put into your graveyard from the battlefield
    /// this turn`: the objects are departures of this turn, not present
    /// objects. Like a relation, it ends a coordinated set.
    departure: Option<ReadDeparture>,
    /// `other than this creature` after the noun (the source is not a
    /// member). Kept with the relations so that a parent reading the bare
    /// predicate after checking `is_empty` never drops it.
    source_exclusion: Option<RelationSite>,
    /// `except for LIST` after the set: kept with the relations for the
    /// same reason, and folded into the predicate by every projection.
    exclusion: Option<ReadExclusion>,
    /// `from your graveyard` after a cast relation (CR 601.2a): the zone the
    /// spells were cast from. Only a parent that records the cast zone takes
    /// it (`take_cast_origin`); every projection refuses one left in place.
    cast_origin: Option<ReadCastOrigin>,
}

/// The cast origin a cast relation states, composed as the shared spell
/// predicate it means (`CastFrom`, its negation, or their union), and where
/// it was read.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ReadCastOrigin {
    predicate: SelectorPredicate,
    site: RelationSite,
}

impl QueryRelations {
    fn slot_mut(&mut self, verb: RelationVerb) -> &mut Option<ReadRelation> {
        match verb {
            RelationVerb::Control => &mut self.controller,
            RelationVerb::Cast => &mut self.caster,
            RelationVerb::Own => &mut self.owner,
        }
    }

    /// Whether the phrase states no player relation, no departure history
    /// and no postnominal source exclusion.
    pub(in crate::oracle::canonical) fn is_empty(&self) -> bool {
        self.controller.is_none()
            && self.caster.is_none()
            && self.owner.is_none()
            && self.departure.is_none()
            && self.source_exclusion.is_none()
            && self.exclusion.is_none()
            && self.cast_origin.is_none()
    }

    /// The departure history the phrase states, if any.
    pub(in crate::oracle::canonical) fn departure(&self) -> Option<super::DepartureHistory> {
        self.departure.as_ref().map(|read| read.history)
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

/// A departure history the parent cannot represent fails at its verb: a
/// present-object consumer must not silently read departed objects as the
/// permanents now on the battlefield (CR 400.7).
fn refuse_departure(
    cursor: &mut OracleTokenCursor<'_>,
    departure: &ReadDeparture,
) -> super::super::MachineFailure {
    let site = &departure.site;
    cursor.enter("selector.departure-history.consumer");
    cursor.fail_with_span(
        "a consumer counting the objects that left the battlefield this turn",
        site.token_index,
        site.span_start,
        site.span_end,
        site.raw.clone(),
    )
}

/// A postnominal source exclusion (`other than this creature`) that no
/// parent took over fails at `other`: dropping it would widen the query.
fn refuse_source_exclusion(
    cursor: &mut OracleTokenCursor<'_>,
    site: &RelationSite,
) -> super::super::MachineFailure {
    cursor.enter("selector.exclusion.consumer");
    cursor.fail_with_span(
        "a parent that lowers the source exclusion of its object query",
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

    /// Takes over the postnominal source exclusion (`other than this
    /// creature`). A parent that lowers a source exclusion calls this before
    /// projecting; every projection refuses an exclusion left in place.
    pub(in crate::oracle::canonical) fn take_source_exclusion(&mut self) -> bool {
        self.relations.source_exclusion.take().is_some()
    }

    /// `predicate` without the objects an `except for LIST` exclusion names.
    fn excluding(&self, predicate: SelectorPredicate) -> SelectorPredicate {
        match &self.relations.exclusion {
            None => predicate,
            Some(read) => SelectorPredicate::Intersection(vec![
                predicate,
                SelectorPredicate::Not(Box::new(read.excluded.clone())),
            ]),
        }
    }

    /// Takes over the cast origin of a cast relation (`spells you cast from
    /// your graveyard`), as its spell predicate. A parent that records the
    /// zone a spell is cast from calls this before projecting.
    pub(in crate::oracle::canonical) fn take_cast_origin(&mut self) -> Option<SelectorPredicate> {
        self.relations.cast_origin.take().map(|read| read.predicate)
    }

    /// The projections' shared refusals: a departure history, a postnominal
    /// exclusion or a cast origin the projection cannot carry.
    fn refuse_unprojected(&self, cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
        if let Some(departure) = &self.relations.departure {
            return Err(refuse_departure(cursor, departure));
        }
        if let Some(site) = &self.relations.source_exclusion {
            return Err(refuse_source_exclusion(cursor, site));
        }
        if let Some(origin) = &self.relations.cast_origin {
            let site = &origin.site;
            cursor.enter("selector.cast-origin.consumer");
            return Err(cursor.fail_with_span(
                "a parent that records the zone its spells are cast from",
                site.token_index,
                site.span_start,
                site.span_end,
                site.raw.clone(),
            ));
        }
        Ok(())
    }

    /// Coordinates the noun phrases of one set (`artifacts and creatures you
    /// control`). The set composes the characteristic predicates; its player
    /// relations are the last branch's, since a relation ends a set. A
    /// postnominal exclusion of an earlier branch has no coordinated scope.
    pub(in crate::oracle::canonical) fn coordinate(
        branches: Vec<RecognizedObjectQuery>,
        compose: impl Fn(Vec<SelectorPredicate>) -> Option<SelectorPredicate>,
    ) -> Option<RecognizedObjectQuery> {
        let (last, preceding) = branches.split_last()?;
        if preceding.iter().any(|branch| {
            branch.relations.source_exclusion.is_some() || branch.relations.exclusion.is_some()
        }) {
            return None;
        }
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
        self.refuse_unprojected(cursor)?;
        if let Some(caster) = &self.relations.caster {
            return Err(refuse_relation(cursor, caster, RelationVerb::Control));
        }
        match &self.relations.owner {
            None => Ok(self.excluding(self.predicate.clone())),
            Some(owner) => match owner.scope {
                QuerySubjectScope::AbilityController | QuerySubjectScope::NotAbilityController => {
                    let owned = cursor.require_value(
                        self.owned_predicate.clone(),
                        "an ownership characteristic composed with the query",
                    )?;
                    Ok(self.excluding(owned))
                }
                QuerySubjectScope::OpponentOfAbilityController
                | QuerySubjectScope::EventPlayer
                | QuerySubjectScope::Player(_) => {
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
        self.refuse_unprojected(cursor)?;
        if let Some(relation) = self
            .relations
            .controller
            .as_ref()
            .or(self.relations.owner.as_ref())
        {
            return Err(refuse_relation(cursor, relation, RelationVerb::Cast));
        }
        Ok(super::SpellCastQueryNode {
            predicate: self.excluding(self.predicate.clone()),
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
        self.refuse_unprojected(cursor)?;
        if let Some(caster) = &self.relations.caster {
            return Err(refuse_relation(cursor, caster, RelationVerb::Control));
        }
        Ok(RelatedObjectQueryNode {
            objects: super::ObjectQueryNode {
                predicate: self.excluding(self.predicate.clone()),
                controller: self
                    .relations
                    .controller_scope()
                    .map(object_controller_scope),
                exclude_source: false,
            },
            ownership: self.relations.owner_scope().map(object_ownership_scope),
        })
    }

    /// The departures of this turn the phrase describes (`creatures that
    /// died this turn`, `nontoken creatures put into your graveyard from the
    /// battlefield this turn`), as the shared scoped query whose source set
    /// is the turn's departure records (CR 700.4: dying is being put into a
    /// graveyard from the battlefield). The objects were permanents; their
    /// characteristics are their last known ones (CR 603.10a). A present
    /// player relation (`you control`) would describe other objects and
    /// fails at its verb; `None` when the phrase states no departure.
    pub(in crate::oracle::canonical) fn into_departure_query(
        self,
        cursor: &mut OracleTokenCursor<'_>,
        exclude_source: bool,
    ) -> MachineResult<Option<super::ScopedObjectQueryNode>> {
        let Some(departure) = self.relations.departure.clone() else {
            return Ok(None);
        };
        if let Some(site) = &self.relations.source_exclusion {
            return Err(refuse_source_exclusion(cursor, site));
        }
        if let Some(relation) = self
            .relations
            .controller
            .as_ref()
            .or(self.relations.owner.as_ref())
            .or(self.relations.caster.as_ref())
        {
            cursor.enter("selector.departure-history.present-relation");
            let site = &relation.site;
            return Err(cursor.fail_with_span(
                "the departure's own player relation (`under your control`, `into your graveyard`)",
                site.token_index,
                site.span_start,
                site.span_end,
                site.raw.clone(),
            ));
        }
        cursor.enter("selector.departure-history.domain");
        cursor.require(
            self.predicate.is_in_domain(ObjectKind::Permanent),
            "departed objects named by a battlefield-permanent noun",
        )?;
        Ok(Some(super::ScopedObjectQueryNode {
            domain: ObjectKind::Permanent,
            objects: RelatedObjectQueryNode {
                objects: super::ObjectQueryNode {
                    predicate: self.excluding(self.predicate.clone()),
                    controller: None,
                    exclude_source,
                },
                ownership: None,
            },
            zones: Vec::new(),
            holders: None,
            departures: Some(departure.history),
        }))
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

/// The postnominal list of an object query continued after an item its
/// parent reads itself (CR 400.1 zone location `in your graveyard`, the
/// collection `from it`, a chooser `of their choice`): `a card from it with
/// mana value 4 or greater`, `target artifact card in your graveyard with
/// lesser mana value`. `head` is the predicate the parent read before that
/// item, `plural_head` the number of its noun. Each further item is read by
/// the same postnominal readers, in any order, until none starts; the
/// returned query is the head composed with them, with every relation they
/// state for the parent to type-check. `None` when no item follows, with the
/// cursor unchanged.
pub(in crate::oracle::canonical) fn recognize_postnominal_continuation(
    cursor: &mut OracleTokenCursor<'_>,
    context: ObjectQueryContext,
    head: SelectorPredicate,
    plural_head: bool,
) -> MachineResult<Option<RecognizedObjectQuery>> {
    let start = cursor.position();
    let continued =
        recognize_object_query_frame_from(cursor, context, false, Some((head, plural_head)))?;
    Ok((cursor.position() != start).then_some(continued))
}

/// The continuation of a parent's object predicate (see
/// [`recognize_postnominal_continuation`]) when the parent represents no
/// player relation: the composed predicate, or the head unchanged when no
/// item follows. A player relation read after the parent's own item (`in
/// your graveyard you control`) fails at its verb: a zone holder is not a
/// controller (CR 108.4, 404.2).
pub(in crate::oracle::canonical) fn continue_unrelated_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    context: ObjectQueryContext,
    head: SelectorPredicate,
    plural_head: bool,
) -> MachineResult<SelectorPredicate> {
    match recognize_postnominal_continuation(cursor, context, head.clone(), plural_head)? {
        None => Ok(head),
        Some(continued) => {
            continued.refuse_unprojected(cursor)?;
            if let Some(relation) = continued
                .relations
                .controller
                .as_ref()
                .or(continued.relations.owner.as_ref())
                .or(continued.relations.caster.as_ref())
            {
                cursor.enter("selector.postnominal-continuation.relation");
                let site = &relation.site;
                return Err(cursor.fail_with_span(
                    "a postnominal qualifier after the parent's own item, not a player relation",
                    site.token_index,
                    site.span_start,
                    site.span_end,
                    site.raw.clone(),
                ));
            }
            Ok(continued.excluding(continued.predicate.clone()))
        }
    }
}

/// Whether `predicate` coordinates branches that each name their own head
/// (`artifact or creature`, `Equipment or Aura`), at its top or under the
/// shared trailing domain.
fn coordinates_headed_branches(predicate: &SelectorPredicate) -> bool {
    let headed_union = |operand: &SelectorPredicate| {
        matches!(operand, SelectorPredicate::Union(branches)
            if branches.len() > 1 && branches.iter().all(SelectorPredicate::has_nominal_head))
    };
    match predicate {
        SelectorPredicate::Intersection(operands) => operands.iter().any(headed_union),
        other => headed_union(other),
    }
}

fn recognize_object_query_frame(
    cursor: &mut OracleTokenCursor<'_>,
    context: ObjectQueryContext,
    logical_properties: bool,
) -> MachineResult<RecognizedObjectQuery> {
    recognize_object_query_frame_from(cursor, context, logical_properties, None)
}

fn recognize_object_query_frame_from(
    cursor: &mut OracleTokenCursor<'_>,
    context: ObjectQueryContext,
    logical_properties: bool,
    continued_head: Option<(SelectorPredicate, bool)>,
) -> MachineResult<RecognizedObjectQuery> {
    let query_source_available = context.source_available;
    let mut state = ObjectQueryState::Start;
    let mut frame = ObjectQueryFrame::default();
    // A continued head that coordinates separately headed branches (`an
    // artifact or creature of their choice`) gives a head-local qualifier no
    // branch to attach to: it is refused rather than hoisted over every
    // branch (the coordinated-head scope rule).
    let continued_coordinated = continued_head
        .as_ref()
        .is_some_and(|(head, _)| coordinates_headed_branches(head));
    if let Some((head, plural)) = continued_head {
        // A continued list: the head was read before the parent's own item,
        // so only postnominal items remain to be read.
        frame.union_branches.push(head);
        frame.branch_has_plural_surface.push(plural);
        frame.branch_number.push(if plural {
            super::GrammaticalNumber::Plural
        } else {
            super::GrammaticalNumber::Singular
        });
        state = ObjectQueryState::ReadPostnominalQualifier;
    }

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
                if let Some(length) = super::chosen_card_name_qualifier_length(cursor) {
                    // `with the chosen name` / `with that name`: the card
                    // name chosen earlier in the program (CR 201.2, 608.2d).
                    // Read before the keyword and counter `with` readers.
                    cursor.enter("selector.chosen-card-name-qualifier");
                    cursor.advance_to(cursor.position() + length)?;
                    frame.postnominal_qualifiers.push(PostnominalQualifier {
                        predicate: SelectorPredicate::Relation(super::ObjectRelation::ChosenName),
                        scope: PostnominalQualifierScope::WholeQuery,
                    });
                    ObjectQueryState::ReadPostnominalQualifier
                } else if let Some(form) = qualifiers::comparative_with_form(cursor) {
                    // `with greater power`, `with equal or lesser mana value
                    // than that creature`, `with that spell's mana value`,
                    // `with the same name as that creature`: the candidate
                    // compared with an anchor object (CR 107.1, 201.2). Read
                    // before the keyword reader, so `with` + comparative
                    // never reads as a keyword.
                    let predicate = qualifiers::recognize_comparative_with(
                        cursor,
                        form,
                        context.comparison_admission(),
                    )?;
                    frame.postnominal_qualifiers.push(PostnominalQualifier {
                        predicate,
                        scope: PostnominalQualifierScope::WholeQuery,
                    });
                    ObjectQueryState::ReadPostnominalQualifier
                } else if let Some(form) = qualifiers::with_qualifier_form(cursor) {
                    // `with different names`, `with total mana value 8 or
                    // less`, `with the greatest power among ...`, `with {X}
                    // in its mana cost`, `with cycling`: the qualifier atoms
                    // read beside the printed characteristics.
                    let plural_head = frame.branch_has_plural_surface.last() == Some(&true);
                    let predicate =
                        qualifiers::recognize_with_qualifier(cursor, form, plural_head)?;
                    let scope = if form == qualifiers::WithQualifierForm::KeywordAbility {
                        PostnominalQualifierScope::LastCoordinatedHead
                    } else {
                        PostnominalQualifierScope::WholeQuery
                    };
                    frame
                        .postnominal_qualifiers
                        .push(PostnominalQualifier { predicate, scope });
                    ObjectQueryState::ReadPostnominalQualifier
                } else if qualifiers::damage_history_participle_starts(cursor) {
                    // `a creature dealt damage by this creature this turn`:
                    // the object's damage history of this turn (CR 120.1).
                    let predicate = qualifiers::recognize_damage_history_participle(cursor)?;
                    frame.postnominal_qualifiers.push(PostnominalQualifier {
                        predicate,
                        scope: PostnominalQualifierScope::WholeQuery,
                    });
                    ObjectQueryState::ReadPostnominalQualifier
                } else if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("with"))
                {
                    let (predicate, scope) = if cursor.peek_ahead(1).is_some_and(|token| {
                        token.eq_ignore_ascii_case("mana")
                            || token.eq_ignore_ascii_case("power")
                            || token.eq_ignore_ascii_case("toughness")
                    }) {
                        (
                            recognize_numeric_comparison_qualifier(
                                cursor,
                                context.comparison_admission(),
                            )?,
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
                } else if frame.relations.caster.is_some()
                    && frame.relations.cast_origin.is_none()
                    && cast_origin_starts(cursor)
                {
                    // `spells you cast from your graveyard`, `from anywhere
                    // other than your hand` (CR 601.2a): where the related
                    // caster cast them. Any other `from` is the parent's.
                    cursor.enter("selector.cast-origin");
                    let site = relation_site_at(cursor, 0)?;
                    let predicate = recognize_cast_origin(cursor)?;
                    frame.relations.cast_origin = Some(ReadCastOrigin { predicate, site });
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
                } else if negated_named_qualifier_starts(cursor) {
                    // `target permanent not named Clever Conjurer` (CR
                    // 201.2): the negation of the same name qualifier.
                    cursor.enter("selector.named-qualifier.negated");
                    cursor.consume("not")?;
                    let predicate = recognize_named_qualifier(cursor)?;
                    frame.postnominal_qualifiers.push(PostnominalQualifier {
                        predicate: SelectorPredicate::Not(Box::new(predicate)),
                        scope: PostnominalQualifierScope::WholeQuery,
                    });
                    ObjectQueryState::ReadPostnominalQualifier
                } else if let Some(clause) = super::departure_clause_at(cursor) {
                    // `that died this turn`, `put into your graveyard from
                    // the battlefield this turn`: the objects are departures
                    // of this turn (CR 700.4). Only a consumer counting them
                    // projects the phrase; every other one fails at the verb.
                    cursor.enter("selector.departure-history");
                    let site = relation_site_at(cursor, clause.verb_offset)?;
                    cursor.require(
                        frame.relations.departure.is_none(),
                        "one departure history for one object query",
                    )?;
                    cursor.advance_to(cursor.position() + clause.width)?;
                    frame.relations.departure = Some(ReadDeparture {
                        history: clause.history,
                        site,
                    });
                    ObjectQueryState::ReadPostnominalQualifier
                } else if source_exclusion_clause_starts(cursor) {
                    // `other than this creature` (CR 109.5): the source is
                    // not a member. A parent that lowers the exclusion takes
                    // it; every projection refuses one left in place.
                    cursor.enter("selector.exclusion.source");
                    let site = relation_site_at(cursor, 0)?;
                    cursor.consume_phrase(&["other", "than"])?;
                    let names = cursor.source_names().to_vec();
                    super::recognize_source_reference(cursor, &names, None)?;
                    cursor.require(
                        frame.relations.source_exclusion.is_none(),
                        "one source exclusion for one object query",
                    )?;
                    frame.relations.source_exclusion = Some(site);
                    ObjectQueryState::ReadPostnominalQualifier
                } else if except_for_starts(cursor) && frame.relations.exclusion.is_none() {
                    // `except for commanders`: a transactional child. A
                    // tail it cannot read (`except for its name` of a copy)
                    // stays with the parent, with this reason recorded.
                    let mut trial = cursor.clone();
                    match recognize_except_for_list(&mut trial, context) {
                        Ok((excluded, source))
                            if source.is_none() || frame.relations.source_exclusion.is_none() =>
                        {
                            *cursor = trial;
                            frame.relations.exclusion = excluded;
                            if source.is_some() {
                                frame.relations.source_exclusion = source;
                            }
                            ObjectQueryState::ReadPostnominalQualifier
                        }
                        Ok(_) => {
                            cursor.defer_failure(
                                cursor.fail("one source exclusion for one object query"),
                            );
                            ObjectQueryState::Complete
                        }
                        Err(failure) => {
                            cursor.defer_failure(failure);
                            ObjectQueryState::Complete
                        }
                    }
                } else if let Some(negated) = copular_characteristic_starts(cursor) {
                    // `that isn't a Dragon`, `that are artifacts`, `that
                    // aren't of the chosen type`: the copula relates the
                    // object to characteristics of the same query grammar,
                    // as `that's` does.
                    cursor.enter("selector.relative-clause.copula-characteristic");
                    let pronoun = cursor.peek().expect("checked relative pronoun").to_string();
                    cursor.consume(&pronoun)?;
                    let copula = cursor.peek().expect("checked copula").to_string();
                    cursor.consume(&copula)?;
                    if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("not"))
                    {
                        cursor.consume("not")?;
                    }
                    let characteristic =
                        if let Some(length) = chosen_creature_type_qualifier_length(cursor) {
                            cursor.advance_to(cursor.position() + length)?;
                            SelectorPredicate::ChosenCreatureType
                        } else {
                            consume_optional_selector_article(cursor)?;
                            let relative = recognize_object_query_frame(
                                cursor,
                                ObjectQueryContext {
                                    // A characteristic complement states no
                                    // quantity: its articles are not quotas.
                                    quota: QueryQuota::ExistingQuery,
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
                            relative.predicate
                        };
                    frame.postnominal_qualifiers.push(PostnominalQualifier {
                        predicate: if negated {
                            SelectorPredicate::Not(Box::new(characteristic))
                        } else {
                            characteristic
                        },
                        scope: PostnominalQualifierScope::WholeQuery,
                    });
                    ObjectQueryState::ReadPostnominalQualifier
                } else if super::shared_characteristic_set_clause_starts(cursor) {
                    // `two target creature cards that share a creature type`
                    // (CR 205.3, 601.2c): a share among the members of the
                    // chosen set, with no anchor. It constrains the set, so
                    // only a plural head states it; a consumer that does not
                    // choose a set refuses the constraint (its validator, or
                    // `split_set_constraints`), never reading it per object.
                    cursor.enter("selector.relative-clause.share-set.plural-head");
                    cursor.require(
                        frame.branch_has_plural_surface.last() == Some(&true),
                        "a plural set whose members share the characteristic",
                    )?;
                    let characteristic = super::recognize_shared_characteristic_set_clause(cursor)?;
                    frame.postnominal_qualifiers.push(PostnominalQualifier {
                        predicate: SelectorPredicate::Qualifier(
                            super::ObjectQualifier::SharedValue(characteristic),
                        ),
                        scope: PostnominalQualifierScope::WholeQuery,
                    });
                    ObjectQueryState::ReadPostnominalQualifier
                } else if let Some((width, opponent)) = super::same_controller_set_at(cursor) {
                    // `two target creatures controlled by the same player`
                    // (CR 108.4, 601.2c): every chosen permanent has one
                    // controller, a constraint over the chosen set that only
                    // a plural head states. `the same opponent` also relates
                    // each candidate to an opponent of the ability
                    // controller, the ordinary control relation.
                    cursor.enter("selector.relative-clause.same-controller.plural-head");
                    cursor.require(
                        frame.branch_has_plural_surface.last() == Some(&true),
                        "a plural set whose members share their controller",
                    )?;
                    let site = relation_site_at(cursor, 0)?;
                    cursor.advance_to(cursor.position() + width)?;
                    frame.postnominal_qualifiers.push(PostnominalQualifier {
                        predicate: SelectorPredicate::Qualifier(
                            super::ObjectQualifier::SameController,
                        ),
                        scope: PostnominalQualifierScope::WholeQuery,
                    });
                    if opponent {
                        retain_player_relation(
                            cursor,
                            &mut frame,
                            RelationVerb::Control,
                            ReadRelation {
                                scope: QuerySubjectScope::OpponentOfAbilityController,
                                site,
                            },
                        )?;
                    }
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
                    if let Some(width) = color_count_complement_at(cursor) {
                        // `a permanent that's one or more colors` (CR
                        // 105.2): the object is not colorless, the negation
                        // of the shared color-quantity atom.
                        cursor.enter("selector.relative-clause.color-count");
                        cursor.advance_to(cursor.position() + width)?;
                        frame.postnominal_qualifiers.push(PostnominalQualifier {
                            predicate: SelectorPredicate::Not(Box::new(
                                SelectorPredicate::ColorQuantity(super::ColorQuantity::Colorless),
                            )),
                            scope: PostnominalQualifierScope::WholeQuery,
                        });
                        state = ObjectQueryState::ReadPostnominalQualifier;
                        continue;
                    }
                    // `that's both green and blue`: `both` announces the
                    // conjunction of exactly two complements (CR 105.2: an
                    // object with each of the colors).
                    let both = cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("both"));
                    if both {
                        cursor.enter("selector.relative-clause.both");
                        cursor.consume("both")?;
                    }
                    // `that's a Wolf or a Werewolf`: the copula's complement
                    // takes its article, as after `that is`; each further
                    // member repeats it inside the logical list.
                    cursor.enter("selector.relative-clause.contracted-copula");
                    consume_optional_selector_article(cursor)?;
                    let relative = recognize_object_query_frame(
                        cursor,
                        ObjectQueryContext {
                            quota: QueryQuota::ExistingQuery,
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
                    if both {
                        cursor.enter("selector.relative-clause.both.conjunction");
                        cursor.require(
                            matches!(&relative.predicate, SelectorPredicate::Intersection(members) if members.len() == 2),
                            "two complements joined by `and` after `both`",
                        )?;
                    }
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
                if continued_coordinated
                    && frame
                        .postnominal_qualifiers
                        .iter()
                        .any(|node| node.scope == PostnominalQualifierScope::LastCoordinatedHead)
                {
                    cursor.enter("selector.postnominal-continuation.coordinated-head");
                    return Err(cursor.fail(
                        "a qualifier of one head before the parent's item, not after coordinated heads",
                    ));
                }
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
    // A related player (`target opponent`, `its controller`) is read for
    // control only (CR 108.4); its ownership and casting readings have no
    // representation yet and fail at their verb.
    if matches!(relation.scope, QuerySubjectScope::Player(_)) && verb != RelationVerb::Control {
        return Err(refuse_relation(cursor, &relation, RelationVerb::Control));
    }
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
    recognize_target_slot_selector_with(cursor, false).map(|(selector, _)| selector)
}

/// The target slot selector for a parent that represents the slot's source
/// exclusion (`target creature other than this creature`, CR 109.5, 115.1):
/// the postnominal exclusion of the object query is returned beside the
/// selector instead of failing at `other`, so the slot excludes the source
/// as `another target creature` does.
pub(in crate::oracle::canonical) fn recognize_target_slot_selector_excluding_source(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(TargetSelector, bool)> {
    recognize_target_slot_selector_with(cursor, true)
}

fn recognize_target_slot_selector_with(
    cursor: &mut OracleTokenCursor<'_>,
    source_exclusion: bool,
) -> MachineResult<(TargetSelector, bool)> {
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("any"))
    {
        cursor.enter("selector.predicate.start");
        cursor.consume_phrase(&["any", "target"])?;
        cursor.enter("selector.predicate.complete");
        return Ok((TargetSelector::Any, false));
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
            return Ok((
                TargetSelector::Union(vec![TargetSelector::Player(player), second]),
                false,
            ));
        }
        cursor.enter("selector.predicate.complete");
        return Ok((TargetSelector::Player(player), false));
    }
    if stack_item_selector_starts(cursor) {
        cursor.enter("selector.predicate.start");
        let stack_item = recognize_stack_item_selector(cursor)?;
        cursor.enter("selector.predicate.complete");
        return Ok((TargetSelector::StackItem(stack_item), false));
    }

    let mut recognized = recognize_object_noun_query(cursor, ObjectQueryContext::DECLARED_TARGET)?;
    let excluded = source_exclusion && recognized.take_source_exclusion();
    let query = recognized.into_object_query(cursor)?;
    Ok((
        match query.controller {
            Some(controller) => TargetSelector::ControlledObject {
                predicate: query.predicate,
                controller,
            },
            None => TargetSelector::Object(query.predicate),
        },
        excluded,
    ))
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
/// opponent`, `your opponents`, a bound player (`defending player
/// controls`, `its controller controls`) or a declared player (`target
/// opponent controls`). Dispatch hint only; the relation reader validates
/// the clause.
fn player_relation_subject_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().is_some_and(|token| {
        token.eq_ignore_ascii_case("you")
            || token.eq_ignore_ascii_case("an")
            || token.eq_ignore_ascii_case("your")
    }) || bound_player_relation_starts(cursor)
        || declared_player_relation_starts(cursor)
        || they_relation_scope(cursor).is_some()
}

/// `they control|own|cast` after an object noun: the plural pronoun of the
/// player `that player` would name (CR 608.2c), with the base verb form.
/// The relation is claimed only when the binding environment names that
/// player without a declaration of this program: the player the triggering
/// event records, when no player of the program is a nearer antecedent.
/// Otherwise the pronoun stays with the parent (a superlative population
/// `among creatures they control` reads its own candidate controller, a
/// performer's restated `they` its own player), whose diagnostic stands.
fn they_relation_scope(cursor: &OracleTokenCursor<'_>) -> Option<QuerySubjectScope> {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if !(word(0, "they") && (word(1, "control") || word(1, "own") || word(1, "cast"))) {
        return None;
    }
    let environment = super::installed_binding_environment(cursor);
    // The distributive `they` of an action whose player slot names it
    // (`each player loses 1 life for each creature they control`): the
    // nearest antecedent, the player of the action being read.
    if environment.acting_player {
        return Some(QuerySubjectScope::Player(RelatedPlayer::Bound(
            super::BoundPlayerReference::ActingPlayer,
        )));
    }
    (environment.player.is_none()
        && environment.player_target.is_none()
        && environment.frame.event_player)
        .then_some(QuerySubjectScope::EventPlayer)
}

/// `target player controls` / `target opponent controls`: a possessor
/// that declares one player target. Pure lookahead; `another target
/// player` and other determiners are not this subject.
fn declared_player_relation_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    word(0, "target")
        && (word(1, "player") || word(1, "opponent"))
        && (word(2, "controls") || word(2, "owns") || word(2, "casts"))
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
                if let Some(player) = they_relation_scope(cursor) {
                    // `Swamps they control`: the event's player, with the
                    // plural verb agreement of the pronoun.
                    cursor.enter("selector.controller-qualifier.they");
                    cursor.consume("they")?;
                    scope = Some(player);
                    PlayerRelationState::Verb
                } else if bound_player_relation_starts(cursor) {
                    // `defending player controls`, `that player owns`, `its
                    // controller controls`: the player is resolved by the
                    // binding environment (C2), never guessed by this query.
                    cursor.enter("selector.controller-qualifier.bound-player");
                    let environment = super::installed_binding_environment(cursor);
                    let player = super::recognize_bound_player_reference(cursor, &environment)?;
                    scope = Some(match player {
                        super::BoundPlayerReference::EventPlayer => QuerySubjectScope::EventPlayer,
                        other => {
                            // The query is lowered without the program's
                            // target table. `that player` after an earlier
                            // player declaration of the program (CR 608.2c)
                            // lowers to a placeholder the program lowering
                            // resolves against its declared players
                            // (`resolve_bound_player_anchors`); any other
                            // player that needs a declaration stays refused.
                            cursor.enter("selector.controller-qualifier.bound-player.lowering");
                            cursor.require(
                                matches!(other, super::BoundPlayerReference::Bound(_))
                                    || crate::oracle::canonical::effects::bound_player_reference_value(
                                        other,
                                        &[],
                                        &[],
                                    )
                                    .is_some(),
                                "a bound player resolvable without a declaration of this program",
                            )?;
                            QuerySubjectScope::Player(RelatedPlayer::Bound(other))
                        }
                    });
                    third_person = true;
                    PlayerRelationState::Verb
                } else if declared_player_relation_starts(cursor) {
                    // `target opponent controls` (CR 115.1, 601.2c): the
                    // action reading this query declares the player target.
                    cursor.enter("selector.controller-qualifier.declared-player");
                    let site = cursor.position();
                    cursor.consume("target")?;
                    let opponent = cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("opponent"));
                    cursor.consume(if opponent { "opponent" } else { "player" })?;
                    cursor.enter("selector.controller-qualifier.declared-player.binding");
                    let binding = cursor.require_value(
                        super::installed_binding_environment(cursor).effect_index,
                        "an action of an effect program that declares the possessor's player target",
                    )?;
                    scope = Some(QuerySubjectScope::Player(RelatedPlayer::DeclaredTarget {
                        binding: super::PlayerBinding(binding),
                        opponent,
                        site,
                    }));
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
        QuerySubjectScope::Player(player) => ObjectControllerScope::Player(player),
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
        QuerySubjectScope::Player(_) => {
            unreachable!("a related player is refused for ownership when its relation is read")
        }
    }
}

/// `defending player controls|owns`, `that player controls|owns`, `its
/// controller controls`, `that creature's owner controls`: a player head the
/// binding environment resolves, followed by a third-person relation verb.
fn bound_player_relation_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let head = if (word(0, "defending") || word(0, "that")) && word(1, "player") {
        2
    } else if let Some(length) = super::object_related_player_head_length(cursor) {
        length
    } else {
        return false;
    };
    word(head, "controls") || word(head, "owns") || word(head, "casts")
}

/// A token of a clause the query read, `offset` tokens ahead, as the site a
/// consumer that cannot represent the clause reports.
fn relation_site_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> MachineResult<RelationSite> {
    let token_index = cursor.position() + offset;
    let token = cursor
        .token_slice(token_index, token_index + 1)
        .first()
        .ok_or_else(|| cursor.fail("a token of the read clause"))?;
    Ok(RelationSite {
        token_index,
        span_start: token.span_start,
        span_end: token.span_end,
        raw: token.raw.clone(),
        negated: false,
        third_person: false,
    })
}

/// `from ORIGIN [or from ORIGIN]` after a cast relation, when the whole
/// origin reads. Pure lookahead: a `from` the origin grammar does not read
/// (`from among them`) stays with the parent.
fn cast_origin_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("from"))
    {
        return false;
    }
    let mut trial = cursor.clone();
    recognize_cast_origin(&mut trial).is_ok()
}

/// `from ORIGIN ("or" "from" ORIGIN)*` (CR 601.2a), where
///
/// ```text
/// ORIGIN := "your" ("hand" | "graveyard" | "library")   the caster's zone
///         | "exile"                                     the shared zone
///         | "anywhere" "other" "than" "your" ZONE        its negation
/// ```
///
/// Alternatives form a union of the same atom; the negated form is the
/// negation of the caster-owned zone (`anywhere other than your hand`).
fn recognize_cast_origin(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<SelectorPredicate> {
    fn word(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    }
    let mut alternatives = Vec::new();
    loop {
        cursor.enter("selector.cast-origin.from");
        cursor.consume("from")?;
        cursor.enter("selector.cast-origin.zone");
        let negated = word(cursor, 0, "anywhere");
        if negated {
            cursor.consume_phrase(&["anywhere", "other", "than"])?;
        }
        let caster_owned = word(cursor, 0, "your");
        if caster_owned {
            cursor.consume("your")?;
        }
        let raw = cursor
            .peek()
            .ok_or_else(|| cursor.fail("a zone a spell is cast from"))?
            .to_string();
        let zone = cursor.require_value(
            super::CastOriginZone::from_word(&raw),
            "`your hand`, `your graveyard`, `your library` or `exile`",
        )?;
        // Exile is one shared zone (CR 406.1); a personal zone names its
        // holder, here the caster (`your`).
        cursor.require(
            caster_owned == (zone != super::CastOriginZone::Exile),
            "`your` before a personal zone and a bare `exile`",
        )?;
        cursor.advance_to(cursor.position() + 1)?;
        let atom =
            SelectorPredicate::Relation(super::ObjectRelation::CastFrom(super::CastOrigin {
                zone,
                caster_owned,
            }));
        alternatives.push(if negated {
            SelectorPredicate::Not(Box::new(atom))
        } else {
            atom
        });
        if word(cursor, 0, "or") && word(cursor, 1, "from") {
            cursor.enter("selector.cast-origin.alternative");
            cursor.consume("or")?;
            continue;
        }
        break;
    }
    cursor.enter("selector.cast-origin.complete");
    cursor.require(
        alternatives.len() == 1
            || alternatives
                .iter()
                .all(|alternative| !matches!(alternative, SelectorPredicate::Not(_))),
        "positive alternatives of a cast origin",
    )?;
    Ok(match alternatives.len() {
        1 => alternatives.pop().expect("one cast origin"),
        _ => SelectorPredicate::Union(alternatives),
    })
}

/// `other than this creature` / `other than NAME` when the name links to
/// the source. Pure lookahead: another reference is not claimed.
fn source_exclusion_clause_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let mut trial = cursor.clone();
    if trial.consume_phrase(&["other", "than"]).is_err() {
        return false;
    }
    let names = trial.source_names().to_vec();
    super::recognize_source_reference(&mut trial, &names, None).is_ok()
}

/// `except for` after an object set. Lexical lookahead only.
fn except_for_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("except"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("for"))
}

/// The width of a list connector at the cursor (`,`, `and`, `or`, `, and`,
/// `, or`) when another object phrase follows it. Lexical lookahead only.
fn exclusion_list_continues(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let mut width = 0;
    if cursor.peek_ahead(width).is_some_and(|raw| raw == ",") {
        width += 1;
    }
    if cursor
        .peek_ahead(width)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("and") || raw.eq_ignore_ascii_case("or"))
    {
        width += 1;
    }
    if width == 0 {
        return None;
    }
    let mut lookahead = cursor.clone();
    lookahead.advance_to(cursor.position() + width).ok()?;
    coordinated_object_selector_phrase_starts(&lookahead).then_some(width)
}

/// `except for ITEM[, ITEM][,] (and|or) ITEM`: the objects excluded from the
/// set. Each item is a characteristic phrase of the shared object query, or
/// the source itself (`except for Mageta the Lion`), which is the query's
/// source exclusion (CR 109.5). An item with its own player relation, a
/// departure or another source exclusion has no representation inside the
/// exclusion and fails. An empty list fails.
fn recognize_except_for_list(
    cursor: &mut OracleTokenCursor<'_>,
    context: ObjectQueryContext,
) -> MachineResult<(Option<ReadExclusion>, Option<RelationSite>)> {
    cursor.enter("selector.exclusion.except-for");
    cursor.consume_phrase(&["except", "for"])?;
    let mut items = Vec::new();
    let mut source = None;
    loop {
        cursor.enter("selector.exclusion.except-for.source");
        let names = cursor.source_names().to_vec();
        let mut trial = cursor.clone();
        if source.is_none() && super::recognize_source_reference(&mut trial, &names, None).is_ok() {
            source = Some(relation_site_at(cursor, 0)?);
            *cursor = trial;
            cursor.enter("selector.exclusion.except-for.connector");
            let Some(width) = exclusion_list_continues(cursor) else {
                break;
            };
            cursor.advance_to(cursor.position() + width)?;
            continue;
        }
        cursor.enter("selector.exclusion.except-for.item");
        cursor.require(
            coordinated_object_selector_phrase_starts(cursor),
            "an excluded object phrase after `except for`",
        )?;
        consume_optional_selector_article(cursor)?;
        let item = recognize_object_query_frame(
            cursor,
            ObjectQueryContext {
                quota: QueryQuota::ExistingQuery,
                source_available: context.source_available,
                stack_targets: false,
            },
            false,
        )?;
        cursor.enter("selector.exclusion.except-for.item-relation");
        cursor.require(
            item.relations.is_empty(),
            "an excluded characteristic without its own player relation",
        )?;
        items.push(item.predicate);
        cursor.enter("selector.exclusion.except-for.connector");
        let Some(width) = exclusion_list_continues(cursor) else {
            break;
        };
        cursor.advance_to(cursor.position() + width)?;
    }
    let excluded = match items.len() {
        0 => None,
        1 => items.pop(),
        _ => Some(SelectorPredicate::Union(items)),
    };
    Ok((excluded.map(|excluded| ReadExclusion { excluded }), source))
}

/// `that|which is|are|isn't|aren't [not] [a|an] CHARACTERISTIC` and `...
/// of the chosen type`: a characteristic relative clause. Returns whether
/// the copula negates it. A status (`that are tapped`) keeps the relative
/// clause's own reading. Pure lookahead.
fn copular_characteristic_starts(cursor: &OracleTokenCursor<'_>) -> Option<bool> {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if !(word(0, "that") || word(0, "which")) {
        return None;
    }
    let copula = cursor
        .peek_ahead(1)?
        .to_ascii_lowercase()
        .replace('\u{2019}', "'");
    let mut negated = match copula.as_str() {
        "is" | "are" => false,
        "isn't" | "aren't" => true,
        _ => return None,
    };
    let mut at = 2;
    if word(at, "not") {
        negated = !negated;
        at += 1;
    }
    if cursor
        .peek_ahead(at)
        .is_some_and(|raw| super::lookup_status_predicate(raw).is_some())
    {
        return None;
    }
    let mut lookahead = cursor.clone();
    lookahead.advance_to(cursor.position() + at).ok()?;
    if chosen_creature_type_qualifier_length(&lookahead).is_some() {
        return Some(negated);
    }
    consume_optional_selector_article(&mut lookahead).ok()?;
    object_selector_phrase_starts(&lookahead).then_some(negated)
}

/// `one or more colors` after a copula, with its width: the object has at
/// least one color (CR 105.2). Pure lookahead.
fn color_count_complement_at(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let phrase = ["one", "or", "more", "colors"];
    phrase
        .iter()
        .enumerate()
        .all(|(index, expected)| {
            cursor
                .peek_ahead(index)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
        })
        .then_some(phrase.len())
}

fn spell_caster_scope(scope: QuerySubjectScope) -> SpellCasterScope {
    match scope {
        QuerySubjectScope::AbilityController => SpellCasterScope::AbilityController,
        QuerySubjectScope::NotAbilityController => SpellCasterScope::NotAbilityController,
        QuerySubjectScope::OpponentOfAbilityController => {
            SpellCasterScope::OpponentOfAbilityController
        }
        QuerySubjectScope::EventPlayer => SpellCasterScope::EventPlayer,
        QuerySubjectScope::Player(_) => {
            unreachable!("a related player is refused for casting when its relation is read")
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NumericComparisonState {
    Relation,
    Property,
    PropertyAlternative,
    Right,
    DirectionOrComplete,
    Direction,
    Complete,
}

/// Which right-hand sides of a candidate comparison its consumer can
/// evaluate (CR 107.1). The grammar of the operand is shared; the consumer
/// states when the operand is evaluated, and only an operand with an
/// evaluator at that moment is admitted. Anything else fails at
/// `selector.numeric-comparison.rhs-context`, never as a false comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ComparisonRhsAdmission {
    /// A consumer that compares each candidate with a value fixed by the
    /// rule text: a literal, the declared `X`, or the source's own power or
    /// toughness, which the target binder reads.
    Literal,
    /// Declared target candidates (CR 115.1, 601.2c, 608.2b): an amount that
    /// does not depend on the candidate and is relative to the ability
    /// controller, or a count of the source's own counters, is bound by the
    /// engine each time the candidates are enumerated, at declaration and
    /// when the target is rechecked.
    DeclaredTarget,
    /// A resolution-owned query, prepared with the resolving ability before
    /// its candidates are enumerated. `source_available` proves the source's
    /// own counters and characteristics; a candidate property other than
    /// mana value has no checked evaluator in these consumers.
    Resolution { source_available: bool },
    /// Cards chosen at resolution from a zone (a library search, CR 701.23):
    /// the operand is prepared with the resolving ability, and the compared
    /// property is a printed characteristic of a card outside the
    /// battlefield (CR 109.3), so power and toughness are readable too.
    CardResolution,
}

impl ObjectQueryContext {
    fn comparison_admission(self) -> ComparisonRhsAdmission {
        match self.source_available {
            Some(source_available) => ComparisonRhsAdmission::Resolution { source_available },
            None if self.stack_targets => ComparisonRhsAdmission::DeclaredTarget,
            None => ComparisonRhsAdmission::Literal,
        }
    }
}

/// An amount that depends neither on the candidate nor on a declaration and
/// is relative to the ability controller: a count of its (or every)
/// permanents or cards, a fact about the controller, devotion, an opponent
/// count, and arithmetic over them. Every such amount has a lowering whose
/// players are the ability controller (or every player).
fn controller_relative_amount(expression: &super::NumericExpression) -> bool {
    use super::NumericExpression;
    match expression {
        // A declared `X` inside an amount has no value where the amount is
        // bound; alone, it is a literal threshold (`ComparisonRhsAdmission`).
        NumericExpression::Atom(super::NumericAtom::Integer(_)) => true,
        NumericExpression::PermanentCount(count) => {
            !count.exclude_source
                && count.measure != super::PermanentCountMeasure::DistinctColors
                && matches!(
                    count.controller,
                    None | Some(
                        ObjectControllerScope::AbilityController
                            | ObjectControllerScope::OpponentOfAbilityController
                    )
                )
                && crate::oracle::canonical::effects::lower_permanent_count(count).is_some()
        }
        NumericExpression::CountQuery(query) => {
            query.departures.is_none()
                && crate::oracle::canonical::effects::lower_scoped_object_query_amount(query)
                    .is_some()
        }
        NumericExpression::CardsInZone {
            player: super::LocalPlayerReference::Controller,
            zone: super::LocalZone::Library,
        }
        | NumericExpression::PlayerFact {
            player: super::LocalPlayerReference::Controller,
            ..
        }
        | NumericExpression::Devotion(_)
        | NumericExpression::OpponentCount(_) => true,
        NumericExpression::GreatestPropertyAmong { property, .. } => {
            *property != SelectorNumericProperty::ManaValue
                && crate::oracle::canonical::effects::lower_numeric_expression(expression).is_some()
        }
        NumericExpression::Add { left, right }
        | NumericExpression::Subtract { left, right }
        | NumericExpression::Multiply { left, right }
        | NumericExpression::Divide { left, right, .. } => {
            controller_relative_amount(left) && controller_relative_amount(right)
        }
        _ => false,
    }
}

/// The right-hand sides a resolution-prepared consumer evaluates: every
/// controller-relative amount, the turn's departures, the source's own
/// counters and characteristics when the source is proved, and a
/// characteristic of the permanent sacrificed to pay a cost (a value the
/// cost payment binds, CR 608.2h).
fn resolution_amount(expression: &super::NumericExpression, source_available: bool) -> bool {
    use super::NumericExpression;
    match expression {
        NumericExpression::Atom(_) => true,
        // The resolving ability's own count reads its source (`other
        // creatures you control`) and every measure.
        NumericExpression::PermanentCount(count) => {
            matches!(
                count.controller,
                None | Some(
                    ObjectControllerScope::AbilityController
                        | ObjectControllerScope::OpponentOfAbilityController
                )
            ) && crate::oracle::canonical::effects::lower_permanent_count(count).is_some()
        }
        NumericExpression::CountCounters { .. }
        | NumericExpression::ObjectProperty {
            object: super::LocalObjectReference::SourceSelf,
            ..
        } => source_available,
        NumericExpression::CountQuery(query) => {
            crate::oracle::canonical::effects::lower_scoped_object_query_amount(query).is_some()
        }
        // One property of an object the resolving ability binds (`that
        // creature's power`, the triggering spell's mana value), read from
        // its bindings or the event it records (CR 608.2h).
        NumericExpression::BoundObjectProperty { .. }
        | NumericExpression::EventValue(super::EventNumericBinding::CastSpellManaValue) => true,
        // The spell's own payment, read from the resolving spell (the card
        // context keeps it in spell abilities).
        NumericExpression::SacrificedCostObjectProperty(_)
        | NumericExpression::ColorsOfManaSpentToCastSource => true,
        NumericExpression::Add { left, right }
        | NumericExpression::Subtract { left, right }
        | NumericExpression::Multiply { left, right }
        | NumericExpression::Divide { left, right, .. } => {
            resolution_amount(left, source_available) && resolution_amount(right, source_available)
        }
        other => controller_relative_amount(other),
    }
}

impl ComparisonRhsAdmission {
    fn admits(self, expression: &super::NumericExpression) -> bool {
        use super::NumericExpression;
        let literal = matches!(
            expression,
            NumericExpression::Atom(_)
                | NumericExpression::ObjectProperty {
                    object: super::LocalObjectReference::SourceSelf,
                    ..
                }
        );
        match self {
            Self::Literal => literal,
            // The source's own counters (`with power less than or equal to
            // the number of treasure counters on this enchantment`) are read
            // by the target binder from the source, or its last known
            // information, each time the candidates are enumerated (CR
            // 601.2c, 608.2b, 608.2h), as its power and toughness are.
            Self::DeclaredTarget => {
                literal
                    || controller_relative_amount(expression)
                    || matches!(
                        expression,
                        NumericExpression::CountCounters {
                            object: super::LocalObjectReference::SourceSelf,
                            ..
                        }
                    )
            }
            Self::Resolution { source_available } => {
                resolution_amount(expression, source_available)
            }
            Self::CardResolution => literal || resolution_amount(expression, true),
        }
    }

    /// Whether the compared candidate property may be power or toughness.
    /// A resolution-owned permanent query has no checked evaluator of the
    /// effective power of a candidate in every zone.
    fn admits_property(self, property: SelectorNumericProperty) -> bool {
        match self {
            Self::Resolution { .. } => property == SelectorNumericProperty::ManaValue,
            Self::Literal | Self::DeclaredTarget | Self::CardResolution => true,
        }
    }
}

/// Recognizes a reusable numeric property constraint attached to one selector.
/// The selector owns the predicate; effect machines only validate their domain.
fn recognize_numeric_comparison_qualifier(
    cursor: &mut OracleTokenCursor<'_>,
    admission: ComparisonRhsAdmission,
) -> MachineResult<SelectorPredicate> {
    recognize_numeric_comparison_from(cursor, admission, NumericComparisonState::Relation, None)
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

/// Whether a numeric property word starts at `offset` (`mana value`,
/// `power`, `toughness`). Lexical lookahead only.
fn numeric_property_starts_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    cursor.peek_ahead(offset).is_some_and(|raw| {
        raw.eq_ignore_ascii_case("power")
            || raw.eq_ignore_ascii_case("toughness")
            || (raw.eq_ignore_ascii_case("mana")
                && cursor
                    .peek_ahead(offset + 1)
                    .is_some_and(|next| next.eq_ignore_ascii_case("value")))
    })
}

/// The comparison after an already read property: `[equal to] N [or less |
/// or greater]`. It shares the selector qualifier's threshold grammar and its
/// literal/declared-X right-hand-side policy.
pub(in crate::oracle::canonical) fn recognize_numeric_property_comparison_tail(
    cursor: &mut OracleTokenCursor<'_>,
    property: SelectorNumericProperty,
) -> MachineResult<SelectorPredicate> {
    recognize_numeric_comparison_from(
        cursor,
        ComparisonRhsAdmission::Literal,
        NumericComparisonState::Right,
        Some(property),
    )
}

fn recognize_numeric_comparison_from(
    cursor: &mut OracleTokenCursor<'_>,
    admission: ComparisonRhsAdmission,
    start: NumericComparisonState,
    initial_property: Option<SelectorNumericProperty>,
) -> MachineResult<SelectorPredicate> {
    let mut state = start;
    // `power or toughness 1 or less` (CR 208.1): one threshold compared with
    // either property, the disjunction of two comparisons.
    let mut properties: Vec<SelectorNumericProperty> = initial_property.into_iter().collect();
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
                properties.push(recognize_selector_numeric_property(cursor)?);
                NumericComparisonState::PropertyAlternative
            }
            NumericComparisonState::PropertyAlternative => {
                cursor.enter("selector.numeric-comparison.property-alternative");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
                    && numeric_property_starts_at(cursor, 1)
                {
                    cursor.consume("or")?;
                    let alternative = recognize_selector_numeric_property(cursor)?;
                    cursor.require(
                        !properties.contains(&alternative),
                        "two different properties compared with one threshold",
                    )?;
                    properties.push(alternative);
                }
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
                // Inside a candidate filter the possessive names the
                // candidate under test, never the environment's object.
                if cursor.peek().is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("its") || raw.eq_ignore_ascii_case("their")
                }) && numeric_property_starts_at(cursor, 1)
                {
                    // `with toughness greater than its power` (CR 208.1):
                    // the possessive names the candidate under test, so the
                    // comparison reads two properties of that one object.
                    cursor.enter("selector.numeric-comparison.candidate-property");
                    let possessive = cursor.peek().expect("checked possessive").to_string();
                    cursor.consume(&possessive)?;
                    let right = recognize_selector_numeric_property(cursor)?;
                    cursor.require(
                        properties.len() == 1 && !properties.contains(&right),
                        "one candidate property compared with another of its properties",
                    )?;
                    cursor.enter("selector.numeric-comparison.property-context");
                    cursor.require(
                        admission.admits_property(properties[0])
                            && admission.admits_property(right),
                        "candidate properties this consumer evaluates",
                    )?;
                    return Ok(SelectorPredicate::Qualifier(
                        super::ObjectQualifier::CandidatePropertyComparison {
                            left: properties[0],
                            operator,
                            right,
                        },
                    ));
                }
                // Any other possessive operand (`its loyalty`) names a
                // candidate property the comparison does not read.
                if cursor.peek().is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("its") || raw.eq_ignore_ascii_case("their")
                }) {
                    cursor.enter("selector.numeric-comparison.candidate-possessive");
                    return Err(cursor.fail(
                        "an operand independent of the candidate (`its PROPERTY` names the candidate under test)",
                    ));
                }
                let expression =
                    super::arithmetic_expression::recognize_comparison_rhs_expression(cursor)?;
                // Card P/T has no checked AnyZone characteristic evaluator in
                // a resolution permanent consumer. Do not route it to the
                // historical printed-stat fallback.
                cursor.enter("selector.numeric-comparison.property-context");
                cursor.require(
                    properties
                        .iter()
                        .all(|property| admission.admits_property(*property)),
                    "mana value as the proved candidate property for this resolution-card query",
                )?;
                cursor.enter("selector.numeric-comparison.rhs-context");
                cursor.require(
                    admission.admits(&expression),
                    "a query expression supported by this consumer's explicit numeric/source context",
                )?;
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
                let right = cursor.require_value(right, "a populated numeric threshold")?;
                cursor.require(!properties.is_empty(), "a populated numeric property")?;
                let mut comparisons = properties
                    .into_iter()
                    .map(|left| SelectorPredicate::NumericComparison {
                        left,
                        operator,
                        right: right.clone(),
                    })
                    .collect::<Vec<_>>();
                return Ok(if comparisons.len() == 1 {
                    comparisons.pop().expect("one comparison")
                } else {
                    SelectorPredicate::Union(comparisons)
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
                let selector = match frame.branches.len() {
                    0 => return Err(cursor.fail("a populated stack-item selector")),
                    1 => frame.branches.pop().expect("one stack-item branch"),
                    _ => StackItemPredicate::Union(frame.branches),
                };
                // `target spell or ability that targets you or a creature
                // you control` (CR 115.1): the shared targets relation of
                // the relative slot, over every branch of the selector.
                return Ok(
                    match super::recognize_stack_item_targets_relative(cursor)? {
                        Some(targeted) => StackItemPredicate::Intersection(vec![
                            selector,
                            StackItemPredicate::Targets(targeted),
                        ]),
                        None => selector,
                    },
                );
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

#[path = "selector_qualifiers.rs"]
mod qualifiers;

#[cfg(test)]
#[path = "selector_qualifier_atom_tests.rs"]
mod qualifier_atom_tests;

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

#[cfg(test)]
#[path = "selector_comparative_tests.rs"]
mod comparative_tests;

#[cfg(test)]
#[path = "selector_cast_relation_tests.rs"]
mod cast_relation_tests;

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
    // `a red or green card`: `or` joins another adjective sharing the same
    // following noun; any other `or` is the parent's.
    let mut alternatives = vec![atom];
    loop {
        let mut trial = cursor.clone();
        if trial.consume("or").is_err() {
            break;
        }
        match recognize_object_predicate_atom(&mut trial, AcceptedForms::SingularOnly) {
            Ok(next) if adjective(&next) => {
                *cursor = trial;
                alternatives.push(next);
            }
            _ => break,
        }
    }
    Some(match alternatives.len() {
        1 => alternatives.pop().expect("one adjective"),
        _ => ObjectPredicate::Union(alternatives),
    })
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
/// the terminating `card` noun. Its numeric comparisons keep the literal
/// right-hand-side policy of a consumer that has not proved when it
/// evaluates them.
pub(in crate::oracle::canonical) fn recognize_search_card_selector(
    cursor: &mut OracleTokenCursor<'_>,
    plural: bool,
) -> MachineResult<ObjectPredicate> {
    recognize_search_card_selector_with(cursor, plural, ComparisonRhsAdmission::Literal)
}

/// The same card phrase for a consumer that states when its numeric
/// comparisons are evaluated (`admission`).
fn recognize_search_card_selector_with(
    cursor: &mut OracleTokenCursor<'_>,
    plural: bool,
    admission: ComparisonRhsAdmission,
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
                } else if cursor
                    .peek()
                    .and_then(|raw| lookup_supertype(raw, AcceptedForms::SingularOnly))
                    == Some(CardSupertype::Snow)
                {
                    // `a snow land card` (CR 205.4a): the snow supertype.
                    cursor.consume(CardSupertype::Snow.oracle_singular())?;
                    predicates.push(ObjectPredicate::Supertype(CardSupertype::Snow));
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
                // `an Assembly-Worker creature card`: the subtype modifies
                // the card type noun that follows it (CR 205.3).
                if cursor
                    .peek()
                    .and_then(|raw| lookup_card_type(raw, AcceptedForms::SingularOnly))
                    .is_some()
                {
                    cursor.enter("selector.search-card.subtype-card-type");
                    predicates.push(recognize_search_card_type_union(cursor)?);
                }
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
                } else if let Some(length) = super::chosen_card_name_qualifier_length(cursor) {
                    // `cards with that name` / `with the chosen name`: the
                    // shared chosen-name relation of the object query
                    // (CR 201.2, 608.2d), read before the `with` readers.
                    cursor.enter("selector.search-card.chosen-card-name-qualifier");
                    cursor.advance_to(cursor.position() + length)?;
                    predicates.push(ObjectPredicate::Relation(super::ObjectRelation::ChosenName));
                    SearchCardSelectorState::QualifierOrComplete
                } else if let Some(form) = qualifiers::comparative_with_form(cursor) {
                    // `a creature card with lesser mana value`, `a card with
                    // the same name as that creature`: the shared comparative
                    // qualifier, under this search's admission.
                    cursor.enter("selector.search-card.comparative");
                    predicates.push(qualifiers::recognize_comparative_with(
                        cursor, form, admission,
                    )?);
                    SearchCardSelectorState::QualifierOrComplete
                } else if let Some(form) = qualifiers::with_qualifier_form(cursor).filter(|form| {
                    matches!(
                        form,
                        qualifiers::WithQualifierForm::Distinct
                            | qualifiers::WithQualifierForm::Total
                            | qualifiers::WithQualifierForm::ManaCostSymbol
                            | qualifiers::WithQualifierForm::KeywordAbility
                    )
                }) {
                    // `cards with different names`, `with total mana value
                    // 6 or less`, `a card with flashback`: the shared
                    // qualifier atoms. A set constraint needs the plural
                    // found set; its consumer lifts it onto the choice.
                    cursor.enter("selector.search-card.with-qualifier");
                    predicates.push(qualifiers::recognize_with_qualifier(cursor, form, plural)?);
                    SearchCardSelectorState::QualifierOrComplete
                } else if plural && super::shared_characteristic_set_clause_starts(cursor) {
                    // `up to two basic land cards that share a land type`:
                    // a constraint over the found set (CR 205.3i), read by
                    // the shared share-clause readers in their set mode.
                    cursor.enter("selector.search-card.shared-characteristic");
                    let characteristic = super::recognize_shared_characteristic_set_clause(cursor)?;
                    // The found-set check the runtime has compares land
                    // types only; another shared characteristic is read by
                    // the same clause and refused here, never approximated.
                    cursor.enter("selector.search-card.shared-characteristic.capability");
                    cursor.require(
                        characteristic == super::SharedCharacteristic::LandType,
                        "a found set sharing a land type (the only shared-set check the runtime performs)",
                    )?;
                    predicates.push(ObjectPredicate::Qualifier(
                        super::ObjectQualifier::SharedValue(characteristic),
                    ));
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
                predicates.push(recognize_numeric_comparison_qualifier(cursor, admission)?);
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
///
/// The found cards are chosen while the search resolves, from a zone where
/// they keep their printed characteristics (CR 109.3, 701.23a): a numeric
/// comparison may read every amount the resolving ability evaluates
/// (`with mana value less than or equal to the number of lands you
/// control`).
pub(in crate::oracle::canonical) fn recognize_search_card_alternatives(
    cursor: &mut OracleTokenCursor<'_>,
    plural: bool,
) -> MachineResult<ObjectPredicate> {
    let admission = ComparisonRhsAdmission::CardResolution;
    let mut alternatives = vec![recognize_search_card_selector_with(
        cursor, plural, admission,
    )?];
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
        match recognize_search_card_selector_with(&mut trial, plural, admission) {
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

/// `not named NAME` at the cursor: the negated name qualifier. Pure
/// lookahead over the same name form.
fn negated_named_qualifier_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("not"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("named"))
        && card_name_width_at(cursor, 2).is_some()
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
