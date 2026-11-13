use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_attached_keyword_grant_family(
    input: &AbilityInput<'_>,
) -> bool {
    matches!(
        input.tokens,
        [relation, object_type, predicate, ..]
            if relation.raw.eq_ignore_ascii_case("equipped")
                && object_type.raw.eq_ignore_ascii_case("creature")
                && predicate.raw.eq_ignore_ascii_case("has")
    )
}

pub(in crate::oracle::canonical) fn parse_promoted_attached_keyword_grant(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("attached-keyword-grant", input.tokens);
    let grant = recognize_attached_keyword_grant(&mut cursor)?;
    cursor.require(
        grant.relation == AttachmentRelationNode::Equipped
            && grant.object_type == ObjectCardType::Creature,
        "an equipped creature relation",
    )?;
    let objects = json!({
        "kind": "attachedPermanent",
        "attachment": self_ref(),
    });
    let modifiers = grant
        .keywords
        .into_iter()
        .map(|keyword| {
            json!({
                "kind": "grantKeyword",
                "objects": objects.clone(),
                "keyword": keyword.canonical_name(),
            })
        })
        .collect::<Vec<_>>();
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": modifiers,
        }),
        &[
            "Recognize an equipped-object relation independently of the source name",
            "Recognize every keyword through the shared parameterless-keyword list",
            "Validate supported reminder clauses against their granted keywords",
            "Lower each keyword to one independent grantKeyword modifier",
            &trace,
        ],
    ))
}
