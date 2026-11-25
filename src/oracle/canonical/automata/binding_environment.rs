//! One binding environment for references (`docs/oracle-binding-environment.md`).
//!
//! The ability frame publishes what its event names (the event player, the
//! defending player, the other combatant); the effect program adds the lexical
//! antecedents of the position being read (the singular object and the player
//! of the previous actions). Every reference child resolves `that player`,
//! `defending player`, `its controller`, `that NOUN's owner`, `it` and `that
//! NOUN` against this one environment instead of a parameter of its own.
//! Nothing is guessed: an absent slot is a committed diagnostic.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    AttachedObjectRole, CardZoneName, EffectRecognitionContext, GrammaticalNumber,
    NumericExpression, ObjectCardType, ObjectKind, ObjectPlayerRelation, ObjectPredicate,
    ObjectSubjectAntecedent, PermanentEffectReference, PermanentSubjectScope, PlayerBinding,
    PlayerEffectSubject, SelectorNumericProperty, TargetNominal, TargetSelector,
    TypedObservedEventObject, selector_implies_nominal,
};
use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, CharacteristicLexeme, ObjectCharacteristic, lookup_card_type,
    lookup_object_characteristic, lookup_object_domain, lookup_object_selector_lexeme,
};
use crate::oracle::model::RecognizedEntity;

/// The object an event record binds, with the predicate the event wrote for
/// it. A nominal anaphor (`that creature`) must follow from that predicate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EventObjectSlot {
    pub reference: PermanentEffectReference,
    pub predicate: ObjectPredicate,
    /// The object is the grammatical subject of the event (`a creature
    /// attacks`, `a creature dies`), so a bare `it` can name it. A damaged
    /// recipient is named only with its nominal (`that creature`).
    pub subject: bool,
    /// The event moved the object off the battlefield (`a creature dies`):
    /// its last known information still answers property and player reads,
    /// but it is no longer a permanent an action can operate on (CR 400.7,
    /// 700.4). The card it became is the frame's `event_card`.
    pub departed: bool,
    /// The controller relation the event wrote for the object (`a creature
    /// an opponent controls`): the player `that player` names (CR 108.4).
    pub controller: Option<super::ObjectControllerScope>,
}

impl EventObjectSlot {
    fn implies(&self, nominal: TargetNominal) -> bool {
        self.names(&[nominal])
    }

    /// The event's predicate names the nominal: the event writes the noun of
    /// an attached object itself (`enchanted land becomes tapped`, `the
    /// upkeep of enchanted creature's controller`), as a lexical antecedent.
    fn names(&self, nominals: &[TargetNominal]) -> bool {
        predicate_names_nominals(&self.predicate, nominals)
    }
}

/// The references the ability frame binds from its typed event. Only the
/// trigger parent sets them; every other frame leaves them unbound.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct FrameBindings {
    /// The event records exactly one player (`that player`, CR 603.2).
    pub event_player: bool,
    /// The event is an attack that records the attacked player (CR 506.2).
    pub defending_player: bool,
    /// The event is a block whose every branch names the other combatant.
    pub combat_counterpart: bool,
    /// The object the event record binds (`that creature`, `that land's
    /// controller` when no lexical antecedent has that nominal).
    pub event_object: Option<EventObjectSlot>,
    /// The event is the cast of one spell (`that spell`).
    pub event_spell: bool,
    /// The event moved its object into this public zone, where the ability
    /// can still find the card it became (CR 400.7, 603.6c): `it` / `that
    /// card` in a card action when the program observed no other card.
    pub event_card: Option<CardZoneName>,
    /// A static combat restriction checks its condition for each combat it
    /// restricts: `defending player` names the player the restricted
    /// creature attacks or would attack (CR 506.2, 508.1c, 509.1b). Only
    /// the static combat-restriction parent sets it.
    pub combat_defending_player: bool,
    /// The card selector of the permanent an earlier program step put onto
    /// the battlefield, when a following independent step inherits that
    /// moved permanent as its antecedent (`inherit_program_step_antecedents`,
    /// CR 400.7, 608.2c). Only the program parent sets it; `None` otherwise.
    pub inherited_moved_selector: Option<TargetSelector>,
    /// The token index where a replacing sentence's postposed `instead`
    /// was written (`If COND, NAME deals 4 damage instead.`). Only the
    /// replacing parent sets it (`recognize_replacing_sequence`); a damage
    /// frame whose recipient is omitted exactly there restates the replaced
    /// damage's recipient (CR 608.2c). `None` otherwise.
    pub replacing_instead_at: Option<usize>,
    /// The number the payer chose while paying a cost of this activation
    /// (`remove any number of storage counters`, CR 601.2h via 602.2b),
    /// which `for each counter removed this way` / `that many` restate.
    /// Only the activation parent sets it from its own typed costs
    /// (`quantity_slot.rs`); `None` otherwise.
    pub cost_chosen_amount: Option<super::CostChosenAmountSlot>,
}

impl FrameBindings {
    pub(in crate::oracle::canonical) fn from_context(context: &EffectRecognitionContext) -> Self {
        context.frame.clone()
    }
}

/// The frame bindings plus the lexical antecedents at one program position.
#[derive(Clone, Debug, Default)]
pub(in crate::oracle::canonical) struct BindingEnvironment {
    pub frame: FrameBindings,
    /// A previous action opened a lexical object scope (even an exhausted
    /// one); the frame's event objects are then no longer the default.
    pub local_object_scope: bool,
    /// The singular object `it` / `that NOUN` / `its controller` names.
    pub object: Option<ObjectSubjectAntecedent>,
    /// The player an earlier `target player` declared (`that player`).
    pub player: Option<PlayerBinding>,
    /// The subject of the clause being read (the object that deals damage):
    /// a possessive `its` in its arguments names it. Installed by the clause
    /// parent around its arguments only.
    pub subject: Option<PermanentEffectReference>,
    /// The index of the action being read in the enclosing program; a
    /// target declared by this action binds to it.
    pub effect_index: Option<usize>,
    /// The ActionResult antecedent: the preceding instruction or resolution
    /// cost whose actual performance an outcome connector (`If you do`,
    /// `If they don't`, `When you do`) observes. Installed by the program
    /// parent around the connector only (CR 608.2c, 603.12).
    pub action_result: Option<ActionResultSlot>,
    /// A number the enclosing frame already read (`that number` in a later
    /// clause of the same frame); the arithmetic grammar reads it as a
    /// primary.
    pub number: Option<NumericExpression>,
    /// The object set the immediately preceding action of the program
    /// declared or produced (`those creatures`, `they`, `that token`). The
    /// program parent installs it from its earlier actions
    /// (`permanent_subject::prior_object_set`); it is never guessed.
    pub objects: Option<ObjectSetAntecedent>,
    /// The latest singular target of the program whose selector admits a
    /// player (`target player`, `target opponent`, `target player or
    /// planeswalker`). The program parent publishes it from its earlier
    /// actions (`publish_program_antecedents`); `that player` names it.
    pub player_target: Option<PlayerTargetAntecedent>,
    /// The one object the latest object-producing action of the program
    /// selected or moved on resolution (`sacrifices a creature of their
    /// choice`, `reveal the top card of your library`, CR 608.2c). The
    /// program parent publishes it (`publish_program_antecedents`); `that
    /// NOUN`, `its PROPERTY`, `that NOUN's PROPERTY` and `the VERBed NOUN's
    /// PROPERTY` resolve against it. A result that is no longer a
    /// battlefield permanent answers only reads of its last known
    /// information (CR 400.7, 608.2h); an operand reader refuses it.
    pub result_object: Option<ActionResultObject>,
    /// The latest singular target the program declared whose selector does
    /// not guarantee a battlefield permanent: a spell or ability on the
    /// stack (`counter target spell`) or `any target` (a creature, a player
    /// or a planeswalker, CR 115.4). Targets are declared as the spell is
    /// cast (CR 601.2c), so a condition may read it, even the condition that
    /// gates the declaring action; it is never a permanent operand. The
    /// program parent publishes it (`publish_program_antecedents`).
    pub declared_target: Option<DeclaredTargetAntecedent>,
    /// The recipient of the damage the latest action of the program dealt,
    /// as a later reference to it: the slot it declared, or the fixed or
    /// bound group it named. A replacing sentence's `deals N damage
    /// instead` restates that recipient (CR 608.2c); the program parent
    /// publishes it (`publish_program_antecedents`).
    pub restated_damage_recipient: Option<super::DamageRecipientNode>,
    /// The latest singular card target a card zone change of the program
    /// declared (`exile target creature card from your graveyard`, CR
    /// 115.1). It is no permanent: only a reader of the card's
    /// characteristics (`a copy of that card`, CR 707.2) names it. The
    /// program parent publishes it (`publish_program_antecedents`).
    pub card_target: Option<DeclaredTargetAntecedent>,
    /// The player who could pay the cost of the latest `unless` frame of the
    /// program (CR 118.12): `that player` of a later sentence names that
    /// payer, whether the cost was paid or refused. The program parent
    /// publishes it (`publish_installed_unless_payer`).
    pub unless_payer: Option<UnlessPayerAntecedent>,
    /// The operand being read belongs to an action whose player slot names
    /// a player other than the ability's controller (the player who loses
    /// life, draws, or is dealt the damage; each member of a population in
    /// turn): a distributive `they` / `that player` without another
    /// antecedent names that player (`each player loses 1 life for each
    /// creature they control`). Installed by the action parent around its
    /// operands only (`player_subject::with_acting_player`); the lowering
    /// binds it to the action's player slot.
    pub acting_player: bool,
}

/// The payer of an `unless` frame as a later reference to that player: the
/// first action the frame guards (a player declared after it is nearer) and
/// the player reference the payer is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct UnlessPayerAntecedent {
    pub first_effect: usize,
    pub payer: BoundPlayerReference,
}

/// A singular declared target that need not be a battlefield permanent: the
/// slot it binds and the selector that declared it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DeclaredTargetAntecedent {
    pub binding: super::TargetBinding,
    pub selector: TargetSelector,
}

/// The one object an earlier action of the same program selected or moved
/// on resolution (CR 608.2c): the action that produced it, the selector its
/// own selection guarantees (a nominal anaphor must follow from it), the
/// number it stated, and the verb a participle (`the sacrificed creature`)
/// restates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActionResultObject {
    pub reference: super::ActionResultReference,
    pub selector: TargetSelector,
    pub number: GrammaticalNumber,
    /// The producing verb (`sacrificed`, `discarded`, `revealed`, `milled`,
    /// `exiled`), when the action is one of those; a move without such a
    /// verb (`put the top card of your library into your graveyard`) has
    /// none.
    pub verb: Option<super::ActionResultVerb>,
}

impl ActionResultObject {
    /// The reference a reader of this result returns.
    pub(in crate::oracle::canonical) fn permanent_reference(&self) -> PermanentEffectReference {
        PermanentEffectReference::ActionResult(self.reference)
    }

    /// A singular nominal anaphor names the result when the producer's
    /// selector guarantees the nominal.
    fn names(&self, nominals: &[TargetNominal]) -> bool {
        self.number == GrammaticalNumber::Singular
            && selector_names_nominals(&self.selector, nominals)
    }
}

/// The operand diagnostic of a reference to an action result that left the
/// battlefield or is a card (CR 400.7): only its last known information is
/// readable (CR 608.2h).
pub(in crate::oracle::canonical) const DEPARTED_ACTION_RESULT_OPERAND: &str = "a battlefield permanent for the anaphor; the object the earlier action selected or moved is not a permanent any more (CR 400.7), only its last known information is readable";

/// A singular declared target that may be a player (CR 115.1): `that
/// player` names the player it is; when its selector also admits
/// permanents, `that player or that NOUN's controller` names the player it
/// is or the controller of the permanent it is (CR 109.4). Both are the one
/// controller relation of the declared object, which the runtime reads as
/// the player itself for a player target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerTargetAntecedent {
    pub binding: super::TargetBinding,
    pub selector: TargetSelector,
}

/// The one producer contract of the program's lexical antecedents: the
/// program parent builds the environment of the action it reads next and
/// hands the actions read so far; this publishes every antecedent derived
/// from them that the parent does not track itself. The parent owns the
/// sentence boundaries; nothing here reads tokens.
pub(in crate::oracle::canonical) fn publish_program_antecedents(
    mut environment: BindingEnvironment,
    effects: &[super::LocalEffectNode],
) -> BindingEnvironment {
    environment.player_target = program_player_target(effects);
    environment.result_object = super::permanent_subject::program_action_result(effects);
    environment.declared_target = program_declared_target(effects);
    environment.restated_damage_recipient = restated_damage_recipient(effects);
    environment.card_target = program_card_target(effects);
    retype_moved_object(&mut environment, effects);
    environment
}

/// The latest card zone change of the program that declared card targets,
/// when it declared exactly one card (`exile target creature card from your
/// graveyard`): the card a later characteristic reader (`a copy of that
/// card`) names (CR 115.1, 608.2c). A nearer declaration of several cards
/// ends the antecedent; the lowering binds the singular slot under the same
/// key (`card_target_bindings`).
fn program_card_target(effects: &[super::LocalEffectNode]) -> Option<DeclaredTargetAntecedent> {
    for (effect_index, effect) in effects.iter().enumerate().rev() {
        if let super::LocalEffectNode::TargetCardZoneChange(change) = effect {
            return (change.cardinality == super::ExpressedTargetCardinality::EXACTLY_ONE).then(
                || DeclaredTargetAntecedent {
                    binding: super::TargetBinding {
                        effect_index,
                        slot_index: 0,
                    },
                    selector: TargetSelector::Object(change.predicate.clone()),
                },
            );
        }
    }
    None
}

/// Publishes the payer of the latest `unless` frame of the program on the
/// environment the program parent installed for the action it reads next
/// (CR 118.12). The parent owns the frames; nothing here reads tokens.
pub(in crate::oracle::canonical) fn publish_installed_unless_payer(
    cursor: &mut OracleTokenCursor<'_>,
    frames: &[super::UnlessFrameNode],
) {
    let mut environment = installed_binding_environment(cursor);
    environment.unless_payer = frames.last().and_then(unless_payer_antecedent);
    cursor.replace_binding_environment(Some(std::sync::Arc::new(environment)));
}

