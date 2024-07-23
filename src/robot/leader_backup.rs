use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::VecDeque;

use crate::robot::order_info::OrderInfo;
use crate::robot::order_waiting::OrderWaiting;

/// Struct to store the leader backup information
/// Allows a new leader to recover the previous leader state
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LeaderBackup {
    pub available_robots: Vec<usize>,
    pub orders_on_queue: VecDeque<OrderInfo>,
    pub robots_orders: HashMap<usize, OrderInfo>,
    pub screens: Vec<usize>,
    pub orders_to_be_sent: Vec<OrderWaiting>,
}

impl LeaderBackup {
    pub fn new(
        available_robots: Vec<usize>,
        screens: Vec<usize>,
        orders_on_queue: VecDeque<OrderInfo>,
        robots_orders: HashMap<usize, OrderInfo>,
        orders_to_be_sent: Vec<OrderWaiting>,
    ) -> Self {
        Self {
            available_robots,
            orders_on_queue,
            robots_orders,
            screens,
            orders_to_be_sent,
        }
    }
}
