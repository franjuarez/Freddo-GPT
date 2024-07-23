use actix::prelude::*;
use colored::*;
use rand::Rng;
use tokio::io::AsyncWriteExt;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio_stream::wrappers::LinesStream;

use crate::common::flavor_id::FlavorID;
use crate::config::MAX_NUMBER_OF_ROBOTS;
use crate::robot::connections::robot_to_leader_connection::RobotToLeaderConnection;
use crate::robot::connections::robot_to_robot_connection::RobotToRobotConnection;
use crate::robot::flavor_token::FlavorToken;
use crate::robot::leader_backup::LeaderBackup;
use crate::robot::leader_elector::LeaderElector;
use crate::robot::messages::*;
use crate::robot::order_manager::OrderManager;
use crate::robot::robot_leader::RobotLeader;
use crate::robot::token_backup::TokenBackup;
use crate::robot::utils::*;

/// Actor that handles the connection of a robot with the other robots and the leader
/// It is in charge of sending the token to the next robot and the finished orders to the leader
/// It also handles all the messages necessary for the election of a new leader
/// It also handles the communication needed to recover a lost token
pub struct RobotConnectionHandler {
    my_id: usize,
    order_manager: Addr<OrderManager>,
    leader_id: usize,
    leader: Option<Addr<RobotToLeaderConnection>>,
    previous_robot: Option<Addr<RobotToRobotConnection>>,
    next_robot: Option<OwnedWriteHalf>,
    next_robot_read: Option<OwnedReadHalf>,
    next_robot_id: usize,
    leader_backup: Option<LeaderBackup>,
    leader_elector: LeaderElector,
    token_backup_msg: Vec<FlavorID>,
}

impl Actor for RobotConnectionHandler {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        start_robots_connection_listener(ctx.address(), self.my_id);
    }
}

impl RobotConnectionHandler {
    pub fn new(order_manager: Addr<OrderManager>, my_id: usize) -> Self {
        Self {
            my_id,
            order_manager,
            leader_id: MAX_NUMBER_OF_ROBOTS,
            leader: None,
            previous_robot: None,
            next_robot: None,
            next_robot_read: None,
            next_robot_id: MAX_NUMBER_OF_ROBOTS,
            leader_backup: None,
            leader_elector: LeaderElector::new(my_id),
            token_backup_msg: Vec::new(),
        }
    }

    /// Function to send a message to the next robot in the ring
    fn safe_send(&mut self, msg: String, ctx: &mut Context<Self>) -> bool {
        let next_id = self.next_robot_id;
        let my_id = self.my_id;
        let addr = ctx.address().clone();

        if self.next_robot.is_some() && self.next_robot_read.is_some() {
            if let Some(mut next_write) = self.next_robot.take() {
                if let Some(mut next_read) = self.next_robot_read.take() {
                    async move {
                        let next_id =
                            safe_send_next(msg, &mut next_write, &mut next_read, my_id, next_id)
                                .await;
                        (next_read, next_write, next_id)
                    }
                    .into_actor(self)
                    .map(move |(next_read, next_write, next_id), actor, _| {
                        if next_id == actor.my_id {
                            if actor.leader_id != actor.my_id {
                                if let Err(e) = addr.try_send(SetNewLeader {
                                    leader_id: actor.my_id,
                                    by_election: true,
                                }) {
                                    print_send_error("[RCH]", "SetNewLeader", &e.to_string());
                                }
                            }
                            let line = "[RCH] Only robot in the ring, I am the Leader".to_string();
                            println!("{}", line.bright_red());
                        } else {
                            actor.next_robot = Some(next_write);
                            actor.next_robot_read = Some(next_read);
                            actor.next_robot_id = next_id;
                        }
                    })
                    .wait(ctx);
                }
            }
            return true;
        }
        // let line = format!("RCH: No tengo un siguiente robot");
        // println!("{}", line.bright_yellow());
        false
    }

    /// Function to send a token to the next robot in the ring
    fn safe_send_token(&mut self, token: FlavorToken, ctx: &mut Context<Self>) {
        let token_msg = RobotCommand::TokenMessage { token }.to_string();
        let msg = match token_msg {
            Ok(r_msg) => r_msg + "\n",
            Err(e) => {
                print_create_error("[RCH]", "TokenMessage", &e.to_string());
                return;
            }
        };

        let could_send = self.safe_send(msg, ctx);

        if self.next_robot_id == self.my_id || !could_send {
            // let line = format!("RCH: No pude enviar el token al siguiente robot, Me lo mando a mi mismo");
            // println!("{}", line.bright_yellow());
            if let Err(e) = ctx.address().try_send(TransferToken {
                flavor_token: token,
            }) {
                print_send_error("[RCH]", "TransferToken", &e.to_string());
            }
        }
    }

