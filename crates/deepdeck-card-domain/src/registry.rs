use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

use crate::{AcceptedForms, NominalError, SubtypeFamilies, SubtypeFamily, SubtypeId};

const CREATURES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../data/oracle-vocabulary/creature-subtypes-cr-205.3m-2026-11-06.json"
));
const NONCREATURES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../data/oracle-vocabulary/noncreature-subtypes-cr-205.3-2026-11-06.json"
));
const ORTHOGRAPHIC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../data/oracle-vocabulary/subtype-orthographic-aliases-2026-11-15.json"
));

struct Lookup {
    folded: String,
    id: SubtypeId,
    query_allowed: bool,
}

struct Registry {
    surfaces: Vec<Lookup>,
    creatures: Vec<SubtypeId>,
}

static REGISTRY: OnceLock<Result<Registry, NominalError>> = OnceLock::new();

fn invalid(message: impl Into<String>) -> NominalError {
    NominalError::RegistryData(message.into())
}

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, NominalError> {
    let result = value[field]
        .as_str()
        .ok_or_else(|| invalid(format!("missing string {field}")))?;
    if result.is_empty()
        || result.trim() != result
        || result.chars().any(|ch| matches!(ch, '\n' | '\r' | '\t'))
    {
        return Err(invalid(format!("invalid registry text {field}")));
    }
    Ok(result)
}

fn array<'a>(value: &'a Value, field: &str) -> Result<&'a [Value], NominalError> {
    value[field]
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| invalid(format!("missing array {field}")))
}

fn parse_data(raw: &str, schema: &str) -> Result<Value, NominalError> {
    let value: Value = serde_json::from_str(raw).map_err(|error| invalid(error.to_string()))?;
    if text(&value, "schemaVersion")? != schema {
        return Err(invalid("unsupported registry schema"));
    }
    Ok(value)
}

fn family(raw: &str) -> Result<SubtypeFamily, NominalError> {
    match raw {
        "creature" => Ok(SubtypeFamily::Creature),
        "artifact" => Ok(SubtypeFamily::Artifact),
        "enchantment" => Ok(SubtypeFamily::Enchantment),
        "land" => Ok(SubtypeFamily::Land),
        "planeswalker" => Ok(SubtypeFamily::Planeswalker),
        "spell" => Ok(SubtypeFamily::Spell),
        "battle" => Ok(SubtypeFamily::Battle),
        "plane" => Ok(SubtypeFamily::Plane),
        "dungeon" => Ok(SubtypeFamily::Dungeon),
        _ => Err(invalid(format!("unknown subtype family {raw}"))),
    }
}

fn add_identity(
    values: &mut BTreeMap<String, (String, String, SubtypeFamilies)>,
    name: &str,
    ir_name: &str,
    families: SubtypeFamilies,
) -> Result<(), NominalError> {
    if name.is_empty() || name.trim() != name || ir_name.is_empty() || families.is_empty() {
        return Err(invalid("invalid canonical subtype identity"));
    }
    let key = name.to_ascii_lowercase();
    if let Some((canonical, existing_ir, existing_families)) = values.get_mut(&key) {
        if canonical != name || existing_ir != ir_name {
            return Err(invalid(format!(
                "conflicting identity/IR spelling for {name}"
            )));
        }
        if !existing_families.intersection(families).is_empty() {
            return Err(invalid(format!(
                "duplicate canonical subtype in the same family: {name}"
            )));
        }
        *existing_families = existing_families.union(families);
    } else {
        values.insert(key, (name.to_owned(), ir_name.to_owned(), families));
    }
    Ok(())
}

fn add_surface(
    values: &mut BTreeMap<String, (SubtypeId, bool)>,
    surface: &str,
    id: SubtypeId,
    query_allowed: bool,
) -> Result<(), NominalError> {
    if surface.is_empty() || surface.trim() != surface {
        return Err(invalid("invalid alias surface"));
    }
    let key = surface.to_ascii_lowercase();
    if let Some((prior, allowed)) = values.get_mut(&key) {
        if *prior != id {
            return Err(invalid(format!("ambiguous exact alias {surface}")));
        }
        *allowed |= query_allowed;
    } else {
        values.insert(key, (id, query_allowed));
    }
    Ok(())
}

