use actix::prelude::*;
use colored::*;
use tokio::net::tcp::OwnedWriteHalf;

use crate::robot::messages::*;
use crate::robot::robot_connection_handler::RobotConnectionHandler;
use crate::robot::utils::print_send_error;

/// Actor that handles the connection between two robots.
#[allow(dead_code)]
pub struct RobotToRobotConnection {
    rch: Addr<RobotConnectionHandler>,
    write_half: Option<OwnedWriteHalf>,
}

impl Actor for RobotToRobotConnection {
    type Context = Context<Self>;
}

impl RobotToRobotConnection {
    pub fn new(rch: Addr<RobotConnectionHandler>, write_half: Option<OwnedWriteHalf>) -> Self {
        Self { rch, write_half }
    }
}

impl Handler<Harakiri> for RobotToRobotConnection {
    type Result = ();
    fn handle(&mut self, _msg: Harakiri, ctx: &mut Self::Context) -> Self::Result {
        // println!("{}", "RPH: Me llego un mensaje de Hirikari".bright_green());
        ctx.stop();
    }
}

impl StreamHandler<Result<String, std::io::Error>> for RobotToRobotConnection {
    fn handle(&mut self, data: Result<String, std::io::Error>, _ctx: &mut Self::Context) {
        match data {
            Ok(t) => match RobotCommand::from_string(&t).map_err(|err| err.to_string()) {
                Ok(msg) => match msg {
                    RobotCommand::TokenMessage { token } => {
                        if let Err(e) = self.rch.try_send(TransferToken {
                            flavor_token: token,
                        }) {
                            print_send_error("[RTR]", "TransferToken", &e.to_string());
                        }
                    }
                    RobotCommand::TokenBackupMsg { token_backup } => {
                        if let Err(e) = self.rch.try_send(GetTokenBackup { token_backup }) {
                            print_send_error("[RTR]", "TokenBackupMsg", &e.to_string());
                        }
                    }
                    RobotCommand::NewLeader { leader } => {
                        // let line = format!("[RTR] Recibi un mensaje de nuevo lider {}", leader);
                        // println!("{}", line.bright_green());
                        if let Err(e) = self.rch.try_send(NewLeaderElected { leader_id: leader }) {
                            print_send_error("[RTR]", "ReceiveNewLeader", &e.to_string());
                        }
                    }
                    RobotCommand::NewElection { candidates } => {
                        // let line = format!("[RTR] Recibi un mensaje de eleccion {:?}", candidates);
                        // println!("{}", line.bright_green());
                        if let Err(e) = self.rch.try_send(ReceiveNewElection { candidates }) {
                            print_send_error("[RTR]", "ReceiveNewElection", &e.to_string());
                        }
                    }
                    _ => {
                        println!(
                            "[RTR]: Error! Did not understand StreamHandler message. I got: {}",
                            t
                        );
                    }
                },
                Err(e) => {
                    let line = format!("[RTR]: Error parsing message\n{}", e);
                    println!("{}", line.red());
                }
            },
            Err(e) => {
                // println!("Error receiving message: {}", e);
                let line = format!("[RTR]: Error! The other robot died!\n{}", e);
                println!("{}", line.red());
            }
        }
    }
}
