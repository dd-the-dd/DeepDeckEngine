//! The `choose DOMAIN` instruction frame. One frame reads every `choose`
//! instruction of an effect program: a chooser, a typed choice domain and the
//! symbolic binding that later references (`it`, `that creature`, `that
//! player`, `those creatures`, `that many`) resolve to through the enclosing
//! sequence's anaphor memory.
//!
//! Two decision moments exist and stay distinct:
//! * `choose target ...` declares targets (CR 115.1, 601.2c / 603.3d): the
//!   instruction is the target declaration itself and adds no resolution step;
//! * every other domain is a decision made while the effect resolves
//!   (CR 608.2d): a color, a creature type, a card type, a card name, a
//!   number, a player, permanents, or `land or nonland`.
//!
//! The chooser is the clause's performer: an imperative `choose` is the
//! ability controller's, and `each player chooses` / `target opponent
//! chooses` is read by the shared performer cell in the performer's person
//! (`they control` is the chooser's `you control`), which puts the performer
//! on every primitive of the instruction.
//!
//! The domain child is classified before any resolution domain is accepted, so
//! an unsupported domain fails at its own automaton state instead of being
//! approximated. Mode choices (`choose one —`) belong to the modal parent.
//!
//! ```text
//! OBJECTS   := COUNT QUERY [REMAINDER]          (battlefield permanents)
//! COUNT     := "a" | "an" | N | "up" "to" N
//! REMAINDER := [","] ("then" | "and") VERB "the" "rest"
//!            | "." VERB "the" "rest"             (VERB: sacrifice | destroy)
//! NUMBER    := "a" "number" ("between" N "and" M | "from" N "to" M)
//! ```
use super::super::{MachineResult, OracleTokenCursor, parse_number_word};
use super::{
    CardSupertype, ExpressedTargetCardinality, GrammaticalNumber, LocalEffectNode, NumericAtom,
    ObjectCardType, ObjectKind, ObjectPredicate, ObjectQueryNode, ObjectSetAntecedent,
    ObjectSetDenotation, PerformerNode, PermanentEffectReference, PlayerPredicate, TargetBinding,
    TargetCountSupport, TargetListNode, TargetSelector, VerbForm, VerbLemma, lookup_verb,
    recognize_expressed_target_cardinality, recognize_object_collection,
    recognize_object_predicate_atom, recognize_object_query, recognize_target_list_with_counts,
    selector_guarantees_permanent,
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
    /// `choose a creature you control`, `chooses up to two creatures they
    /// control`: permanents chosen while the effect resolves (CR 608.2d),
    /// not targets (CR 115.1). The chosen objects are the antecedent of a
    /// later `it` / `that creature` (one chooser, one object) or `those
    /// creatures` (several objects or several choosers).
    Objects(ObjectChoiceNode),
    /// `choose a number between 0 and 10`: an integer chosen while the
    /// effect resolves (CR 608.2d, 107.1), read later as `that many` /
    /// `that much` through the shared action-result quantity.
    Number(NumberChoiceNode),
}

/// What happens to the permanents that were not chosen (`then sacrifices
/// the rest`, `Destroy the rest.`, `Destroy all other permanents.`). `the
/// rest` is the complement of the chosen objects within each chooser's own
/// domain; `all other QUERY` is the complement within a stated query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ChoiceRemainderAction {
    /// CR 701.21a: each unchosen permanent is sacrificed by its controller.
    Sacrifice,
    /// CR 701.8a: each unchosen permanent is destroyed.
    Destroy,
    /// `destroy all other QUERY`: every permanent of QUERY that no chooser
    /// chose is destroyed (CR 701.8a).
    DestroyOthers(ObjectQueryNode),
}

/// A resolution-time choice of battlefield permanents: how many, among
/// which permanents (the shared object query, read in the chooser's
/// person), and the action applied to the unchosen ones, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ObjectChoiceNode {
    pub quantity: ExpressedTargetCardinality,
    pub query: ObjectQueryNode,
    /// The selector a nominal anaphor of the chosen object (`that
    /// creature`) must follow from, built from the same query.
    pub selector: TargetSelector,
    pub remainder: Option<ChoiceRemainderAction>,
    /// `at random`: the game picks the objects among the candidates
    /// instead of the chooser (CR 608.2d); the count and query are the same.
    pub random: bool,
}