/// The one player a payment frame names as its payer: the controller of the
/// guarded action's declared target, or a declared player. The ability's
/// controller is `you`, never `that player`; a population has no single
/// payer.
fn unless_payer_antecedent(frame: &super::UnlessFrameNode) -> Option<UnlessPayerAntecedent> {
    let super::UnlessClauseNode::Payment { payer, .. } = &frame.clause else {
        return None;
    };
    let payer = match payer {
        super::UnlessPayerNode::ControllerOf(binding) => BoundPlayerReference::ObjectRelation {
            relation: ObjectPlayerRelation::Controller,
            object: PermanentEffectReference::Target(*binding),
        },
        super::UnlessPayerNode::BoundPlayer(binding) => {
            BoundPlayerReference::Bound(PlayerBinding(binding.effect_index))
        }
        super::UnlessPayerNode::Controller | super::UnlessPayerNode::QuantifiedPlayer => {
            return None;
        }
    };
    Some(UnlessPayerAntecedent {
        first_effect: frame.first_effect,
        payer,
    })
}

/// Zone-crossing retyping (CR 400.7): the permanent a move put onto the
/// battlefield is a new object, but the object it was is the one the move
/// named, so its nominal follows from what that object was declared or
/// recorded as. When the move restates no card predicate (`return it to the
/// battlefield`, `put that card onto the battlefield`), the selector is the
/// one of the object the moved pronoun named: a declared target's slot,
/// the event's object (the card a dying creature became, read only when the
/// move is the program's first action, so no observed card is nearer), or
/// the one object an earlier action selected. Anything else stays without a
/// selector and a nominal anaphor of it keeps its committed diagnostic.
fn retype_moved_object(environment: &mut BindingEnvironment, effects: &[super::LocalEffectNode]) {
    let moved = PermanentEffectReference::Observed(TypedObservedEventObject::MovedOntoBattlefield);
    let Some(object) = environment
        .object
        .as_mut()
        .filter(|object| object.reference == moved && object.selector.is_none())
    else {
        return;
    };
    let Some((move_index, operand)) =
        effects
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, effect)| {
                super::permanent_subject::moved_onto_battlefield_selector(effect)
                    .map(|_| (index, moved_card_operand(effect)))
            })
    else {
        // No move of this sequence: the permanent an earlier program step
        // put onto the battlefield, with the selector that step published.
        object.selector = environment.frame.inherited_moved_selector.clone();
        return;
    };
    object.selector = match operand {
        Some(super::CardOperandNode::ObjectAntecedent {
            object: PermanentEffectReference::Target(binding),
            ..
        }) => effects
            .get(binding.effect_index)
            .and_then(|effect| {
                super::permanent_subject::target_slot_selector(effect, binding.slot_index)
            })
            .cloned(),
        Some(super::CardOperandNode::ObjectAntecedent {
            object: PermanentEffectReference::Observed(TypedObservedEventObject::EventPermanent),
            ..
        }) => environment
            .frame
            .event_object
            .as_ref()
            .map(|slot| TargetSelector::Object(slot.predicate.clone())),
        Some(super::CardOperandNode::ObjectAntecedent {
            object: PermanentEffectReference::ActionResult(result),
            ..
        }) => effects.get(result.effect_index).and_then(|effect| {
            super::permanent_subject::action_result_object(effect, result.effect_index)
                .map(|produced| produced.selector)
        }),
        // `return it to the battlefield` in a frame whose event put its
        // object into a public zone: the card the event's object became.
        Some(super::CardOperandNode::Reference(super::CardCollectionReferenceNode::Selected))
            if move_index == 0 && environment.frame.event_card.is_some() =>
        {
            environment
                .frame
                .event_object
                .as_ref()
                .filter(|slot| slot.departed)
                .map(|slot| TargetSelector::Object(slot.predicate.clone()))
        }
        _ => None,
    };
}

/// Branch inheritance across independent program steps (CR 608.2c, 400.7):
/// the context a following step of `previous` is read with. When the last
/// written sentence of `previous` ends by putting one card onto the
/// battlefield with an ordinary card move (`You may put a creature card from
/// your hand onto the battlefield.`), the following steps start with that
/// permanent as their singular antecedent (`That creature gains haste.`),
/// with the move's card selector. Such a move always binds the moved
/// permanent program-wide, so a following step reads the same binding; when
/// an optional move was declined the binding names no object and the
/// following action on it does nothing. Every other antecedent (a declared
/// target, an action result) is bound per sentence group and is not
/// inherited: the program parent keeps its committed diagnostic.
///
/// The program parent calls it once per following step group, e.g. in
/// `effect_program::complete_outcome_block`:
/// `let context = &super::inherit_program_step_antecedents(context, &block);`
pub(in crate::oracle::canonical) fn inherit_program_step_antecedents(
    context: &EffectRecognitionContext,
    previous: &super::EffectProgramNode,
) -> EffectRecognitionContext {
    let sequence = match previous {
        super::EffectProgramNode::Sequence(sequence) => sequence,
        super::EffectProgramNode::Optional { body, .. } => match body.as_ref() {
            super::EffectProgramNode::Sequence(sequence) => sequence,
            _ => return context.clone(),
        },
        _ => return context.clone(),
    };
    inherit_sequence_antecedents(context, sequence)
}

/// The same inheritance from one producing sentence: the body of an
/// outcome (`you may return target creature card ... to the battlefield. If
/// you do, it gains haste.`) is read with the permanent its producer put
/// onto the battlefield. The program parent calls it with the producer's
/// sequence, e.g. in `effect_program::recognize_optional_action_block`:
/// `let context = &super::inherit_sequence_antecedents(context, &action);`
/// before reading the outcome body.
pub(in crate::oracle::canonical) fn inherit_sequence_antecedents(
    context: &EffectRecognitionContext,
    sequence: &super::EffectSequenceNode,
) -> EffectRecognitionContext {
    let mut inherited = context.clone();
    let Some(last) = sequence.effects.last() else {
        return inherited;
    };
    // `exile target creature. If you do, return that card ...`: the body's
    // pronoun names the card the exile produced (CR 400.7, 608.2c), read
    // from the producer's observed exile collection; the enclosing
    // ability's subject (`When this creature enters`) is no antecedent of it.
    if exiles_another_object(last) {
        inherited.permanent_subject_scope = PermanentSubjectScope::Local(None);
        inherited.permanent_subject_selector = None;
        return inherited;
    }
    let Some(selector) = always_bound_battlefield_entry(last) else {
        return inherited;
    };
    inherited.permanent_subject_scope = PermanentSubjectScope::Local(Some(
        PermanentEffectReference::Observed(TypedObservedEventObject::MovedOntoBattlefield),
    ));
    inherited.permanent_subject_selector = selector.clone();
    inherited.frame.inherited_moved_selector = selector;
    inherited
}

/// An exile of an object other than the ability's source (a declared
/// target, a chosen or queried permanent, a card), looking through an
/// optional block: its lowering observes the exiled cards for a later card
/// pronoun of the same outcome.
fn exiles_another_object(effect: &super::LocalEffectNode) -> bool {
    match effect {
        super::LocalEffectNode::ZoneChange(zone_change) => {
            zone_change.action == super::ZoneChangeAction::Exile
        }
        super::LocalEffectNode::TargetCardZoneChange(zone_change) => {
            zone_change.action == super::CardZoneChangeAction::Exile
        }
        super::LocalEffectNode::SelectedPermanents(selected) => {
            selected.action == super::SelectedPermanentAction::Exile
        }
        super::LocalEffectNode::OptionalSequence(body) => {
            body.effects.last().is_some_and(exiles_another_object)
        }
        _ => false,
    }
}

/// The card selector of an ordinary card move of exactly one card onto the
/// battlefield (looking through an optional block), whose lowering always
/// binds the permanent it became; `None` for any other action, including
/// entries bound only when read in their own sequence (a library search, a
/// deployment, the source's own return).
fn always_bound_battlefield_entry(
    effect: &super::LocalEffectNode,
) -> Option<Option<TargetSelector>> {
    match effect {
        super::LocalEffectNode::CardAction(super::CardActionNode::Move { .. }) => {
            super::permanent_subject::moved_onto_battlefield_selector(effect)
        }
        super::LocalEffectNode::OptionalSequence(body) => {
            body.effects.last().and_then(always_bound_battlefield_entry)
        }
        _ => None,
    }
}

/// The card operand of the move a moved-onto-battlefield antecedent names,
/// looking through an optional block (`you may return it ...`).
fn moved_card_operand(effect: &super::LocalEffectNode) -> Option<&super::CardOperandNode> {
    match effect {
        super::LocalEffectNode::CardAction(super::CardActionNode::Move { cards, .. }) => {
            Some(cards)
        }
        super::LocalEffectNode::OptionalSequence(body) => {
            body.effects.last().and_then(moved_card_operand)
        }
        _ => None,
    }
}

/// The one declared party of a damage-prevention shield a later anaphor
/// names, with its party position (0: the protected recipient, 1: the
/// source whose damage is prevented, as `PreventDamageEffectNode::parties`
/// orders them): the protected target, else the targeted source when the
/// shield declares no protected target (`Prevent all damage target creature
/// would deal this turn. That creature ...`). The lowering binds exactly
/// this slot, so the parser and the lowering name the same declaration.
pub(in crate::oracle::canonical) fn prevention_declared_party_index(
    node: &super::PreventDamageEffectNode,
) -> Option<(usize, &super::TargetListNode)> {
    match (&node.recipient, &node.source) {
        (Some(super::DamagePartyNode::Targets(targets)), _) => Some((0, targets)),
        (_, Some(super::DamagePartyNode::Targets(targets))) if !node.recipient_also_source => {
            Some((1, targets))
        }
        _ => None,
    }
}

/// Publishes only the declared-target antecedent over `effects`, the
/// action being read included: a postposed condition (`Counter target spell
/// if its mana value is 2 or less`) reads the target its own action
/// declared, never that action's result (CR 601.2c, 608.2c).
pub(in crate::oracle::canonical) fn publish_installed_declared_target(
    cursor: &mut OracleTokenCursor<'_>,
    effects: &[super::LocalEffectNode],
) {
    let mut environment = installed_binding_environment(cursor);
    environment.declared_target = program_declared_target(effects);
    cursor.replace_binding_environment(Some(std::sync::Arc::new(environment)));
}

/// The latest exactly-one target slot whose selector does not guarantee a
/// battlefield permanent: the stack object of a counter or copy action, or
/// the recipient of damage or of a prevention shield written `any target`.
/// A later action that declares a target of its own is the nearer
/// declaration, so nothing earlier is published past it.
/// The recipient of the program's latest action when it dealt damage, as
/// a reference that declares nothing: one declared slot becomes the bound
/// target it is; a fixed or bound group is the same group. Several slots or
/// a variable slot have no single restatement.
fn restated_damage_recipient(
    effects: &[super::LocalEffectNode],
) -> Option<super::DamageRecipientNode> {
    let (effect_index, super::LocalEffectNode::DealDamage(damage)) =
        effects.iter().enumerate().last()?
    else {
        return None;
    };
    match &damage.recipient {
        super::DamageRecipientNode::Targets(targets) => match targets.slots.as_slice() {
            [slot] if slot.cardinality == super::TargetCardinality::EXACTLY_ONE => {
                // A targeted damage source occupies the action's first slot.
                (!matches!(damage.source, super::PermanentOperandNode::Targets(_))).then_some(
                    super::DamageRecipientNode::BoundObject(PermanentEffectReference::Target(
                        super::TargetBinding {
                            effect_index,
                            slot_index: 0,
                        },
                    )),
                )
            }
            _ => None,
        },
        super::DamageRecipientNode::All(groups)
            if groups
                .iter()
                .any(|group| matches!(group, super::DamageRecipientNode::Targets(_))) =>
        {
            None
        }
        recipient => Some(recipient.clone()),
    }
}

fn program_declared_target(effects: &[super::LocalEffectNode]) -> Option<DeclaredTargetAntecedent> {
    for (effect_index, effect) in effects.iter().enumerate().rev() {
        let targets = match effect {
            super::LocalEffectNode::CounterSpell(node) if !node.event_spell => Some(&node.target),
            super::LocalEffectNode::CopyStackObject(node) if !node.event_spell => {
                Some(&node.target)
            }
            // A targeted damage source occupies the action's first slot.
            super::LocalEffectNode::DealDamage(node)
                if !matches!(node.source, super::PermanentOperandNode::Targets(_)) =>
            {
                match &node.recipient {
                    super::DamageRecipientNode::Targets(targets) => Some(targets),
                    _ => None,
                }
            }
            super::LocalEffectNode::PreventDamage(node) => match &node.recipient {
                Some(super::DamagePartyNode::Targets(targets)) => Some(targets),
                _ => None,
            },
            _ => None,
        };
        if let Some(targets) = targets {
            return match targets.slots.as_slice() {
                [slot]
                    if slot.cardinality == super::TargetCardinality::EXACTLY_ONE
                        && !super::permanent_subject::selector_guarantees_permanent(
                            &slot.selector,
                        ) =>
                {
                    Some(DeclaredTargetAntecedent {
                        binding: super::TargetBinding {
                            effect_index,
                            slot_index: 0,
                        },
                        selector: slot.selector.clone(),
                    })
                }
                _ => None,
            };
        }
        if super::permanent_subject::target_slot_selector(effect, 0).is_some() {
            return None;
        }
    }
    None
}

/// Copies the action result the program parent published on the cursor
/// into an environment a context parent built (`for_permanent_context`),
/// so its references see the same antecedents as the program's.
pub(in crate::oracle::canonical) fn inherit_installed_action_result(
    environment: &mut BindingEnvironment,
    cursor: &OracleTokenCursor<'_>,
) {
    if environment.result_object.is_none() {
        environment.result_object = installed_binding_environment(cursor).result_object;
    }
}

