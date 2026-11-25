//! One characteristic-change frame (CR 205.1a/b, 105.3, 613.1a-f, 613.4b):
//!
//! ```text
//! Change      := Subject Verb Complement ("and" Verb Complement)* Duration? StillSentence?
//! Verb        := become | lose | have          (agreeing with the subject)
//! Complement  := "a" "copy" "of" Object                       (layer 1, CR 707.2)
//!              | Article? Supertypes? PT? Supertypes? Colors? TypeNouns With? Retention
//!                                                               (layers 4, 5, 7b)
//!              | ColorComplement                                (layer 5)
//!              | "the" ("creature" | "basic" "land") "type" "of" "your" "choice"
//!              | "all" "abilities" | KeywordList                (after `lose`, layer 6)
//!              | BaseStats StatValue                            (after `have`, layer 7b)
//!              | "base" "power" StatComponent                   (after `have`, layer 7b)
//!              | GrantedList                                    (after `gain`, layer 6)
//! Switch      := "switch" (Possessed "power" "and" "toughness"
//!                         | "the" "power" "and" "toughness" "of" Object) Duration?
//!                                                               (layer 7d, CR 613.4d)
//! Copula      := Complement Retention? StillSentence?           (a static `is`/`are`)
//! BaseStats   := "base" "power" "and" "base"? "toughness"
//!              | "power" "and" "toughness"                      (after `with`)
//! StatValue   := PT | "each" "equal" "to" Amount     (`its` names the clause subject)
//! ```
//!
//! The parent owns the subject and tells the frame its grammatical number
//! and the verb agreement it requires (a plural subject, a causative `have`
//! and a modal take the base form). Each characteristic is an independent
//! typed child (shared power/toughness pair, color complement, type nouns,
//! keyword lists, object reference) and lowers to one layer operation; the
//! frame only owns their Oracle order, the coordination of verbs sharing the
//! subject, and the single duration that scopes the whole chain. Retention
//! is carried as data (CR 205.1b), never inferred again by a lowering parent.
//! The final `It's still a TYPE` sentence belongs to this effect; the
//! enclosing sequence keeps its own final terminator.
use crate::oracle::canonical::automata::{VerbAgreement, VerbForm, VerbLemma, is_verb_form};
use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, lookup_permanent_card_type, lookup_supertype,
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    BasePowerToughnessNode, CardSupertype, ColorComplementNode, EffectDurationNode,
    GRANTED_OBJECT_TYPE_LINE, GrantedKeyword, NumericExpression, ObjectCardType, QuotedAbilityNode,
    RecognizedEffectFragment, TargetListNode, TokenCopyExceptionsNode, TypeChangeNumber,
    TypeChangeSubtype, color_complement_starts, numeric_power_toughness_starts,
    recognize_card_color_set, recognize_clause_subject_equal_amount, recognize_color_complement,
    recognize_copy_exceptions, recognize_effect_owned_granted_ability_list,
    recognize_effect_owned_keyword_list, recognize_numeric_power_toughness,
    recognize_single_stat_component, recognize_trailing_or_supplied_duration,
    recognize_type_change_nouns, type_change_noun_at,
};

/// Whether the new types supplement or replace the object's existing ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TypeChangeRetention {
    /// No retention clause: new card types and subtypes replace those of
    /// their class (CR 205.1a).
    ReplacesSameClass,
    /// `in addition to its other types` (CR 205.1b).
    InAdditionToOtherTypes,
    /// `that's still a TYPE` / `It's still a TYPE` (CR 205.1b): every prior
    /// card type, supertype and subtype is retained.
    StillA(ObjectCardType),
}

impl TypeChangeRetention {
    pub(in crate::oracle::canonical) fn retains_prior_types(self) -> bool {
        !matches!(self, Self::ReplacesSameClass)
    }
}

/// A subtype chosen by the ability's controller as the effect resolves
/// (`becomes the creature type of your choice`, CR 205.3m, 305.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ChosenSubtypeClass {
    CreatureType,
    BasicLandType,
}

/// Characteristics stated by `with ...`, or by a coordinated `has`/`loses`/
/// `gains` verb (CR 613.1f, 613.4b). Keywords and quoted abilities reuse the
/// shared granted list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct BecomeCharacteristicsNode {
    pub keywords: Vec<GrantedKeyword>,
    pub abilities: Vec<QuotedAbilityNode>,
    /// `with all creature types` (CR 205.3m) is a subtype characteristic.
    pub all_creature_types: bool,
    /// `with base power and toughness N/N` / `has base power and toughness N/N`
    /// / `its base power and toughness become X/X`.
    pub base_power_toughness: Option<BasePowerToughnessNode>,
    /// `loses all abilities` (CR 613.1f).
    pub loses_all_abilities: bool,
    /// `loses flying and trample` (CR 613.1f): the keywords removed.
    pub lost_keywords: Vec<GrantedKeyword>,
    /// `has base power N` (CR 613.4b): the base power alone, set in layer
    /// 7b; the toughness keeps its own value.
    pub base_power: Option<NumericExpression>,
    /// `switch ... power and toughness` (CR 613.4d): the switch applies in
    /// layer 7d, after every other power/toughness effect.
    pub switches_power_toughness: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct BecomeTypesNode<Subject = TargetListNode> {
    pub objects: Subject,
    /// `a N/N ...` / `an X/X ...`: base power and toughness set in layer 7b.
    pub power_toughness: Option<BasePowerToughnessNode>,
    /// `a blue and red ...`, `becomes white`, `becomes the color of your
    /// choice`: the new color set (CR 105.3).
    pub colors: Option<ColorComplementNode>,
    /// `a legendary 4/4 red Dragon creature`: supertypes the object gains
    /// (CR 205.4); no change states a supertype it removes.
    pub supertypes: Vec<CardSupertype>,
    pub subtypes: Vec<TypeChangeSubtype>,
    pub card_types: Vec<ObjectCardType>,
    /// `becomes the creature type of your choice`.
    pub chosen_subtype: Option<ChosenSubtypeClass>,
    /// `becomes a copy of OBJECT` (layer 1, CR 707.2).
    pub copy_of: Option<Subject>,
    /// `, except ...` after the copy (CR 707.9b), read by the shared
    /// copy-exception reader.
    pub copy_exceptions: Option<Box<TokenCopyExceptionsNode>>,
    /// `its name is NAME` (CR 201.2, 707.9b): the linked name the object
    /// has, a copiable-value exception only.
    pub copy_name: Option<String>,
    pub characteristics: BecomeCharacteristicsNode,
    pub retention: TypeChangeRetention,
    /// `None` is a continuous effect without a stated duration (CR 611.2a).
    pub duration: Option<EffectDurationNode>,
}

