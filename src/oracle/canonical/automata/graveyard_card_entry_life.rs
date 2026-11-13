use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalEffectNode, LocalPlayerReference, LoseLifeEffectNode, NumericExpression, ObjectPredicate,
    TargetBinding, recognize_card_type_union_selector, recognize_life_effect,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum GraveyardCardSourceNode {
    AnyGraveyard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardEntryDestinationNode {
    Battlefield,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetedGraveyardCardEntryNode {
    pub predicate: ObjectPredicate,
    pub source: GraveyardCardSourceNode,
    pub destination: CardEntryDestinationNode,
    pub controller: LocalPlayerReference,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct GraveyardCardEntryLifeNode {
    pub entry: TargetedGraveyardCardEntryNode,
    pub life: LoseLifeEffectNode,
    pub target_binding: TargetBinding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardEntryState {
    Action,
    Target,
    Selector,
    SourceRelation,
    SourceScope,
    SourceZone,
    DestinationRelation,
    DestinationArticle,
    DestinationZone,
    ControlRelation,
    Controller,
    ControlNoun,
    Complete,
}

/// Recognizes a targeted card move from the shared graveyard domain to the
/// ability controller's battlefield. The selector child owns card types; this
/// machine owns only the source, destination, and control relations.
pub(in crate::oracle::canonical) fn recognize_targeted_graveyard_card_entry(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetedGraveyardCardEntryNode> {
    let mut state = CardEntryState::Action;
    let mut predicate = None;
    loop {
        state = match state {
            CardEntryState::Action => {
                cursor.enter("effect.graveyard-card-entry.action");
                cursor.consume("put")?;
                CardEntryState::Target
            }
            CardEntryState::Target => {
                cursor.enter("effect.graveyard-card-entry.target");
                cursor.consume("target")?;
                CardEntryState::Selector
            }
            CardEntryState::Selector => {
                cursor.enter("effect.graveyard-card-entry.selector");
                predicate = Some(recognize_card_type_union_selector(cursor)?);
                CardEntryState::SourceRelation
            }
            CardEntryState::SourceRelation => {
                cursor.enter("effect.graveyard-card-entry.source-relation");
                cursor.consume("from")?;
                CardEntryState::SourceScope
            }
            CardEntryState::SourceScope => {
                cursor.enter("effect.graveyard-card-entry.source-scope");
                let scope = cursor
                    .peek()
                    .map(str::to_ascii_lowercase)
                    .ok_or_else(|| cursor.fail("`a` or `any` before graveyard"))?;
                cursor.require(matches!(scope.as_str(), "a" | "any"), "`a` or `any`")?;
                cursor.consume(&scope)?;
                CardEntryState::SourceZone
            }
            CardEntryState::SourceZone => {
                cursor.enter("effect.graveyard-card-entry.source-zone");
                cursor.consume("graveyard")?;
                CardEntryState::DestinationRelation
            }
            CardEntryState::DestinationRelation => {
                cursor.enter("effect.graveyard-card-entry.destination-relation");
                let relation = cursor
                    .peek()
                    .map(str::to_ascii_lowercase)
                    .ok_or_else(|| cursor.fail("`onto` or `to` before battlefield"))?;
                cursor.require(matches!(relation.as_str(), "onto" | "to"), "`onto` or `to`")?;
                cursor.consume(&relation)?;
                CardEntryState::DestinationArticle
            }
            CardEntryState::DestinationArticle => {
                cursor.enter("effect.graveyard-card-entry.destination-article");
                cursor.consume("the")?;
                CardEntryState::DestinationZone
            }
            CardEntryState::DestinationZone => {
                cursor.enter("effect.graveyard-card-entry.destination-zone");
                cursor.consume("battlefield")?;
                CardEntryState::ControlRelation
            }
            CardEntryState::ControlRelation => {
                cursor.enter("effect.graveyard-card-entry.control-relation");
                cursor.consume("under")?;
                CardEntryState::Controller
            }
            CardEntryState::Controller => {
                cursor.enter("effect.graveyard-card-entry.controller");
                cursor.consume("your")?;
                CardEntryState::ControlNoun
            }
            CardEntryState::ControlNoun => {
                cursor.enter("effect.graveyard-card-entry.control-noun");
                cursor.consume("control")?;
                CardEntryState::Complete
            }
            CardEntryState::Complete => {
                cursor.enter("effect.graveyard-card-entry.complete");
                return Ok(TargetedGraveyardCardEntryNode {
                    predicate: cursor.require_value(predicate, "a populated card selector")?,
                    source: GraveyardCardSourceNode::AnyGraveyard,
                    destination: CardEntryDestinationNode::Battlefield,
                    controller: LocalPlayerReference::Controller,
                });
            }
        };
    }
}

/// Composes the targeted graveyard move with a later life loss whose numeric
/// expression refers to the same target. The binding is supplied explicitly
/// to the numeric child; no text is reconstructed or reparsed.
pub(in crate::oracle::canonical) fn recognize_graveyard_card_entry_life(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<GraveyardCardEntryLifeNode> {
    let entry = recognize_targeted_graveyard_card_entry(cursor)?;
    cursor.enter("sequence.graveyard-card-entry-life.boundary");
    cursor.consume(".")?;
    let target_binding = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };
    let life = recognize_life_effect(cursor, Some(target_binding))?;
    let LocalEffectNode::LoseLife(life) = life else {
        return Err(cursor.fail("a life-loss effect"));
    };
    cursor.enter("sequence.graveyard-card-entry-life.validation");
    cursor.require(
        life.player == LocalPlayerReference::Controller,
        "the ability controller as life-loss player",
    )?;
    cursor.require(
        life.amount == NumericExpression::ManaValueOf(target_binding),
        "the targeted card's mana value as life-loss amount",
    )?;
    cursor.consume(".")?;
    cursor.finish()?;
    Ok(GraveyardCardEntryLifeNode {
        entry,
        life,
        target_binding,
    })
}

#[cfg(test)]
#[path = "graveyard_card_entry_life_tests.rs"]
mod tests;
