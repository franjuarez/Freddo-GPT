use actix::prelude::*;
use colored::*;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self};
use tokio::time::{timeout_at, Instant};
use tokio_stream::wrappers::LinesStream;

use crate::common::order::KILO;
use crate::common::utils::{id_to_leader_addr, id_to_screen_addr};
use crate::config::MAX_NUMBER_OF_ROBOTS;
use crate::robot::connections::robot_to_leader_connection::RobotToLeaderConnection;
use crate::robot::errors::RobotConnectionError;
use crate::robot::messages::*;
use crate::robot::order_preparer::SCOOP_TIME_FACTOR;
use crate::robot::robot_connection_handler::RobotConnectionHandler;
use crate::robot::robot_leader::RobotLeader;

use super::order_manager::OrderManager;

pub const INITIAL_AMOUNT: usize = 4000;

pub const MAX_ROBOT_ID: usize = MAX_NUMBER_OF_ROBOTS - 1;

pub const NEW_NEXT_ROBOT: char = 'n';
pub const NEW_PREV_ROBOT: char = 'p';
pub const NEW_ROBOT_LEADER: char = 'r';

const TIME_OUT: usize = ((MAX_NUMBER_OF_ROBOTS - 1) * SCOOP_TIME_FACTOR * KILO) / 2;

/// Returns the address of the robot with the given id.
pub fn id_to_robot_addr(id: usize) -> String {
    "127.0.0.1:807".to_owned() + &*id.to_string()
}

/// Tries to write a message to the next robot, it returns false if the connection was closed.
pub async fn try_write_all(
    write_half: &mut OwnedWriteHalf,
    read_half: &OwnedReadHalf,
    msg: &str,
) -> bool {
    let mut buff: [u8; 1] = [0; 1];
    if let Ok(0) = read_half.try_read(buff.as_mut()) {
        let line = "[RCH] The next robot closed the connection!".to_string();
        println!("{}", line.red());
        return false;
    }

    if let Err(e) = write_half.write_all(msg.as_bytes()).await {
        let line = format!("Could not write! : {}", e);
        println!("{}", line.red());
        return false;
    }
    true
}

/// Function that handles the timeout of the token.
pub async fn token_lost_timeout(mut receiver: mpsc::Receiver<usize>, addr: Addr<OrderManager>) {
    loop {
        match timeout_at(
            Instant::now() + Duration::from_millis(TIME_OUT as u64),
            receiver.recv(),
        )
        .await
        {
            Ok(Some(0)) => {}
            Ok(Some(1)) => {
                break;
            }
            Ok(Some(_)) => {
                println!("Error: Unexpected message received!");
                break;
            }
            Ok(None) => {
                println!("Error: Message is None!");
                break;
            }
            Err(_) => {
                if let Err(e) = addr.try_send(TimerWentOff {}) {
                    print_send_error("[OM]", "TimerWentOff", &e.to_string());
                }
                break;
            }
        }
    }
}

/// Sends a message to the next robot, if the connection is closed it tries to connect to the next robot and so on.
/// It returns the id of the robot that is now the next robot.
pub async fn safe_send_next(
    msg: String,
    next_robot: &mut OwnedWriteHalf,
    next_read: &mut OwnedReadHalf,
    my_id: usize,
    next_id: usize,
) -> usize {
    let mut curr_next_id = (my_id + 1) % MAX_NUMBER_OF_ROBOTS;
    if !try_write_all(next_robot, next_read, &msg).await {
        let line =
            "[RCH] Could not send message to the next robot! Trying to connect to the next one"
                .to_string();
        println!("{}", line.red());

        // println!("Next robot to try: {}", curr_next_id);
        while curr_next_id != my_id {
            let next_addr = format!("Trying to connect to {}:", curr_next_id);
            println!("{}", next_addr.bright_cyan());
            match TcpStream::connect(id_to_robot_addr(curr_next_id)).await {
                Ok(mut stream) => {
                    if let Err(e) = stream.write_all(&[NEW_PREV_ROBOT as u8]).await {
                        let line = format!(
                            "[RCH] Error trying to send my id to the new next robot: {}",
                            e
                        );
                        println!("{}", line.red());
                        curr_next_id = (curr_next_id + 1) % MAX_NUMBER_OF_ROBOTS;
                        continue;
                    }
                    let (read_half, mut write_half) = stream.into_split();

                    let line = format!("[RCH] Connecting to next robot: {} !", curr_next_id);
                    println!("{}", line.bright_cyan());

                    if !try_write_all(&mut write_half, &read_half, &msg).await {
                        let line = "[RCH] The next robot closed the connection!".to_string();
                        println!("{}", line.red());
                        curr_next_id = (curr_next_id + 1) % MAX_NUMBER_OF_ROBOTS;
                        continue;
                    }
                    *next_robot = write_half;
                    *next_read = read_half;
                    return curr_next_id;
                }
                Err(_) => {
                    curr_next_id = (curr_next_id + 1) % MAX_NUMBER_OF_ROBOTS;
                    continue;
                }
            }
        }
        my_id
    } else {
        next_id
    }
}

