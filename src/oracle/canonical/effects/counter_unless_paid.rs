use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims one targeted spell counter with a controller-payment exception.
/// The claim commits at token-shape boundaries; target and payment children
/// retain precise diagnostics for malformed family members. Only one bare
/// sentence is claimed: a later sentence or a postposed clause (`, where X`)
/// belongs to the shared sequence and its `unless` frame.
pub(in crate::oracle::canonical) fn claims_promoted_counter_unless_paid_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() >= 8
        && tokens[..tokens.len() - 1]
            .iter()
            .all(|token| token.raw != "." && token.raw != ",")
        && tokens
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("counter"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("target"))
        && tokens.windows(2).any(|window| {
            window[0].raw.eq_ignore_ascii_case("spell")
                && window[1].raw.eq_ignore_ascii_case("unless")
        })
        && tokens.last().is_some_and(|token| token.raw == ".")
}

pub(in crate::oracle::canonical) fn parse_promoted_counter_unless_paid(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("counter-unless-paid", input.tokens);
    let node = recognize_counter_unless_paid_effect(&mut cursor)?;
    let slot = node
        .counter
        .target
        .slots
        .first()
        .expect("counter child validates one target slot");
    cursor.enter("effect.counter-unless-paid.spell-domain");
    cursor.require(
        node.counter.target.slots.len() == 1 && slot.cardinality == TargetCardinality::EXACTLY_ONE,
        "one required spell target",
    )?;

    let mut candidates = match &slot.selector {
        TargetSelector::Object(_) => json!({ "kind": "spells" }),
        TargetSelector::ControlledObject {
            controller: controller_scope,
            ..
        } => json!({
            "kind": "spells",
            "controller": lower_object_controller_scope(*controller_scope),
        }),
        TargetSelector::Any
        | TargetSelector::Union(_)
        | TargetSelector::Player(_)
        | TargetSelector::StackItem(_) => {
            return Err(cursor.fail("a spell target"));
        }
    };
    let predicate = match &slot.selector {
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            predicate
        }
        TargetSelector::Any
        | TargetSelector::Union(_)
        | TargetSelector::Player(_)
        | TargetSelector::StackItem(_) => {
            unreachable!("non-spell target rejected above")
        }
    };
    if let Some(filter) = lower_spell_object_predicate(predicate) {
        candidates["where"] = filter;
    }
    cursor.enter("effect.counter-unless-paid.payment-support");
    let payment = if node.payment.contains_variable_x() {
        let amount = node.payment.generic_payment_expression()
            .and_then(|expression|lower_numeric_expression(&expression))
            .ok_or_else(||cursor.fail("a typed generic/X mana amount; colored variable payments require a wider cost program"))?;
        json!({"kind":"payMana","genericAmount":amount})
    } else {
        node.payment
            .fixed_runtime_mana_value()
            .map_err(|error| cursor.fail(error.expected()))?;
        json!({"kind":"payMana","manaCost":node.payment.as_oracle_cost()})
    };
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetSpell",
                    candidates,
                    1,
                    1,
                )],
            },
            "effects": [
                {
                    "kind":"tryPayCost",
                    "id":"resolutionPayment0",
                    "player":{"kind":"controllerOf","object":chosen_target("targetSpell")},
                    "cost":payment,
                },
                {
                    "kind":"conditional",
                    "condition":{"kind":"costWasPaid","decisionId":"resolutionPayment0","value":false,"required":true},
                    "then":[{"kind":"counterStackObject","object":chosen_target("targetSpell")}],
                    "else":[],
                },
            ],
        }),
        &[
            "Recognize one spell target through the shared counter and target children",
            "Bind the payment exception to the target spell's controller",
            "Delegate payment parsing to the shared mana-cost child",
            "Compose a shared cost attempt, its required Boolean result, and an ordinary conditional counter",
            &trace,
        ],
    ))
}
