//! `OBJECT becomes a [P/T] [COLORS] [SUBTYPES] [CARD TYPES] [with ...]
//! [that's still a TYPE] [in addition to its other types] [DURATION]
//! [. It's still a TYPE]` (CR 205.1a/b, 105.3, 613.1d-f, 613.4b).
//!
//! The same frame owns the sibling characteristic verbs `loses all abilities`
//! (CR 613.1f) and `has base power and toughness N/N` (CR 613.4b), so that
//! coordinated verbs share one subject and one duration.
//!
//! One `becomes` frame over the shared permanent operand. The subject, base
//! power/toughness, color set, type list, granted characteristics, retention
//! clause and duration are independent children; the parent owns only their
//! Oracle order. Retention is carried as data (CR 205.1b), never inferred
//! again by a lowering parent. The final `It's still a TYPE` sentence belongs
//! to this effect; the enclosing sequence keeps its own final terminator.
use crate::oracle::canonical::automata::VerbLemma;
use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, lookup_card_color, lookup_creature_subtype_tokens, lookup_permanent_card_type,
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CardColorSetNode, CreatureSubtype, EffectDurationNode, GRANTED_OBJECT_TYPE_LINE,
    GrantedKeyword, ObjectCardType, PowerToughnessNode, QuotedAbilityNode,
    RecognizedEffectFragment, TargetListNode, recognize_card_color_set,
    recognize_effect_owned_granted_ability_list, recognize_literal_power_toughness,
    recognize_trailing_or_supplied_duration,
};

/// Whether the new types supplement or replace the object's existing ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TypeChangeRetention {
    /// No retention clause: new card types and subtypes replace those of
    /// their class (CR 205.1a).
    ReplacesSameClass,
    /// `in addition to its other types` (CR 205.1b).
    InAdditionToOtherTypes,
    /// `that's still a TYPE` / `It's still a TYPE` (CR 205.1b): every prior
    /// card type, supertype and subtype is retained.
    StillA(ObjectCardType),
}

impl TypeChangeRetention {
    pub(in crate::oracle::canonical) fn retains_prior_types(self) -> bool {
        !matches!(self, Self::ReplacesSameClass)
    }
}

/// Characteristics stated by `with ...` after the new types (CR 613.1f,
/// 613.4b). Keywords and quoted abilities reuse the shared granted list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct BecomeCharacteristicsNode {
    pub keywords: Vec<GrantedKeyword>,
    pub abilities: Vec<QuotedAbilityNode>,
    /// `with all creature types` (CR 205.3m) is a subtype characteristic.
    pub all_creature_types: bool,
    /// `with base power and toughness N/N` / `has base power and toughness N/N`.
    pub base_power_toughness: Option<PowerToughnessNode>,
    /// `loses all abilities` (CR 613.1f).
    pub loses_all_abilities: bool,
}

