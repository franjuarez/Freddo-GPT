use actix::prelude::*;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;

use crate::robot::messages::*;
use crate::robot::robot_leader::RobotLeader;
use crate::robot::utils::{print_create_error, print_send_error};

/// Actor that represents the connection between the Leader and a Robot
pub struct LeaderToRobotConnection {
    leader: Addr<RobotLeader>,
    my_id: usize,
    write_half: Option<OwnedWriteHalf>,
}

impl Actor for LeaderToRobotConnection {
    type Context = Context<Self>;
}

impl LeaderToRobotConnection {
    pub fn new(
        leader: Addr<RobotLeader>,
        my_id: usize,
        write_half: Option<OwnedWriteHalf>,
    ) -> Self {
        Self {
            leader,
            my_id,
            write_half,
        }
    }
}

impl Handler<Harakiri> for LeaderToRobotConnection {
    type Result = ();
    fn handle(&mut self, _msg: Harakiri, ctx: &mut Self::Context) -> Self::Result {
        // println!(
        //     "{}",
        //     "[LTR]: Me llego un mensaje de Hirikari".bright_green()
        // );
        ctx.stop();
    }
}

impl Handler<SendNewOrder> for LeaderToRobotConnection {
    type Result = ();
    fn handle(&mut self, msg: SendNewOrder, ctx: &mut Self::Context) -> Self::Result {
        // let line = format!(
        //     "[LTR]: Recibi un mensaje de orden {:?} y lo envio a mi robotito {}",
        //     msg.new_order,
        //     self.my_id.clone()
        // );
        // println!("{}", line.bright_magenta());
        let order_msg = RobotCommand::NewOrder {
            order: msg.new_order,
            order_id: msg.order_id,
        }
        .to_string();
        let msg: String;
        match order_msg {
            Ok(r_msg) => {
                msg = r_msg + "\n";
                if let Some(mut write_half) = self.write_half.take() {
                    async move {
                        if let Err(e) = write_half.write_all(msg.as_bytes()).await {
                            println!(
                                "[LTR] Error trying to send NewOrder Message. Message dumped\n{}",
                                e
                            );
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
                print_create_error("[LTR]", "NewOrder", &e.to_string());
            }
        }
    }
}

impl Handler<SendLeaderBackup> for LeaderToRobotConnection {
    type Result = ();
    fn handle(&mut self, msg: SendLeaderBackup, ctx: &mut Self::Context) -> Self::Result {
        // let line = format!(
        //     "[LTR]: Recibi un mensaje de orden {:?} y lo envio a mi robotito {}",
        //     msg.backup,
        //     self.my_id.clone()
        // );
        // println!("{}", line.bright_magenta());
        let backup_msg = RobotCommand::ReceiveLeaderBackup { backup: msg.backup }.to_string();
        let msg: String;
        match backup_msg {
            Ok(r_msg) => {
                msg = r_msg + "\n";
                if let Some(mut write_half) = self.write_half.take() {
                    async move {
                        if let Err(e) = write_half.write_all(msg.as_bytes()).await {
                            println!(
                                "[LTR] Error trying to send LeaderBackup Message. Message dumped\n{}",
                                e
                            );
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
                print_create_error("[LTR]", "ReceiveLeaderBackup", &e.to_string());
            }
        }
    }
}

impl StreamHandler<Result<String, std::io::Error>> for LeaderToRobotConnection {
    fn handle(&mut self, data: Result<String, std::io::Error>, _ctx: &mut Self::Context) {
        match data {
            Ok(t) => {
                match RobotCommand::from_string(&t).map_err(|err| err.to_string()) {
                    Ok(msg) => {
                        match msg {
                            RobotCommand::OrderComplete { result, order_id } => {
                                // let line = format!("[LTR]: Recibi un mensaje de orden completada {:?}, con el id {:?}", result, order_id);
                                // println!("{}", line.bright_magenta());
                                if let Err(e) = self.leader.try_send(GetCompletedOrder {
                                    order_result: result,
                                    order_id,
                                    robot_id: self.my_id,
                                }) {
                                    print_send_error("[LTR]", "GetCompletedOrder", &e.to_string());
                                }
                            }
                            RobotCommand::OrderNotFinished {
                                result,
                                order_id,
                                flavor,
                            } => {
                                // let line = format!("[LTR]: Recibi un mensaje de orden abortada {:?}, con el id {:?}", result, order_id);
                                // println!("{}", line.bright_magenta());
                                if let Err(e) = self.leader.try_send(GetAbortedOrder {
                                    order_result: result,
                                    order_id,
                                    robot_id: self.my_id,
                                    flavor,
                                }) {
                                    print_send_error("[LTR]", "GetCompletedOrder", &e.to_string());
                                }
                            }
                            _ => {
                                println!("[LTR]: Error! Did not understand StreamHandler message. I got: {}", t);
                            }
                        }
                    }
                    Err(e) => {
                        println!("Error parsing message:  {}", e);
                    }
                }
            }
            Err(e) => {
                println!("[LTR] Error! Connection with robot died! : {}", e);
            }
        }
    }

    fn finished(&mut self, _ctx: &mut Self::Context) {
        // println!("[LTR]: Robot died!");
        if let Err(e) = self.leader.try_send(RobotDied {
            robot_id: self.my_id,
        }) {
            print_send_error("[LTR]", "Harakiri", &e.to_string());
        }
    }
}
