//! The `choose DOMAIN` instruction frame. One frame reads every `choose`
//! instruction of an effect program: a chooser, a typed choice domain and the
//! symbolic binding that later references (`it`, `that creature`, `that
//! player`) resolve to through the enclosing sequence's anaphor memory.
//!
//! Two decision moments exist and stay distinct:
//! * `choose target ...` declares targets (CR 115.1, 601.2c / 603.3d): the
//!   instruction is the target declaration itself and adds no resolution step;
//! * every other domain is a decision made while the effect resolves
//!   (CR 608.2d): a color, a creature type, a card type, a card name, a
//!   number, a player, an object, or `land or nonland`.
//!
//! The domain child is classified before any resolution domain is accepted, so
//! an unsupported domain fails at its own automaton state instead of being
//! approximated. Mode choices (`choose one —`) belong to the modal parent.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CardSupertype, ObjectCardType, ObjectPredicate, PlayerPredicate, TargetCountSupport,
    TargetListNode, TargetSelector, recognize_object_predicate_atom,
    recognize_target_list_with_counts, selector_guarantees_permanent,
};
use crate::oracle::characteristic_vocabulary::AcceptedForms;

/// The chooser of a `choose` instruction. An imperative `choose` is the
/// ability's controller (CR 608.2d); other choosers are not read yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ChooserNode {
    Controller,
}

/// The typed domain of one `choose` instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ChoiceDomainNode {
    /// `choose target PERMANENT[S]`: the declared slots are the binding.
    PermanentTargets(TargetListNode),
    /// `choose target player|opponent`: one declared player slot.
    PlayerTarget(TargetListNode),
    /// `choose a creature type`: a resolution choice among all creature
    /// types (CR 205.3m), read later as `of the chosen type`.
    CreatureType,
    /// `choose a color`: a resolution choice among the five colors (CR
    /// 105.1), read later as `the chosen color` / `of the chosen color`.
    Color,
    /// `choose an opponent [at random]`, `choose a player`: a player chosen
    /// while the effect resolves (CR 608.2d), not a target. It is the
    /// antecedent of a later `that player`.
    Player(PlayerChoiceNode),
    /// `choose a [nonland] card name`: a name chosen while the effect
    /// resolves (CR 608.2d, 201.3), read later as `with the chosen name` /
    /// `with that name`.
    CardName(CardNameDomainNode),
}

/// The names a card-name choice admits: the names of cards satisfying the
/// shared card predicate (`nonland`, `nonartifact, nonland`, `creature`,
/// `other than a basic land card name`). `None` admits every card name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardNameDomainNode {
    pub predicate: Option<ObjectPredicate>,
}

/// The population a resolution-time player choice draws from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerChoicePopulation {
    /// `an opponent`: an opponent of the chooser (CR 102.2).
    Opponent,
    /// `a player`: any player in the game, the chooser included.
    Player,
}

/// `choose an opponent [at random]`: who may be chosen, and whether the
/// chooser picks or the game picks at random.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerChoiceNode {
    pub population: PlayerChoicePopulation,
    pub random: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChooseInstructionNode {
    pub chooser: ChooserNode,
    pub domain: ChoiceDomainNode,
}

impl ChooseInstructionNode {
    /// The declared target slots of this instruction, if it declares targets.
    pub(in crate::oracle::canonical) fn declared_targets(&self) -> Option<&TargetListNode> {
        match &self.domain {
            ChoiceDomainNode::PermanentTargets(targets)
            | ChoiceDomainNode::PlayerTarget(targets) => Some(targets),
            ChoiceDomainNode::CreatureType
            | ChoiceDomainNode::Color
            | ChoiceDomainNode::Player(_)
            | ChoiceDomainNode::CardName(_) => None,
        }
    }

    pub(in crate::oracle::canonical) fn permanent_targets(&self) -> Option<&TargetListNode> {
        match &self.domain {
            ChoiceDomainNode::PermanentTargets(targets) => Some(targets),
            ChoiceDomainNode::PlayerTarget(_)
            | ChoiceDomainNode::CreatureType
            | ChoiceDomainNode::Color
            | ChoiceDomainNode::Player(_)
            | ChoiceDomainNode::CardName(_) => None,
        }
    }
}

/// Resolution domains recognized by their head words. Each is a CR 608.2d
/// decision. A domain whose binding has an executable consumer returns a
/// typed node; every other domain fails at its own state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResolutionDomain {
    Color,
    CreatureType,
    CardType,
    CardName,
    LandType,
    Number,
    Player,
    LandOrNonland,
    Object,
}

