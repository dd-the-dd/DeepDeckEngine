use crate::oracle::ability_catalog::{AbilityGrammarRole, lookup_registry};
use crate::oracle::characteristic_vocabulary::{AcceptedForms, lookup_permanent_card_type};

use super::super::super::{MachineResult, OracleTokenCursor};
use super::super::{GrantedKeyword, ManaCostNode, recognize_mana_cost};
use super::KeywordAtomNode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeywordReminderNode {
    Parameterless,
    Ward,
}

enum ParameterlessReminderSchema {
    Exact(&'static [&'static str]),
    ExaltedModifier {
        prefix: &'static [&'static str],
        suffix: &'static [&'static str],
    },
    Indestructible,
    Persist,
}

fn keyword_registry_id(keyword: GrantedKeyword) -> Option<&'static str> {
    Some(match keyword {
        GrantedKeyword::Cascade => "cascade",
        GrantedKeyword::Deathtouch => "deathtouch",
        GrantedKeyword::Defender => "defender",
        GrantedKeyword::DoubleStrike => "double-strike",
        GrantedKeyword::Exalted => "exalted",
        GrantedKeyword::FirstStrike => "first-strike",
        GrantedKeyword::Flash => "flash",
        GrantedKeyword::Haste => "haste",
        GrantedKeyword::Hexproof => "hexproof",
        GrantedKeyword::Indestructible => "indestructible",
        GrantedKeyword::Lifelink => "lifelink",
        GrantedKeyword::Menace => "menace",
        GrantedKeyword::Prowess => "prowess",
        GrantedKeyword::Persist => "persist",
        GrantedKeyword::Reach => "reach",
        GrantedKeyword::Trample => "trample",
        GrantedKeyword::Vigilance => "vigilance",
        _ => return None,
    })
}

fn catalog_registers_reminder(canonical_id: &str) -> bool {
    lookup_registry(canonical_id).is_some_and(|entry| {
        entry
            .grammar
            .iter()
            .any(|grammar| grammar.role == AbilityGrammarRole::Reminder)
    })
}

fn parameterless_reminder_schema(keyword: GrantedKeyword) -> Option<ParameterlessReminderSchema> {
    Some(match keyword {
        GrantedKeyword::Cascade => ParameterlessReminderSchema::Exact(&[
            "When", "you", "cast", "this", "spell", ",", "exile", "cards", "from", "the", "top",
            "of", "your", "library", "until", "you", "exile", "a", "nonland", "card", "that",
            "costs", "less", ".", "You", "may", "cast", "it", "without", "paying", "its", "mana",
            "cost", ".", "Put", "the", "exiled", "cards", "on", "the", "bottom", "in", "a",
            "random", "order", ".",
        ]),
        GrantedKeyword::Deathtouch => ParameterlessReminderSchema::Exact(&[
            "Any", "amount", "of", "damage", "this", "deals", "to", "a", "creature", "is",
            "enough", "to", "destroy", "it", ".",
        ]),
        GrantedKeyword::Defender => {
            ParameterlessReminderSchema::Exact(&["This", "creature", "can't", "attack", "."])
        }
        GrantedKeyword::DoubleStrike => ParameterlessReminderSchema::Exact(&[
            "This",
            "creature",
            "deals",
            "both",
            "first-strike",
            "and",
            "regular",
            "combat",
            "damage",
            ".",
        ]),
        GrantedKeyword::Exalted => ParameterlessReminderSchema::ExaltedModifier {
            prefix: &[
                "Whenever", "a", "creature", "you", "control", "attacks", "alone", ",", "that",
                "creature", "gets",
            ],
            suffix: &["until", "end", "of", "turn", "."],
        },
        GrantedKeyword::FirstStrike => ParameterlessReminderSchema::Exact(&[
            "This",
            "creature",
            "deals",
            "combat",
            "damage",
            "before",
            "creatures",
            "without",
            "first",
            "strike",
            ".",
        ]),
        GrantedKeyword::Flash => ParameterlessReminderSchema::Exact(&[
            "You", "may", "cast", "this", "spell", "any", "time", "you", "could", "cast", "an",
            "instant", ".",
        ]),
        GrantedKeyword::Haste => ParameterlessReminderSchema::Exact(&[
            "This", "creature", "can", "attack", "and", "{T}", "as", "soon", "as", "it", "comes",
            "under", "your", "control", ".",
        ]),
        GrantedKeyword::Hexproof => ParameterlessReminderSchema::Exact(&[
            "This",
            "creature",
            "can't",
            "be",
            "the",
            "target",
            "of",
            "spells",
            "or",
            "abilities",
            "your",
            "opponents",
            "control",
            ".",
        ]),
        GrantedKeyword::Indestructible => ParameterlessReminderSchema::Indestructible,
        GrantedKeyword::Lifelink => ParameterlessReminderSchema::Exact(&[
            "Damage", "dealt", "by", "this", "creature", "also", "causes", "you", "to", "gain",
            "that", "much", "life", ".",
        ]),
        GrantedKeyword::Menace => ParameterlessReminderSchema::Exact(&[
            "This",
            "creature",
            "can't",
            "be",
            "blocked",
            "except",
            "by",
            "two",
            "or",
            "more",
            "creatures",
            ".",
        ]),
        GrantedKeyword::Prowess => ParameterlessReminderSchema::Exact(&[
            "Whenever",
            "you",
            "cast",
            "a",
            "noncreature",
            "spell",
            ",",
            "this",
            "creature",
            "gets",
            "+1/+1",
            "until",
            "end",
            "of",
            "turn",
            ".",
        ]),
        GrantedKeyword::Persist => ParameterlessReminderSchema::Persist,
        GrantedKeyword::Reach => ParameterlessReminderSchema::Exact(&[
            "This",
            "creature",
            "can",
            "block",
            "creatures",
            "with",
            "flying",
            ".",
        ]),
        GrantedKeyword::Trample => ParameterlessReminderSchema::Exact(&[
            "This",
            "creature",
            "can",
            "deal",
            "excess",
            "combat",
            "damage",
            "to",
            "the",
            "player",
            "or",
            "planeswalker",
            "it's",
            "attacking",
            ".",
        ]),
        GrantedKeyword::Vigilance => ParameterlessReminderSchema::Exact(&[
            "Attacking",
            "doesn't",
            "cause",
            "this",
            "creature",
            "to",
            "tap",
            ".",
        ]),
        _ => return None,
    })
}

