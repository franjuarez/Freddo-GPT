#[cfg(test)]
use actix::Message;
use actix::{Actor, Addr, AsyncContext, Context, Handler};
use actix::{ContextFutureSpawner, WrapFuture};
use colored::*;
use std::collections::HashMap;
use tokio::sync::mpsc::{self};

use crate::common::flavor_id::FlavorID;
use crate::robot::flavor_token::FlavorToken;
use crate::robot::messages::{
    GetNewOrder, GetTokenBack, GetTokenBackup, OrderAborted, OrderPrepared, ScoopFlavor,
    SendTokenBackup, SetRobotConnectionHandler, TransferToken,
};
use crate::robot::order_preparer::OrderPreparer;
use crate::robot::robot_connection_handler::RobotConnectionHandler;
use crate::robot::token_backup::TokenBackup;
use crate::robot::utils::{print_send_error, token_lost_timeout};

use super::messages::{AbortCurrentOrder, TimerWentOff};
use super::utils::INITIAL_AMOUNT;

/// Actor that manages the order, it receives the order from the RCH and sends the tokens needed to the OrderPreparer
/// If it receives a token, it checks if it can serve the order, and sends it to the OrderPreparer if it can, if not, it sends it back to the RCH
/// It also sends the tokens back to the RCH when the order is ready or aborted
/// When the timer goes off, it is alerted of one or more lost tokens, and starts the recovery process
pub struct OrderManager {
    flavors_needed: Vec<(FlavorID, usize)>,
    order_id: String,
    scooping: bool,
    aborted: bool,
    order_preparer: Addr<OrderPreparer>,
    robot_connection_handler: Option<Addr<RobotConnectionHandler>>,
    tokens_backup: HashMap<FlavorID, FlavorToken>,
    sender: Option<mpsc::Sender<usize>>,
    rch_id: usize,
}

impl Actor for OrderManager {
    type Context = Context<Self>;
}

impl OrderManager {
    pub fn new(order_preparer: Addr<OrderPreparer>, rch_id: usize) -> Self {
        Self {
            flavors_needed: vec![],
            order_id: String::new(),
            scooping: false,
            aborted: false,
            order_preparer,
            robot_connection_handler: None,
            tokens_backup: HashMap::new(),
            sender: None,
            rch_id,
        }
    }

    /// Returns the token to the RCH
    fn return_token(&mut self, t: FlavorToken) {
        self.tokens_backup.insert(t.get_id(), t);
        match self.robot_connection_handler {
            Some(ref rch) => {
                if let Err(e) = rch.try_send(GetTokenBack { flavor_token: t }) {
                    print_send_error("[OM]", "GetTokenBack", &e.to_string());
                }
            }
            None => println!(
                "{}",
                "[OM] Error: There is not a RCH to give the token to".red()
            ),
        }
    }

    /// Sends the order prepared message to the RCH
    fn send_order_prepared(&mut self, result: bool) {
        self.end_timer();
        let line = format!("[OM] Order {} prepared successfully!", self.order_id);
        println!("{}", line.black().on_bright_yellow());

        match self.robot_connection_handler {
            Some(ref rch) => {
                if let Err(e) = rch.try_send(OrderPrepared {
                    order_result: result,
                    id: self.order_id.clone(),
                }) {
                    print_send_error("[OM]", "OrderPrepared", &e.to_string());
                }
            }
            None => println!(
                "{}",
                "[OM] Error: There is not a RCH to give order to".red()
            ),
        }
    }

    /// Sends the order aborted message to the RCH
    fn send_order_aborted(&mut self, result: bool, flavor_id: FlavorID) {
        self.end_timer();
        let line = format!("[OM] Order {} aborted!", self.order_id);
        println!("{}", line.on_bright_red().black());

        match self.robot_connection_handler {
            Some(ref rch) => {
                if let Err(e) = rch.try_send(OrderAborted {
                    order_result: result,
                    id: self.order_id.clone(),
                    flavor: flavor_id,
                }) {
                    print_send_error("[OM]", "OrderAborted", &e.to_string());
                }
            }
            None => println!(
                "{}",
                "[OM] Error: There is not a RCH to give order to".red()
            ),
        }
    }

    /// Ends the timer
    fn end_timer(&mut self) {
        if let Some(send_ref) = &self.sender {
            if send_ref.clone().try_send(1).is_err() {
                // println!("[OM] Error: Could not send message to timer");
            }
        }
    }

    /// Updates the timer, since the OM received a token it was expecting
    fn update_timer(&mut self) {
        if let Some(send_ref) = &self.sender {
            if let Err(e) = send_ref.clone().try_send(0) {
                print_send_error("[OM]", "Channel Communication Update", &e.to_string());
            }
        }
    }