impl ResolutionDomain {
    fn state(self) -> &'static str {
        match self {
            Self::Color => "choose.domain.color",
            Self::CreatureType => "choose.domain.creature-type",
            Self::CardType => "choose.domain.card-type",
            Self::CardName => "choose.domain.card-name",
            Self::LandType => "choose.domain.land-type",
            Self::Number => "choose.domain.number",
            Self::Player => "choose.domain.player",
            Self::LandOrNonland => "choose.domain.land-or-nonland",
            Self::Object => "choose.domain.object",
        }
    }

    fn expectation(self) -> &'static str {
        match self {
            Self::Color => "an unrestricted color choice",
            Self::CreatureType => "an unrestricted creature-type choice",
            Self::CardType => "an executable binding consumer for a chosen card type",
            Self::CardName => "`a [CHARACTERISTICS] card name [other than a basic land card name]`",
            Self::LandType => "an executable binding consumer for a chosen land type",
            Self::Number => "an executable binding consumer for a chosen number",
            Self::Player => "`an opponent` or `a player`, optionally `at random`",
            Self::LandOrNonland => "an executable binding consumer for a chosen land/nonland kind",
            Self::Object => "an executable binding consumer for objects chosen on resolution",
        }
    }
}

/// `of the chosen color` / `of that color`: the postnominal qualifier that
/// reads the color an earlier `choose a color` of the same effect program
/// bound. Lexical lookahead only; the program lowering proves the producer.
/// Outside an effect program only `of the chosen color` is claimed: its
/// producer is the `As ... enters` choice the card-level binding proves
/// (`entry_choice_binding.rs`); the anaphoric `of that color` is not.
pub(in crate::oracle::canonical) fn chosen_color_qualifier_length(
    cursor: &OracleTokenCursor<'_>,
) -> Option<usize> {
    if !word_at(cursor, 0, "of") {
        return None;
    }
    let in_program = super::resolution_choice_in_scope(cursor);
    let determiner = if word_at(cursor, 1, "the") && word_at(cursor, 2, "chosen") {
        3
    } else if in_program && word_at(cursor, 1, "that") {
        2
    } else {
        return None;
    };
    word_at(cursor, determiner, "color").then_some(determiner + 1)
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// Classifies `a|an|any NOUN ...` resolution domains by their typed head.
fn classify_resolution_domain(cursor: &OracleTokenCursor<'_>) -> Option<ResolutionDomain> {
    if word_at(cursor, 0, "land") && word_at(cursor, 1, "or") && word_at(cursor, 2, "nonland") {
        return Some(ResolutionDomain::LandOrNonland);
    }
    if !(word_at(cursor, 0, "a") || word_at(cursor, 0, "an")) {
        return None;
    }
    // The noun phrase after the determiner names the domain; a qualified card
    // name (`a nonland card name`, `a nonartifact, nonland card name`) keeps
    // its trailing `name` head. A comma joins characteristics of the phrase,
    // never a following `, then` instruction.
    let phrase_end = (1..10)
        .take_while(|offset| {
            cursor.peek_ahead(*offset).is_some_and(|raw| {
                raw.chars().all(|c| c.is_alphanumeric() || c == '-')
                    || (raw == ","
                        && !word_at(cursor, offset + 1, "then")
                        && cursor.peek_ahead(offset + 1).is_some_and(|next| {
                            next.chars().all(|c| c.is_alphanumeric() || c == '-')
                        }))
            })
        })
        .last()
        .unwrap_or(0);
    let has = |word: &str| (1..=phrase_end).any(|offset| word_at(cursor, offset, word));
    Some(if word_at(cursor, 1, "color") {
        ResolutionDomain::Color
    } else if word_at(cursor, 1, "number") {
        ResolutionDomain::Number
    } else if word_at(cursor, 1, "creature") && word_at(cursor, 2, "type") {
        ResolutionDomain::CreatureType
    } else if has("name") {
        ResolutionDomain::CardName
    } else if (word_at(cursor, 1, "card") || word_at(cursor, 1, "permanent"))
        && word_at(cursor, 2, "type")
    {
        ResolutionDomain::CardType
    } else if (word_at(cursor, 1, "land") || word_at(cursor, 1, "basic")) && has("type") {
        ResolutionDomain::LandType
    } else if word_at(cursor, 1, "opponent") || word_at(cursor, 1, "player") {
        ResolutionDomain::Player
    } else {
        ResolutionDomain::Object
    })
}

/// `a creature type` / `a color`: an unrestricted characteristic domain.
/// An exclusion (`other than Wall`) narrows the legal set and has no
/// executable producer yet, so it fails at the domain's own state.
fn recognize_characteristic_domain(
    cursor: &mut OracleTokenCursor<'_>,
    phrase: &[&str],
    domain: ResolutionDomain,
) -> MachineResult<()> {
    cursor.enter(domain.state());
    cursor.consume_phrase(phrase)?;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("other"))
    {
        cursor.enter(match domain {
            ResolutionDomain::CreatureType => "choose.domain.creature-type.exclusion",
            _ => "choose.domain.color.exclusion",
        });
        return Err(cursor.fail(domain.expectation()));
    }
    Ok(())
}