    /// Function to send the token backup to the next robot in the ring, to recover a lost token
    fn safe_send_token_backup(&mut self, token_backup: TokenBackup, ctx: &mut Context<Self>) {
        let token_msg = RobotCommand::TokenBackupMsg { token_backup }.to_string();
        let msg = match token_msg {
            Ok(r_msg) => r_msg + "\n",
            Err(e) => {
                print_create_error("[RCH]", "TokenBackup", &e.to_string());
                return;
            }
        };

        self.safe_send(msg, ctx);
    }

    /// Function to send a message to the next robot to inform the new leader's id
    fn safe_send_new_leader(&mut self, new_leader: usize, ctx: &mut Context<Self>) {
        let leader_msg = RobotCommand::NewLeader { leader: new_leader }.to_string();
        let msg = match leader_msg {
            Ok(r_msg) => r_msg + "\n",
            Err(e) => {
                print_create_error("[RCH]", "NewLeader", &e.to_string());
                return;
            }
        };

        self.safe_send(msg, ctx);
    }

    /// Function to send the candidates of an election to the next robot
    fn safe_send_election(&mut self, candidates: Vec<(usize, bool)>, ctx: &mut Context<Self>) {
        let leader_msg = RobotCommand::NewElection { candidates }.to_string();
        let msg = match leader_msg {
            Ok(r_msg) => r_msg + "\n",
            Err(e) => {
                print_create_error("[RCH]", "NewElection", &e.to_string());
                return;
            }
        };

        self.safe_send(msg, ctx);
    }

    /// Creates the RobotToRobotConnection with the previous robot in the ring, to receive all the messages
    fn create_previous_robot_connection(
        &mut self,
        read_half: OwnedReadHalf,
        w_half: OwnedWriteHalf,
        address: Addr<RobotConnectionHandler>,
    ) {
        let pipo = RobotToRobotConnection::create(|own_ctx| {
            let lines = LinesStream::new(BufReader::new(read_half).lines());
            let rpc = RobotToRobotConnection::new(address, Some(w_half));
            RobotToRobotConnection::add_stream(lines, own_ctx);
            rpc
        });
        self.previous_robot = Some(pipo);
    }

    /// Function to make the robot the leader of the ring
    fn make_myself_leader(&mut self, by_election: bool, my_address: Addr<RobotConnectionHandler>) {
        let my_id = self.my_id;
        if !by_election {
            Arbiter::new().spawn_fn(move || {
                RobotLeader::new(my_id, Some(my_address)).start();
            });
            return;
        }
        if self.leader_backup.is_none() {
            let line = "[RCH] Error! I dont have a backup to become leader!".to_string();
            println!("{}", line.bright_yellow());
            return;
        }

        if let Err(e) = self.order_manager.try_send(AbortCurrentOrder {}) {
            print_send_error("[RCH]", "AbortCurrentOrder", &e.to_string());
        }

        if let Some(backup) = self.leader_backup.take() {
            Arbiter::new().spawn_fn(move || {
                RobotLeader::from_backup(my_id, Some(my_address), backup).start();
            });
        }
    }
}

/// Handles the SetNewLeader message, to set a new leader in the ring
impl Handler<SetNewLeader> for RobotConnectionHandler {
    type Result = ();
    fn handle(&mut self, msg: SetNewLeader, ctx: &mut Self::Context) -> Self::Result {
        let new_leader = msg.leader_id;
        let addr = ctx.address().clone();
        let my_id = self.my_id;

        if self.leader_id == new_leader {
            return;
        }

        let line = format!("[RCH] The new Leader is: {}", new_leader);
        println!("{}", line.bright_yellow());

        self.leader_id = new_leader;

        if new_leader == my_id {
            self.make_myself_leader(msg.by_election, addr.clone());
            return;
        }

        async move { connect_to_leader(new_leader, my_id, addr).await }
            .into_actor(self)
            .map(|pipo, actor, _| {
                if let Some(pipo) = pipo {
                    actor.leader = Some(pipo);
                }
            })
            .wait(ctx);
    }
}