/// The same producer contract over the environment the program parent just
/// installed on the cursor for the action it reads next.
pub(in crate::oracle::canonical) fn publish_installed_program_antecedents(
    cursor: &mut OracleTokenCursor<'_>,
    effects: &[super::LocalEffectNode],
) {
    let environment = publish_program_antecedents(installed_binding_environment(cursor), effects);
    cursor.replace_binding_environment(Some(std::sync::Arc::new(environment)));
}

/// True when a target selector admits a player as one of its alternatives.
fn selector_admits_player(selector: &TargetSelector) -> bool {
    match selector {
        TargetSelector::Player(_) => true,
        TargetSelector::Union(branches) => branches.iter().any(selector_admits_player),
        TargetSelector::Any
        | TargetSelector::Object(_)
        | TargetSelector::ControlledObject { .. }
        | TargetSelector::StackItem(_) => false,
    }
}

/// The latest exactly-one target slot of a damage recipient whose selector
/// admits a player (`deals 3 damage to target player or planeswalker`).
/// `any target` is not published: it may be a creature, so `that player`
/// does not name it.
fn program_player_target(effects: &[super::LocalEffectNode]) -> Option<PlayerTargetAntecedent> {
    effects
        .iter()
        .enumerate()
        .rev()
        .find_map(|(effect_index, effect)| match effect {
            super::LocalEffectNode::DealDamage(damage) => match &damage.recipient {
                super::DamageRecipientNode::Targets(targets) => match targets.slots.as_slice() {
                    [slot]
                        if slot.cardinality == super::TargetCardinality::EXACTLY_ONE
                            && selector_admits_player(&slot.selector) =>
                    {
                        Some(PlayerTargetAntecedent {
                            binding: super::TargetBinding {
                                effect_index,
                                slot_index: 0,
                            },
                            selector: slot.selector.clone(),
                        })
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
}

/// An object set an earlier action of the same program declared or produced
/// (CR 608.2c): every object of one plural target slot, or the tokens one
/// creation action created. Its number is the one the producer stated, so a
/// singular anaphor never names several objects and a plural one never
/// names a single declared object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ObjectSetAntecedent {
    /// What the set denotes when a later reference names it.
    pub denotes: ObjectSetDenotation,
    /// The declared selector, or the card types the created tokens have, a
    /// nominal anaphor must follow from. `None` when the producer states no
    /// type (a token copy of an object).
    pub selector: Option<TargetSelector>,
    /// `Singular` only when the producer stated exactly one object (`create a
    /// token`); a declared slot of several targets is `Plural`.
    pub number: GrammaticalNumber,
    /// The set is created tokens, so `token`/`tokens` names it.
    pub tokens: bool,
}

/// What an object set denotes when a later reference names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectSetDenotation {
    /// A declared target set (`TargetSet(slot)`) or the created tokens
    /// (`Observed(CreatedTokens)`), recorded by the runtime.
    Bound(PermanentEffectReference),
    /// The members of a battlefield collection an action just affected,
    /// read again by the same query: the preceding action changes none of
    /// the characteristics the query tests (`prior_object_set` proves it),
    /// so no object joins or leaves the set between the two actions
    /// (CR 608.2c; nothing else happens during the resolution).
    Collection {
        query: super::ObjectQueryNode,
        exclude_source: bool,
    },
}

impl ObjectSetDenotation {
    fn bound(&self) -> Option<PermanentEffectReference> {
        match self {
            Self::Bound(reference) => Some(*reference),
            Self::Collection { .. } => None,
        }
    }

    /// The permanent operand a reference to the set reads.
    fn operand(&self) -> super::PermanentOperandNode {
        match self {
            Self::Bound(reference) => super::PermanentOperandNode::BoundReference(*reference),
            Self::Collection {
                query,
                exclude_source,
            } => super::PermanentOperandNode::Collection {
                objects: query.clone(),
                exclude_source: *exclude_source,
            },
        }
    }
}

impl BindingEnvironment {
    /// The environment of a reference read directly by a context parent: its
    /// frame bindings and its proven permanent antecedent.
    pub(in crate::oracle::canonical) fn for_permanent_context(
        context: &EffectRecognitionContext,
    ) -> Self {
        Self {
            frame: FrameBindings::from_context(context),
            local_object_scope: matches!(
                context.permanent_subject_scope,
                PermanentSubjectScope::Local(_)
            ),
            object: super::permanent_subject::proven_read_antecedent(context).map(|reference| {
                ObjectSubjectAntecedent {
                    reference,
                    selector: match reference {
                        PermanentEffectReference::Target(_)
                        | PermanentEffectReference::Observed(
                            TypedObservedEventObject::MovedOntoBattlefield,
                        ) => context.permanent_subject_selector.clone(),
                        _ => None,
                    },
                }
            }),
            player: None,
            subject: None,
            effect_index: context.current_effect_index,
            action_result: None,
            number: None,
            objects: None,
            player_target: None,
            result_object: None,
            declared_target: None,
            restated_damage_recipient: None,
            card_target: None,
            unless_payer: None,
            acting_player: false,
        }
    }
}

/// Reads with `environment` installed on the cursor, then restores the
/// enclosing environment. Every slot installer of this module uses it.
fn with_installed_environment<T>(
    cursor: &mut OracleTokenCursor<'_>,
    environment: BindingEnvironment,
    read: impl FnOnce(&mut OracleTokenCursor<'_>) -> T,
) -> T {
    let enclosing = cursor.replace_binding_environment(Some(std::sync::Arc::new(environment)));
    let result = read(cursor);
    cursor.replace_binding_environment(enclosing);
    result
}

/// What an outcome connector observes about its producer (CR 608.2c,
/// 603.12): whether the preceding instruction or cost was actually
/// performed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PerformanceOutcome {
    /// `If you do, …`: the body resolves only if the producer was performed.
    Performed,
    /// `If you don't, …`: the body resolves only if it was not performed.
    Declined,
    /// `When you do, …`: performing it creates a reflexive triggered ability
    /// that chooses its own targets (CR 603.12).
    Reflexive,
}

/// Who performs the producer an outcome connector names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum OutcomePerformer {
    /// `you may …`: the ability controller (`If you do`).
    Controller,
    /// `that player may …`: another player the environment names (`If they
    /// do`, `If that player does`, `If the player does`).
    OtherPlayer,
    /// A mandatory instruction: its performer is the instruction's own
    /// subject, which the connector's subject does not restate.
    Instruction,
}

/// The ActionResult antecedent of an outcome connector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActionResultSlot {
    pub performer: OutcomePerformer,
}

/// The grammatical subject of an outcome connector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConnectorSubject {
    You,
    OtherPlayer,
}

impl ActionResultSlot {
    fn admits(self, subject: ConnectorSubject) -> bool {
        match self.performer {
            OutcomePerformer::Controller => subject == ConnectorSubject::You,
            OutcomePerformer::OtherPlayer => subject == ConnectorSubject::OtherPlayer,
            OutcomePerformer::Instruction => true,
        }
    }
}

/// Lexical reading of `if|when SUBJECT do(es)|don't|doesn't ,` at `offset`:
/// the outcome, its subject and the connector's token width. It proves no
/// binding; `recognize_outcome_connector` resolves the subject.
fn outcome_connector_parts(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(PerformanceOutcome, ConnectorSubject, usize)> {
    let word = |index: usize| {
        cursor
            .peek_ahead(offset + index)
            .map(str::to_ascii_lowercase)
    };
    // The connector's subject is the producer's performer: `you`, or another
    // player named by `they` / `that player` / `the player` (CR 608.2c).
    let (subject, subject_width, singular) = match (word(1)?.as_str(), word(2).as_deref()) {
        ("you", _) => (ConnectorSubject::You, 1, false),
        ("they", _) => (ConnectorSubject::OtherPlayer, 1, false),
        ("that" | "the", Some("player")) => (ConnectorSubject::OtherPlayer, 2, true),
        _ => return None,
    };
    let verb = word(1 + subject_width)?;
    let outcome = match (word(0)?.as_str(), verb.as_str(), singular) {
        ("if", "do", false) | ("if", "does", true) => PerformanceOutcome::Performed,
        ("if", "don't" | "don’t", false) | ("if", "doesn't" | "doesn’t", true) => {
            PerformanceOutcome::Declined
        }
        ("when", "do", false) | ("when", "does", true) => PerformanceOutcome::Reflexive,
        _ => return None,
    };
    let width = 2 + subject_width;
    (word(width)?.as_str() == ",").then_some((outcome, subject, width + 1))
}

/// Lexical lookahead: the outcome and token width of a connector at
/// `offset`. It commits nothing.
pub(in crate::oracle::canonical) fn outcome_connector_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(PerformanceOutcome, usize)> {
    outcome_connector_parts(cursor, offset).map(|(outcome, _, width)| (outcome, width))
}

/// True when the first sentence after the cursor is followed by an outcome
/// connector. Lexical lookahead only; it commits nothing.
pub(in crate::oracle::canonical) fn outcome_connector_follows_first_sentence(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let mut offset = 0;
    while let Some(raw) = cursor.peek_ahead(offset) {
        if raw == "." {
            return outcome_connector_at(cursor, offset + 1).is_some();
        }
        offset += 1;
    }
    false
}

/// Reads `If you do ,` / `If they don't ,` / `When that player does ,` at a
/// sentence start and resolves it against the environment's ActionResult
/// antecedent: the connector's subject must name the producer's performer.
pub(in crate::oracle::canonical) fn recognize_outcome_connector(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PerformanceOutcome> {
    cursor.enter("cost.outcome.connector");
    let (outcome, subject, width) = outcome_connector_parts(cursor, 0)
        .ok_or_else(|| cursor.fail("`If you do,`, `If you don't,` or `When you do,`"))?;
    cursor.enter("binding.action-result.performer");
    let slot = installed_binding_environment(cursor)
        .action_result
        .ok_or_else(|| cursor.fail("a preceding instruction or cost for the outcome connector"))?;
    cursor.require(
        slot.admits(subject),
        "an outcome connector whose subject names the performer of the preceding instruction",
    )?;
    cursor.advance_to(cursor.position() + width)?;
    Ok(outcome)
}

/// Reads an outcome connector with `slot` as the ActionResult antecedent.
pub(in crate::oracle::canonical) fn recognize_outcome_connector_after(
    cursor: &mut OracleTokenCursor<'_>,
    slot: ActionResultSlot,
) -> MachineResult<PerformanceOutcome> {
    let mut environment = installed_binding_environment(cursor);
    environment.action_result = Some(slot);
    with_installed_environment(cursor, environment, recognize_outcome_connector)
}

/// Lexical lookahead for `that number`.
pub(in crate::oracle::canonical) fn prior_number_reference_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("number"))
}

/// `that number`: the number the enclosing frame already read (a power the
/// toughness reuses). An absent binding fails; it never becomes a literal.
pub(in crate::oracle::canonical) fn recognize_prior_number_reference(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.prior-number-anaphor.demonstrative");
    cursor.consume_phrase(&["that", "number"])?;
    cursor.enter("numeric.prior-number-anaphor.resolve");
    installed_binding_environment(cursor)
        .number
        .ok_or_else(|| cursor.fail("a prior numeric-expression binding for `that number`"))
}

/// Reads with `number` as the environment's prior number.
pub(in crate::oracle::canonical) fn with_prior_number<T>(
    cursor: &mut OracleTokenCursor<'_>,
    number: NumericExpression,
    read: impl FnOnce(&mut OracleTokenCursor<'_>) -> T,
) -> T {
    let mut environment = installed_binding_environment(cursor);
    environment.number = Some(number);
    with_installed_environment(cursor, environment, read)
}

/// A player named through the environment, never a new declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum BoundPlayerReference {
    /// `that player` after an earlier `target player` of the same program.
    Bound(PlayerBinding),
    /// `that player` / `defending player`: the player the event records.
    EventPlayer,
    /// `its controller` / `that NOUN's owner` (CR 108.3, 109.4).
    ObjectRelation {
        relation: ObjectPlayerRelation,
        object: PermanentEffectReference,
    },
    /// `that spell's controller`: the controller of the spell the cast
    /// record binds (CR 110.2), while it is on the stack, else its caster.
    EventSpellController,
    /// `defending player` in a static combat restriction: the defending
    /// player of the combat the restriction is checked for (CR 506.2,
    /// 508.1c, 509.1b). Only a condition of that frame reads it.
    DefendingPlayer,
    /// A distributive `they` / `that player` inside an operand of an action
    /// whose player slot names it (`BindingEnvironment::acting_player`):
    /// the player who loses the life, draws, or is dealt the damage, each
    /// member of a population in turn. The lowering substitutes that slot.
    ActingPlayer,
}

impl BoundPlayerReference {
    /// The effect-subject form of the reference; a spell's controller has no
    /// effect-subject lowering yet.
    pub(in crate::oracle::canonical) fn into_effect_subject(self) -> Option<PlayerEffectSubject> {
        match self {
            Self::Bound(binding) => Some(PlayerEffectSubject::Bound(binding)),
            Self::EventPlayer => Some(PlayerEffectSubject::EventPlayer),
            Self::ObjectRelation { relation, object } => {
                Some(PlayerEffectSubject::ObjectRelation { relation, object })
            }
            Self::EventSpellController | Self::DefendingPlayer | Self::ActingPlayer => None,
        }
    }
}

fn player_relation(raw: &str) -> Option<ObjectPlayerRelation> {
    if raw.eq_ignore_ascii_case("controller") {
        Some(ObjectPlayerRelation::Controller)
    } else if raw.eq_ignore_ascii_case("owner") {
        Some(ObjectPlayerRelation::Owner)
    } else {
        None
    }
}

/// A registered card type or object domain nominal (`creature`, `land`,
/// `permanent`, `spell`), read from a bare or possessive surface.
fn registered_nominal(surface: &str) -> Option<TargetNominal> {
    match lookup_object_selector_lexeme(surface) {
        Some(CharacteristicLexeme::CardType(value)) => Some(TargetNominal::CardType(value)),
        Some(CharacteristicLexeme::ObjectDomain(value)) => Some(TargetNominal::Domain(value)),
        _ => None,
    }
}

