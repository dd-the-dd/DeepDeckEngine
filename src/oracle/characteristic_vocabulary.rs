// Phase 1 intentionally introduces the shared vocabulary before migrating its
// six independent consumers. Remove this allowance as Phase 2 adopts it.
#![allow(dead_code)]

use std::{collections::HashMap, sync::OnceLock};

use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle) enum CardType {
    Artifact,
    Battle,
    Creature,
    Enchantment,
    Instant,
    Kindred,
    Land,
    Planeswalker,
    Sorcery,
}

pub(in crate::oracle) const ORACLE_CARD_TYPES: &[CardType] = &[
    CardType::Artifact,
    CardType::Battle,
    CardType::Creature,
    CardType::Enchantment,
    CardType::Instant,
    CardType::Kindred,
    CardType::Land,
    CardType::Planeswalker,
    CardType::Sorcery,
];

impl CardType {
    pub(in crate::oracle) const fn oracle_singular(self) -> &'static str {
        match self {
            Self::Artifact => "artifact",
            Self::Battle => "battle",
            Self::Creature => "creature",
            Self::Enchantment => "enchantment",
            Self::Instant => "instant",
            Self::Kindred => "kindred",
            Self::Land => "land",
            Self::Planeswalker => "planeswalker",
            Self::Sorcery => "sorcery",
        }
    }

    pub(in crate::oracle) const fn oracle_plural(self) -> &'static str {
        match self {
            Self::Artifact => "artifacts",
            Self::Battle => "battles",
            Self::Creature => "creatures",
            Self::Enchantment => "enchantments",
            Self::Instant => "instants",
            Self::Kindred => "kindred",
            Self::Land => "lands",
            Self::Planeswalker => "planeswalkers",
            Self::Sorcery => "sorceries",
        }
    }

    pub(in crate::oracle) const fn ir_name(self) -> &'static str {
        match self {
            Self::Artifact => "Artifact",
            Self::Battle => "Battle",
            Self::Creature => "Creature",
            Self::Enchantment => "Enchantment",
            Self::Instant => "Instant",
            Self::Kindred => "Kindred",
            Self::Land => "Land",
            Self::Planeswalker => "Planeswalker",
            Self::Sorcery => "Sorcery",
        }
    }

    /// CR110.4: an instant/sorcery type forbids battlefield presence even
    /// alongside Creature. Kindred is compatible as an additional type, but
    /// does not by itself make an object a permanent (is_permanent_type).
    pub(in crate::oracle) const fn may_appear_on_permanent(self) -> bool {
        self.is_permanent_type() || matches!(self, Self::Kindred)
    }

    pub(in crate::oracle) const fn is_permanent_type(self) -> bool {
        matches!(
            self,
            Self::Artifact
                | Self::Battle
                | Self::Creature
                | Self::Enchantment
                | Self::Land
                | Self::Planeswalker
        )
    }
}

/// Returns whether one card face's type line contains a battlefield permanent
/// card type. Only the card-type segment before the subtype em dash is read,
/// so a subtype cannot accidentally make an instant, sorcery, or emblem look
/// like a permanent face.
pub(in crate::oracle) fn type_line_has_permanent_card_type(type_line: &str) -> bool {
    let card_types = type_line.split('\u{2014}').next().unwrap_or(type_line);
    card_types
        .split(|character: char| !character.is_alphabetic())
        .filter(|part| !part.is_empty())
        .any(|part| lookup_permanent_card_type(part, AcceptedForms::SingularOnly).is_some())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle) enum CardSupertype {
    Basic,
    Legendary,
}

impl CardSupertype {
    pub(in crate::oracle) const fn oracle_singular(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Legendary => "legendary",
        }
    }

    pub(in crate::oracle) const fn ir_name(self) -> &'static str {
        match self {
            Self::Basic => "Basic",
            Self::Legendary => "Legendary",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle) enum BasicLandSubtype {
    Plains,
    Island,
    Swamp,
    Mountain,
    Forest,
}

