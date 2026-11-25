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
    // The copied object or the object the tokens enter attached to: a
    // bound or contextual object here; a target is declared by the program.
    let object = match node.body.object_operand() {
        Some(PermanentOperandNode::BoundReference(reference)) => Some(
            bound_permanent_reference_value(*reference, &[])
                .ok_or(TargetlessLoweringError::RequiresTarget)?,
        ),
        Some(operand) => Some(lower_nontargeted_permanent_operand(operand)?),
        None => None,
    };
    if let TokenSpecNode::Described(template) = &node.body.spec
        && let Some(statistics) = &template.statistics
    {
        // `X/X`: X needs the same declaration as a quantity X.
        lower_targetless_numeric_expression(&statistics.power, declarations)?;
        lower_targetless_numeric_expression(&statistics.toughness, declarations)?;
    }
    lower_token_creation_with_object(&node.body, object, player, quantity)
        .map_err(|_| TargetlessLoweringError::UnsupportedEffect)
}

/// `createTokens` once the program lowered the object the body reads (the
/// copied object, or the object an Aura token enters attached to).
pub(in crate::oracle::canonical) fn lower_token_creation_with_object(
    node: &TokenCreationBodyNode,
    object: Option<Value>,
    player: Value,
    quantity: Value,
) -> Result<Value, &'static str> {
    match (&node.spec, object) {
        (TokenSpecNode::CopyOf(copy), Some(object)) => {
            lower_token_copy_creation(node, copy, object, player, quantity)
        }
        (TokenSpecNode::CopyOf(_), None) => Err("the copied object of a token copy"),
        (_, object) => {
            let mut effect = lower_token_creation_body(node, player, quantity)?;
            match (node.attached_to.is_some(), object) {
                (true, Some(object)) => effect["attachTo"] = object,
                (false, None) => {}
                _ => return Err("the object an Aura token enters attached to"),
            }
            Ok(effect)
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
    if !attack_defender_lowered(node) {
        return Err("an attacked player the triggering event records");
    }
    let template = match &node.spec {
        TokenSpecNode::Described(template) => template,
        TokenSpecNode::CopyOf(_) => {
            return Err("a copied object lowered by the program that declares it");
        }
        // CR 111.10: a predefined token is its catalog identity.
        TokenSpecNode::Predefined(name) => {
            let mut effect = json!({
                "kind": "createTokens",
                "controller": player,
                "quantity": quantity,
                "token": { "kind": "namedToken", "name": name },
            });
            lower_entry_state(&mut effect, node);
            return Ok(effect);
        }
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
    // The one defender the tokens attack (CR 508.4), by the shared
    // attacking-entry policy; an unlowerable player stays unrecorded and is
    // refused by `attack_defender_lowered`.
    if let Some(policy) = node
        .attack_defender
        .as_ref()
        .and_then(super::lower_entering_attack_policy)
    {
        effect["attackingDefender"] = policy;
    }
}

/// Whether the tokens' stated defender has a lowering (a player the event
/// records); otherwise the creation fails explicitly.
fn attack_defender_lowered(node: &TokenCreationBodyNode) -> bool {
    node.attack_defender
        .as_ref()
        .is_none_or(|defender| super::lower_entering_attack_policy(defender).is_some())
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
    if !attack_defender_lowered(node) {
        return Err("an attacked player the triggering event records");
    }
    let mut except = match &copy.exceptions.changes {
        Some(changes) => lower_copiable_characteristic_changes(changes)?,
        None => Vec::new(),
    };
    if copy.exceptions.not_legendary {
        except.push(json!({ "kind": "removeSupertype", "supertype": "Legendary" }));
    }
    if copy.exceptions.no_mana_cost {
        except.push(json!({ "kind": "removeManaCost" }));
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