/// Connects to the leader and returns the Address od the Actor that manages the connection.
pub async fn connect_to_leader(
    new_leader: usize,
    my_id: usize,
    addr: Addr<RobotConnectionHandler>,
) -> Option<Addr<RobotToLeaderConnection>> {
    match TcpStream::connect(id_to_leader_addr(new_leader)).await {
        Ok(mut stream) => {
            if let Err(e) = stream.write_all(&[my_id as u8]).await {
                let line = format!("[RCH] Error trying to send my id to the new leader: {}", e);
                println!("{}", line.red());
                return None;
            }
            let (read_half, write_half) = stream.into_split();

            let pipo = RobotToLeaderConnection::create(|own_ctx| {
                let lines: LinesStream<BufReader<OwnedReadHalf>> =
                    LinesStream::new(BufReader::new(read_half).lines());
                let rpc = RobotToLeaderConnection::new(addr, Some(write_half));
                RobotToLeaderConnection::add_stream(lines, own_ctx);
                rpc
            });
            Some(pipo)
        }
        Err(_) => {
            let line = "[RCH] Could not connect to the leader!".to_string();
            println!("{}", line.red());
            None
        }
    }
}

/// Connects to the screen with the given id.
pub async fn connect_to_screen(screen_id: usize, address: Addr<RobotLeader>) {
    let s_id = screen_id;
    match TcpStream::connect(id_to_screen_addr(s_id)).await {
        Ok(mut stream) => {
            if (stream.write_all(&[NEW_ROBOT_LEADER as u8]).await).is_err() {
                let line = "[RCH] Error trying to send my id to the new screen".to_string();
                println!("{}", line.red());
                return;
            }

            let (r_half, w_half) = stream.into_split();
            if let Err(e) = address.try_send(AddNewScreen {
                screen_id: s_id,
                write_half: w_half,
                read_half: r_half,
            }) {
                print_send_error("[RL]", "AddNewScreen", &e.to_string());
            }
        }
        Err(_) => {
            // println!("Could not connect with screen: {}\n{}", screen_id, e);
        }
    }
}

/// Asks for the leader of the next robot.
async fn ask_for_leader(mut stream: TcpStream) -> Result<(TcpStream, usize), RobotConnectionError> {
    let mut buff_leader_id = [0; 1];
    // println!("Asking for leader");
    if let Err(e) = stream.read_exact(&mut buff_leader_id).await {
        return Err(RobotConnectionError::ColudNotConnectToRobot(e.to_string()));
    }
    Ok((stream, buff_leader_id[0] as usize))
}

