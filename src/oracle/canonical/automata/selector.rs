use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, PlayerRole, lookup_card_type, lookup_player_role, lookup_supertype,
};

use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    CardSupertype, NumericComparisonOperator, ObjectCardType, ObjectControllerScope, ObjectKind,
    ObjectPredicate, PlayerPredicate, SelectorNumericProperty, SelectorPredicate, SpellCasterScope,
    StackAbilityKind, StackItemPredicate, TargetSelector, object_predicate_atom_starts,
    parameterless_keyword_from_oracle_word, recognize_mana_cost, recognize_numeric_atom,
    recognize_object_predicate_atom, recognize_parameterless_keyword,
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
    subject_scope: Option<QuerySubjectScope>,
    ownership_scope: Option<QuerySubjectScope>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueryRelationKind {
    Control,
    Cast,
    Own,
    ObjectRelations,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuerySubjectScope {
    AbilityController,
    NotAbilityController,
    OpponentOfAbilityController,
    /// `defending player` / `that player` bound by the triggering event.
    EventPlayer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecognizedObjectQuery {
    predicate: SelectorPredicate,
    subject_scope: Option<QuerySubjectScope>,
    ownership_scope: Option<QuerySubjectScope>,
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

/// Opt-in relation transport. Historical wrappers still recognize their own
/// controller/caster domain and cannot consume an ownership relation silently.
pub(in crate::oracle::canonical) fn recognize_related_object_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<RelatedObjectQueryNode> {
    let query = recognize_object_query_internal(cursor, QueryRelationKind::ObjectRelations)?;
    Ok(RelatedObjectQueryNode {
        objects: super::ObjectQueryNode {
            predicate: query.predicate,
            controller: query.subject_scope.map(object_controller_scope),
            exclude_source: false,
        },
        ownership: query.ownership_scope.map(|scope| match scope {
            QuerySubjectScope::AbilityController => ObjectOwnershipScope::AbilityController,
            QuerySubjectScope::NotAbilityController => ObjectOwnershipScope::NotAbilityController,
            QuerySubjectScope::OpponentOfAbilityController => {
                ObjectOwnershipScope::OpponentOfAbilityController
            }
            QuerySubjectScope::EventPlayer => ObjectOwnershipScope::EventPlayer,
        }),
    })
}

fn object_relation_kind_at(cursor: &OracleTokenCursor<'_>) -> MachineResult<QueryRelationKind> {
    let offset = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("you") => 1,
        Some("an" | "your") => 2,
        _ if bound_player_relation_starts(cursor) => 2,
        _ => return Err(cursor.fail("an explicit player relation subject")),
    };
    let offset = offset
        + usize::from(
            cursor
                .peek_ahead(offset)
                .is_some_and(|s| s.eq_ignore_ascii_case("don't")),
        );
    match cursor
        .peek_ahead(offset)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("control" | "controls") => Ok(QueryRelationKind::Control),
        Some("own" | "owns") => Ok(QueryRelationKind::Own),
        _ => Err(cursor.fail("a controller or owner relation")),
    }
}

/// Recognizes an object predicate plus its optional controller relation,
/// independently of whether a parent uses it as a target or as a set query.
///
/// Coordinators are transactional: a comma, `or` or `and/or` is consumed only
/// when another object phrase follows, and a comma-separated run commits only
/// once `or`/`and/or` closes it (or when it lists negative modifiers of one
/// head). Otherwise the connector stays with the parent, whatever the parent
/// is (an `if` separator, a cost list, an effect sequence).
pub(in crate::oracle::canonical) fn recognize_object_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::ObjectQueryNode> {
    let query = recognize_object_query_internal(cursor, QueryRelationKind::Control)?;
    Ok(super::ObjectQueryNode {
        predicate: query.predicate,
        controller: query.subject_scope.map(object_controller_scope),
        exclude_source: false,
    })
}

/// One object query whose numeric comparisons may read the ability source
/// when `source_available` (its counters, by last-known information).
pub(in crate::oracle::canonical) fn recognize_object_query_with_source(
    cursor: &mut OracleTokenCursor<'_>,
    source_available: bool,
) -> MachineResult<super::ObjectQueryNode> {
    let query = recognize_object_query_with_cardinality_policy(
        cursor,
        QueryRelationKind::Control,
        CoordinatedCardinalityPolicy::ExistingQuery,
        Some(source_available),
        false,
    )?;
    Ok(super::ObjectQueryNode {
        predicate: query.predicate,
        controller: query.subject_scope.map(object_controller_scope),
        exclude_source: false,
    })
}

/// Recognizes a spell collection whose player relation is the player who cast
/// the spell, not the spell's current controller on the stack.
pub(in crate::oracle::canonical) fn recognize_spell_cast_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::SpellCastQueryNode> {
    let query = recognize_object_query_internal(cursor, QueryRelationKind::Cast)?;
    Ok(super::SpellCastQueryNode {
        predicate: query.predicate,
        caster: query.subject_scope.map(spell_caster_scope),
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CoordinatedCardinalityPolicy {
    ExistingQuery,
    ParentOwnedSharedQuota { exactly_one: bool },
}

/// One parent-owned quota applies to the whole candidate predicate. An
/// explicitly repeated determiner would introduce a separate quota and must
/// not be erased by Boolean normalization into a single union.
pub(in crate::oracle::canonical) fn recognize_object_query_with_shared_cardinality(
    cursor: &mut OracleTokenCursor<'_>,
    count: &super::NumericExpression,
) -> MachineResult<super::ObjectQueryNode> {
    let query = recognize_object_query_with_cardinality_policy(
        cursor,
        QueryRelationKind::Control,
        CoordinatedCardinalityPolicy::ParentOwnedSharedQuota {
            exactly_one: matches!(
                count,
                super::NumericExpression::Atom(super::NumericAtom::Integer(1))
            ),
        },
        None,
        false,
    )?;
    Ok(super::ObjectQueryNode {
        predicate: query.predicate,
        controller: query.subject_scope.map(object_controller_scope),
        exclude_source: false,
    })
}

/// A resolution-owned query permits pure expression operands. Source proof
/// is distinct from implicit event subjects and grammatical antecedents.
pub(in crate::oracle::canonical) fn recognize_resolution_card_query(
    cursor: &mut OracleTokenCursor<'_>,
    source_available: bool,
) -> MachineResult<super::ObjectQueryNode> {
    let query = recognize_object_query_with_cardinality_policy(
        cursor,
        QueryRelationKind::Control,
        CoordinatedCardinalityPolicy::ExistingQuery,
        Some(source_available),
        false,
    )?;
    cursor.enter("selector.resolution-card.domain");
    cursor.require(
        query.predicate.has_domain(ObjectKind::Card)
            && query.predicate.supports_domain(ObjectKind::Card),
        "an explicit card-domain query",
    )?;
    cursor.require(
        query.subject_scope.is_none(),
        "an unscoped card predicate in the source zone",
    )?;
    Ok(super::ObjectQueryNode {
        predicate: query.predicate,
        controller: None,
        exclude_source: false,
    })
}

fn recognize_object_query_internal(
    cursor: &mut OracleTokenCursor<'_>,
    relation_kind: QueryRelationKind,
) -> MachineResult<RecognizedObjectQuery> {
    recognize_object_query_with_cardinality_policy(
        cursor,
        relation_kind,
        CoordinatedCardinalityPolicy::ExistingQuery,
        None,
        false,
    )
}

fn recognize_object_query_with_cardinality_policy(
    cursor: &mut OracleTokenCursor<'_>,
    relation_kind: QueryRelationKind,
    cardinality_policy: CoordinatedCardinalityPolicy,
    query_source_available: Option<bool>,
    logical_properties: bool,
) -> MachineResult<RecognizedObjectQuery> {
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
                }) {
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
                } else if let Some(length) = ownership_relation_length(cursor, relation_kind) {
                    // `creature you own`, `permanents you don't own` (CR
                    // 108.3): the ownership relation of the collection.
                    cursor.enter("selector.ownership-relation");
                    let negated = length == 3;
                    for _ in 0..length {
                        let raw = cursor.peek().unwrap_or_default().to_string();
                        cursor.consume(&raw)?;
                    }
                    frame.postnominal_qualifiers.push(PostnominalQualifier {
                        predicate: SelectorPredicate::OwnedByAbilityController { negated },
                        scope: PostnominalQualifierScope::WholeQuery,
                    });
                    ObjectQueryState::ReadPostnominalQualifier
                } else if cursor.peek().is_some_and(|token| {
                    token.eq_ignore_ascii_case("you")
                        || token.eq_ignore_ascii_case("an")
                        || token.eq_ignore_ascii_case("your")
                }) || bound_player_relation_starts(cursor)
                {
                    let role = if matches!(relation_kind, QueryRelationKind::ObjectRelations) {
                        object_relation_kind_at(cursor)?
                    } else {
                        relation_kind
                    };
                    let slot = if matches!(role, QueryRelationKind::Own) {
                        &mut frame.ownership_scope
                    } else {
                        &mut frame.subject_scope
                    };
                    cursor.require(slot.is_none(), "at most one relation for each player role")?;
                    *slot = Some(recognize_object_subject_scope(cursor, role)?);
                    ObjectQueryState::ReadPostnominalQualifier
                } else if matches!(relation_kind, QueryRelationKind::ObjectRelations)
                    && cursor
                        .peek()
                        .is_some_and(|token| token.eq_ignore_ascii_case("but"))
                {
                    cursor.consume("but")?;
                    if cursor.peek().is_some_and(|token| {
                        token.eq_ignore_ascii_case("you")
                            || token.eq_ignore_ascii_case("an")
                            || token.eq_ignore_ascii_case("your")
                    }) {
                        ObjectQueryState::ReadPostnominalQualifier
                    } else {
                        cursor.require(frame.subject_scope == Some(QuerySubjectScope::AbilityController),
                            "an explicit inherited controller subject for a following ownership relation")?;
                        cursor
                            .require(frame.ownership_scope.is_none(), "one ownership relation")?;
                        let negated = cursor
                            .peek()
                            .is_some_and(|token| token.eq_ignore_ascii_case("don't"));
                        if negated {
                            cursor.consume("don't")?;
                        }
                        cursor.consume("own")?;
                        frame.ownership_scope = Some(if negated {
                            QuerySubjectScope::NotAbilityController
                        } else {
                            QuerySubjectScope::AbilityController
                        });
                        ObjectQueryState::ReadPostnominalQualifier
                    }
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
                } else if super::relative_clause_starts(cursor) {
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
                    let relative = recognize_object_query_with_cardinality_policy(
                        cursor,
                        QueryRelationKind::Control,
                        cardinality_policy,
                        query_source_available,
                        true,
                    )?;
                    cursor.require(
                        relative.subject_scope.is_none(),
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
                if let CoordinatedCardinalityPolicy::ParentOwnedSharedQuota { exactly_one } =
                    cardinality_policy
                {
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
                let shared_qualifiers = compose_postnominal_scopes(
                    cursor,
                    &mut frame.union_branches,
                    frame.postnominal_qualifiers,
                )?;
                let base = match frame.union_branches.len() {
                    0 => return Err(cursor.fail("a populated selector predicate")),
                    1 => frame.union_branches.pop().expect("one branch"),
                    _ if logical_properties => {
                        let atoms = frame
                            .union_branches
                            .into_iter()
                            .map(super::BooleanConditionNode::Atom)
                            .collect();
                        let composed = super::lower_boolean_precedence(
                            cursor,
                            atoms,
                            frame.logical_connectors,
                        )?;
                        object_predicate_from_boolean(composed)
                    }
                    _ => lift_shared_trailing_domain(frame.union_branches),
                };
                let predicate = if shared_qualifiers.is_empty() {
                    base
                } else {
                    let mut operands = Vec::with_capacity(1 + shared_qualifiers.len());
                    operands.push(base);
                    operands.extend(shared_qualifiers);
                    SelectorPredicate::Intersection(operands)
                };
                return Ok(RecognizedObjectQuery {
                    predicate,
                    subject_scope: frame.subject_scope,
                    ownership_scope: frame.ownership_scope,
                });
            }
        };
    }
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
    let Some((checkpoint, snapshot)) = frame.comma_run.take() else {
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
    let plural = &frame.branch_has_plural_surface[run_start..];
    if plural.iter().any(|flag| *flag != plural[0]) {
        abandon_comma_run(cursor, frame);
        return Ok(false);
    }
    cursor.require(
        !modifiers_of_one_head(&frame.union_branches[run_start..]),
        "a negative-modifier list whose head is not coordinated with another branch",
    )?;
    frame.comma_run = None;
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

    let query = recognize_object_query(cursor)?;
    Ok(match query.controller {
        Some(controller) => TargetSelector::ControlledObject {
            predicate: query.predicate,
            controller,
        },
        None => TargetSelector::Object(query.predicate),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControllerQualifierState {
    Subject,
    Opponent,
    Opponents,
    RelationOrNegation,
    NegatedRelation,
    Complete,
}

/// Recognizes the controller relation attached to an object selector. The
/// relation is retained outside the card-characteristic predicate so lowering
/// can populate the generic target-candidate controller field.
fn recognize_object_subject_scope(
    cursor: &mut OracleTokenCursor<'_>,
    relation_kind: QueryRelationKind,
) -> MachineResult<QuerySubjectScope> {
    let mut state = ControllerQualifierState::Subject;
    let mut scope = None;
    let mut plural_opponents = false;
    loop {
        state = match state {
            ControllerQualifierState::Subject => {
                cursor.enter("selector.controller-qualifier.subject");
                if bound_player_relation_starts(cursor) {
                    // `defending player controls`, `that player controls`:
                    // the player is resolved by the binding environment
                    // (C2), never guessed by this query.
                    cursor.enter("selector.controller-qualifier.bound-player");
                    let environment = super::installed_binding_environment(cursor);
                    let player = super::recognize_bound_player_reference(cursor, &environment)?;
                    cursor.require(
                        player == super::BoundPlayerReference::EventPlayer,
                        "a player the triggering event records",
                    )?;
                    scope = Some(QuerySubjectScope::EventPlayer);
                    plural_opponents = false;
                    ControllerQualifierState::RelationOrNegation
                } else if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
                {
                    cursor.consume("you")?;
                    scope = Some(QuerySubjectScope::AbilityController);
                    ControllerQualifierState::RelationOrNegation
                } else if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
                {
                    cursor.consume("your")?;
                    plural_opponents = true;
                    ControllerQualifierState::Opponents
                } else {
                    cursor.consume("an")?;
                    ControllerQualifierState::Opponent
                }
            }
            ControllerQualifierState::Opponent => {
                cursor.enter("selector.controller-qualifier.opponent");
                cursor.consume("opponent")?;
                scope = Some(QuerySubjectScope::OpponentOfAbilityController);
                ControllerQualifierState::RelationOrNegation
            }
            ControllerQualifierState::Opponents => {
                cursor.enter("selector.controller-qualifier.opponents");
                cursor.consume("opponents")?;
                scope = Some(QuerySubjectScope::OpponentOfAbilityController);
                ControllerQualifierState::RelationOrNegation
            }
            ControllerQualifierState::RelationOrNegation => {
                cursor.enter("selector.controller-qualifier.relation-or-negation");
                match scope {
                    Some(QuerySubjectScope::AbilityController)
                        if cursor
                            .peek()
                            .is_some_and(|raw| raw.eq_ignore_ascii_case("don't")) =>
                    {
                        cursor.consume("don't")?;
                        scope = Some(QuerySubjectScope::NotAbilityController);
                        ControllerQualifierState::NegatedRelation
                    }
                    Some(QuerySubjectScope::AbilityController) => {
                        cursor.consume(match relation_kind {
                            QueryRelationKind::Control => "control",
                            QueryRelationKind::Cast => "cast",
                            QueryRelationKind::Own => "own",
                            QueryRelationKind::ObjectRelations => {
                                unreachable!("role selected before subject reader")
                            }
                        })?;
                        ControllerQualifierState::Complete
                    }
                    Some(
                        QuerySubjectScope::OpponentOfAbilityController
                        | QuerySubjectScope::EventPlayer,
                    ) => {
                        cursor.consume(match (relation_kind, plural_opponents) {
                            (QueryRelationKind::Control, true) => "control",
                            (QueryRelationKind::Control, false) => "controls",
                            (QueryRelationKind::Cast, true) => "cast",
                            (QueryRelationKind::Cast, false) => "casts",
                            (QueryRelationKind::Own, true) => "own",
                            (QueryRelationKind::Own, false) => "owns",
                            (QueryRelationKind::ObjectRelations, _) => {
                                unreachable!("role selected before subject reader")
                            }
                        })?;
                        ControllerQualifierState::Complete
                    }
                    Some(QuerySubjectScope::NotAbilityController) => {
                        return Err(cursor.fail("an unresolved positive controller relation"));
                    }
                    None => return Err(cursor.fail("a resolved controller subject")),
                }
            }
            ControllerQualifierState::NegatedRelation => {
                cursor.enter("selector.controller-qualifier.negated-relation");
                cursor.consume(match relation_kind {
                    QueryRelationKind::Control => "control",
                    QueryRelationKind::Cast => "cast",
                    QueryRelationKind::Own => "own",
                    QueryRelationKind::ObjectRelations => {
                        unreachable!("role selected before subject reader")
                    }
                })?;
                ControllerQualifierState::Complete
            }
            ControllerQualifierState::Complete => {
                cursor.enter("selector.controller-qualifier.complete");
                return cursor.require_value(scope, "an object controller scope");
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

/// `you own` (2) / `you don't own` (3) in a query whose relation is
/// control: an ownership qualifier, not the controller subject. Queries
/// that read player relations themselves keep their own reader.
fn ownership_relation_length(
    cursor: &OracleTokenCursor<'_>,
    relation_kind: QueryRelationKind,
) -> Option<usize> {
    if !matches!(relation_kind, QueryRelationKind::Control) {
        return None;
    }
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if !word(0, "you") {
        return None;
    }
    if word(1, "own") {
        Some(2)
    } else if (word(1, "don't") || word(1, "don\u{2019}t")) && word(2, "own") {
        Some(3)
    } else {
        None
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
        && (word(2, "controls") || word(2, "owns"))
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
                } else if let Some(strict_operator) = strict_comparison_operator(cursor) {
                    // `less than EXPR` / `greater than EXPR`: the strict
                    // relation, the shared operator of sentence conditions.
                    operator = strict_operator;
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

/// `less than` / `greater than` before a comparison's right operand.
fn strict_comparison_operator(
    cursor: &mut OracleTokenCursor<'_>,
) -> Option<NumericComparisonOperator> {
    let operator = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("less") => NumericComparisonOperator::LessThan,
        Some("greater") => NumericComparisonOperator::GreaterThan,
        _ => return None,
    };
    if !cursor
        .peek_ahead(1)
        .is_some_and(|word| word.eq_ignore_ascii_case("than"))
    {
        return None;
    }
    let direction = cursor.peek()?.to_string();
    cursor.consume_phrase(&[direction.as_str(), "than"]).ok()?;
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
    match lookup_player_role(raw)? {
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
            .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
        {
            cursor.consume("or")?;
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
                if cursor
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

#[cfg(test)]
#[path = "selector_transactional_coordinator_tests.rs"]
mod transactional_coordinator_tests;

#[cfg(test)]
#[path = "selector_negative_modifier_tests.rs"]
mod negative_modifier_tests;

#[cfg(test)]
#[path = "selector_coordinated_article_tests.rs"]
mod coordinated_article_tests;

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
