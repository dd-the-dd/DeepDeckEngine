use super::super::{MachineFailure, OracleTokenCursor};
use super::counter_named_token::recognize_named_token_identity;
use super::*;
use crate::oracle::canonical::automata::VerbLemma;
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ExplicitTokenCreationNode {
    pub player: LocalPlayerReference,
    pub body: TokenCreationBodyNode,
    /// The further conjuncts of a coordinated description list (`a 1/1 ...
    /// token, a 2/2 ... token, and a 3/3 ... token`): each conjunct is an
    /// independent written description created by the same player.
    pub additional: Vec<TokenCreationBodyNode>,
}

/// What a created token is: a written description (CR 111.4), a copy of
/// an object with copiable-value exceptions (CR 111.8, 707.9), or a
/// predefined catalog identity (CR 111.10) whose quantity or attachment the
/// catalog-only reader cannot carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TokenSpecNode {
    Described(ExplicitTokenTemplateNode),
    CopyOf(TokenCopyNode),
    Predefined(String),
}

/// `token(s) that's|that are a copy|copies of OBJECT[, except EXCEPTIONS]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TokenCopyNode {
    /// The copied object, read by the shared contextual permanent operand
    /// grammar (a target, the source, a bound reference).
    pub object: PermanentOperandNode,
    pub exceptions: TokenCopyExceptionsNode,
}

/// The copiable-value exceptions of a copy (CR 707.9b). The characteristics
/// are the ones of the shared characteristic-change frame; `isn't
/// legendary` removes a supertype, which no `becomes` complement states.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TokenCopyExceptionsNode {
    pub changes: Option<BecomeTypesNode<()>>,
    pub not_legendary: bool,
}

/// What a `create` verb creates, independent of who performs it: the
/// controller's imperative and a player subject's `creates` share it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TokenCreationBodyNode {
    pub quantity: NumericExpression,
    pub spec: TokenSpecNode,
    pub keywords: Vec<GrantedKeyword>,
    /// Complete quoted abilities of the created tokens (CR 113.1).
    pub abilities: Vec<QuotedAbilityNode>,
    pub tapped: bool,
    /// The tokens enter attacking (CR 508.4): `tapped and attacking`,
    /// `that's attacking`.
    pub attacking: bool,
    /// The token's name when the effect gives one (CR 111.4): `NAME, a ...`
    /// or `... token named NAME`.
    pub name: Option<String>,
    /// The `legendary` supertype of the created token (CR 205.4d).
    pub legendary: bool,
    /// `attached to OBJECT`: an Aura token enters attached to that object
    /// (CR 111.3, 303.4f), read by the shared contextual permanent operand
    /// grammar (a target, the source, a bound reference).
    pub attached_to: Option<PermanentOperandNode>,
}

impl TokenCreationBodyNode {
    /// The written description, when the tokens are not copies.
    pub(in crate::oracle::canonical) fn described(&self) -> Option<&ExplicitTokenTemplateNode> {
        match &self.spec {
            TokenSpecNode::Described(template) => Some(template),
            TokenSpecNode::CopyOf(_) | TokenSpecNode::Predefined(_) => None,
        }
    }

    /// The copy specification, when the tokens are copies.
    pub(in crate::oracle::canonical) fn copy_of(&self) -> Option<&TokenCopyNode> {
        match &self.spec {
            TokenSpecNode::CopyOf(copy) => Some(copy),
            TokenSpecNode::Described(_) | TokenSpecNode::Predefined(_) => None,
        }
    }

    /// The catalog identity, when the tokens are predefined (CR 111.10).
    pub(in crate::oracle::canonical) fn predefined(&self) -> Option<&str> {
        match &self.spec {
            TokenSpecNode::Predefined(name) => Some(name),
            TokenSpecNode::Described(_) | TokenSpecNode::CopyOf(_) => None,
        }
    }

    /// The object operand the creation reads: the copied object or the
    /// object the tokens enter attached to. A program declares its target.
    pub(in crate::oracle::canonical) fn object_operand(&self) -> Option<&PermanentOperandNode> {
        self.copy_of()
            .map(|copy| &copy.object)
            .or(self.attached_to.as_ref())
    }
}

/// The context a copied object is read in: the shared contextual permanent
/// operand grammar needs the name links and the effect's binding context.
#[derive(Clone, Copy)]
pub(in crate::oracle::canonical) struct TokenObjectContext<'a> {
    pub name_links: &'a [ResolvedNameLink],
    pub name_failure: Option<&'a NameResolutionFailure>,
    pub context: &'a EffectRecognitionContext,
}