impl<Subject> BecomeTypesNode<Subject> {
    /// A change of `objects` that states no characteristic yet.
    pub(in crate::oracle::canonical) fn empty(objects: Subject) -> Self {
        Self {
            objects,
            power_toughness: None,
            colors: None,
            supertypes: Vec::new(),
            subtypes: Vec::new(),
            card_types: Vec::new(),
            chosen_subtype: None,
            copy_of: None,
            copy_exceptions: None,
            copy_name: None,
            characteristics: BecomeCharacteristicsNode::default(),
            retention: TypeChangeRetention::ReplacesSameClass,
            duration: None,
        }
    }

    /// The base power/toughness from either Oracle position.
    pub(in crate::oracle::canonical) fn base_power_toughness(
        &self,
    ) -> Option<&BasePowerToughnessNode> {
        self.power_toughness
            .as_ref()
            .or(self.characteristics.base_power_toughness.as_ref())
    }

    /// Whether the change names at least one type noun.
    fn names_types(&self) -> bool {
        !self.subtypes.is_empty() || !self.card_types.is_empty()
    }
}

/// The three lemmas that open a characteristic change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CharacteristicVerb {
    Become,
    Lose,
    HaveBase,
    Gain,
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

fn verb_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
    lemma: VerbLemma,
    agreement: VerbAgreement,
) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| is_verb_form(raw, lemma, agreement.form()))
}

/// Whether the complement of `become` at `offset` is a characteristic this
/// frame owns. `becomes the target of`, `becomes tapped` or a designation
/// (`becomes monstrous`) are other frames and are not claimed.
fn become_complement_starts(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
    number: TypeChangeNumber,
) -> bool {
    let mut trial = cursor.clone();
    if trial.advance_to(trial.position() + offset).is_err() {
        return false;
    }
    let article = word_at(&trial, 0, "a") || word_at(&trial, 0, "an");
    if article {
        if number != TypeChangeNumber::Singular {
            return false;
        }
        if word_at(&trial, 1, "copy") && word_at(&trial, 2, "of") {
            return true;
        }
        let _ = trial.advance_to(trial.position() + 1);
        // `a legendary 4/4 red Dragon creature`: prenominal supertypes.
        while trial
            .peek()
            .is_some_and(|raw| lookup_supertype(raw, AcceptedForms::SingularOnly).is_some())
        {
            let _ = trial.advance_to(trial.position() + 1);
        }
    }
    if numeric_power_toughness_starts(&trial) {
        return article || number == TypeChangeNumber::Plural;
    }
    let mut colors = trial.clone();
    if color_complement_starts(&colors) {
        if !article {
            // A bare color complement (`becomes white`) or, for a plural
            // subject, colors before plural type nouns.
            return true;
        }
        if recognize_card_color_set(&mut colors).is_err() && !word_at(&colors, 0, "colorless") {
            return false;
        }
        if word_at(&colors, 0, "colorless") {
            let _ = colors.advance_to(colors.position() + 1);
        }
        return type_change_noun_at(&colors, 0, TypeChangeNumber::Singular);
    }
    if !article
        && word_at(&trial, 0, "the")
        && ((word_at(&trial, 1, "creature") && word_at(&trial, 2, "type"))
            || (word_at(&trial, 1, "basic") && word_at(&trial, 2, "land")))
    {
        return true;
    }
    type_change_noun_at(
        &trial,
        0,
        if article {
            TypeChangeNumber::Singular
        } else {
            number
        },
    ) && (article || number == TypeChangeNumber::Plural)
}

/// Whether the complement of `lose` at `offset` is `all abilities` or a
/// keyword list (`loses flying`). `loses 3 life` stays a player action.
fn loss_complement_starts(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    if word_at(cursor, offset, "all") && word_at(cursor, offset + 1, "abilities") {
        return true;
    }
    let mut trial = cursor.clone();
    trial.advance_to(trial.position() + offset).is_ok()
        && recognize_effect_owned_keyword_list(&mut trial).is_ok()
}

fn characteristic_verb_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
    agreement: VerbAgreement,
    number: TypeChangeNumber,
) -> Option<CharacteristicVerb> {
    if verb_at(cursor, offset, VerbLemma::Become, agreement)
        && become_complement_starts(cursor, offset + 1, number)
    {
        return Some(CharacteristicVerb::Become);
    }
    if verb_at(cursor, offset, VerbLemma::Lose, agreement)
        && loss_complement_starts(cursor, offset + 1)
    {
        return Some(CharacteristicVerb::Lose);
    }
    if verb_at(cursor, offset, VerbLemma::Have, agreement)
        && (base_stats_length(cursor, offset + 1).is_some_and(|(_, base)| base)
            || base_power_alone_at(cursor, offset + 1))
    {
        return Some(CharacteristicVerb::HaveBase);
    }
    None
}

/// `base power N` without `and toughness` (CR 613.4b): layer 7b of the
/// power alone. `base power and toughness` is the pair instead.
fn base_power_alone_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    word_at(cursor, offset, "base")
        && word_at(cursor, offset + 1, "power")
        && !word_at(cursor, offset + 2, "and")
}

