use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CardSupertype, CharacteristicBatch, CharacteristicBatchReminderNode, ObjectCardType,
    ObjectPredicate,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CharacteristicBatchExpansion {
    /// The batch word means "has any one of these characteristics".
    CharacteristicUnion(&'static [CharacteristicBatchMember]),
}

/// One typed member characteristic of a batch word. Members reuse the shared
/// object-predicate atoms; a batch never introduces a second evaluator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum BatchMemberCharacteristic {
    Subtype(&'static str),
    CardType(ObjectCardType),
    Supertype(CardSupertype),
}

impl BatchMemberCharacteristic {
    pub(in crate::oracle::canonical) fn predicate(self) -> ObjectPredicate {
        match self {
            Self::Subtype(subtype) => ObjectPredicate::Subtype(subtype.to_string()),
            Self::CardType(card_type) => ObjectPredicate::CardType(card_type),
            Self::Supertype(supertype) => ObjectPredicate::Supertype(supertype),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CharacteristicBatchMember {
    pub characteristic: BatchMemberCharacteristic,
    /// The member's form in the official reminder (`Rogues`, `legendaries`).
    pub reminder_plural: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CharacteristicBatchDefinition {
    pub singular: &'static str,
    pub plural: &'static str,
    pub expansion: CharacteristicBatchExpansion,
}

const fn subtype_member(
    subtype: &'static str,
    reminder_plural: &'static str,
) -> CharacteristicBatchMember {
    CharacteristicBatchMember {
        characteristic: BatchMemberCharacteristic::Subtype(subtype),
        reminder_plural,
    }
}

const OUTLAW_MEMBERS: &[CharacteristicBatchMember] = &[
    subtype_member("Assassin", "Assassins"),
    subtype_member("Mercenary", "Mercenaries"),
    subtype_member("Pirate", "Pirates"),
    subtype_member("Rogue", "Rogues"),
    subtype_member("Warlock", "Warlocks"),
];

/// CR 700.6: an object is historic if it has the legendary supertype, the
/// artifact card type, or the Saga subtype.
const HISTORIC_MEMBERS: &[CharacteristicBatchMember] = &[
    CharacteristicBatchMember {
        characteristic: BatchMemberCharacteristic::CardType(ObjectCardType::Artifact),
        reminder_plural: "Artifacts",
    },
    CharacteristicBatchMember {
        characteristic: BatchMemberCharacteristic::Supertype(CardSupertype::Legendary),
        reminder_plural: "legendaries",
    },
    subtype_member("Saga", "Sagas"),
];

const CHARACTERISTIC_BATCHES: [CharacteristicBatch; 2] =
    [CharacteristicBatch::Outlaw, CharacteristicBatch::Historic];

pub(in crate::oracle::canonical) fn characteristic_batch_definition(
    batch: CharacteristicBatch,
) -> CharacteristicBatchDefinition {
    match batch {
        CharacteristicBatch::Outlaw => CharacteristicBatchDefinition {
            singular: "outlaw",
            plural: "outlaws",
            expansion: CharacteristicBatchExpansion::CharacteristicUnion(OUTLAW_MEMBERS),
        },
        // An adjective: its reminder's predicate form is the same word.
        CharacteristicBatch::Historic => CharacteristicBatchDefinition {
            singular: "historic",
            plural: "historic",
            expansion: CharacteristicBatchExpansion::CharacteristicUnion(HISTORIC_MEMBERS),
        },
    }
}

pub(in crate::oracle::canonical) fn characteristic_batch_from_word(
    raw: &str,
) -> Option<CharacteristicBatch> {
    CHARACTERISTIC_BATCHES.into_iter().find(|batch| {
        let definition = characteristic_batch_definition(*batch);
        raw.eq_ignore_ascii_case(definition.singular) || raw.eq_ignore_ascii_case(definition.plural)
    })
}

pub(in crate::oracle::canonical) fn canonical_batch_member(
    raw: &str,
) -> Option<BatchMemberCharacteristic> {
    CHARACTERISTIC_BATCHES
        .into_iter()
        .flat_map(|batch| characteristic_batch_members(batch).iter().copied())
        .find(|member| {
            raw.eq_ignore_ascii_case(member.reminder_plural)
                || matches!(
                    member.characteristic,
                    BatchMemberCharacteristic::Subtype(subtype) if raw.eq_ignore_ascii_case(subtype)
                )
        })
        .map(|member| member.characteristic)
}

pub(in crate::oracle::canonical) fn characteristic_batch_members(
    batch: CharacteristicBatch,
) -> &'static [CharacteristicBatchMember] {
    match characteristic_batch_definition(batch).expansion {
        CharacteristicBatchExpansion::CharacteristicUnion(members) => members,
    }
}

/// CR 207.2: an optional trailing batch-definition reminder. Its members are
/// validated by the registry, and the batch it defines must occur in the
/// predicate of the sentence it follows.
pub(in crate::oracle::canonical) fn recognize_optional_batch_reminder_for(
    cursor: &mut OracleTokenCursor<'_>,
    predicate: &ObjectPredicate,
) -> MachineResult<()> {
    if cursor.peek() != Some("(") {
        return Ok(());
    }
    let reminder = recognize_characteristic_batch_reminder(cursor)?;
    cursor.enter("reminder.characteristic-batch.predicate-match");
    cursor.require(
        predicate.contains_characteristic_batch(reminder.batch),
        "a reminder describing a characteristic batch of the sentence",
    )
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
    members: Vec<BatchMemberCharacteristic>,
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
                    .ok_or_else(|| cursor.fail("a registered batch member characteristic"))?
                    .to_string();
                let member = canonical_batch_member(&raw)
                    .ok_or_else(|| cursor.fail("a registered batch member characteristic"))?;
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
                            .map(|member| member.characteristic)),
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
