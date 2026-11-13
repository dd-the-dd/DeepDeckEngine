use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ContextualListConnector, ContextualListPolicy, LocalConditionNode, recognize_contextual_list,
    recognize_controlled_permanent_count_query,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum BooleanConditionNode<A = LocalConditionNode> {
    Atom(A),
    And(Vec<BooleanConditionNode<A>>),
    Or(Vec<BooleanConditionNode<A>>),
    Not(Box<BooleanConditionNode<A>>),
}

/// Recognizes an arbitrary-length Boolean list over a parent-supplied atom
/// grammar. The generic layer owns only Boolean negation, list punctuation,
/// and precedence; it never reparses an atom's source text.
pub(in crate::oracle::canonical) fn recognize_boolean_condition_list<A>(
    cursor: &mut OracleTokenCursor<'_>,
    mut recognize_atom: impl FnMut(&mut OracleTokenCursor<'_>) -> MachineResult<A>,
) -> MachineResult<BooleanConditionNode<A>> {
    let list = recognize_contextual_list(
        cursor,
        ContextualListPolicy::serial(&[ContextualListConnector::And, ContextualListConnector::Or]),
        |cursor| recognize_boolean_atom(cursor, &mut recognize_atom),
    )?;
    lower_boolean_precedence(cursor, list.items, list.connectors)
}

fn recognize_boolean_atom<A>(
    cursor: &mut OracleTokenCursor<'_>,
    recognize_atom: &mut impl FnMut(&mut OracleTokenCursor<'_>) -> MachineResult<A>,
) -> MachineResult<BooleanConditionNode<A>> {
    cursor.enter("condition.boolean.atom-or-negation");
    let mut negated = false;
    while cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("not"))
    {
        cursor.consume("not")?;
        negated = !negated;
        cursor.enter("condition.boolean.negated-atom");
    }
    let atom = BooleanConditionNode::Atom(recognize_atom(cursor)?);
    Ok(if negated {
        BooleanConditionNode::Not(Box::new(atom))
    } else {
        atom
    })
}

/// Applies conventional Boolean precedence: conjunction binds more tightly
/// than disjunction. Homogeneous children are flattened into n-ary nodes.
fn lower_boolean_precedence<A>(
    cursor: &OracleTokenCursor<'_>,
    items: Vec<BooleanConditionNode<A>>,
    connectors: Vec<ContextualListConnector>,
) -> MachineResult<BooleanConditionNode<A>> {
    cursor.require(
        !items.is_empty() && items.len() == connectors.len() + 1,
        "a complete Boolean list",
    )?;

    let mut items = items.into_iter();
    let first = items
        .next()
        .expect("the Boolean list was checked as nonempty");
    let mut conjunction = vec![first];
    let mut disjunction = Vec::new();
    for (connector, item) in connectors.into_iter().zip(items) {
        match connector {
            ContextualListConnector::And => conjunction.push(item),
            ContextualListConnector::Or => {
                disjunction.push(fold_and(conjunction));
                conjunction = vec![item];
            }
            ContextualListConnector::Then => {
                return Err(cursor.fail("only `and` or `or` inside a Boolean condition"));
            }
        }
    }
    disjunction.push(fold_and(conjunction));
    Ok(if disjunction.len() == 1 {
        disjunction
            .pop()
            .expect("one completed Boolean disjunction branch")
    } else {
        BooleanConditionNode::Or(disjunction)
    })
}

fn fold_and<A>(mut conditions: Vec<BooleanConditionNode<A>>) -> BooleanConditionNode<A> {
    if conditions.len() == 1 {
        conditions
            .pop()
            .expect("one completed Boolean conjunction branch")
    } else {
        BooleanConditionNode::And(conditions)
    }
}

/// First integration leaf: compose the existing controlled-permanent-count
/// atom without changing that atom or duplicating its selector grammar. The
/// surrounding ability parent remains responsible for `if` and the final
/// clause separator.
pub(in crate::oracle::canonical) fn recognize_controlled_count_boolean_condition_body(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<BooleanConditionNode> {
    recognize_boolean_condition_list(cursor, |cursor| {
        recognize_controlled_permanent_count_query(cursor)
            .map(LocalConditionNode::ControlledPermanentCount)
    })
}

#[cfg(test)]
#[path = "boolean_condition_tests.rs"]
mod tests;
