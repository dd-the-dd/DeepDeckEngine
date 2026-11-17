use super::super::*;

pub(in crate::oracle::canonical) fn lower_explicit_token_creation(
    node: &ExplicitTokenCreationNode,
    declarations: TargetlessDeclarationSupport,
) -> Result<Value, TargetlessLoweringError> {
    let player = match node.player {
        LocalPlayerReference::Controller => controller(),
        _ => return Err(TargetlessLoweringError::RequiresTarget),
    };
    let quantity = lower_targetless_numeric_expression(&node.quantity, declarations)?;
    let base = &node.template.creature;
    let mut effect = json!({"kind":"createTokens","controller":player,"quantity":quantity,"token":{
        "power":base.power,"toughness":base.toughness,
        "colors":base.colors.iter().map(|color|canonical_color_name(*color)).collect::<Vec<_>>(),
        "types":node.template.types.iter().map(|kind|kind.ir_name()).collect::<Vec<_>>(),
        "subtypes":base.subtypes.iter().map(|kind|kind.ir_name()).collect::<Vec<_>>(),
        "abilities":node.keywords.iter().map(|keyword|json!({"kind":keyword.canonical_name()}))
            .chain(node.abilities.iter().map(|ability| ability.rule.clone())).collect::<Vec<_>>(),
    }});
    if node.tapped {
        effect["tapped"] = json!(true);
    }
    if let Some(name) = &node.name {
        effect["token"]["name"] = json!(name);
    }
    if node.legendary {
        effect["token"]["supertypes"] = json!(["Legendary"]);
    }
    Ok(effect)
}
