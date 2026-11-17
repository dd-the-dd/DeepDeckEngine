use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    EffectDurationNode, GrantedKeyword, ObjectControllerScope, ResourcePaymentNode, TargetListNode,
    recognize_controlled_object_set, recognize_target_list,
    recognize_trailing_or_supplied_duration,
};
use crate::oracle::characteristic_vocabulary::lookup_landwalk_word;

mod quoted_ability;
mod reminder;

use reminder::recognize_optional_keyword_reminder;

pub(in crate::oracle::canonical) use quoted_ability::{
    GRANTED_OBJECT_TYPE_LINE, QuotedAbilityNode, quoted_sentence_just_closed,
    recognize_quoted_ability,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TemporaryKeywordGrantObjectsNode {
    Targets(TargetListNode),
    ControlledSet(super::ObjectSetNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TemporaryKeywordGrantNode<
    Subject = TemporaryKeywordGrantObjectsNode,
> {
    pub objects: Subject,
    pub keywords: Vec<GrantedKeyword>,
    /// Quoted abilities of the granted objects (CR 113.1), compiled recursively.
    pub abilities: Vec<QuotedAbilityNode>,
    pub duration: EffectDurationNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum KeywordAtomNode {
    Parameterless(GrantedKeyword),
    Ward {
        cost: ResourcePaymentNode,
    },
    /// Produced only by a parent that grants abilities to another object.
    Quoted(QuotedAbilityNode),
}

/// Keywords and complete quoted abilities of one grant, each in Oracle order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct GrantedAbilityListNode {
    pub keywords: Vec<GrantedKeyword>,
    pub abilities: Vec<QuotedAbilityNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct KeywordListNode {
    pub keywords: Vec<KeywordAtomNode>,
}

pub(in crate::oracle::canonical) fn parameterless_keyword_from_oracle_word(
    raw: &str,
) -> Option<GrantedKeyword> {
    Some(match raw.to_ascii_lowercase().as_str() {
        "cascade" => GrantedKeyword::Cascade,
        "changeling" => GrantedKeyword::Changeling,
        "deathtouch" => GrantedKeyword::Deathtouch,
        "defender" => GrantedKeyword::Defender,
        "exalted" => GrantedKeyword::Exalted,
        "fear" => GrantedKeyword::Fear,
        "flash" => GrantedKeyword::Flash,
        "flying" => GrantedKeyword::Flying,
        "haste" => GrantedKeyword::Haste,
        "hexproof" => GrantedKeyword::Hexproof,
        "horsemanship" => GrantedKeyword::Horsemanship,
        "indestructible" => GrantedKeyword::Indestructible,
        "infect" => GrantedKeyword::Infect,
        "intimidate" => GrantedKeyword::Intimidate,
        "lifelink" => GrantedKeyword::Lifelink,
        "menace" => GrantedKeyword::Menace,
        "persist" => GrantedKeyword::Persist,
        "prowess" => GrantedKeyword::Prowess,
        "reach" => GrantedKeyword::Reach,
        "shadow" => GrantedKeyword::Shadow,
        "shroud" => GrantedKeyword::Shroud,
        "skulk" => GrantedKeyword::Skulk,
        "sunburst" => GrantedKeyword::Sunburst,
        "trample" => GrantedKeyword::Trample,
        "undying" => GrantedKeyword::Undying,
        "vigilance" => GrantedKeyword::Vigilance,
        // CR 702.14a: `[type]walk` with a land subtype from the shared vocabulary.
        other => GrantedKeyword::Landwalk(lookup_landwalk_word(other)?),
    })
}

pub(in crate::oracle::canonical) fn recognize_parameterless_keyword(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<GrantedKeyword> {
    cursor.enter("keyword-list.item");
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a supported parameterless keyword"))?;
    let normalized = raw.to_ascii_lowercase();
    let keyword = if normalized == "double" {
        cursor.consume_phrase(&["double", "strike"])?;
        GrantedKeyword::DoubleStrike
    } else if normalized == "first" {
        cursor.consume_phrase(&["first", "strike"])?;
        GrantedKeyword::FirstStrike
    } else {
        let keyword = parameterless_keyword_from_oracle_word(raw)
            .ok_or_else(|| cursor.fail("a supported parameterless keyword"))?;
        cursor.advance_to(cursor.position() + 1)?;
        keyword
    };
    Ok(keyword)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeywordResourceCommitment {
    None,
    Failed,
}

pub(in crate::oracle::canonical) enum KeywordListRecognition {
    Parsed(KeywordListNode),
    FailedParameterizedResource(MachineFailure),
    Failed(MachineFailure),
}

fn recognize_ward_resource_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ResourcePaymentNode> {
    cursor.enter("keyword.ward.keyword");
    cursor.consume("ward")?;
    cursor.enter("keyword.ward.cost");
    let cost = if cursor.peek() == Some("—") {
        cursor.consume("—")?;
        cursor.require(
            cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("pay")),
            "a fixed life payment after the Ward separator",
        )?;
        super::recognize_resource_payment(cursor)?
    } else {
        cursor.require(
            cursor
                .peek_token()
                .is_some_and(|token| token.token_type == "manaSymbol"),
            "a mana cost for Ward",
        )?;
        super::recognize_resource_payment(cursor)?
    };
    cursor.enter("keyword.ward.cost-supported");
    match &cost {
        ResourcePaymentNode::Mana(mana) => {
            if let Err(error) = mana.fixed_runtime_mana_value() {
                let expected = match error {
                    super::FixedRuntimeManaCostError::Variable => {
                        "a fixed Ward mana cost without an unbound casting variable"
                    }
                    super::FixedRuntimeManaCostError::UnsupportedSymbol => {
                        "supported whole-mana symbols in a Ward cost"
                    }
                    super::FixedRuntimeManaCostError::AmountOutOfRange => {
                        "a complete Ward mana value fitting a nonnegative i32"
                    }
                };
                return Err(cursor.fail(expected));
            }
        }
        ResourcePaymentNode::Life(life) => cursor.require(
            (0..=i64::from(i32::MAX)).contains(&life.amount),
            "a fixed nonnegative life amount fitting i32 for Ward",
        )?,
    }
    Ok(cost)
}

fn recognize_keyword_atom(
    cursor: &mut OracleTokenCursor<'_>,
    allow_parameterized: bool,
    quoted_type_line: Option<&str>,
    resource_commitment: &mut KeywordResourceCommitment,
) -> MachineResult<KeywordAtomNode> {
    if let Some(type_line) = quoted_type_line
        && cursor.peek() == Some("\"")
    {
        return recognize_quoted_ability(cursor, type_line).map(KeywordAtomNode::Quoted);
    }
    if allow_parameterized
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("ward"))
    {
        let cost = match recognize_ward_resource_cost(cursor) {
            Ok(cost) => cost,
            Err(failure) => {
                *resource_commitment = KeywordResourceCommitment::Failed;
                return Err(failure);
            }
        };
        let atom = KeywordAtomNode::Ward { cost };
        recognize_optional_keyword_reminder(cursor, &atom)?;
        return Ok(atom);
    }
    let atom = KeywordAtomNode::Parameterless(recognize_parameterless_keyword(cursor)?);
    recognize_optional_keyword_reminder(cursor, &atom)?;
    Ok(atom)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeywordListState {
    Item,
    SeparatorOrComplete,
    Complete,
}

fn recognize_keyword_atom_list(
    cursor: &mut OracleTokenCursor<'_>,
    allow_parameterized: bool,
) -> MachineResult<KeywordListNode> {
    recognize_keyword_atom_list_with_commitment(
        cursor,
        allow_parameterized,
        &mut KeywordResourceCommitment::None,
    )
}

fn recognize_keyword_atom_list_with_commitment(
    cursor: &mut OracleTokenCursor<'_>,
    allow_parameterized: bool,
    resource_commitment: &mut KeywordResourceCommitment,
) -> MachineResult<KeywordListNode> {
    recognize_keyword_atom_list_in_context(
        cursor,
        allow_parameterized,
        None,
        resource_commitment,
        false,
    )
}

fn recognize_keyword_atom_list_in_context(
    cursor: &mut OracleTokenCursor<'_>,
    allow_parameterized: bool,
    quoted_type_line: Option<&str>,
    resource_commitment: &mut KeywordResourceCommitment,
    effect_owns_connector: bool,
) -> MachineResult<KeywordListNode> {
    let mut state = KeywordListState::Item;
    let mut keywords = Vec::new();
    loop {
        state = match state {
            KeywordListState::Item => {
                keywords.push(recognize_keyword_atom(
                    cursor,
                    allow_parameterized,
                    quoted_type_line,
                    resource_commitment,
                )?);
                KeywordListState::SeparatorOrComplete
            }
            KeywordListState::SeparatorOrComplete => {
                cursor.enter("keyword-list.separator-or-complete");
                if effect_owns_connector
                    && (cursor.peek() == Some(",")
                        || cursor
                            .peek()
                            .is_some_and(|raw| raw.eq_ignore_ascii_case("and")))
                {
                    let mut trial = cursor.clone();
                    if trial.peek() == Some(",") {
                        trial.consume(",")?;
                    }
                    if trial
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                    {
                        trial.consume("and")?;
                    }
                    if recognize_keyword_atom(
                        &mut trial,
                        allow_parameterized,
                        quoted_type_line,
                        &mut KeywordResourceCommitment::None,
                    )
                    .is_err()
                    {
                        state = KeywordListState::Complete;
                        continue;
                    }
                }
                if cursor.peek().is_some_and(|raw| raw == ",") {
                    cursor.consume(",")?;
                    if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                    {
                        cursor.consume("and")?;
                    }
                    KeywordListState::Item
                } else if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                {
                    cursor.consume("and")?;
                    KeywordListState::Item
                } else {
                    KeywordListState::Complete
                }
            }
            KeywordListState::Complete => {
                cursor.enter("keyword-list.complete");
                cursor.require(!keywords.is_empty(), "one or more keyword atoms")?;
                return Ok(KeywordListNode { keywords });
            }
        };
    }
}

pub(in crate::oracle::canonical) fn recognize_keyword_list(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Vec<GrantedKeyword>> {
    let list = recognize_keyword_atom_list(cursor, false)?;
    list.keywords
        .into_iter()
        .map(|keyword| match keyword {
            KeywordAtomNode::Parameterless(keyword) => Ok(keyword),
            KeywordAtomNode::Ward { .. } | KeywordAtomNode::Quoted(_) => {
                Err(cursor.fail("a parameterless keyword in a granted-keyword list"))
            }
        })
        .collect()
}

pub(in crate::oracle::canonical) fn recognize_effect_owned_keyword_list(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Vec<GrantedKeyword>> {
    recognize_keyword_atom_list_in_context(
        cursor,
        false,
        None,
        &mut KeywordResourceCommitment::None,
        true,
    )?
    .keywords
    .into_iter()
    .map(|atom| match atom {
        KeywordAtomNode::Parameterless(keyword) => Ok(keyword),
        KeywordAtomNode::Ward { .. } | KeywordAtomNode::Quoted(_) => {
            Err(cursor.fail("a parameterless token keyword"))
        }
    })
    .collect()
}

pub(in crate::oracle::canonical) fn recognize_standalone_keyword_list(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<KeywordListNode> {
    recognize_keyword_atom_list(cursor, true)
}

/// Parameterless keywords and complete quoted abilities granted to another
/// object (CR 113.1). Parameterized keywords keep their own grant frames.
pub(in crate::oracle::canonical) fn recognize_granted_ability_list(
    cursor: &mut OracleTokenCursor<'_>,
    granted_type_line: &str,
) -> MachineResult<GrantedAbilityListNode> {
    let list = recognize_keyword_atom_list_in_context(
        cursor,
        false,
        Some(granted_type_line),
        &mut KeywordResourceCommitment::None,
        false,
    )?;
    split_granted_atoms(cursor, list)
}

fn split_granted_atoms(
    cursor: &OracleTokenCursor<'_>,
    list: KeywordListNode,
) -> MachineResult<GrantedAbilityListNode> {
    let mut granted = GrantedAbilityListNode {
        keywords: Vec::new(),
        abilities: Vec::new(),
    };
    for atom in list.keywords {
        match atom {
            KeywordAtomNode::Parameterless(keyword) => granted.keywords.push(keyword),
            KeywordAtomNode::Quoted(ability) => granted.abilities.push(ability),
            KeywordAtomNode::Ward { .. } => {
                return Err(cursor.fail("a parameterless keyword or quoted ability"));
            }
        }
    }
    Ok(granted)
}

/// The effect-owned form of [`recognize_granted_ability_list`]: a trailing
/// `and`/`,` that does not introduce another atom belongs to the parent effect.
pub(in crate::oracle::canonical) fn recognize_effect_owned_granted_ability_list(
    cursor: &mut OracleTokenCursor<'_>,
    granted_type_line: &str,
) -> MachineResult<GrantedAbilityListNode> {
    let list = recognize_keyword_atom_list_in_context(
        cursor,
        false,
        Some(granted_type_line),
        &mut KeywordResourceCommitment::None,
        true,
    )?;
    split_granted_atoms(cursor, list)
}

/// Standalone keyword list of a static predicate, optionally with quoted
/// abilities of the objects it affects.
pub(in crate::oracle::canonical) fn recognize_standalone_granted_list(
    cursor: &mut OracleTokenCursor<'_>,
    granted_type_line: &str,
) -> MachineResult<KeywordListNode> {
    recognize_keyword_atom_list_in_context(
        cursor,
        true,
        Some(granted_type_line),
        &mut KeywordResourceCommitment::None,
        false,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TemporaryKeywordGrantState {
    Objects,
    OptionalAlso,
    Action,
    Keywords,
    Duration,
    Terminator,
    Complete,
}

/// Recognizes one temporary keyword grant clause without requiring end of
/// input. Modal and sequence parents can therefore retain ownership of the
/// following bullet or effect boundary while reusing the same selector,
/// keyword-list, and duration children.
pub(in crate::oracle::canonical) fn recognize_temporary_keyword_grant_clause(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TemporaryKeywordGrantNode> {
    recognize_temporary_keyword_grant_subject(
        cursor,
        |cursor| {
            Ok(
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("target"))
                {
                    TemporaryKeywordGrantObjectsNode::Targets(recognize_target_list(cursor)?)
                } else {
                    TemporaryKeywordGrantObjectsNode::ControlledSet(
                        recognize_controlled_object_set(cursor)?,
                    )
                },
            )
        },
        true,
        None,
    )
}

/// Subject and action payload remain separate child contracts.
pub(in crate::oracle::canonical) fn recognize_temporary_keyword_grant_subject<'tokens, Subject>(
    cursor: &mut OracleTokenCursor<'tokens>,
    mut subject: impl FnMut(&mut OracleTokenCursor<'tokens>) -> MachineResult<Subject>,
    own_period: bool,
    preposed_duration: Option<&EffectDurationNode>,
) -> MachineResult<TemporaryKeywordGrantNode<Subject>> {
    let mut state = TemporaryKeywordGrantState::Objects;
    let mut objects = None;
    let mut keywords = None;
    let mut abilities = Vec::new();
    let mut duration = None;
    loop {
        state = match state {
            TemporaryKeywordGrantState::Objects => {
                cursor.enter("effect.keyword-grant.objects");
                objects = Some(subject(cursor)?);
                TemporaryKeywordGrantState::OptionalAlso
            }
            TemporaryKeywordGrantState::OptionalAlso => {
                cursor.enter("effect.keyword-grant.optional-also");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("also"))
                {
                    cursor.consume("also")?;
                }
                TemporaryKeywordGrantState::Action
            }
            TemporaryKeywordGrantState::Action => {
                cursor.enter("effect.keyword-grant.action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    action.raw.eq_ignore_ascii_case("gain")
                        || action.raw.eq_ignore_ascii_case("gains"),
                    "`gain` or `gains`",
                )?;
                TemporaryKeywordGrantState::Keywords
            }
            TemporaryKeywordGrantState::Keywords => {
                cursor.enter("effect.keyword-grant.keywords");
                let list = recognize_granted_ability_list(cursor, GRANTED_OBJECT_TYPE_LINE)?;
                keywords = Some(list.keywords);
                abilities = list.abilities;
                TemporaryKeywordGrantState::Duration
            }
            TemporaryKeywordGrantState::Duration => {
                cursor.enter("effect.keyword-grant.duration");
                duration = Some(recognize_trailing_or_supplied_duration(
                    cursor,
                    preposed_duration,
                )?);
                TemporaryKeywordGrantState::Terminator
            }
            TemporaryKeywordGrantState::Terminator => {
                cursor.enter("effect.keyword-grant.terminator");
                if own_period && !quoted_sentence_just_closed(cursor) {
                    cursor.consume(".")?;
                }
                TemporaryKeywordGrantState::Complete
            }
            TemporaryKeywordGrantState::Complete => {
                cursor.enter("effect.keyword-grant.complete");
                return Ok(TemporaryKeywordGrantNode {
                    objects: cursor.require_value(objects, "keyword-grant recipients")?,
                    keywords: cursor.require_value(keywords, "one or more keyword atoms")?,
                    abilities,
                    duration: cursor.require_value(duration, "an effect duration")?,
                });
            }
        };
    }
}

/// Shared reminder attachment for parents that own a bound object collection.
/// The parent owns its terminator; the reminder child owns both parentheses.
pub(in crate::oracle::canonical) fn recognize_keyword_reminder_annotation(
    cursor: &mut OracleTokenCursor<'_>,
    keywords: &[GrantedKeyword],
    objects: &super::ObjectPredicate,
) -> MachineResult<()> {
    reminder::recognize_collection_keyword_annotation(cursor, keywords, objects)
}

/// Parameterized atoms retain their own cost and bound-annotation context.
pub(in crate::oracle::canonical) fn recognize_keyword_atom_reminder_annotation(
    cursor: &mut OracleTokenCursor<'_>,
    keywords: &[KeywordAtomNode],
    objects: &super::ObjectPredicate,
) -> MachineResult<()> {
    reminder::recognize_collection_keyword_atom_annotation(cursor, keywords, objects)
}

#[cfg(test)]
#[path = "resource_payment_keyword_tests.rs"]
mod resource_payment_tests;

/// Resource commitment comes from the actual child outcome, never a failure
/// state's spelling or the number of tokens that child happened to consume.
pub(in crate::oracle::canonical) fn try_recognize_standalone_keyword_list(
    cursor: &mut OracleTokenCursor<'_>,
) -> KeywordListRecognition {
    let mut commitment = KeywordResourceCommitment::None;
    match recognize_keyword_atom_list_with_commitment(cursor, true, &mut commitment) {
        Ok(list) => KeywordListRecognition::Parsed(list),
        Err(failure) => match commitment {
            KeywordResourceCommitment::Failed => {
                KeywordListRecognition::FailedParameterizedResource(failure)
            }
            KeywordResourceCommitment::None => KeywordListRecognition::Failed(failure),
        },
    }
}

#[cfg(test)]
#[path = "keyword_grant/quoted_ability_tests.rs"]
mod quoted_ability_tests;
