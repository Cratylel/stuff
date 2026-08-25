//! Stuff core: typed physical-storage and inventory rules plus an async facade.
//!
//! `StuffService::connect` is the production entry point. It runs embedded
//! migrations and uses normalized PostgreSQL tables plus coarse
//! `LISTEN/NOTIFY` revision events. `StuffService::demo` is DB-free and is
//! intended for UI previews and pure tests.
pub mod domain;
pub mod drawer;
pub mod errors;
pub mod inventory;
pub mod occupancy;
pub mod organizer;
mod persistence;
pub mod rack;
pub mod service;

pub use domain::*;
pub use drawer::{
    DRAWER_CABINET_KIND, DrawerCabinetInstance, DrawerCabinetTemplate, DrawerTemplateDrawer,
};
pub use errors::{AmbiguousCandidate, Result, StuffError};
pub use inventory::{Item, SearchResults, StockLocation, StockLot};
pub use occupancy::{FillEstimate, LocationRef, Occupancy, OccupancyState, Reservation};
pub use organizer::{
    CellId, Compartment, DividerAxis, DividerEdge, DividerKind, OrganizerInstance,
    OrganizerTemplate, PrimitiveCell, grid_template,
};
pub use rack::{RackInstance, RackShelfTemplate, RackTemplate};
pub use service::{
    AddOrCreateItemResult, AddOrCreateItemSpec, ChangeEvent, ChangeListener, ChangeTopic,
    CloneDrawerCabinetTemplateSpec, CloneOrganizerSpec, CloneOrganizerTemplateSpec,
    CloneRackTemplateSpec, Contents, ContentsEntry, CreateDrawerCabinetFromTemplateSpec,
    CreateOrganizerFromTemplateSpec, CreateRackFromTemplateSpec, CreateShelfSpec, FreeLocation,
    FreeLocationFilter, FreshOrganizerSpec, ItemSelectorCandidate, ItemSelectorMatch,
    LocationSelectorMatch, MIGRATOR, OrganizerLayoutSummary, SaveOrganizerAsTemplateSpec,
    SaveRackAsTemplateSpec, SelectorMatch, StuffService, TreeNode, demo_snapshot, display_location,
    item_selector_candidates, path_for, resolve_item_selector, resolve_location_selector,
    resolve_selector,
};
