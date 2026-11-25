use crate::oracle::syntax::{AbilityInput, NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalObjectReference, ObjectQueryNode, recognize_object_query, recognize_source_reference,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct BlockOnlyRestrictionNode {
    pub source: LocalObjectReference,
    pub allowed_attackers: ObjectQueryNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockOnlyState {
    Source,
    Permission,
    Action,
    Quantifier,
    Selector,
    Terminator,
    Complete,
}

pub(in crate::oracle::canonical) fn claims_promoted_block_only_family(
    input: &AbilityInput<'_>,
) -> bool {
    let mut cursor = OracleTokenCursor::new("block-only-claim", input.tokens);
    recognize_source_reference(&mut cursor, input.name_links, input.name_resolution_failure).is_ok()
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("can"))
        && input
            .tokens
            .get(cursor.position().saturating_add(1))
            .is_some_and(|token| token.token_type == "actionWord")
        // `can block an additional creature` / `any number of creatures` is
        // a block capacity of the shared combat clause (CR 509.1a), not an
        // attacker restriction: only `can block only QUERY` is this family.
        && input
            .tokens
            .get(cursor.position().saturating_add(2))
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("only"))
}

pub(in crate::oracle::canonical) fn recognize_block_only_restriction(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<BlockOnlyRestrictionNode> {
    let mut state = BlockOnlyState::Source;
    let mut source = None;
    let mut allowed_attackers = None;
    loop {
        state = match state {
            BlockOnlyState::Source => {
                cursor.enter("block-restriction.source");
                source = Some(recognize_source_reference(
                    cursor,
                    name_links,
                    name_failure,
                )?);
                BlockOnlyState::Permission
            }
            BlockOnlyState::Permission => {
                cursor.enter("block-restriction.permission");
                cursor.consume("can")?;
                BlockOnlyState::Action
            }
            BlockOnlyState::Action => {
                cursor.enter("block-restriction.action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    action.raw.eq_ignore_ascii_case("block"),
                    "the `block` action",
                )?;
                BlockOnlyState::Quantifier
            }
            BlockOnlyState::Quantifier => {
                cursor.enter("block-restriction.quantifier");
                cursor.consume("only")?;
                BlockOnlyState::Selector
            }
            BlockOnlyState::Selector => {
                cursor.enter("block-restriction.selector");
                allowed_attackers = Some(recognize_object_query(cursor)?);
                BlockOnlyState::Terminator
            }
            BlockOnlyState::Terminator => {
                cursor.enter("block-restriction.terminator");
                cursor.consume(".")?;
                BlockOnlyState::Complete
            }
            BlockOnlyState::Complete => {
                cursor.enter("block-restriction.complete");
                cursor.finish()?;
                return Ok(BlockOnlyRestrictionNode {
                    source: cursor.require_value(source, "a source reference")?,
                    allowed_attackers: cursor
                        .require_value(allowed_attackers, "an allowed-attacker selector")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "block_restriction_tests.rs"]
mod tests;
