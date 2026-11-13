use crate::oracle::characteristic_vocabulary::{AcceptedForms, lookup_creature_subtype_tokens};

use super::super::{MachineResult, OracleTokenCursor};
use super::CreatureSubtype;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TokenSubtypeListNode {
    pub subtypes: Vec<CreatureSubtype>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenSubtypeListState {
    Start,
    ItemOrCreatureNoun,
    Complete,
}

/// Recognizes the exact creature-subtype list preceding `creature` in a token
/// descriptor. Each item must already be lexical `subtypeWord` evidence from
/// the versioned CR 205.3m registry. The child leaves the `creature` noun for
/// its parent and never infers a subtype from capitalization.
pub(super) fn recognize_token_subtype_list(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TokenSubtypeListNode> {
    let mut state = TokenSubtypeListState::Start;
    let mut subtypes = Vec::new();
    loop {
        state = match state {
            TokenSubtypeListState::Start => {
                cursor.enter("effect.create-token.subtype-list.start");
                TokenSubtypeListState::ItemOrCreatureNoun
            }
            TokenSubtypeListState::ItemOrCreatureNoun => {
                cursor.enter("effect.create-token.subtype-list.item-or-creature");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("creature"))
                {
                    cursor.require(
                        !subtypes.is_empty(),
                        "one or more registered singular creature subtypes",
                    )?;
                    TokenSubtypeListState::Complete
                } else {
                    let first = cursor
                        .peek()
                        .ok_or_else(|| cursor.fail("a registered singular creature subtype"))?;
                    let (subtype, width) = lookup_creature_subtype_tokens(
                        first,
                        cursor.peek_ahead(1),
                        AcceptedForms::SingularOnly,
                    )
                    .ok_or_else(|| cursor.fail("a registered singular creature subtype"))?;
                    cursor.require(
                        cursor
                            .token_slice(cursor.position(), cursor.position() + width)
                            .iter()
                            .all(|token| token.token_type == "subtypeWord"),
                        "lexical subtypeWord evidence from CR 205.3m",
                    )?;
                    cursor.require(!subtypes.contains(&subtype), "distinct creature subtypes")?;
                    subtypes.push(subtype);
                    cursor.advance_to(cursor.position() + width)?;
                    TokenSubtypeListState::ItemOrCreatureNoun
                }
            }
            TokenSubtypeListState::Complete => {
                cursor.enter("effect.create-token.subtype-list.complete");
                return Ok(TokenSubtypeListNode { subtypes });
            }
        };
    }
}

#[cfg(test)]
#[path = "token_subtype_list_tests.rs"]
mod tests;