/// Handles the NewLeaderElected message, to set a new leader in the ring,
/// if the new leader is the robot itself, it becomes the leader
/// it always sends the new leader to the next robot in the ring
impl Handler<NewLeaderElected> for RobotConnectionHandler {
    type Result = ();
    fn handle(&mut self, msg: NewLeaderElected, ctx: &mut Self::Context) -> Self::Result {
        let new_leader = msg.leader_id;
        if new_leader == self.my_id {
            let line = "[RCH] I have been elected Leader!".to_string();
            println!("{}", line.bright_yellow());
            if let Err(e) = ctx.address().try_send(SetNewLeader {
                leader_id: new_leader,
                by_election: true,
            }) {
                print_send_error("[RCH]", "SetNewLeader", &e.to_string());
            }
            return;
        }
        self.safe_send_new_leader(new_leader, ctx);
    }
}

/// Handles the AddNewLeader message, creates the RobotToLeaderConnection with the new leader
impl Handler<AddNewLeader> for RobotConnectionHandler {
    type Result = ();
    fn handle(&mut self, msg: AddNewLeader, ctx: &mut Self::Context) -> Self::Result {
        if self.leader.is_some() {
            self.leader
                .take()
                .expect("Error en harakiri")
                .do_send(Harakiri());
        }
        // println!("{}", "LLEGUE AL ADDNEWLEADER".bright_cyan());
        self.leader = Some(RobotToLeaderConnection::create(|own_ctx| {
            let lines = LinesStream::new(BufReader::new(msg.read_half).lines());
            let rpc = RobotToLeaderConnection::new(ctx.address().clone(), Some(msg.write_half));
            RobotToLeaderConnection::add_stream(lines, own_ctx);
            rpc
        }));
        self.leader_id = msg.leader_id;
    }
}

/// Handles the AddPreviousRobot message, updates the previous robot connection
impl Handler<AddPreviousRobot> for RobotConnectionHandler {
    type Result = ();
    fn handle(&mut self, msg: AddPreviousRobot, ctx: &mut Self::Context) -> Self::Result {
        if self.previous_robot.is_some() {
            self.previous_robot
                .take()
                .expect("Error en harakiri")
                .do_send(Harakiri());
        }

        let addr = ctx.address().clone();

        if msg.asked {
            self.create_previous_robot_connection(msg.read_half, msg.write_half, addr);
            return;
        }

        let mut w_half = msg.write_half;
        let leader_id = self.leader_id;
        async move {
            if let Err(e) = w_half.write_all(&[leader_id as u8]).await {
                let line = format!("[RCH] Error trying to write to new leader ID: {}", e);
                println!("{}", line.bright_yellow());
            }
            w_half
        }
        .into_actor(self)
        .map(|w_half, actor, _| actor.create_previous_robot_connection(msg.read_half, w_half, addr))
        .wait(ctx);
    }
}

/// Handles the AddNextRobot message, updates the next robot connection
impl Handler<AddNextRobot> for RobotConnectionHandler {
    type Result = ();
    fn handle(&mut self, msg: AddNextRobot, ctx: &mut Self::Context) -> Self::Result {
        if self.next_robot_id == msg.robot_id {
            return;
        }

        if let Some(mut write_half) = self.next_robot.take() {
            async move {
                let _ = write_half.shutdown().await;
                msg.robot_id
            }
            .into_actor(self)
            .map(|id, actor, _ctx| {
                actor.next_robot = Some(msg.write_half);
                actor.next_robot_id = id;
                actor.next_robot_read = Some(msg.read_half);
                let line = format!(
                    "[RCH] Connected to my next robot: {:?}",
                    actor.next_robot_id
                );
                println!("{}", line.bright_cyan());
            })
            .wait(ctx);
        } else {
            self.next_robot = Some(msg.write_half);
            self.next_robot_id = msg.robot_id;
            self.next_robot_read = Some(msg.read_half);
        }
    }
}

/// Handles the JoinRing message.
/// The robot tries to connect with all the possible robots in the ring.
/// If there is no robot to connect to, it declare itself the Leader
impl Handler<JoinRing> for RobotConnectionHandler {
    type Result = ();

