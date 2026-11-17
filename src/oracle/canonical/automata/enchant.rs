//! `Enchant OBJECT` (CR 702.5a): the keyword restricts what an Aura spell can
//! target (CR 303.4a) and what the Aura can be attached to (CR 303.4d, 704.5m).
//! The restriction is the shared target-slot object query; this envelope only
//! owns the keyword word, the Aura source face and the optional terminator.
use super::super::{MachineResult, OracleTokenCursor};
use super::{ObjectKind, TargetSelector, recognize_target_slot_selector};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EnchantKeywordNode {
    pub object: TargetSelector,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EnchantState {
    Keyword,
    Object,
    ObjectDomain,
    Terminator,
    Reminder,
    Complete,
}

/// `face_is_aura` is the caller's proof that the printed face has the Aura
/// subtype; the keyword is meaningless on any other object.
pub(in crate::oracle::canonical) fn recognize_enchant_keyword(
    cursor: &mut OracleTokenCursor<'_>,
    face_is_aura: bool,
) -> MachineResult<EnchantKeywordNode> {
    let mut state = EnchantState::Keyword;
    let mut object = None;
    let mut object_span = (0, 0);
    loop {
        state = match state {
            EnchantState::Keyword => {
                cursor.enter("keyword.enchant.keyword");
                cursor.require(face_is_aura, "an Aura source face for the Enchant keyword")?;
                cursor.consume("enchant")?;
                EnchantState::Object
            }
            EnchantState::Object => {
                cursor.enter("keyword.enchant.object");
                let start = cursor.position();
                object = Some(recognize_target_slot_selector(cursor)?);
                object_span = (start, cursor.position());
                EnchantState::ObjectDomain
            }
            EnchantState::ObjectDomain => {
                cursor.enter("keyword.enchant.object-domain");
                let selector = cursor.require_value(object.as_ref(), "an enchantable object")?;
                // The keyword's domain is the battlefield, so a bare subtype
                // (`Forest`) is a permanent query; card-domain predicates are not.
                cursor.require(
                    matches!(
                        selector,
                        TargetSelector::Object(predicate)
                            | TargetSelector::ControlledObject { predicate, .. }
                            if predicate.supports_domain(ObjectKind::Permanent)
                    ),
                    "a permanent object restriction; player, zone and cross-domain Aura attachment are not represented",
                )?;
                EnchantState::Terminator
            }
            EnchantState::Terminator => {
                cursor.enter("keyword.enchant.terminator");
                if cursor.peek() == Some(".") {
                    cursor.consume(".")?;
                }
                EnchantState::Reminder
            }
            EnchantState::Reminder => {
                cursor.enter("keyword.enchant.reminder");
                if cursor.peek() == Some("(") {
                    // The printed reminder restates the same restriction; it is
                    // validated token-for-token and never copied into IR.
                    let words = cursor
                        .token_slice(object_span.0, object_span.1)
                        .iter()
                        .map(|token| token.raw.clone())
                        .collect::<Vec<_>>();
                    cursor.consume("(")?;
                    cursor.consume("target")?;
                    if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("an"))
                    {
                        cursor.consume("an")?;
                    } else {
                        cursor.consume("a")?;
                    }
                    for word in &words {
                        cursor.consume(word)?;
                    }
                    cursor.consume_phrase(&["as", "you", "cast", "this", "."])?;
                    cursor.consume_phrase(&["this", "card", "enters", "attached", "to", "that"])?;
                    let head = words
                        .first()
                        .ok_or_else(|| cursor.fail("the restricted object head"))?;
                    cursor.consume(head)?;
                    cursor.consume_phrase(&[".", ")"])?;
                }
                EnchantState::Complete
            }
            EnchantState::Complete => {
                cursor.enter("keyword.enchant.complete");
                cursor.finish()?;
                return Ok(EnchantKeywordNode {
                    object: cursor.require_value(object, "an enchantable object")?,
                });
            }
        };
    }
}
