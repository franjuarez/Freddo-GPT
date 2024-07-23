use std::sync::Arc;

use actix::prelude::*;
use fut::wrap_future;

use crate::common::order::Order;
use crate::common::robot_messages::RobotMessage;
use crate::common::screen_messages::ScreenMessage;
use crate::screen::payments_gateway::{
    AbortOrder, ConfirmOrder, PaymentsGateway, RegisterRobotConnection,
};

use tokio::io::{AsyncWriteExt, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use super::payments_gateway::StartProcessingIfWaiting;

/// RobotConnectionHandler is an actor that handles the connection between the robot and the screen.
/// It receives messages from the robot, processes them and sends them to the PaymentsGateway actor.
/// It also sends messages to the robot.
pub struct RobotConnectionHandler {
    socket_write: Arc<Mutex<WriteHalf<TcpStream>>>,
    payments_gateway: Addr<PaymentsGateway>,
}

impl Actor for RobotConnectionHandler {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        if let Err(err) = self
            .payments_gateway
            .try_send(RegisterRobotConnection::new(ctx.address()))
        {
            println!("Error sending message to payments gateway: {}", err);
        }
        if let Err(err) = self.payments_gateway.try_send(StartProcessingIfWaiting()) {
            println!("Error sending message to payments gateway: {}", err);
        }
    }
}

impl RobotConnectionHandler {
    pub fn new(
        socket_write: Arc<Mutex<WriteHalf<TcpStream>>>,
        payments_gateway: Addr<PaymentsGateway>,
    ) -> RobotConnectionHandler {
        RobotConnectionHandler {
            socket_write,
            payments_gateway,
        }
    }
}

impl StreamHandler<Result<String, std::io::Error>> for RobotConnectionHandler {
    fn handle(&mut self, msg: Result<String, std::io::Error>, ctx: &mut Self::Context) {
        if let Ok(msg) = msg {
            if ctx
                .address()
                .try_send(HandleRobotMsg { received_msg: msg })
                .is_err()
            {
                println!("Error sending msg to handler");
            }
        }
    }
}

/// Handle every message received from the robot.
/// If the message is an OrderPrepared message, send a ConfirmOrder message to the PaymentsGateway.
/// If the message is an OrderAborted message, send an AbortOrder message to the PaymentsGateway.
#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct HandleRobotMsg {
    received_msg: String,
}

impl Handler<HandleRobotMsg> for RobotConnectionHandler {
    type Result = Result<(), String>;

    fn handle(&mut self, msg: HandleRobotMsg, _ctx: &mut Context<Self>) -> Self::Result {
        match RobotMessage::from_string(&msg.received_msg).map_err(|err| err.to_string())? {
            RobotMessage::OrderPrepared { order_id } => {
                if let Err(err) = self.payments_gateway.try_send(ConfirmOrder::new(order_id)) {
                    println!("Error sending message to payments gateway: {}", err);
                }
                Ok(())
            }
            RobotMessage::OrderAborted { order_id, error } => {
                if let Err(err) = self
                    .payments_gateway
                    .try_send(AbortOrder::new(order_id, error))
                {
                    println!("Error sending message to payments gateway: {}", err);
                }
                Ok(())
            }
        }
    }
}

/// SendOrderToRobotLeader is a message that tells the RobotConnectionHandler actor to send an order to the robot leader.
#[derive(Message)]
#[rtype(result = "()")]
pub struct SendOrderToRobotLeader {
    order: Order,
    id_order: String,
    id_screen: usize,
}

impl SendOrderToRobotLeader {
    pub fn new(order: Order, id_order: String, id_screen: usize) -> SendOrderToRobotLeader {
        SendOrderToRobotLeader {
            order,
            id_order,
            id_screen,
        }
    }
}

impl Handler<SendOrderToRobotLeader> for RobotConnectionHandler {
    type Result = ();

    fn handle(&mut self, msg: SendOrderToRobotLeader, _ctx: &mut Context<Self>) -> Self::Result {
        let msg = match prepare_message(msg) {
            Some(value) => value,
            None => return,
        };
        let arc = self.socket_write.clone();
        wrap_future::<_, Self>(async move {
            arc.lock()
                .await
                .write_all(msg.as_bytes())
                .await
                .expect("should have sent")
        })
        .spawn(_ctx);
    }
}

/// Receives a SendOrderToRobotLeader message and prepares the message to be sent to the robot leader.
fn prepare_message(msg: SendOrderToRobotLeader) -> Option<String> {
    let msg = ScreenMessage::PrepareNewOrder {
        order_id: msg.id_order,
        order: msg.order,
        screen_id: msg.id_screen,
    };
    let msg = match msg.to_string() {
        Ok(msg) => msg + "\n",
        Err(err) => {
            println!("Error converting message to string: {}", err);
            return None;
        }
    };
    Some(msg)
}

/// SendRequestToRobotLeader is a message that tells the RobotConnectionHandler actor to send a request to the robot leader.
/// This will happen when the previous screen is reconnected and needs to connect to the robot leader.
#[derive(Message)]
#[rtype(result = "()")]
pub struct SendRequestToRobotLeader {
    pub id: usize,
}

impl SendRequestToRobotLeader {
    pub fn new(id: usize) -> SendRequestToRobotLeader {
        SendRequestToRobotLeader { id }
    }
}

impl Handler<SendRequestToRobotLeader> for RobotConnectionHandler {
    type Result = ();

    fn handle(&mut self, msg: SendRequestToRobotLeader, _ctx: &mut Context<Self>) -> Self::Result {
        let message = ScreenMessage::RequestRobotLeaderConnection { screen_id: msg.id };
        let msg = match message.to_string() {
            Ok(msg) => msg,
            Err(err) => {
                println!("Error converting message to string: {}", err);
                return;
            }
        };
        let msg = msg + "\n";
        let writer = self.socket_write.clone();
        wrap_future::<_, Self>(async move {
            let _ = writer.lock().await.write_all(msg.as_bytes()).await;
        })
        .spawn(_ctx);
    }
}

/// AskRobotForScreenOrders is a message that tells the RobotConnectionHandler actor to ask the robot for the orders of a screen.
/// This is useful when the previous screen disconnects and the robot needs to send the orders to the screen with the backup
#[derive(Message)]
#[rtype(result = "()")]
pub struct AskRobotForScreenOrders {
    new_screen_id: usize,
    death_screen_id: usize,
}

impl AskRobotForScreenOrders {
    pub fn new(new_screen_id: usize, death_screen_id: usize) -> AskRobotForScreenOrders {
        AskRobotForScreenOrders {
            new_screen_id,
            death_screen_id,
        }
    }
}

impl Handler<AskRobotForScreenOrders> for RobotConnectionHandler {
    type Result = ();

    fn handle(&mut self, msg: AskRobotForScreenOrders, _ctx: &mut Context<Self>) -> Self::Result {
        let message = ScreenMessage::GiveMeThisScreenOrders {
            my_id: msg.new_screen_id,
            death_id: msg.death_screen_id,
        };
        let msg = match message.to_string() {
            Ok(msg) => msg,
            Err(err) => {
                println!("Error converting message to string: {}", err);
                return;
            }
        };
        let msg = msg + "\n";
        let writer = self.socket_write.clone();
        wrap_future::<_, Self>(async move {
            let _ = writer.lock().await.write_all(msg.as_bytes()).await;
        })
        .spawn(_ctx);
    }
}