/// Creature subtypes currently defined by the `amass TYPE N` keyword action.
///
/// This is deliberately a closed mechanic vocabulary rather than a generic
/// singularization rule. It includes every amass subtype represented by the
/// current Oracle/runtime corpus and rejects arbitrary unknown words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle) enum AmassArmySubtype {
    Goblin,
    Orc,
    Zombie,
}

/// One exact creature subtype from the versioned Comprehensive Rules
/// vocabulary. Keeping the canonical spelling in a value object avoids an
/// enum variant for every rules update while retaining a typed parser result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle) struct CreatureSubtype {
    canonical: &'static str,
}

#[allow(non_upper_case_globals)]
impl CreatureSubtype {
    // Compatibility constants used by existing typed tests and automata. The
    // snapshot validator proves these spellings also exist in the registry.
    pub(in crate::oracle) const Shark: Self = Self::new("Shark");
    pub(in crate::oracle) const Spirit: Self = Self::new("Spirit");
    pub(in crate::oracle) const Warrior: Self = Self::new("Warrior");

    const fn new(canonical: &'static str) -> Self {
        Self { canonical }
    }

    pub(in crate::oracle) const fn oracle_singular(self) -> &'static str {
        self.canonical
    }

    pub(in crate::oracle) const fn ir_name(self) -> &'static str {
        self.canonical
    }
}

const CREATURE_SUBTYPE_SNAPSHOT_JSON: &str =
    include_str!("../../data/oracle-vocabulary/creature-subtypes-cr-205.3m-2026-11-06.json");