/// The token length of the stated pair `[base] power and [base] toughness`
/// at `offset`, and whether it is written with `base`. A single stat (`base
/// power 0`) is no pair: layer 7b of one characteristic is not represented.
fn base_stats_length(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<(usize, bool)> {
    let base = word_at(cursor, offset, "base");
    let mut at = offset + usize::from(base);
    if !(word_at(cursor, at, "power") && word_at(cursor, at + 1, "and")) {
        return None;
    }
    at += 2;
    if base && word_at(cursor, at, "base") {
        at += 1;
    }
    word_at(cursor, at, "toughness").then_some((at + 1 - offset, base))
}

/// `[base] power and [base] toughness` then its value: a numeric pair, or
/// `each equal to AMOUNT` (CR 208.1: one amount for both characteristics),
/// whose possessive `its` names the clause subject the parent installed.
fn recognize_stated_base_stats(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<BasePowerToughnessNode> {
    cursor.enter("effect.become-types.base-stats");
    let (length, _) = base_stats_length(cursor, 0)
        .ok_or_else(|| cursor.fail("`[base] power and [base] toughness`"))?;
    cursor.advance_to(cursor.position() + length)?;
    cursor.enter("effect.become-types.base-stats.value");
    if word_at(cursor, 0, "each") {
        cursor.consume("each")?;
        let amount = recognize_clause_subject_equal_amount(cursor)?;
        return Ok(BasePowerToughnessNode {
            power: amount.clone(),
            toughness: amount,
        });
    }
    recognize_numeric_power_toughness(cursor)
}

/// Whether the cursor is at a verb this frame owns, in the agreement the
/// parent's subject requires. The claim reads the verb lemma and the start
/// of a complement the frame can own, never a fixed word sequence.
pub(in crate::oracle::canonical) fn characteristic_change_verb_starts(
    cursor: &OracleTokenCursor<'_>,
    agreement: VerbAgreement,
    number: TypeChangeNumber,
) -> bool {
    characteristic_verb_at(cursor, 0, agreement, number).is_some()
}

/// `its base power and toughness become|are P/T` after a proven antecedent.
pub(in crate::oracle::canonical) fn possessed_base_power_toughness_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    ["its", "base", "power", "and", "toughness"]
        .iter()
        .enumerate()
        .all(|(offset, word)| word_at(cursor, offset, word))
        && (verb_at(cursor, 5, VerbLemma::Become, VerbAgreement::Base) || word_at(cursor, 5, "are"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BecomeTypesState {
    Subject,
    Verb,
    BecomeComplement,
    Article,
    Statistics,
    Colors,
    Types,
    With,
    RelativeRetention,
    Retention,
    Coordination,
    Duration,
    CopyExceptions,
    RetentionSentence,
    Complete,
}

/// Whether the frame reads a change (a verb chain with one duration) or
/// the complement of a static copula (`is`/`are`), which owns neither verbs,
/// durations nor copies: the static parent owns its predicate list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameMode {
    Change,
    Copula,
}

/// The frame after a subject the parent recognized. `copy_object` reads the
/// object of `becomes a copy of`, with the same reference grammar as the
/// subject; it is called only after that complement has been seen.
pub(in crate::oracle::canonical) fn recognize_become_types_subject<'tokens, Subject: Clone>(
    cursor: &mut OracleTokenCursor<'tokens>,
    subject: impl FnMut(&mut OracleTokenCursor<'tokens>) -> MachineResult<Subject>,
    copy_object: impl FnMut(&mut OracleTokenCursor<'tokens>) -> MachineResult<Subject>,
    preposed_duration: Option<&EffectDurationNode>,
    agreement: VerbAgreement,
    number: TypeChangeNumber,
) -> MachineResult<RecognizedEffectFragment<BecomeTypesNode<Subject>>> {
    run_become_frame(
        cursor,
        BecomeTypesState::Subject,
        None,
        subject,
        copy_object,
        preposed_duration,
        agreement,
        number,
        FrameMode::Change,
    )
}

/// The complement of a static copula (`Enchanted land is a 3/3 black Ooze
/// creature. It's still a land.`, `All lands are 1/1 creatures that are
/// still lands.`): the same complement states as `becomes`, without a verb
/// chain, a duration or a copy (CR 205.1b, 613.1). The parent has consumed
/// the copula and owns the subject and the lowering.
pub(in crate::oracle::canonical) fn recognize_characteristic_copula_complement(
    cursor: &mut OracleTokenCursor<'_>,
    number: TypeChangeNumber,
) -> MachineResult<BecomeTypesNode<()>> {
    cursor.enter("static.characteristic-copula");
    run_become_frame(
        cursor,
        BecomeTypesState::BecomeComplement,
        Some(BecomeTypesNode::empty(())),
        |cursor| Err(cursor.fail("a copula subject supplied by the static parent")),
        |cursor| Err(cursor.fail("a copy effect, which a static copula does not state")),
        None,
        match number {
            TypeChangeNumber::Singular => VerbAgreement::ThirdPersonSingular,
            TypeChangeNumber::Plural => VerbAgreement::Base,
        },
        number,
        FrameMode::Copula,
    )
    .map(|fragment| fragment.node)
}

/// Prenominal supertypes after the article (`a legendary 4/4 red Dragon`,
/// `a 6/6 legendary Horror`), each one added (CR 205.4).
fn read_prenominal_supertypes<Subject>(
    cursor: &mut OracleTokenCursor<'_>,
    change: &mut BecomeTypesNode<Subject>,
) -> MachineResult<()> {
    while let Some(supertype) = cursor
        .peek()
        .and_then(|raw| lookup_supertype(raw, AcceptedForms::SingularOnly))
    {
        cursor.enter("effect.become-types.supertype");
        cursor.require(
            !change.supertypes.contains(&supertype),
            "distinct supertypes",
        )?;
        change.supertypes.push(supertype);
        cursor.advance_to(cursor.position() + 1)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_become_frame<'tokens, Subject: Clone>(
    cursor: &mut OracleTokenCursor<'tokens>,
    start: BecomeTypesState,
    initial: Option<BecomeTypesNode<Subject>>,
    mut subject: impl FnMut(&mut OracleTokenCursor<'tokens>) -> MachineResult<Subject>,
    mut copy_object: impl FnMut(&mut OracleTokenCursor<'tokens>) -> MachineResult<Subject>,
    preposed_duration: Option<&EffectDurationNode>,
    agreement: VerbAgreement,
    number: TypeChangeNumber,
    mode: FrameMode,
) -> MachineResult<RecognizedEffectFragment<BecomeTypesNode<Subject>>> {
    let mut state = start;
    let mut node: Option<BecomeTypesNode<Subject>> = initial;
    let mut article = false;
    loop {
        state = match state {
            BecomeTypesState::Subject => {
                cursor.enter("effect.become-types.subject");
                node = Some(BecomeTypesNode::empty(subject(cursor)?));
                BecomeTypesState::Verb
            }
            BecomeTypesState::Verb => {
                cursor.enter("effect.become-types.verb");
                let change = cursor.require_value(node.as_mut(), "a type-change subject")?;
                let verb =
                    characteristic_verb_at(cursor, 0, agreement, number).ok_or_else(|| {
                        cursor.fail("`become`, `lose` or `have base` agreeing with the subject")
                    })?;
                let raw = cursor.peek().expect("a recognized verb").to_string();
                cursor.consume(&raw)?;
                article = false;
                match verb {
                    CharacteristicVerb::Become => BecomeTypesState::BecomeComplement,
                    CharacteristicVerb::Lose => {
                        recognize_loss_into(cursor, &mut change.characteristics)?;
                        BecomeTypesState::Coordination
                    }
                    CharacteristicVerb::HaveBase => {
                        recognize_have_base_into(cursor, change)?;
                        BecomeTypesState::Coordination
                    }
                    CharacteristicVerb::Gain => {
                        return Err(
                            cursor.fail("a characteristic verb before a coordinated `gain`")
                        );
                    }
                }
            }
            BecomeTypesState::BecomeComplement => {
                cursor.enter("effect.become-types.complement");
                let change = cursor.require_value(node.as_mut(), "a type-change subject")?;
                let article_here = word_at(cursor, 0, "a") || word_at(cursor, 0, "an");
                if article_here && word_at(cursor, 1, "copy") && word_at(cursor, 2, "of") {
                    cursor.enter("effect.become-types.copy");
                    cursor.require(
                        change.copy_of.is_none() && !change.names_types(),
                        "one copy effect, without other stated types",
                    )?;
                    let raw = cursor.peek().expect("an article").to_string();
                    cursor.consume(&raw)?;
                    cursor.consume_phrase(&["copy", "of"])?;
                    change.copy_of = Some(copy_object(cursor)?);
                    BecomeTypesState::Coordination
                } else if article_here {
                    BecomeTypesState::Article
                } else if word_at(cursor, 0, "the")
                    && (word_at(cursor, 1, "creature") || word_at(cursor, 1, "basic"))
                {
                    cursor.enter("effect.become-types.chosen-subtype");
                    cursor.require(
                        change.chosen_subtype.is_none() && !change.names_types(),
                        "one subtype choice, without other stated types",
                    )?;
                    cursor.consume("the")?;
                    change.chosen_subtype = Some(if word_at(cursor, 0, "creature") {
                        cursor.consume_phrase(&["creature", "type"])?;
                        ChosenSubtypeClass::CreatureType
                    } else {
                        cursor.consume_phrase(&["basic", "land", "type"])?;
                        ChosenSubtypeClass::BasicLandType
                    });
                    cursor.consume_phrase(&["of", "your", "choice"])?;
                    BecomeTypesState::Coordination
                } else if numeric_power_toughness_starts(cursor) {
                    // A plural subject: `become 0/1 Plant creatures`.
                    cursor.require(
                        number == TypeChangeNumber::Plural,
                        "`a` or `an` before the new types of a singular subject",
                    )?;
                    BecomeTypesState::Statistics
                } else {
                    cursor.require(color_complement_starts(cursor) || number == TypeChangeNumber::Plural,
                        "a color complement, a copy, a chosen type or `a`/`an` before the new types",
                    )?;
                    if color_complement_starts(cursor) {
                        cursor.require(change.colors.is_none(), "one stated color set")?;
                        change.colors = Some(recognize_color_complement(cursor)?);
                        if number == TypeChangeNumber::Plural
                            && type_change_noun_at(cursor, 0, TypeChangeNumber::Plural)
                        {
                            BecomeTypesState::Types
                        } else {
                            BecomeTypesState::Coordination
                        }
                    } else {
                        BecomeTypesState::Types
                    }
                }
            }
            BecomeTypesState::Article => {
                cursor.enter("effect.become-types.article");
                let raw = cursor
                    .peek()
                    .map(str::to_ascii_lowercase)
                    .ok_or_else(|| cursor.fail("`a` or `an` before the new types"))?;
                cursor.require(matches!(raw.as_str(), "a" | "an"), "`a` or `an`")?;
                cursor.require(
                    number == TypeChangeNumber::Singular,
                    "an article only after a singular subject",
                )?;
                cursor.consume(&raw)?;
                article = true;
                BecomeTypesState::Statistics
            }
            BecomeTypesState::Statistics => {
                cursor.enter("effect.become-types.power-toughness");
                if article {
                    let change = cursor.require_value(node.as_mut(), "a type-change subject")?;
                    read_prenominal_supertypes(cursor, change)?;
                }
                if numeric_power_toughness_starts(cursor) {
                    let change = cursor.require_value(node.as_mut(), "a type-change subject")?;
                    cursor.require(
                        change.base_power_toughness().is_none(),
                        "one stated base power/toughness",
                    )?;
                    change.power_toughness = Some(recognize_numeric_power_toughness(cursor)?);
                }
                BecomeTypesState::Colors
            }
            BecomeTypesState::Colors => {
                cursor.enter("effect.become-types.colors");
                let change = cursor.require_value(node.as_mut(), "a type-change subject")?;
                if article {
                    read_prenominal_supertypes(cursor, change)?;
                }
                if word_at(cursor, 0, "colorless")
                    || cursor
                        .peek()
                        .and_then(crate::oracle::characteristic_vocabulary::lookup_card_color)
                        .is_some()
                {
                    cursor.require(change.colors.is_none(), "one stated color set")?;
                    change.colors = Some(if word_at(cursor, 0, "colorless") {
                        cursor.consume("colorless")?;
                        ColorComplementNode::Colors(super::CardColorSetNode { colors: Vec::new() })
                    } else {
                        ColorComplementNode::Colors(recognize_card_color_set(cursor)?)
                    });
                }
                BecomeTypesState::Types
            }
            BecomeTypesState::Types => {
                cursor.enter("effect.become-types.types");
                let change = cursor.require_value(node.as_mut(), "a type-change subject")?;
                cursor.require(!change.names_types(), "one stated type list")?;
                let nouns = recognize_type_change_nouns(
                    cursor,
                    if article {
                        TypeChangeNumber::Singular
                    } else {
                        number
                    },
                )?;
                change.subtypes = nouns.subtypes;
                change.card_types = nouns.card_types;
                // CR 208.1: only a creature has power and toughness.
                cursor.require(
                    change.power_toughness.is_none()
                        || change.card_types.contains(&ObjectCardType::Creature),
                    "the creature card type with a stated power/toughness",
                )?;
                BecomeTypesState::With
            }
            BecomeTypesState::With => {
                cursor.enter("effect.become-types.with");
                if word_at(cursor, 0, "with") {
                    let change = cursor.require_value(node.as_mut(), "a type-change subject")?;
                    cursor.consume("with")?;
                    let had_statistics = change.power_toughness.is_some();
                    recognize_become_characteristics(cursor, &mut change.characteristics)?;
                    cursor.require(
                        !had_statistics || change.characteristics.base_power_toughness.is_none(),
                        "one stated base power/toughness",
                    )?;
                }
                BecomeTypesState::RelativeRetention
            }
            BecomeTypesState::RelativeRetention => {
                cursor.enter("effect.become-types.relative-retention");
                if word_at(cursor, 0, "that's") && word_at(cursor, 1, "still") {
                    let change = cursor.require_value(node.as_mut(), "a type-change subject")?;
                    cursor.consume("that's")?;
                    change.retention = recognize_still_type(cursor, &change.card_types)?;
                } else if number == TypeChangeNumber::Plural
                    && word_at(cursor, 0, "that")
                    && word_at(cursor, 1, "are")
                    && word_at(cursor, 2, "still")
                {
                    // `1/1 creatures that are still lands` (CR 205.1b).
                    let change = cursor.require_value(node.as_mut(), "a type-change subject")?;
                    cursor.consume_phrase(&["that", "are"])?;
                    change.retention = recognize_plural_still_type(cursor, &change.card_types)?;
                }
                BecomeTypesState::Retention
            }
            BecomeTypesState::Retention => {
                cursor.enter("effect.become-types.retention");
                if word_at(cursor, 0, "in") {
                    let change = cursor.require_value(node.as_mut(), "a type-change subject")?;
                    cursor.require(
                        !change.retention.retains_prior_types(),
                        "one type-retention clause",
                    )?;
                    cursor.consume_phrase(&["in", "addition", "to"])?;
                    cursor.consume(match number {
                        TypeChangeNumber::Singular => "its",
                        TypeChangeNumber::Plural => "their",
                    })?;
                    cursor.consume_phrase(&["other", "types"])?;
                    change.retention = TypeChangeRetention::InAdditionToOtherTypes;
                }
                match mode {
                    FrameMode::Change => BecomeTypesState::Coordination,
                    FrameMode::Copula => BecomeTypesState::RetentionSentence,
                }
            }
            BecomeTypesState::Coordination => {
                cursor.enter("effect.become-types.coordination");
                // `becomes blue and loses flying until end of turn`: verbs
                // sharing the subject before one trailing duration belong
                // to this change. A coordinated `lose`/`have`/`gain` this
                // frame cannot read completely is left to the enclosing
                // sequence; a claimed coordinated `become` commits.
                match word_at(cursor, 0, "and")
                    .then(|| coordinated_verb_at(cursor, agreement, number))
                    .flatten()
                {
                    Some(CharacteristicVerb::Become) => {
                        cursor.consume("and")?;
                        let raw = cursor.peek().expect("a recognized verb").to_string();
                        cursor.consume(&raw)?;
                        article = false;
                        BecomeTypesState::BecomeComplement
                    }
                    Some(verb) => {
                        let mut extended = cursor
                            .require_value(node.as_ref(), "a type-change subject")?
                            .clone();
                        let mut trial = cursor.clone();
                        trial.consume("and")?;
                        if recognize_coordinated_verb(&mut trial, &mut extended, verb).is_ok() {
                            *cursor = trial;
                            node = Some(extended);
                            BecomeTypesState::Coordination
                        } else {
                            BecomeTypesState::Duration
                        }
                    }
                    None => BecomeTypesState::Duration,
                }
            }
            BecomeTypesState::Duration => {
                cursor.enter("effect.become-types.duration");
                let change = cursor.require_value(node.as_mut(), "a type-change subject")?;
                change.duration = if preposed_duration.is_some() || word_at(cursor, 0, "until") {
                    Some(recognize_trailing_or_supplied_duration(
                        cursor,
                        preposed_duration,
                    )?)
                } else {
                    // A trailing duration after a coordinated verb this frame
                    // does not own would scope the whole chain; it is not
                    // propagated backwards, so an open-ended change followed
                    // by one is refused.
                    // A coordinated `get(s) +N/+N` is the enclosing verb
                    // chain's next action: its trailing duration scopes this
                    // change too, and the sequence gives it to this node.
                    cursor.require(
                        !(word_at(cursor, 0, "and")
                            && cursor.peek_ahead(1).is_some_and(|raw| {
                                [
                                    "gain", "gains", "become", "becomes", "lose", "loses", "has",
                                    "have",
                                ]
                                .iter()
                                .any(|verb| raw.eq_ignore_ascii_case(verb))
                            })),
                        "a duration for the type change before a coordinated verb",
                    )?;
                    None
                };
                BecomeTypesState::CopyExceptions
            }
            BecomeTypesState::CopyExceptions => {
                cursor.enter("effect.become-types.copy-exceptions");
                let change = cursor.require_value(node.as_mut(), "a type-change subject")?;
                if change.copy_of.is_some()
                    && cursor.peek() == Some(",")
                    && word_at(cursor, 1, "except")
                {
                    change.copy_exceptions = Some(Box::new(recognize_copy_exceptions(
                        cursor,
                        number == TypeChangeNumber::Plural,
                    )?));
                }
                BecomeTypesState::RetentionSentence
            }
            BecomeTypesState::RetentionSentence => {
                cursor.enter("effect.become-types.retention-sentence");
                let change = cursor.require_value(node.as_mut(), "a type-change subject")?;
                let singular = word_at(cursor, 1, "it's") || word_at(cursor, 1, "it’s");
                let plural = word_at(cursor, 1, "they're") || word_at(cursor, 1, "they’re");
                if change.names_types()
                    && cursor.peek() == Some(".")
                    && (singular || plural)
                    && word_at(cursor, 2, "still")
                {
                    cursor.require(
                        !change.retention.retains_prior_types(),
                        "one type-retention clause",
                    )?;
                    cursor.require(
                        plural == (number == TypeChangeNumber::Plural),
                        "a retention pronoun agreeing in number with the changed objects",
                    )?;
                    cursor.consume(".")?;
                    let pronoun = cursor.peek().expect("a contracted pronoun").to_string();
                    cursor.consume(&pronoun)?;
                    change.retention = if plural {
                        recognize_plural_still_type(cursor, &change.card_types)?
                    } else {
                        recognize_still_type(cursor, &change.card_types)?
                    };
                }
                BecomeTypesState::Complete
            }
            BecomeTypesState::Complete => {
                cursor.enter("effect.become-types.complete");
                return Ok(RecognizedEffectFragment {
                    node: cursor.require_value(node, "a type-change subject")?,
                    end_position: cursor.position(),
                });
            }
        };
    }
}

/// A coordinated verb this frame owns after `and`, or `gain(s)` followed by
/// a granted list.
fn coordinated_verb_at(
    cursor: &OracleTokenCursor<'_>,
    agreement: VerbAgreement,
    number: TypeChangeNumber,
) -> Option<CharacteristicVerb> {
    characteristic_verb_at(cursor, 1, agreement, number).or_else(|| {
        verb_at(cursor, 1, VerbLemma::Gain, agreement).then_some(CharacteristicVerb::Gain)
    })
}

/// One coordinated `lose`, `have base` or `gain` verb and its complete
/// complement, on a trial copy of the node. A coordinated `become` is not
/// read here: it re-enters the complement states of the same frame.
fn recognize_coordinated_verb<Subject>(
    cursor: &mut OracleTokenCursor<'_>,
    change: &mut BecomeTypesNode<Subject>,
    verb: CharacteristicVerb,
) -> MachineResult<()> {
    cursor.enter("effect.become-types.coordinated-verb");
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a coordinated characteristic verb"))?
        .to_string();
    cursor.consume(&raw)?;
    match verb {
        CharacteristicVerb::Lose => recognize_loss_into(cursor, &mut change.characteristics),
        CharacteristicVerb::HaveBase => recognize_have_base_into(cursor, change),
        CharacteristicVerb::Gain => recognize_granted_into(cursor, &mut change.characteristics),
        CharacteristicVerb::Become => {
            Err(cursor.fail("a coordinated `become` read by the complement states"))
        }
    }
}

/// `all abilities` or a keyword list after `lose`.
fn recognize_loss_into(
    cursor: &mut OracleTokenCursor<'_>,
    characteristics: &mut BecomeCharacteristicsNode,
) -> MachineResult<()> {
    cursor.enter("effect.become-types.loss");
    if word_at(cursor, 0, "all") {
        cursor.require(
            !characteristics.loses_all_abilities,
            "one `loses all abilities`",
        )?;
        cursor.consume_phrase(&["all", "abilities"])?;
        characteristics.loses_all_abilities = true;
        return Ok(());
    }
    for keyword in recognize_effect_owned_keyword_list(cursor)? {
        cursor.require(
            !characteristics.lost_keywords.contains(&keyword),
            "distinct lost keywords",
        )?;
        characteristics.lost_keywords.push(keyword);
    }
    Ok(())
}

/// `base power and toughness P/T` after `have`.
fn recognize_have_base_into<Subject>(
    cursor: &mut OracleTokenCursor<'_>,
    change: &mut BecomeTypesNode<Subject>,
) -> MachineResult<()> {
    cursor.enter("effect.become-types.base-power-toughness");
    cursor.require(
        change.base_power_toughness().is_none() && change.characteristics.base_power.is_none(),
        "one stated base power/toughness",
    )?;
    if base_power_alone_at(cursor, 0) {
        cursor.enter("effect.become-types.base-power");
        cursor.consume_phrase(&["base", "power"])?;
        change.characteristics.base_power = Some(recognize_single_stat_component(cursor)?);
        return Ok(());
    }
    change.characteristics.base_power_toughness = Some(recognize_stated_base_stats(cursor)?);
    Ok(())
}

/// `gain(s) KEYWORDS and "ABILITY"`: the shared granted list.
fn recognize_granted_into(
    cursor: &mut OracleTokenCursor<'_>,
    characteristics: &mut BecomeCharacteristicsNode,
) -> MachineResult<()> {
    let list = recognize_effect_owned_granted_ability_list(cursor, GRANTED_OBJECT_TYPE_LINE)?;
    for keyword in list.keywords {
        cursor.require(
            !characteristics.keywords.contains(&keyword),
            "distinct granted keywords",
        )?;
        characteristics.keywords.push(keyword);
    }
    characteristics.abilities.extend(list.abilities);
    Ok(())
}

/// `its base power and toughness become|are P/T [DURATION]`: the possessive
/// names the characteristic of a proven antecedent the parent supplies.
pub(in crate::oracle::canonical) fn recognize_possessed_base_power_toughness<Subject>(
    cursor: &mut OracleTokenCursor<'_>,
    objects: Subject,
    preposed_duration: Option<&EffectDurationNode>,
) -> MachineResult<RecognizedEffectFragment<BecomeTypesNode<Subject>>> {
    cursor.enter("effect.base-power-toughness.subject");
    cursor.consume_phrase(&["its", "base", "power", "and", "toughness"])?;
    cursor.enter("effect.base-power-toughness.verb");
    let verb = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("`become` or `are` after a plural characteristic"))?;
    cursor.require(
        is_verb_form(&verb, VerbLemma::Become, VerbForm::Base) || verb == "are",
        "`become` or `are` after a plural characteristic",
    )?;
    cursor.consume(&verb)?;
    let mut node = BecomeTypesNode::empty(objects);
    node.characteristics.base_power_toughness = Some(recognize_numeric_power_toughness(cursor)?);
    cursor.enter("effect.base-power-toughness.duration");
    node.duration = if preposed_duration.is_some() || word_at(cursor, 0, "until") {
        Some(recognize_trailing_or_supplied_duration(
            cursor,
            preposed_duration,
        )?)
    } else {
        None
    };
    Ok(RecognizedEffectFragment {
        node,
        end_position: cursor.position(),
    })
}

/// Whether `switch` (CR 613.4d) opens an imperative instruction here.
pub(in crate::oracle::canonical) fn switch_power_toughness_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    verb_at(cursor, 0, VerbLemma::Switch, VerbAgreement::Base)
}

/// The stated pair `power and toughness` whose values are switched.
fn consume_switched_pair(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("effect.switch-power-toughness.pair");
    cursor.consume_phrase(&["power", "and", "toughness"])
}

/// `switch POSSESSED power and toughness [DURATION]` / `switch the power and
/// toughness of OBJECTS [DURATION]` (CR 613.4d): one characteristic change
/// whose complement is the layer-7d switch. `possessed` reads the genitive
/// object (`this creature's`, `target creature's`, `its`) and `objects` the
/// object after `of`; both are the parent's shared operand grammar.
pub(in crate::oracle::canonical) fn recognize_switch_power_toughness<'tokens, Subject>(
    cursor: &mut OracleTokenCursor<'tokens>,
    possessed: impl FnOnce(&mut OracleTokenCursor<'tokens>) -> MachineResult<Subject>,
    objects: impl FnOnce(&mut OracleTokenCursor<'tokens>) -> MachineResult<Subject>,
    preposed_duration: Option<&EffectDurationNode>,
) -> MachineResult<RecognizedEffectFragment<BecomeTypesNode<Subject>>> {
    cursor.enter("effect.switch-power-toughness.verb");
    cursor.require(
        switch_power_toughness_starts(cursor),
        "the imperative `switch`",
    )?;
    let raw = cursor.peek().expect("a recognized verb").to_string();
    cursor.consume(&raw)?;
    cursor.enter("effect.switch-power-toughness.object");
    let subject =
        if word_at(cursor, 0, "the") && word_at(cursor, 1, "power") && word_at(cursor, 2, "and") {
            cursor.consume("the")?;
            consume_switched_pair(cursor)?;
            cursor.enter("effect.switch-power-toughness.of");
            cursor.consume("of")?;
            objects(cursor)?
        } else {
            let subject = possessed(cursor)?;
            consume_switched_pair(cursor)?;
            subject
        };
    let mut node = BecomeTypesNode::empty(subject);
    node.characteristics.switches_power_toughness = true;
    cursor.enter("effect.switch-power-toughness.duration");
    node.duration = if preposed_duration.is_some() || word_at(cursor, 0, "until") {
        Some(recognize_trailing_or_supplied_duration(
            cursor,
            preposed_duration,
        )?)
    } else {
        None
    };
    Ok(RecognizedEffectFragment {
        node,
        end_position: cursor.position(),
    })
}

/// True at `It's still a TYPE` / `They're still TYPEs` opening a sentence.
pub(in crate::oracle::canonical) fn type_retention_amendment_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    (word_at(cursor, 0, "it's")
        || word_at(cursor, 0, "it’s")
        || word_at(cursor, 0, "they're")
        || word_at(cursor, 0, "they’re"))
        && word_at(cursor, 1, "still")
}

/// The type change a retention sentence amends: the last action of the
/// sentence just closed, inside an optional wrapper when the change was
/// optional (`you may have it become ...`).
fn last_type_change(
    effect: Option<&mut super::LocalEffectNode>,
) -> Option<&mut BecomeTypesNode<super::PermanentOperandNode>> {
    match effect? {
        super::LocalEffectNode::BecomeTypes(node) => Some(node),
        super::LocalEffectNode::OptionalSequence(sequence) => {
            last_type_change(sequence.effects.last_mut())
        }
        _ => None,
    }
}

/// The last action of a program's last written sentence, through the
/// wrappers that only gate it (`Then you may have it become ...`).
pub(in crate::oracle::canonical) fn last_program_action_mut(
    program: &mut super::EffectProgramNode,
) -> Option<&mut super::LocalEffectNode> {
    match program {
        super::EffectProgramNode::Sequence(sequence) => sequence.effects.last_mut(),
        super::EffectProgramNode::Optional { body, .. } => last_program_action_mut(body),
        super::EffectProgramNode::Outcome {
            body,
            otherwise: None,
            ..
        } => last_program_action_mut(body),
        super::EffectProgramNode::Steps(steps) => last_program_action_mut(steps.last_mut()?),
        super::EffectProgramNode::Outcome { .. }
        | super::EffectProgramNode::Alternatives { .. }
        | super::EffectProgramNode::Offer { .. } => None,
    }
}

/// `It's still a TYPE.` / `They're still TYPEs.` as its own sentence after
/// a type change whose sentence another tail closed first (a postposed
/// `where X is ...`, the period of a quoted granted ability, CR 205.1b).
/// The pronoun agrees with the changed objects; the retained type must be a
/// prior card type distinct from the new ones. The sentence terminator stays
/// with the enclosing sequence.
pub(in crate::oracle::canonical) fn recognize_type_retention_amendment(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<&mut super::LocalEffectNode>,
) -> MachineResult<()> {
    cursor.enter("effect.become-types.retention-amendment");
    let change = last_type_change(antecedent).ok_or_else(|| {
        cursor.fail("a type change in the preceding sentence to retain types for")
    })?;
    cursor.require(
        change.names_types() && !change.retention.retains_prior_types(),
        "a type change that names new types and no other retention clause",
    )?;
    let plural = word_at(cursor, 0, "they're") || word_at(cursor, 0, "they’re");
    cursor.enter("effect.become-types.retention-amendment.agreement");
    cursor.require(
        plural == super::permanent_operand_is_plural(&change.objects),
        "a pronoun agreeing in number with the changed objects",
    )?;
    let pronoun = cursor.peek().expect("a contracted pronoun").to_string();
    cursor.consume(&pronoun)?;
    change.retention = if plural {
        recognize_plural_still_type(cursor, &change.card_types)?
    } else {
        recognize_still_type(cursor, &change.card_types)?
    };
    Ok(())
}

/// `still TYPEs` after a plural subject: the plural retained card type.
fn recognize_plural_still_type(
    cursor: &mut OracleTokenCursor<'_>,
    card_types: &[ObjectCardType],
) -> MachineResult<TypeChangeRetention> {
    cursor.enter("effect.become-types.still");
    cursor.consume("still")?;
    cursor.enter("effect.become-types.still.plural-type");
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a retained plural permanent card type"))?;
    let retained = lookup_permanent_card_type(raw, AcceptedForms::SingularOrPlural)
        .filter(|_| lookup_permanent_card_type(raw, AcceptedForms::SingularOnly).is_none())
        .ok_or_else(|| cursor.fail("a retained plural permanent card type"))?;
    cursor.require(
        !card_types.contains(&retained),
        "a retained type distinct from the newly assigned card types",
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    Ok(TypeChangeRetention::StillA(retained))
}

/// `still a TYPE`: the retained type is a prior card type, so it must differ
/// from the newly assigned ones (otherwise the clause carries no meaning).
fn recognize_still_type(
    cursor: &mut OracleTokenCursor<'_>,
    card_types: &[ObjectCardType],
) -> MachineResult<TypeChangeRetention> {
    cursor.enter("effect.become-types.still");
    cursor.consume("still")?;
    cursor.enter("effect.become-types.still.article");
    let article = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("`a` or `an` before the retained type"))?;
    cursor.require(matches!(article.as_str(), "a" | "an"), "`a` or `an`")?;
    cursor.consume(&article)?;
    cursor.enter("effect.become-types.still.type");
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a retained permanent card type"))?;
    let retained = lookup_permanent_card_type(raw, AcceptedForms::SingularOnly)
        .ok_or_else(|| cursor.fail("a retained permanent card type"))?;
    cursor.require(
        !card_types.contains(&retained),
        "a retained type distinct from the newly assigned card types",
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    Ok(TypeChangeRetention::StillA(retained))
}

/// One `with` item: `all creature types`, `base power and toughness N/N`,
/// or a maximal list of keywords and quoted abilities.
fn recognize_become_characteristic_item(
    cursor: &mut OracleTokenCursor<'_>,
    characteristics: &mut BecomeCharacteristicsNode,
) -> MachineResult<()> {
    cursor.enter("effect.become-types.characteristic");
    if word_at(cursor, 0, "all") {
        cursor.require(
            !characteristics.all_creature_types,
            "one `all creature types` characteristic",
        )?;
        cursor.consume_phrase(&["all", "creature", "types"])?;
        characteristics.all_creature_types = true;
    } else if base_stats_length(cursor, 0).is_some() {
        cursor.require(
            characteristics.base_power_toughness.is_none(),
            "one base power/toughness characteristic",
        )?;
        characteristics.base_power_toughness = Some(recognize_stated_base_stats(cursor)?);
    } else {
        recognize_granted_into(cursor, characteristics)?;
    }
    Ok(())
}

/// Items are coordinated by `,`, `and` or `, and`. A connector that does not
/// introduce another item belongs to the enclosing effect sequence.
fn recognize_become_characteristics(
    cursor: &mut OracleTokenCursor<'_>,
    characteristics: &mut BecomeCharacteristicsNode,
) -> MachineResult<()> {
    recognize_become_characteristic_item(cursor, characteristics)?;
    loop {
        cursor.enter("effect.become-types.characteristic-separator");
        let mut trial = cursor.clone();
        let mut separated = false;
        if trial.peek() == Some(",") {
            trial.consume(",")?;
            separated = true;
        }
        if word_at(&trial, 0, "and") {
            trial.consume("and")?;
            separated = true;
        }
        if !separated {
            return Ok(());
        }
        let mut extended = characteristics.clone();
        if recognize_become_characteristic_item(&mut trial, &mut extended).is_err() {
            return Ok(());
        }
        *cursor = trial;
        *characteristics = extended;
    }
}

#[cfg(test)]
#[path = "become_types_tests.rs"]
mod tests;
