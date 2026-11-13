use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ObjectControllerScope, ObjectSetNode, RiotKeywordNode, recognize_controlled_object_set,
    recognize_riot_keyword,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct StaticRiotGrantNode {
    pub objects: ObjectSetNode,
    pub keyword: RiotKeywordNode,
}

/// Recognizes a persistent Riot grant to a controller-scoped permanent set.
/// Selector, controller relation, keyword and reminder remain independent
/// typed children.
pub(in crate::oracle::canonical) fn recognize_static_riot_grant(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StaticRiotGrantNode> {
    cursor.enter("static-keyword-grant.objects");
    let objects = recognize_controlled_object_set(cursor)?;
    cursor.require(
        objects.controller == ObjectControllerScope::AbilityController,
        "a permanent set controlled by the ability controller",
    )?;
    cursor.enter("static-keyword-grant.action");
    let action = cursor
        .peek()
        .filter(|raw| raw.eq_ignore_ascii_case("have") || raw.eq_ignore_ascii_case("has"))
        .ok_or_else(|| cursor.fail("`have` or `has`"))?
        .to_string();
    cursor.consume(&action)?;
    cursor.enter("static-keyword-grant.keyword");
    let keyword = recognize_riot_keyword(cursor)?;
    cursor.enter("static-keyword-grant.complete");
    cursor.finish()?;
    Ok(StaticRiotGrantNode { objects, keyword })
}

#[cfg(test)]
#[path = "static_keyword_grant_tests.rs"]
mod tests;