const CREATURE_SUBTYPE_SNAPSHOT_SCHEMA: &str = "deepdeck-creature-subtypes/v1";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreatureSubtypeSnapshot {
    schema_version: String,
    source: CreatureSubtypeSnapshotSource,
    canonical: Vec<String>,
    exact_aliases: Vec<CreatureSubtypeAlias>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreatureSubtypeSnapshotSource {
    authority: String,
    url: String,
    effective_date: String,
    rule: String,
    document_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreatureSubtypeAlias {
    surface: String,
    canonical: String,
}

struct CreatureSubtypeVocabulary {
    canonical_by_surface: HashMap<String, CreatureSubtype>,
    aliases_by_surface: HashMap<String, CreatureSubtype>,
    canonical: Vec<CreatureSubtype>,
    source: CreatureSubtypeSnapshotSource,
}

static CREATURE_SUBTYPE_VOCABULARY: OnceLock<CreatureSubtypeVocabulary> = OnceLock::new();

fn creature_subtype_vocabulary() -> &'static CreatureSubtypeVocabulary {
    CREATURE_SUBTYPE_VOCABULARY.get_or_init(|| {
        let snapshot: CreatureSubtypeSnapshot =
            serde_json::from_str(CREATURE_SUBTYPE_SNAPSHOT_JSON)
                .expect("the versioned CR 205.3m creature-subtype snapshot is valid JSON");
        assert_eq!(
            snapshot.schema_version, CREATURE_SUBTYPE_SNAPSHOT_SCHEMA,
            "the creature-subtype snapshot schema is supported"
        );
        assert!(
            !snapshot.source.authority.is_empty()
                && !snapshot.source.url.is_empty()
                && !snapshot.source.effective_date.is_empty()
                && snapshot.source.rule == "205.3m"
                && snapshot.source.document_sha256.len() == 64,
            "the creature-subtype snapshot has traceable source metadata"
        );

        let mut canonical_by_surface = HashMap::new();
        let mut canonical = Vec::with_capacity(snapshot.canonical.len());
        for spelling in snapshot.canonical {
            assert!(
                !spelling.is_empty()
                    && spelling.trim() == spelling
                    && matches!(spelling.split_whitespace().count(), 1 | 2),
                "creature subtype {spelling:?} is one or two exact words"
            );
            let leaked = Box::leak(spelling.into_boxed_str());
            let subtype = CreatureSubtype::new(leaked);
            assert!(
                canonical_by_surface
                    .insert(leaked.to_ascii_lowercase(), subtype)
                    .is_none(),
                "creature subtype {leaked:?} is unique"
            );
            canonical.push(subtype);
        }

        let mut aliases_by_surface = HashMap::new();
        for alias in snapshot.exact_aliases {
            assert!(
                !alias.surface.is_empty() && alias.surface.trim() == alias.surface,
                "creature subtype aliases are exact nonempty surfaces"
            );
            let canonical_key = alias.canonical.to_ascii_lowercase();
            let subtype = *canonical_by_surface
                .get(&canonical_key)
                .unwrap_or_else(|| panic!("alias target {:?} is canonical", alias.canonical));
            let alias_key = alias.surface.to_ascii_lowercase();
            assert!(
                !canonical_by_surface.contains_key(&alias_key)
                    && aliases_by_surface.insert(alias_key, subtype).is_none(),
                "creature subtype alias {:?} is unique and not canonical",
                alias.surface
            );
        }

        CreatureSubtypeVocabulary {
            canonical_by_surface,
            aliases_by_surface,
            canonical,
            source: snapshot.source,
        }
    })
}

pub(in crate::oracle) fn lookup_creature_subtype(
    raw: &str,
    forms: AcceptedForms,
) -> Option<CreatureSubtype> {
    if raw.is_empty() || raw.trim() != raw {
        return None;
    }
    let key = raw.to_ascii_lowercase();
    creature_subtype_vocabulary()
        .canonical_by_surface
        .get(&key)
        .copied()
        .or_else(|| {
            (forms == AcceptedForms::SingularOrPlural)
                .then(|| {
                    creature_subtype_vocabulary()
                        .aliases_by_surface
                        .get(&key)
                        .copied()
                })
                .flatten()
        })
}

/// Matches the longest exact one- or two-token creature subtype. Oracle token
/// grammars use this instead of joining arbitrary capitalized words.
pub(in crate::oracle) fn lookup_creature_subtype_tokens(
    first: &str,
    second: Option<&str>,
    forms: AcceptedForms,
) -> Option<(CreatureSubtype, usize)> {
    if let Some(second) = second {
        let phrase = format!("{first} {second}");
        if let Some(subtype) = lookup_creature_subtype(&phrase, forms) {
            return Some((subtype, 2));
        }
    }
    lookup_creature_subtype(first, forms).map(|subtype| (subtype, 1))
}

impl AmassArmySubtype {
    pub(in crate::oracle) const fn oracle_singular(self) -> &'static str {
        match self {
            Self::Goblin => "Goblin",
            Self::Orc => "Orc",
            Self::Zombie => "Zombie",
        }
    }

    pub(in crate::oracle) const fn oracle_plural(self) -> &'static str {
        match self {
            Self::Goblin => "Goblins",
            Self::Orc => "Orcs",
            Self::Zombie => "Zombies",
        }
    }

    pub(in crate::oracle) const fn ir_name(self) -> &'static str {
        self.oracle_singular()
    }
}

const AMASS_ARMY_SUBTYPES: &[AmassArmySubtype] = &[
    AmassArmySubtype::Goblin,
    AmassArmySubtype::Orc,
    AmassArmySubtype::Zombie,
];

pub(in crate::oracle) fn lookup_amass_army_subtype(
    raw: &str,
    forms: AcceptedForms,
) -> Option<AmassArmySubtype> {
    AMASS_ARMY_SUBTYPES.iter().copied().find(|subtype| {
        raw.eq_ignore_ascii_case(subtype.oracle_singular())
            || (forms == AcceptedForms::SingularOrPlural
                && raw.eq_ignore_ascii_case(subtype.oracle_plural()))
    })
}

impl BasicLandSubtype {
    pub(in crate::oracle) const fn oracle_singular(self) -> &'static str {
        match self {
            Self::Plains => "plains",
            Self::Island => "island",
            Self::Swamp => "swamp",
            Self::Mountain => "mountain",
            Self::Forest => "forest",
        }
    }

    pub(in crate::oracle) const fn oracle_plural(self) -> &'static str {
        match self {
            Self::Plains => "plains",
            Self::Island => "islands",
            Self::Swamp => "swamps",
            Self::Mountain => "mountains",
            Self::Forest => "forests",
        }
    }

    pub(in crate::oracle) const fn ir_name(self) -> &'static str {
        match self {
            Self::Plains => "Plains",
            Self::Island => "Island",
            Self::Swamp => "Swamp",
            Self::Mountain => "Mountain",
            Self::Forest => "Forest",
        }
    }
}

