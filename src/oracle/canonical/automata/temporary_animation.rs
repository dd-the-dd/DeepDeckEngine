use crate::oracle::{
    characteristic_vocabulary::{
        AcceptedForms, lookup_card_color, lookup_creature_subtype, lookup_permanent_card_type,
    },
    syntax::{NameResolutionFailure, ResolvedNameLink},
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CardColorSetNode, CreatureSubtype, EffectDurationNode, GrantedKeyword, LocalObjectReference,
    ObjectCardType, PowerToughnessNode, recognize_card_color_set, recognize_effect_duration,
    recognize_literal_power_toughness, recognize_parameterless_keyword, recognize_source_reference,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TemporaryAnimationEffectNode {
    pub source: LocalObjectReference,
    pub duration: Option<EffectDurationNode>,
    pub power_toughness: PowerToughnessNode,
    pub colors: CardColorSetNode,
    pub subtypes: Vec<CreatureSubtype>,
    pub card_types: Vec<ObjectCardType>,
    pub keywords: Vec<GrantedKeyword>,
    pub all_creature_types: bool,
    pub retained_card_types: Vec<ObjectCardType>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TemporaryAnimationState {
    Duration,
    DurationSeparator,
    Source,
    Action,
    Article,
    Statistics,
    Colors,
    Subtype,
    CardTypes,
    With,
    Characteristic,
    CharacteristicSeparatorOrComplete,
    RetentionBoundary,
    RetentionSource,
    Still,
    RetainedArticle,
    RetainedType,
    Complete,
}

/// Recognizes temporary source animation as independent typed characteristics.
/// The internal retention sentence belongs to this effect; EffectSequence
/// retains ownership of the final sentence terminator.
pub(in crate::oracle::canonical) fn recognize_temporary_animation_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<TemporaryAnimationEffectNode> {
    let mut state = TemporaryAnimationState::Duration;
    let mut source = None;
    let mut duration = None;
    let mut power_toughness = None;
    let mut colors = None;
    let mut subtypes = Vec::new();
    let mut card_types = Vec::new();
    let mut keywords = Vec::new();
    let mut all_creature_types = false;
    let mut retained_card_types = Vec::new();
    loop {
        state = match state {
            TemporaryAnimationState::Duration => {
                cursor.enter("effect.source-animation.duration-or-source");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("until"))
                {
                    duration = Some(recognize_effect_duration(cursor)?);
                    TemporaryAnimationState::DurationSeparator
                } else {
                    TemporaryAnimationState::Source
                }
            }
            TemporaryAnimationState::DurationSeparator => {
                cursor.enter("effect.temporary-animation.separator");
                cursor.consume(",")?;
                TemporaryAnimationState::Source
            }
            TemporaryAnimationState::Source => {
                cursor.enter("effect.temporary-animation.source");
                source = Some(recognize_source_reference(
                    cursor,
                    name_links,
                    name_failure,
                )?);
                TemporaryAnimationState::Action
            }
            TemporaryAnimationState::Action => {
                cursor.enter("effect.temporary-animation.action");
                cursor.consume("becomes")?;
                TemporaryAnimationState::Article
            }
            TemporaryAnimationState::Article => {
                cursor.enter("effect.temporary-animation.article");
                cursor.consume("a")?;
                TemporaryAnimationState::Statistics
            }
            TemporaryAnimationState::Statistics => {
                power_toughness = Some(recognize_literal_power_toughness(cursor)?);
                TemporaryAnimationState::Colors
            }
            TemporaryAnimationState::Colors => {
                cursor.enter("effect.source-animation.colors-or-subtypes");
                colors = Some(if cursor.peek().and_then(lookup_card_color).is_some() {
                    recognize_card_color_set(cursor)?
                } else {
                    CardColorSetNode { colors: Vec::new() }
                });
                TemporaryAnimationState::Subtype
            }
            TemporaryAnimationState::Subtype => {
                cursor.enter("effect.temporary-animation.subtype");
                while let Some(subtype) = cursor
                    .peek()
                    .and_then(|raw| lookup_creature_subtype(raw, AcceptedForms::SingularOnly))
                {
                    cursor.require(!subtypes.contains(&subtype), "distinct creature subtypes")?;
                    subtypes.push(subtype);
                    cursor.advance_to(cursor.position() + 1)?;
                }
                cursor.require(
                    cursor
                        .peek()
                        .and_then(|raw| {
                            lookup_permanent_card_type(raw, AcceptedForms::SingularOnly)
                        })
                        .is_some(),
                    "a registered creature subtype or the creature card type",
                )?;
                TemporaryAnimationState::CardTypes
            }
            TemporaryAnimationState::CardTypes => {
                cursor.enter("effect.temporary-animation.card-types");
                while let Some(card_type) = cursor
                    .peek()
                    .and_then(|raw| lookup_permanent_card_type(raw, AcceptedForms::SingularOnly))
                {
                    cursor.advance_to(cursor.position() + 1)?;
                    if !card_types.contains(&card_type) {
                        card_types.push(card_type);
                    }
                }
                cursor.require(
                    card_types.contains(&ObjectCardType::Creature),
                    "the creature card type for an animation effect",
                )?;
                TemporaryAnimationState::With
            }
            TemporaryAnimationState::With => {
                cursor.enter("effect.temporary-animation.with");
                cursor.consume("with")?;
                TemporaryAnimationState::Characteristic
            }
            TemporaryAnimationState::Characteristic => {
                cursor.enter("effect.source-animation.characteristic");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("all"))
                {
                    cursor.enter("effect.source-animation.characteristic.all-creature-types");
                    cursor.require(
                        !all_creature_types,
                        "one `all creature types` characteristic",
                    )?;
                    cursor.consume_phrase(&["all", "creature", "types"])?;
                    all_creature_types = true;
                } else {
                    let keyword = recognize_parameterless_keyword(cursor)?;
                    cursor.require(!keywords.contains(&keyword), "distinct granted keywords")?;
                    keywords.push(keyword);
                }
                TemporaryAnimationState::CharacteristicSeparatorOrComplete
            }
            TemporaryAnimationState::CharacteristicSeparatorOrComplete => {
                cursor.enter("effect.source-animation.characteristic-separator-or-complete");
                if cursor.peek().is_some_and(|raw| raw == ",") {
                    cursor.consume(",")?;
                    if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                    {
                        cursor.consume("and")?;
                    }
                    TemporaryAnimationState::Characteristic
                } else if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                {
                    cursor.consume("and")?;
                    TemporaryAnimationState::Characteristic
                } else {
                    cursor.require(
                        !keywords.is_empty() || all_creature_types,
                        "one or more animation characteristics",
                    )?;
                    TemporaryAnimationState::RetentionBoundary
                }
            }
            TemporaryAnimationState::RetentionBoundary => {
                cursor.enter("effect.temporary-animation.retention-boundary");
                cursor.consume(".")?;
                TemporaryAnimationState::RetentionSource
            }
            TemporaryAnimationState::RetentionSource => {
                cursor.enter("effect.temporary-animation.retention-source");
                cursor.consume("it's")?;
                TemporaryAnimationState::Still
            }
            TemporaryAnimationState::Still => {
                cursor.enter("effect.temporary-animation.still");
                cursor.consume("still")?;
                TemporaryAnimationState::RetainedArticle
            }
            TemporaryAnimationState::RetainedArticle => {
                cursor.enter("effect.temporary-animation.retained-article");
                let article = cursor
                    .peek()
                    .map(str::to_ascii_lowercase)
                    .ok_or_else(|| cursor.fail("`a` or `an` before the retained type"))?;
                cursor.require(matches!(article.as_str(), "a" | "an"), "`a` or `an`")?;
                cursor.consume(&article)?;
                TemporaryAnimationState::RetainedType
            }
            TemporaryAnimationState::RetainedType => {
                cursor.enter("effect.temporary-animation.retained-type");
                let raw = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("a retained permanent card type"))?;
                let retained = lookup_permanent_card_type(raw, AcceptedForms::SingularOnly)
                    .ok_or_else(|| cursor.fail("a retained permanent card type"))?;
                cursor.require(
                    !card_types.contains(&retained),
                    "a retained type distinct from the newly assigned card types",
                )?;
                retained_card_types.push(retained);
                cursor.advance_to(cursor.position() + 1)?;
                TemporaryAnimationState::Complete
            }
            TemporaryAnimationState::Complete => {
                cursor.enter("effect.temporary-animation.complete");
                return Ok(TemporaryAnimationEffectNode {
                    source: cursor.require_value(source, "an animated source")?,
                    duration,
                    power_toughness: cursor
                        .require_value(power_toughness, "animation power/toughness")?,
                    colors: cursor.require_value(colors, "animation colors")?,
                    subtypes,
                    card_types,
                    keywords,
                    all_creature_types,
                    retained_card_types,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "temporary_animation_tests.rs"]
mod tests;