/// `NAME, a|an`: a proper name given by the effect before the article of a
/// single token description (CR 111.4). The name is read up to its comma.
fn recognize_token_proper_name(
    cursor: &mut OracleTokenCursor<'_>,
) -> super::super::MachineResult<Option<String>> {
    cursor.enter("token.creation.proper-name");
    let tokens = cursor.token_slice(cursor.position(), usize::MAX);
    let Some(comma) = tokens.iter().position(|token| token.raw == ",") else {
        return Ok(None);
    };
    let words = &tokens[..comma];
    let is_name = !words.is_empty()
        && words[0]
            .raw
            .starts_with(|character: char| character.is_ascii_uppercase())
        && words.iter().all(|token| {
            token.raw.chars().any(char::is_alphabetic)
                && token.raw.chars().all(|character| {
                    character.is_alphanumeric() || "'\u{2019}-".contains(character)
                })
        })
        && tokens.get(comma + 1).is_some_and(|token| {
            token.raw.eq_ignore_ascii_case("a") || token.raw.eq_ignore_ascii_case("an")
        });
    if !is_name {
        return Ok(None);
    }
    let name = words
        .iter()
        .map(|token| token.raw.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    cursor.advance_to(cursor.position() + comma + 2)?;
    Ok(Some(name))
}

#[cfg(test)]
#[path = "token_creation_action_tests.rs"]
mod tests;

pub(in crate::oracle::canonical) enum TokenCreationAttempt {
    NotClaimed,
    Recognized(LocalEffectNode),
    FailedCommitted(MachineFailure),
}

/// A trial owns transient actor/quantity/template slots. Named and explicit
/// definitions are different typed outcomes; only a completed child commits.
pub(in crate::oracle::canonical) fn try_recognize_token_creation_action(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> TokenCreationAttempt {
    try_recognize_token_creation_action_in_context(cursor, antecedent, None)
}

/// The same action with the context a copied object is read in. Without it,
/// `a token that's a copy of OBJECT` fails explicitly at its object.
pub(in crate::oracle::canonical) fn try_recognize_token_creation_action_in_context(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
    objects: Option<TokenObjectContext<'_>>,
) -> TokenCreationAttempt {
    let mut trial = cursor.clone();
    let player = if crate::oracle::canonical::automata::lemma_at(
        &trial,
        VerbLemma::Create,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    ) {
        LocalPlayerReference::Controller
    } else {
        match recognize_player_reference(&mut trial, antecedent) {
            // A player the binding environment names (`its controller` of
            // an event object) is a performer: the clause layer's performer
            // slot runs the ordinary creation as that player's instruction.
            Ok(LocalPlayerReference::Bound(_)) => return TokenCreationAttempt::NotClaimed,
            Ok(player) => player,
            Err(_) => return TokenCreationAttempt::NotClaimed,
        }
    };
    let verb = match player {
        LocalPlayerReference::Controller => "create",
        _ => "creates",
    };
    if !trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case(verb))
    {
        return TokenCreationAttempt::NotClaimed;
    }
    trial.enter("token.creation.action");
    if let Err(f) = trial.consume(verb) {
        return TokenCreationAttempt::FailedCommitted(f);
    }
    let head = match recognize_token_body_head(
        &mut trial,
        antecedent,
        objects,
        CatalogReader::Available,
    ) {
        Ok(Some(head)) => head,
        Ok(None) => {
            // Reuse the complete catalog reader and all of its existing
            // quantity/list/reminder rules. This is an explicit alternative,
            // not recovery after an explicit descriptor has committed.
            return match recognize_create_named_tokens_effect(cursor, antecedent) {
                Ok(node) => {
                    TokenCreationAttempt::Recognized(LocalEffectNode::CreateNamedTokens(node))
                }
                Err(f) => TokenCreationAttempt::FailedCommitted(f),
            };
        }
        Err(f) => return TokenCreationAttempt::FailedCommitted(f),
    };
    let body = match recognize_token_body_tail(&mut trial, head, objects) {
        Ok(body) => body,
        Err(f) => return TokenCreationAttempt::FailedCommitted(f),
    };
    if player != LocalPlayerReference::Controller {
        // This old typed body is the input of parents that capture the
        // referenced controller BEFORE destroying/countering the object.
        // Pass the parsed node and ending cursor, never recompute its player
        // through the ordinary controller-only lowerer after that departure.
        // A description with modifiers the old node cannot carry keeps its
        // referenced player in the shared explicit node instead.
        if let Some(effect) = referenced_controller_creature_tokens(&body, &player) {
            *cursor = trial;
            return TokenCreationAttempt::Recognized(LocalEffectNode::CreateTokens(effect));
        }
        trial.enter("token.creation.referenced-controller-body");
        if body.copy_of().is_some() {
            return TokenCreationAttempt::FailedCommitted(
                trial.fail("a written description for the referenced controller's tokens"),
            );
        }
        *cursor = trial;
        return TokenCreationAttempt::Recognized(LocalEffectNode::CreateExplicitTokens(
            ExplicitTokenCreationNode {
                player,
                body,
                additional: Vec::new(),
            },
        ));
    }
    let additional = match recognize_token_conjuncts(&mut trial, &body, antecedent) {
        Ok(additional) => additional,
        Err(f) => return TokenCreationAttempt::FailedCommitted(f),
    };
    *cursor = trial;
    TokenCreationAttempt::Recognized(LocalEffectNode::CreateExplicitTokens(
        ExplicitTokenCreationNode {
            player,
            body,
            additional,
        },
    ))
}

/// The historical controller-snapshot node: an untapped, unnamed, literal
/// Creature-only description without abilities and a positive literal or
/// declared-X quantity. Any other description is not representable there.
fn referenced_controller_creature_tokens(
    body: &TokenCreationBodyNode,
    player: &LocalPlayerReference,
) -> Option<CreateTokensEffectNode> {
    if body.tapped
        || body.attacking
        || body.legendary
        || body.name.is_some()
        || !body.keywords.is_empty()
        || !body.abilities.is_empty()
    {
        return None;
    }
    let template = body
        .described()
        .filter(|template| template.literal_statistics())?;
    if template.types.as_slice() != [ObjectCardType::Creature] {
        return None;
    }
    let quantity = match body.quantity {
        NumericExpression::Atom(NumericAtom::Integer(n)) if n > 0 && i32::try_from(n).is_ok() => {
            NumericAtom::Integer(n)
        }
        NumericExpression::Atom(NumericAtom::VariableX) => NumericAtom::VariableX,
        _ => return None,
    };
    Some(CreateTokensEffectNode {
        player: *player,
        quantity,
        token: template.creature.clone(),
    })
}

/// Whether another written description of a coordinated list starts here:
/// an article or quantity followed by a description's introducer (stats,
/// color, entry state or supertype). A following verb or catalog identity
/// is not a description conjunct and stays with the enclosing sequence.
fn token_conjunct_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let quantity = cursor.peek_token().is_some_and(|token| {
        token.raw.eq_ignore_ascii_case("a")
            || token.raw.eq_ignore_ascii_case("an")
            || (token.token_type == "quantity" && !token.raw.eq_ignore_ascii_case("x"))
    });
    if !quantity {
        return false;
    }
    let mut next = cursor.clone();
    if next.advance_to(next.position() + 1).is_err() {
        return false;
    }
    next.peek_token()
        .is_some_and(|token| token.token_type == "counterValue")
        || numeric_power_toughness_starts(&next)
        || next.peek().is_some_and(|raw| {
            ["tapped", "attacking", "legendary", "colorless"]
                .iter()
                .any(|word| raw.eq_ignore_ascii_case(word))
                || lookup_card_color_word(raw).is_some()
        })
}

