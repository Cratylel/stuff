use crate::domain::{NodeId, OrganizerId, ReservationId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum OccupancyState {
    #[default]
    Free,
    Occupied,
    Reserved,
    Blocked,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct FillEstimate(u8);
impl FillEstimate {
    pub const fn new(percent: u8) -> Option<Self> {
        if percent <= 100 {
            Some(Self(percent))
        } else {
            None
        }
    }
    pub const fn percent(self) -> u8 {
        self.0
    }
    pub const EMPTY: Self = Self(0);
    pub const FULL: Self = Self(100);
}
impl TryFrom<u8> for FillEstimate {
    type Error = &'static str;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value).ok_or("fill estimate must be between 0 and 100")
    }
}
impl From<FillEstimate> for u8 {
    fn from(value: FillEstimate) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LocationRef {
    pub node_id: NodeId,
    pub organizer_id: Option<OrganizerId>,
    pub compartment_key: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Occupancy {
    pub location: LocationRef,
    pub state: OccupancyState,
    pub fill: Option<FillEstimate>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reservation {
    pub id: ReservationId,
    pub location: LocationRef,
    pub label: String,
    pub notes: Option<String>,
}

impl Occupancy {
    pub fn is_free(&self) -> bool {
        self.state == OccupancyState::Free
    }
}
