use actix::prelude::*;
use rand::Rng;

use super::{
    robot_connection_handler::{
        RobotConnectionHandler, SendOrderToRobotLeader, SendRequestToRobotLeader,
    },
    screen_connection_sender::{
        RequestRobotLeaderConnection, ScreenConnectionSender, SendMyBackup,
    },
};
use crate::common::order::Order;
use crate::screen::robot_connection_handler::AskRobotForScreenOrders;
use actix::prelude::AsyncContext;
use colored::Colorize;
use std::collections::HashMap;
#[cfg(not(test))]
use tokio::time::Duration;
use uuid::Uuid;
/// PaymentsGateway is an actor that is in charge of capturing the orders and processing the payments.
/// This actor will receive the orders from the OrderReader actor and will capture them one by one.
/// After the order is prepared, it will confirm the payment.
pub struct PaymentsGateway {
    id: usize,
    orders_waiting: Vec<Order>,
    orders_captured: HashMap<String, Order>,
    robot_connection_handler: Option<Addr<RobotConnectionHandler>>,
    screen_connection_sender: Option<Addr<ScreenConnectionSender>>,
    orders_pending_to_prepare: Vec<(String, Order)>,
}

impl PaymentsGateway {
    pub fn new(id: usize) -> PaymentsGateway {
        PaymentsGateway {
            id,
            orders_waiting: Vec::new(),
            orders_captured: HashMap::new(),
            orders_pending_to_prepare: Vec::new(),
            robot_connection_handler: None,
            screen_connection_sender: None,
        }
    }

    /// This method will send a ProcessNewOrder message to itself.
    fn process_new_order(&mut self, _ctx: &mut Context<PaymentsGateway>) {
        #[cfg(not(test))]
        match _ctx.address().try_send(ProcessNewOrder()) {
            Ok(_) => (),
            Err(_) => println!("Error sending ProcessNewOrder"),
        };
    }

    /// This method will check if the robot connection is active and send the order to the robot leader.
    /// If the connection is not active, it will store the order in the orders_pending_to_prepare vector.
    fn check_robot_connection_and_send_order(&mut self, id: String, order: Order) {
        if self.robot_connection_handler.is_none()
            || !self
                .robot_connection_handler
                .clone()
                .expect("This should never happen")
                .connected()
        {
            self.orders_pending_to_prepare
                .push((id.clone(), order.clone()));
            if self.robot_connection_handler.is_none() {
                if let Some(sender) = self.screen_connection_sender.clone() {
                    sender.do_send(RequestRobotLeaderConnection::new(self.id));
                }
            }
        } else if let Some(handler) = self.robot_connection_handler.clone() {
            handler.do_send(SendOrderToRobotLeader::new(order, id.clone(), self.id));
        }
    }

    /// This method will send a backup to the screen connection sender.
    /// It will send the orders_waiting, orders_captured and orders_pending_to_prepare vectors.
    fn send_backup(&mut self) {
        if self.screen_connection_sender.is_none()
            || !self
                .screen_connection_sender
                .clone()
                .expect("This should never happen")
                .connected()
        {
            return;
        }
        self.screen_connection_sender
            .clone()
            .expect("This should never happen")
            .do_send(SendMyBackup::new(
                self.orders_waiting.clone(),
                self.orders_captured.clone(),
                self.orders_pending_to_prepare.clone(),
                self.id,
            ));
    }

    fn check_all_processed(&mut self) {
        if self.orders_captured.is_empty() && self.orders_waiting.is_empty() {
            println!("{}", "All orders processed".bright_green());
        }
    }
}

impl Actor for PaymentsGateway {
    type Context = Context<Self>;
}

/// ReceiveOrders is a message that tells the PaymentsGateway actor to receive the orders from the OrderReader actor.
/// The orders are stored in the orders_waiting vector.
#[derive(Message)]
#[rtype(result = "Result<Vec<Order>, std::io::Error>")]
pub struct ReceiveOrders {
    orders: Vec<Order>,
}

impl ReceiveOrders {
    pub fn new(orders: Vec<Order>) -> ReceiveOrders {
        ReceiveOrders { orders }
    }
}