/// `, a 2/2 ... token, and a 3/3 ... token`: further written descriptions
/// coordinated with the first one. Once a conjunct starts, it is committed:
/// an unsupported conjunct fails the whole list instead of truncating it.
fn recognize_token_conjuncts(
    trial: &mut OracleTokenCursor<'_>,
    first: &TokenCreationBodyNode,
    antecedent: Option<TargetBinding>,
) -> super::super::MachineResult<Vec<TokenCreationBodyNode>> {
    let mut additional = Vec::new();
    let mut closed = false;
    if first.copy_of().is_some() {
        return Ok(additional);
    }
    loop {
        let mut next = trial.clone();
        let comma = next.peek() == Some(",");
        if comma {
            next.consume(",")?;
        }
        let and = next
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"));
        if and {
            next.consume("and")?;
        }
        if !(comma || and) || !token_conjunct_starts(&next) {
            break;
        }
        next.enter("token.coordination.description");
        let head =
            recognize_token_body_head(&mut next, antecedent, None, CatalogReader::Available)?
                .ok_or_else(|| next.fail("a written token description in the coordinated list"))?;
        next.require(
            head.body.described().is_some(),
            "a written description, not a copy or a catalog identity, in a token list",
        )?;
        let body = recognize_token_body_tail(&mut next, head, None)?;
        additional.push(body);
        *trial = next;
        if and {
            closed = true;
            break;
        }
    }
    if !additional.is_empty() {
        trial.enter("token.coordination.closing-connector");
        trial.require(closed, "an explicit closing `and` for this token list")?;
    }
    Ok(additional)
}

