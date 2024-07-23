use actix::prelude::*;
use colored::*;
use std::collections::{HashMap, VecDeque};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::TcpStream;
use tokio_stream::wrappers::LinesStream;

use crate::common::flavor_id::FlavorID;
use crate::config::MAX_NUMBER_OF_SCREENS;
use crate::robot::connections::leader_to_robot_connection::LeaderToRobotConnection;
use crate::robot::connections::leader_to_screen_connection::LeaderToScreenConnection;
use crate::robot::flavor_token::FlavorToken;
use crate::robot::leader_backup::LeaderBackup;
use crate::robot::messages::*;
use crate::robot::order_info::OrderInfo;
use crate::robot::order_waiting::OrderWaiting;
use crate::robot::robot_connection_handler::RobotConnectionHandler;
use crate::robot::utils::*;

pub const INITIAL_TOKENS: &[(FlavorID, usize)] = &[
    (FlavorID::Chocolate, INITIAL_AMOUNT - 2800),
    (FlavorID::Vanilla, INITIAL_AMOUNT),
    (FlavorID::Strawberry, INITIAL_AMOUNT),
    (FlavorID::Mint, INITIAL_AMOUNT),
    (FlavorID::Pistachio, INITIAL_AMOUNT),
    (FlavorID::DulceDeLeche, INITIAL_AMOUNT),
    (FlavorID::Lemon, INITIAL_AMOUNT),
];

/// Actor that represents the Robot Leader, it manages the duties of the robots and the connection with the screens
/// Can be initialized as the first leader or as a backup leader
/// Receives Orders from the Screens, sends them to the RCH to be prepared and then informs the Screen if it was successfull or aborted
pub struct RobotLeader {
    my_id: usize,
    first_leader: bool,
    available_robots: Vec<usize>,
    orders_on_queue: VecDeque<OrderInfo>,
    my_robot: Option<Addr<RobotConnectionHandler>>,
    robots_connections: HashMap<usize, Addr<LeaderToRobotConnection>>,
    robots_orders: HashMap<usize, OrderInfo>,
    screens_connections: HashMap<usize, Addr<LeaderToScreenConnection>>,
    screen_ids: Vec<usize>,
    orders_to_be_sent: Vec<OrderWaiting>,
}

impl Actor for RobotLeader {
    type Context = Context<Self>;

    /// Starts the Leader, if it is the first leader it will start the tokens and connect to all screens
    /// If it is a backup leader it will connect to the robots and screens that were connected to the previous leader
    fn started(&mut self, ctx: &mut Self::Context) {
        start_leader_connection_listener(ctx.address(), self.my_id);
        if self.first_leader {
            self.start_tokens();
            self.setup_all_screen_connections(ctx);
        } else {
            let line = "Creating leader from backup!".to_string();
            println!("{}", line.bright_cyan());
            self.setup_robot_connections(ctx);
            self.setup_screen_connections(ctx, self.screen_ids.clone());
            self.screen_ids.clear();
        }
    }
}

impl RobotLeader {
    /// Creates a new Robot Leader from scratch
    pub fn new(my_id: usize, my_robot: Option<Addr<RobotConnectionHandler>>) -> Self {
        Self {
            my_id,
            first_leader: true,
            available_robots: Vec::new(),
            orders_on_queue: VecDeque::new(),
            my_robot,
            robots_connections: HashMap::new(),
            robots_orders: HashMap::new(),
            screens_connections: HashMap::new(),
            screen_ids: Vec::new(),
            orders_to_be_sent: Vec::new(),
        }
    }

    /// Creates a new Robot Leader from a backup
    pub fn from_backup(
        my_id: usize,
        my_robot: Option<Addr<RobotConnectionHandler>>,
        mut backup: LeaderBackup,
    ) -> Self {
        remove_me_from_backup(&mut backup, my_id);
        Self {
            my_id,
            first_leader: false,
            available_robots: backup.available_robots,
            orders_on_queue: backup.orders_on_queue,
            my_robot,
            robots_connections: HashMap::new(),
            robots_orders: backup.robots_orders,
            screens_connections: HashMap::new(),
            screen_ids: backup.screens,
            orders_to_be_sent: backup.orders_to_be_sent,
        }
    }

