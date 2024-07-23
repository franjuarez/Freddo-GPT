use crate::common::flavor_id::FlavorID;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Struct that represents a Flavor Token
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct FlavorToken {
    id: FlavorID,
    amount: usize,
}

impl FlavorToken {
    pub fn new(id: FlavorID, amount: usize) -> Self {
        Self { id, amount }
    }

    /// Serializes the FlavorToken into a string
    pub fn serialize(&self) -> String {
        format!("{},{},", self.id, self.amount)
    }

    /// Serializes the FlavorToken into a byte vector
    pub fn as_bytes(&self) -> Vec<u8> {
        self.serialize().into_bytes()
    }

    /// Deserializes a string into a FlavorToken
    pub fn decode(instance: String) -> Option<Self> {
        let parts: Vec<&str> = instance.split(',').collect();

        if parts.len() != 4 {
            println!("Error No se pudo deserializar correctamente");
            return None; // Error No se pudo deserializar correctamente
        }

        Some(Self {
            id: FlavorID::from_str(parts[0]).ok()?,
            amount: parts[1].trim_end_matches('\n').parse::<usize>().ok()?,
        })
    }

    /// Serve a certain amount of ice cream
    pub fn serve(&mut self, serve_amount: usize) {
        self.amount -= serve_amount;
    }

    /// Check if the FlavorToken can serve a certain amount of ice cream
    pub fn can_serve(&self, serve_amount: usize) -> bool {
        serve_amount < self.amount
    }

    /// Get the ID of the FlavorToken
    pub fn get_id(self) -> FlavorID {
        self.id
    }

    /// Get the amount of ice cream the FlavorToken has
    pub fn get_amnt(self) -> usize {
        self.amount
    }
}