/// The token description after a performer's `create(s)` verb (CR 111.1):
/// quantity, characteristics and granted abilities, or a catalog identity
/// (`a Treasure token`, CR 111.10) read as the same shared body.
pub(in crate::oracle::canonical) fn recognize_performed_token_creation_body(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> super::super::MachineResult<TokenCreationBodyNode> {
    let Some(head) = recognize_token_body_head(cursor, antecedent, None, CatalogReader::Absent)?
    else {
        cursor.enter("token.creation.performed.description");
        return Err(cursor.fail(
            "a written token description or a catalog identity after a player subject's verb",
        ));
    };
    recognize_token_body_tail(cursor, head, None)
}

/// Whether the token noun that opens a copy specification starts here.
fn token_copy_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    (word(0, "token") && word(1, "that's") && word(2, "a") && word(3, "copy"))
        || (word(0, "tokens") && word(1, "that") && word(2, "are") && word(3, "copies"))
}

/// The quantity of a description: an article, `a number of ... equal to
/// EXPR` (read after the noun) or the shared numeric expression.
enum TokenQuantity {
    Stated(NumericExpression),
    /// `a number of SPEC tokens equal to EXPR`: the amount follows the noun.
    EqualToAfterNoun,
}

/// The description up to its noun, and whether its `a number of ... equal
/// to EXPR` amount follows the modifiers (`... tokens with flying equal to
/// EXPR`): the tail then reads it.
struct TokenBodyHead {
    body: TokenCreationBodyNode,
    amount_after_modifiers: bool,
}

impl From<TokenCreationBodyNode> for TokenBodyHead {
    fn from(body: TokenCreationBodyNode) -> Self {
        Self {
            body,
            amount_after_modifiers: false,
        }
    }
}

/// The description up to its `token(s)` noun; `Ok(None)` when no explicit
/// descriptor starts (the catalog reader's input).
fn recognize_token_body_head(
    trial: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
    objects: Option<TokenObjectContext<'_>>,
    catalog_reader: CatalogReader,
) -> super::super::MachineResult<Option<TokenBodyHead>> {
    let name = recognize_token_proper_name(trial)?;
    trial.enter("token.creation.quantity");
    let quantity = if name.is_some() {
        TokenQuantity::Stated(NumericExpression::from(1))
    } else if trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("a"))
        && trial
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("number"))
        && trial
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
    {
        trial.consume_phrase(&["a", "number", "of"])?;
        TokenQuantity::EqualToAfterNoun
    } else if trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
    {
        trial
            .advance_to(trial.position() + 1)
            .expect("one present article");
        TokenQuantity::Stated(NumericExpression::from(1))
    } else {
        TokenQuantity::Stated(recognize_numeric_expression(trial, antecedent)?)
    };
    let entry = recognize_token_entry_prefix(trial)?;
    if token_copy_starts(trial) {
        let TokenQuantity::Stated(quantity) = quantity else {
            return Err(trial.fail("a stated quantity of token copies"));
        };
        if name.is_some() {
            return Err(trial.fail("a token copy without a given name"));
        }
        let copy = recognize_token_copy(trial, &quantity, objects)?;
        return Ok(Some(TokenBodyHead::from(TokenCreationBodyNode {
            quantity,
            spec: TokenSpecNode::CopyOf(copy),
            keywords: Vec::new(),
            abilities: Vec::new(),
            tapped: entry.tapped,
            attacking: entry.attacking,
            name: None,
            legendary: false,
            attached_to: None,
        })));
    }
    trial.enter("token.creation.supertype");
    // `legendary` is the description's supertype only before its stats or
    // color (CR 205.4d); a catalog identity is never read through it.
    let legendary = trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("legendary"))
        && {
            let mut next = trial.clone();
            next.advance_to(next.position() + 1).is_ok()
                && (next
                    .peek_token()
                    .is_some_and(|token| token.token_type == "counterValue")
                    || numeric_power_toughness_starts(&next)
                    || next.peek().is_some_and(|raw| {
                        raw.eq_ignore_ascii_case("colorless")
                            || lookup_card_color_word(raw).is_some()
                    }))
        };
    if legendary {
        trial
            .advance_to(trial.position() + 1)
            .expect("one present supertype");
    }
    let template = match try_recognize_token_descriptor(trial) {
        TokenDescriptorAttempt::Recognized(node) => node,
        TokenDescriptorAttempt::FailedCommitted(f) => return Err(f),
        TokenDescriptorAttempt::NotClaimed => {
            if legendary || name.is_some() {
                return Ok(None);
            }
            return recognize_predefined_token_head(trial, quantity, entry, catalog_reader);
        }
    };
    trial.enter("token.creation.noun");
    let mut amount_after_modifiers = false;
    let quantity = match quantity {
        TokenQuantity::Stated(quantity) => {
            let singular = matches!(quantity, NumericExpression::Atom(NumericAtom::Integer(1)));
            trial.consume(if singular { "token" } else { "tokens" })?;
            quantity
        }
        TokenQuantity::EqualToAfterNoun => {
            trial.consume("tokens")?;
            if word_at(trial, 0, "equal") {
                trial.enter("token.creation.equal-to-quantity");
                cursor_equal_to_quantity(trial)?
            } else {
                // Read by the tail after the modifiers; never lowered.
                amount_after_modifiers = true;
                NumericExpression::from(0)
            }
        }
    };
    let body = TokenCreationBodyNode {
        quantity,
        spec: TokenSpecNode::Described(template),
        keywords: Vec::new(),
        abilities: Vec::new(),
        tapped: entry.tapped,
        attacking: entry.attacking,
        name,
        legendary,
        attached_to: None,
    };
    Ok(Some(TokenBodyHead {
        body,
        amount_after_modifiers,
    }))
}

