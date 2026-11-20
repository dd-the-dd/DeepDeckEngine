use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, PlayerRole, lookup_card_type, lookup_player_role, lookup_supertype,
};

use super::super::{MachineResult, OracleTokenCursor};
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct RelatedObjectQueryNode {
    pub objects: super::ObjectQueryNode,
    pub ownership: Option<ObjectOwnershipScope>,
}

/// Opt-in relation transport. Historical wrappers still recognize their own
/// controller/caster domain and cannot consume an ownership relation silently.
pub(in crate::oracle::canonical) fn recognize_related_object_query_before_separator(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<RelatedObjectQueryNode> {
    let query = recognize_object_query_internal(
        cursor,
        true,
        QueryRelationKind::ObjectRelations,
        false,
        false,
    )?;
    Ok(RelatedObjectQueryNode {
        objects: super::ObjectQueryNode {
            predicate: query.predicate,
            controller: query.subject_scope.map(object_controller_scope),
        },
        ownership: query.ownership_scope.map(|scope| match scope {
            QuerySubjectScope::AbilityController => ObjectOwnershipScope::AbilityController,
            QuerySubjectScope::NotAbilityController => ObjectOwnershipScope::NotAbilityController,
            QuerySubjectScope::OpponentOfAbilityController => {
                ObjectOwnershipScope::OpponentOfAbilityController
            }
        }),
    })
}

fn object_relation_kind_at(cursor: &OracleTokenCursor<'_>) -> MachineResult<QueryRelationKind> {
    let offset = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("you") => 1,
        Some("an" | "your") => 2,
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
pub(in crate::oracle::canonical) fn recognize_object_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::ObjectQueryNode> {
    let query =
        recognize_object_query_internal(cursor, false, QueryRelationKind::Control, false, false)?;
    Ok(super::ObjectQueryNode {
        predicate: query.predicate,
        controller: query.subject_scope.map(object_controller_scope),
    })
}

/// Recognizes an object query whose trailing comma belongs to the parent
/// grammar, such as the separator between an `if` condition and its effect.
pub(in crate::oracle::canonical) fn recognize_object_query_before_separator(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::ObjectQueryNode> {
    let query =
        recognize_object_query_internal(cursor, true, QueryRelationKind::Control, false, false)?;
    Ok(super::ObjectQueryNode {
        predicate: query.predicate,
        controller: query.subject_scope.map(object_controller_scope),
    })
}

/// One object query whose numeric comparisons may read the ability source
/// when `source_available` (its counters, by last-known information).
pub(in crate::oracle::canonical) fn recognize_object_query_before_separator_with_source(
    cursor: &mut OracleTokenCursor<'_>,
    source_available: bool,
) -> MachineResult<super::ObjectQueryNode> {
    let query = recognize_object_query_with_cardinality_policy(
        cursor,
        true,
        QueryRelationKind::Control,
        false,
        false,
        CoordinatedCardinalityPolicy::ExistingQuery,
        Some(source_available),
        false,
    )?;
    Ok(super::ObjectQueryNode {
        predicate: query.predicate,
        controller: query.subject_scope.map(object_controller_scope),
    })
}

/// A payment operand inside a cost list. A comma ends the operand unless it
/// continues the object list (`a creature, artifact, or land`); a comma that
/// opens the next payment action (`, discard a card`) stays with the parent.
pub(in crate::oracle::canonical) fn recognize_object_query_before_cost_separator(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::ObjectQueryNode> {
    let query =
        recognize_object_query_internal(cursor, false, QueryRelationKind::Control, false, true)?;
    Ok(super::ObjectQueryNode {
        predicate: query.predicate,
        controller: query.subject_scope.map(object_controller_scope),
    })
}

/// Recognizes a spell collection whose player relation is the player who cast
/// the spell, not the spell's current controller on the stack.
pub(in crate::oracle::canonical) fn recognize_spell_cast_query_before_separator(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::SpellCastQueryNode> {
    let query =
        recognize_object_query_internal(cursor, true, QueryRelationKind::Cast, false, false)?;
    Ok(super::SpellCastQueryNode {
        predicate: query.predicate,
        caster: query.subject_scope.map(spell_caster_scope),
    })
}

/// Reads a condition's object query. Its parent owns both the comma and an
/// `or` whose next token cannot begin an object predicate. The Boolean parent
/// must then recognize another complete condition atom; an unknown suffix is
/// never accepted merely because this child returned a prefix.
pub(in crate::oracle::canonical) fn recognize_object_query_before_condition_connector(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::ObjectQueryNode> {
    let query =
        recognize_object_query_internal(cursor, true, QueryRelationKind::Control, true, false)?;
    Ok(super::ObjectQueryNode {
        predicate: query.predicate,
        controller: query.subject_scope.map(object_controller_scope),
    })
}

/// The effect parent owns a comma immediately introducing `then`/`and`.
/// Ordinary comma-separated object branches keep the shared query grammar.
pub(in crate::oracle::canonical) fn recognize_object_query_before_effect_separator(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::ObjectQueryNode> {
    let query =
        recognize_object_query_internal(cursor, false, QueryRelationKind::Control, false, true)?;
    Ok(super::ObjectQueryNode {
        predicate: query.predicate,
        controller: query.subject_scope.map(object_controller_scope),
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
        false,
        QueryRelationKind::Control,
        false,
        true,
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
        true,
        QueryRelationKind::Control,
        false,
        true,
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
    })
}

fn recognize_object_query_internal(
    cursor: &mut OracleTokenCursor<'_>,
    comma_terminates: bool,
    relation_kind: QueryRelationKind,
    condition_connector_terminates: bool,
    effect_connector_terminates: bool,
) -> MachineResult<RecognizedObjectQuery> {
    recognize_object_query_with_cardinality_policy(
        cursor,
        comma_terminates,
        relation_kind,
        condition_connector_terminates,
        effect_connector_terminates,
        CoordinatedCardinalityPolicy::ExistingQuery,
        None,
        false,
    )
}

fn recognize_object_query_with_cardinality_policy(
    cursor: &mut OracleTokenCursor<'_>,
    comma_terminates: bool,
    relation_kind: QueryRelationKind,
    condition_connector_terminates: bool,
    effect_connector_terminates: bool,
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
                    consume_optional_selector_article(cursor)?;
                }
                let branch_start = cursor.position();
                frame
                    .union_branches
                    .push(recognize_object_intersection(cursor)?);
                let branch = cursor.token_slice(branch_start, cursor.position());
                frame
                    .branch_has_plural_surface
                    .push((0..branch.len()).any(|index| {
                        super::atom_surface_is_plural(
                            &branch[index].raw,
                            branch.get(index + 1).map(|token| token.raw.as_str()),
                        )
                    }));
                ObjectQueryState::ReadUnion
            }
            ObjectQueryState::ReadUnion => {
                cursor.enter("selector.predicate.union");
                if cursor.peek().is_some_and(|token| token == ",") {
                    frame.logical_comma = logical_properties;
                    // A comma that is followed by neither another object
                    // phrase nor a list `or` belongs to the enclosing effect
                    // list (`sacrifice a creature, discard a card`).
                    let comma_ends_effect_list = effect_connector_terminates && {
                        let checkpoint = cursor.checkpoint();
                        cursor.consume(",")?;
                        let continues = cursor.peek().is_some_and(|word| {
                            word.eq_ignore_ascii_case("or") || word.eq_ignore_ascii_case("and/or")
                        }) || coordinated_object_selector_phrase_starts(cursor);
                        cursor.rollback(checkpoint);
                        !continues
                    };
                    if comma_terminates
                        || comma_ends_effect_list
                        || (effect_connector_terminates
                            && cursor.peek_ahead(1).is_some_and(|word| {
                                word.eq_ignore_ascii_case("then")
                                    || word.eq_ignore_ascii_case("and")
                            }))
                    {
                        ObjectQueryState::Complete
                    } else {
                        cursor.consume(",")?;
                        frame.last_connector_is_exclusive = false;
                        ObjectQueryState::ReadAfterComma
                    }
                } else if cursor.peek().is_some_and(|token| {
                    token.eq_ignore_ascii_case("or")
                        || token.eq_ignore_ascii_case("and/or")
                        || (logical_properties && token.eq_ignore_ascii_case("and"))
                }) {
                    let checkpoint = cursor.checkpoint();
                    let connector = cursor.peek().expect("checked connector").to_string();
                    cursor.consume(&connector)?;
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
                    if condition_connector_terminates
                        && connector.eq_ignore_ascii_case("or")
                        && !coordinated_object_selector_phrase_starts(cursor)
                    {
                        cursor.rollback(checkpoint);
                        ObjectQueryState::Complete
                    } else {
                        frame.has_explicit_union = true;
                        cursor.require(
                            coordinated_object_selector_phrase_starts(cursor),
                            "an object type after the selector union",
                        )?;
                        ObjectQueryState::ReadObjectIntersection
                    }
                } else {
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
                } else if cursor.peek().is_some_and(|token| {
                    token.eq_ignore_ascii_case("you")
                        || token.eq_ignore_ascii_case("an")
                        || token.eq_ignore_ascii_case("your")
                }) {
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
                } else if cursor.peek().is_some_and(|token| {
                    token.eq_ignore_ascii_case("that's") || token.eq_ignore_ascii_case("that’s")
                }) {
                    let relative_word = cursor.peek().expect("checked relative word").to_string();
                    cursor.consume(&relative_word)?;
                    let relative = recognize_object_query_with_cardinality_policy(
                        cursor,
                        comma_terminates,
                        QueryRelationKind::Control,
                        condition_connector_terminates,
                        effect_connector_terminates,
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

/// Commas between negative adjectives constrain one following nominal head.
/// Explicit `or`/`and/or` remain unions; a list without a positive head remains
/// unresolved here and is rejected by the caller's existing domain validation.
fn compose_comma_separated_negative_modifiers(branches: &mut Vec<SelectorPredicate>) {
    if branches.len() < 2 {
        return;
    }
    fn negative_modifier(predicate: &SelectorPredicate) -> bool {
        match predicate {
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

fn recognize_keyword_qualifier(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SelectorPredicate> {
    recognize_keyword_qualifier_after(cursor, "with")
}

/// True when a counter qualifier (`a +1/+1 counter on it`, `one or more
/// counters on them`) begins `offset` tokens ahead. Lookahead only: the
/// counter-name child still owns the name and its diagnostics.
fn counter_qualifier_starts(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    let word = |at: usize, expected: &str| {
        cursor
            .peek_ahead(offset + at)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let after_determiner = if word(0, "a") || word(0, "an") {
        1
    } else if word(0, "one") && word(1, "or") && word(2, "more") {
        3
    } else {
        return false;
    };
    (after_determiner..after_determiner + 4).any(|at| word(at, "counter") || word(at, "counters"))
}

/// `a [KIND] counter on it` / `one or more [KIND] counters on them`
/// (CR 122.1): the object has at least one counter, of that kind when named.
/// The relation word (`with`/`without`) belongs to the caller.
fn recognize_counter_qualifier(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SelectorPredicate> {
    cursor.enter("selector.counter-qualifier.determiner");
    let plural = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("one"))
    {
        cursor.consume_phrase(&["one", "or", "more"])?;
        true
    } else {
        consume_optional_selector_article(cursor)?;
        false
    };
    cursor.enter("selector.counter-qualifier.kind");
    let counter = if cursor.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case("counter") || raw.eq_ignore_ascii_case("counters")
    }) {
        None
    } else {
        Some(super::recognize_counter_name(cursor)?)
    };
    cursor.enter("selector.counter-qualifier.noun");
    cursor.consume(if plural { "counters" } else { "counter" })?;
    cursor.enter("selector.counter-qualifier.location");
    cursor.consume("on")?;
    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("it") => cursor.consume("it")?,
        Some("them") => cursor.consume("them")?,
        _ => return Err(cursor.fail("`it` or `them` after the counter location")),
    }
    Ok(SelectorPredicate::HasCounter(counter))
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
    Ok(match alternatives.len() {
        1 => alternatives.remove(0),
        _ => SelectorPredicate::Union(alternatives),
    })
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
            .all(|predicate| predicate.supports_domain(*domain) && !predicate.has_domain(*domain))
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
                if cursor
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
                    Some(QuerySubjectScope::OpponentOfAbilityController) => {
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
    }
}

fn spell_caster_scope(scope: QuerySubjectScope) -> SpellCasterScope {
    match scope {
        QuerySubjectScope::AbilityController => SpellCasterScope::AbilityController,
        QuerySubjectScope::NotAbilityController => SpellCasterScope::NotAbilityController,
        QuerySubjectScope::OpponentOfAbilityController => {
            SpellCasterScope::OpponentOfAbilityController
        }
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
                    None => matches!(expression, super::NumericExpression::Atom(_)),
                    Some(source_available) => supported(&expression, source_available),
                };
                cursor.require(admitted, "a query expression supported by this consumer's explicit numeric/source context")?;
                right = Some(Box::new(expression));
                NumericComparisonState::DirectionOrComplete
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
