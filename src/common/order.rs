use serde::{Deserialize, Serialize};

use crate::common::flavor_id::FlavorID;

pub const KILO: usize = 1000;
const MEDIO: usize = KILO / 2;
const CUARTO: usize = KILO / 4;

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize, Clone)]
pub enum Order {
    Cucurucho((FlavorID, usize)),
    Cuarto(Vec<(FlavorID, usize)>),
    Medio(Vec<(FlavorID, usize)>),
    Kilo(Vec<(FlavorID, usize)>),
}

impl Order {
    pub fn new_cucurucho(flavor: FlavorID) -> Self {
        Order::Cucurucho((flavor, CUARTO))
    }

    pub fn new_cuarto(flavors: Vec<FlavorID>) -> Result<Self, String> {
        if flavors.len() > 2 {
            Err("A cuarto can have up to 2 flavors.".to_string())
        } else {
            let flavors_needed = [(flavors[0], CUARTO / 2), (flavors[1], CUARTO / 2)].to_vec();
            Ok(Order::Cuarto(flavors_needed))
        }
    }

    pub fn new_medio(flavors: Vec<FlavorID>) -> Result<Self, String> {
        if flavors.len() > 3 {
            Err("A medio can have up to 3 flavors".to_string())
        } else {
            let flavors_needed = [
                (flavors[1], MEDIO / 3_usize),
                (flavors[2], MEDIO / 3_usize),
                (flavors[0], MEDIO / 3_usize),
            ]
            .to_vec();
            Ok(Order::Medio(flavors_needed))
        }
    }

    pub fn new_kilo(flavors: Vec<FlavorID>) -> Result<Self, String> {
        if flavors.len() > 4 {
            Err("A kilo can have up to 4 flavors.".to_string())
        } else {
            let flavors_needed = [
                (flavors[0], CUARTO),
                (flavors[1], CUARTO),
                (flavors[2], CUARTO),
                (flavors[3], CUARTO),
            ]
            .to_vec();
            Ok(Order::Kilo(flavors_needed))
        }
    }

    pub fn get_flavors(&self) -> Vec<(FlavorID, usize)> {
        match self {
            Order::Cucurucho(flavor) => vec![*flavor],
            Order::Cuarto(flavors) => flavors.clone(),
            Order::Medio(flavors) => flavors.clone(),
            Order::Kilo(flavors) => flavors.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_cuarto_failure() {
        let flavors = vec![FlavorID::Chocolate, FlavorID::Vanilla, FlavorID::Chocolate];
        let order = Order::new_cuarto(flavors);
        assert_eq!(order, Err("A cuarto can have up to 2 flavors.".to_string()));
    }

    #[test]
    fn test_new_medio_failure() {
        let flavors = vec![
            FlavorID::Chocolate,
            FlavorID::Mint,
            FlavorID::Strawberry,
            FlavorID::DulceDeLeche,
        ];
        let order = Order::new_medio(flavors);
        assert_eq!(order, Err("A medio can have up to 3 flavors".to_string()));
    }

    #[test]
    fn test_new_kilo_failure() {
        let flavors = vec![
            FlavorID::Chocolate,
            FlavorID::Vanilla,
            FlavorID::Mint,
            FlavorID::DulceDeLeche,
            FlavorID::Lemon,
        ];
        let order = Order::new_kilo(flavors);
        assert_eq!(order, Err("A kilo can have up to 4 flavors.".to_string()));
    }
}