/// Whether the caller has the catalog-only creation reader as an
/// alternative for a predefined identity with a positive literal quantity.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CatalogReader {
    Available,
    Absent,
}

/// A predefined catalog identity (CR 111.10) read as the shared body when
/// the catalog-only reader cannot carry it: a quantity that is not a positive
/// literal (`X Treasure tokens`, `a number of Food tokens equal to EXPR`) or
/// an Aura entering attached (`a Monster Role token attached to ...`). Any
/// other catalog creation stays the catalog reader's input (`Ok(None)`).
fn recognize_predefined_token_head(
    trial: &mut OracleTokenCursor<'_>,
    quantity: TokenQuantity,
    entry: TokenEntryStateNode,
    catalog_reader: CatalogReader,
) -> super::super::MachineResult<Option<TokenBodyHead>> {
    let mut next = trial.clone();
    let Ok(name) = recognize_named_token_identity(&mut next) else {
        if matches!(quantity, TokenQuantity::EqualToAfterNoun) {
            return Err(trial.fail("an explicit token description after `a number of`"));
        }
        return Ok(None);
    };
    let literal = matches!(
        &quantity,
        TokenQuantity::Stated(NumericExpression::Atom(NumericAtom::Integer(n))) if *n > 0
    );
    let singular = matches!(
        &quantity,
        TokenQuantity::Stated(NumericExpression::Atom(NumericAtom::Integer(1)))
    );
    let noun = if singular { "token" } else { "tokens" };
    let attached = word_at(&next, 1, "attached") && word_at(&next, 2, "to");
    if (catalog_reader == CatalogReader::Available && literal && !attached) || entry.attacking {
        return Ok(None);
    }
    next.enter("token.predefined.noun");
    next.consume(noun)?;
    let quantity = match quantity {
        TokenQuantity::Stated(quantity) => quantity,
        TokenQuantity::EqualToAfterNoun => {
            next.enter("token.creation.equal-to-quantity");
            cursor_equal_to_quantity(&mut next)?
        }
    };
    *trial = next;
    Ok(Some(TokenBodyHead::from(TokenCreationBodyNode {
        quantity,
        spec: TokenSpecNode::Predefined(name),
        keywords: Vec::new(),
        abilities: Vec::new(),
        tapped: entry.tapped,
        attacking: false,
        name: None,
        legendary: false,
        attached_to: None,
    })))
}

/// `equal to EXPR` after `a number of SPEC tokens`: the shared definition
/// grammar of every `equal to` amount.
fn cursor_equal_to_quantity(
    trial: &mut OracleTokenCursor<'_>,
) -> super::super::MachineResult<NumericExpression> {
    trial.consume_phrase(&["equal", "to"])?;
    recognize_equal_to_amount_expression(trial, None)
}

/// `token that's a copy of OBJECT` / `tokens that are copies of OBJECT`,
/// then the optional `, except ...` exceptions.
fn recognize_token_copy(
    trial: &mut OracleTokenCursor<'_>,
    quantity: &NumericExpression,
    objects: Option<TokenObjectContext<'_>>,
) -> super::super::MachineResult<TokenCopyNode> {
    trial.enter("token.copy.noun");
    let singular = matches!(quantity, NumericExpression::Atom(NumericAtom::Integer(1)));
    if singular {
        trial.consume_phrase(&["token", "that's", "a", "copy", "of"])?;
    } else {
        trial.consume_phrase(&["tokens", "that", "are", "copies", "of"])?;
    }
    trial.enter("token.copy.object");
    let objects =
        trial.require_value(objects, "the effect context of a copied object (CR 707.2)")?;
    let object = recognize_contextual_permanent_operand(
        trial,
        objects.name_links,
        objects.name_failure,
        objects.context,
    )?;
    trial.require(
        !matches!(object, PermanentOperandNode::Collection { .. }),
        "one copied object, not a collection",
    )?;
    let exceptions = if trial.peek() == Some(",")
        && trial
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("except"))
    {
        recognize_copy_exceptions(trial, !singular)?
    } else {
        TokenCopyExceptionsNode::default()
    };
    Ok(TokenCopyNode { object, exceptions })
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// The start of one exception item: a pronoun subject (with or without its
/// contracted copula), or a verb sharing the previous item's subject.
fn exception_item_starts(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    [
        "it", "they", "it's", "they're", "has", "have", "isn't", "aren't", "is", "are",
    ]
    .iter()
    .any(|word| word_at(cursor, offset, word))
}