    fn handle(&mut self, _msg: JoinRing, ctx: &mut Self::Context) -> Self::Result {
        let addr = ctx.address().clone();
        let my_id = self.my_id;

        async move {
            let connected_to_prev = match connect_to_prev_robot(my_id, addr.clone()).await {
                Ok(_) => true,
                Err(e) => {
                    let line = format!("[RCH] Could not connect to any previous robot: {}", e);
                    println!("{}", line.bright_yellow());
                    false
                }
            };

            let (connected_to_next, leader_id) =
                match connect_to_next_robot_and_get_leader(my_id, addr.clone()).await {
                    Ok(leader_id) => (true, leader_id),
                    Err(e) => {
                        let line = format!("[RCH] Could not connect to any next robot: {}", e);
                        println!("{}", line.bright_yellow());
                        (false, MAX_NUMBER_OF_ROBOTS)
                    }
                };

            if !connected_to_next && !connected_to_prev {
                let line = "[RCH] Only robot in the ring, I am the Leader".to_string();
                println!("{}", line.bright_yellow());
                return my_id;
            }
            leader_id
        }
        .into_actor(self)
        .map(|leader_id, _, _ctx| {
            if let Err(e) = _ctx.address().try_send(SetNewLeader {
                leader_id,
                by_election: false,
            }) {
                print_send_error("[RCH]", "SetNewLeader", &e.to_string());
            }
        })
        .wait(ctx);
    }
}

/// Handles the StartToken message, to start the tokens in the ring
impl Handler<StartTokens> for RobotConnectionHandler {
    type Result = ();

    fn handle(&mut self, msg: StartTokens, _ctx: &mut Self::Context) -> Self::Result {
        for token in msg.all_tokens {
            // let line = format!("RCH: Creo Token de {} con cant: {}  al siguiente robot", token.get_id(), token.get_amnt());
            // println!("{}", line.bright_cyan());
            if let Err(e) = self.order_manager.try_send(TransferToken {
                flavor_token: token,
            }) {
                print_send_error("[RCH]", "TransferToken", &e.to_string());
            }
        }
    }
}

/// Handles a message to transfer a token to the next robot
impl Handler<TransferToken> for RobotConnectionHandler {
    type Result = ();

    fn handle(&mut self, msg: TransferToken, _ctx: &mut Self::Context) -> Self::Result {
        //so we dont flood
        async move {
            tokio::time::sleep(std::time::Duration::from_millis(
                rand::thread_rng().gen_range(0.0..1000.0) as u64,
            ))
            .await;
        }
        .into_actor(self)
        .map(move |_, actor, _| {
            if let Err(e) = actor.order_manager.try_send(TransferToken {
                flavor_token: msg.flavor_token,
            }) {
                print_send_error("[RCH]", "TransferToken", &e.to_string());
            }
        })
        .spawn(_ctx);
    }
}

/// Handles a message to send an order prepared to the leader
impl Handler<OrderPrepared> for RobotConnectionHandler {
    type Result = ();
    fn handle(&mut self, msg: OrderPrepared, _ctx: &mut Self::Context) -> Self::Result {
        // println!("RCH: Pedido listo para ser enviado");
        let order_id = msg.id.clone();
        if let Some(leader) = &self.leader {
            if leader
                .try_send(OrderPrepared {
                    order_result: msg.order_result,
                    id: msg.id,
                })
                .is_err()
            {
                let line = "RCH: Error trying to send OrderPrepared to Leader. Retrying in 1s."
                    .to_string();
                println!("{}", line.bright_yellow());
                _ctx.notify_later(
                    OrderPrepared {
                        order_result: msg.order_result,
                        id: order_id,
                    },
                    std::time::Duration::from_secs(1),
                );
            }
            return;
        }
        let line = "[RCH] I dont have a leader to send the OrderPrepared".to_string();
        println!("{}", line.bright_yellow());
    }
}

/// Handles a message to send an OrderAborted to the leader
impl Handler<OrderAborted> for RobotConnectionHandler {
    type Result = ();
    fn handle(&mut self, msg: OrderAborted, _ctx: &mut Self::Context) -> Self::Result {
        let order_id = msg.id.clone();
        if let Some(leader) = &self.leader {
            if leader
                .try_send(OrderAborted {
                    order_result: msg.order_result,
                    id: msg.id,
                    flavor: msg.flavor,
                })
                .is_err()
            {
                let line =
                    "RCH: Error trying to send OrderAborted to Leader. Retrying in 1s.".to_string();
                println!("{}", line.bright_yellow());
                _ctx.notify_later(
                    OrderAborted {
                        order_result: msg.order_result,
                        id: order_id,
                        flavor: msg.flavor,
                    },
                    std::time::Duration::from_secs(1),
                );
            }
            return;
        }
        let line = "[RCH] I dont have a leader to send the OrderAborted".to_string();
        println!("{}", line.bright_yellow());
    }
}