/// Whether the cursor is at one of the verbs this frame owns.
pub(in crate::oracle::canonical) fn characteristic_change_verb_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    word(0, "becomes")
        || (word(0, "loses") && word(1, "all") && word(2, "abilities"))
        || (word(0, "has") && word(1, "base"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct BecomeTypesNode<Subject = TargetListNode> {
    pub objects: Subject,
    /// `a N/N ...`: base power and toughness set in layer 7b.
    pub power_toughness: Option<PowerToughnessNode>,
    /// `a blue and red ...` / `a colorless ...`: the new color set (CR 105.3).
    pub colors: Option<CardColorSetNode>,
    pub subtypes: Vec<CreatureSubtype>,
    pub card_types: Vec<ObjectCardType>,
    pub characteristics: BecomeCharacteristicsNode,
    pub retention: TypeChangeRetention,
    /// `None` is a continuous effect without a stated duration (CR 611.2a).
    pub duration: Option<EffectDurationNode>,
}

impl<Subject> BecomeTypesNode<Subject> {
    /// The base power/toughness from either Oracle position.
    pub(in crate::oracle::canonical) fn base_power_toughness(&self) -> Option<PowerToughnessNode> {
        self.power_toughness
            .or(self.characteristics.base_power_toughness)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BecomeTypesState {
    Subject,
    Verb,
    Article,
    Statistics,
    Colors,
    Types,
    With,
    RelativeRetention,
    Retention,
    Duration,
    RetentionSentence,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_become_types_subject<'tokens, Subject>(
    cursor: &mut OracleTokenCursor<'tokens>,
    mut subject: impl FnMut(&mut OracleTokenCursor<'tokens>) -> MachineResult<Subject>,
    preposed_duration: Option<&EffectDurationNode>,
) -> MachineResult<RecognizedEffectFragment<BecomeTypesNode<Subject>>> {
    let mut state = BecomeTypesState::Subject;
    let mut objects = None;
    let mut power_toughness = None;
    let mut colors = None;
    let mut subtypes = Vec::new();
    let mut card_types = Vec::new();
    let mut characteristics = BecomeCharacteristicsNode::default();
    let mut retention = TypeChangeRetention::ReplacesSameClass;
    let mut duration = None;
    let mut becomes = false;
    loop {
        state = match state {
            BecomeTypesState::Subject => {
                cursor.enter("effect.become-types.subject");
                objects = Some(subject(cursor)?);
                BecomeTypesState::Verb
            }
            BecomeTypesState::Verb => {
                cursor.enter("effect.become-types.verb");
                if crate::oracle::canonical::automata::lemma_at(
                    &cursor,
                    VerbLemma::Lose,
                    crate::oracle::canonical::automata::VerbAgreement::ThirdPersonSingular,
                ) {
                    cursor.consume_phrase(&["loses", "all", "abilities"])?;
                    characteristics.loses_all_abilities = true;
                    BecomeTypesState::Duration
                } else if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("has"))
                {
                    cursor.consume_phrase(&["has", "base", "power", "and", "toughness"])?;
                    characteristics.base_power_toughness =
                        Some(recognize_literal_power_toughness(cursor)?);
                    BecomeTypesState::Duration
                } else {
                    cursor.consume("becomes")?;
                    becomes = true;
                    BecomeTypesState::Article
                }
            }
            BecomeTypesState::Article => {
                cursor.enter("effect.become-types.article");
                let article = cursor
                    .peek()
                    .map(str::to_ascii_lowercase)
                    .ok_or_else(|| cursor.fail("`a` or `an` before the new types"))?;
                cursor.require(matches!(article.as_str(), "a" | "an"), "`a` or `an`")?;
                cursor.consume(&article)?;
                BecomeTypesState::Statistics
            }
            BecomeTypesState::Statistics => {
                cursor.enter("effect.become-types.power-toughness");
                if cursor
                    .peek_token()
                    .is_some_and(|token| token.token_type == "counterValue")
                {
                    power_toughness = Some(recognize_literal_power_toughness(cursor)?);
                }
                BecomeTypesState::Colors
            }
            BecomeTypesState::Colors => {
                cursor.enter("effect.become-types.colors");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("colorless"))
                {
                    cursor.consume("colorless")?;
                    colors = Some(CardColorSetNode { colors: Vec::new() });
                } else if cursor.peek().and_then(lookup_card_color).is_some() {
                    colors = Some(recognize_card_color_set(cursor)?);
                }
                BecomeTypesState::Types
            }
            BecomeTypesState::Types => {
                cursor.enter("effect.become-types.types");
                loop {
                    let Some(first) = cursor.peek() else { break };
                    if card_types.is_empty()
                        && let Some((subtype, width)) = lookup_creature_subtype_tokens(
                            first,
                            cursor.peek_ahead(1),
                            AcceptedForms::SingularOnly,
                        )
                    {
                        cursor.require(
                            cursor
                                .token_slice(cursor.position(), cursor.position() + width)
                                .iter()
                                .all(|token| token.token_type == "subtypeWord"),
                            "lexical subtypeWord evidence from CR 205.3m",
                        )?;
                        cursor
                            .require(!subtypes.contains(&subtype), "distinct creature subtypes")?;
                        subtypes.push(subtype);
                        cursor.advance_to(cursor.position() + width)?;
                    } else if let Some(card_type) =
                        lookup_permanent_card_type(first, AcceptedForms::SingularOnly)
                    {
                        cursor.require(!card_types.contains(&card_type), "distinct card types")?;
                        card_types.push(card_type);
                        cursor.advance_to(cursor.position() + 1)?;
                    } else {
                        break;
                    }
                }
                cursor.require(
                    !subtypes.is_empty() || !card_types.is_empty(),
                    "a registered creature subtype or permanent card type",
                )?;
                // CR 208.1: only a creature has power and toughness.
                cursor.require(
                    power_toughness.is_none() || card_types.contains(&ObjectCardType::Creature),
                    "the creature card type with a stated power/toughness",
                )?;
                BecomeTypesState::With
            }
            BecomeTypesState::With => {
                cursor.enter("effect.become-types.with");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("with"))
                {
                    cursor.consume("with")?;
                    recognize_become_characteristics(cursor, &mut characteristics)?;
                    cursor.require(
                        power_toughness.is_none() || characteristics.base_power_toughness.is_none(),
                        "one stated base power/toughness",
                    )?;
                }
                BecomeTypesState::RelativeRetention
            }
            BecomeTypesState::RelativeRetention => {
                cursor.enter("effect.become-types.relative-retention");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("that's"))
                    && cursor
                        .peek_ahead(1)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("still"))
                {
                    cursor.consume("that's")?;
                    retention = recognize_still_type(cursor, &card_types)?;
                }
                BecomeTypesState::Retention
            }
            BecomeTypesState::Retention => {
                cursor.enter("effect.become-types.retention");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("in"))
                {
                    cursor.require(
                        !retention.retains_prior_types(),
                        "one type-retention clause",
                    )?;
                    cursor.consume_phrase(&["in", "addition", "to", "its", "other", "types"])?;
                    retention = TypeChangeRetention::InAdditionToOtherTypes;
                }
                BecomeTypesState::Duration
            }
            BecomeTypesState::Duration => {
                cursor.enter("effect.become-types.duration");
                duration = if preposed_duration.is_some()
                    || cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("until"))
                {
                    Some(recognize_trailing_or_supplied_duration(
                        cursor,
                        preposed_duration,
                    )?)
                } else {
                    // A trailing duration after a coordinated verb scopes the
                    // whole chain; it is not propagated backwards here, so an
                    // open-ended type change followed by one is refused.
                    cursor.require(
                        !(cursor
                            .peek()
                            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                            && cursor.peek_ahead(1).is_some_and(|raw| {
                                [
                                    "gain", "gains", "become", "becomes", "get", "gets", "lose",
                                    "loses", "has", "have",
                                ]
                                .iter()
                                .any(|verb| raw.eq_ignore_ascii_case(verb))
                            })),
                        "a duration for the type change before a coordinated verb",
                    )?;
                    None
                };
                BecomeTypesState::RetentionSentence
            }
            BecomeTypesState::RetentionSentence => {
                cursor.enter("effect.become-types.retention-sentence");
                if becomes
                    && cursor.peek() == Some(".")
                    && cursor
                        .peek_ahead(1)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("it's"))
                    && cursor
                        .peek_ahead(2)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("still"))
                {
                    cursor.require(
                        !retention.retains_prior_types(),
                        "one type-retention clause",
                    )?;
                    cursor.consume(".")?;
                    cursor.consume("it's")?;
                    retention = recognize_still_type(cursor, &card_types)?;
                }
                BecomeTypesState::Complete
            }
            BecomeTypesState::Complete => {
                cursor.enter("effect.become-types.complete");
                return Ok(RecognizedEffectFragment {
                    node: BecomeTypesNode {
                        objects: cursor.require_value(objects, "a type-change subject")?,
                        power_toughness,
                        colors,
                        subtypes,
                        card_types,
                        characteristics,
                        retention,
                        duration,
                    },
                    end_position: cursor.position(),
                });
            }
        };
    }
}