/// The registered nominal of a `this NOUN` source surface at the cursor
/// (`this creature`), before it is read. Lexical lookahead only.
pub(in crate::oracle::canonical) fn this_object_nominal_at(
    cursor: &OracleTokenCursor<'_>,
) -> Option<TargetNominal> {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
        .then(|| {
            cursor
                .token_slice(cursor.position() + 1, cursor.position() + 2)
                .first()
                .filter(|token| token.token_type != "possessiveTypeWord")
                .and_then(|token| registered_nominal(&token.raw))
        })
        .flatten()
}

fn possessive_base(raw: &str) -> Option<String> {
    let normalized = raw.to_ascii_lowercase();
    normalized
        .strip_suffix("'s")
        .or_else(|| normalized.strip_suffix("\u{2019}s"))
        .map(str::to_string)
}

/// The registered nominal of a possessive word (`creature's`, `spell’s`):
/// the one genitive-nominal reader every reference child shares.
pub(in crate::oracle::canonical) fn possessive_nominal(raw: &str) -> Option<TargetNominal> {
    possessive_base(raw).and_then(|surface| registered_nominal(&surface))
}

pub(in crate::oracle::canonical) fn defending_player_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("defending"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("player"))
}

fn that_player_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("player"))
}

/// Bounded lexical head over tokens: `its controller|owner` or `that NOUN's
/// controller|owner`, with its token length and relation. It proves no
/// binding; the resolver below does.
pub(in crate::oracle::canonical) fn object_related_player_head(
    tokens: &[RecognizedEntity],
) -> Option<(usize, ObjectPlayerRelation)> {
    let raw = |index: usize| tokens.get(index).map(|token| token.raw.as_str());
    let first = raw(0)?;
    if first.eq_ignore_ascii_case("its") {
        return player_relation(raw(1)?).map(|relation| (2, relation));
    }
    // `that land's controller`, or the definite `the land's controller`,
    // which names the same antecedent (`recognize_definite_object_reference`).
    if (first.eq_ignore_ascii_case("that") || first.eq_ignore_ascii_case("the"))
        && tokens
            .get(1)
            .is_some_and(|token| token.token_type == "possessiveTypeWord")
    {
        return player_relation(raw(2)?).map(|relation| (3, relation));
    }
    None
}

/// The token length of an object-related player head at the cursor.
pub(in crate::oracle::canonical) fn object_related_player_head_length(
    cursor: &OracleTokenCursor<'_>,
) -> Option<usize> {
    object_related_player_head(cursor.token_slice(cursor.position(), cursor.position() + 3))
        .map(|(length, _)| length)
}

/// Lexical lookahead for any player reference this resolver owns.
pub(in crate::oracle::canonical) fn bound_player_reference_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    defending_player_starts(cursor)
        || that_player_starts(cursor)
        || object_related_player_head_length(cursor).is_some()
}

/// Resolves `defending player`, `that player`, `its controller|owner` and
/// `that NOUN's controller|owner` against the environment.
pub(in crate::oracle::canonical) fn recognize_bound_player_reference(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<BoundPlayerReference> {
    if defending_player_starts(cursor) {
        return recognize_defending_player(cursor, environment.frame.defending_player);
    }
    if that_player_starts(cursor) {
        return recognize_that_player(cursor, environment);
    }
    // Without a lexical object antecedent or an event subject, `its
    // controller|owner` in a clause's argument names that clause's singular
    // subject (`enchanted creature deals 1 damage to its owner`), as a
    // possessive `its` does. An event subject keeps its own reading.
    let subject = (environment.object.is_none()
        && environment
            .frame
            .event_object
            .as_ref()
            .is_none_or(|slot| !slot.subject))
    .then_some(environment.subject)
    .flatten()
    .filter(|reference| singular_reference(*reference))
    .map(|reference| ObjectSubjectAntecedent {
        reference,
        selector: None,
    });
    recognize_object_related_player(
        cursor,
        environment.object.as_ref().or(subject.as_ref()),
        environment.frame.event_object.as_ref(),
        environment.frame.event_spell,
    )
}

/// `defending player` in an attack trigger (CR 506.2): the attacked player
/// the event records. Elsewhere it has no binding and fails explicitly.
pub(in crate::oracle::canonical) fn recognize_defending_player(
    cursor: &mut OracleTokenCursor<'_>,
    available: bool,
) -> MachineResult<BoundPlayerReference> {
    cursor.enter("player-subject.defending-player");
    let first = cursor
        .peek_token()
        .cloned()
        .ok_or_else(|| cursor.fail("`defending player`"))?;
    cursor.consume_phrase(&["defending", "player"])?;
    if !available {
        return Err(cursor.fail_with_span(
            "an attack trigger event that records the defending player",
            first.index,
            first.span_start,
            first.span_end,
            first.raw,
        ));
    }
    cursor.enter("player-subject.complete");
    Ok(BoundPlayerReference::EventPlayer)
}

/// Lexical lookahead for the player subject of a condition (`that player`,
/// `defending player`).
pub(in crate::oracle::canonical) fn condition_player_reference_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    defending_player_starts(cursor) || that_player_starts(cursor)
}

/// `that player` / `defending player` as the subject of a condition: the
/// one resolver of a condition's bound player. In an attack or block
/// trigger the defending player is the player the event records; in a
/// static combat restriction it is the defending player of the combat the
/// restriction is checked for. Anywhere else it fails explicitly.
pub(in crate::oracle::canonical) fn recognize_condition_player_reference(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<BoundPlayerReference> {
    if !defending_player_starts(cursor) {
        return recognize_that_player(cursor, environment);
    }
    if environment.frame.combat_defending_player && !environment.frame.defending_player {
        cursor.enter("player-subject.defending-player");
        cursor.consume_phrase(&["defending", "player"])?;
        cursor.enter("player-subject.complete");
        return Ok(BoundPlayerReference::DefendingPlayer);
    }
    recognize_defending_player(cursor, environment.frame.defending_player)
}

/// Reads with the static combat-restriction frame installed: a condition
/// may name the defending player of the combat being checked.
pub(in crate::oracle::canonical) fn with_combat_defending_player<T>(
    cursor: &mut OracleTokenCursor<'_>,
    read: impl FnOnce(&mut OracleTokenCursor<'_>) -> T,
) -> T {
    let mut environment = installed_binding_environment(cursor);
    environment.frame.combat_defending_player = true;
    with_installed_environment(cursor, environment, read)
}

/// True when an atom of `condition` reads the defending player of a
/// combat (a static combat restriction checks it per combat).
pub(in crate::oracle::canonical) fn condition_reads_defending_player(
    condition: &super::ConditionNode,
) -> bool {
    match condition {
        super::BooleanConditionNode::Atom(super::ConditionAtom::Compare(comparison)) => {
            comparison.subject == super::ConditionPlayerSubject::DefendingPlayer
        }
        super::BooleanConditionNode::Atom(_) => false,
        super::BooleanConditionNode::Not(inner) => condition_reads_defending_player(inner),
        super::BooleanConditionNode::And(operands) | super::BooleanConditionNode::Or(operands) => {
            operands.iter().any(condition_reads_defending_player)
        }
    }
}

/// `that player`: the exact earlier player binding of the program, or,
/// without one, the player of a triggering event that names one. It never
/// opens a new target declaration.
pub(in crate::oracle::canonical) fn recognize_that_player(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<BoundPlayerReference> {
    let first = cursor
        .peek_token()
        .cloned()
        .ok_or_else(|| cursor.fail("`that player`"))?;
    cursor.enter("player-subject.bound.reference");
    cursor.consume_phrase(&["that", "player"])?;
    cursor.enter("player-subject.bound.resolve");
    // The latest lexical declaration wins (CR 608.2c): a target that admits
    // a player, unless a player declared later, or the payer of a later
    // `unless` frame, is the program's antecedent.
    if let Some(target) = environment.player_target.as_ref().filter(|target| {
        environment
            .player
            .is_none_or(|player| target.binding.effect_index > player.0)
            && environment
                .unless_payer
                .is_none_or(|payer| target.binding.effect_index >= payer.first_effect)
    }) {
        return recognize_player_target_reference(cursor, target);
    }
    if disjunctive_controller_follows(cursor) {
        cursor.enter("player-subject.bound.disjunction");
        return Err(cursor.fail(
            "a declared target that may be a player or a permanent for `that player or that NOUN's controller`",
        ));
    }
    // `Counter target spell unless its controller pays {1}. That player
    // discards a card.`: the payer the `unless` frame named (CR 118.12),
    // unless a player declared after that frame is nearer.
    if let Some(payer) = environment.unless_payer.filter(|payer| {
        environment
            .player
            .is_none_or(|player| player.0 < payer.first_effect)
    }) {
        cursor.enter("player-subject.unless-payer");
        cursor.enter("player-subject.complete");
        return Ok(payer.payer);
    }
    if let Some(binding) = environment.player {
        cursor.enter("player-subject.complete");
        return Ok(BoundPlayerReference::Bound(binding));
    }
    if environment.frame.event_player {
        cursor.enter("player-subject.event-player");
        cursor.enter("player-subject.complete");
        return Ok(BoundPlayerReference::EventPlayer);
    }
    // `Destroy target creature an opponent controls. That player loses 3
    // life.` / `Whenever a creature an opponent controls dies, that player
    // ...`: the other player the object's controller relation named.
    if let Some(object) = controlled_object_player(environment) {
        cursor.enter("player-subject.object-controller");
        cursor.enter("player-subject.complete");
        return Ok(BoundPlayerReference::ObjectRelation {
            relation: ObjectPlayerRelation::Controller,
            object,
        });
    }
    // `deals damage to each player equal to the number of lands that player
    // controls`: the player of the action whose operand is being read.
    if environment.acting_player {
        cursor.enter("player-subject.acting-player");
        cursor.enter("player-subject.complete");
        return Ok(BoundPlayerReference::ActingPlayer);
    }
    Err(cursor.fail_with_span(
        "a prior player binding for `that player`",
        first.index,
        first.span_start,
        first.span_end,
        first.raw,
    ))
}

/// Lexical lookahead after `that player`: `or that NOUN's controller`.
fn disjunctive_controller_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
        && cursor
            .token_slice(cursor.position() + 2, cursor.position() + 3)
            .first()
            .is_some_and(|token| token.token_type == "possessiveTypeWord")
        && cursor
            .peek_ahead(3)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("controller"))
}

/// `that player` naming a declared target that admits a player, after
/// `that player` was read. A player-only selector (`target player`,
/// `target opponent`) is named by `that player` alone; a selector that also
/// admits permanents (`target player or planeswalker`) is named only by the
/// disjunction `that player or that NOUN's controller` whose nominal names
/// every permanent branch (CR 109.4). Either is the controller relation of
/// the declared object: the player itself, or the permanent's controller.
fn recognize_player_target_reference(
    cursor: &mut OracleTokenCursor<'_>,
    target: &PlayerTargetAntecedent,
) -> MachineResult<BoundPlayerReference> {
    let reference = BoundPlayerReference::ObjectRelation {
        relation: ObjectPlayerRelation::Controller,
        object: PermanentEffectReference::Target(target.binding),
    };
    let branches = match &target.selector {
        TargetSelector::Player(_) => {
            cursor.enter("player-subject.bound.disjunction");
            cursor.require(
                !disjunctive_controller_follows(cursor),
                "a declared target that may be a permanent for `that player or that NOUN's controller`",
            )?;
            cursor.enter("player-subject.complete");
            return Ok(reference);
        }
        TargetSelector::Union(branches) => branches,
        _ => {
            return Err(cursor.fail("a declared target that may be a player for `that player`"));
        }
    };
    cursor.enter("player-subject.bound.disjunction");
    cursor.require(
        disjunctive_controller_follows(cursor),
        "`that player or that NOUN's controller` for a target that may be a player or a permanent",
    )?;
    cursor.consume_phrase(&["or", "that"])?;
    let possessive = cursor.consume_type("possessiveTypeWord")?;
    let nominal = possessive_nominal(&possessive.raw)
        .ok_or_else(|| cursor.fail("a registered card type or object domain nominal"))?;
    cursor.consume("controller")?;
    cursor.enter("player-subject.bound.disjunction.branches");
    cursor.require(
        branches.iter().all(|branch| {
            matches!(branch, TargetSelector::Player(_))
                || selector_names_nominals(branch, &[nominal])
        }),
        "a nominal naming every permanent alternative of the declared target",
    )?;
    cursor.enter("player-subject.complete");
    Ok(reference)
}

/// A controller relation that names a player other than the ability's
/// controller (`an opponent controls`, `you don't control`).
fn names_other_player(scope: super::ObjectControllerScope) -> bool {
    matches!(
        scope,
        super::ObjectControllerScope::NotAbilityController
            | super::ObjectControllerScope::OpponentOfAbilityController
    )
}

/// The singular object whose controller relation named another player:
/// the program's antecedent declared `target NOUN an opponent controls`,
/// else the event's object written `a NOUN an opponent controls`. Its
/// controller, live or as last known information, is `that player`.
fn controlled_object_player(environment: &BindingEnvironment) -> Option<PermanentEffectReference> {
    let event = environment
        .frame
        .event_object
        .as_ref()
        .filter(|slot| {
            slot.controller.is_some_and(names_other_player) && relation_supported(slot.reference)
        })
        .map(|slot| slot.reference);
    let Some(antecedent) = environment.object.as_ref() else {
        return event;
    };
    other_player_controlled_antecedent(antecedent).or_else(|| {
        // The lexical antecedent is the event's own object (`it` after `a
        // land an opponent controls enters`).
        event.filter(|reference| *reference == antecedent.reference)
    })
}

/// The one reader of a singular object antecedent declared among the
/// permanents another player controls (`target creature an opponent
/// controls`, `... you don't control`, CR 109.4): its controller is the
/// player `that player` names. Shared by the binding environment and the
/// clause layer's performer slot.
pub(in crate::oracle::canonical) fn other_player_controlled_antecedent(
    antecedent: &ObjectSubjectAntecedent,
) -> Option<PermanentEffectReference> {
    match antecedent.selector.as_ref() {
        Some(TargetSelector::ControlledObject { controller, .. })
            if names_other_player(*controller)
                && singular_reference(antecedent.reference)
                && relation_supported(antecedent.reference) =>
        {
            Some(antecedent.reference)
        }
        _ => None,
    }
}

/// `its controller|owner` / `that NOUN's controller|owner` (CR 108.3,
/// 109.4). The object is the environment's antecedent; a nominal must follow
/// from the selector that declared that target. Nothing is guessed.
pub(in crate::oracle::canonical) fn recognize_object_related_player(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<&ObjectSubjectAntecedent>,
    event_object: Option<&EventObjectSlot>,
    event_spell: bool,
) -> MachineResult<BoundPlayerReference> {
    cursor.enter("player-subject.object-relation");
    let first = cursor
        .peek_token()
        .cloned()
        .ok_or_else(|| cursor.fail("`its` or `that NOUN's`"))?;
    let nominal = if first.raw.eq_ignore_ascii_case("its") {
        cursor.consume("its")?;
        None
    } else {
        cursor.consume(if first.raw.eq_ignore_ascii_case("the") {
            "the"
        } else {
            "that"
        })?;
        cursor.enter("player-subject.object-relation.nominal");
        let possessive = cursor.consume_type("possessiveTypeWord")?;
        let surface = possessive_base(&possessive.raw)
            .ok_or_else(|| cursor.fail("a possessive registered object nominal"))?;
        Some(
            registered_nominal(&surface)
                .ok_or_else(|| cursor.fail("a registered card type or object domain nominal"))?,
        )
    };
    cursor.enter("player-subject.object-relation.relation");
    let relation = cursor
        .peek()
        .and_then(player_relation)
        .ok_or_else(|| cursor.fail("`controller` or `owner`"))?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("player-subject.object-relation.antecedent");
    // `that spell's controller`: the spell the cast record binds.
    if event_spell
        && nominal == Some(TargetNominal::Domain(super::ObjectKind::Spell))
        && relation == ObjectPlayerRelation::Controller
    {
        cursor.enter("player-subject.complete");
        return Ok(BoundPlayerReference::EventSpellController);
    }
    // `that NOUN's`: a lexical antecedent with that nominal, else the event's
    // object when the event wrote that nominal for it.
    if let Some(nominal) = nominal
        && !antecedent.is_some_and(|antecedent| lexical_antecedent_implies(antecedent, nominal))
        && let Some(slot) = event_object.filter(|slot| slot.implies(nominal))
        && relation_supported(slot.reference)
    {
        cursor.enter("player-subject.complete");
        return Ok(BoundPlayerReference::ObjectRelation {
            relation,
            object: slot.reference,
        });
    }
    // `its controller` of a departed event subject (`a creature dies`): the
    // object is no permanent antecedent any more, but its last known
    // controller and owner still answer the relation (CR 608.2h).
    if nominal.is_none()
        && antecedent.is_none()
        && let Some(slot) = event_object.filter(|slot| slot.subject && slot.departed)
        && relation_supported(slot.reference)
    {
        cursor.enter("player-subject.complete");
        return Ok(BoundPlayerReference::ObjectRelation {
            relation,
            object: slot.reference,
        });
    }
    // No lexical antecedent proves the object: the object the frame binds
    // that the one shared reference resolver names (`player_subject.rs`).
    if let Some(object) =
        super::source_sacrifice::atomic_action::player_subject::unbound_relation_object(
            cursor, antecedent, nominal,
        )
    {
        cursor.enter("player-subject.complete");
        return Ok(BoundPlayerReference::ObjectRelation { relation, object });
    }
    let antecedent = antecedent.ok_or_else(|| {
        cursor.fail_with_span(
            "a preceding singular object binding for the possessive",
            first.index,
            first.span_start,
            first.span_end,
            first.raw.clone(),
        )
    })?;
    cursor.require(
        relation_supported(antecedent.reference),
        "a declared target, the source, its attached permanent or an event object as the possessive antecedent",
    )?;
    if let Some(nominal) = nominal {
        cursor.require(
            lexical_antecedent_implies(antecedent, nominal),
            "a nominal implied by the declared selector of the preceding target",
        )?;
    }
    cursor.enter("player-subject.complete");
    Ok(BoundPlayerReference::ObjectRelation {
        relation,
        object: antecedent.reference,
    })
}

/// The objects whose controller and owner the runtime reads, live or as
/// last known information: declared targets, the source, its attached
/// permanent, the record's permanent and the entering one.
fn relation_supported(reference: PermanentEffectReference) -> bool {
    match reference {
        PermanentEffectReference::Observed(observed) => matches!(
            observed,
            TypedObservedEventObject::EventPermanent | TypedObservedEventObject::EnteringPermanent
        ),
        // A set of declared targets has no single controller or owner.
        PermanentEffectReference::TargetSet(_) => false,
        // An action result's player relations are read only while it is
        // still a permanent; no last known controller is recorded for it.
        PermanentEffectReference::ActionResult(result) => result.permanent,
        _ => true,
    }
}

fn lexical_antecedent_implies(
    antecedent: &ObjectSubjectAntecedent,
    nominal: TargetNominal,
) -> bool {
    antecedent_names(antecedent, &[nominal])
}

/// The failure states of a reference read without any antecedent in its
/// lexical scope: a nominal or plural anaphor, a bound player, a possessive
/// player relation, a condition pronoun, a genitive property, or an
/// `instead` with no replaced sentence before it. Only these may be read
/// again in an enclosing scope (a labeled spell paragraph continuing the
/// paragraph before it, CR 207.2c, 608.2c); every other failure is the
/// reference's own.
pub(in crate::oracle::canonical) fn unresolved_antecedent_state(state: &str) -> bool {
    matches!(
        state,
        "permanent.subject.nominal-anaphor.antecedent"
            | "permanent.subject.object-set-anaphor.antecedent"
            | "player-subject.bound.resolve"
            | "player-subject.object-relation.antecedent"
            | "condition.sentence.object.antecedent"
            | "condition.sentence.object.card-antecedent"
            | "numeric.genitive.antecedent"
            | "condition.sentence.instead"
    )
}

/// Lexical lookahead for `it` / `that NOUN` (a registered nominal that is
/// not possessive). It proves no binding.
pub(in crate::oracle::canonical) fn bound_object_reference_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(super::reference::is_source_anaphor)
        || cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
            && cursor
                .token_slice(cursor.position() + 1, cursor.position() + 2)
                .first()
                .is_some_and(|token| {
                    token.token_type != "possessiveTypeWord"
                        && registered_nominal(&token.raw).is_some()
                })
}