/// One of Magic's five card colors.
///
/// This vocabulary deliberately excludes `colorless`: colorless is the
/// absence of a color, not a sixth `CardColor`. Mana symbols remain owned by
/// the mana grammar and are not aliases for these characteristic words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle) enum CardColor {
    White,
    Blue,
    Black,
    Red,
    Green,
}

impl CardColor {
    pub(in crate::oracle) const fn oracle_name(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Blue => "blue",
            Self::Black => "black",
            Self::Red => "red",
            Self::Green => "green",
        }
    }

    pub(in crate::oracle) const fn ir_name(self) -> &'static str {
        match self {
            Self::White => "White",
            Self::Blue => "Blue",
            Self::Black => "Black",
            Self::Red => "Red",
            Self::Green => "Green",
        }
    }

    /// Compact color code used by primitive IR fields that model a set of
    /// colors rather than a mana symbol or a characteristic name.
    pub(in crate::oracle) const fn ir_symbol(self) -> &'static str {
        match self {
            Self::White => "W",
            Self::Blue => "U",
            Self::Black => "B",
            Self::Red => "R",
            Self::Green => "G",
        }
    }
}

const CARD_COLORS: &[CardColor] = &[
    CardColor::White,
    CardColor::Blue,
    CardColor::Black,
    CardColor::Red,
    CardColor::Green,
];

pub(in crate::oracle) fn lookup_card_color(raw: &str) -> Option<CardColor> {
    CARD_COLORS
        .iter()
        .copied()
        .find(|color| raw.eq_ignore_ascii_case(color.oracle_name()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle) enum CardCategory {
    Permanent,
}

impl CardCategory {
    pub(in crate::oracle) const fn oracle_singular(self) -> &'static str {
        match self {
            Self::Permanent => "permanent",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle) enum ObjectDomain {
    Card,
    Permanent,
    Spell,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle) enum ObjectCharacteristic {
    Commander,
    Token,
}

impl ObjectCharacteristic {
    pub(in crate::oracle) const fn oracle_singular(self) -> &'static str {
        match self {
            Self::Commander => "commander",
            Self::Token => "token",
        }
    }

    pub(in crate::oracle) const fn oracle_plural(self) -> &'static str {
        match self {
            Self::Commander => "commanders",
            Self::Token => "tokens",
        }
    }
}

impl ObjectDomain {
    pub(in crate::oracle) const fn oracle_singular(self) -> &'static str {
        match self {
            Self::Card => "card",
            Self::Permanent => "permanent",
            Self::Spell => "spell",
        }
    }

    pub(in crate::oracle) const fn oracle_plural(self) -> &'static str {
        match self {
            Self::Card => "cards",
            Self::Permanent => "permanents",
            Self::Spell => "spells",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle) enum PlayerRole {
    Any,
    Opponent,
}

impl PlayerRole {
    pub(in crate::oracle) const fn oracle_singular(self) -> &'static str {
        match self {
            Self::Any => "player",
            Self::Opponent => "opponent",
        }
    }
}

const PLAYER_ROLES: &[PlayerRole] = &[PlayerRole::Any, PlayerRole::Opponent];