impl Handler<ReceiveOrders> for PaymentsGateway {
    type Result = Result<Vec<Order>, std::io::Error>;

    fn handle(&mut self, msg: ReceiveOrders, _ctx: &mut Context<Self>) -> Self::Result {
        self.orders_waiting = self
            .orders_waiting
            .clone()
            .into_iter()
            .chain(msg.orders.clone())
            .collect();
        #[cfg(not(test))]
        if _ctx.address().try_send(ProcessNewOrder()).is_err() {
            println!("Error sending ProcessNewOrder");
        }
        #[cfg(test)]
        _ctx.address()
            .do_send(CaptureNewOrder::new(0.5, "id".to_string()));
        Ok(self.orders_waiting.clone())
    }
}

/// GetOrdersWaiting is a message that tells the PaymentsGateway actor to return the orders that are waiting to be captured.
#[derive(Message)]
#[rtype(result = "Vec<Order>")]
pub struct GetOrdersWaiting();

impl Handler<GetOrdersWaiting> for PaymentsGateway {
    type Result = Vec<Order>;

    fn handle(&mut self, _msg: GetOrdersWaiting, _ctx: &mut Context<Self>) -> Self::Result {
        self.orders_waiting.clone()
    }
}

/// ProcessNewOrder is a message that tells the PaymentsGateway actor to capture a new order.
/// This will happen when the orders_waiting vector is not empty.
/// The actor will wait for 2 seconds to simulate the payment processing.
#[cfg(not(test))]
#[derive(Message)]
#[rtype(result = "()")]
pub struct ProcessNewOrder();

#[cfg(not(test))]
impl Handler<ProcessNewOrder> for PaymentsGateway {
    type Result = ();

    fn handle(&mut self, _msg: ProcessNewOrder, _ctx: &mut Context<Self>) -> Self::Result {
        if self.orders_waiting.is_empty() {
            return;
        }
        async move {
            let output = "Processing new order...".to_string();
            println!("[GTW] {}", output.yellow());
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        .into_actor(self)
        .wait(_ctx);
        _ctx.address().do_send(CaptureOrder());
    }
}

/// CaptureOrder is a message that tells the PaymentsGateway actor to capture a new order.
/// This is made by removing the first order from the orders_waiting vector and adding it to the orders_captured hashmap.
/// If the card is invalid, the order is not captured and the method returns None. It depends on a random number generator to simulate the card validation.
#[derive(Message)]
#[rtype(result = "()")]
pub struct CaptureOrder();

impl Handler<CaptureOrder> for PaymentsGateway {
    type Result = ();

    fn handle(&mut self, _msg: CaptureOrder, _ctx: &mut Context<Self>) -> Self::Result {
        if self.orders_waiting.is_empty() {
            return;
        }
        let order = self.orders_waiting.remove(0);
        let id = Uuid::new_v4().to_string();
        if rand::thread_rng().gen_range(0.0..1.0) <= 0.1 {
            let output = format!(" Order: {:?} aborted, card declined", id);
            println!("[GTW]{}", output.red());
            #[cfg(not(test))]
            if let Err(err) = _ctx.address().try_send(ProcessNewOrder()) {
                println!("Failed to capture order: {:?}", err);
                return;
            }
            return;
        }
        self.orders_captured.insert(id.clone(), order.clone());
        self.send_backup();
        let id_clone_output = id.clone();
        let output = format!(" Order: {:?} captured", id_clone_output);
        println!("[GTW]{}", output.green());
        self.check_robot_connection_and_send_order(id, order);
        self.process_new_order(_ctx);
    }
}

/// ConfirmOrder is a message that tells the PaymentsGateway actor to confirm the payment of an order.
/// This is made by removing the order from the orders_captured hashmap.
/// If the hashmap is empty, it will print a message saying that all orders have been processed.
#[derive(Message)]
#[rtype(result = "()")]
pub struct ConfirmOrder {
    id: String,
}

impl ConfirmOrder {
    pub fn new(id: String) -> ConfirmOrder {
        ConfirmOrder { id }
    }
}

impl Handler<ConfirmOrder> for PaymentsGateway {
    type Result = ();