/// Resolves `it` / `he` / `she` or `that NOUN` against the environment.
pub(in crate::oracle::canonical) fn recognize_bound_object_reference(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<PermanentEffectReference> {
    if cursor
        .peek()
        .is_some_and(super::reference::is_source_anaphor)
    {
        let reference =
            resolve_pronoun_antecedent(environment).map_err(|expected| cursor.fail(expected))?;
        let pronoun = cursor.peek().expect("checked object anaphor").to_owned();
        cursor.consume(&pronoun)?;
        return Ok(reference);
    }
    recognize_nominal_object_reference(cursor, environment)
}

/// `that creature` / `that permanent`: a nominal anaphor. Without a lexical
/// antecedent a block event's other combatant is the creature named (every
/// blocker and blocked object is a creature); otherwise the nominal must
/// follow from the selector that declared the antecedent target.
pub(in crate::oracle::canonical) fn recognize_nominal_object_reference(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<PermanentEffectReference> {
    recognize_determined_object_reference(cursor, environment, "that", false)
}

/// `that creature` read for its last known information (`If that creature
/// was a Villain`, CR 608.2h): unlike an operand, it may name an event
/// object that left the battlefield.
pub(in crate::oracle::canonical) fn recognize_nominal_object_read(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<PermanentEffectReference> {
    recognize_determined_object_reference(cursor, environment, "that", true)
}

/// `the creature` (a definite description, CR 702.32a style): it names the
/// same antecedent `that creature` would, validated against the same
/// declared selector; it never names a new object.
pub(in crate::oracle::canonical) fn recognize_definite_object_reference(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<PermanentEffectReference> {
    recognize_determined_object_reference(cursor, environment, "the", false)
}

/// The registered nominals of a disjunctive nominal phrase starting `offset`
/// tokens after the cursor (`creature`, `creature or planeswalker`), with
/// its token width. Lexical lookahead only; it proves no binding.
fn nominal_disjunction_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(Vec<TargetNominal>, usize)> {
    let nominal_at = |index: usize| {
        cursor
            .token_slice(cursor.position() + index, cursor.position() + index + 1)
            .first()
            .filter(|token| token.token_type != "possessiveTypeWord")
            .and_then(|token| registered_nominal(&token.raw))
    };
    let mut nominals = vec![nominal_at(offset)?];
    let mut width = 1;
    // `that creature or planeswalker`: each further nominal follows `or`.
    while cursor
        .peek_ahead(offset + width)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
        && let Some(next) = nominal_at(offset + width + 1)
    {
        nominals.push(next);
        width += 2;
    }
    Some((nominals, width))
}

fn recognize_determined_object_reference(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
    determiner: &str,
    departed_readable: bool,
) -> MachineResult<PermanentEffectReference> {
    cursor.enter("permanent.subject.nominal-anaphor");
    cursor.consume(determiner)?;
    cursor.enter("permanent.subject.nominal-anaphor.nominal");
    let (nominals, width) = nominal_disjunction_at(cursor, 0)
        .ok_or_else(|| cursor.fail("a registered card type or permanent nominal"))?;
    cursor.enter("permanent.subject.nominal-anaphor.antecedent");
    let reference = resolve_nominal_antecedent(environment, &nominals, departed_readable)
        .map_err(|expected| cursor.fail(expected))?;
    cursor.advance_to(cursor.position() + width)?;
    Ok(reference)
}

/// A registered nominal names a predicate when the predicate guarantees it,
/// or, for `creature`, when every branch heads with a creature subtype
/// (`a Knight you control`, `another Ally`): a creature type is carried only
/// by creatures and Kindred permanents (CR 205.3m), and the anaphor names the
/// same object either way.
pub(in crate::oracle::canonical) fn predicate_names_nominal(
    predicate: &ObjectPredicate,
    nominal: TargetNominal,
) -> bool {
    selector_implies_nominal(
        &TargetSelector::Object(predicate.clone()),
        &nominal.as_predicate(),
    ) || (nominal == TargetNominal::CardType(ObjectCardType::Creature)
        && predicate.has_creature_subtype_head()
        && predicate.supports_domain(ObjectKind::Permanent))
}

/// A disjunctive nominal (`creature or planeswalker`) names a predicate when
/// one of its nominals does, or when every branch of a disjunctive predicate
/// is named by one of them (`target creature or planeswalker`).
fn predicate_names_nominals(predicate: &ObjectPredicate, nominals: &[TargetNominal]) -> bool {
    nominals
        .iter()
        .any(|nominal| predicate_names_nominal(predicate, *nominal))
        || (nominals.len() > 1
            && matches!(
                predicate,
                ObjectPredicate::Union(branches)
                    if !branches.is_empty()
                        && branches
                            .iter()
                            .all(|branch| predicate_names_nominals(branch, nominals))
            ))
}

/// The selector form of [`predicate_names_nominals`]: each branch of a
/// selector union must be named.
pub(in crate::oracle::canonical) fn selector_names_nominals(
    selector: &TargetSelector,
    nominals: &[TargetNominal],
) -> bool {
    match selector {
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            predicate_names_nominals(predicate, nominals)
        }
        TargetSelector::Union(branches) => {
            !branches.is_empty()
                && branches
                    .iter()
                    .all(|branch| selector_names_nominals(branch, nominals))
        }
        TargetSelector::Any | TargetSelector::Player(_) | TargetSelector::StackItem(_) => false,
    }
}

/// The permanent a move put onto the battlefield is named by `permanent`
/// whatever card it was (CR 110.1), and by any nominal its retyped
/// selector names (`creature` after `return target creature card ... to the
/// battlefield`, CR 400.7).
fn moved_permanent_names(antecedent: &ObjectSubjectAntecedent, nominals: &[TargetNominal]) -> bool {
    nominals
        .iter()
        .any(|nominal| *nominal == TargetNominal::Domain(ObjectKind::Permanent))
        || antecedent_names(antecedent, nominals)
}

fn antecedent_names(antecedent: &ObjectSubjectAntecedent, nominals: &[TargetNominal]) -> bool {
    antecedent
        .selector
        .as_ref()
        .is_some_and(|selector| selector_names_nominals(selector, nominals))
}

/// A reference that denotes one object. A declared set and the tokens of a
/// creation action are object sets, never a singular antecedent.
fn singular_reference(reference: PermanentEffectReference) -> bool {
    !matches!(
        reference,
        PermanentEffectReference::TargetSet(_)
            | PermanentEffectReference::Observed(TypedObservedEventObject::CreatedTokens)
    )
}

const PLURAL_SET_FOR_SINGULAR: &str =
    "a singular antecedent; the preceding action declared or produced several objects";

/// The one resolver of a singular object pronoun (`it`, `he`, `she`): the
/// proven lexical antecedent of the position being read. A preceding plural
/// set is a number mismatch, not an antecedent.
pub(in crate::oracle::canonical) fn resolve_pronoun_antecedent(
    environment: &BindingEnvironment,
) -> Result<PermanentEffectReference, &'static str> {
    if let Some(object) = environment.object.as_ref() {
        return Ok(object.reference);
    }
    // The one object an earlier action selected or moved: an operand reader
    // acts on a permanent, so a result that is no longer one is refused
    // with its own diagnostic (CR 400.7).
    if let Some(result) = environment
        .result_object
        .as_ref()
        .filter(|result| result.number == GrammaticalNumber::Singular)
    {
        return if result.reference.permanent {
            Ok(result.permanent_reference())
        } else {
            Err(DEPARTED_ACTION_RESULT_OPERAND)
        };
    }
    if environment
        .objects
        .as_ref()
        .is_some_and(|set| set.number == GrammaticalNumber::Plural)
    {
        return Err(PLURAL_SET_FOR_SINGULAR);
    }
    Err("a source binding or preceding singular permanent subject in this lexical scope")
}

/// The diagnostic of a copied card without a producer in the program.
pub(in crate::oracle::canonical) const COPIED_CARD_MISSING: &str = "a card this program declared as its one target or moved or selected on resolution, as the copied object (CR 707.2)";

/// The diagnostic of a copied card two antecedents of the same action name.
pub(in crate::oracle::canonical) const COPIED_CARD_AMBIGUOUS: &str = "exactly one antecedent for the copied card; a declared card target and the card the same action moved both match";

/// The diagnostic of `the exiled card` without an exile of this program: the
/// card a linked ability exiled (CR 607.2a) has no copy reference yet.
pub(in crate::oracle::canonical) const COPIED_LINKED_EXILE: &str = "an exile of this program for `the exiled card`; a card a linked ability exiled (CR 607.2a) is not a copied object here";

/// The diagnostic of a copy of a permanent that left the battlefield whose
/// copiable values as it last existed are not recorded (CR 400.7, 608.2h).
pub(in crate::oracle::canonical) const COPIED_DEPARTED_UNRECORDED: &str = "an object whose copiable values as it last existed are recorded (CR 608.2h, 707.2); only the triggering event's object and cards keep them";

/// The one resolver of a copied card (CR 707.2): a copy reads only the
/// copiable values of its object, in whatever zone it is, so a card the
/// program declared or moved is a legal antecedent although it is no
/// permanent. The nearest of the program's singular card target and of the
/// one card an earlier action moved or selected wins (CR 608.2c); `verb`
/// restricts the result to its producing verb (`the exiled card`). A
/// sacrificed permanent is not the card it became (CR 400.7) and keeps no
/// copiable values as it last existed. Missing and ambiguous antecedents
/// keep distinct diagnostics.
pub(in crate::oracle::canonical) fn resolve_copied_card_antecedent(
    environment: &BindingEnvironment,
    verb: Option<super::ActionResultVerb>,
) -> Result<PermanentEffectReference, &'static str> {
    let target = environment
        .card_target
        .as_ref()
        .filter(|_| verb.is_none())
        .map(|target| target.binding);
    let result = environment.result_object.as_ref().filter(|result| {
        result.number == GrammaticalNumber::Singular
            && !result.reference.permanent
            && verb.is_none_or(|verb| result.verb == Some(verb))
    });
    match (target, result) {
        (Some(target), Some(result)) if target.effect_index == result.reference.effect_index => {
            Err(COPIED_CARD_AMBIGUOUS)
        }
        (Some(target), Some(result)) if target.effect_index > result.reference.effect_index => {
            Ok(PermanentEffectReference::Target(target))
        }
        (_, Some(result)) => {
            if result.verb == Some(super::ActionResultVerb::Sacrificed) {
                return Err(COPIED_DEPARTED_UNRECORDED);
            }
            Ok(result.permanent_reference())
        }
        (Some(target), None) => Ok(PermanentEffectReference::Target(target)),
        (None, None) if verb == Some(super::ActionResultVerb::Exiled) => Err(COPIED_LINKED_EXILE),
        (None, None) => Err(COPIED_CARD_MISSING),
    }
}

/// The object a copy names with a pronoun (`a copy of it`): the proven
/// pronoun antecedent, else, when that antecedent is a card an earlier
/// action moved or no permanent antecedent exists, the copied card the
/// program declared or moved (`resolve_copied_card_antecedent`).
pub(in crate::oracle::canonical) fn resolve_copied_pronoun_antecedent(
    environment: &BindingEnvironment,
) -> Result<PermanentEffectReference, &'static str> {
    match resolve_pronoun_antecedent(environment) {
        Ok(reference) => Ok(reference),
        Err(expected) if expected == DEPARTED_ACTION_RESULT_OPERAND => {
            resolve_copied_card_antecedent(environment, None)
        }
        Err(expected) => resolve_copied_card_antecedent(environment, None).map_err(|_| expected),
    }
}