/// Connects to a robot it can be its previous and next.
async fn connect_to_robot(
    id: usize,
    my_id: usize,
    address: Addr<RobotConnectionHandler>,
    place: char,
) -> Result<usize, RobotConnectionError> {
    match TcpStream::connect(id_to_robot_addr(id)).await {
        Ok(mut stream) => {
            if place == NEW_PREV_ROBOT {
                // println!("Trying to connect to previous robot: {}", id);
                if let Err(err) = stream.write_all(&[NEW_NEXT_ROBOT as u8]).await {
                    return Err(RobotConnectionError::ColudNotConnectToRobot(
                        err.to_string(),
                    ));
                }
                if let Err(err) = stream.write_all(&[my_id as u8]).await {
                    return Err(RobotConnectionError::ColudNotConnectToRobot(
                        err.to_string(),
                    ));
                }

                let line = format!("[RCH] Connected to the previous robot, ID: {}!", id);
                println!("{}", line.bright_cyan());

                let (r_half, w_half) = stream.into_split();
                if let Err(e) = address.try_send(AddPreviousRobot {
                    write_half: w_half,
                    read_half: r_half,
                    asked: true,
                }) {
                    print_send_error("[RCH]", "AddPreviousRobot", &e.to_string());
                }
                Ok(MAX_NUMBER_OF_ROBOTS)
            } else {
                // println!("Trying to connect to next robot: {}", id);
                if let Err(err) = stream.write_all(&[NEW_PREV_ROBOT as u8]).await {
                    return Err(RobotConnectionError::ColudNotConnectToRobot(
                        err.to_string(),
                    ));
                }

                match ask_for_leader(stream).await {
                    Ok((stream_2, leader_id)) => {
                        let line = format!("[RCH] Connected to the next robot, ID: {}!", id);
                        println!("{}", line.bright_cyan());

                        let (r_half, w_half) = stream_2.into_split();
                        if let Err(e) = address.try_send(AddNextRobot {
                            robot_id: id,
                            write_half: w_half,
                            read_half: r_half,
                        }) {
                            print_send_error("[RCH]", "AddNextRobot", &e.to_string());
                        }
                        Ok(leader_id)
                    }
                    Err(e) => {
                        println!("Could not ask for leader: {}", e);
                        Err(e)
                    }
                }
            }
        }
        Err(e) => {
            // println!("Could not connect to robot: {}", e);
            Err(RobotConnectionError::ColudNotConnectToRobot(e.to_string()))
        }
    }
}

/// Connects to the next robot and gets the leader's id.
pub async fn connect_to_next_robot_and_get_leader(
    my_id: usize,
    address: Addr<RobotConnectionHandler>,
) -> Result<usize, RobotConnectionError> {
    let line = "[RCH] Trying to connect to the next robot".to_string();
    println!("{}", line.bright_cyan());
    let mut curr_id = (my_id + 1) % MAX_NUMBER_OF_ROBOTS;
    while curr_id != my_id {
        if let Ok(leader_id) =
            connect_to_robot(curr_id, my_id, address.clone(), NEW_NEXT_ROBOT).await
        {
            return Ok(leader_id);
        };
        curr_id = (curr_id + 1) % MAX_NUMBER_OF_ROBOTS;
    }
    Err(RobotConnectionError::NoRobotsAvailableError())
}

/// Connects to the previous robot.
pub async fn connect_to_prev_robot(
    my_id: usize,
    address: Addr<RobotConnectionHandler>,
) -> Result<(), RobotConnectionError> {
    let line = "[RCH] Trying to connect to the previous robot".to_string();
    println!("{}", line.bright_cyan());
    let mut curr_id = (my_id + MAX_NUMBER_OF_ROBOTS - 1) % MAX_NUMBER_OF_ROBOTS;
    while curr_id != my_id {
        if (connect_to_robot(curr_id, my_id, address.clone(), NEW_PREV_ROBOT).await).is_ok() {
            return Ok(());
        };
        curr_id = (curr_id + MAX_NUMBER_OF_ROBOTS - 1) % MAX_NUMBER_OF_ROBOTS;
    }
    Err(RobotConnectionError::NoRobotsAvailableError())
}

/// Prints an error message when trying to send a message.
pub fn print_send_error(origin: &str, msg: &str, e: &str) {
    let line = format!(
        "{} Error trying to send {} Message. Message dumped\n {}",
        origin, msg, e
    );
    println!("{}", line.red());
}

/// Prints an error message when trying to create a message.
pub fn print_create_error(origin: &str, msg: &str, e: &str) {
    let line = format!(
        "{} Error trying to create the {} Message. Message dumped\n {}",
        origin, msg, e
    );
    println!("{}", line.red());
}

