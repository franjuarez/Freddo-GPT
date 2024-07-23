use crate::common::order::Order;
use serde::{Deserialize, Serialize};

/// Struct to store the information of an order
/// Holds the order and the order id and the screen id
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrderInfo {
    pub order: Order,
    pub order_id: String,
    pub screen_id: usize,
}