/// Whether a copy may read `reference` (CR 707.2): every object it names
/// for its copiable values, except an event object that left the
/// battlefield whose departure the runtime does not capture for this
/// reference kind, and a sacrificed permanent read as an action result.
pub(in crate::oracle::canonical) fn copied_reference_admitted(
    environment: &BindingEnvironment,
    reference: PermanentEffectReference,
) -> Result<(), &'static str> {
    if environment
        .frame
        .event_object
        .as_ref()
        .is_some_and(|slot| slot.departed && slot.reference == reference)
        && reference != PermanentEffectReference::Observed(TypedObservedEventObject::EventPermanent)
    {
        return Err(COPIED_DEPARTED_UNRECORDED);
    }
    if let PermanentEffectReference::ActionResult(produced) = reference
        && !produced.permanent
        && environment.result_object.as_ref().is_some_and(|result| {
            result.reference == produced && result.verb == Some(super::ActionResultVerb::Sacrificed)
        })
    {
        return Err(COPIED_DEPARTED_UNRECORDED);
    }
    Ok(())
}

/// The one resolver of a singular nominal anaphor (`that creature`, `the
/// permanent`, `that creature or planeswalker`, CR 608.2c). In order: a
/// block event's other combatant; a lexical target whose declared selector
/// names the nominal; the event's object when the event wrote that nominal
/// for it; the program's singular antecedent (a declared target, the card a
/// move put onto the battlefield, an equipped creature); the one token the
/// preceding action created. `departed_readable` admits an event object that
/// left the battlefield, for a read of its last known information only
/// (CR 400.7, 608.2h). Anything else is a committed diagnostic.
pub(in crate::oracle::canonical) fn resolve_nominal_antecedent(
    environment: &BindingEnvironment,
    nominals: &[TargetNominal],
    departed_readable: bool,
) -> Result<PermanentEffectReference, &'static str> {
    let creature = TargetNominal::CardType(ObjectCardType::Creature);
    if environment.frame.combat_counterpart
        && !environment.local_object_scope
        && nominals == [creature]
    {
        return Ok(PermanentEffectReference::Observed(
            TypedObservedEventObject::CombatCounterpart,
        ));
    }
    // The object a later action of the program selected or moved is the
    // nearest antecedent with this nominal (CR 608.2c), unless a target an
    // even later action declared has it: `Target player sacrifices a
    // creature of their choice. You gain life equal to that creature's
    // toughness.` A result that is no longer a permanent answers only a
    // read of its last known information (CR 400.7, 608.2h).
    if let Some(result) = environment
        .result_object
        .as_ref()
        .filter(|result| result.names(nominals))
        && !environment.object.as_ref().is_some_and(|antecedent| {
            matches!(
                antecedent.reference,
                PermanentEffectReference::Target(binding)
                    if binding.effect_index > result.reference.effect_index
            ) && antecedent_names(antecedent, nominals)
        })
    {
        return if result.reference.permanent || departed_readable {
            Ok(result.permanent_reference())
        } else {
            Err(DEPARTED_ACTION_RESULT_OPERAND)
        };
    }
    // A lexical target with this nominal wins, and so does the permanent a
    // move of this program put onto the battlefield (the event's dying
    // object a move returned is that new permanent, CR 400.7); otherwise
    // the event's object when the event wrote this nominal for it.
    let lexical = environment
        .object
        .as_ref()
        .filter(|antecedent| match antecedent.reference {
            PermanentEffectReference::Target(_) => antecedent_names(antecedent, nominals),
            PermanentEffectReference::Observed(TypedObservedEventObject::MovedOntoBattlefield) => {
                moved_permanent_names(antecedent, nominals)
            }
            _ => false,
        });
    if lexical.is_none()
        && let Some(slot) = environment
            .frame
            .event_object
            .as_ref()
            .filter(|slot| slot.names(nominals))
    {
        // An operand reader acts on the permanent (tap, destroy, deal damage
        // to). A departed event object (`a creature dies`) is a new object in
        // another zone (CR 400.7); only a read of its last known information
        // (a condition, a property, its controller) may name it.
        if slot.departed && !departed_readable {
            return Err(
                "a permanent for the nominal anaphor; the event's object left the battlefield",
            );
        }
        return Ok(slot.reference);
    }
    if let Some(antecedent) = environment.object.as_ref() {
        match antecedent.reference {
            // The permanent a move put onto the battlefield is a permanent
            // whatever card it was (CR 110.1, 400.7): `that permanent`
            // names it; any other nominal follows from its card's selector.
            PermanentEffectReference::Observed(TypedObservedEventObject::MovedOntoBattlefield)
                if moved_permanent_names(antecedent, nominals) =>
            {
                return Ok(antecedent.reference);
            }
            // A declared target, or the card a move put onto the battlefield
            // (its declared card predicate is the selector the nominal must
            // follow from).
            PermanentEffectReference::Target(_)
            | PermanentEffectReference::Observed(TypedObservedEventObject::MovedOntoBattlefield) => {
                let selector = antecedent
                    .selector
                    .as_ref()
                    .ok_or("the declared selector of the preceding permanent target")?;
                // `Counter target spell. If that spell ...`: a read of the
                // declared stack object, never an operand on it.
                return if selector_names_nominals(selector, nominals)
                    || (departed_readable && stack_selector_names_spell(selector, nominals))
                {
                    Ok(antecedent.reference)
                } else {
                    Err("a nominal implied by the preceding target's declared selector")
                };
            }
            // An Equipment equips only a creature (CR 301.5).
            PermanentEffectReference::Attached(AttachedObjectRole::Equipped)
                if nominals == [creature] =>
            {
                return Ok(antecedent.reference);
            }
            // `enchanted creature ... that creature`: the one permanent the
            // Aura enchants, whose kind its enchant ability fixes (CR
            // 303.4a); the anaphor names that object by a permanent nominal.
            PermanentEffectReference::Attached(AttachedObjectRole::Enchanted)
                if names_one_permanent_kind(nominals) =>
            {
                return Ok(antecedent.reference);
            }
            _ => {}
        }
    }
    // `Counter target spell if that spell ...`: a read of the declared
    // target that need not be a permanent, when its selector names the
    // nominal (a spell on the stack); never an operand.
    if departed_readable
        && let Some(target) = environment.declared_target.as_ref().filter(|target| {
            stack_selector_names_spell(&target.selector, nominals)
                || selector_names_nominals(&target.selector, nominals)
        })
        && environment.object.is_none()
    {
        return Ok(PermanentEffectReference::Target(target.binding));
    }
    // `Create a 1/1 Otter creature token. ... that creature`: the one token
    // the preceding action created, with the types it was created with.
    if let Some(set) = environment.objects.as_ref() {
        match (set.number, set.denotes.bound()) {
            (GrammaticalNumber::Singular, Some(reference))
                if set
                    .selector
                    .as_ref()
                    .is_some_and(|selector| selector_names_nominals(selector, nominals)) =>
            {
                return Ok(reference);
            }
            (GrammaticalNumber::Plural, _) if environment.object.is_none() => {
                return Err(PLURAL_SET_FOR_SINGULAR);
            }
            _ => {}
        }
    }
    Err(match environment.object {
        None => "a preceding singular permanent subject in this lexical scope",
        Some(_) => "a declared target as the nominal anaphor antecedent",
    })
}

/// `spell` names a declared stack-object target whose selector guarantees
/// a spell (CR 112.1, 115.1); an ability on the stack is no spell.
fn stack_selector_names_spell(selector: &TargetSelector, nominals: &[TargetNominal]) -> bool {
    fn guarantees_spell(predicate: &super::StackItemPredicate) -> bool {
        match predicate {
            super::StackItemPredicate::Spell => true,
            super::StackItemPredicate::Intersection(operands) => {
                operands.iter().any(guarantees_spell)
            }
            super::StackItemPredicate::Union(operands) => {
                !operands.is_empty() && operands.iter().all(guarantees_spell)
            }
            super::StackItemPredicate::Ability(_) | super::StackItemPredicate::Colorless => false,
        }
    }
    nominals == [TargetNominal::Domain(ObjectKind::Spell)]
        && matches!(selector, TargetSelector::StackItem(predicate) if guarantees_spell(predicate))
}

/// One nominal that names a kind of battlefield permanent (`creature`,
/// `land`, `permanent`).
fn names_one_permanent_kind(nominals: &[TargetNominal]) -> bool {
    match nominals {
        [TargetNominal::CardType(card_type)] => card_type.is_permanent_type(),
        [TargetNominal::Domain(kind)] => *kind == ObjectKind::Permanent,
        _ => false,
    }
}

/// The one resolver of a possessive `its` in a numeric or condition read
/// (`its power`, `its mana value`): the subject of the clause being read
/// (`it deals damage equal to its power`), else the program's singular
/// antecedent, else the event's grammatical subject, whose last known
/// information still answers the read (CR 608.2h). A set of objects is a
/// number mismatch; nothing is guessed.
pub(in crate::oracle::canonical) fn resolve_possessive_antecedent(
    environment: &BindingEnvironment,
) -> Result<PermanentEffectReference, &'static str> {
    if let Some(subject) = environment.subject {
        return singular_reference(subject)
            .then_some(subject)
            .ok_or(PLURAL_SET_FOR_SINGULAR);
    }
    if let Some(object) = environment.object.as_ref() {
        return singular_reference(object.reference)
            .then_some(object.reference)
            .ok_or(PLURAL_SET_FOR_SINGULAR);
    }
    // The object an earlier action selected or moved is nearer than the
    // event's subject (`When this creature dies, exile the top card of your
    // library. Target opponent loses life equal to its mana value.`); its
    // last known information answers the read (CR 608.2h).
    if let Some(result) = environment
        .result_object
        .as_ref()
        .filter(|result| result.number == GrammaticalNumber::Singular)
    {
        return Ok(result.permanent_reference());
    }
    if let Some(slot) = environment
        .frame
        .event_object
        .as_ref()
        .filter(|slot| slot.subject && relation_supported(slot.reference))
    {
        return Ok(slot.reference);
    }
    if environment
        .objects
        .as_ref()
        .is_some_and(|set| set.number == GrammaticalNumber::Plural)
    {
        return Err(PLURAL_SET_FOR_SINGULAR);
    }
    Err("an event subject or a preceding target that `its` names")
}