    /// Gets the index of the flavor in the flavors_needed vector
    fn get_flavor_index(&mut self, token: FlavorToken) -> isize {
        for (i, (id, _)) in self.flavors_needed.iter().enumerate() {
            if *id == token.get_id() {
                return i as isize;
            }
        }
        -1
    }

    /// Checks if the flavor is needed in the order, and if it can serve the amount needed
    fn check_needed(&mut self, token: FlavorToken) -> usize {
        if self.scooping {
            return 0;
        }

        let i = self.get_flavor_index(token);

        if i == -1 {
            return 0;
        }

        let amount_needed = self.flavors_needed[i as usize];

        if !token.can_serve(amount_needed.1) {
            let line = format!("[OM] Not enough flavor left in {}!", token.get_id());
            println!("{}", line.blue());
            self.flavors_needed.clear();
            self.send_order_aborted(false, token.get_id());
            return 0;
        }

        self.scooping = true;
        self.flavors_needed.remove(i as usize);

        amount_needed.1
    }
}

/// Handles the TransferToken message, it receives a token from the RCH and checks if it can serve the order
/// If it can, it sends the token to the OrderPreparer, if not, it sends it back to the RCH
impl Handler<TransferToken> for OrderManager {
    type Result = ();
    fn handle(&mut self, msg: TransferToken, _ctx: &mut Self::Context) -> Self::Result {
        let token = msg.flavor_token;
        self.tokens_backup.insert(token.get_id(), token);

        let amount_needed = self.check_needed(token);

        if amount_needed == 0 {
            self.return_token(token)
        } else {
            self.update_timer();
            if let Err(e) = self.order_preparer.try_send(ScoopFlavor {
                flavor_token: token,
                amount: amount_needed,
            }) {
                print_send_error("[OM]", "ScoopFlavor", &e.to_string());
            }
        }
    }
}

/// Handles the GetTokenBack message, it receives a token from the OrderPreparer and returns it to the RCH,
/// If the order is ready, it sends the OrderPrepared message to the RCH
impl Handler<GetTokenBack> for OrderManager {
    type Result = ();
    fn handle(&mut self, msg: GetTokenBack, _ctx: &mut Self::Context) -> Self::Result {
        let token = msg.flavor_token;
        self.scooping = false;

        self.return_token(token);

        if self.flavors_needed.is_empty() && !self.aborted {
            self.send_order_prepared(true);
        }
    }
}

/// Handles the AbortCurrentOrder message, it aborts the current order
impl Handler<AbortCurrentOrder> for OrderManager {
    type Result = ();

    fn handle(&mut self, _msg: AbortCurrentOrder, _ctx: &mut Self::Context) -> Self::Result {
        self.flavors_needed.clear();
        self.scooping = false;
        self.aborted = true;
        self.end_timer();
    }
}

/// Handles the SetRobotConnectionHandler message, it sets the RCH address
impl Handler<SetRobotConnectionHandler> for OrderManager {
    type Result = ();

    fn handle(&mut self, msg: SetRobotConnectionHandler, _ctx: &mut Self::Context) -> Self::Result {
        self.robot_connection_handler = Some(msg.rch_address);
    }
}

/// Handles the GetNewOrder message, it receives a new order from the RCH and starts the order process, starting the timer
impl Handler<GetNewOrder> for OrderManager {
    type Result = ();

    fn handle(&mut self, msg: GetNewOrder, ctx: &mut Self::Context) -> Self::Result {
        self.flavors_needed = msg.new_order.get_flavors();
        self.order_id = msg.id;
        let line = format!("[OM] Got a new order with {:?}", self.flavors_needed);
        println!("{}", line.purple());

        let (sndr, receiver) = mpsc::channel::<usize>(10);

        self.sender = Some(sndr);
        let addr = ctx.address();

        async move { token_lost_timeout(receiver, addr).await }
            .into_actor(self)
            .spawn(ctx);
    }
}

/// Handles the GetTokenBackup message, it receives a token backup from the RCH and updates the token backup
impl Handler<GetTokenBackup> for OrderManager {
    type Result = ();
    fn handle(&mut self, msg: GetTokenBackup, _ctx: &mut Self::Context) -> Self::Result {
        let mut token_backup = msg.token_backup;

        if let Some(flavor_token) = self.tokens_backup.get(&token_backup.get_flavor_id()) {
            token_backup.change_amount_if_necessary(flavor_token.get_amnt());
        }

        match self.robot_connection_handler {
            Some(ref rch) => {
                if let Err(e) = rch.try_send(SendTokenBackup {
                    token_backup: token_backup.clone(),
                }) {
                    print_send_error("[OM]", "SendTockenBackup", &e.to_string());
                }
            }
            None => println!(
                "{}",
                "[OM] Error: There is not a RCH to give the token to".red()
            ),
        }
    }
}

