use actix::{Actor, Addr, AsyncContext, Context, Handler, Message};
use colored::*;
use std::time::Duration;

use crate::robot::flavor_token::FlavorToken;
use crate::robot::messages::{GetTokenBack, ScoopFlavor, SetOrderManager};
use crate::robot::order_manager::OrderManager;
use crate::robot::utils::print_send_error;

//Scoop time is calculated as SCOOP_TIME_FACTOR * amount needed
pub const SCOOP_TIME_FACTOR: usize = 10;

#[derive(Message)]
#[rtype(result = "()")]
struct ReturnToken(pub FlavorToken);

/// OrderPreparer is an actor that serves the ice cream scoops
#[derive(Default)]
pub struct OrderPreparer {
    order_manager: Option<Addr<OrderManager>>,
}

impl Actor for OrderPreparer {
    type Context = Context<Self>;
}

impl OrderPreparer {
    pub fn new() -> Self {
        Self {
            order_manager: None,
        }
    }
}

impl Handler<SetOrderManager> for OrderPreparer {
    type Result = ();

    fn handle(&mut self, msg: SetOrderManager, _ctx: &mut Self::Context) -> Self::Result {
        self.order_manager = Some(msg.order_manager);
    }
}

/// Handles the ScoopFlavor message, serving the ice cream
impl Handler<ScoopFlavor> for OrderPreparer {
    type Result = ();

    fn handle(&mut self, msg: ScoopFlavor, _ctx: &mut Self::Context) -> Self::Result {
        let mut flavor = msg.flavor_token;
        let amnt = msg.amount;

        let line = format!("[OP] Scooping {} grams of {}", amnt, flavor.get_id());
        println!("{}", line.bright_blue());

        flavor.serve(amnt);

        _ctx.notify_later(
            ReturnToken(flavor),
            Duration::from_millis((amnt * SCOOP_TIME_FACTOR) as u64),
        );
    }
}

/// Handles the ReturnToken message, returning the token to the OrderManager
impl Handler<ReturnToken> for OrderPreparer {
    type Result = ();

    fn handle(&mut self, msg: ReturnToken, _ctx: &mut Self::Context) -> Self::Result {
        let token = msg.0;

        let line = format!(
            "[OP] Returning {} Token with {} grams",
            token.get_id(),
            token.get_amnt()
        );
        println!("{}", line.bright_purple());

        match self.order_manager {
            Some(ref om) => {
                if let Err(e) = om.try_send(GetTokenBack {
                    flavor_token: token,
                }) {
                    print_send_error("[OP]", "GetTokenBack", &e.to_string());
                }
            }
            None => println!(
                "{}",
                "[OP] Error: There is not an OrderManager to give the token to ".red()
            ),
        }
    }
}