/// `, except it's 1/1 and it's a Nightmare in addition to its other types`,
/// `, except it has haste`, `, except they aren't legendary`. Each item is
/// one copiable-value exception (CR 707.9b); items are coordinated by `and`.
fn recognize_copy_exceptions(
    cursor: &mut OracleTokenCursor<'_>,
    plural: bool,
) -> super::super::MachineResult<TokenCopyExceptionsNode> {
    cursor.enter("token.copy.except");
    cursor.consume_phrase(&[",", "except"])?;
    let mut exceptions = TokenCopyExceptionsNode::default();
    let mut changes = BecomeTypesNode::empty(());
    let number = if plural {
        TypeChangeNumber::Plural
    } else {
        TypeChangeNumber::Singular
    };
    let mut first = true;
    loop {
        cursor.enter("token.copy.except.item");
        let copula = if word_at(cursor, 0, "it's") || word_at(cursor, 0, "they're") {
            let raw = cursor.peek().expect("a contracted copula").to_string();
            cursor.consume(&raw)?;
            true
        } else {
            if word_at(cursor, 0, "it") || word_at(cursor, 0, "they") {
                let raw = cursor.peek().expect("a pronoun").to_string();
                cursor.consume(&raw)?;
            } else {
                cursor.require(!first, "a pronoun subject for the first copy exception")?;
            }
            false
        };
        first = false;
        if copula {
            recognize_copula_exception(cursor, number, &mut changes, &mut exceptions)?;
        } else if word_at(cursor, 0, "has") || word_at(cursor, 0, "have") {
            let raw = cursor.peek().expect("a possession verb").to_string();
            cursor.consume(&raw)?;
            let list =
                recognize_effect_owned_granted_ability_list(cursor, GRANTED_OBJECT_TYPE_LINE)?;
            changes.characteristics.keywords.extend(list.keywords);
            changes.characteristics.abilities.extend(list.abilities);
        } else if word_at(cursor, 0, "isn't") || word_at(cursor, 0, "aren't") {
            let raw = cursor.peek().expect("a negated copula").to_string();
            cursor.consume(&raw)?;
            cursor.consume("legendary")?;
            cursor.require(!exceptions.not_legendary, "one `isn't legendary` exception")?;
            exceptions.not_legendary = true;
        } else {
            cursor.enter("token.copy.except.copula");
            let raw = cursor
                .peek()
                .map(str::to_ascii_lowercase)
                .ok_or_else(|| cursor.fail("a copy exception"))?;
            cursor.require(
                matches!(raw.as_str(), "is" | "are"),
                "a copy exception verb",
            )?;
            cursor.consume(&raw)?;
            recognize_copula_exception(cursor, number, &mut changes, &mut exceptions)?;
        }
        cursor.enter("token.copy.except.separator");
        let mut next = cursor.clone();
        if next.peek() == Some(",") {
            next.consume(",")?;
        }
        if word_at(&next, 0, "and") && exception_item_starts(&next, 1) {
            next.consume("and")?;
            *cursor = next;
            continue;
        }
        break;
    }
    let states_changes = changes.names_types_or_characteristics();
    exceptions.changes = states_changes.then_some(changes);
    cursor.require(
        exceptions.changes.is_some() || exceptions.not_legendary,
        "at least one copy exception",
    )?;
    Ok(exceptions)
}