    fn handle(&mut self, msg: ConfirmOrder, _ctx: &mut Context<Self>) -> Self::Result {
        self.orders_captured.remove(&msg.id);
        let output = format!(" Order: {:?} confirmed", msg.id);
        println!("[GTW]{}", output.bright_cyan());
        self.check_all_processed();
    }
}

/// AbortOrder is a message that tells the PaymentsGateway actor to abort the payment of an order.
/// This is made by removing the order from the orders_captured hashmap.
/// If the hashmap is empty, it will print a message saying that all orders have been processed.
#[derive(Message)]
#[rtype(result = "()")]
pub struct AbortOrder {
    id: String,
    error: String,
}

impl AbortOrder {
    pub fn new(id: String, error: String) -> AbortOrder {
        AbortOrder { id, error }
    }
}

impl Handler<AbortOrder> for PaymentsGateway {
    type Result = ();

    fn handle(&mut self, msg: AbortOrder, _ctx: &mut Context<Self>) -> Self::Result {
        self.orders_captured.remove(&msg.id);
        let output = format!("[GTW] Order: {:?} aborted, reason: {:?}", msg.id, msg.error);
        println!("{}", output.red());
        self.check_all_processed();
    }
}

/// This message is used to register the robot connection handler.
#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterRobotConnection {
    robot_connection_handler: Addr<RobotConnectionHandler>,
}

impl RegisterRobotConnection {
    pub fn new(robot_connection_handler: Addr<RobotConnectionHandler>) -> RegisterRobotConnection {
        RegisterRobotConnection {
            robot_connection_handler,
        }
    }
}

impl Handler<RegisterRobotConnection> for PaymentsGateway {
    type Result = ();

    fn handle(&mut self, msg: RegisterRobotConnection, _ctx: &mut Context<Self>) -> Self::Result {
        self.robot_connection_handler = Some(msg.robot_connection_handler);
        if !self.orders_pending_to_prepare.is_empty() {
            _ctx.address().do_send(SendPendingOrdersToRobot());
        }
    }
}

/// This message is used to register the screen connection sender.
/// It will send a SendMyBackup message to the screen connection sender.
#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterScreenConnection {
    screen_connection_sender: Addr<ScreenConnectionSender>,
}

impl RegisterScreenConnection {
    pub fn new(screen_connection_sender: Addr<ScreenConnectionSender>) -> RegisterScreenConnection {
        RegisterScreenConnection {
            screen_connection_sender,
        }
    }
}

impl Handler<RegisterScreenConnection> for PaymentsGateway {
    type Result = ();

    fn handle(&mut self, msg: RegisterScreenConnection, _ctx: &mut Context<Self>) -> Self::Result {
        self.screen_connection_sender = Some(msg.screen_connection_sender);
        if let Some(sender) = self.screen_connection_sender.as_ref() {
            sender.do_send(SendMyBackup::new(
                self.orders_waiting.clone(),
                self.orders_captured.clone(),
                self.orders_pending_to_prepare.clone(),
                self.id,
            ))
        }
    }
}

/// This message is used to send the pending orders to the robot leader.
/// It will send the orders that are pending to be prepared to the robot leader.
/// This occurs when the gateway processes orders and the robot connection handler is not active.
#[derive(Message)]
#[rtype(result = "()")]
pub struct SendPendingOrdersToRobot();
impl Handler<SendPendingOrdersToRobot> for PaymentsGateway {
    type Result = ();

    fn handle(&mut self, _msg: SendPendingOrdersToRobot, _ctx: &mut Context<Self>) -> Self::Result {
        if self.robot_connection_handler.is_none()
            || !self
                .robot_connection_handler
                .clone()
                .expect("This should never happen")
                .connected()
        {
            return;
        }
        for (id, order) in self.orders_pending_to_prepare.clone() {
            if let Some(handler) = self.robot_connection_handler.as_ref() {
                if let Err(err) = handler.try_send(SendOrderToRobotLeader::new(order, id, self.id))
                {
                    println!("Failed to send order to robot leader: {:?}", err);
                }
            }
        }

        self.orders_pending_to_prepare.clear();
    }
}

