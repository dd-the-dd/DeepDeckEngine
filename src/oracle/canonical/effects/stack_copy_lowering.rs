//! Lowering of one copy-on-the-stack instruction (CR 707.10) once its copied
//! object is known: a declared target, the spell a cast record binds, or the
//! resolving spell itself.
use super::super::*;

/// The copy actions of one instruction. A literal count is that many
/// ordinary copies; a variable count (`X times`, `for each COUNTABLE`) is
/// one action whose `times` the runtime evaluates as it resolves.
pub(in crate::oracle::canonical) fn lower_stack_copy_actions(
    copy: &CopyStackObjectEffectNode,
    object: Value,
    controller: Value,
    except: Option<&Value>,
) -> Result<Vec<Value>, &'static str> {
    let mut action = if copy.resolving_spell {
        // CR 707.10: a copy of the spell that is resolving, put onto the
        // stack under the copying player's control.
        json!({
            "kind": "copyResolvingStackObject",
            "controller": controller,
            "mayChooseNewTargets": copy.may_choose_new_targets,
        })
    } else {
        json!({
            "kind": "copyStackItem",
            "object": object,
            "controller": controller,
            "mayChooseNewTargets": copy.may_choose_new_targets,
        })
    };
    if let Some(except) = except {
        if copy.resolving_spell {
            return Err("a copy of the resolving spell without copiable-value exceptions");
        }
        action["except"] = except.clone();
    }
    match &copy.variable_count {
        None => {
            let count = usize::try_from(copy.count)
                .ok()
                .filter(|count| *count >= 1)
                .ok_or("a positive literal number of copies")?;
            Ok(vec![action; count])
        }
        Some(count) => {
            action["times"] = lower_numeric_expression(count)
                .ok_or("a number of copies the shared numeric lowering represents")?;
            Ok(vec![action])
        }
    }
}
