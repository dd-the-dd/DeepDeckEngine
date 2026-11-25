//! Program composition is independent of the enclosing ability and action.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) enum EffectProgramNode {
    Sequence(EffectSequenceNode),
    Optional {
        player: LocalPlayerReference,
        body: Box<EffectProgramNode>,
    },
    /// `[PLAYER may] PRODUCER. (If you do | If you don't | When you do),
    /// BODY`: one outcome block for every producer. The producer is a
    /// resolution cost or an ordinary instruction; the connector resolves
    /// against its ActionResult antecedent (`binding_environment.rs`) and
    /// observes whether it was actually performed (CR 608.2c, 603.12), not
    /// merely chosen. Producer and body are each bounded to one sentence.
    Outcome {
        /// `PLAYER may`: the producer is optional. `None`: a mandatory
        /// instruction whose measured performance gates the body.
        permission: Option<LocalPlayerReference>,
        producer: OutcomeProducer,
        outcome: PerformanceOutcome,
        body: Box<EffectProgramNode>,
        /// `If you do, A. If you don't, B.`: the complementary outcome's own
        /// sentence, which resolves exactly when `body` does not (CR 608.2c).
        /// Only an optional producer has one; a reflexive outcome never does.
        otherwise: Option<Box<EffectProgramNode>>,
        /// Which part of an instruction producer the outcome observes.
        scope: ProducerScope,
        /// `When you discard a nonland card this way,`: the reflexive
        /// ability triggers on the objects the producer affected that the
        /// noun names, instead of on its performance (CR 603.12).
        result: Option<ReflexiveResultNode>,
    },
    /// Independent sentence groups in written order. An outcome continuation
    /// conditions its own sentence only (CR 608.2c); later sentences are
    /// ordinary instructions. Steps are flat: no step is itself `Steps`.
    Steps(Vec<EffectProgramNode>),
    /// `PLAYER may VERB or VERB OBJECT` (`You may tap or untap target
    /// creature.`): the performer chooses one of the coordinated actions
    /// over the same written object, or neither. Each option is the shared
    /// sequence of its own verb over the same tokens, so the object is one
    /// declaration whichever action is chosen (CR 601.2c, 608.2c).
    Alternatives {
        player: LocalPlayerReference,
        /// The base verb of each option and its sequence, in written order.
        options: Vec<(&'static str, EffectSequenceNode)>,
    },
    /// `any [other] player|opponent may INSTRUCTION. If a player does, A.
    /// If no one does, B.` (CR 608.2d, 101.4): the instruction is offered to
    /// each player of the population in turn order until one performs it.
    /// The instruction is read in the performer's person (`their` reads as
    /// the performer's own); the outcomes observe whether any player
    /// actually performed it, and resolve as the controller's instructions.
    Offer {
        players: PlayerSetReferenceNode,
        /// `any other player`: the population minus the ability controller.
        excluding_controller: bool,
        producer: EffectSequenceNode,
        /// `If a player does, ...`.
        performed: Option<EffectSequenceNode>,
        /// `If no one does, ...` / `If no player does, ...`.
        declined: Option<EffectSequenceNode>,
    },
}

/// The part of an instruction producer an outcome connector observes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ProducerScope {
    /// The producing sentence, as one instruction group (`discard your hand
    /// and draw two cards. If you do, ...`).
    Sentence,
    /// The producer also reads the earlier sentences whose antecedents its
    /// own sentence names (`Exile the top card of that player's library. If
    /// it's a permanent card, you may put it onto the battlefield. If you
    /// don't, ...`): they run first, unmeasured, and the outcome observes the
    /// final instruction only.
    FinalInstruction,
}

/// What an outcome block's connector observes.
#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) enum OutcomeProducer {
    /// A cost paid at resolution (`you may pay {1}`).
    Cost(ResolutionCostNode),
    /// An ordinary instruction sentence (`you may draw a card`, `sacrifice a
    /// creature`), read by the shared effect sequence.
    Action(Box<EffectProgramNode>),
}

impl EffectProgramNode {
    pub(in crate::oracle::canonical) fn inner_sequence(&self) -> &EffectSequenceNode {
        match self {
            Self::Sequence(sequence) => sequence,
            // The outcome sentence is the last written sentence of the block.
            Self::Optional { body, .. } | Self::Outcome { body, .. } => body.inner_sequence(),
            // The last written sentence group: trailing reminders and limits
            // read its final action. Lowering visits every step instead.
            Self::Steps(steps) => steps
                .last()
                .expect("a program has at least one step")
                .inner_sequence(),
            // Every option reads the same written sentence.
            Self::Alternatives { options, .. } => {
                &options
                    .first()
                    .expect("alternatives have at least two options")
                    .1
            }
            // The last written outcome sentence, else the offered one.
            Self::Offer {
                producer,
                performed,
                declined,
                ..
            } => declined.as_ref().or(performed.as_ref()).unwrap_or(producer),
        }
    }
    /// Program steps in written order; a single program is its own one step.
    pub(in crate::oracle::canonical) fn steps(&self) -> &[EffectProgramNode] {
        match self {
            Self::Steps(steps) => steps,
            other => std::slice::from_ref(other),
        }
    }
    pub(in crate::oracle::canonical) fn action_count(&self) -> usize {
        match self {
            Self::Sequence(sequence) => sequence.effects.len(),
            Self::Optional { body, .. } => body.action_count(),
            Self::Outcome {
                producer: OutcomeProducer::Cost(_),
                body,
                otherwise,
                ..
            } => body.action_count() + otherwise.as_ref().map_or(0, |other| other.action_count()),
            Self::Outcome {
                producer: OutcomeProducer::Action(action),
                body,
                otherwise,
                ..
            } => {
                action.action_count()
                    + body.action_count()
                    + otherwise.as_ref().map_or(0, |other| other.action_count())
            }
            Self::Steps(steps) => steps.iter().map(Self::action_count).sum(),
            Self::Alternatives { options, .. } => options
                .iter()
                .map(|(_, sequence)| sequence.effects.len())
                .max()
                .unwrap_or(0),
            Self::Offer {
                producer,
                performed,
                declined,
                ..
            } => {
                producer.effects.len()
                    + performed.as_ref().map_or(0, |body| body.effects.len())
                    + declined.as_ref().map_or(0, |body| body.effects.len())
            }
        }
    }
}