/// This message is used to send a request to the screen connection sender.
/// When a screen does not have connection with robot leader, it will send a request to the screen connection sender.
#[derive(Message)]
#[rtype(result = "()")]
pub struct SendRequestFromScreen {
    id: usize,
}

impl SendRequestFromScreen {
    pub fn new(id: usize) -> SendRequestFromScreen {
        SendRequestFromScreen { id }
    }
}

impl Handler<SendRequestFromScreen> for PaymentsGateway {
    type Result = ();

    fn handle(&mut self, msg: SendRequestFromScreen, _ctx: &mut Context<Self>) -> Self::Result {
        if self.robot_connection_handler.is_none()
            || !self
                .robot_connection_handler
                .clone()
                .expect("This should never happen")
                .connected()
        {
        } else if let Some(handler) = self.robot_connection_handler.as_ref() {
            if handler
                .try_send(SendRequestToRobotLeader::new(msg.id))
                .is_err()
            {}
        }
    }
}

/// This message is used to handle a backup from a screen.
/// When the previous screen disconnects, the gateway will handle the orders of it.
#[derive(Message)]
#[rtype(result = "()")]
pub struct HandleBackUp {
    orders_to_process: Vec<Order>,
    orders_processing: HashMap<String, Order>,
    orders_pending_to_prepare: Vec<(String, Order)>,
    screen_backup_id: Option<usize>,
}

impl HandleBackUp {
    pub fn new(
        orders_to_process: Vec<Order>,
        orders_processing: HashMap<String, Order>,
        orders_pending_to_prepare: Vec<(String, Order)>,
        screen_backup_id: Option<usize>,
    ) -> HandleBackUp {
        HandleBackUp {
            orders_to_process,
            orders_processing,
            screen_backup_id,
            orders_pending_to_prepare,
        }
    }
}

impl Handler<HandleBackUp> for PaymentsGateway {
    type Result = ();

    fn handle(&mut self, msg: HandleBackUp, _ctx: &mut Context<Self>) -> Self::Result {
        if msg.screen_backup_id.is_none() {
            return;
        }
        println!("[GTW]{}", "Handling backup".bright_yellow());
        self.orders_waiting.extend(msg.orders_to_process);
        self.orders_captured.extend(msg.orders_processing);
        self.orders_pending_to_prepare
            .extend(msg.orders_pending_to_prepare.clone());
        if let Some(screen_backup_id) = msg.screen_backup_id {
            if let Some(screen_connection_sender) = self.screen_connection_sender.clone() {
                screen_connection_sender.do_send(SendMyBackup::new(
                    self.orders_waiting.clone(),
                    self.orders_captured.clone(),
                    self.orders_pending_to_prepare.clone(),
                    screen_backup_id,
                ))
            }
        }
        if let Some(robot_connection_handler) = self.robot_connection_handler.clone() {
            robot_connection_handler.do_send(AskRobotForScreenOrders::new(
                self.id,
                msg.screen_backup_id.expect("this should never fail"),
            ))
        }
        #[cfg(not(test))]
        if _ctx.address().try_send(ProcessNewOrder()).is_err() {
            println!("Error sending ProcessNewOrder");
        }
    }
}

/// Starts processing orders if there are any waiting.
#[derive(Message)]
#[rtype(result = "()")]
pub struct StartProcessingIfWaiting();

impl Handler<StartProcessingIfWaiting> for PaymentsGateway {
    type Result = ();

    fn handle(&mut self, _msg: StartProcessingIfWaiting, _ctx: &mut Context<Self>) -> Self::Result {
        if !self.orders_waiting.is_empty() {
            return;
        }
        #[cfg(not(test))]
        _ctx.address().do_send(ProcessNewOrder());
    }
}

/// This is the message that the CaptureNewOrder handler will receive. It is only for testing purposes.
#[cfg(test)]
#[derive(Message)]
#[rtype(result = "Option<(String, Order)>")]
pub struct CaptureNewOrder {
    probability: f32,
    key: String,
}

#[cfg(test)]
impl CaptureNewOrder {
    pub fn new(probability: f32, key: String) -> CaptureNewOrder {
        CaptureNewOrder {
            probability: probability,
            key: key,
        }
    }
}

#[cfg(test)]
impl Handler<CaptureNewOrder> for PaymentsGateway {
    type Result = Option<(String, Order)>;

