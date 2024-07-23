use std::collections::HashMap;
use std::sync::Arc;

use actix::prelude::*;
use actix::ActorFutureExt;
use fut::wrap_future;

use crate::common::order::Order;
use crate::common::screen_messages::ScreenMessage;
use crate::config::MAX_NUMBER_OF_SCREENS;
use crate::screen::payments_gateway::{PaymentsGateway, RegisterScreenConnection};
use tokio::io::{AsyncWriteExt, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use super::communication::connect_to_following_screen;
use super::payments_gateway::SendBackupToNewScreen;

/// ScreenConnectionSender is an actor that sends messages to the next screen.
/// It sends backups to the next screen.
pub struct ScreenConnectionSender {
    my_id: usize,
    socket_write: Arc<Mutex<WriteHalf<TcpStream>>>,
    payments_gateway: Addr<PaymentsGateway>,
}

impl Actor for ScreenConnectionSender {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.payments_gateway
            .do_send(RegisterScreenConnection::new(ctx.address()));
        self.payments_gateway.do_send(SendBackupToNewScreen());
    }
}

impl ScreenConnectionSender {
    pub fn new(
        socket_write: Arc<Mutex<WriteHalf<TcpStream>>>,
        payments_gateway: Addr<PaymentsGateway>,
        my_id: usize,
    ) -> ScreenConnectionSender {
        ScreenConnectionSender {
            my_id,
            socket_write,
            payments_gateway,
        }
    }
}

impl StreamHandler<Result<String, std::io::Error>> for ScreenConnectionSender {
    fn handle(&mut self, _msg: Result<String, std::io::Error>, _ctx: &mut Self::Context) {}
    fn finished(&mut self, ctx: &mut Self::Context) {
        let my_id = self.my_id;
        let following = (my_id + 1) % MAX_NUMBER_OF_SCREENS;
        let payments_gateway = self.payments_gateway.clone();
        async move {
            connect_to_following_screen(following, payments_gateway, my_id).await;
        }
        .into_actor(self)
        .map(|_, _, ctx| {
            ctx.stop();
        })
        .wait(ctx);
    }
}

/// SendMyBackup is a message that tells the ScreenConnectionSender actor to send a backup to the next screen.
#[derive(Message)]
#[rtype(result = "()")]
pub struct SendMyBackup {
    pub orders_to_process: Vec<Order>,
    pub orders_processing: HashMap<String, Order>,
    pub orders_pending_to_send: Vec<(String, Order)>,
    pub id_backup: usize,
}

impl SendMyBackup {
    pub fn new(
        orders_to_process: Vec<Order>,
        orders_processing: HashMap<String, Order>,
        orders_pending_to_send: Vec<(String, Order)>,
        id_backup: usize,
    ) -> SendMyBackup {
        SendMyBackup {
            orders_to_process,
            orders_processing,
            id_backup,
            orders_pending_to_send,
        }
    }
}

impl Handler<SendMyBackup> for ScreenConnectionSender {
    type Result = ();

    fn handle(&mut self, msg: SendMyBackup, _ctx: &mut Context<Self>) -> Self::Result {
        if msg.orders_processing.is_empty()
            && msg.orders_to_process.is_empty()
            && msg.orders_pending_to_send.is_empty()
        {
            return;
        }
        let msg = ScreenMessage::TakeMyBackup {
            orders_to_process: msg.orders_to_process,
            orders_processing: msg.orders_processing,
            orders_pending_to_send: msg.orders_pending_to_send,
            id_backup: msg.id_backup,
        };
        let msg = match msg.to_string() {
            Ok(string) => string,
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

/// RequestRobotLeaderConnection is a message that tells the ScreenConnectionSender actor to request a connection with the robot leader.
/// This happens when this screen connects after the robot leader.
#[derive(Message)]
#[rtype(result = "()")]
pub struct RequestRobotLeaderConnection {
    pub id: usize,
}

impl RequestRobotLeaderConnection {
    pub fn new(id: usize) -> RequestRobotLeaderConnection {
        RequestRobotLeaderConnection { id }
    }
}

impl Handler<RequestRobotLeaderConnection> for ScreenConnectionSender {
    type Result = ();

    fn handle(
        &mut self,
        msg: RequestRobotLeaderConnection,
        _ctx: &mut Context<Self>,
    ) -> Self::Result {
        let message = ScreenMessage::RequestRobotLeaderConnection { screen_id: msg.id };
        let msg = match message.to_string() {
            Ok(string) => string,
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