/// The first bounded optional program owns one complete Oracle sentence.
/// Its `and`/`then` connectors belong to the shared inner effect sequence.
/// An optional cost owns its outcome sentence; any other sentence is an
/// independent program step before or after that block.
pub(in crate::oracle::canonical) fn recognize_effect_program_with_context(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectProgramNode> {
    // A program-initial `Then` restates that this sentence follows the
    // previous one (CR 608.2c): the program's step order already says so.
    // `Then you may ...` keeps its own permission reader.
    if sentence_then_width(cursor) == 1
        && !optional_program_starts(cursor)
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw != "." && raw != ",")
    {
        cursor.enter("program.steps.leading-then");
        cursor.consume("then")?;
    }
    // `Unless PLAYER pays COST, PROGRAM.` (CR 118.12): the preposed form of
    // the `unless` frame, guarding its own sentence.
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("unless"))
        && context.unless_frames == UnlessFrameSupport::SharedLowering
    {
        let block = recognize_preposed_unless(cursor, name_links, name_failure, context)?;
        return complete_outcome_block(cursor, block, name_links, name_failure, context);
    }
    // `any [other] player|opponent may ...`: an offer to every player of
    // the population in turn order (CR 608.2d).
    if offered_permission_width(cursor).is_some() {
        let offer = recognize_offered_program(cursor, name_links, name_failure, context)?;
        return complete_outcome_block(cursor, offer, name_links, name_failure, context);
    }
    if !optional_program_starts(cursor) {
        let sequence = cursor.attempt(|attempt| {
            recognize_effect_sequence_with_context(attempt, name_links, name_failure, context)
        });
        let failure = match sequence {
            Ok(sequence) => return Ok(EffectProgramNode::Sequence(sequence)),
            Err(failure) => failure,
        };
        // `If COND, BODY instead.` replaces the sentence before it (CR
        // 608.2c). It is read here when it ends before any outcome block;
        // an outcome body it amends is read by `recognize_outcome_body`.
        let outcome_start = if mandatory_outcome_follows_first_sentence(cursor) {
            Some(cursor.position())
        } else {
            match (
                later_optional_outcome_block(cursor),
                later_mandatory_outcome_block(cursor),
            ) {
                (Some(optional), Some(mandatory)) => Some(optional.min(mandatory)),
                (optional, mandatory) => optional.or(mandatory),
            }
        };
        let failure = match replacing_sentence(cursor)
            .or_else(|| super::postposed_replacing_sentence(cursor))
            .filter(|sentence| outcome_start.is_none_or(|start| sentence.end <= start))
        {
            Some(sentence) => match cursor.attempt(|attempt| {
                recognize_replacing_sequence(attempt, sentence, name_links, name_failure, context)
            }) {
                Ok(sequence) => {
                    return complete_outcome_block(
                        cursor,
                        EffectProgramNode::Sequence(sequence),
                        name_links,
                        name_failure,
                        context,
                    );
                }
                // Report the reading that progressed furthest.
                Err(replacing_failure) if replacing_failure.token_index > failure.token_index => {
                    replacing_failure
                }
                Err(_) => failure,
            },
            None => failure,
        };
        // A mandatory first sentence may be the producer of an `If you do,`
        // or reflexive `When you do,` outcome (CR 608.2c, 603.12).
        let failure = if mandatory_outcome_follows_first_sentence(cursor) {
            match cursor.attempt(|attempt| {
                recognize_mandatory_outcome_block(attempt, name_links, name_failure, context)
            }) {
                Ok(block) => {
                    return complete_outcome_block(
                        cursor,
                        block,
                        name_links,
                        name_failure,
                        context,
                    );
                }
                // Report the reading that progressed furthest.
                Err(block_failure) if block_failure.token_index > failure.token_index => {
                    block_failure
                }
                Err(_) => failure,
            }
        } else {
            failure
        };
        // `that player may ...`: another player's permission, read by the
        // optional program below when no ordinary sentence claimed it.
        let other_player_permission = other_player_permission_starts(cursor);
        // A later sentence may open an optional-cost block. The leading
        // sentences are then an ordinary, separately bounded program step.
        // The earliest later block: an optional-cost block or a mandatory
        // sentence followed by its outcome connector.
        let block_start = match (
            later_optional_outcome_block(cursor),
            later_mandatory_outcome_block(cursor),
        ) {
            (Some(optional), Some(reflexive)) => Some(optional.min(reflexive)),
            (optional, reflexive) => optional.or(reflexive),
        };
        let Some(block_start) = block_start.filter(|_| !other_player_permission) else {
            if other_player_permission {
                return recognize_other_player_permission(
                    cursor,
                    name_links,
                    name_failure,
                    context,
                    failure,
                );
            }
            return Err(failure);
        };
        cursor.enter("program.steps.leading");
        // The leading sentences are a program of their own: a replacing
        // sentence among them is read with the sentence it replaces.
        let split = cursor.attempt(|cursor| {
            let leading = cursor.bounded(block_start, |cursor| {
                recognize_effect_program_with_context(cursor, name_links, name_failure, context)
            })?;
            let mut steps = Vec::new();
            push_program_steps(&mut steps, leading);
            push_program_steps(
                &mut steps,
                recognize_effect_program_with_context(cursor, name_links, name_failure, context)?,
            );
            Ok(EffectProgramNode::Steps(steps))
        });
        let split_failure = match split {
            Ok(program) => return Ok(program),
            Err(failure) => failure,
        };
        // The producing sentence may read an antecedent a leading sentence
        // bound (`Exile the top card of that player's library. If it's a
        // permanent card, you may put it onto the battlefield. If you don't,
        // ...`): the leading sentences and the producer are then one
        // sequence, whose final instruction the outcome observes.
        cursor.enter("program.steps.extended-producer");
        let extended = cursor.attempt(|attempt| {
            recognize_instruction_outcome_block(
                attempt,
                block_start,
                ProducerScope::FinalInstruction,
                name_links,
                name_failure,
                context,
            )
        });
        return match extended {
            Ok(block) => complete_outcome_block(cursor, block, name_links, name_failure, context),
            // Report the reading that progressed furthest.
            Err(extended) if extended.token_index > split_failure.token_index => Err(extended),
            Err(_) => Err(split_failure),
        };
    }
    // Preserve already supported action-local permissions (such as optional
    // draw followed by another sentence) and their exact historical IR.
    // This speculative typed child owns no semantic commit until complete.
    if let Ok(sequence) = cursor.attempt(|attempt| {
        recognize_effect_sequence_with_context(attempt, name_links, name_failure, context)
    }) {
        return Ok(EffectProgramNode::Sequence(sequence));
    }
    recognize_permission_program(cursor, name_links, name_failure, context)
}

