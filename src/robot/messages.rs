use actix::{Addr, Message};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

use crate::common::flavor_id::FlavorID;
use crate::common::order::Order;
use crate::robot::flavor_token::FlavorToken;
use crate::robot::leader_backup::LeaderBackup;
use crate::robot::order_manager::OrderManager;
use crate::robot::robot_connection_handler::RobotConnectionHandler;
use crate::robot::token_backup::TokenBackup;

/// All the messages that can be sent between the Actors

#[derive(Debug)]
pub enum RobotCommandError {
    ErrorParsing(String),
}

impl fmt::Display for RobotCommandError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
impl Error for RobotCommandError {}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum RobotCommand {
    NewRobot,
    NewPreviousRobot,
    GetLeaderId,
    ReceiveLeaderBackup {
        backup: LeaderBackup,
    },
    NewNextRobot {
        next_robot: usize,
    },
    TokenMessage {
        token: FlavorToken,
    },
    TokenBackupMsg {
        token_backup: TokenBackup,
    },
    NewLeader {
        leader: usize,
    },
    NewElection {
        candidates: Vec<(usize, bool)>,
    },
    NewOrder {
        order: Order,
        order_id: String,
    },
    OrderComplete {
        result: bool,
        order_id: String,
    },
    OrderNotFinished {
        result: bool,
        order_id: String,
        flavor: FlavorID,
    },
}

impl RobotCommand {
    pub fn from_string(msg: &str) -> Result<Self, RobotCommandError> {
        serde_json::from_str(msg).map_err(|err| RobotCommandError::ErrorParsing(err.to_string()))
    }

    pub fn to_string(&self) -> Result<String, RobotCommandError> {
        serde_json::to_string(self).map_err(|err| RobotCommandError::ErrorParsing(err.to_string()))
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ConnectToScreen {
    pub screen_id: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct GetTokenBackup {
    pub token_backup: TokenBackup,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SendTokenBackup {
    pub token_backup: TokenBackup,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SendFirstToken {
    pub flavor_id: FlavorID,
    pub amount: usize,
    pub robot_id: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct TimerWentOff();

#[derive(Message)]
#[rtype(result = "()")]
pub struct ScreenDied {
    pub screen_id: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ChangeScreen {
    pub original_screen_id: usize,
    pub new_screen_id: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct StoreBackup {
    pub backup: LeaderBackup,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SendLeaderBackup {
    pub backup: LeaderBackup,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct AddNewScreen {
    pub screen_id: usize,
    pub write_half: OwnedWriteHalf,
    pub read_half: OwnedReadHalf,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct AssignNewOrder();

#[derive(Message)]
#[rtype(result = "()")]
pub struct StartElection();

#[derive(Message)]
#[rtype(result = "()")]
pub struct ConnectToNewScreen {
    pub screen_id: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ReceiveNewElection {
    pub candidates: Vec<(usize, bool)>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SetNewLeader {
    pub leader_id: usize,
    pub by_election: bool,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct AddNewLeader {
    pub write_half: OwnedWriteHalf,
    pub read_half: OwnedReadHalf,
    pub leader_id: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct NewLeaderElected {
    pub leader_id: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct AddNewRobot {
    pub robot_id: usize,
    pub write_half: OwnedWriteHalf,
    pub read_half: OwnedReadHalf,
    pub asked: bool,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SetOrderManager {
    pub order_manager: Addr<OrderManager>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ScoopFlavor {
    pub flavor_token: FlavorToken,
    pub amount: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct TransferToken {
    pub flavor_token: FlavorToken,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct GetTokenBack {
    pub flavor_token: FlavorToken,
}
#[derive(Message)]
#[rtype(result = "()")]
pub struct AbortCurrentOrder {}

#[derive(Message)]
#[rtype(result = "()")]
pub struct OrderPrepared {
    pub order_result: bool,
    pub id: String,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct OrderAborted {
    pub order_result: bool,
    pub id: String,
    pub flavor: FlavorID,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct GetNewOrder {
    pub new_order: Order,
    pub id: String,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct CreateNewOrder {
    pub new_order: Order,
    pub id: String,
    pub screen_id: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SendNewOrder {
    pub new_order: Order,
    pub order_id: String,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct GetCompletedOrder {
    pub order_result: bool,
    pub order_id: String,
    pub robot_id: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct GetAbortedOrder {
    pub order_result: bool,
    pub order_id: String,
    pub robot_id: usize,
    pub flavor: FlavorID,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct AddOrderToBeSent {
    pub order_result: bool,
    pub id: String,
    pub screen_id: usize,
    pub flavor: Option<FlavorID>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RobotDied {
    pub robot_id: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct JoinRing();

#[derive(Message)]
#[rtype(result = "()")]
pub struct StartTokens {
    pub all_tokens: Vec<FlavorToken>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct AddPreviousRobot {
    pub write_half: OwnedWriteHalf,
    pub read_half: OwnedReadHalf,
    pub asked: bool,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct AddNextRobot {
    pub robot_id: usize,
    pub write_half: OwnedWriteHalf,
    pub read_half: OwnedReadHalf,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SetRobotConnectionHandler {
    pub rch_address: Addr<RobotConnectionHandler>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Harakiri();
