use super::super::{MachineResult, OracleTokenCursor};
use super::{CharacteristicBatch, CharacteristicBatchReminderNode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CharacteristicBatchExpansion {
    SubtypeUnion(&'static [CharacteristicBatchMember]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CharacteristicBatchMember {
    pub subtype: &'static str,
    pub reminder_plural: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CharacteristicBatchDefinition {
    pub singular: &'static str,
    pub plural: &'static str,
    pub expansion: CharacteristicBatchExpansion,
}

const OUTLAW_MEMBERS: &[CharacteristicBatchMember] = &[
    CharacteristicBatchMember {
        subtype: "Assassin",
        reminder_plural: "Assassins",
    },
    CharacteristicBatchMember {
        subtype: "Mercenary",
        reminder_plural: "Mercenaries",
    },
    CharacteristicBatchMember {
        subtype: "Pirate",
        reminder_plural: "Pirates",
    },
    CharacteristicBatchMember {
        subtype: "Rogue",
        reminder_plural: "Rogues",
    },
    CharacteristicBatchMember {
        subtype: "Warlock",
        reminder_plural: "Warlocks",
    },
];

pub(in crate::oracle::canonical) fn characteristic_batch_definition(
    batch: CharacteristicBatch,
) -> CharacteristicBatchDefinition {
    match batch {
        CharacteristicBatch::Outlaw => CharacteristicBatchDefinition {
            singular: "outlaw",
            plural: "outlaws",
            expansion: CharacteristicBatchExpansion::SubtypeUnion(OUTLAW_MEMBERS),
        },
    }
}

pub(in crate::oracle::canonical) fn characteristic_batch_from_word(
    raw: &str,
) -> Option<CharacteristicBatch> {
    [CharacteristicBatch::Outlaw].into_iter().find(|batch| {
        let definition = characteristic_batch_definition(*batch);
        raw.eq_ignore_ascii_case(definition.singular) || raw.eq_ignore_ascii_case(definition.plural)
    })
}

pub(in crate::oracle::canonical) fn canonical_batch_member(raw: &str) -> Option<&'static str> {
    [CharacteristicBatch::Outlaw]
        .into_iter()
        .flat_map(
            |batch| match characteristic_batch_definition(batch).expansion {
                CharacteristicBatchExpansion::SubtypeUnion(members) => members.iter().copied(),
            },
        )
        .find(|member| {
            raw.eq_ignore_ascii_case(member.subtype)
                || raw.eq_ignore_ascii_case(member.reminder_plural)
        })
        .map(|member| member.subtype)
}

pub(in crate::oracle::canonical) fn characteristic_batch_members(
    batch: CharacteristicBatch,
) -> &'static [CharacteristicBatchMember] {
    match characteristic_batch_definition(batch).expansion {
        CharacteristicBatchExpansion::SubtypeUnion(members) => members,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BatchReminderState {
    Open,
    Member,
    SeparatorOrCopula,
    Batch,
    DefinitionTerminator,
    ComplementOrClose,
    ComplementSubject,
    ComplementPredicate,
    ComplementTerminator,
    Close,
    Complete,
}

#[derive(Default)]
struct BatchReminderFrame {
    members: Vec<&'static str>,
    batch: Option<CharacteristicBatch>,
}

/// Consumes a parenthetical characteristic-batch definition from the shared
/// token stream. The semantic member list must exactly match the central
/// registry; reminder prose is never stripped or reparsed as raw text.
pub(in crate::oracle::canonical) fn recognize_characteristic_batch_reminder(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CharacteristicBatchReminderNode> {
    let mut state = BatchReminderState::Open;
    let mut frame = BatchReminderFrame::default();
    loop {
        state = match state {
            BatchReminderState::Open => {
                cursor.enter("reminder.characteristic-batch.open");
                cursor.consume("(")?;
                BatchReminderState::Member
            }
            BatchReminderState::Member => {
                cursor.enter("reminder.characteristic-batch.member");
                let raw = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("a registered batch member subtype"))?
                    .to_string();
                let member = canonical_batch_member(&raw)
                    .ok_or_else(|| cursor.fail("a registered batch member subtype"))?;
                cursor.advance_to(cursor.position() + 1)?;
                frame.members.push(member);
                BatchReminderState::SeparatorOrCopula
            }
            BatchReminderState::SeparatorOrCopula => {
                cursor.enter("reminder.characteristic-batch.separator-or-copula");
                match cursor.peek() {
                    Some(raw) if raw == "," => {
                        cursor.consume(",")?;
                        if cursor
                            .peek()
                            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                        {
                            cursor.consume("and")?;
                        }
                        BatchReminderState::Member
                    }
                    Some(raw) if raw.eq_ignore_ascii_case("and") => {
                        cursor.consume("and")?;
                        BatchReminderState::Member
                    }
                    Some(raw) if raw.eq_ignore_ascii_case("are") => {
                        cursor.consume("are")?;
                        BatchReminderState::Batch
                    }
                    _ => return Err(cursor.fail("a list separator or `are`")),
                }
            }
            BatchReminderState::Batch => {
                cursor.enter("reminder.characteristic-batch.batch");
                let raw = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("a registered characteristic batch"))?
                    .to_string();
                let batch = characteristic_batch_from_word(&raw)
                    .ok_or_else(|| cursor.fail("a registered characteristic batch"))?;
                cursor.require(
                    raw.eq_ignore_ascii_case(characteristic_batch_definition(batch).plural),
                    "the plural characteristic-batch noun",
                )?;
                cursor.advance_to(cursor.position() + 1)?;
                cursor.enter("reminder.characteristic-batch.members-validation");
                cursor.require(
                    frame
                        .members
                        .iter()
                        .copied()
                        .eq(characteristic_batch_members(batch)
                            .iter()
                            .map(|member| member.subtype)),
                    "the registry's complete ordered batch member list",
                )?;
                frame.batch = Some(batch);
                BatchReminderState::DefinitionTerminator
            }
            BatchReminderState::DefinitionTerminator => {
                cursor.enter("reminder.characteristic-batch.definition-terminator");
                cursor.consume(".")?;
                BatchReminderState::ComplementOrClose
            }
            BatchReminderState::ComplementOrClose => {
                cursor.enter("reminder.characteristic-batch.complement-or-close");
                if cursor.peek().is_some_and(|raw| raw == ")") {
                    BatchReminderState::Close
                } else {
                    BatchReminderState::ComplementSubject
                }
            }
            BatchReminderState::ComplementSubject => {
                cursor.enter("reminder.characteristic-batch.complement-subject");
                cursor.consume_phrase(&["Everyone", "else"])?;
                BatchReminderState::ComplementPredicate
            }
            BatchReminderState::ComplementPredicate => {
                cursor.enter("reminder.characteristic-batch.complement-predicate");
                cursor.consume_phrase(&["is", "fair", "game"])?;
                BatchReminderState::ComplementTerminator
            }
            BatchReminderState::ComplementTerminator => {
                cursor.enter("reminder.characteristic-batch.complement-terminator");
                cursor.consume(".")?;
                BatchReminderState::Close
            }
            BatchReminderState::Close => {
                cursor.enter("reminder.characteristic-batch.close");
                cursor.consume(")")?;
                BatchReminderState::Complete
            }
            BatchReminderState::Complete => {
                cursor.enter("reminder.characteristic-batch.complete");
                return Ok(CharacteristicBatchReminderNode {
                    batch: cursor.require_value(frame.batch, "a resolved characteristic batch")?,
                });
            }
        };
    }
}
