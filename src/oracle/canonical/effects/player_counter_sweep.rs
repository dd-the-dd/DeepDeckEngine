use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_player_counter_payment_sweep_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("you"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("get"))
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("pay"))
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("destroy"))
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("paid"))
        && tokens.last().is_some_and(|token| token.raw == ".")
}

fn counter_name(counter: PlayerCounterKind) -> &'static str {
    match counter {
        PlayerCounterKind::Energy => "energy",
    }
}

fn payment_decision_id(binding: PlayerCounterPaymentBinding) -> &'static str {
    debug_assert_eq!(binding, PlayerCounterPaymentBinding(0));
    "paidPlayerCounters"
}

pub(in crate::oracle::canonical) fn parse_promoted_player_counter_payment_sweep(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("player-counter-payment-sweep", input.tokens);
    let sequence = recognize_player_counter_payment_sweep_sequence(&mut cursor)?;
    cursor.enter("sequence.player-counter-sweep.lowering");
    cursor.require(
        sequence.gain.player == LocalPlayerReference::Controller
            && sequence.payment.player == LocalPlayerReference::Controller,
        "the controller as player-counter recipient and payer",
    )?;
    let counter = counter_name(sequence.gain.counter);
    let payment_id = payment_decision_id(sequence.payment.binding);
    let paid = decision_result(payment_id);
    let effects = vec![
        json!({
            "kind": "addPlayerCounters",
            "player": controller(),
            "counter": counter,
            "count": lower_numeric_atom(sequence.gain.count.clone()),
        }),
        json!({
            "kind": "chooseNumber",
            "id": payment_id,
            "player": controller(),
            "minimum": integer(0),
            "maximum": {
                "kind": "countPlayerCounters",
                "player": controller(),
                "counter": counter,
            },
        }),
        json!({
            "kind": "removePlayerCounters",
            "player": controller(),
            "counter": counter,
            "count": paid.clone(),
        }),
        json!({
            "kind": "destroyPermanentsByManaValue",
            "where": lower_permanent_object_predicate(&sequence.destruction.predicate),
            "maximum": paid,
        }),
    ];
    let mut rule = json!({
        "kind": "spellAbility",
        "source": self_ref(),
        "effects": effects,
    });
    if numeric_atom_uses_x(&sequence.gain.count) {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": [x_value()],
        });
    }
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        rule,
        &[
            "Recognize player-counter gain through a typed counter symbol",
            "Bind the optional counter payment as one numeric decision",
            "Resolve the paid-this-way threshold through the payment binding",
            "Lower the permanent-type list through one Boolean union predicate",
            "Compose four existing primitive effects in Oracle order",
            &trace,
        ],
    ))
}
