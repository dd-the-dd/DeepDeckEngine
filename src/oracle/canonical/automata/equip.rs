//! Equip (CR 702.6a) and its restricted form (CR 702.6c):
//!
//! ```text
//! Equip    := "equip" [Quality ["creature"]] Cost [Reminder]
//! Cost     := MANA | "—" COSTS "."          (the shared cost sequence)
//! Quality  := ObjectPredicateAtom+           (`Knight`, `legendary`)
//! Reminder := "(" Cost ":" "attach" "to" "target" Object "you" "control" "."
//!             "equip" "only" "as" "a" "sorcery" "." ")"
//! Object   := "creature" | the printed Quality [ "creature" ]
//! ```
//!
//! The quality restricts the target to a creature with that quality; it is
//! read by the shared object-predicate atom reader, never matched as text.
//! `Equip planeswalker` (CR 702.6e) is a different variant and fails.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ActivationCostSequenceNode, LocalActivationCostNode, ManaCostNode, ObjectCardType,
    ObjectPredicate, recognize_activation_cost_sequence, recognize_mana_cost,
    recognize_object_predicate_atom,
};
use crate::oracle::characteristic_vocabulary::AcceptedForms;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EquipKeywordNode {
    /// The printed activation cost: one mana cost, or the shared cost
    /// sequence after an em dash (`Equip—Sacrifice a creature.`).
    pub costs: ActivationCostSequenceNode,
    /// CR 702.6c `Equip [quality]`: the legal-target restriction.
    pub quality: Option<ObjectPredicate>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EquipState {
    Keyword,
    Quality,
    Cost,
    ReminderOrComplete,
    ReminderOpen,
    ReminderCost,
    ReminderCostMatch,
    ReminderSeparator,
    ReminderAttach,
    ReminderRelation,
    ReminderTarget,
    ReminderObject,
    ReminderPlayer,
    ReminderControl,
    ReminderAttachTerminator,
    ReminderKeyword,
    ReminderOnly,
    ReminderAs,
    ReminderArticle,
    ReminderTiming,
    ReminderTimingTerminator,
    ReminderClose,
    Complete,
}

#[derive(Default)]
struct EquipMemory {
    quality: Option<ObjectPredicate>,
    printed_cost: Option<ActivationCostSequenceNode>,
    reminder_cost: Option<ActivationCostSequenceNode>,
}

fn mana_sequence(mana: ManaCostNode) -> ActivationCostSequenceNode {
    ActivationCostSequenceNode {
        costs: vec![LocalActivationCostNode::PayMana(mana)],
        connectors: Vec::new(),
    }
}

fn at_mana_symbol(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "manaSymbol")
}

