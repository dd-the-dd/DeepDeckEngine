use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalPlayerReference, NumericAtom, ObjectKind, ObjectPredicate, recognize_numeric_atom,
    recognize_object_type_union_list,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerCounterKind {
    Energy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerCounterPaymentBinding(pub usize);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerCounterGainEffectNode {
    pub player: LocalPlayerReference,
    pub counter: PlayerCounterKind,
    pub count: NumericAtom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct OptionalPlayerCounterPaymentEffectNode {
    pub player: LocalPlayerReference,
    pub counter: PlayerCounterKind,
    pub binding: PlayerCounterPaymentBinding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DestroyPermanentsByCounterPaymentEffectNode {
    pub predicate: ObjectPredicate,
    pub payment: PlayerCounterPaymentBinding,
    pub counter: PlayerCounterKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerCounterPaymentSweepSequenceNode {
    pub gain: PlayerCounterGainEffectNode,
    pub payment: OptionalPlayerCounterPaymentEffectNode,
    pub destruction: DestroyPermanentsByCounterPaymentEffectNode,
}

fn recognize_player_counter_symbol(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerCounterKind> {
    cursor.enter("player-counter.symbol");
    let symbol = cursor.consume_type("counterSymbol")?;
    cursor.require(
        symbol.raw.eq_ignore_ascii_case("{E}"),
        "the registered energy-counter symbol `{E}`",
    )?;
    Ok(PlayerCounterKind::Energy)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CounterGainState {
    Player,
    Action,
    Count,
    Symbol,
    ReminderOrComplete,
    ReminderOpen,
    ReminderArticleOrType,
    ReminderType,
    ReminderNoun,
    ReminderClose,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_player_counter_gain_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerCounterGainEffectNode> {
    let mut state = CounterGainState::Player;
    let mut count = None;
    let mut counter = None;
    loop {
        state = match state {
            CounterGainState::Player => {
                cursor.enter("effect.player-counter-gain.player");
                cursor.consume("you")?;
                CounterGainState::Action
            }
            CounterGainState::Action => {
                cursor.enter("effect.player-counter-gain.action");
                cursor.consume("get")?;
                CounterGainState::Count
            }
            CounterGainState::Count => {
                cursor.enter("effect.player-counter-gain.count");
                count = Some(recognize_numeric_atom(cursor)?);
                CounterGainState::Symbol
            }
            CounterGainState::Symbol => {
                cursor.enter("effect.player-counter-gain.symbol");
                counter = Some(recognize_player_counter_symbol(cursor)?);
                CounterGainState::ReminderOrComplete
            }
            CounterGainState::ReminderOrComplete => {
                cursor.enter("effect.player-counter-gain.reminder-or-complete");
                if cursor.peek().is_some_and(|raw| raw == "(") {
                    CounterGainState::ReminderOpen
                } else {
                    CounterGainState::Complete
                }
            }
            CounterGainState::ReminderOpen => {
                cursor.enter("effect.player-counter-gain.reminder-open");
                cursor.consume("(")?;
                CounterGainState::ReminderArticleOrType
            }
            CounterGainState::ReminderArticleOrType => {
                cursor.enter("effect.player-counter-gain.reminder-article-or-type");
                if cursor.peek().is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an")
                }) {
                    let article = cursor.peek().expect("checked article").to_string();
                    cursor.consume(&article)?;
                }
                CounterGainState::ReminderType
            }
            CounterGainState::ReminderType => {
                cursor.enter("effect.player-counter-gain.reminder-type");
                let token = cursor.consume_type("counterTypeWord")?;
                cursor.require(
                    token.raw.eq_ignore_ascii_case("energy"),
                    "the energy counter type",
                )?;
                CounterGainState::ReminderNoun
            }
            CounterGainState::ReminderNoun => {
                cursor.enter("effect.player-counter-gain.reminder-noun");
                let noun = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("`counter` or `counters`"))?
                    .to_string();
                cursor.require(
                    noun.eq_ignore_ascii_case("counter") || noun.eq_ignore_ascii_case("counters"),
                    "`counter` or `counters`",
                )?;
                cursor.consume(&noun)?;
                CounterGainState::ReminderClose
            }
            CounterGainState::ReminderClose => {
                cursor.enter("effect.player-counter-gain.reminder-close");
                cursor.consume(")")?;
                CounterGainState::Complete
            }
            CounterGainState::Complete => {
                cursor.enter("effect.player-counter-gain.complete");
                return Ok(PlayerCounterGainEffectNode {
                    player: LocalPlayerReference::Controller,
                    counter: cursor.require_value(counter, "a player-counter kind")?,
                    count: cursor.require_value(count, "a player-counter quantity")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CounterPaymentState {
    Player,
    Modality,
    Action,
    Quantifier,
    QuantityNoun,
    Of,
    Symbol,
    Terminator,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_optional_player_counter_payment_effect(
    cursor: &mut OracleTokenCursor<'_>,
    binding: PlayerCounterPaymentBinding,
) -> MachineResult<OptionalPlayerCounterPaymentEffectNode> {
    let mut state = CounterPaymentState::Player;
    let mut counter = None;
    loop {
        state = match state {
            CounterPaymentState::Player => {
                cursor.enter("effect.player-counter-payment.player");
                cursor.consume("you")?;
                CounterPaymentState::Modality
            }
            CounterPaymentState::Modality => {
                cursor.enter("effect.player-counter-payment.modality");
                cursor.consume("may")?;
                CounterPaymentState::Action
            }
            CounterPaymentState::Action => {
                cursor.enter("effect.player-counter-payment.action");
                cursor.consume("pay")?;
                CounterPaymentState::Quantifier
            }
            CounterPaymentState::Quantifier => {
                cursor.enter("effect.player-counter-payment.quantifier");
                cursor.consume("any")?;
                CounterPaymentState::QuantityNoun
            }
            CounterPaymentState::QuantityNoun => {
                cursor.enter("effect.player-counter-payment.quantity-noun");
                cursor.consume("amount")?;
                CounterPaymentState::Of
            }
            CounterPaymentState::Of => {
                cursor.enter("effect.player-counter-payment.of");
                cursor.consume("of")?;
                CounterPaymentState::Symbol
            }
            CounterPaymentState::Symbol => {
                cursor.enter("effect.player-counter-payment.symbol");
                counter = Some(recognize_player_counter_symbol(cursor)?);
                CounterPaymentState::Terminator
            }
            CounterPaymentState::Terminator => {
                cursor.enter("effect.player-counter-payment.terminator");
                cursor.consume(".")?;
                CounterPaymentState::Complete
            }
            CounterPaymentState::Complete => {
                cursor.enter("effect.player-counter-payment.complete");
                return Ok(OptionalPlayerCounterPaymentEffectNode {
                    player: LocalPlayerReference::Controller,
                    counter: cursor.require_value(counter, "a paid player-counter kind")?,
                    binding,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CounterPaymentSweepState {
    Action,
    Quantifier,
    Predicate,
    With,
    Property,
    Comparison,
    Determiner,
    QuantityNoun,
    Of,
    Symbol,
    Paid,
    Reference,
    Manner,
    Terminator,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_destroy_by_counter_payment_effect(
    cursor: &mut OracleTokenCursor<'_>,
    payment: PlayerCounterPaymentBinding,
) -> MachineResult<DestroyPermanentsByCounterPaymentEffectNode> {
    let mut state = CounterPaymentSweepState::Action;
    let mut predicate = None;
    let mut counter = None;
    loop {
        state = match state {
            CounterPaymentSweepState::Action => {
                cursor.enter("effect.counter-payment-sweep.action");
                cursor.consume("destroy")?;
                CounterPaymentSweepState::Quantifier
            }
            CounterPaymentSweepState::Quantifier => {
                cursor.enter("effect.counter-payment-sweep.quantifier");
                cursor.consume("each")?;
                CounterPaymentSweepState::Predicate
            }
            CounterPaymentSweepState::Predicate => {
                cursor.enter("effect.counter-payment-sweep.predicate");
                let parsed = recognize_object_type_union_list(cursor)?;
                cursor.require(
                    parsed.supports_domain(ObjectKind::Permanent)
                        && parsed.has_domain(ObjectKind::Permanent),
                    "one or more permanent card types",
                )?;
                predicate = Some(parsed);
                CounterPaymentSweepState::With
            }
            CounterPaymentSweepState::With => {
                cursor.enter("effect.counter-payment-sweep.with");
                cursor.consume("with")?;
                CounterPaymentSweepState::Property
            }
            CounterPaymentSweepState::Property => {
                cursor.enter("effect.counter-payment-sweep.property");
                cursor.consume_phrase(&["mana", "value"])?;
                CounterPaymentSweepState::Comparison
            }
            CounterPaymentSweepState::Comparison => {
                cursor.enter("effect.counter-payment-sweep.comparison");
                cursor.consume_phrase(&["less", "than", "or", "equal", "to"])?;
                CounterPaymentSweepState::Determiner
            }
            CounterPaymentSweepState::Determiner => {
                cursor.enter("effect.counter-payment-sweep.determiner");
                cursor.consume("the")?;
                CounterPaymentSweepState::QuantityNoun
            }
            CounterPaymentSweepState::QuantityNoun => {
                cursor.enter("effect.counter-payment-sweep.quantity-noun");
                cursor.consume("amount")?;
                CounterPaymentSweepState::Of
            }
            CounterPaymentSweepState::Of => {
                cursor.enter("effect.counter-payment-sweep.of");
                cursor.consume("of")?;
                CounterPaymentSweepState::Symbol
            }
            CounterPaymentSweepState::Symbol => {
                cursor.enter("effect.counter-payment-sweep.symbol");
                counter = Some(recognize_player_counter_symbol(cursor)?);
                CounterPaymentSweepState::Paid
            }
            CounterPaymentSweepState::Paid => {
                cursor.enter("effect.counter-payment-sweep.paid");
                cursor.consume("paid")?;
                CounterPaymentSweepState::Reference
            }
            CounterPaymentSweepState::Reference => {
                cursor.enter("effect.counter-payment-sweep.reference");
                cursor.consume("this")?;
                CounterPaymentSweepState::Manner
            }
            CounterPaymentSweepState::Manner => {
                cursor.enter("effect.counter-payment-sweep.manner");
                cursor.consume("way")?;
                CounterPaymentSweepState::Terminator
            }
            CounterPaymentSweepState::Terminator => {
                cursor.enter("effect.counter-payment-sweep.terminator");
                cursor.consume(".")?;
                CounterPaymentSweepState::Complete
            }
            CounterPaymentSweepState::Complete => {
                cursor.enter("effect.counter-payment-sweep.complete");
                return Ok(DestroyPermanentsByCounterPaymentEffectNode {
                    predicate: cursor.require_value(predicate, "a permanent-type predicate")?,
                    payment,
                    counter: cursor.require_value(counter, "a referenced player-counter kind")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlayerCounterSequenceState {
    Gain,
    ConnectorComma,
    ConnectorThen,
    Payment,
    Destruction,
    Validate,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_player_counter_payment_sweep_sequence(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerCounterPaymentSweepSequenceNode> {
    let mut state = PlayerCounterSequenceState::Gain;
    let mut gain = None;
    let mut payment = None;
    let mut destruction = None;
    let payment_binding = PlayerCounterPaymentBinding(0);
    loop {
        state = match state {
            PlayerCounterSequenceState::Gain => {
                cursor.enter("sequence.player-counter-sweep.gain");
                gain = Some(recognize_player_counter_gain_effect(cursor)?);
                PlayerCounterSequenceState::ConnectorComma
            }
            PlayerCounterSequenceState::ConnectorComma => {
                cursor.enter("sequence.player-counter-sweep.connector-comma");
                cursor.consume(",")?;
                PlayerCounterSequenceState::ConnectorThen
            }
            PlayerCounterSequenceState::ConnectorThen => {
                cursor.enter("sequence.player-counter-sweep.connector-then");
                cursor.consume("then")?;
                PlayerCounterSequenceState::Payment
            }
            PlayerCounterSequenceState::Payment => {
                cursor.enter("sequence.player-counter-sweep.payment");
                payment = Some(recognize_optional_player_counter_payment_effect(
                    cursor,
                    payment_binding,
                )?);
                PlayerCounterSequenceState::Destruction
            }
            PlayerCounterSequenceState::Destruction => {
                cursor.enter("sequence.player-counter-sweep.destruction");
                destruction = Some(recognize_destroy_by_counter_payment_effect(
                    cursor,
                    payment_binding,
                )?);
                PlayerCounterSequenceState::Validate
            }
            PlayerCounterSequenceState::Validate => {
                cursor.enter("sequence.player-counter-sweep.validate");
                let gain_ref = gain
                    .as_ref()
                    .ok_or_else(|| cursor.fail("a player-counter gain effect"))?;
                let payment_ref = payment
                    .as_ref()
                    .ok_or_else(|| cursor.fail("an optional player-counter payment"))?;
                let destruction_ref = destruction
                    .as_ref()
                    .ok_or_else(|| cursor.fail("a payment-linked destruction effect"))?;
                cursor.require(
                    gain_ref.counter == payment_ref.counter
                        && payment_ref.counter == destruction_ref.counter,
                    "the same player-counter kind across gain, payment, and threshold",
                )?;
                cursor.require(
                    payment_ref.binding == destruction_ref.payment,
                    "the destruction threshold linked to the preceding payment",
                )?;
                PlayerCounterSequenceState::Complete
            }
            PlayerCounterSequenceState::Complete => {
                cursor.enter("sequence.player-counter-sweep.complete");
                cursor.finish()?;
                return Ok(PlayerCounterPaymentSweepSequenceNode {
                    gain: cursor.require_value(gain, "a player-counter gain effect")?,
                    payment: cursor.require_value(payment, "an optional player-counter payment")?,
                    destruction: cursor
                        .require_value(destruction, "a payment-linked destruction effect")?,
                });
            }
        };
    }
}
