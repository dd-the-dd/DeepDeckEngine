use super::super::*;

pub(in crate::oracle::canonical) fn lower_sacrifice_effect(
    effect: &SacrificeEffectNode,
    declarations: TargetlessDeclarationSupport,
) -> Result<Value, TargetlessLoweringError> {
    match effect {
        SacrificeEffectNode::Source(effect) => Ok(json!({
            "kind":"sacrificePermanent",
            "permanent": match effect.source { LocalObjectReference::SourceSelf => self_ref() },
        })),
        SacrificeEffectNode::Selected { object, count } => {
            if !domain_filter_is_representable(&object.predicate) {
                return Err(TargetlessLoweringError::UnsupportedCollectionPredicate);
            }
            validate_object_predicate_numeric_expressions(&object.predicate, declarations)?;
            let mut sacrifice = json!({
                "kind":"sacrificePermanents", "player":controller(),
                "where":lower_permanent_object_predicate(&object.predicate),
                "count":lower_targetless_numeric_expression(count, declarations)?,
            });
            if object.exclude_source {
                sacrifice["excludeSource"] = json!(true);
            }
            Ok(sacrifice)
        }
    }
}