/// The complement of a copula in an exception: `not legendary`, a base
/// power/toughness, colors, or type nouns in addition to the copy's types.
fn recognize_copula_exception(
    cursor: &mut OracleTokenCursor<'_>,
    number: TypeChangeNumber,
    changes: &mut BecomeTypesNode<()>,
    exceptions: &mut TokenCopyExceptionsNode,
) -> super::super::MachineResult<()> {
    cursor.enter("token.copy.except.complement");
    if word_at(cursor, 0, "not") && word_at(cursor, 1, "legendary") {
        cursor.consume_phrase(&["not", "legendary"])?;
        cursor.require(!exceptions.not_legendary, "one `not legendary` exception")?;
        exceptions.not_legendary = true;
        return Ok(());
    }
    let article = word_at(cursor, 0, "a") || word_at(cursor, 0, "an");
    if article {
        let raw = cursor.peek().expect("an article").to_string();
        cursor.consume(&raw)?;
    }
    if numeric_power_toughness_starts(cursor) {
        cursor.require(
            changes.base_power_toughness().is_none(),
            "one base power/toughness exception",
        )?;
        changes.power_toughness = Some(recognize_numeric_power_toughness(cursor)?);
    }
    if cursor.peek().and_then(lookup_card_color_word).is_some() || word_at(cursor, 0, "colorless") {
        cursor.require(changes.colors.is_none(), "one color exception")?;
        changes.colors = Some(recognize_color_complement(cursor)?);
    }
    let nouns_number = if article {
        TypeChangeNumber::Singular
    } else {
        number
    };
    if type_change_noun_at(cursor, 0, nouns_number) {
        cursor.require(
            changes.subtypes.is_empty() && changes.card_types.is_empty(),
            "one type exception",
        )?;
        let nouns = recognize_type_change_nouns(cursor, nouns_number)?;
        cursor.enter("token.copy.except.retention");
        // A copy exception that adds types keeps the copied ones only when
        // the text says so; a replacing type exception has no primitive.
        cursor.consume_phrase(&["in", "addition", "to"])?;
        cursor.consume(match number {
            TypeChangeNumber::Singular => "its",
            TypeChangeNumber::Plural => "their",
        })?;
        cursor.consume_phrase(&["other", "types"])?;
        changes.subtypes = nouns.subtypes;
        changes.card_types = nouns.card_types;
        changes.retention = TypeChangeRetention::InAdditionToOtherTypes;
    } else {
        cursor.require(
            changes.power_toughness.is_some() || changes.colors.is_some(),
            "a power/toughness, color or type exception after the copula",
        )?;
        cursor.require(!article, "type nouns after `a` or `an`")?;
    }
    Ok(())
}

fn lookup_card_color_word(raw: &str) -> Option<CardColor> {
    crate::oracle::characteristic_vocabulary::lookup_card_color(raw)
}

impl<Subject> BecomeTypesNode<Subject> {
    /// Whether any copiable characteristic is stated.
    fn names_types_or_characteristics(&self) -> bool {
        !self.subtypes.is_empty()
            || !self.card_types.is_empty()
            || self.colors.is_some()
            || self.base_power_toughness().is_some()
            || !self.characteristics.keywords.is_empty()
            || !self.characteristics.abilities.is_empty()
    }
}

/// The modifiers written after the noun, in any order and each once
/// (CR 111.4): a `with` list of granted abilities, a given `named NAME`, and
/// an entry-state relative clause (`that's tapped and attacking`). A
/// following `It has|They have` sentence states more of the same abilities.
fn recognize_token_body_tail(
    trial: &mut OracleTokenCursor<'_>,
    head: TokenBodyHead,
    objects: Option<TokenObjectContext<'_>>,
) -> super::super::MachineResult<TokenCreationBodyNode> {
    let TokenBodyHead {
        body: mut head,
        amount_after_modifiers,
    } = head;
    if head.predefined().is_some() {
        if word_at(trial, 0, "attached") && word_at(trial, 1, "to") {
            head.attached_to = Some(recognize_token_attachment(trial, objects)?);
        }
        trial.enter("token.creation.complete");
        return Ok(head);
    }
    let Some(mut template) = head.described().cloned() else {
        trial.enter("token.creation.complete");
        return Ok(head);
    };
    let type_line = template.granted_type_line();
    let singular = !amount_after_modifiers
        && matches!(
            head.quantity,
            NumericExpression::Atom(NumericAtom::Integer(1))
        );
    let mut granted = GrantedAbilityListNode {
        keywords: Vec::new(),
        abilities: Vec::new(),
    };
    let mut with_read = false;
    let mut entry_clause_read = false;
    let mut color_clause_read = false;
    let mut name = head.name.clone();
    let mut tapped = head.tapped;
    let mut attacking = head.attacking;
    loop {
        trial.enter("token.creation.modifier");
        if !with_read && word_at(trial, 0, "with") {
            trial.consume("with")?;
            let list = recognize_effect_owned_granted_ability_list(trial, &type_line)?;
            granted.keywords.extend(list.keywords);
            granted.abilities.extend(list.abilities);
            with_read = true;
            continue;
        }
        if name.is_none() && word_at(trial, 0, "named") {
            trial.enter("token.creation.given-name");
            trial.consume("named")?;
            name = Some(recognize_given_token_name(trial)?);
            continue;
        }
        if !entry_clause_read && let Some(state) = recognize_token_entry_clause(trial, singular)? {
            trial.enter("token.creation.entry-clause");
            trial.require(
                !(tapped || attacking),
                "one entry state for the created tokens",
            )?;
            tapped = state.tapped;
            attacking = state.attacking;
            entry_clause_read = true;
            continue;
        }
        if !color_clause_read && let Some(colors) = recognize_token_color_clause(trial, singular)? {
            trial.enter("token.creation.color-clause");
            trial.require(
                template.creature.colors.is_empty(),
                "one statement of the token's colors",
            )?;
            template.creature.colors = colors;
            color_clause_read = true;
            continue;
        }
        break;
    }
    if amount_after_modifiers {
        trial.enter("token.creation.equal-to-quantity");
        head.quantity = cursor_equal_to_quantity(trial)?;
    }
    // `... tokens. They have ABILITIES` / `... token. It has ABILITIES`: the
    // next sentence states abilities the created tokens have, the same
    // characteristics as a `with` list (CR 111.4). A temporary `gain` grant
    // stays a separate action owned by the sequence.
    let (pronoun, verb) = if singular {
        ("it", "has")
    } else {
        ("they", "have")
    };
    if trial.peek() == Some(".")
        && trial
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(pronoun))
        && trial
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(verb))
    {
        trial.enter("token.creation.characteristic-sentence");
        trial.consume_phrase(&[".", pronoun, verb])?;
        let more = recognize_effect_owned_granted_ability_list(trial, &type_line)?;
        granted.keywords.extend(more.keywords);
        granted.abilities.extend(more.abilities);
    }
    if template.statistics.is_none() && template.is_creature() {
        // CR 604.3, 208.2: a creature token whose description states no
        // power and toughness has them defined by its own ability.
        trial.enter("token.creation.characteristic-defining-statistics");
        trial.require(
            granted
                .abilities
                .iter()
                .any(|ability| defines_power_and_toughness(&ability.rule)),
            "a characteristic-defining power and toughness ability for a creature token without stated power and toughness",
        )?;
    }
    trial.enter("token.creation.complete");
    Ok(TokenCreationBodyNode {
        spec: TokenSpecNode::Described(template),
        keywords: granted.keywords,
        abilities: granted.abilities,
        name,
        tapped,
        attacking,
        ..head
    })
}