/// Handles a message to get a token back and send it to the next robot
impl Handler<GetTokenBack> for RobotConnectionHandler {
    type Result = ();
    fn handle(&mut self, msg: GetTokenBack, ctx: &mut Self::Context) -> Self::Result {
        let token = msg.flavor_token;

        // let line = format!("RCH: tengo el token {} con la cantidad {:?} . enviando al siguiente robot, iniciando el pase {}", token.get_id(), token.get_amnt(), token.get_count());
        // println!("{}", line.green());

        self.safe_send_token(token, ctx);
    }
}

/// Handles a message to get a new order
impl Handler<GetNewOrder> for RobotConnectionHandler {
    type Result = ();
    fn handle(&mut self, msg: GetNewOrder, _ctx: &mut Self::Context) -> Self::Result {
        // println!("RCH: Recibi un nuevo pedido");
        if let Err(e) = self.order_manager.try_send(msg) {
            print_send_error("[RCH]", "GetNewOrder", &e.to_string());
        }
    }
}

/// Handles a message to store a backup of the leader
impl Handler<StoreBackup> for RobotConnectionHandler {
    type Result = ();
    fn handle(&mut self, msg: StoreBackup, _ctx: &mut Self::Context) -> Self::Result {
        self.leader_backup = Some(msg.backup);
        self.leader_elector.validate_backup();
    }
}

/// Handles a message to receive the candidates of a new election
/// It checks if the round is finished, if it is, it chooses a new leader and sends it to the next robot
impl Handler<ReceiveNewElection> for RobotConnectionHandler {
    type Result = ();
    fn handle(&mut self, msg: ReceiveNewElection, ctx: &mut Self::Context) -> Self::Result {
        if self
            .leader_elector
            .check_round_finished(msg.candidates.clone())
        {
            let line = "[RCH] Round finished, choosing new leader".to_string();
            println!("{}", line.bright_yellow());
            let new_leader = self.leader_elector.choose_leader(msg.candidates.clone());

            if self.my_id == new_leader {
                if let Err(e) = ctx.address().try_send(SetNewLeader {
                    leader_id: new_leader,
                    by_election: true,
                }) {
                    print_send_error("[RCH]", "SetNewLeader", &e.to_string());
                }
            } else {
                self.safe_send_new_leader(new_leader, ctx);
            }
        } else {
            let line = "[RCH] Adding myself to the election candidates".to_string();
            println!("{}", line.bright_yellow());
            let candidates = self.leader_elector.add_candidate(msg.candidates.clone());
            self.safe_send_election(candidates, ctx);
        }
    }
}

/// Handles a message to start a new election
impl Handler<StartElection> for RobotConnectionHandler {
    type Result = ();
    fn handle(&mut self, _msg: StartElection, ctx: &mut Self::Context) -> Self::Result {
        let candidates = self.leader_elector.start_election();
        self.safe_send_election(candidates, ctx);
    }
}

/// Handles a message to recover a lost token using a backup
impl Handler<GetTokenBackup> for RobotConnectionHandler {
    type Result = ();
    fn handle(&mut self, msg: GetTokenBackup, ctx: &mut Self::Context) -> Self::Result {
        let token_backup = msg.token_backup;
        let flavor_id = token_backup.get_flavor_id();

        if token_backup.get_start_robot_id() == self.my_id
            && self.token_backup_msg.contains(&flavor_id)
        {
            let line = "[RCH] Round finished, restored token using backup".to_string();
            println!("{}", line.bright_yellow());
            self.token_backup_msg.retain(|&x| x != flavor_id);
            let token = FlavorToken::new(flavor_id, token_backup.get_amount());
            self.safe_send_token(token, ctx);
            return;
        }
        if token_backup.get_start_robot_id() > self.my_id
            && self.token_backup_msg.contains(&flavor_id)
        {
            println!("[RCH] Another robot with higher ID is handeling the token recovery");
            self.token_backup_msg.retain(|&x| x != flavor_id);
            return;
        }

        if let Err(e) = self.order_manager.try_send(GetTokenBackup { token_backup }) {
            print_send_error("[RCH]", "GetTokenBackUp", &e.to_string());
        }
    }
}

/// Handles a message to send a token backup to the next robot in the ring
impl Handler<SendTokenBackup> for RobotConnectionHandler {
    type Result = ();
    fn handle(&mut self, msg: SendTokenBackup, ctx: &mut Self::Context) -> Self::Result {
        let flavor_id = msg.token_backup.get_flavor_id();

        if msg.token_backup.get_start_robot_id() == self.my_id {
            self.token_backup_msg.push(flavor_id);
        }
        self.safe_send_token_backup(msg.token_backup, ctx);
    }
}
