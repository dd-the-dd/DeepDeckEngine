//! Lowering of an action operand chosen while the action resolves (CR
//! 608.2d): `put a +1/+1 counter on a creature you control`.
//!
//! The operand is the shared resolution choice (`ObjectChoiceNode`), lowered
//! by the one choice writer `choose a creature you control` uses
//! (`resolution_object_choice::lower_object_choice`): a `choosePermanents`
//! decision of the acting player and a `bind` of its result. The program
//! lowering puts both right before the action's own primitives and binds the
//! action's key to that result, so the action's operand and a later `it` /
//! `that creature` / `those creatures` read the same chosen objects. A
//! performer (`each player may put ... on a creature they control`) wraps
//! the choice with the action, so each player chooses among their own.
use super::super::*;

/// The lowered resolution choice of one action's chosen operand.
pub(in crate::oracle::canonical) struct LoweredChosenOperand {
    /// `choosePermanents` and the `bind` of its result.
    pub choice: Vec<Value>,
    /// The binding that holds the chosen objects.
    pub binding: String,
}

/// Every permanent operand of an action, in written order.
fn action_permanent_operands(effect: &LocalEffectNode) -> Vec<&PermanentOperandNode> {
    match effect {
        LocalEffectNode::PutCounters(node) => vec![&node.permanent],
        LocalEffectNode::MoveCounters(node) => {
            vec![&node.removal.permanent, &node.destination]
        }
        LocalEffectNode::TemporaryKeywordGrant(node) => vec![&node.objects],
        LocalEffectNode::ModifyStats(node) => vec![&node.targets],
        LocalEffectNode::BecomeTypes(node) => vec![&node.objects],
        LocalEffectNode::TapPermanents(node) => vec![&node.targets],
        LocalEffectNode::UntapRestriction(node) => vec![&node.objects],
        LocalEffectNode::PermanentKeywordAction(node) => vec![&node.targets],
        LocalEffectNode::ControlChange(node) => vec![&node.objects],
        LocalEffectNode::Attach(node) => vec![&node.attachment, &node.recipient],
        LocalEffectNode::TransformPermanent(node) => vec![&node.permanent],
        // Damage and fight declare their own slots outside these operands;
        // a chosen operand there has no free key and stays unlowered.
        _ => Vec::new(),
    }
}

/// The resolution choice of the action at `effect_index`, lowered, when one
/// of its operands is chosen on resolution. The choice shares the action's
/// own binding key, so an action has at most one chosen operand and declares
/// no permanent target beside it; anything else fails explicitly.
pub(in crate::oracle::canonical) fn lower_action_chosen_operand(
    effect: &LocalEffectNode,
    effect_index: usize,
    declarations: TargetlessDeclarationSupport,
) -> Result<Option<LoweredChosenOperand>, &'static str> {
    let operands = action_permanent_operands(effect);
    let mut choices = operands.iter().filter_map(|operand| match operand {
        PermanentOperandNode::Chosen(choice) => Some(choice),
        _ => None,
    });
    let Some(choice) = choices.next() else {
        return Ok(None);
    };
    if choices.next().is_some() {
        return Err("one operand chosen on resolution per action");
    }
    if operands
        .iter()
        .any(|operand| matches!(operand, PermanentOperandNode::Targets(_)))
    {
        return Err(
            "an action whose chosen operand is its only bound object; a declared target beside it shares its key",
        );
    }
    if choice.remainder.is_some() {
        return Err("a chosen operand without an action on the unchosen permanents");
    }
    let lowered =
        super::resolution_object_choice::lower_object_choice(choice, effect_index, declarations)?;
    let binding = lowered
        .chosen
        .ok_or("the chosen objects' binding the action reads")?;
    if !lowered.remainder.is_empty() {
        return Err("a chosen operand without an action on the unchosen permanents");
    }
    Ok(Some(LoweredChosenOperand {
        choice: lowered.choice,
        binding,
    }))
}

#[cfg(test)]
#[path = "chosen_operand_tests.rs"]
mod tests;