/// The object a condition reads (`If it's a creature`, `if its mana value is
/// 2 or less`, `If that creature was a Villain`): the possessive antecedent
/// for a pronoun (empty `nominals`), else the nominal antecedent read for its
/// last known information (CR 608.2h). The sentence-condition grammar calls
/// this resolver instead of guessing a target or an event object.
pub(in crate::oracle::canonical) fn resolve_condition_antecedent(
    environment: &BindingEnvironment,
    nominals: &[TargetNominal],
) -> Result<PermanentEffectReference, &'static str> {
    if nominals.is_empty() {
        resolve_possessive_antecedent(environment)
    } else {
        resolve_nominal_antecedent(environment, nominals, true)
    }
}

/// What a condition pronoun (`it`, `its`, `that card`) names when the
/// program has no permanent subject and no card result in scope: the
/// nearest of a declared target that need not be a permanent and of the one
/// object an earlier action selected or moved, else the spell a cast record
/// binds (CR 601.2c, 603.2, 608.2c).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ConditionPronounAntecedent {
    /// A declared spell, `any target` or other non-permanent target slot.
    Target(super::TargetBinding),
    /// The one object an earlier action of the program selected or moved.
    ActionResult(super::ActionResultReference),
    /// The object the triggering event records as its subject, read for
    /// its last known information once it left the battlefield (CR
    /// 603.10a, 608.2h): `draw a card if its power is 3 or greater` after
    /// the entering creature's draw.
    EventObject(PermanentEffectReference),
    /// The spell the triggering cast record binds.
    EventSpell,
}

/// The diagnostic of a condition over a permanent an earlier action
/// sacrificed: the card it became is another object (CR 400.7) whose
/// characteristics are not its last known information (CR 608.2h).
pub(in crate::oracle::canonical) const SACRIFICED_RESULT_CONDITION: &str = "a condition over an object whose last known information is recorded; the sacrificed permanent is not the card it became (CR 400.7, 608.2h)";

/// The one resolver of a condition pronoun outside the permanent scope
/// (`Counter target spell if its mana value is 2 or less`, `defending
/// player reveals the top card of their library. If it's a land card`,
/// `Whenever you cast a spell, if its mana value is ...`). The latest
/// lexical antecedent wins (CR 608.2c); the cast record is the frame's.
pub(in crate::oracle::canonical) fn resolve_condition_pronoun(
    environment: &BindingEnvironment,
) -> Result<ConditionPronounAntecedent, &'static str> {
    let target = environment
        .declared_target
        .as_ref()
        .map(|target| target.binding);
    let result = environment
        .result_object
        .as_ref()
        .filter(|result| result.number == GrammaticalNumber::Singular);
    match (target, result) {
        (Some(target), Some(result)) if target.effect_index == result.reference.effect_index => {
            Err(
                "exactly one proven antecedent for the pronoun, not a target and a result of the same action",
            )
        }
        (Some(target), Some(result)) if target.effect_index > result.reference.effect_index => {
            Ok(ConditionPronounAntecedent::Target(target))
        }
        (_, Some(result)) => condition_result_antecedent(result),
        (Some(target), None) => Ok(ConditionPronounAntecedent::Target(target)),
        (None, None) => {
            // No lexical antecedent: the event's grammatical subject, then
            // the spell a cast record binds (the frame's antecedents).
            if let Some(slot) = environment
                .frame
                .event_object
                .as_ref()
                .filter(|slot| slot.subject && relation_supported(slot.reference))
            {
                return Ok(ConditionPronounAntecedent::EventObject(slot.reference));
            }
            if environment.frame.event_spell {
                return Ok(ConditionPronounAntecedent::EventSpell);
            }
            Err("one proven singular antecedent for the pronoun")
        }
    }
}

/// `that card` in a condition outside the card-collection scope: the one
/// card an earlier action selected or moved (`Target opponent exiles the
/// top card of their library. If that card is a land card`).
pub(in crate::oracle::canonical) fn resolve_condition_card(
    environment: &BindingEnvironment,
) -> Result<ConditionPronounAntecedent, &'static str> {
    let result = environment
        .result_object
        .as_ref()
        .filter(|result| {
            result.number == GrammaticalNumber::Singular && !result.reference.permanent
        })
        .ok_or("one proven singular card antecedent for `that card`")?;
    condition_result_antecedent(result)
}

fn condition_result_antecedent(
    result: &ActionResultObject,
) -> Result<ConditionPronounAntecedent, &'static str> {
    if !result.reference.permanent && result.verb == Some(super::ActionResultVerb::Sacrificed) {
        return Err(SACRIFICED_RESULT_CONDITION);
    }
    Ok(ConditionPronounAntecedent::ActionResult(result.reference))
}

/// `that NOUN` as the subject of a condition: the nominal antecedent read
/// for its last known information (CR 608.2h), else, for `that spell`, the
/// spell a cast record binds when no lexical spell antecedent has that
/// nominal. Nothing else is guessed.
pub(in crate::oracle::canonical) fn recognize_condition_nominal_object(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<ConditionPronounAntecedentOrObject> {
    let start = cursor.clone();
    match recognize_nominal_object_read(cursor, environment) {
        Ok(reference) => Ok(ConditionPronounAntecedentOrObject::Object(reference)),
        Err(_)
            if environment.frame.event_spell
                && start
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
                && start
                    .peek_ahead(1)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("spell")) =>
        {
            *cursor = start;
            cursor.enter("condition.sentence.object.cast-record-spell");
            cursor.consume_phrase(&["that", "spell"])?;
            Ok(ConditionPronounAntecedentOrObject::Antecedent(
                ConditionPronounAntecedent::EventSpell,
            ))
        }
        Err(failure) => Err(failure),
    }
}

/// A condition subject: a permanent reference, or an antecedent outside the
/// permanent scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ConditionPronounAntecedentOrObject {
    Object(PermanentEffectReference),
    Antecedent(ConditionPronounAntecedent),
}

/// The one object every atom of a condition is about, when they share it
/// (`As long as this creature is enchanted or equipped`): the antecedent a
/// following `it` names in the same sentence. `None` when an atom is about
/// another object, a player or no object.
pub(in crate::oracle::canonical) fn condition_object_antecedent(
    condition: &super::ConditionNode,
) -> Option<PermanentEffectReference> {
    match condition {
        super::BooleanConditionNode::Atom(super::ConditionAtom::ObjectSatisfies(node)) => {
            match node.object {
                super::ConditionObjectReference::Permanent(reference)
                    if singular_reference(reference) =>
                {
                    Some(reference)
                }
                _ => None,
            }
        }
        // `As long as this creature's power is 5 or greater`: a comparison
        // of one object's numeric property with a fixed number is about
        // that object as much as a predicate over it is.
        super::BooleanConditionNode::Atom(super::ConditionAtom::Compare(comparison)) => {
            compared_object(comparison)
        }
        super::BooleanConditionNode::Atom(_) => None,
        super::BooleanConditionNode::Not(inner) => condition_object_antecedent(inner),
        super::BooleanConditionNode::And(operands) | super::BooleanConditionNode::Or(operands) => {
            let (first, rest) = operands.split_first()?;
            let reference = condition_object_antecedent(first)?;
            rest.iter()
                .all(|operand| condition_object_antecedent(operand) == Some(reference))
                .then_some(reference)
        }
    }
}

/// The one object a comparison reads: a numeric property of one object
/// compared with a fixed number. Any other operand pair names no object.
fn compared_object(comparison: &super::ConditionComparison) -> Option<PermanentEffectReference> {
    let object_of = |number: &super::ConditionNumber| match number {
        super::ConditionNumber::Expression(
            NumericExpression::ObjectProperty { object, .. }
            | NumericExpression::CountCounters { object, .. },
        ) => Some(PermanentEffectReference::Source(*object)),
        _ => None,
    };
    match (&comparison.left, &comparison.right) {
        (object, super::ConditionNumber::Integer(_))
        | (super::ConditionNumber::Integer(_), object) => object_of(object),
        _ => None,
    }
}

/// `it` as the subject after a leading condition about one object (`As long
/// as this creature is equipped, it gets +1/+1`; `As long as enchanted
/// creature is red, it has ...`): the source or the permanent it is attached
/// to that every atom of the condition reads. `Ok(None)` when no pronoun
/// subject starts; a pronoun without such a condition antecedent is a
/// committed diagnostic, never the source by default.
pub(in crate::oracle::canonical) fn recognize_condition_pronoun_subject(
    cursor: &mut OracleTokenCursor<'_>,
    condition: Option<&super::ConditionNode>,
) -> MachineResult<Option<PermanentEffectReference>> {
    let Some(condition) = condition else {
        return Ok(None);
    };
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("it"))
    {
        return Ok(None);
    }
    cursor.enter("static.condition-antecedent.pronoun");
    let reference = condition_object_antecedent(condition)
        .filter(|reference| {
            matches!(
                reference,
                PermanentEffectReference::Source(_) | PermanentEffectReference::Attached(_)
            )
        })
        .ok_or_else(|| {
            cursor.fail("a leading condition about the source or its attached permanent for `it`")
        })?;
    cursor.consume("it")?;
    Ok(Some(reference))
}

/// The nominal and number of an object nominal surface, or `None` when it
/// is not a registered nominal (`creatures` is plural, `creature` singular).
fn nominal_number(raw: &str) -> Option<(TargetNominal, GrammaticalNumber)> {
    let nominal = registered_nominal(raw)?;
    let singular = lookup_card_type(raw, AcceptedForms::SingularOnly).is_some()
        || lookup_object_domain(raw, AcceptedForms::SingularOnly).is_some();
    Some((
        nominal,
        if singular {
            GrammaticalNumber::Singular
        } else {
            GrammaticalNumber::Plural
        },
    ))
}

/// `token` / `tokens`: the created-token nominal and its number.
fn token_nominal_number(raw: &str) -> Option<GrammaticalNumber> {
    if lookup_object_characteristic(raw, AcceptedForms::SingularOnly)
        == Some(ObjectCharacteristic::Token)
    {
        Some(GrammaticalNumber::Singular)
    } else if lookup_object_characteristic(raw, AcceptedForms::SingularOrPlural)
        == Some(ObjectCharacteristic::Token)
    {
        Some(GrammaticalNumber::Plural)
    } else {
        None
    }
}

/// The head of an object-set reference at the cursor: its token width, the
/// number it states, the nominal it names (`None` for a pronoun or the token
/// noun) and whether it is the created-token noun.
struct ObjectSetHead {
    width: usize,
    number: GrammaticalNumber,
    nominal: Option<TargetNominal>,
    tokens: bool,
}

fn object_set_head(cursor: &OracleTokenCursor<'_>) -> Option<ObjectSetHead> {
    let word = |offset: usize| {
        cursor
            .peek_ahead(offset)
            .map(|raw| raw.to_ascii_lowercase())
    };
    let plural_pronoun = |width: usize| ObjectSetHead {
        width,
        number: GrammaticalNumber::Plural,
        nominal: None,
        tokens: false,
    };
    // `each of them` / `each of those NOUNs`: each member of the same set
    // (CR 608.2c); an action on a set already acts on each member.
    let first = word(0)?;
    let (prefix, determiner) = match (first.as_str(), word(1).as_deref(), word(2).as_deref()) {
        ("each", Some("of"), Some("them")) => return Some(plural_pronoun(3)),
        ("each", Some("of"), Some("those")) => (2, "those"),
        // `they` and its object form `them` (`Untap them.`, `gain control
        // of them`) name the same set (CR 608.2c).
        ("they" | "them", _, _) => return Some(plural_pronoun(1)),
        ("those", _, _) => (0, "those"),
        ("that", _, _) => (0, "that"),
        ("the", _, _) => (0, "the"),
        _ => return None,
    };
    let noun = cursor
        .token_slice(
            cursor.position() + prefix + 1,
            cursor.position() + prefix + 2,
        )
        .first()
        .filter(|token| token.token_type != "possessiveTypeWord")?
        .raw
        .clone();
    if let Some(number) = token_nominal_number(&noun) {
        // `that token` / `the token` / `those tokens` / `the tokens`.
        let agrees = match determiner {
            "that" => number == GrammaticalNumber::Singular,
            "those" => number == GrammaticalNumber::Plural,
            _ => true,
        };
        return agrees.then_some(ObjectSetHead {
            width: prefix + 2,
            number,
            nominal: None,
            tokens: true,
        });
    }
    // `those creatures`: only the plural demonstrative opens a nominal set
    // reference; `that creature` / `the creature` stay singular anaphors.
    let (nominal, number) = nominal_number(&noun)?;
    (determiner == "those" && number == GrammaticalNumber::Plural).then_some(ObjectSetHead {
        width: prefix + 2,
        number,
        nominal: Some(nominal),
        tokens: false,
    })
}

/// True when `failure` is an unresolved antecedent of a demonstrative
/// object reference (`that creature`, `those creatures`, `that token`): the
/// reference was read and no proven antecedent has its nominal or number.
pub(in crate::oracle::canonical) fn demonstrative_binding_failure(
    failure: &super::super::MachineFailure,
) -> bool {
    matches!(
        failure.state,
        "permanent.subject.nominal-anaphor.antecedent"
            | "permanent.subject.object-set-anaphor.antecedent"
            | "permanent.subject.object-set-anaphor.nominal"
    )
}

/// Lexical lookahead for an object-set reference (`they`, `those
/// creatures`, `each of them`, `that token`, `the tokens`). It proves no
/// binding.
pub(in crate::oracle::canonical) fn object_set_reference_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    object_set_head(cursor).is_some()
}