fn consume_exact_tokens(
    cursor: &mut OracleTokenCursor<'_>,
    expected: &[&str],
) -> MachineResult<()> {
    for token in expected {
        cursor.consume(token)?;
    }
    Ok(())
}

fn consume_fixed_counter_value(
    cursor: &mut OracleTokenCursor<'_>,
    expected: &str,
    expectation: &str,
) -> MachineResult<()> {
    cursor.require(
        cursor
            .peek_token()
            .is_some_and(|token| token.token_type == "counterValue"),
        "counterValue",
    )?;
    cursor.require(cursor.peek() == Some(expected), expectation)?;
    cursor.consume_type("counterValue")?;
    Ok(())
}

fn recognize_indestructible_reminder(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("keyword.indestructible.reminder.damage-or-effects");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("damage"))
    {
        cursor.consume_phrase(&["Damage", "and"])?;
    }
    cursor.enter("keyword.indestructible.reminder.destroy-rule");
    consume_exact_tokens(
        cursor,
        &[
            "Effects", "that", "say", "\"", "destroy", "\"", "don't", "destroy", "this",
        ],
    )?;
    cursor.enter("keyword.indestructible.reminder.subject");
    let subject = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a singular permanent type in the Indestructible reminder"))?;
    cursor.require(
        lookup_permanent_card_type(subject, AcceptedForms::SingularOnly).is_some(),
        "a singular permanent type in the Indestructible reminder",
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("keyword.indestructible.reminder.terminator");
    cursor.consume(".")
}

fn recognize_persist_reminder(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("keyword.persist.reminder.condition");
    consume_exact_tokens(
        cursor,
        &[
            "When", "this", "creature", "dies", ",", "if", "it", "had", "no",
        ],
    )?;
    cursor.enter("keyword.persist.reminder.condition-counter");
    consume_fixed_counter_value(cursor, "-1/-1", "the Persist -1/-1 condition counter")?;
    cursor.enter("keyword.persist.reminder.return");
    consume_exact_tokens(
        cursor,
        &[
            "counters",
            "on",
            "it",
            ",",
            "return",
            "it",
            "to",
            "the",
            "battlefield",
            "under",
            "its",
            "owner's",
            "control",
            "with",
            "a",
        ],
    )?;
    cursor.enter("keyword.persist.reminder.return-counter");
    consume_fixed_counter_value(cursor, "-1/-1", "the Persist -1/-1 return counter")?;
    cursor.enter("keyword.persist.reminder.terminator");
    consume_exact_tokens(cursor, &["counter", "on", "it", "."])
}

