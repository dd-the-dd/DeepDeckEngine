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
    PlayerPredicate, TargetListNode, TargetSelector, recognize_target_list,
    selector_guarantees_permanent,
};

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
            ChoiceDomainNode::CreatureType => None,
        }
    }

    pub(in crate::oracle::canonical) fn permanent_targets(&self) -> Option<&TargetListNode> {
        match &self.domain {
            ChoiceDomainNode::PermanentTargets(targets) => Some(targets),
            ChoiceDomainNode::PlayerTarget(_) | ChoiceDomainNode::CreatureType => None,
        }
    }
}

/// Resolution domains recognized by their head words. Each is a CR 608.2d
/// decision whose executable consumer (`the chosen color`, `that player`, ...)
/// is not yet represented, so it fails at its own state.
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
            Self::Color => "an executable binding consumer for a chosen color",
            Self::CreatureType => "an executable binding consumer for a chosen creature type",
            Self::CardType => "an executable binding consumer for a chosen card type",
            Self::CardName => "an executable binding consumer for a chosen card name",
            Self::LandType => "an executable binding consumer for a chosen land type",
            Self::Number => "an executable binding consumer for a chosen number",
            Self::Player => "an executable binding consumer for a player chosen on resolution",
            Self::LandOrNonland => "an executable binding consumer for a chosen land/nonland kind",
            Self::Object => "an executable binding consumer for objects chosen on resolution",
        }
    }
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
    // name (`a nonland card name`) keeps its trailing `name` head.
    let phrase_end = (1..8)
        .take_while(|offset| {
            cursor
                .peek_ahead(*offset)
                .is_some_and(|raw| raw.chars().all(|c| c.is_alphanumeric() || c == '-'))
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
    let explicit_target = trial.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case("target")
            || raw.eq_ignore_ascii_case("another")
            || (raw.eq_ignore_ascii_case("any") && word_at(&trial, 1, "target"))
    }) || (quantity_head && target_after(1))
        || (word_at(&trial, 0, "up") && word_at(&trial, 1, "to") && target_after(3));
    if !explicit_target {
        if word_at(&trial, 0, "any") && word_at(&trial, 1, "number") && word_at(&trial, 2, "of") {
            *cursor = trial;
            cursor.enter("choose.domain.target.cardinality");
            return Err(cursor.fail("a literal or `up to N` target cardinality"));
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
            if domain == ResolutionDomain::CreatureType {
                trial.consume_phrase(&["a", "creature", "type"])?;
                if trial
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("other"))
                {
                    *cursor = trial;
                    cursor.enter("choose.domain.creature-type.exclusion");
                    return Err(cursor.fail("an unrestricted creature-type choice"));
                }
                *cursor = trial;
                cursor.enter("choose.complete");
                return Ok(ChooseInstructionNode {
                    chooser: ChooserNode::Controller,
                    domain: ChoiceDomainNode::CreatureType,
                });
            }
            *cursor = trial;
            cursor.enter(domain.state());
            return Err(cursor.fail(domain.expectation()));
        }
        *cursor = trial;
        return Err(cursor.fail("a typed choice domain"));
    }
    cursor.enter("choose.domain.target");
    let targets = recognize_target_list(&mut trial)?;
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
