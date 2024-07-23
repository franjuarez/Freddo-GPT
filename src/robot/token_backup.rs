use crate::common::flavor_id::FlavorID;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]

/// Struct to store the information of a token that needs to be recovered
/// Holds the flavor_id, amount and the robot_id that has started the recovery process
pub struct TokenBackup {
    flavor_id: FlavorID,
    amount: usize,
    start_robot_id: usize,
}

impl TokenBackup {
    pub fn new(flavor_id: FlavorID, amount: usize, start_robot_id: usize) -> TokenBackup {
        TokenBackup {
            flavor_id,
            amount,
            start_robot_id,
        }
    }

    /// Change the amount of the token if the new amount is less than the current amount
    pub fn change_amount_if_necessary(&mut self, new_amount: usize) {
        if new_amount < self.amount {
            self.amount = new_amount;
        }
    }

    /// Gets the amount of the token
    pub fn get_amount(&self) -> usize {
        self.amount
    }

    /// Gets the robot_id that has started the recovery process
    pub fn get_start_robot_id(&self) -> usize {
        self.start_robot_id
    }

    /// Gets the flavor_id of the token
    pub fn get_flavor_id(&self) -> FlavorID {
        self.flavor_id
    }
}
