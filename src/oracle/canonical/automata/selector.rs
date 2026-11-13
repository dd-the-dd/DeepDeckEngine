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
    postnominal_qualifiers: Vec<PostnominalQualifier>,
    subject_scope: Option<QuerySubjectScope>,
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
}

/// Recognizes an object predicate plus its optional controller relation,
/// independently of whether a parent uses it as a target or as a set query.
pub(in crate::oracle::canonical) fn recognize_object_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::ObjectQueryNode> {
    let query = recognize_object_query_internal(cursor, false, QueryRelationKind::Control)?;
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
    let query = recognize_object_query_internal(cursor, true, QueryRelationKind::Control)?;
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
    let query = recognize_object_query_internal(cursor, true, QueryRelationKind::Cast)?;
    Ok(super::SpellCastQueryNode {
        predicate: query.predicate,
        caster: query.subject_scope.map(spell_caster_scope),
    })
}

fn recognize_object_query_internal(
    cursor: &mut OracleTokenCursor<'_>,
    comma_terminates: bool,
    relation_kind: QueryRelationKind,
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
                frame
                    .union_branches
                    .push(recognize_object_intersection(cursor)?);
                ObjectQueryState::ReadUnion
            }
            ObjectQueryState::ReadUnion => {
                cursor.enter("selector.predicate.union");
                if cursor.peek().is_some_and(|token| token == ",") {
                    if comma_terminates {
                        ObjectQueryState::Complete
                    } else {
                        cursor.consume(",")?;
                        ObjectQueryState::ReadAfterComma
                    }
                } else if cursor.peek().is_some_and(|token| {
                    token.eq_ignore_ascii_case("or") || token.eq_ignore_ascii_case("and/or")
                }) {
                    let connector = cursor.peek().expect("checked connector").to_string();
                    cursor.consume(&connector)?;
                    if !object_selector_phrase_starts(cursor) {
                        return Err(cursor.fail("an object type after the selector union"));
                    }
                    ObjectQueryState::ReadObjectIntersection
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
                }
                cursor.require(
                    object_selector_phrase_starts(cursor),
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
                            recognize_numeric_comparison_qualifier(cursor)?,
                            PostnominalQualifierScope::WholeQuery,
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
                } else if cursor.peek().is_some_and(|token| {
                    token.eq_ignore_ascii_case("you")
                        || token.eq_ignore_ascii_case("an")
                        || token.eq_ignore_ascii_case("your")
                }) {
                    cursor.require(frame.subject_scope.is_none(), "at most one player relation")?;
                    frame.subject_scope =
                        Some(recognize_object_subject_scope(cursor, relation_kind)?);
                    ObjectQueryState::ReadPostnominalQualifier
                } else if cursor.peek().is_some_and(|token| {
                    token.eq_ignore_ascii_case("that's") || token.eq_ignore_ascii_case("that’s")
                }) {
                    let relative_word = cursor.peek().expect("checked relative word").to_string();
                    cursor.consume(&relative_word)?;
                    let relative = recognize_object_query_internal(
                        cursor,
                        comma_terminates,
                        QueryRelationKind::Control,
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
                let shared_qualifiers = compose_postnominal_scopes(
                    cursor,
                    &mut frame.union_branches,
                    frame.postnominal_qualifiers,
                )?;
                let base = match frame.union_branches.len() {
                    0 => return Err(cursor.fail("a populated selector predicate")),
                    1 => frame.union_branches.pop().expect("one branch"),
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
                });
            }
        };
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
    cursor.enter("selector.keyword-qualifier.relation");
    cursor.consume("with")?;
    cursor.enter("selector.keyword-qualifier.keyword");
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a supported keyword after `with`"))?;
    cursor.require(
        parameterless_keyword_from_oracle_word(raw).is_some()
            || raw.eq_ignore_ascii_case("double")
            || raw.eq_ignore_ascii_case("first"),
        "a supported keyword after `with`",
    )?;
    recognize_parameterless_keyword(cursor).map(SelectorPredicate::HasKeyword)
}

/// In Oracle phrases such as `artifact or enchantment spell`, the final
/// explicit domain scopes the complete union rather than only its nearest
/// branch. Keep the union of characteristics intact and lift that trailing
/// domain into a surrounding intersection.
fn lift_shared_trailing_domain(branches: Vec<SelectorPredicate>) -> SelectorPredicate {
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
                        })?;
                        ControllerQualifierState::Complete
                    }
                    Some(QuerySubjectScope::OpponentOfAbilityController) => {
                        cursor.consume(match (relation_kind, plural_opponents) {
                            (QueryRelationKind::Control, true) => "control",
                            (QueryRelationKind::Control, false) => "controls",
                            (QueryRelationKind::Cast, true) => "cast",
                            (QueryRelationKind::Cast, false) => "casts",
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
) -> MachineResult<SelectorPredicate> {
    let mut state = NumericComparisonState::Relation;
    let mut property = None;
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
                property = Some(
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
                );
                NumericComparisonState::Right
            }
            NumericComparisonState::Right => {
                cursor.enter("selector.numeric-comparison.right");
                right = Some(recognize_numeric_atom(cursor)?);
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
            cursor.enter("selector.stack-item.ability-noun");
            cursor.consume("ability")?;
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
#[path = "selector_type_union_list_tests.rs"]
mod type_union_list_tests;

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

fn player_predicate(raw: &str) -> Option<PlayerPredicate> {
    match lookup_player_role(raw)? {
        PlayerRole::Any => Some(PlayerPredicate::Any),
        PlayerRole::Opponent => Some(PlayerPredicate::Opponent),
    }
}

fn object_selector_phrase_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    object_predicate_atom_starts(cursor, AcceptedForms::SingularOrPlural)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchCardSelectorState {
    Predicate,
    Subtype,
    CardNoun,
    QualifierOrComplete,
    BasicLandTypeQualifier,
    ExactManaCostQualifier,
    Complete,
}

fn search_subtype_starts(raw: Option<&str>) -> bool {
    raw.is_some_and(|raw| {
        raw.chars().next().is_some_and(char::is_uppercase) && raw.chars().any(char::is_alphabetic)
    })
}

fn consume_optional_search_selector_article(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<()> {
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
    {
        let article = cursor.peek().expect("checked article").to_string();
        cursor.consume(&article)?;
    }
    Ok(())
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
        consume_optional_search_selector_article(cursor)?;
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
        consume_optional_search_selector_article(cursor)?;
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
                } else if cursor
                    .peek()
                    .and_then(|raw| lookup_supertype(raw, AcceptedForms::SingularOnly))
                    == Some(CardSupertype::Basic)
                {
                    cursor.consume(CardSupertype::Basic.oracle_singular())?;
                    predicates.push(ObjectPredicate::Supertype(CardSupertype::Basic));
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
                    {
                        SearchCardSelectorState::ExactManaCostQualifier
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