/// The width of `any [other] player|opponent` before `may` at the cursor,
/// with the population it offers to. Lexical lookahead only.
fn offered_permission_width(
    cursor: &OracleTokenCursor<'_>,
) -> Option<(usize, PlayerSetReferenceNode, bool)> {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if !word(0, "any") {
        return None;
    }
    let other = word(1, "other");
    let noun = 1 + usize::from(other);
    let players = if word(noun, "player") {
        PlayerSetReferenceNode::EachPlayer
    } else if word(noun, "opponent") {
        PlayerSetReferenceNode::OpponentsOfAbilityController
    } else {
        return None;
    };
    // `any other opponent` names no population the engine distinguishes
    // from `any opponent`; it stays unclaimed here.
    if other && players != PlayerSetReferenceNode::EachPlayer {
        return None;
    }
    word(noun + 1, "may").then_some((noun + 1, players, other))
}

/// The outcome of an offer at a sentence start, with its token width: `If a
/// player does,` / `If they do,` (a player performed it) or `If no one
/// does,` / `If no player does,` / `If no opponent does,` (nobody did).
fn offer_outcome_at(cursor: &OracleTokenCursor<'_>) -> Option<(PerformanceOutcome, usize)> {
    let words = (0..5)
        .map(|offset| cursor.peek_ahead(offset).map(str::to_ascii_lowercase))
        .collect::<Vec<_>>();
    let word = |offset: usize| words[offset].as_deref().unwrap_or("");
    if word(0) != "if" {
        return None;
    }
    match (word(1), word(2), word(3), word(4)) {
        ("a", "player", "does", ",") => Some((PerformanceOutcome::Performed, 5)),
        ("they", "do", ",", _) => Some((PerformanceOutcome::Performed, 4)),
        ("no", "one" | "player" | "opponent", "does", ",") => {
            Some((PerformanceOutcome::Declined, 5))
        }
        _ => None,
    }
}