pub(in crate::oracle) fn lookup_player_role(raw: &str) -> Option<PlayerRole> {
    PLAYER_ROLES
        .iter()
        .copied()
        .find(|role| raw.eq_ignore_ascii_case(role.oracle_singular()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle) enum LexemeRole {
    CardType,
    Supertype,
    BasicLandSubtype,
    CardCategory,
    ObjectDomain,
    ObjectCharacteristic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle) enum AcceptedForms {
    SingularOnly,
    SingularOrPlural,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle) enum CharacteristicLexeme {
    CardType(CardType),
    Supertype(CardSupertype),
    BasicLandSubtype(BasicLandSubtype),
    CardCategory(CardCategory),
    ObjectDomain(ObjectDomain),
    ObjectCharacteristic(ObjectCharacteristic),
}

impl CharacteristicLexeme {
    const fn role(self) -> LexemeRole {
        match self {
            Self::CardType(_) => LexemeRole::CardType,
            Self::Supertype(_) => LexemeRole::Supertype,
            Self::BasicLandSubtype(_) => LexemeRole::BasicLandSubtype,
            Self::CardCategory(_) => LexemeRole::CardCategory,
            Self::ObjectDomain(_) => LexemeRole::ObjectDomain,
            Self::ObjectCharacteristic(_) => LexemeRole::ObjectCharacteristic,
        }
    }

    const fn singular(self) -> &'static str {
        match self {
            Self::CardType(value) => value.oracle_singular(),
            Self::Supertype(value) => value.oracle_singular(),
            Self::BasicLandSubtype(value) => value.oracle_singular(),
            Self::CardCategory(value) => value.oracle_singular(),
            Self::ObjectDomain(value) => value.oracle_singular(),
            Self::ObjectCharacteristic(value) => value.oracle_singular(),
        }
    }

    const fn plural(self) -> Option<&'static str> {
        match self {
            Self::CardType(value) => Some(value.oracle_plural()),
            Self::BasicLandSubtype(value) => Some(value.oracle_plural()),
            Self::ObjectDomain(value) => Some(value.oracle_plural()),
            Self::ObjectCharacteristic(value) => Some(value.oracle_plural()),
            Self::Supertype(_) | Self::CardCategory(_) => None,
        }
    }
}

const CHARACTERISTIC_LEXEMES: &[CharacteristicLexeme] = &[
    CharacteristicLexeme::CardType(CardType::Artifact),
    CharacteristicLexeme::CardType(CardType::Battle),
    CharacteristicLexeme::CardType(CardType::Creature),
    CharacteristicLexeme::CardType(CardType::Enchantment),
    CharacteristicLexeme::CardType(CardType::Instant),
    CharacteristicLexeme::CardType(CardType::Kindred),
    CharacteristicLexeme::CardType(CardType::Land),
    CharacteristicLexeme::CardType(CardType::Planeswalker),
    CharacteristicLexeme::CardType(CardType::Sorcery),
    CharacteristicLexeme::Supertype(CardSupertype::Basic),
    CharacteristicLexeme::Supertype(CardSupertype::Legendary),
    CharacteristicLexeme::BasicLandSubtype(BasicLandSubtype::Plains),
    CharacteristicLexeme::BasicLandSubtype(BasicLandSubtype::Island),
    CharacteristicLexeme::BasicLandSubtype(BasicLandSubtype::Swamp),
    CharacteristicLexeme::BasicLandSubtype(BasicLandSubtype::Mountain),
    CharacteristicLexeme::BasicLandSubtype(BasicLandSubtype::Forest),
    CharacteristicLexeme::CardCategory(CardCategory::Permanent),
    CharacteristicLexeme::ObjectDomain(ObjectDomain::Card),
    CharacteristicLexeme::ObjectDomain(ObjectDomain::Permanent),
    CharacteristicLexeme::ObjectDomain(ObjectDomain::Spell),
    CharacteristicLexeme::ObjectCharacteristic(ObjectCharacteristic::Commander),
    CharacteristicLexeme::ObjectCharacteristic(ObjectCharacteristic::Token),
];

pub(in crate::oracle) fn lookup_characteristic(
    raw: &str,
    role: LexemeRole,
    forms: AcceptedForms,
) -> Option<CharacteristicLexeme> {
    CHARACTERISTIC_LEXEMES.iter().copied().find(|lexeme| {
        if lexeme.role() != role {
            return false;
        }
        if raw.eq_ignore_ascii_case(lexeme.singular()) {
            return true;
        }
        forms == AcceptedForms::SingularOrPlural
            && lexeme
                .plural()
                .is_some_and(|plural| raw.eq_ignore_ascii_case(plural))
    })
}

