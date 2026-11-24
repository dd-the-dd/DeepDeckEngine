//! Lowering of the shared token-creation body (CR 111.1): a written
//! description (`createTokens` with its characteristics, power and toughness
//! as shared numeric expressions) or a copy (`createTokens` whose token is
//! `copyOf` an object, with its copiable-value exceptions, CR 707.9b). Both
//! forms are the same primitive; the runtime binds the created tokens for a
//! following action that names them.
use super::super::*;

/// One `createTokens` per written description of the action, in printed
/// order: a coordinated list (`a 1/1 ... token, a 2/2 ... token, and ...`)
/// is several ordinary creations by the same player.
pub(in crate::oracle::canonical) fn lower_explicit_token_creations(
    node: &ExplicitTokenCreationNode,
    declarations: TargetlessDeclarationSupport,
) -> Result<Vec<Value>, TargetlessLoweringError> {
    let mut effects = vec![lower_explicit_token_creation(node, declarations)?];
    for body in &node.additional {
        effects.push(lower_explicit_token_creation(
            &ExplicitTokenCreationNode {
                player: node.player,
                body: body.clone(),
                additional: Vec::new(),
            },
            declarations,
        )?);
    }
    Ok(effects)
}

pub(in crate::oracle::canonical) fn lower_explicit_token_creation(
    node: &ExplicitTokenCreationNode,
    declarations: TargetlessDeclarationSupport,
) -> Result<Value, TargetlessLoweringError> {
    let player = match node.player {
        LocalPlayerReference::Controller => controller(),
        _ => return Err(TargetlessLoweringError::RequiresTarget),
    };
    let quantity = lower_targetless_numeric_expression(&node.body.quantity, declarations)?;
    match &node.body.spec {
        TokenSpecNode::Described(template) => {
            // `X/X`: X needs the same declaration as a quantity X.
            if let Some(statistics) = &template.statistics {
                lower_targetless_numeric_expression(&statistics.power, declarations)?;
                lower_targetless_numeric_expression(&statistics.toughness, declarations)?;
            }
            lower_token_creation_body(&node.body, player, quantity)
                .map_err(|_| TargetlessLoweringError::UnsupportedEffect)
        }
        TokenSpecNode::CopyOf(copy) => {
            let object = match &copy.object {
                PermanentOperandNode::BoundReference(reference) => {
                    bound_permanent_reference_value(*reference, &[])
                        .ok_or(TargetlessLoweringError::RequiresTarget)?
                }
                operand => lower_nontargeted_permanent_operand(operand)?,
            };
            lower_token_copy_creation(&node.body, copy, object, player, quantity)
                .map_err(|_| TargetlessLoweringError::UnsupportedEffect)
        }
    }
}

/// `createTokens` for the player the parent lowered (the controller of an
/// imperative, or a player subject's typed reference).
pub(in crate::oracle::canonical) fn lower_token_creation_body(
    node: &TokenCreationBodyNode,
    player: Value,
    quantity: Value,
) -> Result<Value, &'static str> {
    let TokenSpecNode::Described(template) = &node.spec else {
        return Err("a copied object lowered by the program that declares it");
    };
    let base = &template.creature;
    let stat = |expression: &NumericExpression| {
        lower_numeric_expression(expression)
            .ok_or("a token power/toughness the shared numeric lowering represents")
    };
    let mut effect = json!({"kind":"createTokens","controller":player,"quantity":quantity,"token":{
        "colors":base.colors.iter().map(|color|canonical_color_name(*color)).collect::<Vec<_>>(),
        "types":template.types.iter().map(|kind|kind.ir_name()).collect::<Vec<_>>(),
        "subtypes":template.subtypes,
        "abilities":node.keywords.iter().map(|keyword|json!({"kind":keyword.canonical_name()}))
            .chain(node.abilities.iter().map(|ability| ability.rule.clone())).collect::<Vec<_>>(),
    }});
    // A description without stated power and toughness (a noncreature
    // token, or a creature defined by its own ability, CR 604.3) has none.
    if let Some(statistics) = &template.statistics {
        effect["token"]["power"] = stat(&statistics.power)?;
        effect["token"]["toughness"] = stat(&statistics.toughness)?;
    }
    lower_entry_state(&mut effect, node);
    if let Some(name) = &node.name {
        effect["token"]["name"] = json!(name);
    }
    if node.legendary {
        effect["token"]["supertypes"] = json!(["Legendary"]);
    }
    Ok(effect)
}

/// The battlefield entry state of the created tokens (CR 111.1, 508.4).
fn lower_entry_state(effect: &mut Value, node: &TokenCreationBodyNode) {
    if node.tapped {
        effect["tapped"] = json!(true);
    }
    if node.attacking {
        effect["attacking"] = json!(true);
    }
}

/// `createTokens` whose token is a copy of `object` (CR 707.2, 707.9b): the
/// exceptions are the characteristic-change operations of the shared frame,
/// without object or duration, plus the removal of `Legendary`.
pub(in crate::oracle::canonical) fn lower_token_copy_creation(
    node: &TokenCreationBodyNode,
    copy: &TokenCopyNode,
    object: Value,
    player: Value,
    quantity: Value,
) -> Result<Value, &'static str> {
    let mut except = match &copy.exceptions.changes {
        Some(changes) => lower_copiable_characteristic_changes(changes)?,
        None => Vec::new(),
    };
    if copy.exceptions.not_legendary {
        except.push(json!({ "kind": "removeSupertype", "supertype": "Legendary" }));
    }
    let mut effect = json!({
        "kind": "createTokens",
        "controller": player,
        "quantity": quantity,
        "token": { "kind": "copyOf", "object": object, "except": except },
    });
    lower_entry_state(&mut effect, node);
    Ok(effect)
}