/// `an opponent` / `a player`, optionally `at random` (CR 608.2d). Any
/// other player phrase (`another player`, `an opponent who ...`) fails at
/// this state instead of being widened.
fn recognize_player_choice(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<PlayerChoiceNode> {
    cursor.enter("choose.domain.player");
    let population = if word_at(cursor, 0, "an") && word_at(cursor, 1, "opponent") {
        PlayerChoicePopulation::Opponent
    } else if word_at(cursor, 0, "a") && word_at(cursor, 1, "player") {
        PlayerChoicePopulation::Player
    } else {
        return Err(cursor.fail(ResolutionDomain::Player.expectation()));
    };
    cursor.advance_to(cursor.position() + 2)?;
    cursor.enter("choose.domain.player.selection");
    let random = word_at(cursor, 0, "at") && word_at(cursor, 1, "random");
    if random {
        cursor.consume_phrase(&["at", "random"])?;
    }
    cursor.enter("choose.domain.player.boundary");
    cursor.require(
        cursor
            .peek()
            .is_none_or(|raw| matches!(raw, "." | "," | ";")),
        "the end of the player choice; a qualified player choice is not read",
    )?;
    Ok(PlayerChoiceNode { population, random })
}

/// `a [CHARACTERISTIC[,] ...] card name [other than a basic land card name]`
/// (CR 201.3): the admitted names are those of cards with the shared card
/// characteristics. Each characteristic is the shared predicate atom; the
/// list is their intersection. Anything else fails at this state. The same
/// domain serves a resolution choice and a choice stored as a permanent
/// enters (`As this creature enters, choose a nonland card name`).
pub(in crate::oracle::canonical) fn recognize_card_name_domain(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardNameDomainNode> {
    cursor.enter(ResolutionDomain::CardName.state());
    cursor.require(
        word_at(cursor, 0, "a") || word_at(cursor, 0, "an") || word_at(cursor, 0, "any"),
        ResolutionDomain::CardName.expectation(),
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    let mut atoms = Vec::new();
    while !word_at(cursor, 0, "card") {
        cursor.enter("choose.domain.card-name.characteristic");
        atoms.push(recognize_object_predicate_atom(
            cursor,
            AcceptedForms::SingularOnly,
        )?);
        if cursor.peek() == Some(",") {
            cursor.consume(",")?;
        }
    }
    cursor.enter("choose.domain.card-name.noun");
    cursor.consume_phrase(&["card", "name"])?;
    if word_at(cursor, 0, "other") {
        // `other than a basic land card name`: the basic land names are
        // excluded from the admitted names.
        cursor.enter("choose.domain.card-name.exclusion");
        cursor.consume_phrase(&["other", "than", "a", "basic", "land", "card", "name"])?;
        atoms.push(ObjectPredicate::Not(Box::new(
            ObjectPredicate::Intersection(vec![
                ObjectPredicate::Supertype(CardSupertype::Basic),
                ObjectPredicate::CardType(ObjectCardType::Land),
            ]),
        )));
    }
    cursor.enter("choose.domain.card-name.boundary");
    cursor.require(
        cursor
            .peek()
            .is_none_or(|raw| matches!(raw, "." | "," | ";")),
        "the end of the card-name choice",
    )?;
    Ok(CardNameDomainNode {
        predicate: match atoms.len() {
            0 => None,
            1 => atoms.pop(),
            _ => Some(ObjectPredicate::Intersection(atoms)),
        },
    })
}

/// Recognizes `choose DOMAIN`. Sentence punctuation stays with the sequence.
pub(in crate::oracle::canonical) fn recognize_choose_instruction(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ChooseInstructionNode> {
    cursor.enter("choose.verb");
    let mut trial = cursor.clone();
    trial.consume("choose")?;
    cursor.enter("choose.domain");
    let quantity_head = trial
        .peek_token()
        .is_some_and(|token| token.token_type == "quantity");
    // A cardinality (`two`, `up to one`) heads a target list only when the
    // `target` head follows it; otherwise it counts modes.
    let target_after = |offset: usize| {
        word_at(&trial, offset, "target")
            || (word_at(&trial, offset, "other") && word_at(&trial, offset + 1, "target"))
    };
    // A typed count (`any number of`, `X`, `that many`, `up to X`) heads a
    // target list only when the `target` head follows it.
    let expressed_count_width = {
        let mut probe = trial.clone();
        (super::expressed_target_count_starts(&probe)
            && super::recognize_target_count(&mut probe).is_ok())
        .then(|| probe.position() - trial.position())
    };
    let explicit_target = trial.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case("target")
            || raw.eq_ignore_ascii_case("another")
            || (raw.eq_ignore_ascii_case("any") && word_at(&trial, 1, "target"))
    }) || (quantity_head && target_after(1))
        || (word_at(&trial, 0, "up") && word_at(&trial, 1, "to") && target_after(3))
        || expressed_count_width.is_some_and(target_after);
    if !explicit_target {
        if word_at(&trial, 0, "any") && word_at(&trial, 1, "number") && word_at(&trial, 2, "of") {
            *cursor = trial;
            cursor.enter("choose.domain.target.cardinality");
            return Err(cursor.fail("a `target` head after the declared target count"));
        }
        if quantity_head
            || (word_at(&trial, 0, "any") && word_at(&trial, 1, "number"))
            || word_at(&trial, 0, "up")
        {
            *cursor = trial;
            cursor.enter("choose.domain.mode");
            return Err(cursor.fail("a mode choice owned by the modal ability parent"));
        }
        if let Some(domain) = classify_resolution_domain(&trial) {
            let bound = match domain {
                ResolutionDomain::CreatureType => {
                    recognize_characteristic_domain(&mut trial, &["a", "creature", "type"], domain)
                        .map(|()| ChoiceDomainNode::CreatureType)
                }
                ResolutionDomain::Color => {
                    recognize_characteristic_domain(&mut trial, &["a", "color"], domain)
                        .map(|()| ChoiceDomainNode::Color)
                }
                ResolutionDomain::Player => {
                    recognize_player_choice(&mut trial).map(ChoiceDomainNode::Player)
                }
                ResolutionDomain::CardName => {
                    recognize_card_name_domain(&mut trial).map(ChoiceDomainNode::CardName)
                }
                _ => {
                    *cursor = trial;
                    cursor.enter(domain.state());
                    return Err(cursor.fail(domain.expectation()));
                }
            };
            *cursor = trial;
            let domain = bound?;
            cursor.enter("choose.complete");
            return Ok(ChooseInstructionNode {
                chooser: ChooserNode::Controller,
                domain,
            });
        }
        *cursor = trial;
        return Err(cursor.fail("a typed choice domain"));
    }
    cursor.enter("choose.domain.target");
    // The declared permanent slots are lowered by the shared permanent
    // subject lowering, which reads a typed target count.
    let targets = recognize_target_list_with_counts(&mut trial, TargetCountSupport::Expressed)?;
    let domain = if targets
        .slots
        .iter()
        .all(|slot| selector_guarantees_permanent(&slot.selector))
    {
        ChoiceDomainNode::PermanentTargets(targets)
    } else if let [slot] = targets.slots.as_slice()
        && matches!(
            slot.selector,
            TargetSelector::Player(PlayerPredicate::Any | PlayerPredicate::Opponent)
        )
        && slot.cardinality.minimum == 1
        && slot.cardinality.maximum == 1
        && !slot.exclude_source
    {
        ChoiceDomainNode::PlayerTarget(targets)
    } else {
        *cursor = trial;
        cursor.enter("choose.domain.target.kind");
        return Err(cursor.fail(
            "battlefield-permanent target slots or one player target slot for the chosen target binding",
        ));
    };
    *cursor = trial;
    cursor.enter("choose.complete");
    Ok(ChooseInstructionNode {
        chooser: ChooserNode::Controller,
        domain,
    })
}

#[cfg(test)]
#[path = "choose_card_name_tests.rs"]
mod card_name_tests;