fn load_registry() -> Result<Registry, NominalError> {
    let creatures = parse_data(CREATURES, "deepdeck-creature-subtypes/v1")?;
    let noncreatures = parse_data(NONCREATURES, "deepdeck-noncreature-subtypes/v1")?;
    let orthographic = parse_data(ORTHOGRAPHIC, "deepdeck-subtype-orthographic-aliases/v1")?;
    let mut canonical = BTreeMap::new();
    for value in array(&creatures, "canonical")? {
        let name = value
            .as_str()
            .ok_or_else(|| invalid("creature canonical must be a string"))?;
        add_identity(
            &mut canonical,
            name,
            name,
            SubtypeFamilies::single(SubtypeFamily::Creature),
        )?;
    }
    for value in array(&noncreatures, "canonical")? {
        let mut families = SubtypeFamilies::single(family(text(value, "family")?)?);
        for other in array(value, "otherFamilies")? {
            let other = other
                .as_str()
                .ok_or_else(|| invalid("other family must be a string"))?;
            families = families.union(SubtypeFamilies::single(family(other)?));
        }
        add_identity(
            &mut canonical,
            text(value, "canonical")?,
            text(value, "irName")?,
            families,
        )?;
    }
    let ids = canonical
        .into_iter()
        .map(|(key, (name, ir_name, families))| {
            let value = SubtypeId {
                canonical: Box::leak(name.into_boxed_str()),
                ir_name: Box::leak(ir_name.into_boxed_str()),
                families,
            };
            (key, value)
        })
        .collect::<BTreeMap<_, _>>();
    let mut surfaces = BTreeMap::new();
    for id in ids.values().copied() {
        add_surface(&mut surfaces, id.canonical_name(), id, true)?;
        add_surface(&mut surfaces, id.ir_name(), id, true)?;
    }
    for (source, all_plural) in [(&creatures, true), (&noncreatures, false)] {
        for alias in array(source, "exactAliases")? {
            let target = text(alias, "canonical")?.to_ascii_lowercase();
            let id = *ids
                .get(&target)
                .ok_or_else(|| invalid("alias target is not canonical"))?;
            let plural = if all_plural {
                true
            } else {
                alias["plural"]
                    .as_bool()
                    .ok_or_else(|| invalid("alias must declare plural role"))?
            };
            add_surface(&mut surfaces, text(alias, "surface")?, id, !plural)?;
        }
    }
    for alias in array(&orthographic, "aliases")? {
        let surface = text(alias, "surface")?;
        let target = text(alias, "canonical")?.to_ascii_lowercase();
        let id = *ids
            .get(&target)
            .ok_or_else(|| invalid("orthographic target is not canonical"))?;
        let witness = &alias["witness"];
        for field in ["oracleId", "printingId", "name", "typeLine"] {
            text(witness, field)?;
        }
        if !text(witness, "typeLine")?
            .split_whitespace()
            .any(|word| word == surface)
        {
            return Err(invalid(
                "orthographic alias lacks its exact printed-word witness",
            ));
        }
        add_surface(&mut surfaces, surface, id, true)?;
    }
    let creatures = ids
        .values()
        .copied()
        .filter(|id| id.families().contains(SubtypeFamily::Creature))
        .collect();
    let surfaces = surfaces
        .into_iter()
        .map(|(folded, (id, query_allowed))| Lookup {
            folded,
            id,
            query_allowed,
        })
        .collect();
    Ok(Registry {
        surfaces,
        creatures,
    })
}

fn registry() -> Result<&'static Registry, NominalError> {
    REGISTRY
        .get_or_init(load_registry)
        .as_ref()
        .map_err(Clone::clone)
}

fn lookup(raw: &str) -> Result<Option<&'static Lookup>, NominalError> {
    let registry = registry()?;
    let index = registry.surfaces.binary_search_by(|candidate| {
        candidate
            .folded
            .bytes()
            .cmp(raw.bytes().map(|byte| byte.to_ascii_lowercase()))
    });
    Ok(index.ok().map(|index| &registry.surfaces[index]))
}

fn validate_surface(raw: &str) -> Result<(), NominalError> {
    if raw.is_empty() {
        return Err(NominalError::EmptySurface);
    }
    if raw.trim() != raw || !raw.split_whitespace().eq(raw.split(' ')) {
        return Err(NominalError::NonCanonicalWhitespace(raw.to_owned()));
    }
    Ok(())
}

/// Canonical primitive/query input: no plural surface is guessed or accepted.
/// Explicit witnessed orthographic aliases preserve the same nominal identity.
pub fn resolve_query(raw: &str) -> Result<SubtypeId, NominalError> {
    validate_surface(raw)?;
    match lookup(raw)? {
        Some(value) if value.query_allowed => Ok(value.id),
        Some(_) => Err(NominalError::PluralQueryNotAllowed(raw.to_owned())),
        None => Err(NominalError::UnknownQuery(raw.to_owned())),
    }
}

/// Surface admission for a grammar child; its caller still owns the domain,
/// cardinality, source availability, complete envelope and bindings.
pub fn resolve_surface(raw: &str, forms: AcceptedForms) -> Result<SubtypeId, NominalError> {
    validate_surface(raw)?;
    match lookup(raw)? {
        Some(value) if value.query_allowed || forms == AcceptedForms::SingularOrPlural => {
            Ok(value.id)
        }
        _ => Err(NominalError::UnknownSurface(raw.to_owned())),
    }
}

/// Full stable canonical list, independent of cards observed in a game world.
pub fn all_creature_subtypes() -> Result<&'static [SubtypeId], NominalError> {
    Ok(&registry()?.creatures)
}

pub fn resolve_creature_choice(raw: &str) -> Result<SubtypeId, NominalError> {
    let id = resolve_query(raw)?;
    if !id.families().contains(SubtypeFamily::Creature) {
        return Err(NominalError::WrongChoiceFamily {
            surface: raw.to_owned(),
            families: id.families(),
        });
    }
    Ok(id)
}

/// Internal lexical segmentation probe. None is only a bounded prefix mismatch;
/// runtime queries must use the explicit-error resolve_query API.
pub(crate) fn query_at(raw: &str) -> Result<Option<SubtypeId>, NominalError> {
    Ok(lookup(raw)?
        .filter(|value| value.query_allowed)
        .map(|value| value.id))
}
