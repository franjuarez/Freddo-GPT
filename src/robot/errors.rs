use std::fmt::{self};

/// Error type for robot connection
#[derive(Debug)]
pub enum RobotConnectionError {
    ColudNotConnectToRobot(String),
    NoRobotsAvailableError(),
}

impl fmt::Display for RobotConnectionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RobotConnectionError::ColudNotConnectToRobot(err) => {
                write!(f, "Could not connect to robot: {}", err)
            }
            RobotConnectionError::NoRobotsAvailableError() => {
                write!(f, "No robots available to connect")
            }
        }
    }
}

impl std::error::Error for RobotConnectionError {}