    /// Sets up the connections to the screens that are passed as parameters
    fn setup_screen_connections(&mut self, ctx: &mut Context<Self>, ids: Vec<usize>) {
        println!("[RL] Connecting to Screens: {:?}", ids);
        for screen_id in ids {
            let addr = ctx.address().clone();
            async move {
                connect_to_screen(screen_id, addr).await;
            }
            .into_actor(self)
            .wait(ctx);
        }
    }

    /// Sets up the connections to all screens
    fn setup_all_screen_connections(&mut self, ctx: &mut Context<Self>) {
        let screen_ids = (0..MAX_NUMBER_OF_SCREENS).collect::<Vec<usize>>();
        self.setup_screen_connections(ctx, screen_ids);
    }

    /// Sets up the connections to all robots
    fn setup_robot_connections(&mut self, ctx: &mut Context<Self>) {
        let mut robots_ids = self.available_robots.clone();
        robots_ids.extend_from_slice(&self.robots_orders.keys().cloned().collect::<Vec<usize>>());

        let line = format!("[RL] Connecting to Robots: {:?}", robots_ids);
        println!("{}", line.bright_cyan());

        for robot_id in robots_ids {
            let address = ctx.address().clone();
            let my_id = self.my_id;

            if my_id == robot_id {
                continue;
            }

            async move {
                match TcpStream::connect(id_to_robot_addr(robot_id)).await {
                    Ok(stream) => {
                        let (read_half,mut write_half) = stream.into_split();
                        if let Err(e) = write_half.write_all(&[NEW_ROBOT_LEADER as u8]).await {
                            let line = format!("[RL] Error! Could not send new leader robot to robot {}. Error: {}", robot_id, e);
                            println!("{}", line.bright_cyan());
                            return;
                        }
                        if let Err(e) = write_half.write_all(&[my_id as u8]).await {
                            let line = format!("[RL] Error! Could not send new leader ID robot to robot {}. Error: {}", robot_id, e);
                            println!("{}", line.bright_cyan());
                            return;
                        }

                        if let Err(e) = address.try_send(AddNewRobot{robot_id, read_half, write_half, asked: true}) {
                            print_send_error("[RL]", "AddNewRobot", &e.to_string());
                        }
                    },
                    Err(e) => {
                        let line = format!("[RL] Error! Could not connecto to robot: {}. Error: {}", robot_id, e);
                        println!("{}", line.bright_cyan());
                    }
                };
            }
            .into_actor(self)
            .wait(ctx);
        }
    }

    /// Starts the flavor tokens with the initial values
    fn start_tokens(&mut self) {
        let initial_tokens: Vec<FlavorToken> = INITIAL_TOKENS
            .iter()
            .map(|(flavor, amount)| FlavorToken::new(*flavor, *amount))
            .collect();
        if let Some(my_robot) = &self.my_robot {
            if let Err(e) = my_robot.try_send(StartTokens {
                all_tokens: initial_tokens,
            }) {
                print_send_error("[RL]", "StartTokens", &e.to_string());
            }
        } else {
            let line = "RL: Error! No robot connection handler found".to_string();
            println!("{}", line.bright_cyan());
        }
    }

    /// Assigns a new order to a robot
    /// If there are no orders or robots available it will print an error message and do nothing
    fn assign_new_order(&mut self) {
        if self.orders_on_queue.is_empty() || self.available_robots.is_empty() {
            let line = "[RL] No orders or robots available".to_string();
            println!("{}", line.bright_magenta());
            return;
        }
        let order_info = match self.orders_on_queue.pop_front() {
            Some(order) => order,
            None => {
                let line = "[RL] Error! No orders available, but we should have!".to_string();
                println!("{}", line.bright_cyan());
                return;
            }
        };

        let robot_id = match self.available_robots.pop() {
            Some(id) => id,
            None => {
                let line = "[RL] Error! No robots available, but there should be!".to_string();
                println!("{}", line.bright_cyan());
                self.orders_on_queue.push_front(order_info);
                return;
            }
        };

        let robot = match self.robots_connections.get(&robot_id) {
            Some(robot) => robot,
            None => {
                let line = format!(
                    "[RL] Error! Robot {} not found in connections and pushed order {} back into the front",
                    robot_id, order_info.order_id
                );
                println!("{}", line.bright_cyan());
                self.available_robots.push(robot_id);
                self.orders_on_queue.push_front(order_info);
                return;
            }
        };

        if let Err(e) = robot.try_send(SendNewOrder {
            new_order: order_info.order.clone(),
            order_id: order_info.order_id.clone(),
        }) {
            print_send_error("[RL]", "SendNewOrder", &e.to_string());
        }

        let line = format!(
            "[RL] Assigning order {} to Robot {}",
            order_info.order_id, robot_id
        );
        println!("{}", line.bright_green());
        self.robots_orders.insert(robot_id, order_info);
    }

