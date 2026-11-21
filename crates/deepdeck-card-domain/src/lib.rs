//! Pure nominal card-domain knowledge shared by Oracle and runtime consumers.
//! No game world, Oracle sentence grammar, actions, or mutable characteristics.

mod identity;
mod printed;
mod registry;

pub use identity::{AcceptedForms, NominalError, SubtypeFamilies, SubtypeFamily, SubtypeId};
pub use printed::{PrintedSubtype, PrintedSubtypeSet, parse_printed_type_line};
pub use registry::{
    all_creature_subtypes, resolve_creature_choice, resolve_query, resolve_surface,
};
