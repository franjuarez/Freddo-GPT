//! This module contains the robot logic.
//! The robot is the main component of the system, it is responsible for managing the orders and the connections with the other robots.

pub mod connections;
pub mod errors;
pub mod flavor_token;
pub mod leader_backup;
pub mod leader_elector;
pub mod messages;
pub mod order_info;
pub mod order_manager;
pub mod order_preparer;
pub mod order_waiting;
pub mod robot_connection_handler;
pub mod robot_leader;
pub mod token_backup;
pub mod utils;