pub(in crate::oracle) fn lookup_card_type(raw: &str, forms: AcceptedForms) -> Option<CardType> {
    match lookup_characteristic(raw, LexemeRole::CardType, forms) {
        Some(CharacteristicLexeme::CardType(value)) => Some(value),
        _ => None,
    }
}

pub(in crate::oracle) fn lookup_permanent_card_type(
    raw: &str,
    forms: AcceptedForms,
) -> Option<CardType> {
    lookup_card_type(raw, forms).filter(|card_type| card_type.is_permanent_type())
}

pub(in crate::oracle) fn lookup_supertype(
    raw: &str,
    forms: AcceptedForms,
) -> Option<CardSupertype> {
    match lookup_characteristic(raw, LexemeRole::Supertype, forms) {
        Some(CharacteristicLexeme::Supertype(value)) => Some(value),
        _ => None,
    }
}

pub(in crate::oracle) fn lookup_basic_land_subtype(
    raw: &str,
    forms: AcceptedForms,
) -> Option<BasicLandSubtype> {
    match lookup_characteristic(raw, LexemeRole::BasicLandSubtype, forms) {
        Some(CharacteristicLexeme::BasicLandSubtype(value)) => Some(value),
        _ => None,
    }
}

pub(in crate::oracle) fn lookup_card_category(
    raw: &str,
    forms: AcceptedForms,
) -> Option<CardCategory> {
    match lookup_characteristic(raw, LexemeRole::CardCategory, forms) {
        Some(CharacteristicLexeme::CardCategory(value)) => Some(value),
        _ => None,
    }
}

pub(in crate::oracle) fn lookup_object_domain(
    raw: &str,
    forms: AcceptedForms,
) -> Option<ObjectDomain> {
    match lookup_characteristic(raw, LexemeRole::ObjectDomain, forms) {
        Some(CharacteristicLexeme::ObjectDomain(value)) => Some(value),
        _ => None,
    }
}

pub(in crate::oracle) fn lookup_object_characteristic(
    raw: &str,
    forms: AcceptedForms,
) -> Option<ObjectCharacteristic> {
    match lookup_characteristic(raw, LexemeRole::ObjectCharacteristic, forms) {
        Some(CharacteristicLexeme::ObjectCharacteristic(value)) => Some(value),
        _ => None,
    }
}

/// Resolves the lexical head shared by object-target and object-query
/// grammars. Supertypes remain singular adjectives; card types and object
/// domains retain their currently supported singular and plural noun forms.
pub(in crate::oracle) fn lookup_object_selector_lexeme(raw: &str) -> Option<CharacteristicLexeme> {
    lookup_characteristic(raw, LexemeRole::Supertype, AcceptedForms::SingularOnly)
        .or_else(|| {
            lookup_characteristic(raw, LexemeRole::CardType, AcceptedForms::SingularOrPlural)
        })
        .or_else(|| {
            lookup_characteristic(
                raw,
                LexemeRole::ObjectDomain,
                AcceptedForms::SingularOrPlural,
            )
        })
        .or_else(|| {
            lookup_characteristic(
                raw,
                LexemeRole::ObjectCharacteristic,
                AcceptedForms::SingularOrPlural,
            )
        })
}

#[cfg(test)]
#[path = "characteristic_vocabulary_tests.rs"]
mod tests;

const NONCREATURE_SUBTYPE_SNAPSHOT_JSON: &str =
    include_str!("../../data/oracle-vocabulary/noncreature-subtypes-cr-205.3-2026-11-06.json");

static NONCREATURE_SUBTYPES: OnceLock<HashMap<String, &'static str>> = OnceLock::new();

