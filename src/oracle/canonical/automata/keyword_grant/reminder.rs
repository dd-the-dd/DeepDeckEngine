use crate::oracle::ability_catalog::{AbilityGrammarRole, lookup_registry};
use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, LandwalkWord, lookup_permanent_card_type,
};

use super::super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::super::{
    GrantedKeyword, ObjectCardType, ObjectPredicate, ResourcePaymentNode,
    recognize_resource_payment_predicate,
};
use super::KeywordAtomNode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeywordReminderNode {
    Parameterless,
    Ward,
}

#[derive(Clone, Copy)]
enum KeywordReminderSubjectContext<'a> {
    SelfObject,
    BoundCollection(&'a ObjectPredicate),
}

#[derive(Clone, Copy)]
enum ParameterlessReminderSchema {
    SubjectPredicate {
        subject_type: ObjectCardType,
        predicate: &'static [&'static str],
    },
    Exact(&'static [&'static str]),
    ExaltedModifier {
        prefix: &'static [&'static str],
        suffix: &'static [&'static str],
    },
    Indestructible,
    /// CR 702.14c reminder, validated against the keyword's own land subtype.
    Landwalk(LandwalkWord),
    Persist,
    /// CR 702.44a reminder: the counter kind follows the named subject type.
    Sunburst,
    Undying,
}

