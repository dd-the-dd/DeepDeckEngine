use super::super::*;

pub(in crate::oracle::canonical) fn lower_destroy_permanent_collection_effect(
    effect: &DestroyPermanentCollectionEffectNode,
) -> Value {
    let mut permanent = json!({
        "kind": "eachPermanent",
        "where": lower_permanent_object_predicate(&effect.objects.predicate),
    });
    if let Some(controller_scope) = effect.objects.controller {
        permanent["player"] = lower_object_controller_scope(controller_scope);
    }
    json!({
        "kind": "destroyPermanent",
        "permanent": permanent,
    })
}

#[cfg(test)]
#[path = "collection_destroy_tests.rs"]
mod tests;