/// Starts the listener for the robots.
pub fn start_robots_connection_listener(addr: Addr<RobotConnectionHandler>, id: usize) {
    tokio::spawn(async move {
        let port = id_to_robot_addr(id);
        let listener = match TcpListener::bind(port.clone()).await {
            Ok(l) => l,
            Err(e) => {
                let line = format!("Error! Could not bind to port: {}", e);
                println!("{}", line.red());
                return;
            }
        };

        loop {
            let (stream, src_addr) = match listener.accept().await {
                Ok((s, a)) => (s, a),
                Err(e) => {
                    let line = format!("Error! Could not accept connection: {}", e);
                    println!("{}", line.red());
                    continue;
                }
            };
            let (mut r_half, w_half) = stream.into_split();

            // println!(
            //     "{}",
            //     "RobotConnectionHandler: connection accepted".bright_cyan()
            // );

            let mut buf = vec![0; 1];
            if let Err(e) = r_half.read_exact(buf.as_mut_slice()).await {
                let line = format!("Error! Could not read from stream: {}", e);
                println!("{}", line.red());
                continue;
            }

            if buf[0] as char == NEW_NEXT_ROBOT {
                // println!("RCH: Recibi un mensaje de NextRobot de: {}", src_addr);
                if let Err(e) = r_half.read_exact(buf.as_mut_slice()).await {
                    let line = format!("Error! Could not read from stream: {}", e);
                    println!("{}", line.red());
                    continue;
                }
                // println!("ID LEIDO DEL SIGUIENTE ROBOT: {}", buf[0]);
                if let Err(e) = addr.try_send(AddNextRobot {
                    robot_id: buf[0] as usize,
                    write_half: w_half,
                    read_half: r_half,
                }) {
                    print_send_error("[RCH]", "AddNextRobot", &e.to_string());
                }
            } else if buf[0] as char == NEW_PREV_ROBOT {
                // println!("RCH: Recibi un mensaje de PreviousRobot de: {}", src_addr);
                if let Err(e) = addr.try_send(AddPreviousRobot {
                    write_half: w_half,
                    read_half: r_half,
                    asked: false,
                }) {
                    print_send_error("[RCH]", "AddPreviousRobot", &e.to_string());
                }
            } else if buf[0] as char == NEW_ROBOT_LEADER {
                if let Err(e) = r_half.read_exact(buf.as_mut_slice()).await {
                    let line = format!("Error! Could not read from stream: {}", e);
                    println!("{}", line.red());
                    continue;
                }
                println!(
                    "[RCH] New message from RobotLeader: {} the ID is: {}",
                    src_addr, buf[0] as usize
                );
                if let Err(e) = addr.try_send(AddNewLeader {
                    write_half: w_half,
                    read_half: r_half,
                    leader_id: buf[0] as usize,
                }) {
                    print_send_error("[RCH]", "AddNewLeader", &e.to_string());
                }
            } else {
                println!("[RCH] Received something unexpected: {:?}", buf[0] as char);
            }
        }
    });
}

/// Starts the listener for the leader connection.
pub fn start_leader_connection_listener(addr: Addr<RobotLeader>, id: usize) {
    tokio::spawn(async move {
        let port = id_to_leader_addr(id);
        let listener = match TcpListener::bind(port.clone()).await {
            Ok(l) => l,
            Err(e) => {
                let line = format!("Error! Could not bind to port: {}", e);
                println!("{}", line.red());
                return;
            }
        };

        loop {
            let (stream, _) = match listener.accept().await {
                Ok((s, a)) => (s, a),
                Err(e) => {
                    let line = format!("Error! Could not accept connection: {}", e);
                    println!("{}", line.red());
                    continue;
                }
            };

            let (mut r_half, w_half) = stream.into_split();

            // println!(
            //     "{}",
            //     "Robot Lider: connection accepted".bright_cyan()
            // );

            let mut buf_id = vec![0; 1];
            if let Err(e) = r_half.read_exact(buf_id.as_mut_slice()).await {
                let line = format!("Error! Could not read from stream: {}", e);
                println!("{}", line.red());
                continue;
            }
            if let Err(e) = addr.try_send(AddNewRobot {
                robot_id: buf_id[0] as usize,
                write_half: w_half,
                read_half: r_half,
                asked: false,
            }) {
                print_send_error("[RL]", "AddNewRobot", &e.to_string());
            }
        }
    });
}