fn keyword_registry_id(keyword: GrantedKeyword) -> Option<&'static str> {
    Some(match keyword {
        GrantedKeyword::Cascade => "cascade",
        GrantedKeyword::Changeling => "changeling",
        GrantedKeyword::Deathtouch => "deathtouch",
        GrantedKeyword::Defender => "defender",
        GrantedKeyword::DoubleStrike => "double-strike",
        GrantedKeyword::Exalted => "exalted",
        GrantedKeyword::Fear => "fear",
        GrantedKeyword::FirstStrike => "first-strike",
        GrantedKeyword::Flash => "flash",
        GrantedKeyword::Flying => "flying",
        GrantedKeyword::Haste => "haste",
        GrantedKeyword::Hexproof => "hexproof",
        GrantedKeyword::Horsemanship => "horsemanship",
        GrantedKeyword::Indestructible => "indestructible",
        GrantedKeyword::Infect => "infect",
        GrantedKeyword::Intimidate => "intimidate",
        GrantedKeyword::Landwalk(_) => "landwalk",
        GrantedKeyword::Lifelink => "lifelink",
        GrantedKeyword::Menace => "menace",
        GrantedKeyword::Prowess => "prowess",
        GrantedKeyword::Persist => "persist",
        GrantedKeyword::Reach => "reach",
        GrantedKeyword::Shadow => "shadow",
        GrantedKeyword::Shroud => "shroud",
        GrantedKeyword::Skulk => "skulk",
        GrantedKeyword::Sunburst => "sunburst",
        GrantedKeyword::Trample => "trample",
        GrantedKeyword::Undying => "undying",
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
        GrantedKeyword::Changeling => ParameterlessReminderSchema::Exact(&[
            "This", "card", "is", "every", "creature", "type", ".",
        ]),
        GrantedKeyword::Deathtouch => ParameterlessReminderSchema::Exact(&[
            "Any", "amount", "of", "damage", "this", "deals", "to", "a", "creature", "is",
            "enough", "to", "destroy", "it", ".",
        ]),
        GrantedKeyword::Defender => ParameterlessReminderSchema::SubjectPredicate {
            subject_type: ObjectCardType::Creature,
            predicate: &["can't", "attack", "."],
        },
        GrantedKeyword::DoubleStrike => ParameterlessReminderSchema::SubjectPredicate {
            subject_type: ObjectCardType::Creature,
            predicate: &[
                "deals",
                "both",
                "first-strike",
                "and",
                "regular",
                "combat",
                "damage",
                ".",
            ],
        },
        GrantedKeyword::Exalted => ParameterlessReminderSchema::ExaltedModifier {
            prefix: &[
                "Whenever", "a", "creature", "you", "control", "attacks", "alone", ",", "that",
                "creature", "gets",
            ],
            suffix: &["until", "end", "of", "turn", "."],
        },
        GrantedKeyword::Fear => ParameterlessReminderSchema::SubjectPredicate {
            subject_type: ObjectCardType::Creature,
            predicate: &[
                "can't",
                "be",
                "blocked",
                "except",
                "by",
                "artifact",
                "creatures",
                "and/or",
                "black",
                "creatures",
                ".",
            ],
        },
        GrantedKeyword::Flying => ParameterlessReminderSchema::SubjectPredicate {
            subject_type: ObjectCardType::Creature,
            predicate: &[
                "can't",
                "be",
                "blocked",
                "except",
                "by",
                "creatures",
                "with",
                "flying",
                "or",
                "reach",
                ".",
            ],
        },
        GrantedKeyword::Horsemanship => ParameterlessReminderSchema::SubjectPredicate {
            subject_type: ObjectCardType::Creature,
            predicate: &[
                "can't",
                "be",
                "blocked",
                "except",
                "by",
                "creatures",
                "with",
                "horsemanship",
                ".",
            ],
        },
        GrantedKeyword::Infect => ParameterlessReminderSchema::SubjectPredicate {
            subject_type: ObjectCardType::Creature,
            predicate: &[
                "deals",
                "damage",
                "to",
                "creatures",
                "in",
                "the",
                "form",
                "of",
                "-1/-1",
                "counters",
                "and",
                "to",
                "players",
                "in",
                "the",
                "form",
                "of",
                "poison",
                "counters",
                ".",
            ],
        },
        GrantedKeyword::Intimidate => ParameterlessReminderSchema::SubjectPredicate {
            subject_type: ObjectCardType::Creature,
            predicate: &[
                "can't",
                "be",
                "blocked",
                "except",
                "by",
                "artifact",
                "creatures",
                "and/or",
                "creatures",
                "that",
                "share",
                "a",
                "color",
                "with",
                "it",
                ".",
            ],
        },
        GrantedKeyword::Shadow => ParameterlessReminderSchema::SubjectPredicate {
            subject_type: ObjectCardType::Creature,
            predicate: &[
                "can",
                "block",
                "or",
                "be",
                "blocked",
                "by",
                "only",
                "creatures",
                "with",
                "shadow",
                ".",
            ],
        },
        GrantedKeyword::Shroud => ParameterlessReminderSchema::SubjectPredicate {
            subject_type: ObjectCardType::Creature,
            predicate: &[
                "can't",
                "be",
                "the",
                "target",
                "of",
                "spells",
                "or",
                "abilities",
                ".",
            ],
        },
        GrantedKeyword::Skulk => ParameterlessReminderSchema::SubjectPredicate {
            subject_type: ObjectCardType::Creature,
            predicate: &[
                "can't",
                "be",
                "blocked",
                "by",
                "creatures",
                "with",
                "greater",
                "power",
                ".",
            ],
        },
        GrantedKeyword::Landwalk(word) => ParameterlessReminderSchema::Landwalk(word),
        GrantedKeyword::Undying => ParameterlessReminderSchema::Undying,
        GrantedKeyword::FirstStrike => ParameterlessReminderSchema::SubjectPredicate {
            subject_type: ObjectCardType::Creature,
            predicate: &[
                "deals",
                "combat",
                "damage",
                "before",
                "creatures",
                "without",
                "first",
                "strike",
                ".",
            ],
        },
        GrantedKeyword::Flash => ParameterlessReminderSchema::Exact(&[
            "You", "may", "cast", "this", "spell", "any", "time", "you", "could", "cast", "an",
            "instant", ".",
        ]),
        GrantedKeyword::Haste => ParameterlessReminderSchema::SubjectPredicate {
            subject_type: ObjectCardType::Creature,
            predicate: &[
                "can", "attack", "and", "{T}", "as", "soon", "as", "it", "comes", "under", "your",
                "control", ".",
            ],
        },
        GrantedKeyword::Hexproof => ParameterlessReminderSchema::SubjectPredicate {
            subject_type: ObjectCardType::Creature,
            predicate: &[
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
            ],
        },
        GrantedKeyword::Indestructible => ParameterlessReminderSchema::Indestructible,
        GrantedKeyword::Lifelink => ParameterlessReminderSchema::Exact(&[
            "Damage", "dealt", "by", "this", "creature", "also", "causes", "you", "to", "gain",
            "that", "much", "life", ".",
        ]),
        GrantedKeyword::Menace => ParameterlessReminderSchema::SubjectPredicate {
            subject_type: ObjectCardType::Creature,
            predicate: &[
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
            ],
        },
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
        GrantedKeyword::Sunburst => ParameterlessReminderSchema::Sunburst,
        GrantedKeyword::Reach => ParameterlessReminderSchema::SubjectPredicate {
            subject_type: ObjectCardType::Creature,
            predicate: &["can", "block", "creatures", "with", "flying", "."],
        },
        GrantedKeyword::Trample => ParameterlessReminderSchema::SubjectPredicate {
            subject_type: ObjectCardType::Creature,
            predicate: &[
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
            ],
        },
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

/// Further printed wordings of the same reminder (pronoun and printing
/// variants). Each is validated exactly; the primary schema is tried first.
fn parameterless_reminder_variants(
    keyword: GrantedKeyword,
) -> &'static [ParameterlessReminderSchema] {
    macro_rules! creature_predicate {
        ($($token:literal),+ $(,)?) => {
            ParameterlessReminderSchema::SubjectPredicate {
                subject_type: ObjectCardType::Creature,
                predicate: &[$($token),+],
            }
        };
    }
    macro_rules! trample_target {
        ($pronoun:literal) => {
            creature_predicate!(
                "can",
                "deal",
                "excess",
                "combat",
                "damage",
                "to",
                "the",
                "player",
                $pronoun,
                "attacking",
                "."
            )
        };
    }
    match keyword {
        GrantedKeyword::Flash => &[ParameterlessReminderSchema::Exact(&[
            "You", "may", "cast", "this", "card", "any", "time", "you", "could", "cast", "an",
            "instant", ".",
        ])],
        GrantedKeyword::Haste => &[
            creature_predicate!(
                "can", "attack", "and", "{T}", "as", "soon", "as", "he", "comes", "under", "your",
                "control", "."
            ),
            creature_predicate!(
                "can", "attack", "as", "soon", "as", "it", "comes", "under", "your", "control", "."
            ),
        ],
        GrantedKeyword::Trample => &[
            trample_target!("it's"),
            trample_target!("he's"),
            trample_target!("she's"),
            trample_target!("they're"),
        ],
        _ => &[],
    }
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
    cursor.consume(".")?;
    // Printed second sentence about zero toughness; both observed wordings
    // are registered and anything else fails at the closing parenthesis.
    if cursor.peek() == Some("If") {
        cursor.enter("keyword.indestructible.reminder.zero-toughness");
        consume_exact_tokens(
            cursor,
            &["If", "its", "toughness", "is", "0", "or", "less", ","],
        )?;
        if cursor.peek() == Some("it's") {
            consume_exact_tokens(
                cursor,
                &[
                    "it's",
                    "still",
                    "put",
                    "into",
                    "its",
                    "owner's",
                    "graveyard",
                    ".",
                ],
            )?;
        } else {
            consume_exact_tokens(cursor, &["it", "still", "dies", "."])?;
        }
    }
    Ok(())
}

/// `[This creature | They] can't be blocked as long as defending player
/// controls a[n] SUBTYPE.` The land subtype is the keyword's own parameter;
/// a reminder naming another type is an explicit failure.
fn recognize_landwalk_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    word: LandwalkWord,
    context: KeywordReminderSubjectContext<'_>,
) -> MachineResult<()> {
    cursor.enter("keyword.landwalk.reminder.subject");
    match context {
        KeywordReminderSubjectContext::SelfObject => {
            recognize_reminder_subject(cursor, ObjectCardType::Creature, context)?;
        }
        KeywordReminderSubjectContext::BoundCollection(predicate) => {
            cursor.consume("They")?;
            cursor.require(
                collection_implies_type(predicate, ObjectCardType::Creature),
                "a reminder subject implied by every object in the bound collection",
            )?;
        }
    }
    cursor.enter("keyword.landwalk.reminder.restriction");
    consume_exact_tokens(
        cursor,
        &[
            "can't",
            "be",
            "blocked",
            "as",
            "long",
            "as",
            "defending",
            "player",
            "controls",
        ],
    )?;
    cursor.enter("keyword.landwalk.reminder.land-type");
    let article = if word.subtype.starts_with(['A', 'E', 'I', 'O', 'U']) {
        "an"
    } else {
        "a"
    };
    cursor.consume(article)?;
    cursor.require(
        cursor.peek() == Some(word.subtype),
        "the land subtype named by the landwalk keyword",
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("keyword.landwalk.reminder.terminator");
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

fn recognize_undying_reminder(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("keyword.undying.reminder.condition");
    consume_exact_tokens(
        cursor,
        &[
            "When", "this", "creature", "dies", ",", "if", "it", "had", "no",
        ],
    )?;
    cursor.enter("keyword.undying.reminder.condition-counter");
    consume_fixed_counter_value(cursor, "+1/+1", "the Undying +1/+1 condition counter")?;
    cursor.enter("keyword.undying.reminder.return");
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
    cursor.enter("keyword.undying.reminder.return-counter");
    consume_fixed_counter_value(cursor, "+1/+1", "the Undying +1/+1 return counter")?;
    cursor.enter("keyword.undying.reminder.terminator");
    consume_exact_tokens(cursor, &["counter", "on", "it", "."])
}

/// `This PERMANENT_TYPE enters with a COUNTER counter on it for each color of
/// mana spent to cast it.` CR 702.44a: a creature subject names a +1/+1
/// counter and every other permanent type names a charge counter; a reminder
/// pairing a subject with the other counter kind is an explicit failure.
fn recognize_sunburst_reminder(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("keyword.sunburst.reminder.subject");
    cursor.consume("This")?;
    let subject = cursor
        .peek()
        .and_then(|raw| lookup_permanent_card_type(raw, AcceptedForms::SingularOnly))
        .ok_or_else(|| cursor.fail("a singular permanent type in the Sunburst reminder"))?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("keyword.sunburst.reminder.entry");
    consume_exact_tokens(cursor, &["enters", "with", "a"])?;
    cursor.enter("keyword.sunburst.reminder.counter");
    if subject == ObjectCardType::Creature {
        consume_fixed_counter_value(cursor, "+1/+1", "the Sunburst +1/+1 counter of a creature")?;
    } else {
        cursor.consume("charge")?;
    }
    cursor.enter("keyword.sunburst.reminder.count");
    consume_exact_tokens(
        cursor,
        &[
            "counter", "on", "it", "for", "each", "color", "of", "mana", "spent", "to", "cast",
            "it", ".",
        ],
    )
}

/// A bound reminder noun must hold for every branch of its antecedent.
/// Negative adjectives do not establish a positive nominal identity.
fn collection_implies_type(predicate: &ObjectPredicate, card_type: ObjectCardType) -> bool {
    match predicate {
        ObjectPredicate::CardType(actual) => *actual == card_type,
        ObjectPredicate::Intersection(operands) => operands
            .iter()
            .any(|operand| collection_implies_type(operand, card_type)),
        ObjectPredicate::Union(operands) => {
            !operands.is_empty()
                && operands
                    .iter()
                    .all(|operand| collection_implies_type(operand, card_type))
        }
        _ => false,
    }
}

/// Resolves a typed reminder subject against the parent's transient binding;
/// the remainder is still consumed by the registered keyword predicate schema.
fn recognize_reminder_subject(
    cursor: &mut OracleTokenCursor<'_>,
    subject_type: ObjectCardType,
    context: KeywordReminderSubjectContext<'_>,
) -> MachineResult<()> {
    cursor.enter("keyword.reminder.subject");
    match context {
        KeywordReminderSubjectContext::SelfObject => {
            // Preserve the established inline reminder diagnostic contract.
            cursor.enter("keyword.reminder.body");
            cursor.consume("This")?;
            let noun = cursor
                .peek()
                .ok_or_else(|| cursor.fail("a singular reminder subject"))?;
            cursor.require(
                lookup_permanent_card_type(noun, AcceptedForms::SingularOnly) == Some(subject_type),
                "the registered singular keyword reminder subject",
            )?;
        }
        KeywordReminderSubjectContext::BoundCollection(predicate) => {
            cursor.consume_phrase(&["each", "of", "those"])?;
            let noun = cursor
                .peek()
                .ok_or_else(|| cursor.fail("a plural reminder subject"))?;
            cursor.require(
                lookup_permanent_card_type(noun, AcceptedForms::SingularOrPlural)
                    == Some(subject_type)
                    && lookup_permanent_card_type(noun, AcceptedForms::SingularOnly).is_none(),
                "the registered plural keyword reminder subject",
            )?;
            cursor.enter("keyword.reminder.subject.binding");
            cursor.require(
                collection_implies_type(predicate, subject_type),
                "a reminder subject implied by every object in the bound collection",
            )?;
        }
    }
    cursor.advance_to(cursor.position() + 1)?;
    Ok(())
}

fn recognize_parameterless_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    keyword: GrantedKeyword,
    context: KeywordReminderSubjectContext<'_>,
) -> MachineResult<KeywordReminderNode> {
    let canonical_id = keyword_registry_id(keyword)
        .ok_or_else(|| cursor.fail("a registered reminder grammar for this keyword"))?;
    cursor.enter("keyword.reminder.catalog");
    cursor.require(
        catalog_registers_reminder(canonical_id),
        "a keyword catalog entry with reminder grammar",
    )?;
    let primary = parameterless_reminder_schema(keyword)
        .ok_or_else(|| cursor.fail("a supported token reminder schema for this keyword"))?;
    let checkpoint = cursor.checkpoint();
    let mut best_failure: Option<MachineFailure> = None;
    for (index, schema) in std::iter::once(primary)
        .chain(parameterless_reminder_variants(keyword).iter().copied())
        .enumerate()
    {
        if index > 0 {
            cursor.rollback(checkpoint);
        }
        match recognize_reminder_schema(cursor, schema, context) {
            Ok(()) => return Ok(KeywordReminderNode::Parameterless),
            Err(failure) => {
                if best_failure
                    .as_ref()
                    .is_none_or(|best| failure.token_index > best.token_index)
                {
                    best_failure = Some(failure);
                }
            }
        }
    }
    Err(best_failure.expect("the primary schema always runs"))
}

fn recognize_reminder_schema(
    cursor: &mut OracleTokenCursor<'_>,
    schema: ParameterlessReminderSchema,
    context: KeywordReminderSubjectContext<'_>,
) -> MachineResult<()> {
    cursor.enter("keyword.reminder.body");
    match schema {
        ParameterlessReminderSchema::SubjectPredicate {
            subject_type,
            predicate,
        } => {
            recognize_reminder_subject(cursor, subject_type, context)?;
            cursor.enter("keyword.reminder.body");
            consume_exact_tokens(cursor, predicate)?;
        }
        ParameterlessReminderSchema::Exact(expected) => {
            cursor.require(
                matches!(context, KeywordReminderSubjectContext::SelfObject),
                "a registered reminder predicate with a bound collection subject",
            )?;
            consume_exact_tokens(cursor, expected)?;
        }
        ParameterlessReminderSchema::ExaltedModifier { prefix, suffix } => {
            cursor.require(
                matches!(context, KeywordReminderSubjectContext::SelfObject),
                "a registered reminder predicate with a bound collection subject",
            )?;
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
            cursor.require(
                matches!(context, KeywordReminderSubjectContext::SelfObject),
                "a registered reminder predicate with a bound collection subject",
            )?;
            recognize_indestructible_reminder(cursor)?;
        }
        ParameterlessReminderSchema::Landwalk(word) => {
            recognize_landwalk_reminder(cursor, word, context)?;
        }
        ParameterlessReminderSchema::Persist => {
            cursor.require(
                matches!(context, KeywordReminderSubjectContext::SelfObject),
                "a registered reminder predicate with a bound collection subject",
            )?;
            recognize_persist_reminder(cursor)?;
        }
        ParameterlessReminderSchema::Undying => {
            cursor.require(
                matches!(context, KeywordReminderSubjectContext::SelfObject),
                "a registered reminder predicate with a bound collection subject",
            )?;
            recognize_undying_reminder(cursor)?;
        }
        ParameterlessReminderSchema::Sunburst => {
            cursor.require(
                matches!(context, KeywordReminderSubjectContext::SelfObject),
                "a registered reminder predicate with a bound collection subject",
            )?;
            recognize_sunburst_reminder(cursor)?;
        }
    }
    Ok(())
}

fn recognize_ward_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    printed_cost: &ResourcePaymentNode,
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
        "controls", ",", "counter", "it", "unless", "that", "player",
    ])?;
    cursor.enter("keyword.ward.reminder.cost");
    let reminder_cost = recognize_resource_payment_predicate(cursor)?;
    cursor.enter("keyword.ward.reminder.cost-match");
    cursor.require(
        printed_cost.same_resource(&reminder_cost),
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
    recognize_optional_keyword_reminder_in_context(
        cursor,
        keyword,
        KeywordReminderSubjectContext::SelfObject,
    )
}

