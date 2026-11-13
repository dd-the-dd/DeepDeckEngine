use super::super::{MachineResult, OracleTokenCursor};
use super::{GrantedKeyword, ObjectCardType, recognize_keyword_list};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AttachmentRelationNode {
    Equipped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AttachedKeywordGrantNode {
    pub relation: AttachmentRelationNode,
    pub object_type: ObjectCardType,
    pub keywords: Vec<GrantedKeyword>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttachedKeywordGrantState {
    Relation,
    ObjectType,
    Predicate,
    Keywords,
    Terminator,
    ReminderOrComplete,
    Complete,
}

fn consume_at(
    cursor: &mut OracleTokenCursor<'_>,
    state: &'static str,
    expected: &str,
) -> MachineResult<()> {
    cursor.enter(state);
    cursor.consume(expected)
}

fn recognize_protection_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    keywords: &[GrantedKeyword],
    saw_hexproof: &mut bool,
    saw_shroud: &mut bool,
) -> MachineResult<()> {
    consume_at(cursor, "attachment.keyword-reminder.protection.be", "be")?;
    consume_at(
        cursor,
        "attachment.keyword-reminder.protection.article",
        "the",
    )?;
    consume_at(
        cursor,
        "attachment.keyword-reminder.protection.target",
        "target",
    )?;
    consume_at(cursor, "attachment.keyword-reminder.protection.of", "of")?;
    consume_at(
        cursor,
        "attachment.keyword-reminder.protection.spells",
        "spells",
    )?;
    consume_at(
        cursor,
        "attachment.keyword-reminder.protection.connector",
        "or",
    )?;
    consume_at(
        cursor,
        "attachment.keyword-reminder.protection.abilities",
        "abilities",
    )?;
    cursor.enter("attachment.keyword-reminder.protection.scope-or-terminator");
    if cursor.peek().is_some_and(|raw| raw == ".") {
        cursor.require(
            keywords.contains(&GrantedKeyword::Shroud) && !*saw_shroud,
            "an unrepeated shroud keyword matching its reminder",
        )?;
        *saw_shroud = true;
    } else {
        consume_at(
            cursor,
            "attachment.keyword-reminder.hexproof.possessive",
            "your",
        )?;
        consume_at(
            cursor,
            "attachment.keyword-reminder.hexproof.players",
            "opponents",
        )?;
        consume_at(
            cursor,
            "attachment.keyword-reminder.hexproof.control",
            "control",
        )?;
        cursor.require(
            keywords.contains(&GrantedKeyword::Hexproof) && !*saw_hexproof,
            "an unrepeated hexproof keyword matching its reminder",
        )?;
        *saw_hexproof = true;
    }
    consume_at(
        cursor,
        "attachment.keyword-reminder.protection.terminator",
        ".",
    )
}

fn recognize_haste_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    keywords: &[GrantedKeyword],
    saw_haste: &mut bool,
) -> MachineResult<()> {
    consume_at(cursor, "attachment.keyword-reminder.haste.attack", "attack")?;
    consume_at(cursor, "attachment.keyword-reminder.haste.connector", "and")?;
    cursor.enter("attachment.keyword-reminder.haste.tap");
    let tap = cursor.consume_type("tapSymbol")?;
    cursor.require(tap.raw.eq_ignore_ascii_case("{T}"), "the {T} symbol")?;
    for (state, token) in [
        ("attachment.keyword-reminder.haste.no", "no"),
        ("attachment.keyword-reminder.haste.matter", "matter"),
        ("attachment.keyword-reminder.haste.when", "when"),
        ("attachment.keyword-reminder.haste.subject", "it"),
        ("attachment.keyword-reminder.haste.came", "came"),
        ("attachment.keyword-reminder.haste.under", "under"),
        ("attachment.keyword-reminder.haste.possessive", "your"),
        ("attachment.keyword-reminder.haste.control", "control"),
    ] {
        consume_at(cursor, state, token)?;
    }
    cursor.require(
        keywords.contains(&GrantedKeyword::Haste) && !*saw_haste,
        "an unrepeated haste keyword matching its reminder",
    )?;
    *saw_haste = true;
    consume_at(cursor, "attachment.keyword-reminder.haste.terminator", ".")
}