/// Resolves an object-set reference against the set the preceding action
/// declared or produced (CR 608.2c). The stated number must be the
/// producer's: `those creatures` never names one declared target, `that
/// token` never names several created tokens. The nominal must follow from
/// the declared selector, or name the created tokens.
pub(in crate::oracle::canonical) fn recognize_object_set_reference(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<super::PermanentOperandNode> {
    cursor.enter("permanent.subject.object-set-anaphor");
    let head = object_set_head(cursor)
        .ok_or_else(|| cursor.fail("`they`, `those NOUNs`, `each of them` or `that token`"))?;
    cursor.enter("permanent.subject.object-set-anaphor.antecedent");
    let set = environment
        .objects
        .as_ref()
        .ok_or_else(|| cursor.fail("an object set the preceding action declared or produced"))?;
    cursor.require(
        set.number == head.number,
        match head.number {
            GrammaticalNumber::Singular => PLURAL_SET_FOR_SINGULAR,
            _ => "a plural antecedent; the preceding action declared or produced one object",
        },
    )?;
    cursor.enter("permanent.subject.object-set-anaphor.nominal");
    cursor.require(
        !head.tokens || set.tokens,
        "tokens the preceding action created for `token`",
    )?;
    if let Some(nominal) = head.nominal {
        cursor.require(
            set.selector
                .as_ref()
                .is_some_and(|selector| selector_names_nominals(selector, &[nominal])),
            "a nominal implied by the declared selector of the preceding object set",
        )?;
    }
    cursor.advance_to(cursor.position() + head.width)?;
    Ok(set.denotes.operand())
}

/// Lexical lookahead for `that NOUN's power|toughness|mana value` and the
/// source's own `this NOUN's power|toughness`.
pub(in crate::oracle::canonical) fn bound_object_property_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let possessive_property = |offset: usize| {
        cursor
            .token_slice(cursor.position() + offset, cursor.position() + offset + 1)
            .first()
            .is_some_and(|token| token.token_type == "possessiveTypeWord")
            && cursor.peek_ahead(offset + 1).is_some_and(|raw| {
                raw.eq_ignore_ascii_case("power") || raw.eq_ignore_ascii_case("toughness")
            })
    };
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
    {
        return possessive_property(1);
    }
    if participle_property_starts(cursor) {
        return true;
    }
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
        && cursor
            .token_slice(cursor.position() + 1, cursor.position() + 2)
            .first()
            .is_some_and(|token| token.token_type == "possessiveTypeWord")
        && cursor.peek_ahead(2).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("power")
                || raw.eq_ignore_ascii_case("toughness")
                || raw.eq_ignore_ascii_case("mana")
        })
}

/// The numeric read of one property of a singular bound object (CR 208,
/// 202.3). A declared target keeps the declared-slot property, validated
/// against its selector when lowered (`required_nominal`); the ability source
/// reads its own power and toughness; any other singular object the binding
/// environment names is read live while it is the same object, else as last
/// known information (CR 400.7, 608.2h).
pub(in crate::oracle::canonical) fn bound_object_numeric_property(
    reference: PermanentEffectReference,
    property: SelectorNumericProperty,
    required_nominal: ObjectPredicate,
) -> Result<NumericExpression, &'static str> {
    if !singular_reference(reference) {
        return Err(PLURAL_SET_FOR_SINGULAR);
    }
    Ok(match (reference, property) {
        (PermanentEffectReference::Target(binding), _) => {
            NumericExpression::QualifiedTargetProperty {
                object: binding,
                required_nominal,
                property,
            }
        }
        (PermanentEffectReference::Source(source), SelectorNumericProperty::Power) => {
            NumericExpression::ObjectProperty {
                object: source,
                property: super::NumericObjectProperty::Power,
            }
        }
        (PermanentEffectReference::Source(source), SelectorNumericProperty::Toughness) => {
            NumericExpression::ObjectProperty {
                object: source,
                property: super::NumericObjectProperty::Toughness,
            }
        }
        (PermanentEffectReference::Source(_), SelectorNumericProperty::ManaValue) => {
            return Err("a mana-value read the runtime records for this object");
        }
        (object, property) => NumericExpression::BoundObjectProperty { object, property },
    })
}

/// The genitive `that NOUN's power|toughness|mana value` (CR 208, 202.3):
/// the property of the object the environment names with that nominal,
/// resolved by the shared nominal resolver for its last known information.
/// `that spell's mana value` in a cast trigger is the cast record's value.
pub(in crate::oracle::canonical) fn recognize_bound_object_property(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<NumericExpression> {
    if participle_property_starts(cursor) {
        return recognize_participle_object_property(cursor, environment);
    }
    cursor.enter("numeric.genitive.reference");
    let source = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("this"));
    cursor.consume(if source { "this" } else { "that" })?;
    cursor.enter("numeric.genitive.nominal");
    let possessive = cursor.consume_type("possessiveTypeWord")?;
    let nominal = possessive_nominal(&possessive.raw)
        .ok_or_else(|| cursor.fail("a registered card type or object domain nominal"))?;
    if source {
        // `this creature's power`: the ability source's own power or
        // toughness, live or as last known information (CR 608.2h).
        cursor.enter("numeric.genitive.source-property");
        let property = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
            Some("power") => super::NumericObjectProperty::Power,
            Some("toughness") => super::NumericObjectProperty::Toughness,
            _ => return Err(cursor.fail("`power` or `toughness` of the ability source")),
        };
        let raw = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&raw)?;
        return Ok(NumericExpression::ObjectProperty {
            object: super::LocalObjectReference::SourceSelf,
            property,
        });
    }
    cursor.enter("numeric.genitive.property");
    let property = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("power"))
    {
        cursor.consume("power")?;
        SelectorNumericProperty::Power
    } else if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("toughness"))
    {
        cursor.consume("toughness")?;
        SelectorNumericProperty::Toughness
    } else {
        cursor.consume_phrase(&["mana", "value"])?;
        SelectorNumericProperty::ManaValue
    };
    cursor.enter("numeric.genitive.antecedent");
    // `that spell's mana value`: the spell the cast record binds, unless a
    // lexical spell target of the program has that nominal.
    if environment.frame.event_spell
        && nominal == TargetNominal::Domain(ObjectKind::Spell)
        && property == SelectorNumericProperty::ManaValue
        && !environment.object.as_ref().is_some_and(|antecedent| {
            matches!(antecedent.reference, PermanentEffectReference::Target(_))
                && antecedent_names(antecedent, &[nominal])
        })
    {
        return Ok(NumericExpression::EventValue(
            super::EventNumericBinding::CastSpellManaValue,
        ));
    }
    let reference = resolve_condition_antecedent(environment, &[nominal])
        .map_err(|_| cursor.fail("a preceding target or an event object with this nominal"))?;
    // An event's observed object is read only where the runtime records it.
    if let PermanentEffectReference::Observed(observed) = reference {
        cursor.require(
            matches!(
                observed,
                TypedObservedEventObject::EventPermanent
                    | TypedObservedEventObject::EnteringPermanent
                    | TypedObservedEventObject::MovedOntoBattlefield
                    | TypedObservedEventObject::CombatCounterpart
            ),
            "a preceding target or an event object with this nominal",
        )?;
    }
    bound_object_numeric_property(reference, property, nominal.as_predicate())
        .map_err(|expected| cursor.fail(expected))
}

/// Lexical lookahead for `the VERBed NOUN's power|toughness|mana value`
/// (`the sacrificed creature's power`, `the exiled card's mana value`): a
/// definite description of an object an action selected or moved.
fn participle_property_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        && cursor
            .peek_ahead(1)
            .and_then(super::ActionResultVerb::from_word)
            .is_some()
        && cursor
            .token_slice(cursor.position() + 2, cursor.position() + 3)
            .first()
            .is_some_and(|token| token.token_type == "possessiveTypeWord")
        && cursor.peek_ahead(3).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("power")
                || raw.eq_ignore_ascii_case("toughness")
                || (raw.eq_ignore_ascii_case("mana")
                    && cursor
                        .peek_ahead(4)
                        .is_some_and(|next| next.eq_ignore_ascii_case("value")))
        })
}

/// `the VERBed NOUN's PROPERTY` (CR 608.2c, 608.2h): the property of the one
/// object an earlier action of this program VERBed, when its selection
/// guarantees the nominal; read as last known information once it left the
/// battlefield. `the sacrificed|tapped NOUN's PROPERTY` without such an
/// action keeps its reading as the permanent sacrificed or tapped to pay a
/// cost of this ability (`recognize_sacrificed_cost_object_property`). Any other
/// participle without a producing action is a committed diagnostic.
fn recognize_participle_object_property(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<NumericExpression> {
    let start = cursor.clone();
    cursor.enter("numeric.participle.reference");
    cursor.consume("the")?;
    let verb = cursor
        .peek()
        .and_then(super::ActionResultVerb::from_word)
        .ok_or_else(|| cursor.fail("the past participle of an object-producing verb"))?;
    let participle = cursor.peek().unwrap_or_default().to_owned();
    cursor.consume(&participle)?;
    cursor.enter("numeric.participle.nominal");
    let possessive = cursor.consume_type("possessiveTypeWord")?;
    let nominal = possessive_nominal(&possessive.raw)
        .ok_or_else(|| cursor.fail("a registered card type or object domain nominal"))?;
    cursor.enter("numeric.participle.property");
    let property = super::recognize_selector_numeric_property(cursor)?;
    cursor.enter("numeric.participle.antecedent");
    let result = environment
        .result_object
        .as_ref()
        .filter(|result| result.verb == Some(verb) && result.names(&[nominal]));
    match result {
        Some(result) => {
            let reference = result.permanent_reference();
            bound_object_numeric_property(reference, property, nominal.as_predicate())
                .map_err(|expected| cursor.fail(expected))
        }
        // A sacrifice or tap cost of this ability is the other producer of
        // `the sacrificed|tapped NOUN's PROPERTY` (`cost_paid_object.rs`).
        None if matches!(
            verb,
            super::ActionResultVerb::Sacrificed | super::ActionResultVerb::Tapped
        ) =>
        {
            *cursor = start;
            super::recognize_sacrificed_cost_object_property(cursor)
        }
        None => Err(cursor.fail(PARTICIPLE_WITHOUT_PRODUCER)),
    }
}

/// The diagnostic of `the VERBed NOUN's PROPERTY` without a producer.
const PARTICIPLE_WITHOUT_PRODUCER: &str = "an earlier action of this program that selected or moved exactly one object of this nominal with this verb";

/// Reads a clause's arguments with `subject` as the clause subject of the
/// installed environment, then restores the enclosing environment.
pub(in crate::oracle::canonical) fn with_clause_subject<T>(
    cursor: &mut OracleTokenCursor<'_>,
    subject: Option<PermanentEffectReference>,
    read: impl FnOnce(&mut OracleTokenCursor<'_>) -> T,
) -> T {
    let mut environment = installed_binding_environment(cursor);
    environment.subject = subject;
    with_installed_environment(cursor, environment, read)
}

/// Reads with `antecedent` as the singular object antecedent when one is
/// proven, else with the installed environment unchanged. A postposed
/// `where X is its power` reads the object of the clause it closes.
pub(in crate::oracle::canonical) fn with_object_antecedent<T>(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<ObjectSubjectAntecedent>,
    read: impl FnOnce(&mut OracleTokenCursor<'_>) -> T,
) -> T {
    let Some(antecedent) = antecedent else {
        return read(cursor);
    };
    let mut environment = installed_binding_environment(cursor);
    // The closed clause's object is what a possessive `its` names first.
    // It replaces the lexical antecedent only when it is itself a declared
    // target (whose selector a nominal anaphor follows from); a source or
    // event subject keeps the earlier target reachable for `that NOUN`
    // (`Destroy target artifact. This creature gets +X/+0 ..., where X is
    // that artifact's mana value.`).
    environment.subject = Some(antecedent.reference);
    if environment.object.is_none()
        || matches!(antecedent.reference, PermanentEffectReference::Target(_))
    {
        environment.object = Some(antecedent);
    }
    with_installed_environment(cursor, environment, read)
}

/// The clause subject a damage source operand denotes: the ability source,
/// a bound singular object, or the single target this action declares.
pub(in crate::oracle::canonical) fn damage_source_subject(
    cursor: &OracleTokenCursor<'_>,
    source: &super::PermanentOperandNode,
) -> Option<PermanentEffectReference> {
    match source {
        super::PermanentOperandNode::Reference(reference) => {
            Some(PermanentEffectReference::Source(*reference))
        }
        super::PermanentOperandNode::BoundReference(reference)
            if singular_reference(*reference) =>
        {
            Some(*reference)
        }
        super::PermanentOperandNode::Targets(targets)
            if targets.slots.len() == 1 && targets.slots[0].cardinality.maximum == 1 =>
        {
            cursor
                .binding_environment()
                .and_then(|environment| environment.effect_index)
                .map(|effect_index| {
                    PermanentEffectReference::Target(super::TargetBinding {
                        effect_index,
                        slot_index: 0,
                    })
                })
        }
        _ => None,
    }
}

/// Whether the cursor reads an action of an effect program, where an
/// earlier `choose` instruction of the same program may produce the value a
/// `the chosen ...` reference reads (CR 608.2d). The program lowering and
/// `contextual_binding` prove that producer. Outside a program (a static
/// ability, a trigger event pattern) such a reference has no resolution
/// producer, so its readers do not claim it.
pub(in crate::oracle::canonical) fn resolution_choice_in_scope(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .binding_environment()
        .is_some_and(|environment| environment.effect_index.is_some())
}

/// The environment the parent installed. A parent that installs none binds
/// nothing, so every reference then fails explicitly.
pub(in crate::oracle::canonical) fn installed_binding_environment(
    cursor: &OracleTokenCursor<'_>,
) -> BindingEnvironment {
    cursor.binding_environment().cloned().unwrap_or_default()
}

#[cfg(test)]
#[path = "binding_environment_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "antecedent_environment_tests.rs"]
mod antecedent_environment_tests;