/// `any [other] player|opponent may INSTRUCTION. [OUTCOME, BODY.]`
/// (CR 608.2d, 101.4). The instruction after `may` is the performer's own,
/// read in the performer's person by the shared person shift and the shared
/// effect sequence; a written `you` in it names the ability's controller
/// and fails explicitly. Each outcome owns its own sentence; a performed and
/// a declined outcome may both be written, once each.
fn recognize_offered_program(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectProgramNode> {
    cursor.enter("program.offer.population");
    let (width, players, excluding_controller) = offered_permission_width(cursor)
        .ok_or_else(|| cursor.fail("`any player`, `any other player` or `any opponent`"))?;
    cursor.advance_to(cursor.position() + width)?;
    cursor.enter("program.offer.permission");
    cursor.consume("may")?;
    let description = "a player offered the instruction (`any player may`)";
    let tokens = person_shifted_tokens(cursor, VerbAgreement::Base, description)?;
    let mut shifted = cursor.with_tokens(&tokens);
    shifted.enter("program.offer.instruction");
    let end = permission_sentence_end(&shifted)?;
    // CR 118.12: no `unless` payer is ordered with the offer.
    let mut instruction_context = context.clone();
    instruction_context.unless_frames = UnlessFrameSupport::Unsupported;
    let producer = shifted.bounded(end + 1, |body| {
        recognize_effect_sequence_with_context(body, name_links, name_failure, &instruction_context)
    })?;
    if let Some(ambiguous) = controller_reference_failure(cursor, cursor.position(), end) {
        return Err(ambiguous);
    }
    cursor.advance_to(end)?;
    cursor.enter("program.offer.sentence-end");
    cursor.consume(".")?;
    let mut performed = None;
    let mut declined = None;
    while let Some((outcome, connector)) = offer_outcome_at(cursor) {
        let slot = match outcome {
            PerformanceOutcome::Performed => &mut performed,
            PerformanceOutcome::Declined | PerformanceOutcome::Reflexive => &mut declined,
        };
        if slot.is_some() {
            cursor.enter("program.offer.outcome.repeated");
            return Err(cursor.fail("one outcome sentence of each kind after the offer"));
        }
        cursor.enter("program.offer.outcome.connector");
        cursor.advance_to(cursor.position() + connector)?;
        let body = recognize_outcome_body(
            cursor,
            "program.offer.outcome.body",
            outcome,
            name_links,
            name_failure,
            context,
        )?;
        let EffectProgramNode::Sequence(body) = body else {
            cursor.enter("program.offer.outcome.body-kind");
            return Err(cursor.fail("one ordinary sentence as the offer's outcome"));
        };
        *slot = Some(body);
    }
    cursor.enter("program.offer.complete");
    Ok(EffectProgramNode::Offer {
        players,
        excluding_controller,
        producer,
        performed,
        declined,
    })
}

/// `Unless PLAYER pays COST, ACTIONS.`: the shared `unless` clause child
/// reads the payer and the cost (no action precedes it, so no guarded
/// declaration binds the payer); the guarded sentence is the shared
/// sequence, and the frame guards every action of that sentence.
fn recognize_preposed_unless(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectProgramNode> {
    cursor.enter("program.preposed-unless.clause");
    let clause = recognize_unless_clause(cursor, None, None, false)?;
    cursor.enter("program.preposed-unless.separator");
    cursor.consume(",")?;
    cursor.enter("program.preposed-unless.guarded");
    let end = top_level_sentence_end(cursor)
        .ok_or_else(|| cursor.fail("one guarded sentence after the `unless` clause"))?;
    let mut sequence = cursor.bounded(end, |cursor| {
        recognize_effect_sequence_with_context(cursor, name_links, name_failure, context)
    })?;
    cursor.require(
        !sequence.effects.is_empty() && sequence.unless.is_empty(),
        "one guarded sentence without another `unless` frame",
    )?;
    sequence.unless.push(UnlessFrameNode {
        first_effect: 0,
        end_effect: sequence.effects.len(),
        clause,
    });
    Ok(EffectProgramNode::Sequence(sequence))
}

/// `PLAYER may ...`: an optional program whose actor is the controller or a
/// player the binding environment names.
fn recognize_permission_program(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectProgramNode> {
    if sentence_then_width(cursor) == 1 {
        cursor.enter("program.optional.then");
        cursor.consume("then")?;
    }
    cursor.enter("program.optional.actor");
    // The permission's actor reads the frame the program's parent owns
    // (`that player may ...` names the event's player).
    let enclosing =
        cursor.replace_binding_environment(Some(std::sync::Arc::new(BindingEnvironment {
            frame: FrameBindings::from_context(context),
            ..BindingEnvironment::default()
        })));
    let player = recognize_player_reference(cursor, None);
    cursor.replace_binding_environment(enclosing);
    let player = player?;
    cursor.enter("program.optional.permission");
    cursor.consume("may")?;
    if let Some(lemmas) = coordinated_alternative_verbs(cursor) {
        let program = recognize_alternative_actions(
            cursor,
            player,
            lemmas,
            name_links,
            name_failure,
            context,
        )?;
        if cursor.peek().is_none() {
            return Ok(program);
        }
        return complete_outcome_block(cursor, program, name_links, name_failure, context);
    }
    if connector_follows_first_sentence(cursor) {
        // A payment head is first read as a resolution cost. Only when that
        // typed cost child rejects the sentence (for example a target or an
        // anaphor, which a cost cannot name) is it read as an instruction.
        let cost_failure = if optional_cost_head(cursor) {
            match cursor.attempt(|attempt| {
                recognize_optional_cost_block(attempt, player, name_links, name_failure, context)
            }) {
                Ok(block) => {
                    return complete_outcome_block(
                        cursor,
                        block,
                        name_links,
                        name_failure,
                        context,
                    );
                }
                Err(failure) => Some(failure),
            }
        } else {
            None
        };
        let block = cursor.attempt(|attempt| {
            recognize_optional_action_block(attempt, player, name_links, name_failure, context)
        });
        return match (block, cost_failure) {
            (Ok(block), _) => {
                complete_outcome_block(cursor, block, name_links, name_failure, context)
            }
            // Report the reading that progressed furthest; a tie keeps the cost.
            (Err(failure), Some(cost_failure))
                if failure.token_index > cost_failure.token_index =>
            {
                Err(failure)
            }
            (Err(_), Some(cost_failure)) => Err(cost_failure),
            (Err(failure), None) => Err(failure),
        };
    }
    // The permission owns its own sentence (and directly following reminders)
    // only. Without an outcome connector, a later sentence is an independent
    // program step that runs whether or not the permission was used (CR
    // 608.2c). Another permission inside the same sentence has no enclosing
    // frame here and stays unsupported. A postposed `if CONDITION` gating the
    // sentence's one action is read by the shared sequence; the lowering puts
    // the permission inside that condition. Reminder text and quoted
    // abilities are nested scopes (CR 207.2a, 113.1): their own `may` is not
    // a second frame of this sentence. The body reader still reads and
    // validates every reminder it owns.
    cursor.enter("program.optional.scope");
    let body_end = sentence_end_with_reminders(cursor);
    let mut offset = 0;
    let mut depth = 0_usize;
    let mut quoted = false;
    while cursor.position() + offset < body_end {
        let Some(raw) = cursor.peek_ahead(offset) else {
            break;
        };
        match raw {
            "\"" => quoted = !quoted,
            "(" if !quoted => depth += 1,
            ")" if !quoted => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth == 0 && !quoted && raw.eq_ignore_ascii_case("may") {
            cursor.advance_to(cursor.position() + offset)?;
            return Err(cursor.fail("one optional sentence without a second permission"));
        }
        offset += 1;
    }
    cursor.enter("program.optional.body");
    // CR 118.12: an `unless` payer decides before this permission is used;
    // the optional wrapper cannot order that choice, so the body rejects it.
    let mut permission_context = context.clone();
    permission_context.unless_frames = UnlessFrameSupport::Unsupported;
    let body = cursor.bounded(body_end, |cursor| {
        recognize_effect_sequence_with_context(
            cursor,
            name_links,
            name_failure,
            &permission_context,
        )
    })?;
    cursor.enter("program.optional.complete");
    let optional = EffectProgramNode::Optional {
        player,
        body: Box::new(EffectProgramNode::Sequence(body)),
    };
    if cursor.peek().is_none() {
        return Ok(optional);
    }
    complete_outcome_block(cursor, optional, name_links, name_failure, context)
}

/// `VERB or VERB` at the cursor: two different effect verbs in their base
/// form coordinated by `or`, whose shared object follows the second one.
fn coordinated_alternative_verbs(cursor: &OracleTokenCursor<'_>) -> Option<(VerbLemma, VerbLemma)> {
    let base = |offset: usize| {
        cursor
            .peek_ahead(offset)
            .and_then(lookup_effect_verb)
            .filter(|(_, forms)| forms.contains(&VerbForm::Base))
            .map(|(entry, _)| entry.lemma)
    };
    let first = base(0)?;
    cursor
        .peek_ahead(1)
        .filter(|raw| raw.eq_ignore_ascii_case("or"))?;
    let second = base(2)?;
    (first != second).then_some((first, second))
}

/// `PLAYER may V1 or V2 OBJECT.`: each verb reads the shared object from the
/// second verb's slot (the first one through a view writing its own surface
/// in that slot, every token keeping its index and span). The permission
/// owns its sentence and directly following reminders only.
fn recognize_alternative_actions(
    cursor: &mut OracleTokenCursor<'_>,
    player: LocalPlayerReference,
    (first, second): (VerbLemma, VerbLemma),
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectProgramNode> {
    cursor.enter("program.optional.alternatives");
    let slot = cursor.position() + 2;
    let end = sentence_end_with_reminders(cursor);
    // CR 118.12: no `unless` payer is ordered with this choice.
    let mut permission_context = context.clone();
    permission_context.unless_frames = UnlessFrameSupport::Unsupported;
    let mut options = Vec::new();
    for lemma in [first, second] {
        let tokens = elided_verb_tokens(cursor, slot, lemma);
        let mut view = cursor.with_tokens(&tokens);
        view.advance_to(slot)?;
        let sequence = view.bounded(end, |view| {
            recognize_effect_sequence_with_context(
                view,
                name_links,
                name_failure,
                &permission_context,
            )
        })?;
        options.push((verb_entry(lemma).surface(VerbForm::Base), sequence));
    }
    cursor.advance_to(end)?;
    cursor.enter("program.optional.alternatives.complete");
    Ok(EffectProgramNode::Alternatives { player, options })
}

/// The ActionResult antecedent a permission's actor opens: `you may` is
/// performed by the controller, `that player may` by another player.
fn permission_performer(player: LocalPlayerReference) -> ActionResultSlot {
    ActionResultSlot {
        performer: match player {
            LocalPlayerReference::Controller => OutcomePerformer::Controller,
            LocalPlayerReference::Bound(_)
            | LocalPlayerReference::ControllerOf(_)
            | LocalPlayerReference::ControllerOfQualified(_) => OutcomePerformer::OtherPlayer,
        },
    }
}

/// `COST. OUTCOME, BODY.` after `PLAYER may`.
fn recognize_optional_cost_block(
    cursor: &mut OracleTokenCursor<'_>,
    player: LocalPlayerReference,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectProgramNode> {
    cursor.enter("program.optional-cost.cost");
    let cost = recognize_resolution_cost(cursor)?;
    cursor.enter("cost.outcome.sentence-end");
    cursor.consume(".")?;
    let outcome = recognize_outcome_connector_after(cursor, permission_performer(player))?;
    let body = recognize_outcome_body(
        cursor,
        "program.optional-cost.body",
        outcome,
        name_links,
        name_failure,
        context,
    )?;
    let otherwise = recognize_complementary_outcome(
        cursor,
        permission_performer(player),
        outcome,
        None,
        name_links,
        name_failure,
        context,
    )?;
    Ok(EffectProgramNode::Outcome {
        permission: Some(player),
        producer: OutcomeProducer::Cost(cost),
        outcome,
        body: Box::new(body),
        otherwise,
        scope: ProducerScope::Sentence,
        result: None,
    })
}

/// `If you don't, BODY.` directly after `If you do, BODY.` (or the reverse)
/// of the same optional producer: the other branch of one outcome, read with
/// the same ActionResult antecedent. A reflexive outcome has no complement.
fn recognize_complementary_outcome(
    cursor: &mut OracleTokenCursor<'_>,
    slot: ActionResultSlot,
    outcome: PerformanceOutcome,
    producer: Option<(usize, usize)>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<Option<Box<EffectProgramNode>>> {
    let complement = match outcome {
        PerformanceOutcome::Performed => PerformanceOutcome::Declined,
        PerformanceOutcome::Declined => PerformanceOutcome::Performed,
        PerformanceOutcome::Reflexive => return Ok(None),
    };
    if !producer_connector_at(cursor, 0, producer).is_some_and(|(next, _)| next == complement) {
        return Ok(None);
    }
    cursor.enter("program.outcome.complement");
    let read = read_outcome_connector(cursor, slot, producer)?;
    cursor.require(
        read.outcome == complement && read.result.is_none(),
        "the complementary outcome connector",
    )?;
    let body = recognize_outcome_body(
        cursor,
        "program.outcome.complement.body",
        complement,
        name_links,
        name_failure,
        context,
    )?;
    Ok(Some(Box::new(body)))
}

/// `ACTION. OUTCOME, BODY.` after `PLAYER may`. The action is the shared
/// effect sequence bounded to its own sentence; it never reads the outcome.
fn recognize_optional_action_block(
    cursor: &mut OracleTokenCursor<'_>,
    player: LocalPlayerReference,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectProgramNode> {
    cursor.enter("program.optional-action.action");
    let action_start = cursor.position();
    let action_end = top_level_sentence_end(cursor)
        .ok_or_else(|| cursor.fail("one complete optional action sentence"))?;
    let action = cursor.bounded(action_end, |cursor| {
        recognize_effect_sequence_with_context(cursor, name_links, name_failure, context)
    })?;
    cursor.enter("program.optional-action.outcome");
    let producer = Some((action_start, action_end));
    let reading = read_outcome_connector(cursor, permission_performer(player), producer)?;
    let outcome = reading.outcome;
    let body = recognize_outcome_body(
        cursor,
        "program.optional-action.body",
        outcome,
        name_links,
        name_failure,
        context,
    )?;
    let otherwise = recognize_complementary_outcome(
        cursor,
        permission_performer(player),
        outcome,
        producer,
        name_links,
        name_failure,
        context,
    )?;
    Ok(EffectProgramNode::Outcome {
        permission: Some(player),
        producer: OutcomeProducer::Action(Box::new(EffectProgramNode::Sequence(action))),
        outcome,
        body: Box::new(body),
        otherwise,
        scope: ProducerScope::Sentence,
        result: reading.result,
    })
}

/// The outcomes a sentence without a leading permission can produce: `If you
/// do,` gates the body on its measured performance, `When you do,` creates a
/// reflexive ability (CR 608.2c, 603.12). `If you don't` names a declined
/// permission, so it follows only a sentence that writes its own `may`
/// (`If it's a land card, you may put it onto the battlefield.`); after any
/// other mandatory instruction it stays an explicit failure.
fn mandatory_outcome(outcome: PerformanceOutcome, permits: bool) -> bool {
    match outcome {
        PerformanceOutcome::Performed | PerformanceOutcome::Reflexive => true,
        PerformanceOutcome::Declined => permits,
    }
}

/// Lexical: a top-level `may` between the absolute positions `start` and
/// `end`, outside reminders and quoted abilities (their own scopes).
fn sentence_permits(cursor: &OracleTokenCursor<'_>, start: usize, end: usize) -> bool {
    let mut depth = 0usize;
    let mut quoted = false;
    (start..end).any(|position| {
        let Some(raw) = cursor.peek_ahead(position.saturating_sub(cursor.position())) else {
            return false;
        };
        match raw {
            "\"" => quoted = !quoted,
            "(" if !quoted => depth += 1,
            ")" if !quoted => depth = depth.saturating_sub(1),
            _ => {}
        }
        depth == 0 && !quoted && raw.eq_ignore_ascii_case("may")
    })
}

/// True when the current top-level sentence is followed by an outcome
/// connector a mandatory instruction can produce; the connector is read by
/// the shared ActionResult resolver, whoever performs the instruction.
fn mandatory_outcome_follows_first_sentence(cursor: &OracleTokenCursor<'_>) -> bool {
    let Some(end) = top_level_sentence_end(cursor) else {
        return false;
    };
    let permits = sentence_permits(cursor, cursor.position(), end);
    let producer = Some((cursor.position(), end));
    match producer_connector_at(cursor, end - cursor.position(), producer) {
        Some((outcome, _)) => {
            mandatory_outcome(outcome, permits)
                || restated_connector_at(cursor, end - cursor.position(), producer)
        }
        None => false,
    }
}

/// The absolute start of a later top-level sentence followed by a connector
/// that `accepts` (given whether that sentence writes its own permission):
/// the program reads the sentences before it as an ordinary step and the
/// outcome block from there.
fn later_outcome_block(
    cursor: &OracleTokenCursor<'_>,
    accepts: impl Fn(PerformanceOutcome, bool) -> bool,
) -> Option<usize> {
    let mut depth = 0usize;
    let mut offset = 0;
    let mut sentence_start = cursor.position();
    while let Some(raw) = cursor.peek_ahead(offset) {
        match raw {
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            "." if depth == 0 => {
                let sentence_end = cursor.position() + offset;
                let producer = Some((sentence_start, sentence_end));
                // A restated `If you don't VERB ...` names the instruction
                // itself, whatever permission the sentence writes.
                if producer_connector_at(cursor, offset + 1, producer).is_some_and(
                    |(outcome, _)| {
                        accepts(
                            outcome,
                            sentence_permits(cursor, sentence_start, sentence_end)
                                || restated_connector_at(cursor, offset + 1, producer),
                        )
                    },
                ) {
                    return (sentence_start > cursor.position()).then_some(sentence_start);
                }
                sentence_start = cursor.position() + offset + 1;
            }
            _ => {}
        }
        offset += 1;
    }
    None
}

/// A later mandatory sentence followed by its outcome connector.
fn later_mandatory_outcome_block(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    later_outcome_block(cursor, mandatory_outcome)
}

/// The absolute start of a later top-level sentence followed by a reflexive
/// connector (CR 603.12).
pub(in crate::oracle::canonical) fn later_reflexive_action_block(
    cursor: &OracleTokenCursor<'_>,
) -> Option<usize> {
    later_outcome_block(cursor, |outcome, _| {
        outcome == PerformanceOutcome::Reflexive
    })
}

/// True when any top-level sentence of the body is followed by a reflexive
/// connector: the body is an effect program, not one sequence.
pub(in crate::oracle::canonical) fn reflexive_action_block_follows(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    top_level_sentence_end(cursor).is_some_and(|end| {
        producer_connector_at(
            cursor,
            end - cursor.position(),
            Some((cursor.position(), end)),
        )
        .is_some_and(|(outcome, _)| outcome == PerformanceOutcome::Reflexive)
    }) || later_reflexive_action_block(cursor).is_some()
}

/// `ACTION. (If you do | When you do), BODY.` without a permission: the
/// action is the shared effect sequence bounded to its own sentence, and the
/// connector observes its measured performance.
fn recognize_mandatory_outcome_block(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectProgramNode> {
    let start = cursor.position();
    recognize_instruction_outcome_block(
        cursor,
        start,
        ProducerScope::Sentence,
        name_links,
        name_failure,
        context,
    )
}

/// `[LEADING.] PRODUCER. OUTCOME, BODY. [COMPLEMENT, BODY.]` without a
/// leading permission. The producer is the shared effect sequence from the
/// cursor through the sentence that starts at `final_sentence` (absolute):
/// with `ProducerScope::FinalInstruction` the earlier sentences are read in
/// the same sequence, so the final sentence reads their antecedents (`Look at
/// the top card of your library. If it's a land card, you may put it onto the
/// battlefield.`). A permission written inside the final sentence (`If COND,
/// you may ACTION.`) admits `If you don't` and its complement.
fn recognize_instruction_outcome_block(
    cursor: &mut OracleTokenCursor<'_>,
    final_sentence: usize,
    scope: ProducerScope,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectProgramNode> {
    cursor.enter("program.reflexive-action.action");
    let mut sentence = cursor.clone();
    sentence.advance_to(final_sentence)?;
    let action_end = top_level_sentence_end(&sentence)
        .ok_or_else(|| cursor.fail("one complete instruction sentence"))?;
    let permits = sentence_permits(cursor, final_sentence, action_end);
    let action = cursor.bounded(action_end, |cursor| {
        recognize_effect_sequence_with_context(cursor, name_links, name_failure, context)
    })?;
    cursor.enter("program.reflexive-action.outcome");
    let slot = ActionResultSlot {
        performer: OutcomePerformer::Instruction,
    };
    let producer = Some((final_sentence, action_end));
    let reading = read_outcome_connector(cursor, slot, producer)?;
    let outcome = reading.outcome;
    cursor.require(
        mandatory_outcome(outcome, permits || reading.restated),
        "an `If you do,` or reflexive `When you do,` outcome after a mandatory instruction, or `If you don't,` after the sentence's own permission",
    )?;
    let body = recognize_outcome_body(
        cursor,
        "program.reflexive-action.body",
        outcome,
        name_links,
        name_failure,
        context,
    )?;
    // Only a permission has a declined branch to complement.
    let otherwise = if permits {
        recognize_complementary_outcome(
            cursor,
            slot,
            outcome,
            producer,
            name_links,
            name_failure,
            context,
        )?
    } else {
        None
    };
    Ok(EffectProgramNode::Outcome {
        permission: None,
        producer: OutcomeProducer::Action(Box::new(EffectProgramNode::Sequence(action))),
        outcome,
        body: Box::new(body),
        otherwise,
        scope,
        result: reading.result,
    })
}

/// An `If you do` / `If you don't` outcome governs its own sentence (and its
/// reminder) only (CR 608.2c). A reflexive `When you do` creates a triggered
/// ability whose text is the rest of the enclosing ability (CR 603.12): its
/// later sentences read the reflexive ability's own targets (`When you do,
/// put a +1/+1 counter on target creature. That creature gains trample`),
/// so they are one sequence with one binding environment. The extended body
/// stops before a trigger-limit sentence, which belongs to the enclosing
/// ability; when the rest is not one supported sequence, the body keeps its
/// own sentence and the later sentences stay independent steps.
fn recognize_outcome_body(
    cursor: &mut OracleTokenCursor<'_>,
    state: &'static str,
    outcome: PerformanceOutcome,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectProgramNode> {
    cursor.enter(state);
    // `If you do, put a +1/+1 counter on it. If it's a Unicorn, put two
    // +1/+1 counters on it instead.`: the replacing sentence amends the
    // outcome's own sentence and belongs to the same outcome body.
    if let Some(sentence) =
        replacing_sentence(cursor).filter(|sentence| sentence.replaced_start == cursor.position())
    {
        return recognize_replacing_sequence(cursor, sentence, name_links, name_failure, context)
            .map(EffectProgramNode::Sequence);
    }
    let body_end = sentence_end_with_reminders(cursor);
    if outcome == PerformanceOutcome::Reflexive {
        let ability_end = reflexive_ability_end(cursor, context);
        if ability_end > body_end
            && let Ok(body) = cursor.attempt(|attempt| {
                attempt.bounded(ability_end, |cursor| {
                    recognize_effect_sequence_with_context(
                        cursor,
                        name_links,
                        name_failure,
                        context,
                    )
                })
            })
        {
            return Ok(EffectProgramNode::Sequence(body));
        }
    }
    let body = cursor.bounded(body_end, |cursor| {
        recognize_effect_sequence_with_context(cursor, name_links, name_failure, context)
    })?;
    Ok(EffectProgramNode::Sequence(body))
}

/// The end (exclusive, absolute) of the text a reflexive ability owns from
/// the cursor: the end of the enclosing program, or the start of the
/// enclosing trigger's limit sentence (`This ability triggers only once
/// each turn.`). Lexical scan of top-level sentence starts only.
fn reflexive_ability_end(
    cursor: &OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> usize {
    let end = cursor.position() + cursor.token_slice(cursor.position(), usize::MAX).len();
    if context.ability_restriction_owner != AbilityRestrictionOwner::Triggered {
        return end;
    }
    let mut depth = 0usize;
    let mut offset = 0;
    while let Some(raw) = cursor.peek_ahead(offset) {
        match raw {
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            "." if depth == 0 => {
                let start = cursor.position() + offset + 1;
                let mut probe = cursor.clone();
                if probe.advance_to(start).is_ok() && trigger_limit_starts(&probe) {
                    return start;
                }
            }
            _ => {}
        }
        offset += 1;
    }
    end
}

/// An outcome block ends the program, yields to the enclosing trigger's
/// limit sentence, or is followed by independent program steps.
fn complete_outcome_block(
    cursor: &mut OracleTokenCursor<'_>,
    block: EffectProgramNode,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectProgramNode> {
    if cursor.peek().is_none()
        || (context.ability_restriction_owner == AbilityRestrictionOwner::Triggered
            && trigger_limit_starts(cursor))
        || (context.ability_restriction_owner == AbilityRestrictionOwner::Activated
            && activation_restriction_starts(cursor))
    {
        cursor.enter("program.optional-outcome.complete");
        return Ok(block);
    }
    if super::copy_new_targets_amendment_starts(cursor) {
        // CR 707.10c: the permission amends the copy instruction the block
        // ends with, whatever wrapper gates that instruction.
        let mut block = block;
        super::recognize_copy_new_targets_amendment(
            cursor,
            super::last_program_copy_instruction_mut(&mut block),
        )?;
        cursor.enter("program.copy-new-targets.terminator");
        cursor.consume(".")?;
        return complete_outcome_block(cursor, block, name_links, name_failure, context);
    }
    if super::type_retention_amendment_starts(cursor) {
        // CR 205.1b: the sentence amends the type change the block ends
        // with, whatever wrapper gates that change.
        let mut block = block;
        super::recognize_type_retention_amendment(
            cursor,
            super::last_program_action_mut(&mut block),
        )?;
        cursor.enter("program.type-retention.terminator");
        cursor.consume(".")?;
        return complete_outcome_block(cursor, block, name_links, name_failure, context);
    }
    cursor.enter("program.steps.following");
    // A sentence-initial `Then` only restates the written order of the
    // independent steps (CR 608.2c); the following step owns its sentence.
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("then"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw != "." && raw != ",")
    {
        cursor.enter("program.steps.then");
        cursor.consume("then")?;
    }
    let mut steps = vec![block];
    push_program_steps(
        &mut steps,
        recognize_effect_program_with_context(cursor, name_links, name_failure, context)?,
    );
    Ok(EffectProgramNode::Steps(steps))
}

/// The outcome connector at `offset` after a producer written between the
/// absolute positions of `producer`: a written connector (`If you do,`,
/// `When you do,`) or a declined one that restates the producer's
/// instruction (`If you don't put it onto the battlefield,`).
fn producer_connector_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
    producer: Option<(usize, usize)>,
) -> Option<(PerformanceOutcome, usize)> {
    outcome_connector_at(cursor, offset)
        .or_else(|| {
            reflexive_result_connector_at(cursor, offset)
                .map(|width| (PerformanceOutcome::Reflexive, width))
        })
        .or_else(|| {
            let (start, end) = producer?;
            restated_declined_connector_at(cursor, offset, start, end)
                .map(|width| (PerformanceOutcome::Declined, width))
        })
}

/// True when the connector at `offset` restates the producer's instruction.
fn restated_connector_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
    producer: Option<(usize, usize)>,
) -> bool {
    outcome_connector_at(cursor, offset).is_none()
        && reflexive_result_connector_at(cursor, offset).is_none()
        && producer.is_some_and(|(start, end)| {
            restated_declined_connector_at(cursor, offset, start, end).is_some()
        })
}

/// True when the first sentence after the cursor is followed by an outcome
/// connector, written or restated. Lexical lookahead only.
fn connector_follows_first_sentence(cursor: &OracleTokenCursor<'_>) -> bool {
    if outcome_connector_follows_first_sentence(cursor) {
        return true;
    }
    let Some(end) = top_level_sentence_end(cursor) else {
        return false;
    };
    producer_connector_at(
        cursor,
        end - cursor.position(),
        Some((cursor.position(), end)),
    )
    .is_some()
}

/// Reads the outcome connector of a producer: a restated declined connector
/// (whose performer is the controller, `you`), else a written connector
/// resolved against `slot`. Returns the outcome and whether it was restated.
fn read_outcome_connector(
    cursor: &mut OracleTokenCursor<'_>,
    slot: ActionResultSlot,
    producer: Option<(usize, usize)>,
) -> MachineResult<ConnectorReading> {
    if outcome_connector_at(cursor, 0).is_none()
        && reflexive_result_connector_at(cursor, 0).is_some()
    {
        cursor.enter("program.outcome.reflexive-result");
        cursor.require(
            slot.performer != OutcomePerformer::OtherPlayer,
            "a reflexive `When you VERB ... this way` after an instruction the controller performs",
        )?;
        let result = recognize_reflexive_result_connector(cursor)?;
        return Ok(ConnectorReading {
            outcome: PerformanceOutcome::Reflexive,
            restated: false,
            result: Some(result),
        });
    }
    if let Some((start, end)) = producer
        && restated_connector_at(cursor, 0, producer)
        && let Some(width) = restated_declined_connector_at(cursor, 0, start, end)
    {
        cursor.enter("program.outcome.restated-connector");
        cursor.require(
            slot.performer != OutcomePerformer::OtherPlayer,
            "a restated `If you don't` after an instruction the controller performs",
        )?;
        cursor.advance_to(cursor.position() + width)?;
        return Ok(ConnectorReading {
            outcome: PerformanceOutcome::Declined,
            restated: true,
            result: None,
        });
    }
    Ok(ConnectorReading {
        outcome: recognize_outcome_connector_after(cursor, slot)?,
        restated: false,
        result: None,
    })
}

/// One read outcome connector.
struct ConnectorReading {
    outcome: PerformanceOutcome,
    /// `If you don't VERB ...`: the connector restated the instruction.
    restated: bool,
    /// `When you VERB NOUN this way`: the reflexive event's restated objects.
    result: Option<ReflexiveResultNode>,
}

/// End (exclusive, absolute) of the current top-level sentence, including
/// its period; `None` when no top-level period follows.
fn top_level_sentence_end(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let mut depth = 0usize;
    let mut offset = 0;
    while let Some(raw) = cursor.peek_ahead(offset) {
        match raw {
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            "." if depth == 0 => return Some(cursor.position() + offset + 1),
            _ => {}
        }
        offset += 1;
    }
    None
}

/// `that player may`, `defending player may`, `its controller may`: a
/// permission whose actor the binding environment names (lexical only).
fn other_player_permission_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let fixed_head = ["that", "defending"].iter().any(|word| {
        cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    }) && cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("player"));
    let head = if fixed_head {
        Some(2)
    } else {
        super::object_related_player_head_length(cursor)
    };
    head.is_some_and(|head| {
        cursor
            .peek_ahead(head)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("may"))
    })
}

/// Reads another player's permission with the optional program reader; the
/// sentence reading's failure is kept when it progressed further.
fn recognize_other_player_permission(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
    sequence_failure: super::super::MachineFailure,
) -> MachineResult<EffectProgramNode> {
    match cursor
        .attempt(|attempt| recognize_permission_program(attempt, name_links, name_failure, context))
    {
        Ok(program) => Ok(program),
        Err(failure) if failure.token_index > sequence_failure.token_index => Err(failure),
        Err(_) => Err(sequence_failure),
    }
}

/// True when the body at the cursor needs the program reader rather than
/// one shared sequence: a permission, an outcome connector, a reflexive
/// ability, a replacing `instead` sentence, a preposed `Unless` or a
/// program-initial `Then` (lexical lookahead only; the program's children
/// prove it).
pub(in crate::oracle::canonical) fn effect_program_operator_follows(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    optional_program_starts(cursor)
        || sentence_then_width(cursor) == 1
        || cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("unless"))
        || mandatory_outcome_follows_first_sentence(cursor)
        || later_optional_outcome_block(cursor).is_some()
        || later_mandatory_outcome_block(cursor).is_some()
        || reflexive_action_block_follows(cursor)
        || replacing_sentence(cursor).is_some()
        || super::postposed_replacing_sentence(cursor).is_some()
}

fn optional_program_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let then = sentence_then_width(cursor);
    cursor
        .peek_ahead(then)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
        && cursor
            .peek_ahead(then + 1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("may"))
}

/// `Then` opening a sentence orders it after the previous one, which the
/// program's step order already states (CR 608.2c): one token, or none.
fn sentence_then_width(cursor: &OracleTokenCursor<'_>) -> usize {
    usize::from(
        cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("then")),
    )
}

fn push_program_steps(steps: &mut Vec<EffectProgramNode>, program: EffectProgramNode) {
    match program {
        EffectProgramNode::Steps(inner) => steps.extend(inner),
        other => steps.push(other),
    }
}

/// Absolute index of the first later top-level sentence that opens
/// `you may COST-OR-ACTION. (If you do | If you don't | When you do), ...`.
/// Lexical lookahead only; the block's children prove it afterwards.
pub(in crate::oracle::canonical) fn later_optional_outcome_block(
    cursor: &OracleTokenCursor<'_>,
) -> Option<usize> {
    let mut depth = 0usize;
    let mut offset = 0;
    while let Some(raw) = cursor.peek_ahead(offset) {
        match raw {
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            "." if depth == 0 => {
                let start = cursor.position() + offset + 1;
                let mut probe = cursor.clone();
                if probe.advance_to(start).is_ok() && optional_program_starts(&probe) {
                    let mut head = probe.clone();
                    if head
                        .advance_to(start + sentence_then_width(&probe) + 2)
                        .is_ok()
                        && connector_follows_first_sentence(&head)
                    {
                        return Some(start);
                    }
                }
            }
            _ => {}
        }
        offset += 1;
    }
    None
}

/// End (exclusive, absolute) of the current top-level sentence, extended over
/// parenthesized reminders that directly follow it (CR 207.2).
fn sentence_end_with_reminders(cursor: &OracleTokenCursor<'_>) -> usize {
    let mut depth = 0usize;
    let mut offset = 0;
    let mut end: Option<usize> = None;
    while let Some(raw) = cursor.peek_ahead(offset) {
        if raw == "(" {
            depth += 1;
        } else if raw == ")" {
            depth = depth.saturating_sub(1);
            if depth == 0 && end.is_some() {
                end = Some(offset + 1);
            }
        } else if depth == 0 {
            if end.is_some() {
                break;
            }
            if raw == "." {
                end = Some(offset + 1);
            }
        }
        offset += 1;
    }
    cursor.position() + end.unwrap_or(offset)
}

#[cfg(test)]
#[path = "effect_program_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "effect_program_reminder_scope_tests.rs"]
mod reminder_scope_tests;
