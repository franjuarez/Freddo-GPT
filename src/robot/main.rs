use actix::prelude::*;
use tp2::robot::messages::{self, JoinRing};
use tp2::robot::order_manager::OrderManager;
use tp2::robot::order_preparer::OrderPreparer;
use tp2::robot::robot_connection_handler::RobotConnectionHandler;
use tp2::robot::utils::print_send_error;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let system = System::new();

    system.block_on(async {
        let id = match args[1].parse::<usize>() {
            Ok(id) => id,
            Err(_) => {
                println!("Error: Invalid Robot ID");
                return;
            }
        };

        let order_preparer: Addr<OrderPreparer> = OrderPreparer::new().start();
        let o_manager: Addr<OrderManager> = OrderManager::new(order_preparer.clone(), id).start();

        let robot_connection_handler =
            RobotConnectionHandler::create(|_| RobotConnectionHandler::new(o_manager.clone(), id));
        if let Err(e) = o_manager.try_send(messages::SetRobotConnectionHandler {
            rch_address: robot_connection_handler.clone(),
        }) {
            print_send_error("[Main]", "SetRobotConnecionHandler", &e.to_string());
        }
        if let Err(e) = order_preparer.try_send(messages::SetOrderManager {
            order_manager: o_manager.clone(),
        }) {
            print_send_error("[Main]", "SetOrderManager", &e.to_string());
        }
        if let Err(e) = robot_connection_handler.try_send(JoinRing()) {
            print_send_error("[Main]", "JoinRing", &e.to_string());
        }
    });

    if let Err(e) = system.run() {
        println!("Error Running System: {:?}", e);
    }
}