impl ObjectChoiceNode {
    /// At most one object: a singular antecedent.
    pub(in crate::oracle::canonical) fn singular(&self) -> bool {
        self.quantity.maximum == NumericAtom::Integer(1)
    }
}

/// `a number between MINIMUM and MAXIMUM` (inclusive bounds, CR 107.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct NumberChoiceNode {
    pub minimum: i64,
    pub maximum: i64,
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
            | ChoiceDomainNode::CardName(_)
            | ChoiceDomainNode::Objects(_)
            | ChoiceDomainNode::Number(_) => None,
        }
    }

    pub(in crate::oracle::canonical) fn permanent_targets(&self) -> Option<&TargetListNode> {
        match &self.domain {
            ChoiceDomainNode::PermanentTargets(targets) => Some(targets),
            ChoiceDomainNode::PlayerTarget(_)
            | ChoiceDomainNode::CreatureType
            | ChoiceDomainNode::Color
            | ChoiceDomainNode::Player(_)
            | ChoiceDomainNode::CardName(_)
            | ChoiceDomainNode::Objects(_)
            | ChoiceDomainNode::Number(_) => None,
        }
    }
}

/// The chosen-object binding of a program action is published under the
/// same symbolic key a declared slot of that action would use: the action's
/// index and slot zero. Its value is no declared target: the program
/// lowering binds it to the resolution result
/// (`effects::resolution_object_choice`), never to a target designation.
pub(in crate::oracle::canonical) fn resolution_choice_binding(
    effect_index: usize,
) -> TargetBinding {
    TargetBinding {
        effect_index,
        slot_index: 0,
    }
}

/// The resolution-time permanent choice an action makes, looking through a
/// performer (`each player chooses ...`), and whether one player chooses.
pub(in crate::oracle::canonical) fn resolution_object_choice(
    effect: &LocalEffectNode,
) -> Option<(&ObjectChoiceNode, bool)> {
    match effect {
        LocalEffectNode::Choose(ChooseInstructionNode {
            domain: ChoiceDomainNode::Objects(choice),
            ..
        }) => Some((choice, true)),
        LocalEffectNode::Performed(node) => match node.effect.as_ref() {
            LocalEffectNode::Choose(ChooseInstructionNode {
                domain: ChoiceDomainNode::Objects(choice),
                ..
            }) => Some((
                choice,
                matches!(
                    node.performer,
                    PerformerNode::Player(_) | PerformerNode::Chosen { .. }
                ),
            )),
            _ => None,
        },
        _ => None,
    }
}

/// The singular permanent antecedent an object choice leaves for a later
/// `it` / `that creature`: one chooser choosing at most one object, whose
/// chosen object stays bound (no `the rest` action consumes the choice).
pub(in crate::oracle::canonical) fn resolution_choice_subject(
    effect: &LocalEffectNode,
    effect_index: usize,
) -> Option<PermanentEffectReference> {
    let (choice, one_chooser) = resolution_object_choice(effect)?;
    (one_chooser && choice.singular() && choice.remainder.is_none())
        .then(|| PermanentEffectReference::Target(resolution_choice_binding(effect_index)))
}

/// The selector of the chosen object at `slot_index`, for a nominal
/// anaphor (`that creature`) of the choice.
pub(in crate::oracle::canonical) fn resolution_choice_selector(
    effect: &LocalEffectNode,
    slot_index: usize,
) -> Option<&TargetSelector> {
    let (choice, _) = resolution_object_choice(effect)?;
    (slot_index == 0).then_some(&choice.selector)
}

