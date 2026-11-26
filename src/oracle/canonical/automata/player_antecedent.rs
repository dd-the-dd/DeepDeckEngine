//! Player antecedents an effect program's earlier actions publish for the
//! performer of a later clause (CR 608.2c). The program parent hands the
//! actions read so far; nothing here reads tokens. A plural player anaphor
//! (`those players`, `they each`) names the one player set the immediately
//! preceding action named, and a relative of a later population (`each
//! player who does`) observes the per-player performance of the
//! immediately preceding population action. Nothing is guessed: without a
//! producer the reader's diagnostic stands.
use super::{
    DamageRecipientNode, LocalEffectNode, PerformerNode, PlayerSetReferenceNode,
    QuantifiedPlayerActionNode,
};

/// The plural player set the immediately preceding action named, as the
/// performer a later `those players` / `they each` names (CR 608.2c): the
/// population a damage action was dealt to (`deals 2 damage to each
/// opponent`), the population of a quantified player action (`each player
/// discards a card`), or the population or declared players of a performed
/// action (`each opponent sacrifices ...`, `two target players each ...`).
/// A restricted population (`each player who ...`) is not re-read after
/// the action changed the game: it publishes no set. A singular player is
/// `that player`'s antecedent, never a plural one.
pub(in crate::oracle::canonical) fn prior_player_set_performer(
    effects: &[LocalEffectNode],
) -> Option<PerformerNode> {
    match effects.last()? {
        LocalEffectNode::DealDamage(damage) => damage_player_set(&damage.recipient),
        LocalEffectNode::QuantifiedPlayerAction(QuantifiedPlayerActionNode {
            players,
            excluding_controller,
            ..
        }) => Some(PerformerNode::Population {
            players: *players,
            excluding_controller: *excluding_controller,
        }),
        LocalEffectNode::Performed(node) => match &node.performer {
            PerformerNode::Population {
                players,
                excluding_controller,
            } => Some(PerformerNode::Population {
                players: *players,
                excluding_controller: *excluding_controller,
            }),
            PerformerNode::DeclaredPlayers { binding, .. }
            | PerformerNode::BoundPlayers(binding) => Some(PerformerNode::BoundPlayers(*binding)),
            _ => None,
        },
        _ => None,
    }
}

/// The one player population a damage recipient names, alone or as the
/// only player group of a coordinated recipient (`each creature and each
/// player`).
fn damage_player_set(recipient: &DamageRecipientNode) -> Option<PerformerNode> {
    let population = |players: PlayerSetReferenceNode| PerformerNode::Population {
        players,
        excluding_controller: false,
    };
    match recipient {
        DamageRecipientNode::PlayerSet(players) => Some(population(*players)),
        DamageRecipientNode::All(groups) => {
            let mut sets = groups.iter().filter_map(|group| match group {
                DamageRecipientNode::PlayerSet(players) => Some(*players),
                _ => None,
            });
            let players = sets.next()?;
            sets.next().is_none().then(|| population(players))
        }
        _ => None,
    }
}

/// The immediately preceding population action whose per-player
/// performance a later relative observes (`Each player may shuffle ...
/// Each player who does draws seven cards`, `Each opponent discards a card.
/// Each opponent who can't loses 3 life.`, CR 608.2c): an action performed
/// by an unrestricted population, or one action of a quantified player
/// subject that each member performs in a player loop (a discard, a draw or
/// a mill). `optional` is true for a permission (`each player may`), where
/// `who doesn't` names the players who declined or could not; a mandatory
/// instruction's `who can't` names the players who could not perform it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PopulationPerformanceProducer {
    pub effect_index: usize,
    pub players: PlayerSetReferenceNode,
    pub excluding_controller: bool,
    pub optional: bool,
}

/// The population performance producer the immediately preceding action is,
/// when it is one.
pub(in crate::oracle::canonical) fn population_performance_producer(
    effects: &[LocalEffectNode],
) -> Option<PopulationPerformanceProducer> {
    let effect_index = effects.len().checked_sub(1)?;
    let node = match effects.last()? {
        LocalEffectNode::Performed(node) => node,
        LocalEffectNode::QuantifiedPlayerAction(QuantifiedPlayerActionNode {
            players,
            excluding_controller,
            actions,
        }) => {
            return matches!(
                actions.as_slice(),
                [super::QuantifiedPlayerAction::Discard(_)
                    | super::QuantifiedPlayerAction::Draw(_)
                    | super::QuantifiedPlayerAction::Mill(_)]
            )
            .then_some(PopulationPerformanceProducer {
                effect_index,
                players: *players,
                excluding_controller: *excluding_controller,
                optional: false,
            });
        }
        _ => return None,
    };
    let PerformerNode::Population {
        players,
        excluding_controller,
    } = &node.performer
    else {
        return None;
    };
    Some(PopulationPerformanceProducer {
        effect_index,
        players: *players,
        excluding_controller: *excluding_controller,
        optional: matches!(node.effect.as_ref(), LocalEffectNode::OptionalSequence(_)),
    })
}

/// True when every member of the consumer population is a member of the
/// producer's population, so each member either performed the producer's
/// instruction or did not (CR 101.4): every player contains any
/// population; the opponents contain the opponents only.
pub(in crate::oracle::canonical) fn population_within_producer(
    producer: &PopulationPerformanceProducer,
    players: PlayerSetReferenceNode,
    excluding_controller: bool,
) -> bool {
    let opponents = |players: PlayerSetReferenceNode, excluding_controller: bool| {
        players == PlayerSetReferenceNode::OpponentsOfAbilityController && !excluding_controller
    };
    (producer.players == PlayerSetReferenceNode::EachPlayer && !producer.excluding_controller)
        || (opponents(producer.players, producer.excluding_controller)
            && opponents(players, excluding_controller))
}