    fn handle(&mut self, msg: CaptureNewOrder, _ctx: &mut Context<Self>) -> Self::Result {
        if self.orders_waiting.is_empty() {
            return None;
        }
        let order = self.orders_waiting.remove(0);
        if msg.probability <= 0.2 {
            return None;
        }
        self.orders_captured.insert(msg.key.clone(), order.clone());
        Some((msg.key, order))
    }
}

pub struct SendBackupToNewScreen();

impl Message for SendBackupToNewScreen {
    type Result = ();
}

impl Handler<SendBackupToNewScreen> for PaymentsGateway {
    type Result = ();

    fn handle(&mut self, _msg: SendBackupToNewScreen, _ctx: &mut Context<Self>) -> Self::Result {
        if let Some(sender) = self.screen_connection_sender.clone() {
            sender.do_send(SendMyBackup::new(
                self.orders_waiting.clone(),
                self.orders_captured.clone(),
                self.orders_pending_to_prepare.clone(),
                self.id,
            ));
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::common::flavor_id::FlavorID;

    use super::*;
    #[actix::test]
    async fn test_payments_gateway_receives_orders_from_order_reader_successfully() {
        let payments_gateway = PaymentsGateway::new(0).start();
        let orders = vec![Order::new_cucurucho(FlavorID::Chocolate)];
        let orders_waiting = ReceiveOrders::new(orders.clone());
        let orders_received = payments_gateway
            .send(orders_waiting)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(orders_received, orders);
    }

    #[actix::test]
    async fn test_payments_gateway_captures_a_new_order_if_card_is_valid() {
        let payments_gateway = PaymentsGateway::new(0).start();
        let orders_waiting = ReceiveOrders::new(vec![Order::new_cucurucho(FlavorID::Chocolate)]);
        payments_gateway.do_send(orders_waiting);
        let next_order = payments_gateway
            .send(CaptureNewOrder::new(0.5, "id1".to_string()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(next_order.1, Order::new_cucurucho(FlavorID::Chocolate));
        assert_eq!(next_order.0, "id1");
    }

    #[actix::test]
    async fn test_payments_gateway_does_not_capture_a_new_order_if_card_is_invalid() {
        let payments_gateway = PaymentsGateway::new(0).start();
        let orders_waiting = ReceiveOrders::new(vec![Order::new_cucurucho(FlavorID::Chocolate)]);
        payments_gateway.do_send(orders_waiting);
        let next_order = payments_gateway
            .send(CaptureNewOrder::new(0.1, "id2".to_string()))
            .await;
        assert_eq!(next_order, Ok(None));
    }

    #[actix::test]
    async fn test_payments_gateway_confirms_order_and_deletes_from_orders_captured() {
        let payments_gateway = PaymentsGateway::new(0).start();
        let orders_waiting = ReceiveOrders::new(vec![Order::new_cucurucho(FlavorID::Chocolate)]);
        payments_gateway.do_send(orders_waiting);
        let next_order = payments_gateway
            .send(CaptureNewOrder::new(0.5, "id3".to_string()))
            .await
            .unwrap()
            .unwrap();
        let _ = payments_gateway.send(ConfirmOrder::new(next_order.0)).await;
        let orders_received = payments_gateway.send(GetOrdersWaiting()).await.unwrap();
        assert_eq!(orders_received, vec![]);
    }

    #[actix::test]
    async fn test_payments_gateway_aborts_order_and_deletes_from_orders_captured() {
        let payments_gateway = PaymentsGateway::new(0).start();
        let orders_waiting = ReceiveOrders::new(vec![Order::new_cucurucho(FlavorID::Chocolate)]);
        payments_gateway.do_send(orders_waiting);
        let next_order = payments_gateway
            .send(CaptureNewOrder::new(0.5, "id4".to_string()))
            .await
            .unwrap()
            .unwrap();
        let _ = payments_gateway
            .send(AbortOrder::new(next_order.0, "error".to_string()))
            .await;
        let orders_received = payments_gateway.send(GetOrdersWaiting()).await.unwrap();
        assert_eq!(orders_received, vec![]);
    }
}
