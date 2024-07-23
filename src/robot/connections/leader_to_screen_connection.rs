use actix::prelude::*;
use colored::*;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;

use crate::common::robot_messages::*;
use crate::common::screen_messages::*;
use crate::robot::messages::*;
use crate::robot::robot_leader::RobotLeader;
use crate::robot::utils::{print_create_error, print_send_error};

/// Actor that represents the connection between the RobotLeader and a Screen
pub struct LeaderToScreenConnection {
    leader: Addr<RobotLeader>,
    write_half: Option<OwnedWriteHalf>,
    screen_id: usize,
}

impl Actor for LeaderToScreenConnection {
    type Context = Context<Self>;
}

impl LeaderToScreenConnection {
    pub fn new(
        screen_id: usize,
        leader: Addr<RobotLeader>,
        write_half: Option<OwnedWriteHalf>,
    ) -> Self {
        Self {
            screen_id,
            leader,
            write_half,
        }
    }
}

impl Handler<Harakiri> for LeaderToScreenConnection {
    type Result = ();
    fn handle(&mut self, _msg: Harakiri, ctx: &mut Self::Context) -> Self::Result {
        // println!("{}", "RPH: Me llego un mensaje de Hirikari".bright_purple());
        ctx.stop();
    }
}

impl StreamHandler<Result<String, std::io::Error>> for LeaderToScreenConnection {
    fn handle(&mut self, data: Result<String, std::io::Error>, _ctx: &mut Self::Context) {
        match data {
            Ok(t) => {
                match ScreenMessage::from_string(&t).map_err(|err| err.to_string()) {
                    Ok(msg) => {
                        match msg {
                            ScreenMessage::PrepareNewOrder {
                                order_id,
                                order,
                                screen_id,
                            } => {
                                // let line =
                                //     format!("[SC]: Recibi un mensaje de orden {:?}", order_id);
                                // println!("{}", line.bright_purple());
                                if let Err(e) = self.leader.try_send(CreateNewOrder {
                                    id: order_id,
                                    new_order: order,
                                    screen_id,
                                }) {
                                    print_send_error("[SC]", "GetNewOrder", &e.to_string());
                                }
                            }
                            ScreenMessage::RequestRobotLeaderConnection { screen_id } => {
                                if let Err(e) =
                                    self.leader.try_send(ConnectToNewScreen { screen_id })
                                {
                                    print_send_error("[SC]", "ConnectToNewScreen", &e.to_string());
                                }
                            }
                            ScreenMessage::GiveMeThisScreenOrders { my_id, death_id } => {
                                if let Err(e) = self.leader.try_send(ChangeScreen {
                                    original_screen_id: death_id,
                                    new_screen_id: my_id,
                                }) {
                                    print_send_error("[SC]", "ConnectToNewScreen", &e.to_string());
                                }
                            }
                            _ => {
                                println!("[SC]: Error! Did not understand StreamHandler message. I got: {}", t);
                            }
                        }
                    }
                    Err(e) => {
                        println!("[SC]: Error parsing message: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("Error! Screen died\n{}", e);
            }
        }
    }

    fn finished(&mut self, _ctx: &mut Self::Context) {
        if let Err(e) = self.leader.try_send(ScreenDied {
            screen_id: self.screen_id,
        }) {
            print_send_error("[SC]", "ScreenDied", &e.to_string());
        }
    }
}

impl Handler<OrderPrepared> for LeaderToScreenConnection {
    type Result = ();
    fn handle(&mut self, msg: OrderPrepared, ctx: &mut Self::Context) -> Self::Result {
        let result = msg.order_result;
        let order_id = msg.id;

        let order_msg = RobotMessage::OrderPrepared {
            order_id: order_id.clone(),
        }
        .to_string();

        let msg: String;
        match order_msg {
            Ok(r_msg) => {
                msg = r_msg + "\n";
                if let Some(mut write_half) = self.write_half.take() {
                    let leader = self.leader.clone();
                    let screen_id = self.screen_id;
                    async move {
                        if let Err(e) = write_half.write_all(msg.as_bytes()).await {
                            println!(
                                "[SC] Error trying to send OrderResult to Screen. Stashing...: {}",
                                e
                            );
                            if let Err(e) = leader.try_send(AddOrderToBeSent {
                                id: order_id,
                                order_result: result,
                                screen_id,
                                flavor: None,
                            }) {
                                print_send_error("[SC]", "ConnectToNewScreen", &e.to_string());
                            }
                        }
                        write_half
                    }
                    .into_actor(self)
                    .map(|w_half, actor, _| {
                        actor.write_half = Some(w_half);
                    })
                    .wait(ctx);
                }
            }
            Err(e) => {
                print_create_error("[SC]", "OrderCompleated", &e.to_string());
            }
        }
    }
}

impl Handler<OrderAborted> for LeaderToScreenConnection {
    type Result = ();
    fn handle(&mut self, msg: OrderAborted, ctx: &mut Self::Context) -> Self::Result {
        let line = "[SC]: Recibi un mensaje de orden aborted".to_string();
        println!("{}", line.bright_green());

        let result = msg.order_result;
        let order_id = msg.id;
        let flavor_id = msg.flavor;

        let error_msg = format!(
            "Order Aborted because of insuficient amount of: {}",
            flavor_id
        )
        .to_string();

        let order_msg = RobotMessage::OrderAborted {
            order_id: order_id.clone(),
            error: error_msg,
        }
        .to_string();

        let msg: String;
        match order_msg {
            Ok(r_msg) => {
                msg = r_msg + "\n";
                if let Some(mut write_half) = self.write_half.take() {
                    let leader = self.leader.clone();
                    let screen_id = self.screen_id;
                    async move {
                        if let Err(e) = write_half.write_all(msg.as_bytes()).await {
                            println!(
                                "[SC] Error trying to send OrderResult to Screen. Stashing...: {}",
                                e
                            );
                            if let Err(e) = leader.try_send(AddOrderToBeSent {
                                id: order_id,
                                order_result: result,
                                screen_id,
                                flavor: Some(flavor_id),
                            }) {
                                print_send_error("[SC]", "ConnectToNewScreen", &e.to_string());
                            }
                        }
                        write_half
                    }
                    .into_actor(self)
                    .map(|w_half, actor, _| {
                        actor.write_half = Some(w_half);
                    })
                    .wait(ctx);
                }
            }
            Err(e) => {
                print_create_error("[SC]", "OrderCompleated", &e.to_string());
            }
        }
    }
}
