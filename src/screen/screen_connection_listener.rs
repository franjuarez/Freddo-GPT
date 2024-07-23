use actix::prelude::*;

use crate::common::screen_messages::ScreenMessage;
use crate::screen::backup_handler::SendBackupToGateway;

use super::backup_handler::{BackUpHandler, SaveBackup};
use super::payments_gateway::{PaymentsGateway, SendRequestFromScreen};

/// ScreenConnectionListener is an actor that listens to the connection with the previous screen.
/// It receives backups from the previous screen and sends them to the BackUpHandler actor.
pub struct ScreenConnectionListener {
    backup_handler: Addr<BackUpHandler>,
    payments_gateway: Addr<PaymentsGateway>,
}

impl Actor for ScreenConnectionListener {
    type Context = Context<Self>;
}

impl ScreenConnectionListener {
    pub fn new(
        backup_handler: Addr<BackUpHandler>,
        payments_gateway: Addr<PaymentsGateway>,
    ) -> ScreenConnectionListener {
        ScreenConnectionListener {
            backup_handler,
            payments_gateway,
        }
    }
}

impl StreamHandler<Result<String, std::io::Error>> for ScreenConnectionListener {
    fn handle(&mut self, msg: Result<String, std::io::Error>, ctx: &mut Self::Context) {
        if let Ok(msg) = msg {
            if ctx
                .address()
                .try_send(HandleScreenMsg { received_msg: msg })
                .is_err()
            {
                println!("Error sending msg to handler");
            }
        } else {
            println!("Error reading message from screen. finishing");
            if self
                .backup_handler
                .try_send(SendBackupToGateway::new())
                .is_err()
            {
                println!("Error sending backup to gateway");
            }
            ctx.stop();
        }
    }

    fn finished(&mut self, _ctx: &mut Self::Context) {
        self.backup_handler.do_send(SendBackupToGateway::new())
    }
}

/// HandleScreenMsg is a message that tells the ScreenConnectionListener actor to handle a message from the screen.
/// The message is a string that is parsed into a ScreenMessage.
/// This could be a backup message or a request from the previous robot for a connection with the robot leader.
#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct HandleScreenMsg {
    received_msg: String,
}

impl Handler<HandleScreenMsg> for ScreenConnectionListener {
    type Result = Result<(), String>;

    fn handle(&mut self, msg: HandleScreenMsg, _ctx: &mut Context<Self>) -> Self::Result {
        match ScreenMessage::from_string(&msg.received_msg).map_err(|err| err.to_string())? {
            ScreenMessage::TakeMyBackup {
                orders_processing,
                orders_to_process,
                id_backup,
                orders_pending_to_send,
            } => {
                if self
                    .backup_handler
                    .try_send(SaveBackup::new(
                        orders_to_process,
                        orders_processing,
                        orders_pending_to_send,
                        id_backup,
                    ))
                    .is_err()
                {
                    println!("Error sending backup to handler");
                }
                Ok(())
            }
            ScreenMessage::RequestRobotLeaderConnection { screen_id } => {
                if self
                    .payments_gateway
                    .try_send(SendRequestFromScreen::new(screen_id))
                    .is_err()
                {
                    println!("Error sending request to payments gateway");
                }
                Ok(())
            }
            _ => {
                println!("Message not recognized");
                Ok(())
            }
        }
    }
}