    /// Adds a new order to the queue
    /// If there are robots available it will assign the order to one of them
    fn add_new_order(&mut self, order_info: OrderInfo) {
        self.orders_on_queue.push_back(order_info);
        if !self.available_robots.is_empty() {
            self.assign_new_order();
        } else {
            let line = "[RL] No robots available, order pushed to queue".to_string();
            println!("{}", line.bright_magenta());
        }
    }

    /// Creates a backup with the current state and sends it to all robots
    fn make_and_send_backup(&self) {
        let backup = LeaderBackup::new(
            self.available_robots.clone(),
            self.screen_ids.clone(),
            self.orders_on_queue.clone(),
            self.robots_orders.clone(),
            self.orders_to_be_sent.clone(),
        );
        for robot in self.robots_connections.values() {
            if let Err(e) = robot.try_send(SendLeaderBackup {
                backup: backup.clone(),
            }) {
                print_send_error("[RL]", "SendLeaderBackup", &e.to_string());
            }
        }
    }

    /// Stashes an order to be sent later
    fn stash_order_waiting(
        &mut self,
        id: String,
        order_result: bool,
        screen_id: usize,
        flavor: Option<FlavorID>,
    ) {
        let order = OrderWaiting {
            id: id.clone(),
            order_result,
            screen_id,
            flavor,
        };
        self.orders_to_be_sent.push(order);
    }

    /// Gets the result of an order from a robot and returns the screen to send the result
    fn get_order_result(
        &mut self,
        robot_id: usize,
        order_result: bool,
        flavor: Option<FlavorID>,
    ) -> Option<(Addr<LeaderToScreenConnection>, OrderInfo)> {
        self.available_robots.push(robot_id);
        let order_info = self.robots_orders.remove(&robot_id);

        let order = match order_info {
            Some(order) => order,
            None => {
                let line = format!("[RL] Error! Order not found for robot {}", robot_id);
                println!("{}", line.bright_cyan());
                return None;
            }
        };

        let screen_id = order.screen_id;
        if let Some(screen) = self.screens_connections.get(&screen_id) {
            Some((screen.clone(), order))
        } else {
            let line = format!(
                "[RL] Error! Screen {} not found, saved order to send later",
                screen_id
            );
            println!("{}", line.bright_cyan());
            self.stash_order_waiting(order.order_id, order_result, screen_id, flavor);
            None
        }
    }
}

/// Removes the robot from the backup
fn remove_me_from_backup(backup: &mut LeaderBackup, my_id: usize) {
    backup.available_robots.retain(|&id| id != my_id);
    let my_order = backup.robots_orders.remove(&my_id);
    if let Some(order) = my_order {
        backup.orders_on_queue.push_front(order);
    }
}

/// Adds a new robot to the leader and creates an actor for the connection
impl Handler<AddNewRobot> for RobotLeader {
    type Result = ();

    fn handle(&mut self, msg: AddNewRobot, ctx: &mut Context<Self>) {
        let robot_id = msg.robot_id;
        let asked = msg.asked;
        let addr = ctx.address();

        async move {
            let pipo = LeaderToRobotConnection::create(|own_ctx| {
                let cr = LeaderToRobotConnection::new(addr, msg.robot_id, Some(msg.write_half));
                let lines: LinesStream<BufReader<OwnedReadHalf>> =
                    LinesStream::new(BufReader::new(msg.read_half).lines());
                LeaderToRobotConnection::add_stream(lines, own_ctx);
                cr
            });
            (Some(pipo), robot_id)
        }
        .into_actor(self)
        .map(move |(pipo, rob_id), actor, _ctx| {
            if let Some(pip) = pipo {
                let line = format!("[RL] Connected to Robot {}", rob_id);
                println!("{}", line.bright_cyan());
                actor.robots_connections.insert(rob_id, pip);

                if !asked {
                    actor.available_robots.push(rob_id);
                    actor.assign_new_order();
                    actor.make_and_send_backup();
                }
            }
        })
        .wait(ctx);
    }
}

