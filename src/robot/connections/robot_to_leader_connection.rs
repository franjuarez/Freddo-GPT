use actix::prelude::*;
use colored::*;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;

use crate::robot::messages::*;
use crate::robot::robot_connection_handler::RobotConnectionHandler;
use crate::robot::utils::{print_create_error, print_send_error};

/// Actor that handles the connection between a robot and the leader.
pub struct RobotToLeaderConnection {
    rch: Addr<RobotConnectionHandler>,
    // my_id: usize,
    write_half: Option<OwnedWriteHalf>,
}

impl Actor for RobotToLeaderConnection {
    type Context = Context<Self>;
}

impl RobotToLeaderConnection {
    pub fn new(rch: Addr<RobotConnectionHandler>, write_half: Option<OwnedWriteHalf>) -> Self {
        Self { rch, write_half }
    }
}

impl Handler<Harakiri> for RobotToLeaderConnection {
    type Result = ();
    fn handle(&mut self, _msg: Harakiri, ctx: &mut Self::Context) -> Self::Result {
        // println!("{}", "RPH: Me llego un mensaje de Hirikari".bright_green());
        ctx.stop();
    }
}

impl Handler<OrderPrepared> for RobotToLeaderConnection {
    type Result = ();
    fn handle(&mut self, result_msg: OrderPrepared, ctx: &mut Self::Context) -> Self::Result {
        let order_msg = RobotCommand::OrderComplete {
            result: result_msg.order_result,
            order_id: result_msg.id.clone(),
        }
        .to_string();
        let msg: String;
        match order_msg {
            Ok(r_msg) => {
                msg = r_msg + "\n";
                if let Some(mut write_half) = self.write_half.take() {
                    async move {
                        let mut could_send = true;
                        if let Err(e) = write_half.write_all(msg.as_bytes()).await {
                            println!("[RTLC] Error trying to send OrderPrepared to Leader: {}", e);
                            could_send = false;
                        }
                        (could_send, write_half)
                    }
                    .into_actor(self)
                    .map(move |(could_send, w_half), actor, _| {
                        actor.write_half = Some(w_half);
                        if !could_send {
                            if let Err(e) = actor.rch.try_send(OrderPrepared {
                                order_result: result_msg.order_result,
                                id: result_msg.id,
                            }) {
                                print_send_error("[RTLC]", "OrderPrepared", &e.to_string());
                            }
                        }
                    })
                    .wait(ctx);
                }
            }
            Err(e) => {
                print_create_error("[RTLC]", "OrderPrepared", &e.to_string());
            }
        }
    }
}

impl Handler<OrderAborted> for RobotToLeaderConnection {
    type Result = ();
    fn handle(&mut self, result_msg: OrderAborted, ctx: &mut Self::Context) -> Self::Result {
        let order_msg = RobotCommand::OrderNotFinished {
            result: result_msg.order_result,
            order_id: result_msg.id.clone(),
            flavor: result_msg.flavor,
        }
        .to_string();
        let msg: String;
        match order_msg {
            Ok(r_msg) => {
                msg = r_msg + "\n";
                if let Some(mut write_half) = self.write_half.take() {
                    async move {
                        let mut could_send = true;
                        if let Err(e) = write_half.write_all(msg.as_bytes()).await {
                            println!("[RTLC] Error trying to send OrderAborted to Leader: {}", e);
                            could_send = false;
                        }
                        (could_send, write_half)
                    }
                    .into_actor(self)
                    .map(move |(could_send, w_half), actor, _| {
                        actor.write_half = Some(w_half);
                        if !could_send {
                            if let Err(e) = actor.rch.try_send(OrderAborted {
                                order_result: result_msg.order_result,
                                id: result_msg.id,
                                flavor: result_msg.flavor,
                            }) {
                                print_send_error("[RTLC]", "OrderAborted", &e.to_string());
                            }
                        }
                    })
                    .wait(ctx);
                }
            }
            Err(e) => {
                print_create_error("[RTLC]", "OrderAborted", &e.to_string());
            }
        }
    }
}

impl StreamHandler<Result<String, std::io::Error>> for RobotToLeaderConnection {
    fn handle(&mut self, data: Result<String, std::io::Error>, _ctx: &mut Self::Context) {
        match data {
            Ok(t) => {
                match RobotCommand::from_string(&t).map_err(|err| err.to_string()) {
                    Ok(msg) => match msg {
                        RobotCommand::NewOrder { order, order_id } => {
                            if let Err(e) = self.rch.try_send(GetNewOrder {
                                new_order: order,
                                id: order_id,
                            }) {
                                print_send_error("[RTLC]", "GetNewOrder", &e.to_string());
                            }
                        }
                        RobotCommand::ReceiveLeaderBackup { backup } => {
                            // let line = format!("[RTLC]: New message from Leader: ReceiveLeaderBackup");
                            // println!("{}", line.bright_green());
                            if let Err(e) = self.rch.try_send(StoreBackup { backup }) {
                                print_send_error("[RTLC]", "StoreBackup", &e.to_string());
                            }
                        }
                        _ => {
                            println!("[RTLC]: Error! Did not understand StreamHandler message. I got: {}", t);
                        }
                    },
                    Err(e) => {
                        println!("[RTLC]: Error parsing message: {}", e);
                    }
                }
            }
            Err(e) => {
                let line = format!("[RTLC]: Leader failed!\n{:?}", e);
                println!("{}", line.bright_red());
            }
        }
    }

    fn finished(&mut self, _ctx: &mut Self::Context) {
        println!("[RTLC] Leader failed!");
        if let Err(e) = self.rch.try_send(StartElection()) {
            print_send_error("[RTLC]", "StartElection", &e.to_string());
        }
    }
}
