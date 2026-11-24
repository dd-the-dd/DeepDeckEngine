use super::super::{MachineFailure, OracleTokenCursor};
use super::*;
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

pub(in crate::oracle::canonical) struct StaticCollectionActivatedGrantNode {
    pub collection: DomainObjectCollectionNode,
    pub ability: ActivatedManaAbilityNode,
}

pub(in crate::oracle::canonical) enum StaticCollectionActivatedGrantAttempt {
    NotClaimed,
    Parsed(StaticCollectionActivatedGrantNode),
    Failed(MachineFailure),
}

/// The collection and the quoted activation are separate children borrowing
/// the original token stream. Only a complete static envelope commits.
pub(in crate::oracle::canonical) fn try_recognize_static_collection_activated_grant(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> StaticCollectionActivatedGrantAttempt {
    let mut trial = cursor.clone();
    trial.enter("static.granted-activation.objects");
    let Ok(collection) = recognize_object_collection_in_domain(&mut trial, ObjectKind::Permanent)
    else {
        return StaticCollectionActivatedGrantAttempt::NotClaimed;
    };
    trial.enter("static.granted-activation.predicate");
    let Some(verb) = trial
        .peek()
        .filter(|word| word.eq_ignore_ascii_case("have") || word.eq_ignore_ascii_case("has"))
    else {
        return StaticCollectionActivatedGrantAttempt::NotClaimed;
    };
    let verb = verb.to_owned();
    if trial.consume(&verb).is_err() || trial.peek() != Some("\"") {
        return StaticCollectionActivatedGrantAttempt::NotClaimed;
    }
    let result = (|| {
        trial.enter("static.granted-activation.quote-open");
        trial.consume("\"")?;
        let start = trial.position();
        let mut length = 0;
        while trial.peek_ahead(length).is_some_and(|word| word != "\"") {
            length += 1;
        }
        trial.enter("static.granted-activation.quote-close");
        trial.require(
            trial.peek_ahead(length) == Some("\""),
            "a closing ability quote",
        )?;
        let end = start + length;
        // Inspect the entire outer envelope before claiming a child. A
        // conditional/duration/additional sentence belongs to another parent.
        let mut envelope = trial.clone();
        envelope.advance_to(end + 1)?;
        if envelope.peek() == Some(".") {
            envelope.consume(".")?;
        }
        if envelope.finish().is_err() {
            return Ok(None);
        }
        if !trial
            .token_slice(start, end)
            .iter()
            .any(|token| token.raw == ":")
        {
            return Ok(None);
        }
        trial.enter("static.granted-activation.permanent-domain");
        trial.require(
            collection.domain == ObjectKind::Permanent
                && collection.quantifier == ObjectCollectionQuantifier::Universal,
            "a battlefield-permanent collection",
        )?;
        trial.require(
            !object_predicate_uses_x(&collection.objects.predicate),
            "a bound static collection predicate",
        )?;
        let mut child =
            OracleTokenCursor::new("quoted-activated-ability", trial.token_slice(start, end));
        let ability = recognize_activated_mana_ability(&mut child, name_links, name_failure)?;
        child.enter("static.granted-activation.mana-capability");
        child.require(
            matches!(
                ability.costs.costs.as_slice(),
                [LocalActivationCostNode::TapSelf(_)]
            ),
            "one tap-self cost in the supported granted mana program",
        )?;
        child.require(
            ability.followup.is_none(),
            "one independent mana-production effect",
        )?;
        child.require(
            match &ability.effect {
                AddManaEffectNode::Fixed(effect) => !effect.mana.contains_variable_x(),
                AddManaEffectNode::Choice(effect) => effect
                    .options
                    .iter()
                    .all(|mana| !mana.contains_variable_x()),
                AddManaEffectNode::AnyColor(_) => true,
                AddManaEffectNode::EachColor(_)
                | AddManaEffectNode::AnyColorAmount(_)
                | AddManaEffectNode::Counted(_)
                | AddManaEffectNode::ScopedColor(_)
                | AddManaEffectNode::Qualified { .. } => false,
            },
            "a closed fixed, alternative, or literal-color-choice mana value",
        )?;
        trial.advance_to(end)?;
        trial.consume("\"")?;
        if trial.peek() == Some(".") {
            trial.consume(".")?;
        }
        trial.enter("static.granted-activation.complete");
        trial.finish()?;
        Ok(Some(ability))
    })();
    match result {
        Ok(Some(ability)) => {
            *cursor = trial;
            StaticCollectionActivatedGrantAttempt::Parsed(StaticCollectionActivatedGrantNode {
                collection,
                ability,
            })
        }
        Ok(None) => StaticCollectionActivatedGrantAttempt::NotClaimed,
        Err(failure) => StaticCollectionActivatedGrantAttempt::Failed(failure),
    }
}
