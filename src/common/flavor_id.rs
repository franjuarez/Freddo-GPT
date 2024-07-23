use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub enum FlavorID {
    Chocolate,
    Vanilla,
    Strawberry,
    Mint,
    Pistachio,
    DulceDeLeche,
    Lemon,
}

impl FromStr for FlavorID {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Chocolate" => Ok(FlavorID::Chocolate),
            "Vanilla" => Ok(FlavorID::Vanilla),
            "Strawberry" => Ok(FlavorID::Strawberry),
            "Mint" => Ok(FlavorID::Mint),
            _ => Err(()),
        }
    }
}

impl fmt::Display for FlavorID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            FlavorID::Chocolate => write!(f, "Chocolate"),
            FlavorID::Vanilla => write!(f, "Vanilla"),
            FlavorID::Strawberry => write!(f, "Strawberry"),
            FlavorID::Mint => write!(f, "Mint"),
            FlavorID::Pistachio => write!(f, "Pistachio"),
            FlavorID::DulceDeLeche => write!(f, "DulceDeLeche"),
            FlavorID::Lemon => write!(f, "Lemon"),
        }
    }
}