/// The plural set a choice of several objects (or by several choosers)
/// leaves for `those creatures` / `them`: every object chosen.
pub(in crate::oracle::canonical) fn resolution_choice_object_set(
    effect: &LocalEffectNode,
    effect_index: usize,
) -> Option<ObjectSetAntecedent> {
    let (choice, one_chooser) = resolution_object_choice(effect)?;
    if choice.remainder.is_some() || (one_chooser && choice.singular()) {
        return None;
    }
    Some(ObjectSetAntecedent {
        denotes: ObjectSetDenotation::Bound(PermanentEffectReference::TargetSet(
            resolution_choice_binding(effect_index),
        )),
        selector: Some(choice.selector.clone()),
        number: GrammaticalNumber::Plural,
        tokens: false,
    })
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
            Self::Number => "`a number between N and M` or `a number from N to M`",
            Self::Player => "`an opponent` or `a player`, optionally `at random`",
            Self::LandOrNonland => "an executable binding consumer for a chosen land/nonland kind",
            Self::Object => {
                "a counted permanent query (`a creature you control`, `up to two lands they control`)"
            }
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

/// `COUNT QUERY`: a resolution choice of battlefield permanents (CR
/// 608.2d). The count is `a`/`an`, a literal or `up to N`; the query is the
/// shared object query, already read in the chooser's person by a performer
/// clause. A coordinated list of articled heads (`an artifact, a creature,
/// and a land`) is one choice per head, not one choice among their union,
/// and fails at its own state; so do cards in another zone, a random
/// selection and a qualifier this frame does not read.
fn recognize_object_domain(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<ObjectChoiceNode> {
    cursor.enter(ResolutionDomain::Object.state());
    let quantity = if word_at(cursor, 0, "a") || word_at(cursor, 0, "an") {
        let article = cursor.peek().unwrap_or("a").to_string();
        cursor.consume(&article)?;
        ExpressedTargetCardinality::EXACTLY_ONE
    } else {
        cursor.enter("choose.domain.object.count");
        recognize_expressed_target_cardinality(cursor)?
    };
    cursor.enter("choose.domain.object.query");
    let query_start = cursor.position();
    let query = recognize_object_query(cursor)?;
    cursor.enter("choose.domain.object.each-head");
    // An article inside the query heads another object noun; an article of a
    // player phrase (`a creature an opponent controls`) does not.
    let query_tokens = cursor.token_slice(query_start, cursor.position());
    let repeated_head = query_tokens.iter().enumerate().any(|(index, token)| {
        (token.raw.eq_ignore_ascii_case("a") || token.raw.eq_ignore_ascii_case("an"))
            && !query_tokens.get(index + 1).is_some_and(|next| {
                ["opponent", "player", "single"]
                    .iter()
                    .any(|word| next.raw.eq_ignore_ascii_case(word))
            })
    });
    cursor.require(
        !repeated_head,
        "one counted query; one object of each coordinated head is a choice per head",
    )?;
    cursor.enter("choose.domain.object.zone");
    if !query.denotes_battlefield_permanents()
        && query.predicate.is_in_domain(ObjectKind::Card)
        && (word_at(cursor, 0, "in")
            || word_at(cursor, 0, "from")
            || super::card_location_starts(cursor))
    {
        // The controller's own choice of located cards (`choose a card in
        // your graveyard`, `choose a card exiled with this artifact`) is the
        // card action's chosen-card binding; this frame is reached for a
        // performer's choice or a location the card action does not read.
        cursor.enter("choose.domain.object.zone.card-location");
        return Err(cursor.fail(
            "a card choice the card-action frame binds: the controller choosing cards in a named zone or a linked exile",
        ));
    }
    cursor.require(
        query.denotes_battlefield_permanents(),
        "permanents on the battlefield; a card choice names its zone (`in your graveyard`, `exiled with this artifact`)",
    )?;
    // `at random` (CR 608.2d): the game picks among the same candidates.
    // Only an exact count is picked at random; a range has no random size.
    cursor.enter("choose.domain.object.selection");
    let random = word_at(cursor, 0, "at") && word_at(cursor, 1, "random");
    if random {
        cursor.require(
            quantity.minimum == quantity.maximum,
            "an exact number of permanents picked at random",
        )?;
        cursor.consume_phrase(&["at", "random"])?;
    }
    let selector = match query.controller {
        Some(controller) => TargetSelector::ControlledObject {
            predicate: query.predicate.clone(),
            controller,
        },
        None => TargetSelector::Object(query.predicate.clone()),
    };
    let remainder = recognize_choice_remainder(cursor)?;
    cursor.enter("choose.domain.object.boundary");
    // A comma closes the choice only before a coordinated instruction
    // (`, then ...`); a comma before another noun continues a list of heads.
    let view: &OracleTokenCursor<'_> = cursor;
    let connector_at =
        |offset: usize| word_at(view, offset, "and") || word_at(view, offset, "then");
    view.require(
        view.peek().is_none_or(|raw| {
            matches!(raw, "." | ";")
                || (remainder.is_none() && ((raw == "," && connector_at(1)) || connector_at(0)))
        }),
        "the end of the permanent choice; a further qualifier of the chosen permanents is not read",
    )?;
    Ok(ObjectChoiceNode {
        quantity,
        query,
        selector,
        remainder,
        random,
    })
}

/// `[,] then|and VERB the rest` in the same clause, or `. Destroy the
/// rest.` as the next sentence, or `destroy all other QUERY` in either
/// position: the action applied to the permanents that were not chosen (CR
/// 608.2c). Right after a choice, `other` names the unchosen objects, not
/// objects other than the ability's source: any other clause opening with
/// `other` fails here instead of being read with a source exclusion. Nothing
/// is consumed unless the whole remainder clause is read.
fn recognize_choice_remainder(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<ChoiceRemainderAction>> {
    let mut probe = cursor.clone();
    let next_sentence = probe.peek() == Some(".");
    if next_sentence {
        probe.consume(".")?;
    } else {
        if probe.peek() == Some(",") {
            probe.consume(",")?;
        }
        if word_at(&probe, 0, "then") || word_at(&probe, 0, "and") {
            let connector = probe.peek().unwrap_or("then").to_string();
            probe.consume(&connector)?;
        } else {
            return Ok(None);
        }
    }
    // `other` in the opening words of the following clause (`Other
    // creatures they control can't block`, `each other creature you
    // control becomes ...`, `destroy all other permanents`).
    let names_other = (0..3).any(|offset| {
        word_at(&probe, offset, "other")
            && !["player", "players", "opponent", "opponents"]
                .iter()
                .any(|player| word_at(&probe, offset + 1, player))
    });
    let lemmas = if next_sentence {
        // The next sentence is the controller's own instruction: only a
        // destruction is independent of who controls the unchosen permanents.
        &[VerbLemma::Destroy][..]
    } else {
        &[VerbLemma::Sacrifice, VerbLemma::Destroy][..]
    };
    let verb = probe
        .peek()
        .and_then(|raw| lookup_verb(raw, lemmas))
        .filter(|(_, forms)| forms.contains(&VerbForm::Base))
        .map(|(entry, _)| entry.lemma);
    let action = match verb {
        Some(lemma) if word_at(&probe, 1, "the") && word_at(&probe, 2, "rest") => {
            probe.advance_to(probe.position() + 3)?;
            cursor.enter("choose.domain.object.remainder");
            if lemma == VerbLemma::Sacrifice {
                ChoiceRemainderAction::Sacrifice
            } else {
                ChoiceRemainderAction::Destroy
            }
        }
        Some(VerbLemma::Destroy) if word_at(&probe, 1, "all") && word_at(&probe, 2, "other") => {
            probe.advance_to(probe.position() + 3)?;
            cursor.enter("choose.domain.object.remainder.others");
            // `all other creatures and planeswalkers`: the shared coordinated
            // set reader, as for any universal collection.
            let query = recognize_object_collection(&mut probe)?;
            probe.require(
                query.controller.is_none() && !query.exclude_source,
                "an unrestricted query of the permanents no chooser chose",
            )?;
            ChoiceRemainderAction::DestroyOthers(query)
        }
        _ if names_other => {
            *cursor = probe;
            cursor.enter("choose.domain.object.other");
            return Err(cursor.fail(
                "`the rest` or `destroy all other QUERY`; after a choice, `other` names the unchosen permanents",
            ));
        }
        _ => return Ok(None),
    };
    *cursor = probe;
    cursor.require(
        cursor.peek().is_none_or(|raw| matches!(raw, "." | ";")),
        "the end of the sentence after the unchosen permanents",
    )?;
    Ok(Some(action))
}

/// `a number between N and M` / `a number from N to M` (CR 107.1, inclusive
/// bounds). `a number of OBJECTS` is a counted object choice, and an
/// unbounded or random number has no decision protocol here: each fails at
/// its own state.
fn recognize_number_domain(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<NumberChoiceNode> {
    cursor.enter(ResolutionDomain::Number.state());
    let article = cursor.peek().unwrap_or("a").to_string();
    cursor.consume(&article)?;
    cursor.consume("number")?;
    cursor.enter("choose.domain.number.range");
    let (open, close) = if word_at(cursor, 0, "between") {
        ("between", "and")
    } else if word_at(cursor, 0, "from") {
        ("from", "to")
    } else if word_at(cursor, 0, "of") {
        cursor.enter("choose.domain.number.objects");
        return Err(cursor
            .fail("a bounded number; `a number of OBJECTS` counted by an expression is not read"));
    } else {
        return Err(cursor.fail(ResolutionDomain::Number.expectation()));
    };
    cursor.consume(open)?;
    let minimum = recognize_number_bound(cursor)?;
    cursor.consume(close)?;
    let maximum = recognize_number_bound(cursor)?;
    cursor.require(
        maximum >= minimum,
        "an upper bound not below the lower bound",
    )?;
    cursor.enter("choose.domain.number.boundary");
    cursor.require(
        !(word_at(cursor, 0, "at") && word_at(cursor, 1, "random")),
        "a number chosen by the chooser; a random number is not read",
    )?;
    cursor.require(
        cursor
            .peek()
            .is_none_or(|raw| matches!(raw, "." | "," | ";")),
        "the end of the number choice",
    )?;
    Ok(NumberChoiceNode { minimum, maximum })
}

/// One literal bound of a number choice; an article is not a bound.
fn recognize_number_bound(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<i64> {
    cursor.enter("choose.domain.number.bound");
    let value = cursor
        .peek()
        .filter(|raw| !raw.eq_ignore_ascii_case("a") && !raw.eq_ignore_ascii_case("an"))
        .and_then(parse_number_word)
        .filter(|value| *value >= 0);
    let value = cursor.require_value(value, "a nonnegative literal number")?;
    cursor.advance_to(cursor.position() + 1)?;
    Ok(value)
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
        // A count followed by a noun counts permanents (`chooses six lands
        // they control`); a count closing the clause (`choose one —`, `choose
        // two.`, `choose one or both`, `choose one of ...`) counts modes or
        // members of another collection.
        let count_width = if quantity_head {
            Some(1)
        } else if word_at(&trial, 0, "up") && word_at(&trial, 1, "to") {
            Some(3)
        } else {
            None
        };
        let counts_objects = count_width.is_some_and(|width| {
            trial.peek_ahead(width).is_some_and(|raw| {
                raw.chars().all(char::is_alphabetic)
                    && !["or", "of", "at", "more", "and", "that", "each"]
                        .iter()
                        .any(|word| raw.eq_ignore_ascii_case(word))
            })
        });
        if counts_objects {
            let bound = recognize_object_domain(&mut trial);
            *cursor = trial;
            let choice = bound?;
            cursor.enter("choose.complete");
            return Ok(ChooseInstructionNode {
                chooser: ChooserNode::Controller,
                domain: ChoiceDomainNode::Objects(choice),
            });
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
                ResolutionDomain::Object => {
                    recognize_object_domain(&mut trial).map(ChoiceDomainNode::Objects)
                }
                ResolutionDomain::Number => {
                    recognize_number_domain(&mut trial).map(ChoiceDomainNode::Number)
                }
                ResolutionDomain::CardType
                | ResolutionDomain::LandType
                | ResolutionDomain::LandOrNonland => {
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

#[cfg(test)]
#[path = "choose_object_domain_tests.rs"]
mod object_domain_tests;
