use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, CardSupertype, CardType, ObjectDomain, lookup_card_type, lookup_object_domain,
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    BasicLandTypeChoiceNode, ChoiceBinding, ChosenLandTypeEntryNode, ConditionalEnteringStateNode,
    EffectConnector, EntryObjectKind, LocalObjectReference, LocalPlayerReference, NumericAtom,
    OptionalLifePaymentNode, RetainChosenLandTypeNode, SetEnteringStateNode,
    recognize_cost_not_paid_condition, recognize_numeric_atom,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObjectKindState {
    Noun,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_entry_object_kind(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EntryObjectKind> {
    let mut state = ObjectKindState::Noun;
    let mut kind = None;
    loop {
        state = match state {
            ObjectKindState::Noun => {
                cursor.enter("reference.entry-object.noun");
                kind = Some(match cursor.peek() {
                    Some(value)
                        if lookup_card_type(value, AcceptedForms::SingularOnly)
                            == Some(CardType::Land) =>
                    {
                        cursor.consume(CardType::Land.oracle_singular())?;
                        EntryObjectKind::Land
                    }
                    Some(value)
                        if lookup_object_domain(value, AcceptedForms::SingularOnly)
                            == Some(ObjectDomain::Permanent) =>
                    {
                        cursor.consume(ObjectDomain::Permanent.oracle_singular())?;
                        EntryObjectKind::Permanent
                    }
                    _ => return Err(cursor.fail("land or permanent")),
                });
                ObjectKindState::Complete
            }
            ObjectKindState::Complete => {
                cursor.enter("reference.entry-object.complete");
                return cursor.require_value(kind, "a populated entering-object kind");
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LandTypeChoiceState {
    Action,
    Article,
    Supertype,
    CardType,
    Property,
    Terminator,
    Complete,
}

fn recognize_basic_land_type_choice(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<BasicLandTypeChoiceNode> {
    let mut state = LandTypeChoiceState::Action;
    loop {
        state = match state {
            LandTypeChoiceState::Action => {
                cursor.enter("choice.basic-land-type.action");
                cursor.consume("choose")?;
                LandTypeChoiceState::Article
            }
            LandTypeChoiceState::Article => {
                cursor.enter("choice.basic-land-type.article");
                cursor.consume("a")?;
                LandTypeChoiceState::Supertype
            }
            LandTypeChoiceState::Supertype => {
                cursor.enter("choice.basic-land-type.supertype");
                cursor.consume(CardSupertype::Basic.oracle_singular())?;
                LandTypeChoiceState::CardType
            }
            LandTypeChoiceState::CardType => {
                cursor.enter("choice.basic-land-type.card-type");
                cursor.consume(CardType::Land.oracle_singular())?;
                LandTypeChoiceState::Property
            }
            LandTypeChoiceState::Property => {
                cursor.enter("choice.basic-land-type.property");
                cursor.consume("type")?;
                LandTypeChoiceState::Terminator
            }
            LandTypeChoiceState::Terminator => {
                cursor.enter("choice.basic-land-type.terminator");
                cursor.consume(".")?;
                LandTypeChoiceState::Complete
            }
            LandTypeChoiceState::Complete => {
                cursor.enter("choice.basic-land-type.complete");
                return Ok(BasicLandTypeChoiceNode {
                    player: LocalPlayerReference::Controller,
                    binding: ChoiceBinding::BasicLandType,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LifePaymentState {
    Connector,
    Player,
    Modality,
    Action,
    Amount,
    Resource,
    Terminator,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_optional_life_payment(
    cursor: &mut OracleTokenCursor<'_>,
    connector: Option<EffectConnector>,
) -> MachineResult<OptionalLifePaymentNode> {
    let mut state = if connector.is_some() {
        LifePaymentState::Connector
    } else {
        LifePaymentState::Player
    };
    let mut amount = None;
    loop {
        state = match state {
            LifePaymentState::Connector => {
                cursor.enter("choice.optional-life-payment.connector");
                match connector {
                    Some(EffectConnector::Then) => cursor.consume("then")?,
                    Some(EffectConnector::And) => cursor.consume("and")?,
                    None => return Err(cursor.fail("an explicit effect connector")),
                }
                LifePaymentState::Player
            }
            LifePaymentState::Player => {
                cursor.enter("choice.optional-life-payment.player");
                cursor.consume("you")?;
                LifePaymentState::Modality
            }
            LifePaymentState::Modality => {
                cursor.enter("choice.optional-life-payment.modality");
                cursor.consume("may")?;
                LifePaymentState::Action
            }
            LifePaymentState::Action => {
                cursor.enter("choice.optional-life-payment.action");
                cursor.consume("pay")?;
                LifePaymentState::Amount
            }
            LifePaymentState::Amount => {
                cursor.enter("choice.optional-life-payment.amount");
                let NumericAtom::Integer(value) = recognize_numeric_atom(cursor)? else {
                    return Err(cursor.fail("a fixed positive life amount"));
                };
                cursor.require(value > 0, "a fixed positive life amount")?;
                amount = Some(value);
                LifePaymentState::Resource
            }
            LifePaymentState::Resource => {
                cursor.enter("choice.optional-life-payment.resource");
                cursor.consume("life")?;
                LifePaymentState::Terminator
            }
            LifePaymentState::Terminator => {
                cursor.enter("choice.optional-life-payment.terminator");
                cursor.consume(".")?;
                LifePaymentState::Complete
            }
            LifePaymentState::Complete => {
                cursor.enter("choice.optional-life-payment.complete");
                let amount = cursor.require_value(amount, "a populated life amount")?;
                return Ok(OptionalLifePaymentNode {
                    player: LocalPlayerReference::Controller,
                    amount,
                    binding: ChoiceBinding::OptionalLifePayment,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EnteringStateEffectState {
    ObjectReference,
    Event,
    State,
    Terminator,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_tapped_entry_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SetEnteringStateNode> {
    let mut state = EnteringStateEffectState::ObjectReference;
    loop {
        state = match state {
            EnteringStateEffectState::ObjectReference => {
                cursor.enter("effect.set-entering-state.object-reference");
                cursor.consume("it")?;
                EnteringStateEffectState::Event
            }
            EnteringStateEffectState::Event => {
                cursor.enter("effect.set-entering-state.event");
                cursor.consume("enters")?;
                EnteringStateEffectState::State
            }
            EnteringStateEffectState::State => {
                cursor.enter("effect.set-entering-state.state");
                cursor.consume("tapped")?;
                EnteringStateEffectState::Terminator
            }
            EnteringStateEffectState::Terminator => {
                cursor.enter("effect.set-entering-state.terminator");
                cursor.consume(".")?;
                EnteringStateEffectState::Complete
            }
            EnteringStateEffectState::Complete => {
                cursor.enter("effect.set-entering-state.complete");
                return Ok(SetEnteringStateNode {
                    object: LocalObjectReference::SourceSelf,
                    tapped: true,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChosenEntryState {
    As,
    Source,
    ObjectKind,
    Event,
    Separator,
    Choice,
    Payment,
    ConditionalEntry,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_chosen_land_type_entry(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ChosenLandTypeEntryNode> {
    let mut state = ChosenEntryState::As;
    let mut object_kind = None;
    let mut choice = None;
    let mut payment = None;
    let mut unpaid_entry = None;
    loop {
        state = match state {
            ChosenEntryState::As => {
                cursor.enter("replacement.chosen-land-type.as");
                cursor.consume("as")?;
                ChosenEntryState::Source
            }
            ChosenEntryState::Source => {
                cursor.enter("replacement.chosen-land-type.source");
                cursor.consume("this")?;
                ChosenEntryState::ObjectKind
            }
            ChosenEntryState::ObjectKind => {
                cursor.enter("replacement.chosen-land-type.object-kind");
                object_kind = Some(recognize_entry_object_kind(cursor)?);
                ChosenEntryState::Event
            }
            ChosenEntryState::Event => {
                cursor.enter("replacement.chosen-land-type.event");
                cursor.consume("enters")?;
                ChosenEntryState::Separator
            }
            ChosenEntryState::Separator => {
                cursor.enter("replacement.chosen-land-type.separator");
                cursor.consume(",")?;
                ChosenEntryState::Choice
            }
            ChosenEntryState::Choice => {
                cursor.enter("replacement.chosen-land-type.choice");
                choice = Some(recognize_basic_land_type_choice(cursor)?);
                ChosenEntryState::Payment
            }
            ChosenEntryState::Payment => {
                cursor.enter("replacement.chosen-land-type.payment");
                payment = Some(recognize_optional_life_payment(
                    cursor,
                    Some(EffectConnector::Then),
                )?);
                ChosenEntryState::ConditionalEntry
            }
            ChosenEntryState::ConditionalEntry => {
                cursor.enter("replacement.chosen-land-type.conditional-entry");
                let binding = payment
                    .as_ref()
                    .map(|node: &OptionalLifePaymentNode| node.binding)
                    .ok_or_else(|| cursor.fail("a bound optional life payment"))?;
                let condition = recognize_cost_not_paid_condition(cursor, binding)?;
                let effect = recognize_tapped_entry_effect(cursor)?;
                unpaid_entry = Some(ConditionalEnteringStateNode { condition, effect });
                ChosenEntryState::Complete
            }
            ChosenEntryState::Complete => {
                cursor.enter("replacement.chosen-land-type.complete");
                cursor.finish()?;
                return Ok(ChosenLandTypeEntryNode {
                    object_kind: cursor.require_value(object_kind, "an entering-object kind")?,
                    choice: cursor.require_value(choice, "a basic-land-type choice")?,
                    payment: cursor.require_value(payment, "an optional life payment")?,
                    unpaid_entry: cursor
                        .require_value(unpaid_entry, "an unpaid tapped-entry consequence")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RetainChoiceState {
    Source,
    ObjectKind,
    Predicate,
    Determiner,
    ChoiceReference,
    Property,
    Terminator,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_retain_chosen_land_type(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<RetainChosenLandTypeNode> {
    let mut state = RetainChoiceState::Source;
    let mut object_kind = None;
    loop {
        state = match state {
            RetainChoiceState::Source => {
                cursor.enter("static.retain-chosen-land-type.source");
                cursor.consume("this")?;
                RetainChoiceState::ObjectKind
            }
            RetainChoiceState::ObjectKind => {
                cursor.enter("static.retain-chosen-land-type.object-kind");
                object_kind = Some(recognize_entry_object_kind(cursor)?);
                RetainChoiceState::Predicate
            }
            RetainChoiceState::Predicate => {
                cursor.enter("static.retain-chosen-land-type.predicate");
                cursor.consume("is")?;
                RetainChoiceState::Determiner
            }
            RetainChoiceState::Determiner => {
                cursor.enter("static.retain-chosen-land-type.determiner");
                cursor.consume("the")?;
                RetainChoiceState::ChoiceReference
            }
            RetainChoiceState::ChoiceReference => {
                cursor.enter("static.retain-chosen-land-type.choice-reference");
                cursor.consume("chosen")?;
                RetainChoiceState::Property
            }
            RetainChoiceState::Property => {
                cursor.enter("static.retain-chosen-land-type.property");
                cursor.consume("type")?;
                RetainChoiceState::Terminator
            }
            RetainChoiceState::Terminator => {
                cursor.enter("static.retain-chosen-land-type.terminator");
                cursor.consume(".")?;
                RetainChoiceState::Complete
            }
            RetainChoiceState::Complete => {
                cursor.enter("static.retain-chosen-land-type.complete");
                cursor.finish()?;
                return Ok(RetainChosenLandTypeNode {
                    object_kind: cursor.require_value(object_kind, "a referenced object kind")?,
                    choice: ChoiceBinding::BasicLandType,
                });
            }
        };
    }
}
