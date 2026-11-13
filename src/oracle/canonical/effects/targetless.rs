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
}

impl TargetlessLoweringError {
    pub(in crate::oracle::canonical) fn expectation(self) -> &'static str {
        match self {
            Self::RequiresTarget => "a targetless effect requiring no target declaration",
            Self::RequiresDeclaration => {
                "a targetless effect requiring no unavailable value declaration"
            }
            Self::UnsupportedEffect => "a supported atomic targetless effect",
        }
    }
}

fn lower_atom(
    atom: &NumericAtom,
    declarations: TargetlessDeclarationSupport,
) -> Result<Value, TargetlessLoweringError> {
    if atom == &NumericAtom::VariableX && declarations != TargetlessDeclarationSupport::XValue {
        return Err(TargetlessLoweringError::RequiresDeclaration);
    }
    Ok(lower_numeric_atom(atom.clone()))
}

fn lower_expression(
    expression: &NumericExpression,
    declarations: TargetlessDeclarationSupport,
) -> Result<Value, TargetlessLoweringError> {
    match expression {
        NumericExpression::Atom(atom) => lower_atom(atom, declarations),
        NumericExpression::PowerOf(_) | NumericExpression::ManaValueOf(_) => {
            Err(TargetlessLoweringError::RequiresTarget)
        }
        NumericExpression::ObjectProperty { .. }
        | NumericExpression::SourceCastX
        | NumericExpression::EventValue(_)
        | NumericExpression::CountDistinctCardTypes { .. }
        | NumericExpression::Add { .. }
        | NumericExpression::Subtract { .. }
        | NumericExpression::Multiply { .. }
        | NumericExpression::CardsInZone { .. }
        | NumericExpression::Divide { .. } => {
            lower_numeric_expression(expression).ok_or(TargetlessLoweringError::UnsupportedEffect)
        }
    }
}

/// Lowers atomic local effects whose recipient is fixed by the ability's
/// controller. Target selection and declaration collection intentionally stay
/// outside this boundary.
pub(in crate::oracle::canonical) fn lower_targetless_local_effect(
    effect: &LocalEffectNode,
    declarations: TargetlessDeclarationSupport,
) -> Result<Value, TargetlessLoweringError> {
    match effect {
        LocalEffectNode::AddMana(effect) => Ok(lower_add_mana_effect(effect)),
        LocalEffectNode::DealDamage(effect) => {
            let recipient = match &effect.recipient {
                DamageRecipientNode::Player(LocalPlayerReference::Controller) => controller(),
                DamageRecipientNode::Targets(_)
                | DamageRecipientNode::Player(LocalPlayerReference::ControllerOf(_)) => {
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
                "amount": lower_atom(&effect.amount, declarations)?,
                "recipient": recipient,
            }))
        }
        LocalEffectNode::GainLife(effect) => {
            let player = match effect.player {
                LocalPlayerReference::Controller => controller(),
                LocalPlayerReference::ControllerOf(_) => {
                    return Err(TargetlessLoweringError::RequiresTarget);
                }
            };
            Ok(json!({
                "kind": "gainLife",
                "player": player,
                "amount": lower_expression(&effect.amount, declarations)?,
            }))
        }
        LocalEffectNode::LoseLife(effect) => {
            let player = match effect.player {
                LocalPlayerReference::Controller => controller(),
                LocalPlayerReference::ControllerOf(_) => {
                    return Err(TargetlessLoweringError::RequiresTarget);
                }
            };
            Ok(json!({
                "kind": "loseLife",
                "player": player,
                "amount": lower_expression(&effect.amount, declarations)?,
            }))
        }
        LocalEffectNode::DrawCards(effect) => {
            let player = match effect.player {
                LocalPlayerReference::Controller => controller(),
                LocalPlayerReference::ControllerOf(_) => {
                    return Err(TargetlessLoweringError::RequiresTarget);
                }
            };
            Ok(json!({
                "kind": "drawCards",
                "player": player,
                "count": lower_expression(&effect.count, declarations)?,
            }))
        }
        LocalEffectNode::OptionalDrawCards(_) => Err(TargetlessLoweringError::RequiresDeclaration),
        LocalEffectNode::Amass(effect) => {
            let player = match effect.player {
                LocalPlayerReference::Controller => controller(),
                LocalPlayerReference::ControllerOf(_) => {
                    return Err(TargetlessLoweringError::RequiresTarget);
                }
            };
            Ok(json!({
                "kind": "amass",
                "player": player,
                "armySubtype": effect.army_subtype.ir_name(),
                "count": lower_expression(&effect.count, declarations)?,
            }))
        }
        LocalEffectNode::Surveil(effect) => {
            let player = match effect.player {
                LocalPlayerReference::Controller => controller(),
                LocalPlayerReference::ControllerOf(_) => {
                    return Err(TargetlessLoweringError::RequiresTarget);
                }
            };
            Ok(json!({
                "kind": "surveil",
                "player": player,
                "count": lower_expression(&effect.count, declarations)?,
            }))
        }
        LocalEffectNode::TransformPermanent(effect) => Ok(json!({
            "kind": "transformPermanent",
            "object": match &effect.permanent {
                LocalObjectReference::SourceSelf => self_ref(),
            },
        })),
        LocalEffectNode::PutCounters(effect) => Ok(json!({
            "kind": "putCounters",
            "permanent": match &effect.permanent {
                LocalObjectReference::SourceSelf => self_ref(),
            },
            "counter": effect.counter.as_str(),
            "count": lower_expression(&effect.count, declarations)?,
        })),
        LocalEffectNode::ZoneBatch(effect) => Ok(match (effect.action, effect.collection) {
            (ZoneBatchActionNode::Exile, ZoneCollectionNode::AllGraveyards) => {
                json!({ "kind": "exileAllGraveyards" })
            }
            (ZoneBatchActionNode::Exile, ZoneCollectionNode::OpponentGraveyards) => json!({
                "kind": "exileAllGraveyards",
                "exceptPlayer": controller(),
            }),
        }),
        LocalEffectNode::SacrificeSource(effect) => Ok(json!({
            "kind": "sacrificePermanent",
            "permanent": match effect.source {
                LocalObjectReference::SourceSelf => self_ref(),
            },
        })),
        LocalEffectNode::DestroyCollection(effect) => {
            Ok(lower_destroy_permanent_collection_effect(effect))
        }
        LocalEffectNode::CounterSpell(_)
        | LocalEffectNode::TemporaryKeywordGrant(_)
        | LocalEffectNode::MillCards(_)
        | LocalEffectNode::ZoneChange(_)
        | LocalEffectNode::TargetCardZoneChange(_)
        | LocalEffectNode::TargetPlayerLife(_)
        | LocalEffectNode::ModifyStats(_)
        | LocalEffectNode::UntapTarget(_) => Err(TargetlessLoweringError::RequiresTarget),
        LocalEffectNode::MoveCards(_)
        | LocalEffectNode::CreateTokens(_)
        | LocalEffectNode::CreateNamedTokens(_)
        | LocalEffectNode::OptionalLibrarySearch(_)
        | LocalEffectNode::ControllerLibrarySearch(_)
        | LocalEffectNode::Earthbend(_)
        | LocalEffectNode::OptionalCardDeployment(_) => {
            Err(TargetlessLoweringError::RequiresDeclaration)
        }
        LocalEffectNode::AnimateSource(_)
        | LocalEffectNode::Investigate(_)
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
