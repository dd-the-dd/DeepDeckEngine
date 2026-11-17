use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_enchant_family(
    input: &AbilityInput<'_>,
) -> bool {
    input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("enchant"))
}

fn face_is_aura(type_line: &str) -> bool {
    type_line
        .split(|character: char| !character.is_alphabetic())
        .any(|word| word == "Aura")
}

pub(in crate::oracle::canonical) fn parse_promoted_enchant(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("keyword-enchant", input.tokens);
    let enchant = recognize_enchant_keyword(&mut cursor, face_is_aura(input.face_type_line))?;
    let (_, candidates) = lower_target_selector_candidates(&enchant.object)
        .ok_or_else(|| cursor.fail("a lowerable permanent object restriction"))?;
    // A null restriction is only the bare `permanent` head; any other
    // predicate that failed to lower must not become an unrestricted Aura.
    if candidates["where"].is_null() {
        let (TargetSelector::Object(predicate)
        | TargetSelector::ControlledObject { predicate, .. }) = &enchant.object
        else {
            return Err(cursor.fail("a permanent object restriction"));
        };
        cursor.require(
            matches!(
                predicate,
                SelectorPredicate::ObjectKind(ObjectKind::Permanent)
            ),
            "a lowerable permanent object restriction",
        )?;
    }
    // The engine reads exactly `where` and the optional `controller` when it
    // declares the Aura spell's target, attaches it on resolution and applies
    // the state-based action for an illegally attached Aura.
    let mut ability = json!({"kind": "enchant", "where": candidates["where"]});
    if let Some(controller) = candidates.get("controller") {
        ability["controller"] = controller.clone();
    }
    Ok(draft(
        json!({"kind": "keywordAbility", "source": self_ref(), "ability": ability}),
        &[
            "Recognize the Enchant keyword independently of any card name",
            "Delegate the restriction to the shared target-slot object query",
            "Lower to the permanent candidate shape the engine reads for Aura targets",
        ],
    ))
}

#[cfg(test)]
#[path = "enchant_native_tests.rs"]
mod tests;
