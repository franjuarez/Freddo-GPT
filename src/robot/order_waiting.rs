use crate::common::flavor_id::FlavorID;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]

/// Struct to store the information of an order that is waiting for a new screen
pub struct OrderWaiting {
    pub order_result: bool,
    pub id: String,
    pub screen_id: usize,
    pub flavor: Option<FlavorID>,
}