/// Singular canonical noncreature subtypes from the versioned CR 205.3
/// snapshot (Equipment, Aura, Vehicle, Saga...). Plural aliases are not
/// admitted: callers that need a collection noun use their own plural grammar.
pub(in crate::oracle) fn lookup_noncreature_subtype(raw: &str) -> Option<&'static str> {
    if raw.is_empty() || raw.trim() != raw {
        return None;
    }
    NONCREATURE_SUBTYPES
        .get_or_init(|| {
            let snapshot: serde_json::Value =
                serde_json::from_str(NONCREATURE_SUBTYPE_SNAPSHOT_JSON)
                    .expect("the versioned CR 205.3 noncreature-subtype snapshot is valid JSON");
            assert_eq!(
                snapshot["schemaVersion"], "deepdeck-noncreature-subtypes/v1",
                "the noncreature-subtype snapshot schema is supported"
            );
            snapshot["canonical"]
                .as_array()
                .expect("canonical noncreature subtypes")
                .iter()
                .map(|entry| {
                    let canonical: &'static str = Box::leak(
                        entry["canonical"]
                            .as_str()
                            .expect("canonical noncreature subtype spelling")
                            .to_owned()
                            .into_boxed_str(),
                    );
                    (canonical.to_ascii_lowercase(), canonical)
                })
                .collect()
        })
        .get(&raw.to_ascii_lowercase())
        .copied()
}

/// Noncreature subtype noun at a selector-atom position, with its plural
/// form (`Treasure`/`Treasures`, `Vehicle`/`Vehicles`). Oracle prints subtypes
/// capitalized, so a lowercase word (`case`, `will`, `map`) is never a subtype
/// noun here. Returns the IR subtype name read by the engine's
/// `subtypeContains` predicate; the implied card type is not added because
/// CR 205.3 only constrains which types may carry a subtype. The possessive
/// land subtype `Urza's` (only meaningful inside `Urza's Mine`) is excluded.
pub(in crate::oracle) fn lookup_noncreature_subtype_forms(
    raw: &str,
    forms: AcceptedForms,
) -> Option<&'static str> {
    if !raw.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return None;
    }
    let found = lookup_noncreature_subtype(raw).or_else(|| {
        if forms != AcceptedForms::SingularOrPlural {
            return None;
        }
        let stem = raw.strip_suffix('s').or_else(|| raw.strip_suffix('S'))?;
        lookup_noncreature_subtype(stem)
    })?;
    (!found.contains('\u{2019}')).then_some(found)
}

/// One `[type]walk` keyword word (CR 702.14a) whose type is a land subtype
/// from the versioned CR 205.3 snapshot: the canonical subtype and the
/// lowercase keyword name the engine reads (`Swamp` / `swampwalk`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle) struct LandwalkWord {
    pub subtype: &'static str,
    pub keyword: &'static str,
}

static LANDWALK_WORDS: OnceLock<HashMap<String, LandwalkWord>> = OnceLock::new();

/// Resolves one Oracle word of the form `[land subtype]walk`. The land
/// subtype vocabulary is shared data (family `land` of the CR 205.3
/// snapshot); no per-subtype keyword is listed in code. Subtypes that are
/// not one alphabetic word (`Urza's`, `Power-Plant`) cannot prefix `walk`.
pub(in crate::oracle) fn lookup_landwalk_word(raw: &str) -> Option<LandwalkWord> {
    LANDWALK_WORDS
        .get_or_init(|| {
            let snapshot: serde_json::Value =
                serde_json::from_str(NONCREATURE_SUBTYPE_SNAPSHOT_JSON)
                    .expect("the versioned CR 205.3 noncreature-subtype snapshot is valid JSON");
            snapshot["canonical"]
                .as_array()
                .expect("canonical noncreature subtypes")
                .iter()
                .filter(|entry| entry["family"] == "land")
                .filter_map(|entry| entry["irName"].as_str())
                .filter(|name| name.chars().all(|c| c.is_ascii_alphabetic()))
                .map(|name| {
                    let subtype: &'static str = Box::leak(name.to_owned().into_boxed_str());
                    let keyword: &'static str =
                        Box::leak(format!("{}walk", name.to_ascii_lowercase()).into_boxed_str());
                    (keyword.to_owned(), LandwalkWord { subtype, keyword })
                })
                .collect()
        })
        .get(&raw.to_ascii_lowercase())
        .copied()
}
