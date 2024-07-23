use std::{
    fs::File,
    io::{BufRead, BufReader},
};

use crate::common::order::Order;
use actix::prelude::*;

use super::payments_gateway::ReceiveOrders;
/// OrderReader is an actor that reads a file with orders, processes them and sends them to the PaymentsGateway actor.
///
/// # Attributes
///
/// * `orders` - A vector of orders read from the file.
/// * `file_name` - The name of the file to read.
///
///
/// # Panics
///
/// If the file does not exist, the actor will panic.
///
/// # Errors
///
/// If the file is empty, the actor will return an empty vector.
///
/// If the file contains an invalid order, the actor will ignore it.
///
pub struct OrderReader {
    orders: Vec<Order>,
    file_name: String,
    payments_gateway: Recipient<ReceiveOrders>,
}

impl OrderReader {
    pub fn new(file_name: String, payments_gateway: Recipient<ReceiveOrders>) -> OrderReader {
        OrderReader {
            orders: Vec::new(),
            file_name,
            payments_gateway,
        }
    }
}

impl Actor for OrderReader {
    type Context = Context<Self>;
}

// /// ReadOrders is a message that tells the OrderReader actor to read the orders from the file.
// struct ReadOrders();

#[derive(Message)]
#[rtype(result = "Result<Vec<Order>, std::io::Error>")]
pub struct ReadOrders();

impl Handler<ReadOrders> for OrderReader {
    type Result = Result<Vec<Order>, std::io::Error>;

    fn handle(&mut self, _msg: ReadOrders, _ctx: &mut Context<Self>) -> Self::Result {
        let file = File::open(&self.file_name)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            if let Ok(order) = serde_json::from_str(&line?) {
                self.orders.push(order);
            }
        }
        match _ctx.address().try_send(SendOrdersToPaymentsGateway()) {
            Ok(_) => (),
            Err(_) => println!("Error sending orders to PaymentsGateway"),
        };
        Ok(self.orders.clone())
    }
}

/// SendOrdersToPaymentsGateway is a message that tells the OrderReader actor to send the orders to the PaymentsGateway actor.
#[derive(Message)]
#[rtype(result = "Vec<Order>")]
pub struct SendOrdersToPaymentsGateway();

impl Handler<SendOrdersToPaymentsGateway> for OrderReader {
    type Result = Vec<Order>;

    fn handle(
        &mut self,
        _msg: SendOrdersToPaymentsGateway,
        _ctx: &mut Context<Self>,
    ) -> Self::Result {
        match self
            .payments_gateway
            .try_send(ReceiveOrders::new(self.orders.clone()))
        {
            Ok(_) => (),
            Err(_) => println!("Error sending orders to PaymentsGateway"),
        };
        self.orders.clone()
    }
}

#[cfg(test)]
mod tests {

    use crate::{common::flavor_id::FlavorID, screen::payments_gateway::PaymentsGateway};

    use super::*;

    #[actix::test]
    async fn test_order_reader_raises_error_when_file_not_found() {
        let payments_gateway_recipient = PaymentsGateway::new(0).start().recipient();
        let order_reader = OrderReader::new(
            "non_existent_file.txt".to_string(),
            payments_gateway_recipient,
        )
        .start();
        let error = order_reader.send(ReadOrders()).await;
        assert!(error.unwrap().is_err());
    }

    #[actix::test]
    async fn test_order_reader_reads_empty_file() {
        let payments_gateway_recipient = PaymentsGateway::new(0).start().recipient();
        let order_reader = OrderReader::new(
            "./src/orders_samples/order_sample_empty.txt".to_string(),
            payments_gateway_recipient,
        )
        .start();
        let orders = order_reader.send(ReadOrders()).await;
        assert_eq!(orders.unwrap().unwrap(), vec![]);
    }

    #[actix::test]
    async fn test_order_reader_reads_file_of_one_order() {
        let payments_gateway_recipient = PaymentsGateway::new(0).start().recipient();
        let order_reader = OrderReader::new(
            "./src/orders_samples/orders_sample_1.txt".to_string(),
            payments_gateway_recipient,
        )
        .start();
        let orders = order_reader.send(ReadOrders()).await;
        assert_eq!(
            orders.unwrap().unwrap(),
            vec![Order::new_cucurucho(FlavorID::Chocolate)]
        );
    }

    #[actix::test]
    async fn test_order_reader_reads_file_of_two_orders() {
        let payments_gateway_recipient = PaymentsGateway::new(0).start().recipient();
        let order_reader = OrderReader::new(
            "./src/orders_samples/orders_sample_2.txt".to_string(),
            payments_gateway_recipient,
        )
        .start();
        let orders = order_reader.send(ReadOrders()).await;
        assert_eq!(
            orders.unwrap().unwrap(),
            vec![
                Order::new_cucurucho(FlavorID::Chocolate),
                Order::new_cuarto(vec![FlavorID::Chocolate, FlavorID::Vanilla]).unwrap()
            ]
        );
    }

    #[actix::test]
    async fn test_order_reader_ignore_invalid_order() {
        let payments_gateway_recipient = PaymentsGateway::new(0).start().recipient();
        let order_reader = OrderReader::new(
            "./src/orders_samples/order_sample_invalid.txt".to_string(),
            payments_gateway_recipient,
        )
        .start();
        let orders = order_reader.send(ReadOrders()).await;
        assert_eq!(
            orders.unwrap().unwrap(),
            vec![Order::new_cuarto(vec![FlavorID::Chocolate, FlavorID::Vanilla]).unwrap()]
        );
    }

    #[actix::test]
    async fn test_order_reader_sends_orders_to_payments_gateway_correctly() {
        let payments_gateway_recipient = PaymentsGateway::new(0).start().recipient();
        let order_reader = OrderReader::new(
            "./src/orders_samples/orders_sample_2.txt".to_string(),
            payments_gateway_recipient,
        )
        .start();
        order_reader.do_send(ReadOrders());
        let orders_sent = order_reader.send(SendOrdersToPaymentsGateway()).await;
        assert_eq!(
            orders_sent.unwrap(),
            vec![
                Order::new_cucurucho(FlavorID::Chocolate),
                Order::new_cuarto(vec![FlavorID::Chocolate, FlavorID::Vanilla]).unwrap()
            ]
        );
    }
}
