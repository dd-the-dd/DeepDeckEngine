use super::super::*;

/// Describes which declaration-backed values are already available to a
/// caller lowering an otherwise targetless local effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TargetlessDeclarationSupport {
    None,
    XValue,
}

/// A targetless effect can be structurally recognized but still be illegal in
/// a parent that cannot collect a target or declaration. Keeping those cases
/// distinct lets the parent emit a committed machine diagnostic instead of
/// silently dropping or approximating the effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TargetlessLoweringError {
    RequiresTarget,
    RequiresDeclaration,
    UnsupportedEffect,
    RuntimeIntegerRange,
    RuntimeArithmeticRange,
    InvalidNumericArithmetic,
    UnsupportedPermissionAmount,
    UnsupportedPermissionLifetime,
    UnsupportedCollectionController,
    UnsupportedCollectionPredicate,
}

impl TargetlessLoweringError {
    pub(in crate::oracle::canonical) fn expectation(self) -> &'static str {
        match self {
            Self::RequiresTarget => "a targetless effect requiring no target declaration",
            Self::RequiresDeclaration => {
                "a targetless effect requiring no unavailable value declaration"
            }
            Self::UnsupportedEffect => "a supported atomic targetless effect",
            Self::RuntimeIntegerRange => "a runtime numeric literal representable as i32",
            Self::RuntimeArithmeticRange => {
                "constant arithmetic with every intermediate result representable as i32"
            }
            Self::InvalidNumericArithmetic => "a nonzero constant divisor for numeric arithmetic",
            Self::UnsupportedPermissionAmount => "a positive i32 literal additional-land allowance",
            Self::UnsupportedPermissionLifetime => {
                "a temporary land allowance ending with the current turn"
            }
            Self::UnsupportedCollectionController => {
                "a permanent collection with one fixed controller or no player restriction"
            }
            Self::UnsupportedCollectionPredicate => {
                "a permanent collection predicate executable without declaration-dependent filtering"
            }
        }
    }
}

fn lower_atom(
    atom: &NumericAtom,
    declarations: TargetlessDeclarationSupport,
) -> Result<Value, TargetlessLoweringError> {
    if let NumericAtom::Integer(value) = atom
        && i32::try_from(*value).is_err()
    {
        return Err(TargetlessLoweringError::RuntimeIntegerRange);
    }
    if atom == &NumericAtom::VariableX && declarations != TargetlessDeclarationSupport::XValue {
        return Err(TargetlessLoweringError::RequiresDeclaration);
    }
    Ok(lower_numeric_atom(atom.clone()))
}

#[path = "targetless_numeric_validation.rs"]
mod numeric_validation;
pub(in crate::oracle::canonical) use numeric_validation::validate as validate_targetless_numeric_expression;

pub(in crate::oracle::canonical) fn lower_targetless_numeric_expression(
    expression: &NumericExpression,
    declarations: TargetlessDeclarationSupport,
) -> Result<Value, TargetlessLoweringError> {
    validate_targetless_numeric_expression(expression, declarations)?;
    match expression {
        NumericExpression::Atom(atom) => lower_atom(atom, declarations),
        NumericExpression::CountQuery(_) => Err(TargetlessLoweringError::UnsupportedEffect),
        NumericExpression::PowerOf(_)
        | NumericExpression::ManaValueOf(_)
        | NumericExpression::QualifiedTargetProperty { .. } => {
            Err(TargetlessLoweringError::RequiresTarget)
        }
        NumericExpression::ObjectProperty { .. }
        | NumericExpression::CountCounters { .. }
        | NumericExpression::PermanentCount(_)
        | NumericExpression::SourceCastX
        | NumericExpression::EventValue(_)
        | NumericExpression::CountDistinctCardTypes { .. }
        | NumericExpression::Add { .. }
        | NumericExpression::Subtract { .. }
        | NumericExpression::Multiply { .. }
        | NumericExpression::PlayerFact { .. }
        | NumericExpression::SacrificedCostObjectProperty(_)
        | NumericExpression::GreatestPropertyAmong { .. }
        | NumericExpression::CardsInZone { .. }
        | NumericExpression::Divide { .. } => {
            lower_numeric_expression(expression).ok_or(TargetlessLoweringError::UnsupportedEffect)
        }
    }
}

