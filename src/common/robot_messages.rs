use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum RobotMessageError {
    ErrorParsing(String),
}

impl fmt::Display for RobotMessageError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
impl Error for RobotMessageError {}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum RobotMessage {
    OrderPrepared { order_id: String },
    OrderAborted { order_id: String, error: String },
}

impl RobotMessage {
    pub fn from_string(msg: &str) -> Result<Self, RobotMessageError> {
        serde_json::from_str(msg).map_err(|err| RobotMessageError::ErrorParsing(err.to_string()))
    }

    pub fn to_string(&self) -> Result<String, RobotMessageError> {
        serde_json::to_string(self).map_err(|err| RobotMessageError::ErrorParsing(err.to_string()))
    }
}