/// Handles the TimerWentOff message, it is alerted that one or more tokens were lost, and starts the recovery process
impl Handler<TimerWentOff> for OrderManager {
    type Result = ();

    fn handle(&mut self, _msg: TimerWentOff, ctx: &mut Self::Context) -> Self::Result {
        match self.robot_connection_handler {
            Some(ref rch) => {
                for (flavor_id, _) in &self.flavors_needed {
                    println!("[OM]: Lost Token: {}", flavor_id);
                    let amount: usize;
                    if let Some(amnt) = self.tokens_backup.get(flavor_id) {
                        amount = amnt.get_amnt();
                    } else {
                        amount = INITIAL_AMOUNT;
                    }
                    let token_backup = TokenBackup::new(*flavor_id, amount, self.rch_id);

                    if let Err(e) = rch.try_send(SendTokenBackup {
                        token_backup: token_backup.clone(),
                    }) {
                        print_send_error("[OM]", "SendTockenBackup", &e.to_string());
                    }
                }
            }
            None => println!(
                "{}",
                "[OM] Error: There is not a RCH to give the token to".red()
            ),
        }

        let (sndr, receiver) = mpsc::channel::<usize>(10);

        self.sender = Some(sndr);
        let addr = ctx.address();

        async move { token_lost_timeout(receiver, addr).await }
            .into_actor(self)
            .spawn(ctx);
    }
}

#[cfg(test)]
#[derive(Message)]
#[rtype(result = "Vec<(FlavorID, usize)>")]
pub struct GetFlavorsNeeded();

#[cfg(test)]
impl Handler<GetFlavorsNeeded> for OrderManager {
    type Result = Vec<(FlavorID, usize)>;
    fn handle(&mut self, _msg: GetFlavorsNeeded, _ctx: &mut Self::Context) -> Self::Result {
        self.flavors_needed.clone()
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::common::order::Order;

    #[actix::test]
    async fn order_arrived_properly() {
        let order_preparer: Addr<OrderPreparer> = OrderPreparer::new().start();
        let o_manager: Addr<OrderManager> = OrderManager::new(order_preparer.clone(), 0).start();
        o_manager
            .send(GetNewOrder {
                new_order: Order::new_cucurucho(FlavorID::Chocolate),
                id: "1".to_string(),
            })
            .await
            .unwrap();
        let flavors_needed = o_manager.send(GetFlavorsNeeded()).await.unwrap();
        assert_eq!(flavors_needed, vec![(FlavorID::Chocolate, 250)]);
    }

    #[actix::test]
    async fn order_arrived_and_token_arrived() {
        let order_preparer: Addr<OrderPreparer> = OrderPreparer::new().start();
        let o_manager: Addr<OrderManager> = OrderManager::new(order_preparer.clone(), 0).start();
        o_manager
            .send(GetNewOrder {
                new_order: Order::new_cucurucho(FlavorID::Chocolate),
                id: "1".to_string(),
            })
            .await
            .unwrap();
        let flavors_needed = o_manager.send(GetFlavorsNeeded()).await.unwrap();
        assert_eq!(flavors_needed, vec![(FlavorID::Chocolate, 250)]);
        o_manager
            .send(TransferToken {
                flavor_token: FlavorToken::new(FlavorID::Chocolate, 250),
            })
            .await
            .unwrap();
        let flavors_needed = o_manager.send(GetFlavorsNeeded()).await.unwrap();
        assert_eq!(flavors_needed, vec![]);
    }

    #[actix::test]
    async fn order_arrived_and_incorrect_token_arrived_and_token_returned() {
        let order_preparer: Addr<OrderPreparer> = OrderPreparer::new().start();
        let o_manager: Addr<OrderManager> = OrderManager::new(order_preparer.clone(), 0).start();
        o_manager
            .send(GetNewOrder {
                new_order: Order::new_cucurucho(FlavorID::Chocolate),
                id: "1".to_string(),
            })
            .await
            .unwrap();
        let flavors_needed = o_manager.send(GetFlavorsNeeded()).await.unwrap();
        assert_eq!(flavors_needed, vec![(FlavorID::Chocolate, 250)]);
        o_manager
            .send(GetTokenBack {
                flavor_token: FlavorToken::new(FlavorID::Mint, 250),
            })
            .await
            .unwrap();
        let flavors_needed = o_manager.send(GetFlavorsNeeded()).await.unwrap();
        assert_eq!(flavors_needed, vec![(FlavorID::Chocolate, 250)]);
    }
}