/// Expands procedural actions and retains atomic effects as one-element lists.
/// The caller's scope isolates transient bindings between sibling actions.
pub(in crate::oracle::canonical) fn lower_targetless_local_effects(
    effect: &LocalEffectNode,
    declarations: TargetlessDeclarationSupport,
    scope: Option<&str>,
) -> Result<Vec<Value>, TargetlessLoweringError> {
    if let LocalEffectNode::CreateExplicitTokens(node) = effect {
        return lower_explicit_token_creation(node, declarations).map(|effect| vec![effect]);
    }
    if let LocalEffectNode::DiscardCards(action) = effect {
        if action.player != PlayerEffectSubject::Controller {
            return Err(TargetlessLoweringError::RequiresTarget);
        }
        // Contextual numeric validation precedes the shared discard lowering;
        // a variable use cannot manufacture its own producer.
        validate_targetless_numeric_expression(&action.count, declarations)?;
        let random_selection = scope.map_or_else(
            || "discardSelection".to_string(),
            |scope| format!("{scope}:discardSelection"),
        );
        return lower_player_discard_action_for_player(
            action,
            PlayerDiscardLoweringIds {
                target_player: "unusedControllerDiscardTarget",
                random_selection: &random_selection,
            },
            controller(),
        )
        .map_err(|_| TargetlessLoweringError::UnsupportedEffect);
    }
    if let LocalEffectNode::QuantifiedPlayerAction(node) = effect {
        return lower_quantified_player_actions(node, declarations, scope);
    }
    if let LocalEffectNode::SelectedPermanents(untap) = effect {
        return lower_selected_permanents_effects(untap, declarations, scope);
    }
    if let LocalEffectNode::Amass(amass) = effect {
        return lower_amass_effects(amass, declarations, scope);
    }
    if let LocalEffectNode::EmpowerJace(empower) = effect {
        return lower_empower_jace_effects(empower, declarations, scope);
    }
    if let LocalEffectNode::Investigate(investigate) = effect {
        return lower_investigate(investigate)
            .map(|effect| vec![effect])
            .ok_or(TargetlessLoweringError::RequiresTarget);
    }
    if let LocalEffectNode::PermanentKeywordAction(keyword_action) = effect {
        let quantity = keyword_action
            .quantity
            .as_ref()
            .map(|quantity| lower_targetless_numeric_expression(quantity, declarations))
            .transpose()?;
        let object = lower_nontargeted_permanent_operand(&keyword_action.targets)?;
        let scope = scope.map_or_else(
            || keyword_action.action.name().to_string(),
            |scope| format!("{scope}:{}", keyword_action.action.name()),
        );
        return lower_permanent_keyword_action(keyword_action.action, object, quantity, &scope)
            .map_err(|_| TargetlessLoweringError::UnsupportedEffect);
    }
    if let LocalEffectNode::ControllerKeywordAction(keyword_action) = effect {
        let quantity = keyword_action
            .quantity
            .as_ref()
            .map(|quantity| lower_targetless_numeric_expression(quantity, declarations))
            .transpose()?;
        let scope = scope.unwrap_or("controllerKeywordAction");
        return lower_controller_keyword_action(keyword_action.action, quantity, scope)
            .map_err(|_| TargetlessLoweringError::UnsupportedEffect);
    }
    let partition = match effect {
        LocalEffectNode::Scry(node) => {
            Some((LibraryPartitionAction::Scry, &node.player, &node.count))
        }
        LocalEffectNode::Surveil(node) => {
            Some((LibraryPartitionAction::Surveil, &node.player, &node.count))
        }
        _ => None,
    };
    if let Some((action, recipient, quantity)) = partition {
        let player = match recipient {
            LocalPlayerReference::Controller => controller(),
            LocalPlayerReference::ControllerOf(_)
            | LocalPlayerReference::ControllerOfQualified(_) => {
                return Err(TargetlessLoweringError::RequiresTarget);
            }
        };
        let count = lower_targetless_numeric_expression(quantity, declarations)?;
        return lower_library_partition_effects_for_player(action, quantity, count, player, scope)
            .map_err(|_| TargetlessLoweringError::UnsupportedEffect);
    }
    lower_targetless_local_effect(effect, declarations).map(|effect| vec![effect])
}

