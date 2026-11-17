use super::super::{MachineResult, OracleTokenCursor};
use super::{
    NumericAtom, ObjectControllerScope, TapSelfCostNode, recognize_numeric_atom,
    recognize_tap_self_cost,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ManaChoiceDomain {
    CommanderColorIdentity,
    LandColorsControlledBy(ObjectControllerScope),
    /// Mana types include colorless; bounded to controller-owned lands.
    LandTypesControlledByAbilityController,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AddScopedManaEffectNode {
    pub amount: i64,
    pub domain: ManaChoiceDomain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SimpleTapScopedManaAbilityNode {
    pub cost: TapSelfCostNode,
    pub effect: AddScopedManaEffectNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScopedManaState {
    Action,
    Amount,
    ManaNoun,
    AnyColor,
    ScopeRelation,
    ControllerReference,
    CommanderReference,
    ColorIdentity,
    Complete,
}

#[derive(Default)]
struct ScopedManaAttemptFrame {
    amount: Option<i64>,
    domain: Option<ManaChoiceDomain>,
}

/// Recognizes `Add <literal> mana of any color in your commander's color
/// identity` from the shared token stream. The scope is retained as typed
/// syntax so lowering can select the existing generic commander-color mana
/// domain without encoding a particular card.
pub(in crate::oracle::canonical) fn recognize_add_scoped_mana_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AddScopedManaEffectNode> {
    let mut state = ScopedManaState::Action;
    let mut frame = ScopedManaAttemptFrame::default();
    loop {
        state = match state {
            ScopedManaState::Action => {
                cursor.enter("effect.add-scoped-mana.action");
                cursor.consume("add")?;
                ScopedManaState::Amount
            }
            ScopedManaState::Amount => {
                cursor.enter("effect.add-scoped-mana.amount");
                frame.amount = match recognize_numeric_atom(cursor)? {
                    NumericAtom::Integer(amount) if amount > 0 => Some(amount),
                    _ => return Err(cursor.fail("a positive literal mana amount")),
                };
                ScopedManaState::ManaNoun
            }
            ScopedManaState::ManaNoun => {
                cursor.enter("effect.add-scoped-mana.mana-noun");
                let token = cursor.consume_type("costWord")?;
                cursor.require(token.raw.eq_ignore_ascii_case("mana"), "the mana noun")?;
                ScopedManaState::AnyColor
            }
            ScopedManaState::AnyColor => {
                cursor.enter("effect.add-scoped-mana.any-color");
                cursor.consume("of")?;
                cursor.consume("any")?;
                cursor.consume_type("colorNoun")?;
                ScopedManaState::ScopeRelation
            }
            ScopedManaState::ScopeRelation => {
                cursor.enter("effect.add-scoped-mana.scope-relation");
                cursor.consume("in")?;
                ScopedManaState::ControllerReference
            }
            ScopedManaState::ControllerReference => {
                cursor.enter("effect.add-scoped-mana.controller-reference");
                cursor.consume("your")?;
                ScopedManaState::CommanderReference
            }
            ScopedManaState::CommanderReference => {
                cursor.enter("effect.add-scoped-mana.commander-reference");
                cursor.consume_type("possessivePlayerRoleWord")?;
                ScopedManaState::ColorIdentity
            }
            ScopedManaState::ColorIdentity => {
                cursor.enter("effect.add-scoped-mana.color-identity");
                cursor.consume_type("colorNoun")?;
                cursor.consume_type("identityWord")?;
                frame.domain = Some(ManaChoiceDomain::CommanderColorIdentity);
                ScopedManaState::Complete
            }
            ScopedManaState::Complete => {
                cursor.enter("effect.add-scoped-mana.complete");
                return Ok(AddScopedManaEffectNode {
                    amount: cursor.require_value(frame.amount, "a populated mana amount")?,
                    domain: cursor.require_value(frame.domain, "a populated mana choice domain")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScopedManaActivationState {
    Cost,
    Separator,
    Effect,
    Terminator,
    Complete,
}

#[derive(Default)]
struct ScopedManaActivationAttemptFrame {
    cost: Option<TapSelfCostNode>,
    effect: Option<AddScopedManaEffectNode>,
}

/// Activated-ability envelope for a tap cost and one scoped mana-production
/// effect. Cost and mana-domain children remain independently typed.
pub(in crate::oracle::canonical) fn recognize_simple_tap_scoped_mana_ability(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SimpleTapScopedManaAbilityNode> {
    let mut state = ScopedManaActivationState::Cost;
    let mut frame = ScopedManaActivationAttemptFrame::default();
    loop {
        state = match state {
            ScopedManaActivationState::Cost => {
                cursor.enter("activation.scoped-mana.cost");
                frame.cost = Some(recognize_tap_self_cost(cursor)?);
                ScopedManaActivationState::Separator
            }
            ScopedManaActivationState::Separator => {
                cursor.enter("activation.scoped-mana.separator");
                cursor.consume(":")?;
                ScopedManaActivationState::Effect
            }
            ScopedManaActivationState::Effect => {
                cursor.enter("activation.scoped-mana.effect");
                frame.effect = Some(recognize_add_scoped_mana_effect(cursor)?);
                ScopedManaActivationState::Terminator
            }
            ScopedManaActivationState::Terminator => {
                cursor.enter("activation.scoped-mana.terminator");
                cursor.consume(".")?;
                ScopedManaActivationState::Complete
            }
            ScopedManaActivationState::Complete => {
                cursor.enter("activation.scoped-mana.complete");
                cursor.finish()?;
                return Ok(SimpleTapScopedManaAbilityNode {
                    cost: cursor.require_value(frame.cost, "a populated tap cost")?,
                    effect: cursor.require_value(frame.effect, "a populated scoped mana effect")?,
                });
            }
        };
    }
}