/// `still a TYPE`: the retained type is a prior card type, so it must differ
/// from the newly assigned ones (otherwise the clause carries no meaning).
fn recognize_still_type(
    cursor: &mut OracleTokenCursor<'_>,
    card_types: &[ObjectCardType],
) -> MachineResult<TypeChangeRetention> {
    cursor.enter("effect.become-types.still");
    cursor.consume("still")?;
    cursor.enter("effect.become-types.still.article");
    let article = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("`a` or `an` before the retained type"))?;
    cursor.require(matches!(article.as_str(), "a" | "an"), "`a` or `an`")?;
    cursor.consume(&article)?;
    cursor.enter("effect.become-types.still.type");
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a retained permanent card type"))?;
    let retained = lookup_permanent_card_type(raw, AcceptedForms::SingularOnly)
        .ok_or_else(|| cursor.fail("a retained permanent card type"))?;
    cursor.require(
        !card_types.contains(&retained),
        "a retained type distinct from the newly assigned card types",
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    Ok(TypeChangeRetention::StillA(retained))
}

/// One `with` item: `all creature types`, `base power and toughness N/N`,
/// or a maximal list of keywords and quoted abilities.
fn recognize_become_characteristic_item(
    cursor: &mut OracleTokenCursor<'_>,
    characteristics: &mut BecomeCharacteristicsNode,
) -> MachineResult<()> {
    cursor.enter("effect.become-types.characteristic");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("all"))
    {
        cursor.require(
            !characteristics.all_creature_types,
            "one `all creature types` characteristic",
        )?;
        cursor.consume_phrase(&["all", "creature", "types"])?;
        characteristics.all_creature_types = true;
    } else if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("base"))
    {
        cursor.require(
            characteristics.base_power_toughness.is_none(),
            "one base power/toughness characteristic",
        )?;
        cursor.consume_phrase(&["base", "power", "and", "toughness"])?;
        characteristics.base_power_toughness = Some(recognize_literal_power_toughness(cursor)?);
    } else {
        let list = recognize_effect_owned_granted_ability_list(cursor, GRANTED_OBJECT_TYPE_LINE)?;
        for keyword in list.keywords {
            cursor.require(
                !characteristics.keywords.contains(&keyword),
                "distinct granted keywords",
            )?;
            characteristics.keywords.push(keyword);
        }
        characteristics.abilities.extend(list.abilities);
    }
    Ok(())
}

/// Items are coordinated by `,`, `and` or `, and`. A connector that does not
/// introduce another item belongs to the enclosing effect sequence.
fn recognize_become_characteristics(
    cursor: &mut OracleTokenCursor<'_>,
    characteristics: &mut BecomeCharacteristicsNode,
) -> MachineResult<()> {
    recognize_become_characteristic_item(cursor, characteristics)?;
    loop {
        cursor.enter("effect.become-types.characteristic-separator");
        let mut trial = cursor.clone();
        let mut separated = false;
        if trial.peek() == Some(",") {
            trial.consume(",")?;
            separated = true;
        }
        if trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
        {
            trial.consume("and")?;
            separated = true;
        }
        if !separated {
            return Ok(());
        }
        let mut extended = characteristics.clone();
        if recognize_become_characteristic_item(&mut trial, &mut extended).is_err() {
            return Ok(());
        }
        *cursor = trial;
        *characteristics = extended;
    }
}

#[cfg(test)]
#[path = "become_types_tests.rs"]
mod tests;
