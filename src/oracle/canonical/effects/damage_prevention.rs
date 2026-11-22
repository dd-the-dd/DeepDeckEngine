//! Lowers `prevent AMOUNT [combat] damage ...` (CR 615) to the engine's
//! `preventDamage` shield. The shield names an amount (`all` or a numeric
//! expression), an optional recipient, an optional source and the turn it
//! lasts; the engine reads each of these fields.
use super::super::*;

/// The shield for already lowered parties.
pub(in crate::oracle::canonical) fn lower_prevent_damage_shield(
    node: &PreventDamageEffectNode,
    amount: Value,
    recipient: Option<Value>,
    source: Option<Value>,
) -> Value {
    let mut shield = json!({
        "kind": "preventDamage",
        "amount": amount,
        "combatOnly": node.combat_only,
        "duration": {"kind": "untilEndOfCurrentTurn"},
    });
    if let Some(recipient) = recipient {
        shield["recipient"] = recipient;
    }
    if let Some(source) = source {
        shield["source"] = source;
    }
    shield
}

pub(in crate::oracle::canonical) fn lower_prevention_amount(
    node: &PreventDamageEffectNode,
    declarations: TargetlessDeclarationSupport,
) -> Result<Value, TargetlessLoweringError> {
    match &node.amount {
        PreventedDamageAmount::All => Ok(json!({"kind": "all"})),
        PreventedDamageAmount::Next(atom) => lower_targetless_numeric_expression(
            &NumericExpression::Atom(atom.clone()),
            declarations,
        ),
    }
}

/// A party that needs no target declaration or earlier binding.
pub(in crate::oracle::canonical) fn lower_nontargeted_damage_party(
    party: &DamagePartyNode,
) -> Result<Value, TargetlessLoweringError> {
    match party {
        DamagePartyNode::Player(LocalPlayerReference::Controller) => Ok(controller()),
        DamagePartyNode::Player(
            LocalPlayerReference::ControllerOf(_)
            | LocalPlayerReference::ControllerOfQualified(_)
            | LocalPlayerReference::Bound(_),
        )
        | DamagePartyNode::Targets(_) => Err(TargetlessLoweringError::RequiresTarget),
        DamagePartyNode::Object(operand) => lower_nontargeted_permanent_operand(operand),
    }
}

pub(in crate::oracle::canonical) fn lower_targetless_prevent_damage(
    node: &PreventDamageEffectNode,
    declarations: TargetlessDeclarationSupport,
) -> Result<Value, TargetlessLoweringError> {
    let recipient = node
        .recipient
        .as_ref()
        .map(lower_nontargeted_damage_party)
        .transpose()?;
    let source = node
        .source
        .as_ref()
        .map(lower_nontargeted_damage_party)
        .transpose()?;
    let amount = lower_prevention_amount(node, declarations)?;
    Ok(lower_prevent_damage_shield(node, amount, recipient, source))
}

/// The single target slot of a party, if it declares one.
pub(in crate::oracle::canonical) fn damage_party_targets(
    party: &Option<DamagePartyNode>,
) -> Option<&TargetListNode> {
    match party {
        Some(DamagePartyNode::Targets(targets)) => Some(targets),
        _ => None,
    }
}