fn at_word(cursor: &OracleTokenCursor<'_>, word: &str) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// Quality atoms up to `creature` or `stop`, then the optional `creature`.
/// Only a creature can be equipped (CR 301.5): any other card type is a
/// different Equip variant (CR 702.6e) or a contradiction.
fn recognize_equip_quality(
    cursor: &mut OracleTokenCursor<'_>,
    stop: impl Fn(&OracleTokenCursor<'_>) -> bool,
) -> MachineResult<Option<ObjectPredicate>> {
    let mut atoms = Vec::new();
    while !at_word(cursor, "creature") && !stop(cursor) {
        atoms.push(recognize_object_predicate_atom(
            cursor,
            AcceptedForms::SingularOnly,
        )?);
    }
    if at_word(cursor, "creature") {
        cursor.consume("creature")?;
    }
    cursor.require(
        atoms.iter().all(|atom| {
            !matches!(atom, ObjectPredicate::CardType(card_type) if *card_type != ObjectCardType::Creature)
        }),
        "a creature quality (Equip [quality], CR 702.6c)",
    )?;
    Ok(match atoms.len() {
        0 => None,
        1 => Some(atoms.remove(0)),
        _ => Some(ObjectPredicate::Intersection(atoms)),
    })
}

/// `{cost}` or `—COSTS.` (the shared cost sequence and its terminator).
fn recognize_equip_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActivationCostSequenceNode> {
    if cursor.peek() == Some("\u{2014}") {
        cursor.consume("\u{2014}")?;
        let costs = recognize_activation_cost_sequence(cursor)?;
        if cursor.peek() == Some(".") {
            cursor.consume(".")?;
        }
        return Ok(costs);
    }
    recognize_mana_cost(cursor).map(mana_sequence)
}

/// Recognizes an Equip keyword and structurally validates its optional
/// canonical attach-and-timing reminder without reparsing a text slice.
pub(in crate::oracle::canonical) fn recognize_mana_equip(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EquipKeywordNode> {
    let mut state = EquipState::Keyword;
    let mut memory = EquipMemory::default();
    loop {
        state = match state {
            EquipState::Keyword => {
                cursor.enter("keyword.equip.keyword");
                cursor.consume("equip")?;
                EquipState::Quality
            }
            EquipState::Quality => {
                cursor.enter("keyword.equip.quality");
                if !at_mana_symbol(cursor) && cursor.peek() != Some("\u{2014}") {
                    memory.quality = recognize_equip_quality(cursor, at_mana_symbol)?;
                    cursor.require(memory.quality.is_some(), "a quality before the Equip cost")?;
                }
                EquipState::Cost
            }
            EquipState::Cost => {
                cursor.enter("keyword.equip.cost");
                memory.printed_cost = Some(recognize_equip_cost(cursor)?);
                EquipState::ReminderOrComplete
            }
            EquipState::ReminderOrComplete => {
                cursor.enter("keyword.equip.reminder-or-complete");
                match cursor.peek() {
                    None => EquipState::Complete,
                    Some("(") => EquipState::ReminderOpen,
                    _ => return Err(cursor.fail("a canonical Equip reminder or end of input")),
                }
            }
            EquipState::ReminderOpen => {
                cursor.enter("keyword.equip.reminder.open");
                cursor.consume("(")?;
                EquipState::ReminderCost
            }
            EquipState::ReminderCost => {
                cursor.enter("keyword.equip.reminder.cost");
                memory.reminder_cost = Some(if at_mana_symbol(cursor) {
                    mana_sequence(recognize_mana_cost(cursor)?)
                } else {
                    recognize_activation_cost_sequence(cursor)?
                });
                EquipState::ReminderCostMatch
            }
            EquipState::ReminderCostMatch => {
                cursor.enter("keyword.equip.reminder.cost-match");
                let printed = memory
                    .printed_cost
                    .as_ref()
                    .ok_or_else(|| cursor.fail("the printed Equip cost"))?;
                let reminder = memory
                    .reminder_cost
                    .as_ref()
                    .ok_or_else(|| cursor.fail("the reminder Equip cost"))?;
                let same = match (printed.costs.as_slice(), reminder.costs.as_slice()) {
                    (
                        [LocalActivationCostNode::PayMana(printed)],
                        [LocalActivationCostNode::PayMana(reminder)],
                    ) => printed.same_symbols(reminder),
                    _ => printed == reminder,
                };
                cursor.require(same, "a reminder cost equal to the printed Equip cost")?;
                EquipState::ReminderSeparator
            }
            EquipState::ReminderSeparator => {
                cursor.enter("keyword.equip.reminder.separator");
                cursor.consume(":")?;
                EquipState::ReminderAttach
            }
            EquipState::ReminderAttach => {
                cursor.enter("keyword.equip.reminder.attach");
                cursor.consume("attach")?;
                EquipState::ReminderRelation
            }
            EquipState::ReminderRelation => {
                cursor.enter("keyword.equip.reminder.relation");
                cursor.consume("to")?;
                EquipState::ReminderTarget
            }
            EquipState::ReminderTarget => {
                cursor.enter("keyword.equip.reminder.target");
                cursor.consume("target")?;
                EquipState::ReminderObject
            }
            EquipState::ReminderObject => {
                cursor.enter("keyword.equip.reminder.object");
                // The reminder restates the printed quality (`target
                // Halfling you control`) or names an unrestricted creature.
                let restated = recognize_equip_quality(cursor, |cursor| at_word(cursor, "you"))?;
                cursor.require(
                    restated == memory.quality,
                    "a reminder target equal to the printed Equip quality",
                )?;
                EquipState::ReminderPlayer
            }
            EquipState::ReminderPlayer => {
                cursor.enter("keyword.equip.reminder.player");
                cursor.consume("you")?;
                EquipState::ReminderControl
            }
            EquipState::ReminderControl => {
                cursor.enter("keyword.equip.reminder.control");
                cursor.consume("control")?;
                EquipState::ReminderAttachTerminator
            }
            EquipState::ReminderAttachTerminator => {
                cursor.enter("keyword.equip.reminder.attach-terminator");
                cursor.consume(".")?;
                EquipState::ReminderKeyword
            }
            EquipState::ReminderKeyword => {
                cursor.enter("keyword.equip.reminder.keyword");
                cursor.consume("equip")?;
                EquipState::ReminderOnly
            }
            EquipState::ReminderOnly => {
                cursor.enter("keyword.equip.reminder.only");
                cursor.consume("only")?;
                EquipState::ReminderAs
            }
            EquipState::ReminderAs => {
                cursor.enter("keyword.equip.reminder.as");
                cursor.consume("as")?;
                EquipState::ReminderArticle
            }
            EquipState::ReminderArticle => {
                cursor.enter("keyword.equip.reminder.article");
                cursor.consume("a")?;
                EquipState::ReminderTiming
            }
            EquipState::ReminderTiming => {
                cursor.enter("keyword.equip.reminder.timing");
                cursor.consume("sorcery")?;
                EquipState::ReminderTimingTerminator
            }
            EquipState::ReminderTimingTerminator => {
                cursor.enter("keyword.equip.reminder.timing-terminator");
                cursor.consume(".")?;
                EquipState::ReminderClose
            }
            EquipState::ReminderClose => {
                cursor.enter("keyword.equip.reminder.close");
                cursor.consume(")")?;
                EquipState::Complete
            }
            EquipState::Complete => {
                cursor.enter("keyword.equip.complete");
                cursor.finish()?;
                return Ok(EquipKeywordNode {
                    costs: cursor.require_value(memory.printed_cost, "a populated Equip cost")?,
                    quality: memory.quality,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "equip_quality_tests.rs"]
mod quality_tests;