fn recognize_optional_keyword_reminder_in_context(
    cursor: &mut OracleTokenCursor<'_>,
    keyword: &KeywordAtomNode,
    context: KeywordReminderSubjectContext<'_>,
) -> MachineResult<Option<KeywordReminderNode>> {
    cursor.enter("keyword.reminder.optional");
    if cursor.peek() != Some("(") {
        return Ok(None);
    }
    cursor.enter("keyword.reminder.open");
    cursor.consume("(")?;
    let reminder = match keyword {
        KeywordAtomNode::Parameterless(keyword) => {
            recognize_parameterless_reminder(cursor, *keyword, context)?
        }
        KeywordAtomNode::Ward { cost } => {
            cursor.require(
                matches!(context, KeywordReminderSubjectContext::SelfObject),
                "a Ward reminder attached to its local keyword atom",
            )?;
            recognize_ward_reminder(cursor, cost)?
        }
        KeywordAtomNode::Quoted(_) => {
            return Err(cursor.fail("a keyword atom that carries reminder text"));
        }
    };
    cursor.enter("keyword.reminder.close");
    cursor.consume(")")?;
    Ok(Some(reminder))
}

/// A trailing annotation must describe one keyword granted by this parent.
/// Every candidate reuses the same immutable tokens and typed subject binding;
/// an unknown or altered predicate is a committed failure after `(`.
pub(super) fn recognize_collection_keyword_annotation(
    cursor: &mut OracleTokenCursor<'_>,
    keywords: &[GrantedKeyword],
    objects: &ObjectPredicate,
) -> MachineResult<()> {
    recognize_collection_keyword_atom_annotation(
        cursor,
        &keywords
            .iter()
            .copied()
            .map(KeywordAtomNode::Parameterless)
            .collect::<Vec<_>>(),
        objects,
    )
}

pub(super) fn recognize_collection_keyword_atom_annotation(
    cursor: &mut OracleTokenCursor<'_>,
    keywords: &[KeywordAtomNode],
    objects: &ObjectPredicate,
) -> MachineResult<()> {
    if cursor.peek() != Some("(") {
        return Ok(());
    }
    let checkpoint = cursor.checkpoint();
    let mut best_failure = None;
    for (index, keyword) in keywords.iter().enumerate() {
        if index > 0 {
            cursor.rollback(checkpoint);
        }
        match recognize_optional_keyword_reminder_in_context(
            cursor,
            keyword,
            KeywordReminderSubjectContext::BoundCollection(objects),
        ) {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => unreachable!("annotation opener was checked"),
            Err(failure) => {
                if best_failure
                    .as_ref()
                    .is_none_or(|best: &super::super::super::MachineFailure| {
                        failure.token_index > best.token_index
                    })
                {
                    best_failure = Some(failure);
                }
            }
        }
    }
    Err(best_failure.unwrap_or_else(|| cursor.fail("a granted keyword for the annotation")))
}