/// Lowers atomic local effects whose recipient is fixed by the ability's
/// controller. Target selection and declaration collection intentionally stay
/// outside this boundary.
pub(in crate::oracle::canonical) fn lower_targetless_local_effect(
    effect: &LocalEffectNode,
    declarations: TargetlessDeclarationSupport,
) -> Result<Value, TargetlessLoweringError> {
    match effect {
        LocalEffectNode::CardAction(_) | LocalEffectNode::OptionalSequence(_) => {
            Err(TargetlessLoweringError::UnsupportedEffect)
        }
        LocalEffectNode::QuantifiedPlayerAction(node) => {
            lower_quantified_player_action(node, declarations)
        }
        LocalEffectNode::AddMana(effect) => Ok(lower_add_mana_effect(effect)),
        LocalEffectNode::TemporaryPlayerPermission(node) => lower_temporary_player_permission(node),
        LocalEffectNode::DealDamage(effect) => {
            let recipient = match &effect.recipient {
                DamageRecipientNode::Player(LocalPlayerReference::Controller) => controller(),
                DamageRecipientNode::PlayerSet(players) => lower_player_set_reference(*players),
                DamageRecipientNode::Targets(_)
                | DamageRecipientNode::Player(
                    LocalPlayerReference::ControllerOf(_)
                    | LocalPlayerReference::ControllerOfQualified(_),
                ) => {
                    return Err(TargetlessLoweringError::RequiresTarget);
                }
                DamageRecipientNode::EachPermanent(_) => {
                    return Err(TargetlessLoweringError::UnsupportedEffect);
                }
            };
            Ok(json!({
                "kind": "dealDamage",
                "source": match &effect.source {
                    LocalObjectReference::SourceSelf => self_ref(),
                },
                "amount": lower_targetless_numeric_expression(&effect.amount, declarations)?,
                "recipient": recipient,
            }))
        }
        LocalEffectNode::GainLife(effect) => {
            let player = match effect.player {
                LocalPlayerReference::Controller => controller(),
                LocalPlayerReference::ControllerOf(_)
                | LocalPlayerReference::ControllerOfQualified(_) => {
                    return Err(TargetlessLoweringError::RequiresTarget);
                }
            };
            Ok(json!({
                "kind": "gainLife",
                "player": player,
                "amount": lower_targetless_numeric_expression(&effect.amount, declarations)?,
            }))
        }
        LocalEffectNode::LoseLife(effect) => {
            let player = match effect.player {
                LocalPlayerReference::Controller => controller(),
                LocalPlayerReference::ControllerOf(_)
                | LocalPlayerReference::ControllerOfQualified(_) => {
                    return Err(TargetlessLoweringError::RequiresTarget);
                }
            };
            Ok(json!({
                "kind": "loseLife",
                "player": player,
                "amount": lower_targetless_numeric_expression(&effect.amount, declarations)?,
            }))
        }
        LocalEffectNode::DrawCards(effect) => {
            let player = match effect.player {
                LocalPlayerReference::Controller => controller(),
                LocalPlayerReference::ControllerOf(_)
                | LocalPlayerReference::ControllerOfQualified(_) => {
                    return Err(TargetlessLoweringError::RequiresTarget);
                }
            };
            Ok(json!({
                "kind": "drawCards",
                "player": player,
                "count": lower_targetless_numeric_expression(&effect.count, declarations)?,
            }))
        }
        LocalEffectNode::OptionalDrawCards(_) => Err(TargetlessLoweringError::RequiresDeclaration),
        LocalEffectNode::Amass(_) | LocalEffectNode::EmpowerJace(_) => {
            Err(TargetlessLoweringError::UnsupportedEffect)
        }
        LocalEffectNode::DiscardCards(_)
        | LocalEffectNode::SelectedPermanents(_)
        | LocalEffectNode::Scry(_)
        | LocalEffectNode::Surveil(_) => {
            // A library procedure is a vector of primitives. Scalar callers
            // must opt into the shared plural adapter rather than emit a
            // high-level engine operation or silently wrap a compound effect.
            Err(TargetlessLoweringError::UnsupportedEffect)
        }
        LocalEffectNode::TransformPermanent(effect) => Ok(json!({
            "kind": "transformPermanent",
            "object": match &effect.permanent {
                LocalObjectReference::SourceSelf => self_ref(),
            },
        })),
        LocalEffectNode::TapPermanents(effect) => Ok(lower_tap_state_change(
            effect.untap,
            lower_nontargeted_permanent_operand(&effect.targets)?,
        )),
        LocalEffectNode::PreventDamage(effect) => {
            lower_targetless_prevent_damage(effect, declarations)
        }
        // A procedure is a vector of primitives; scalar callers use the
        // plural adapter above. A single designation stays scalar.
        LocalEffectNode::PermanentKeywordAction(effect) => {
            match lower_permanent_keyword_action(
                effect.action,
                lower_nontargeted_permanent_operand(&effect.targets)?,
                None,
                effect.action.name(),
            )
            .map_err(|_| TargetlessLoweringError::UnsupportedEffect)?
            .as_slice()
            {
                [single] if effect.quantity.is_none() => Ok(single.clone()),
                _ => Err(TargetlessLoweringError::UnsupportedEffect),
            }
        }
        LocalEffectNode::ControllerKeywordAction(_) => {
            Err(TargetlessLoweringError::UnsupportedEffect)
        }
        LocalEffectNode::PutCounters(effect) => Ok(json!({
            "kind": "putCounters",
            "permanent": lower_nontargeted_permanent_operand(&effect.permanent)?,
            "counter": effect.counter.as_str(),
            "count": lower_targetless_numeric_expression(&effect.count, declarations)?,
        })),
        // Zone collection recognition remains shared. Its old combined
        // transport is retired until the generic moveCards path is proved.
        LocalEffectNode::ZoneBatch(_) => Err(TargetlessLoweringError::UnsupportedEffect),
        LocalEffectNode::Sacrifice(effect) => lower_sacrifice_effect(effect, declarations),
        LocalEffectNode::DestroyCollection(effect) => {
            Ok(lower_destroy_permanent_collection_effect(effect))
        }
        LocalEffectNode::Retarget(_)
        | LocalEffectNode::CounterSpell(_)
        | LocalEffectNode::TemporaryKeywordGrant(_)
        | LocalEffectNode::MillCards(_)
        | LocalEffectNode::ZoneChange(_)
        | LocalEffectNode::TargetCardZoneChange(_)
        | LocalEffectNode::PlayerAction(_)
        | LocalEffectNode::TargetPlayerLife(_)
        | LocalEffectNode::ModifyStats(_)
        | LocalEffectNode::BecomeTypes(_)
        | LocalEffectNode::Attach(_)
        | LocalEffectNode::Choose(_)
        | LocalEffectNode::UntapTarget(_) => Err(TargetlessLoweringError::RequiresTarget),
        LocalEffectNode::MoveCards(_)
        | LocalEffectNode::CreateExplicitTokens(_)
        | LocalEffectNode::CreateTokens(_)
        | LocalEffectNode::CreateNamedTokens(_)
        | LocalEffectNode::OptionalLibrarySearch(_)
        | LocalEffectNode::ControllerLibrarySearch(_)
        | LocalEffectNode::Earthbend(_)
        | LocalEffectNode::OptionalCardDeployment(_) => {
            Err(TargetlessLoweringError::RequiresDeclaration)
        }
        // Only a source move with an explicitly proved origin is a standalone
        // primitive here; the other source moves need a caller-owned binding.
        LocalEffectNode::MoveSource(movement)
            if movement.source == LocalObjectReference::SourceSelf
                && (movement.from == SourceZone::Graveyard
                    // `return SOURCE to its owner's hand` (CR 108.3).
                    || (movement.from == SourceZone::Battlefield
                        && movement.to == SourceZone::Hand
                        && movement.controller == SourceMoveController::Owner)) =>
        {
            Ok(lower_source_move_effect(movement, self_ref(), None))
        }
        // CR 701.16a: one ordinary named-token creation.
        LocalEffectNode::Investigate(effect) => {
            lower_investigate(effect).ok_or(TargetlessLoweringError::RequiresTarget)
        }
        LocalEffectNode::AnimateSource(_)
        | LocalEffectNode::MoveSource(_)
        | LocalEffectNode::LoseAbilities(_)
        | LocalEffectNode::DestroyReferencedSet(_) => {
            Err(TargetlessLoweringError::UnsupportedEffect)
        }
    }
}

#[cfg(test)]
#[path = "surveil_lowering_tests.rs"]
mod surveil_tests;

#[cfg(test)]
#[path = "targetless_numeric_context_tests.rs"]
mod numeric_context_tests;