fn recognize_parameterless_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    keyword: GrantedKeyword,
) -> MachineResult<KeywordReminderNode> {
    let canonical_id = keyword_registry_id(keyword)
        .ok_or_else(|| cursor.fail("a registered reminder grammar for this keyword"))?;
    cursor.enter("keyword.reminder.catalog");
    cursor.require(
        catalog_registers_reminder(canonical_id),
        "a keyword catalog entry with reminder grammar",
    )?;
    let schema = parameterless_reminder_schema(keyword)
        .ok_or_else(|| cursor.fail("a supported token reminder schema for this keyword"))?;
    cursor.enter("keyword.reminder.body");
    match schema {
        ParameterlessReminderSchema::Exact(expected) => {
            consume_exact_tokens(cursor, expected)?;
        }
        ParameterlessReminderSchema::ExaltedModifier { prefix, suffix } => {
            consume_exact_tokens(cursor, prefix)?;
            cursor.enter("keyword.exalted.reminder.modifier");
            cursor.require(
                cursor
                    .peek_token()
                    .is_some_and(|token| token.token_type == "counterValue"),
                "counterValue",
            )?;
            cursor.require(cursor.peek() == Some("+1/+1"), "the Exalted +1/+1 modifier")?;
            cursor.consume_type("counterValue")?;
            cursor.enter("keyword.reminder.body");
            consume_exact_tokens(cursor, suffix)?;
        }
        ParameterlessReminderSchema::Indestructible => {
            recognize_indestructible_reminder(cursor)?;
        }
        ParameterlessReminderSchema::Persist => {
            recognize_persist_reminder(cursor)?;
        }
    }
    Ok(KeywordReminderNode::Parameterless)
}

fn recognize_ward_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    printed_cost: &ManaCostNode,
) -> MachineResult<KeywordReminderNode> {
    cursor.enter("keyword.reminder.catalog");
    cursor.require(
        catalog_registers_reminder("ward"),
        "a Ward catalog entry with reminder grammar",
    )?;
    cursor.enter("keyword.ward.reminder.trigger");
    cursor.consume_phrase(&["Whenever", "this"])?;
    cursor.enter("keyword.ward.reminder.subject");
    let subject = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a singular permanent type in the Ward reminder"))?;
    cursor.require(
        lookup_permanent_card_type(subject, AcceptedForms::SingularOnly).is_some(),
        "a singular permanent type in the Ward reminder",
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("keyword.ward.reminder.targeting");
    cursor.consume_phrase(&[
        "becomes", "the", "target", "of", "a", "spell", "or", "ability", "an", "opponent",
        "controls", ",", "counter", "it", "unless", "that", "player", "pays",
    ])?;
    cursor.enter("keyword.ward.reminder.cost");
    let reminder_cost = recognize_mana_cost(cursor)?;
    cursor.enter("keyword.ward.reminder.cost-match");
    cursor.require(
        printed_cost.same_symbols(&reminder_cost),
        "a reminder cost equal to the printed Ward cost",
    )?;
    cursor.enter("keyword.ward.reminder.terminator");
    cursor.consume(".")?;
    Ok(KeywordReminderNode::Ward)
}

/// Consumes no tokens when an atom has no reminder. Once `(` is seen, this
/// child commits and validates the registered reminder schema for that exact
/// keyword atom. Parentheses are therefore never discarded as arbitrary text.
pub(super) fn recognize_optional_keyword_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    keyword: &KeywordAtomNode,
) -> MachineResult<Option<KeywordReminderNode>> {
    cursor.enter("keyword.reminder.optional");
    if cursor.peek() != Some("(") {
        return Ok(None);
    }
    cursor.enter("keyword.reminder.open");
    cursor.consume("(")?;
    let reminder = match keyword {
        KeywordAtomNode::Parameterless(keyword) => {
            recognize_parameterless_reminder(cursor, *keyword)?
        }
        KeywordAtomNode::Ward { cost } => recognize_ward_reminder(cursor, cost)?,
    };
    cursor.enter("keyword.reminder.close");
    cursor.consume(")")?;
    Ok(Some(reminder))
}
