use super::super::{MachineResult, OracleTokenCursor};
use super::{
    AddAnyColorManaEffectNode, CharacteristicChoiceBinding, ManaSpendClauseNode,
    ManaSpendConsequenceNode, ManaSpendConstraintNode, ManaSpendPredicateNode, ManaSpendUsageNode,
    ObjectCardType, ObjectPredicate, TapSelfCostNode, positive_object_selector_atom,
    recognize_add_any_color_mana_effect, recognize_tap_self_cost,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct RestrictedManaAbilityNode {
    pub cost: TapSelfCostNode,
    pub effect: AddAnyColorManaEffectNode,
    pub restriction: ManaSpendClauseNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpendRestrictionState {
    Action,
    ManaReference,
    Exclusivity,
    Infinitive,
    SpendAction,
    Article,
    ObjectPredicate,
    SpellNoun,
    ChoiceRelation,
    ChoiceArticle,
    ChoiceReference,
    ChoiceProperty,
    ConsequenceSeparator,
    ConsequenceConnector,
    ConsequenceSubject,
    ConsequenceModality,
    ConsequencePredicate,
    ConsequenceAction,
    Terminator,
    Complete,
}

/// Recognizes a cast-only spend restriction linked to a creature-type choice,
/// followed by the independent uncounterability consequence.
fn recognize_chosen_creature_type_spend_restriction(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ManaSpendClauseNode> {
    let mut state = SpendRestrictionState::Action;
    let mut predicate = None;
    loop {
        state = match state {
            SpendRestrictionState::Action => {
                cursor.enter("mana.spend-restriction.action");
                let action = cursor.consume_type("costActionWord")?;
                cursor.require(
                    action.raw.eq_ignore_ascii_case("spend"),
                    "the mana spend action",
                )?;
                SpendRestrictionState::ManaReference
            }
            SpendRestrictionState::ManaReference => {
                cursor.enter("mana.spend-restriction.mana-reference");
                cursor.consume("this")?;
                cursor.consume("mana")?;
                SpendRestrictionState::Exclusivity
            }
            SpendRestrictionState::Exclusivity => {
                cursor.enter("mana.spend-restriction.exclusivity");
                let qualifier = cursor.consume_type("qualifierWord")?;
                cursor.require(
                    qualifier.raw.eq_ignore_ascii_case("only"),
                    "the exclusive spend qualifier",
                )?;
                SpendRestrictionState::Infinitive
            }
            SpendRestrictionState::Infinitive => {
                cursor.enter("mana.spend-restriction.infinitive");
                cursor.consume("to")?;
                SpendRestrictionState::SpendAction
            }
            SpendRestrictionState::SpendAction => {
                cursor.enter("mana.spend-restriction.spend-action");
                cursor.consume("cast")?;
                SpendRestrictionState::Article
            }
            SpendRestrictionState::Article => {
                cursor.enter("mana.spend-restriction.article");
                cursor.consume("a")?;
                SpendRestrictionState::ObjectPredicate
            }
            SpendRestrictionState::ObjectPredicate => {
                cursor.enter("mana.spend-restriction.object-predicate");
                let raw = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("the creature card type"))?
                    .to_string();
                let parsed = positive_object_selector_atom(&raw);
                cursor.require(
                    parsed == Some(ObjectPredicate::CardType(ObjectCardType::Creature)),
                    "the creature card type",
                )?;
                cursor.consume(&raw)?;
                predicate = parsed;
                SpendRestrictionState::SpellNoun
            }
            SpendRestrictionState::SpellNoun => {
                cursor.enter("mana.spend-restriction.spell-noun");
                cursor.consume("spell")?;
                SpendRestrictionState::ChoiceRelation
            }
            SpendRestrictionState::ChoiceRelation => {
                cursor.enter("mana.spend-restriction.choice-relation");
                cursor.consume("of")?;
                SpendRestrictionState::ChoiceArticle
            }
            SpendRestrictionState::ChoiceArticle => {
                cursor.enter("mana.spend-restriction.choice-article");
                cursor.consume("the")?;
                SpendRestrictionState::ChoiceReference
            }
            SpendRestrictionState::ChoiceReference => {
                cursor.enter("mana.spend-restriction.choice-reference");
                let reference = cursor.consume_type("selectionReferenceWord")?;
                cursor.require(
                    reference.raw.eq_ignore_ascii_case("chosen"),
                    "the chosen-characteristic reference",
                )?;
                SpendRestrictionState::ChoiceProperty
            }
            SpendRestrictionState::ChoiceProperty => {
                cursor.enter("mana.spend-restriction.choice-property");
                cursor.consume("type")?;
                SpendRestrictionState::ConsequenceSeparator
            }
            SpendRestrictionState::ConsequenceSeparator => {
                cursor.enter("mana.spend-restriction.consequence-separator");
                cursor.consume(",")?;
                SpendRestrictionState::ConsequenceConnector
            }
            SpendRestrictionState::ConsequenceConnector => {
                cursor.enter("mana.spend-restriction.consequence-connector");
                cursor.consume("and")?;
                SpendRestrictionState::ConsequenceSubject
            }
            SpendRestrictionState::ConsequenceSubject => {
                cursor.enter("mana.spend-restriction.consequence-subject");
                cursor.consume("that")?;
                cursor.consume("spell")?;
                SpendRestrictionState::ConsequenceModality
            }
            SpendRestrictionState::ConsequenceModality => {
                cursor.enter("mana.spend-restriction.consequence-modality");
                cursor.consume("can't")?;
                SpendRestrictionState::ConsequencePredicate
            }
            SpendRestrictionState::ConsequencePredicate => {
                cursor.enter("mana.spend-restriction.consequence-predicate");
                cursor.consume("be")?;
                SpendRestrictionState::ConsequenceAction
            }
            SpendRestrictionState::ConsequenceAction => {
                cursor.enter("mana.spend-restriction.consequence-action");
                cursor.consume("countered")?;
                SpendRestrictionState::Terminator
            }
            SpendRestrictionState::Terminator => {
                cursor.enter("mana.spend-restriction.terminator");
                cursor.consume(".")?;
                SpendRestrictionState::Complete
            }
            SpendRestrictionState::Complete => {
                cursor.enter("mana.spend-restriction.complete");
                return Ok(ManaSpendClauseNode {
                    constraint: ManaSpendConstraintNode::Only {
                        usage: ManaSpendUsageNode::CastSpell {
                            spell: ManaSpendPredicateNode {
                                predicate: cursor
                                    .require_value(predicate, "a creature spell predicate")?,
                                binding: Some(CharacteristicChoiceBinding::CreatureType),
                            },
                        },
                        alternatives: vec![],
                    },
                    consequences: vec![ManaSpendConsequenceNode::SpellCantBeCountered],
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestrictedManaAbilityState {
    Cost,
    Separator,
    Mana,
    ManaTerminator,
    Restriction,
    Complete,
}

/// Activated-ability envelope for an unrestricted color choice whose produced
/// mana carries a typed cast restriction. Production and spend semantics are
/// kept as separate children and lowered to the existing `addMana` primitive.
pub(in crate::oracle::canonical) fn recognize_restricted_mana_ability(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<RestrictedManaAbilityNode> {
    let mut state = RestrictedManaAbilityState::Cost;
    let mut cost = None;
    let mut effect = None;
    let mut restriction = None;
    loop {
        state = match state {
            RestrictedManaAbilityState::Cost => {
                cursor.enter("activation.restricted-mana.cost");
                cost = Some(recognize_tap_self_cost(cursor)?);
                RestrictedManaAbilityState::Separator
            }
            RestrictedManaAbilityState::Separator => {
                cursor.enter("activation.restricted-mana.separator");
                cursor.consume(":")?;
                RestrictedManaAbilityState::Mana
            }
            RestrictedManaAbilityState::Mana => {
                cursor.enter("activation.restricted-mana.production");
                effect = Some(recognize_add_any_color_mana_effect(cursor)?);
                RestrictedManaAbilityState::ManaTerminator
            }
            RestrictedManaAbilityState::ManaTerminator => {
                cursor.enter("activation.restricted-mana.production-terminator");
                cursor.consume(".")?;
                RestrictedManaAbilityState::Restriction
            }
            RestrictedManaAbilityState::Restriction => {
                cursor.enter("activation.restricted-mana.restriction");
                restriction = Some(recognize_chosen_creature_type_spend_restriction(cursor)?);
                RestrictedManaAbilityState::Complete
            }
            RestrictedManaAbilityState::Complete => {
                cursor.enter("activation.restricted-mana.complete");
                cursor.finish()?;
                return Ok(RestrictedManaAbilityNode {
                    cost: cursor.require_value(cost, "a tap-self activation cost")?,
                    effect: cursor.require_value(effect, "an any-color mana effect")?,
                    restriction: cursor.require_value(restriction, "a mana spend restriction")?,
                });
            }
        };
    }
}
