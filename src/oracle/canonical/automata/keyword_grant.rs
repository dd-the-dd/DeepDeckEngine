use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ControlledKeywordGrantNode, EffectDurationNode, GrantedKeyword, ManaCostNode,
    ObjectControllerScope, TargetListNode, recognize_controlled_object_set,
    recognize_effect_duration, recognize_mana_cost, recognize_target_list,
};

mod reminder;

use reminder::recognize_optional_keyword_reminder;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TemporaryKeywordGrantObjectsNode {
    Targets(TargetListNode),
    ControlledSet(super::ObjectSetNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TemporaryKeywordGrantNode {
    pub objects: TemporaryKeywordGrantObjectsNode,
    pub keywords: Vec<GrantedKeyword>,
    pub duration: EffectDurationNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum KeywordAtomNode {
    Parameterless(GrantedKeyword),
    Ward { cost: ManaCostNode },
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
        "deathtouch" => GrantedKeyword::Deathtouch,
        "defender" => GrantedKeyword::Defender,
        "exalted" => GrantedKeyword::Exalted,
        "fear" => GrantedKeyword::Fear,
        "flash" => GrantedKeyword::Flash,
        "flying" => GrantedKeyword::Flying,
        "forestwalk" => GrantedKeyword::Forestwalk,
        "haste" => GrantedKeyword::Haste,
        "hexproof" => GrantedKeyword::Hexproof,
        "horsemanship" => GrantedKeyword::Horsemanship,
        "indestructible" => GrantedKeyword::Indestructible,
        "infect" => GrantedKeyword::Infect,
        "intimidate" => GrantedKeyword::Intimidate,
        "islandwalk" => GrantedKeyword::Islandwalk,
        "lifelink" => GrantedKeyword::Lifelink,
        "menace" => GrantedKeyword::Menace,
        "mountainwalk" => GrantedKeyword::Mountainwalk,
        "persist" => GrantedKeyword::Persist,
        "plainswalk" => GrantedKeyword::Plainswalk,
        "prowess" => GrantedKeyword::Prowess,
        "reach" => GrantedKeyword::Reach,
        "shadow" => GrantedKeyword::Shadow,
        "shroud" => GrantedKeyword::Shroud,
        "skulk" => GrantedKeyword::Skulk,
        "swampwalk" => GrantedKeyword::Swampwalk,
        "trample" => GrantedKeyword::Trample,
        "undying" => GrantedKeyword::Undying,
        "vigilance" => GrantedKeyword::Vigilance,
        _ => return None,
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

fn recognize_keyword_atom(
    cursor: &mut OracleTokenCursor<'_>,
    allow_parameterized: bool,
) -> MachineResult<KeywordAtomNode> {
    if allow_parameterized
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("ward"))
    {
        cursor.enter("keyword.ward.keyword");
        cursor.consume("ward")?;
        cursor.enter("keyword.ward.cost");
        cursor.require(
            cursor
                .peek_token()
                .is_some_and(|token| token.token_type == "manaSymbol"),
            "a mana cost for Ward",
        )?;
        let atom = KeywordAtomNode::Ward {
            cost: recognize_mana_cost(cursor)?,
        };
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
    let mut state = KeywordListState::Item;
    let mut keywords = Vec::new();
    loop {
        state = match state {
            KeywordListState::Item => {
                keywords.push(recognize_keyword_atom(cursor, allow_parameterized)?);
                KeywordListState::SeparatorOrComplete
            }
            KeywordListState::SeparatorOrComplete => {
                cursor.enter("keyword-list.separator-or-complete");
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
            KeywordAtomNode::Ward { .. } => {
                Err(cursor.fail("a parameterless keyword in a granted-keyword list"))
            }
        })
        .collect()
}

pub(in crate::oracle::canonical) fn recognize_standalone_keyword_list(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<KeywordListNode> {
    recognize_keyword_atom_list(cursor, true)
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
    let mut state = TemporaryKeywordGrantState::Objects;
    let mut objects = None;
    let mut keywords = None;
    let mut duration = None;
    loop {
        state = match state {
            TemporaryKeywordGrantState::Objects => {
                cursor.enter("effect.keyword-grant.objects");
                objects = Some(
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
                );
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
                keywords = Some(recognize_keyword_list(cursor)?);
                TemporaryKeywordGrantState::Duration
            }
            TemporaryKeywordGrantState::Duration => {
                cursor.enter("effect.keyword-grant.duration");
                duration = Some(recognize_effect_duration(cursor)?);
                TemporaryKeywordGrantState::Terminator
            }
            TemporaryKeywordGrantState::Terminator => {
                cursor.enter("effect.keyword-grant.terminator");
                cursor.consume(".")?;
                TemporaryKeywordGrantState::Complete
            }
            TemporaryKeywordGrantState::Complete => {
                cursor.enter("effect.keyword-grant.complete");
                return Ok(TemporaryKeywordGrantNode {
                    objects: cursor.require_value(objects, "keyword-grant recipients")?,
                    keywords: cursor.require_value(keywords, "one or more keyword atoms")?,
                    duration: cursor.require_value(duration, "an effect duration")?,
                });
            }
        };
    }
}

/// Recognizes a temporary keyword grant to a controller-scoped permanent set.
/// The object query, list punctuation, keyword atoms, and duration are distinct
/// children so an `and` inside the keyword list cannot become effect sequence.
pub(in crate::oracle::canonical) fn recognize_controlled_keyword_grant(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ControlledKeywordGrantNode> {
    let grant = recognize_temporary_keyword_grant_clause(cursor)?;
    let TemporaryKeywordGrantObjectsNode::ControlledSet(objects) = grant.objects else {
        return Err(cursor.fail("a controller-scoped permanent set"));
    };
    cursor.require(
        objects.controller == ObjectControllerScope::AbilityController,
        "a permanent set controlled by the ability controller",
    )?;
    cursor.finish()?;
    Ok(ControlledKeywordGrantNode {
        objects,
        keywords: grant.keywords,
        duration: grant.duration,
    })
}