/// `attached to OBJECT`: the object an Aura token enters attached to
/// (CR 111.3, 303.4f), read by the shared contextual permanent operand
/// grammar. One object; a collection is not one attachment.
fn recognize_token_attachment(
    trial: &mut OracleTokenCursor<'_>,
    objects: Option<TokenObjectContext<'_>>,
) -> super::super::MachineResult<PermanentOperandNode> {
    trial.enter("token.attachment.object");
    trial.consume_phrase(&["attached", "to"])?;
    let objects = trial.require_value(
        objects,
        "the effect context of the object an Aura token enters attached to",
    )?;
    let object = recognize_contextual_permanent_operand(
        trial,
        objects.name_links,
        objects.name_failure,
        objects.context,
    )?;
    trial.require(
        !matches!(object, PermanentOperandNode::Collection { .. }),
        "one object for the Aura token to enter attached to",
    )?;
    Ok(object)
}

/// Whether a compiled quoted ability is a characteristic-defining ability
/// setting both power and toughness of its own object (CR 604.3a).
fn defines_power_and_toughness(rule: &serde_json::Value) -> bool {
    if rule["characteristicDefining"].as_bool() != Some(true) {
        return false;
    }
    let kinds = rule["modifiers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|modifier| modifier["kind"].as_str())
        .collect::<Vec<_>>();
    kinds.contains(&"setBasePowerToughness")
        || (kinds.contains(&"setPower") && kinds.contains(&"setToughness"))
}

/// The words of a given token name (CR 111.4, 201.2): capitalized words,
/// joined by lowercase particles only between capitalized words (`Kobolds
/// of Kher Keep`). It ends at punctuation or at the next lowercase word.
fn recognize_given_token_name(
    trial: &mut OracleTokenCursor<'_>,
) -> super::super::MachineResult<String> {
    let capitalized = |raw: &str| {
        raw.chars().next().is_some_and(char::is_uppercase)
            && raw.chars().any(char::is_alphabetic)
            && raw
                .chars()
                .all(|character| character.is_alphanumeric() || "'\u{2019}-".contains(character))
    };
    let mut words = Vec::new();
    loop {
        match trial.peek() {
            Some(raw) if capitalized(raw) => {
                words.push(raw.to_string());
                trial.advance_to(trial.position() + 1)?;
            }
            Some(raw)
                if !words.is_empty()
                    && ["of", "the"]
                        .iter()
                        .any(|particle| raw.eq_ignore_ascii_case(particle))
                    && trial.peek_ahead(1).is_some_and(capitalized) =>
            {
                words.push(raw.to_string());
                trial.advance_to(trial.position() + 1)?;
            }
            _ => break,
        }
    }
    trial.require(!words.is_empty(), "a capitalized token name after `named`")?;
    Ok(words.join(" "))
}
