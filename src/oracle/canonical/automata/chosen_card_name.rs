use crate::oracle::{
    characteristic_vocabulary::{AcceptedForms, lookup_permanent_card_type},
    syntax::{NameResolutionFailure, ResolvedNameLink},
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ChoiceBinding, LocalObjectReference, LocalPlayerReference, ManaCostNode, ObjectCardType,
    recognize_mana_cost, recognize_source_reference,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardNameDomain {
    Any,
    CardType(ObjectCardType),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardNameChoiceNode {
    pub player: LocalPlayerReference,
    pub binding: ChoiceBinding,
    pub domain: CardNameDomain,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AsEntersCardNameChoiceNode {
    pub source: LocalObjectReference,
    pub asserted_source_type: Option<ObjectCardType>,
    pub choice: CardNameChoiceNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChosenCardNamePredicateNode {
    pub binding: ChoiceBinding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChosenNameSpellTaxNode {
    pub predicate: ChosenCardNamePredicateNode,
    pub cost: ManaCostNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChosenNameActivationProhibitionNode {
    pub predicate: ChosenCardNamePredicateNode,
    pub excludes_mana_abilities: bool,
}

fn permanent_type(raw: Option<&str>) -> Option<ObjectCardType> {
    lookup_permanent_card_type(raw?, AcceptedForms::SingularOnly)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EnteringSourceState {
    Reference,
    ThisObjectType,
    Complete,
}

fn recognize_entering_source(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<(LocalObjectReference, Option<ObjectCardType>)> {
    let mut state = EnteringSourceState::Reference;
    let mut reference = None;
    let mut asserted_type = None;
    loop {
        state = match state {
            EnteringSourceState::Reference => {
                cursor.enter("reference.entering-source.start");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
                {
                    cursor.consume("this")?;
                    reference = Some(LocalObjectReference::SourceSelf);
                    EnteringSourceState::ThisObjectType
                } else {
                    reference = Some(recognize_source_reference(
                        cursor,
                        name_links,
                        name_failure,
                    )?);
                    EnteringSourceState::Complete
                }
            }
            EnteringSourceState::ThisObjectType => {
                cursor.enter("reference.entering-source.object-type");
                let object_type = permanent_type(cursor.peek())
                    .ok_or_else(|| cursor.fail("a permanent type after `this`"))?;
                cursor.advance_to(cursor.position() + 1)?;
                asserted_type = Some(object_type);
                EnteringSourceState::Complete
            }
            EnteringSourceState::Complete => {
                cursor.enter("reference.entering-source.complete");
                return Ok((
                    cursor.require_value(reference, "the entering source object")?,
                    asserted_type,
                ));
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardNameChoiceState {
    Action,
    Quantifier,
    DomainOrCard,
    CardNoun,
    Property,
    Terminator,
    Complete,
}

/// Recognizes a free card-name decision. This is deliberately distinct from
/// lexical name resolution: no concrete card name appears in this phrase.
pub(in crate::oracle::canonical) fn recognize_card_name_choice(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardNameChoiceNode> {
    let mut state = CardNameChoiceState::Action;
    let mut domain = CardNameDomain::Any;
    loop {
        state = match state {
            CardNameChoiceState::Action => {
                cursor.enter("choice.card-name.action");
                cursor.consume("choose")?;
                CardNameChoiceState::Quantifier
            }
            CardNameChoiceState::Quantifier => {
                cursor.enter("choice.card-name.quantifier");
                let quantifier = cursor
                    .peek()
                    .filter(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("any"))
                    .ok_or_else(|| cursor.fail("`a` or `any`"))?
                    .to_string();
                cursor.consume(&quantifier)?;
                CardNameChoiceState::DomainOrCard
            }
            CardNameChoiceState::DomainOrCard => {
                cursor.enter("choice.card-name.domain");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("card"))
                {
                    CardNameChoiceState::CardNoun
                } else {
                    let object_type = permanent_type(cursor.peek())
                        .ok_or_else(|| cursor.fail("`card` or a permanent card type"))?;
                    cursor.advance_to(cursor.position() + 1)?;
                    domain = CardNameDomain::CardType(object_type);
                    CardNameChoiceState::CardNoun
                }
            }
            CardNameChoiceState::CardNoun => {
                cursor.enter("choice.card-name.card-noun");
                cursor.consume("card")?;
                CardNameChoiceState::Property
            }
            CardNameChoiceState::Property => {
                cursor.enter("choice.card-name.property");
                cursor.consume("name")?;
                CardNameChoiceState::Terminator
            }
            CardNameChoiceState::Terminator => {
                cursor.enter("choice.card-name.terminator");
                cursor.consume(".")?;
                CardNameChoiceState::Complete
            }
            CardNameChoiceState::Complete => {
                cursor.enter("choice.card-name.complete");
                return Ok(CardNameChoiceNode {
                    player: LocalPlayerReference::Controller,
                    binding: ChoiceBinding::CardName,
                    domain,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AsEntersChoiceState {
    As,
    Source,
    Event,
    Separator,
    Choice,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_as_enters_card_name_choice(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<AsEntersCardNameChoiceNode> {
    let mut state = AsEntersChoiceState::As;
    let mut source = None;
    let mut asserted_source_type = None;
    let mut choice = None;
    loop {
        state = match state {
            AsEntersChoiceState::As => {
                cursor.enter("replacement.card-name-choice.as");
                cursor.consume("as")?;
                AsEntersChoiceState::Source
            }
            AsEntersChoiceState::Source => {
                cursor.enter("replacement.card-name-choice.source");
                let (recognized_source, object_type) =
                    recognize_entering_source(cursor, name_links, name_failure)?;
                source = Some(recognized_source);
                asserted_source_type = object_type;
                AsEntersChoiceState::Event
            }
            AsEntersChoiceState::Event => {
                cursor.enter("replacement.card-name-choice.event");
                cursor.consume("enters")?;
                AsEntersChoiceState::Separator
            }
            AsEntersChoiceState::Separator => {
                cursor.enter("replacement.card-name-choice.separator");
                cursor.consume(",")?;
                AsEntersChoiceState::Choice
            }
            AsEntersChoiceState::Choice => {
                cursor.enter("replacement.card-name-choice.choice");
                choice = Some(recognize_card_name_choice(cursor)?);
                AsEntersChoiceState::Complete
            }
            AsEntersChoiceState::Complete => {
                cursor.enter("replacement.card-name-choice.complete");
                cursor.finish()?;
                return Ok(AsEntersCardNameChoiceNode {
                    source: cursor.require_value(source, "the entering source")?,
                    asserted_source_type,
                    choice: cursor.require_value(choice, "a card-name choice")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChosenNamePredicateState {
    Relation,
    Determiner,
    Selection,
    Property,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_chosen_card_name_predicate(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ChosenCardNamePredicateNode> {
    let mut state = ChosenNamePredicateState::Relation;
    loop {
        state = match state {
            ChosenNamePredicateState::Relation => {
                cursor.enter("predicate.chosen-card-name.relation");
                cursor.consume("with")?;
                ChosenNamePredicateState::Determiner
            }
            ChosenNamePredicateState::Determiner => {
                cursor.enter("predicate.chosen-card-name.determiner");
                cursor.consume("the")?;
                ChosenNamePredicateState::Selection
            }
            ChosenNamePredicateState::Selection => {
                cursor.enter("predicate.chosen-card-name.selection");
                cursor.consume("chosen")?;
                ChosenNamePredicateState::Property
            }
            ChosenNamePredicateState::Property => {
                cursor.enter("predicate.chosen-card-name.property");
                cursor.consume("name")?;
                ChosenNamePredicateState::Complete
            }
            ChosenNamePredicateState::Complete => {
                cursor.enter("predicate.chosen-card-name.complete");
                return Ok(ChosenCardNamePredicateNode {
                    binding: ChoiceBinding::CardName,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpellTaxState {
    Subject,
    Predicate,
    CostVerb,
    Cost,
    Increase,
    Infinitive,
    Terminator,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_chosen_name_spell_tax(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ChosenNameSpellTaxNode> {
    let mut state = SpellTaxState::Subject;
    let mut predicate = None;
    let mut cost = None;
    loop {
        state = match state {
            SpellTaxState::Subject => {
                cursor.enter("static.chosen-name-spell-tax.subject");
                cursor.consume("spells")?;
                SpellTaxState::Predicate
            }
            SpellTaxState::Predicate => {
                cursor.enter("static.chosen-name-spell-tax.predicate");
                predicate = Some(recognize_chosen_card_name_predicate(cursor)?);
                SpellTaxState::CostVerb
            }
            SpellTaxState::CostVerb => {
                cursor.enter("static.chosen-name-spell-tax.cost-verb");
                cursor.consume("cost")?;
                SpellTaxState::Cost
            }
            SpellTaxState::Cost => {
                cursor.enter("static.chosen-name-spell-tax.cost");
                cost = Some(recognize_mana_cost(cursor)?);
                SpellTaxState::Increase
            }
            SpellTaxState::Increase => {
                cursor.enter("static.chosen-name-spell-tax.increase");
                cursor.consume("more")?;
                SpellTaxState::Infinitive
            }
            SpellTaxState::Infinitive => {
                cursor.enter("static.chosen-name-spell-tax.infinitive");
                cursor.consume_phrase(&["to", "cast"])?;
                SpellTaxState::Terminator
            }
            SpellTaxState::Terminator => {
                cursor.enter("static.chosen-name-spell-tax.terminator");
                cursor.consume(".")?;
                SpellTaxState::Complete
            }
            SpellTaxState::Complete => {
                cursor.enter("static.chosen-name-spell-tax.complete");
                cursor.finish()?;
                return Ok(ChosenNameSpellTaxNode {
                    predicate: cursor.require_value(predicate, "a chosen-name predicate")?,
                    cost: cursor.require_value(cost, "an additional mana cost")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivationProhibitionState {
    AbilityKind,
    Relation,
    SourceNoun,
    Predicate,
    Prohibition,
    ActivationVerb,
    ExceptionOrTerminator,
    ExceptionSubject,
    ExceptionAbilityKind,
    Terminator,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_chosen_name_activation_prohibition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ChosenNameActivationProhibitionNode> {
    let mut state = ActivationProhibitionState::AbilityKind;
    let mut predicate = None;
    let mut excludes_mana_abilities = false;
    loop {
        state = match state {
            ActivationProhibitionState::AbilityKind => {
                cursor.enter("static.activation-prohibition.ability-kind");
                cursor.consume_phrase(&["activated", "abilities"])?;
                ActivationProhibitionState::Relation
            }
            ActivationProhibitionState::Relation => {
                cursor.enter("static.activation-prohibition.relation");
                cursor.consume("of")?;
                ActivationProhibitionState::SourceNoun
            }
            ActivationProhibitionState::SourceNoun => {
                cursor.enter("static.activation-prohibition.source-noun");
                cursor.consume("sources")?;
                ActivationProhibitionState::Predicate
            }
            ActivationProhibitionState::Predicate => {
                cursor.enter("static.activation-prohibition.source-predicate");
                predicate = Some(recognize_chosen_card_name_predicate(cursor)?);
                ActivationProhibitionState::Prohibition
            }
            ActivationProhibitionState::Prohibition => {
                cursor.enter("static.activation-prohibition.prohibition");
                cursor.consume_phrase(&["can't", "be"])?;
                ActivationProhibitionState::ActivationVerb
            }
            ActivationProhibitionState::ActivationVerb => {
                cursor.enter("static.activation-prohibition.activation-verb");
                cursor.consume("activated")?;
                ActivationProhibitionState::ExceptionOrTerminator
            }
            ActivationProhibitionState::ExceptionOrTerminator => {
                cursor.enter("static.activation-prohibition.exception-or-terminator");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("unless"))
                {
                    cursor.consume("unless")?;
                    excludes_mana_abilities = true;
                    ActivationProhibitionState::ExceptionSubject
                } else {
                    ActivationProhibitionState::Terminator
                }
            }
            ActivationProhibitionState::ExceptionSubject => {
                cursor.enter("static.activation-prohibition.exception-subject");
                cursor.consume("they're")?;
                ActivationProhibitionState::ExceptionAbilityKind
            }
            ActivationProhibitionState::ExceptionAbilityKind => {
                cursor.enter("static.activation-prohibition.exception-ability-kind");
                cursor.consume_phrase(&["mana", "abilities"])?;
                ActivationProhibitionState::Terminator
            }
            ActivationProhibitionState::Terminator => {
                cursor.enter("static.activation-prohibition.terminator");
                cursor.consume(".")?;
                ActivationProhibitionState::Complete
            }
            ActivationProhibitionState::Complete => {
                cursor.enter("static.activation-prohibition.complete");
                cursor.finish()?;
                return Ok(ChosenNameActivationProhibitionNode {
                    predicate: cursor.require_value(predicate, "a chosen-name source predicate")?,
                    excludes_mana_abilities,
                });
            }
        };
    }
}
