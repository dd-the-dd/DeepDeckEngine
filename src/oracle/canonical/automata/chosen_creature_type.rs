use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalPlayerReference, ObjectCardType, ObjectKind, ObjectPredicate,
    positive_object_selector_atom,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CharacteristicChoiceBinding {
    CreatureType,
}

impl CharacteristicChoiceBinding {
    pub(in crate::oracle::canonical) const fn decision_id(self) -> &'static str {
        match self {
            Self::CreatureType => "chosenCreatureType",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CharacteristicChoiceNode {
    pub player: LocalPlayerReference,
    pub binding: CharacteristicChoiceBinding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChosenCreatureTypeEntryNode {
    pub entering_object: ObjectPredicate,
    pub choice: CharacteristicChoiceNode,
}

fn is_permanent_entry_predicate(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::CardType(card_type) => card_type.is_permanent_type(),
        ObjectPredicate::ObjectKind(ObjectKind::Permanent) => true,
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntryObjectState {
    Noun,
    Complete,
}

fn recognize_permanent_entry_object(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectPredicate> {
    let mut state = EntryObjectState::Noun;
    let mut predicate = None;
    loop {
        state = match state {
            EntryObjectState::Noun => {
                cursor.enter("reference.permanent-entry-object.noun");
                let raw = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("a permanent card type or `permanent`"))?
                    .to_string();
                let parsed = positive_object_selector_atom(&raw)
                    .filter(is_permanent_entry_predicate)
                    .ok_or_else(|| cursor.fail("a permanent card type or `permanent`"))?;
                cursor.consume(&raw)?;
                predicate = Some(parsed);
                EntryObjectState::Complete
            }
            EntryObjectState::Complete => {
                cursor.enter("reference.permanent-entry-object.complete");
                return cursor.require_value(predicate, "a populated entering-object predicate");
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CreatureTypeChoiceState {
    Action,
    Article,
    CharacteristicDomain,
    Property,
    Complete,
}

fn recognize_creature_type_choice(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CharacteristicChoiceNode> {
    let mut state = CreatureTypeChoiceState::Action;
    loop {
        state = match state {
            CreatureTypeChoiceState::Action => {
                cursor.enter("choice.creature-type.action");
                cursor.consume("choose")?;
                CreatureTypeChoiceState::Article
            }
            CreatureTypeChoiceState::Article => {
                cursor.enter("choice.creature-type.article");
                cursor.consume("a")?;
                CreatureTypeChoiceState::CharacteristicDomain
            }
            CreatureTypeChoiceState::CharacteristicDomain => {
                cursor.enter("choice.creature-type.characteristic-domain");
                let predicate = positive_object_selector_atom(
                    cursor
                        .peek()
                        .ok_or_else(|| cursor.fail("the creature card type"))?,
                );
                cursor.require(
                    predicate == Some(ObjectPredicate::CardType(ObjectCardType::Creature)),
                    "the creature card type",
                )?;
                cursor.consume(ObjectCardType::Creature.oracle_singular())?;
                CreatureTypeChoiceState::Property
            }
            CreatureTypeChoiceState::Property => {
                cursor.enter("choice.creature-type.property");
                cursor.consume("type")?;
                CreatureTypeChoiceState::Complete
            }
            CreatureTypeChoiceState::Complete => {
                cursor.enter("choice.creature-type.complete");
                return Ok(CharacteristicChoiceNode {
                    player: LocalPlayerReference::Controller,
                    binding: CharacteristicChoiceBinding::CreatureType,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChosenCreatureTypeEntryState {
    As,
    Source,
    Object,
    Event,
    Separator,
    Choice,
    Terminator,
    Complete,
}

/// Recognizes `As this <permanent-kind> enters, choose a creature type.`
/// from the shared token stream. The entering-object domain and the persistent
/// characteristic choice remain independent typed children.
pub(in crate::oracle::canonical) fn recognize_chosen_creature_type_entry(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ChosenCreatureTypeEntryNode> {
    let mut state = ChosenCreatureTypeEntryState::As;
    let mut entering_object = None;
    let mut choice = None;
    loop {
        state = match state {
            ChosenCreatureTypeEntryState::As => {
                cursor.enter("replacement.chosen-creature-type.as");
                cursor.consume("as")?;
                ChosenCreatureTypeEntryState::Source
            }
            ChosenCreatureTypeEntryState::Source => {
                cursor.enter("replacement.chosen-creature-type.source");
                cursor.consume("this")?;
                ChosenCreatureTypeEntryState::Object
            }
            ChosenCreatureTypeEntryState::Object => {
                cursor.enter("replacement.chosen-creature-type.object");
                entering_object = Some(recognize_permanent_entry_object(cursor)?);
                ChosenCreatureTypeEntryState::Event
            }
            ChosenCreatureTypeEntryState::Event => {
                cursor.enter("replacement.chosen-creature-type.event");
                cursor.consume("enters")?;
                ChosenCreatureTypeEntryState::Separator
            }
            ChosenCreatureTypeEntryState::Separator => {
                cursor.enter("replacement.chosen-creature-type.separator");
                cursor.consume(",")?;
                ChosenCreatureTypeEntryState::Choice
            }
            ChosenCreatureTypeEntryState::Choice => {
                cursor.enter("replacement.chosen-creature-type.choice");
                choice = Some(recognize_creature_type_choice(cursor)?);
                ChosenCreatureTypeEntryState::Terminator
            }
            ChosenCreatureTypeEntryState::Terminator => {
                cursor.enter("replacement.chosen-creature-type.terminator");
                cursor.consume(".")?;
                ChosenCreatureTypeEntryState::Complete
            }
            ChosenCreatureTypeEntryState::Complete => {
                cursor.enter("replacement.chosen-creature-type.complete");
                cursor.finish()?;
                return Ok(ChosenCreatureTypeEntryNode {
                    entering_object: cursor
                        .require_value(entering_object, "an entering permanent predicate")?,
                    choice: cursor.require_value(choice, "a creature-type choice")?,
                });
            }
        };
    }
}