/// Connects to a new screen
impl Handler<ConnectToScreen> for RobotLeader {
    type Result = ();

    fn handle(&mut self, msg: ConnectToScreen, ctx: &mut Context<Self>) {
        let addr = ctx.address();

        async move {
            connect_to_screen(msg.screen_id, addr).await;
        }
        .into_actor(self)
        .wait(ctx);
    }
}

/// Adds a new screen to the leader and creates an actor for the connection
impl Handler<AddNewScreen> for RobotLeader {
    type Result = ();

    fn handle(&mut self, msg: AddNewScreen, ctx: &mut Context<Self>) {
        let line = format!("[RL] Connected to Screen: {}", msg.screen_id);
        println!("{}", line.bright_cyan());

        let pipo = LeaderToScreenConnection::create(|own_ctx| {
            let lines = LinesStream::new(BufReader::new(msg.read_half).lines());
            let rpc =
                LeaderToScreenConnection::new(msg.screen_id, ctx.address(), Some(msg.write_half));
            LeaderToScreenConnection::add_stream(lines, own_ctx);
            rpc
        });

        self.screens_connections.insert(msg.screen_id, pipo);

        self.screen_ids.push(msg.screen_id);

        self.make_and_send_backup();
    }
}

/// Connects to a new screen that is requested by another screen
impl Handler<ConnectToNewScreen> for RobotLeader {
    type Result = ();

    fn handle(&mut self, msg: ConnectToNewScreen, ctx: &mut Context<Self>) {
        let line = format!(
            "[RL] A Screen requested Leader to connect to new Screen witd id {}",
            msg.screen_id
        );
        println!("{}", line.bright_cyan());

        let address = ctx.address();
        let screen_id = msg.screen_id;

        async move {
            connect_to_screen(screen_id, address).await;
        }
        .into_actor(self)
        .wait(ctx);
    }
}

/// Handles the creation of a new order and assigns it to a robot
impl Handler<CreateNewOrder> for RobotLeader {
    type Result = ();
    fn handle(&mut self, msg: CreateNewOrder, _ctx: &mut Context<Self>) {
        let order_id = msg.id.clone();

        let line = format!("[RL] Assigning order {}", order_id);
        println!("{}", line.bright_magenta());

        let order_info = OrderInfo {
            order: msg.new_order.clone(),
            order_id: order_id.clone(),
            screen_id: msg.screen_id,
        };

        self.add_new_order(order_info.clone());
        self.make_and_send_backup();
    }
}

/// Handles the completion of an order and informs the screen
impl Handler<GetCompletedOrder> for RobotLeader {
    type Result = ();
    fn handle(&mut self, msg: GetCompletedOrder, _ctx: &mut Context<Self>) {
        let robot_id = msg.robot_id;
        let line = format!("[RL] Got Order Completed from Robot {}", robot_id);
        println!("{}", line.bright_green());

        if let Some((screen, order)) = self.get_order_result(robot_id, msg.order_result, None) {
            if let Err(e) = screen.try_send(OrderPrepared {
                order_result: msg.order_result,
                id: order.order_id.clone(),
            }) {
                self.stash_order_waiting(
                    order.order_id.clone(),
                    msg.order_result,
                    order.screen_id,
                    None,
                );
                print_send_error("[RL]", "Sending Order Completed", &e.to_string());
            }

            self.assign_new_order();
            self.make_and_send_backup();
        }
    }
}

