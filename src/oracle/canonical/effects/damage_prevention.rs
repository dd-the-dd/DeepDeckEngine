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
    match source {
        // `a SOURCE of your choice` (CR 609.7a): chosen as the shield is
        // created, by the shared resolution choice.
        Some(source) if source["kind"] == "chosenSource" => {
            shield["sourceChoice"] = json!({"where": source["where"].clone()});
        }
        Some(source) => shield["source"] = source,
        None => {}
    }
    // `dealt to and dealt by PARTY`: a second filter over the same party as
    // the damage source (CR 615.1a).
    if node.recipient_also_source {
        shield["recipientAlsoSource"] = json!(true);
    }
    shield
}

/// The shield for the lowered parties in [`PreventDamageEffectNode::parties`]
/// order. A redirected party turns the prevention into a redirection of the
/// same amount, scope and lifetime (CR 614.9); the engine reads `redirectTo`.
pub(in crate::oracle::canonical) fn lower_damage_shield_parties(
    node: &PreventDamageEffectNode,
    amount: Value,
    parties: Vec<Option<Value>>,
) -> Value {
    let mut parties = parties.into_iter();
    let recipient = parties.next().flatten();
    let source = parties.next().flatten();
    let redirect = parties.next().flatten();
    let mut shield = lower_prevent_damage_shield(node, amount, recipient, source);
    if let Some(redirect) = redirect {
        shield["redirectTo"] = redirect;
    }
    shield
}

pub(in crate::oracle::canonical) fn lower_prevention_amount(
    node: &PreventDamageEffectNode,
    declarations: TargetlessDeclarationSupport,
) -> Result<Value, TargetlessLoweringError> {
    match &node.amount {
        PreventedDamageAmount::All => Ok(json!({"kind": "all"})),
        // Only a created next-damage rule reads this placeholder; the engine
        // never accepts it as an amount (CR 615.7).
        PreventedDamageAmount::EventDamage => Ok(json!({"kind": "eventDamage"})),
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
        // The choice is made on resolution; `lower_prevent_damage_shield`
        // turns this placeholder into the shield's `sourceChoice`.
        DamagePartyNode::ChosenSource(source) => {
            let cursor = OracleTokenCursor::new("prevention.chosen-source.lowering", &[]);
            let filter = lower_chosen_damage_source_filter(&cursor, source)
                .map_err(|_| TargetlessLoweringError::UnsupportedEffect)?;
            Ok(json!({"kind": "chosenSource", "where": filter}))
        }
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
    let redirect = node
        .redirect
        .as_ref()
        .map(lower_nontargeted_damage_party)
        .transpose()?;
    let amount = lower_prevention_amount(node, declarations)?;
    Ok(lower_damage_shield_parties(
        node,
        amount,
        vec![recipient, source, redirect],
    ))
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
