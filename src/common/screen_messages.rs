use std::{collections::HashMap, error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::common::order::Order;

#[derive(Debug)]
pub enum ScreenMessageError {
    ErrorParsing(String),
}

impl fmt::Display for ScreenMessageError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
impl Error for ScreenMessageError {}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ScreenMessage {
    PrepareNewOrder {
        screen_id: usize,
        order_id: String,
        order: Order,
    },
    TakeMyBackup {
        orders_to_process: Vec<Order>,
        orders_processing: HashMap<String, Order>,
        orders_pending_to_send: Vec<(String, Order)>,
        id_backup: usize,
    },
    RequestRobotLeaderConnection {
        screen_id: usize,
    },
    GiveMeThisScreenOrders {
        my_id: usize,
        death_id: usize,
    },
}

impl ScreenMessage {
    pub fn from_string(msg: &str) -> Result<Self, ScreenMessageError> {
        serde_json::from_str(msg).map_err(|err| ScreenMessageError::ErrorParsing(err.to_string()))
    }

    pub fn to_string(&self) -> Result<String, ScreenMessageError> {
        serde_json::to_string(self).map_err(|err| ScreenMessageError::ErrorParsing(err.to_string()))
    }
}