/// Handles the abortion of an order and informs the screen
impl Handler<GetAbortedOrder> for RobotLeader {
    type Result = ();
    fn handle(&mut self, msg: GetAbortedOrder, _ctx: &mut Context<Self>) {
        let robot_id = msg.robot_id;
        let line = format!("[RL] Got Order Aborted from Robot {}", robot_id);
        println!("{}", line.bright_green());

        if let Some((screen, order)) =
            self.get_order_result(robot_id, msg.order_result, Some(msg.flavor))
        {
            if let Err(e) = screen.try_send(OrderAborted {
                order_result: msg.order_result,
                id: order.order_id.clone(),
                flavor: msg.flavor,
            }) {
                self.stash_order_waiting(
                    order.order_id.clone(),
                    msg.order_result,
                    order.screen_id,
                    Some(msg.flavor),
                );
                print_send_error("[RL]", "Sending Order Completed", &e.to_string());
            }

            self.assign_new_order();
            self.make_and_send_backup();
        }
    }
}

/// Handles the death of a robot and reassigns the order
impl Handler<RobotDied> for RobotLeader {
    type Result = ();

    fn handle(&mut self, msg: RobotDied, _ctx: &mut Context<Self>) {
        let robot_id = msg.robot_id;
        let line = format!("[RL] Robot {} died! Reassigning order", robot_id);
        println!("{}", line.bright_cyan());

        self.available_robots.retain(|&id| id != robot_id);
        if let Some(order) = self.robots_orders.remove(&robot_id) {
            self.orders_on_queue.push_front(order);
            self.assign_new_order();
        }
        self.robots_connections.remove(&robot_id);
        self.make_and_send_backup();
    }
}

/// Handles the death of a screen and reassigns the order
impl Handler<ScreenDied> for RobotLeader {
    type Result = ();

    fn handle(&mut self, msg: ScreenDied, _ctx: &mut Context<Self>) {
        let screen_id = msg.screen_id;
        let line = format!("[RL] Screen {} died!", screen_id);
        println!("{}", line.bright_cyan());

        self.screen_ids.retain(|&id| id != screen_id);
        self.screens_connections.remove(&screen_id);
        self.make_and_send_backup();
    }
}

/// Handles the change of a screen due to a failure in one of them
impl Handler<ChangeScreen> for RobotLeader {
    type Result = ();

    fn handle(&mut self, msg: ChangeScreen, _ctx: &mut Context<Self>) {
        let original_screen_id = msg.original_screen_id;
        let new_screen_id = msg.new_screen_id;
        let line = format!(
            "Screen {} reeplaces Screen {}",
            new_screen_id, original_screen_id
        );
        println!("{}", line.bright_cyan());

        for order in self.orders_to_be_sent.iter_mut() {
            if order.screen_id != original_screen_id {
                continue;
            }

            let screen = match self.screens_connections.get(&new_screen_id) {
                Some(screen) => screen,
                None => {
                    let line = format!(
                        "[RL] Error! Screen {} not found saved order to send later",
                        order.screen_id
                    );
                    println!("{}", line.bright_cyan());
                    return;
                }
            };

            if let Some(flavor_id) = order.flavor {
                if let Err(e) = screen.try_send(OrderAborted {
                    order_result: order.order_result,
                    id: order.id.clone(),
                    flavor: flavor_id,
                }) {
                    print_send_error("[RL]", "Sending Order Completed", &e.to_string());
                }
            } else if let Err(e) = screen.try_send(OrderPrepared {
                order_result: order.order_result,
                id: order.id.clone(),
            }) {
                print_send_error("[RL]", "Sending Order Completed", &e.to_string());
            }
        }

        for order in self.orders_on_queue.iter_mut() {
            if order.screen_id == original_screen_id {
                order.screen_id = new_screen_id;
            }
        }

        for (_, order) in self.robots_orders.iter_mut() {
            if order.screen_id == original_screen_id {
                order.screen_id = new_screen_id;
            }
        }

        self.make_and_send_backup();
    }
}

/// Handles the stash of orders to be sent to a screen that is dead
/// If the screen is back online it will send the orders or if it gets a new screen it will send the orders to the new screen
impl Handler<AddOrderToBeSent> for RobotLeader {
    type Result = ();

    fn handle(&mut self, msg: AddOrderToBeSent, _ctx: &mut Context<Self>) {
        self.stash_order_waiting(msg.id.clone(), msg.order_result, msg.screen_id, msg.flavor);
    }
}
