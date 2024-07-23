use std::collections::HashMap;

use actix::prelude::*;
use colored::Colorize;

use crate::{common::order::Order, screen::payments_gateway::HandleBackUp};

/// This actor is responsible for handling the backup of the screen.
/// It stores the orders that are pending to be processed, the orders that are being processed and the orders that are pending to be prepared.
/// It also stores the id of the backup.
/// It can save a backup, send the backup to the payments gateway and set the payments gateway.
/// It is used by the screen connection listener.
pub struct BackUpHandler {
    screen_backup_id: Option<usize>,
    orders_to_process: Vec<Order>,
    orders_processing: HashMap<String, Order>,
    orders_pending_to_prepare: Vec<(String, Order)>,
    payments_gateway: Option<Recipient<HandleBackUp>>,
}

impl Actor for BackUpHandler {
    type Context = Context<Self>;
}

impl Default for BackUpHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl BackUpHandler {
    pub fn new() -> BackUpHandler {
        BackUpHandler {
            screen_backup_id: None,
            orders_to_process: Vec::new(),
            orders_processing: HashMap::new(),
            orders_pending_to_prepare: Vec::new(),
            payments_gateway: None,
        }
    }

    pub fn are_empty(&self) -> bool {
        self.orders_to_process.is_empty()
            && self.orders_processing.is_empty()
            && self.orders_pending_to_prepare.is_empty()
    }

    pub fn same_backup(&self, msg: SaveBackup) -> bool {
        self.orders_pending_to_prepare == msg.orders_pending_to_prepare
            && self.orders_processing == msg.orders_processing
            && self.orders_to_process == msg.orders_to_process
    }
}

/// SaveBackup is a message that tells the BackUpHandler actor to save a backup.
/// It contains the orders that are pending to be processed, the orders that are being processed and the orders that are pending to be prepared.
/// It also contains the id of the backup.
/// The actor will store the orders and the id of the backup.
#[derive(Message, Clone)]
#[rtype(result = "()")]
pub struct SaveBackup {
    orders_to_process: Vec<Order>,
    orders_processing: HashMap<String, Order>,
    orders_pending_to_prepare: Vec<(String, Order)>,
    id_backup: usize,
}

impl SaveBackup {
    pub fn new(
        orders_to_process: Vec<Order>,
        orders_processing: HashMap<String, Order>,
        orders_pending_to_prepare: Vec<(String, Order)>,
        id_backup: usize,
    ) -> SaveBackup {
        SaveBackup {
            orders_to_process,
            orders_processing,
            orders_pending_to_prepare,
            id_backup,
        }
    }
}

impl Handler<SaveBackup> for BackUpHandler {
    type Result = ();

    fn handle(&mut self, msg: SaveBackup, _ctx: &mut Context<Self>) -> Self::Result {
        if self.same_backup(msg.clone()) {
            return;
        }
        println!("[BCKUP]{}{}", "Backup saved from ".yellow(), msg.id_backup);
        self.orders_to_process = msg.orders_to_process;
        self.orders_processing = msg.orders_processing;
        self.orders_pending_to_prepare = msg.orders_pending_to_prepare;
        self.screen_backup_id = Some(msg.id_backup);
    }
}

/// SendBackupToGateway is a message that tells the BackUpHandler actor to send the backup to the payments gateway.
/// This will happen when the previous screen is disconnected.
/// The actor will send the backup to the payments gateway and clear the orders that are being processed and the orders that are pending to be processed.
#[derive(Message)]
#[rtype(result = "()")]
pub struct SendBackupToGateway();

impl Default for SendBackupToGateway {
    fn default() -> Self {
        Self::new()
    }
}

impl SendBackupToGateway {
    pub fn new() -> SendBackupToGateway {
        SendBackupToGateway {}
    }
}

impl Handler<SendBackupToGateway> for BackUpHandler {
    type Result = ();

    fn handle(&mut self, _msg: SendBackupToGateway, _ctx: &mut Context<Self>) -> Self::Result {
        if self.are_empty() {
            return;
        }
        if let Some(payments_gateway) = &self.payments_gateway {
            payments_gateway.do_send(HandleBackUp::new(
                self.orders_to_process.clone(),
                self.orders_processing.clone(),
                self.orders_pending_to_prepare.clone(),
                self.screen_backup_id,
            ));
        }
        self.orders_processing.clear();
        self.orders_to_process.clear();
    }
}

/// SetPaymentsGateway is a message that tells the BackUpHandler actor to set the payments gateway.
#[derive(Message)]
#[rtype(result = "()")]
pub struct SetPaymentsGateway {
    payments_gateway: Recipient<HandleBackUp>,
}

impl SetPaymentsGateway {
    pub fn new(payments_gateway: Recipient<HandleBackUp>) -> SetPaymentsGateway {
        SetPaymentsGateway { payments_gateway }
    }
}

impl Handler<SetPaymentsGateway> for BackUpHandler {
    type Result = ();

    fn handle(&mut self, msg: SetPaymentsGateway, _ctx: &mut Context<Self>) -> Self::Result {
        self.payments_gateway = Some(msg.payments_gateway);
    }
}

#[cfg(test)]
mod tests {

    use crate::common::flavor_id::FlavorID;

    use super::*;
    #[actix::test]
    async fn test_save_backup() {
        let mut backup_handler = BackUpHandler::new();
        let orders_to_process = vec![Order::new_cucurucho(FlavorID::Chocolate)];
        let mut orders_processing = HashMap::new();
        orders_processing.insert("123".to_string(), Order::new_cucurucho(FlavorID::Vanilla));
        let save_backup = SaveBackup::new(
            orders_to_process.clone(),
            orders_processing.clone(),
            Vec::new(),
            123,
        );
        backup_handler.handle(save_backup, &mut Context::new());
        assert_eq!(backup_handler.orders_to_process, orders_to_process);
        assert_eq!(backup_handler.orders_processing, orders_processing);
    }

    #[actix::test]
    async fn test_overwrite_backup() {
        let mut backup_handler = BackUpHandler::new();
        let orders_to_process = vec![Order::new_cucurucho(FlavorID::Chocolate)];
        let mut orders_processing = HashMap::new();
        orders_processing.insert("123".to_string(), Order::new_cucurucho(FlavorID::Vanilla));
        let save_backup = SaveBackup::new(
            orders_to_process.clone(),
            orders_processing.clone(),
            Vec::new(),
            123,
        );
        backup_handler.handle(save_backup, &mut Context::new());
        let orders_to_process = vec![Order::new_cucurucho(FlavorID::Strawberry)];
        let mut orders_processing = HashMap::new();
        orders_processing.insert("123".to_string(), Order::new_cucurucho(FlavorID::Pistachio));
        let save_backup = SaveBackup::new(
            orders_to_process.clone(),
            orders_processing.clone(),
            Vec::new(),
            123,
        );
        backup_handler.handle(save_backup, &mut Context::new());
        assert_eq!(backup_handler.orders_to_process, orders_to_process);
        assert_eq!(backup_handler.orders_processing, orders_processing);
    }
}