fn recognize_keyword_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    keywords: &[GrantedKeyword],
) -> MachineResult<()> {
    consume_at(cursor, "attachment.keyword-reminder.open", "(")?;
    let mut saw_hexproof = false;
    let mut saw_shroud = false;
    let mut saw_haste = false;
    while !cursor.peek().is_some_and(|raw| raw == ")") {
        consume_at(cursor, "attachment.keyword-reminder.subject", "it")?;
        cursor.enter("attachment.keyword-reminder.modality");
        match cursor.peek() {
            Some(raw) if raw.eq_ignore_ascii_case("can't") => {
                cursor.consume("can't")?;
                recognize_protection_reminder(
                    cursor,
                    keywords,
                    &mut saw_hexproof,
                    &mut saw_shroud,
                )?;
            }
            Some(raw) if raw.eq_ignore_ascii_case("can") => {
                cursor.consume("can")?;
                recognize_haste_reminder(cursor, keywords, &mut saw_haste)?;
            }
            _ => return Err(cursor.fail("`can't` or `can` in a supported keyword reminder")),
        }
    }
    consume_at(cursor, "attachment.keyword-reminder.close", ")")
}

/// Recognizes a continuous keyword grant to the creature attached through an
/// Equipment relation. Keyword atoms and canonical reminders remain reusable
/// child grammars rather than source-card branches.
pub(in crate::oracle::canonical) fn recognize_attached_keyword_grant(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AttachedKeywordGrantNode> {
    let mut state = AttachedKeywordGrantState::Relation;
    let mut keywords = None;
    loop {
        state = match state {
            AttachedKeywordGrantState::Relation => {
                cursor.enter("attachment.keyword-grant.relation");
                cursor.consume("equipped")?;
                AttachedKeywordGrantState::ObjectType
            }
            AttachedKeywordGrantState::ObjectType => {
                cursor.enter("attachment.keyword-grant.object-type");
                cursor.consume("creature")?;
                AttachedKeywordGrantState::Predicate
            }
            AttachedKeywordGrantState::Predicate => {
                cursor.enter("attachment.keyword-grant.predicate");
                cursor.consume("has")?;
                AttachedKeywordGrantState::Keywords
            }
            AttachedKeywordGrantState::Keywords => {
                cursor.enter("attachment.keyword-grant.keywords");
                keywords = Some(recognize_keyword_list(cursor)?);
                AttachedKeywordGrantState::Terminator
            }
            AttachedKeywordGrantState::Terminator => {
                cursor.enter("attachment.keyword-grant.terminator");
                cursor.consume(".")?;
                AttachedKeywordGrantState::ReminderOrComplete
            }
            AttachedKeywordGrantState::ReminderOrComplete => {
                cursor.enter("attachment.keyword-grant.reminder-or-complete");
                if cursor.peek().is_some_and(|raw| raw == "(") {
                    let parsed_keywords = keywords
                        .as_deref()
                        .ok_or_else(|| cursor.fail("a populated keyword list"))?;
                    recognize_keyword_reminder(cursor, parsed_keywords)?;
                }
                AttachedKeywordGrantState::Complete
            }
            AttachedKeywordGrantState::Complete => {
                cursor.enter("attachment.keyword-grant.complete");
                cursor.finish()?;
                return Ok(AttachedKeywordGrantNode {
                    relation: AttachmentRelationNode::Equipped,
                    object_type: ObjectCardType::Creature,
                    keywords: cursor.require_value(keywords, "one or more granted keywords")?,
                });
            }
        };
    }
}
