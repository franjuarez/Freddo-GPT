use std::sync::Arc;

use actix::{Actor, Addr, StreamHandler};
use colored::Colorize;
use tokio::{
    io::{split, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::Mutex,
};
use tokio_stream::wrappers::LinesStream;

use crate::{
    common::utils::{id_to_screen_addr, ROBOT, SCREEN_NEXT, SCREEN_PREVIOUS},
    config::MAX_NUMBER_OF_SCREENS,
    screen::{
        order_reader::ReadOrders, robot_connection_handler::RobotConnectionHandler,
        screen_connection_listener::ScreenConnectionListener,
        screen_connection_sender::ScreenConnectionSender, screen_error::ScreenError,
    },
};

use super::{
    backup_handler::{self, BackUpHandler, SetPaymentsGateway},
    order_reader::OrderReader,
    payments_gateway::PaymentsGateway,
};

/// Starts the actors and connections for the screens.
pub async fn start_actors_and_connections(num_screen: usize, order_file: String) {
    let backup_handler = backup_handler::BackUpHandler::new().start();
    let payments_gateway = PaymentsGateway::new(num_screen).start();
    let _ = backup_handler
        .send(SetPaymentsGateway::new(
            payments_gateway.clone().recipient(),
        ))
        .await;
    let order_reader = OrderReader::new(order_file, payments_gateway.clone().recipient()).start();
    setup_connections(
        num_screen,
        payments_gateway,
        order_reader,
        backup_handler.clone(),
    )
    .await;
}

/// Sets up the connections between the screens and the robots.
async fn setup_connections(
    num_screen: usize,
    payments_gateway: Addr<PaymentsGateway>,
    order_reader: Addr<OrderReader>,
    backup_handler: Addr<BackUpHandler>,
) {
    let screen_communication_future =
        connect_following_and_notify_previous(num_screen, payments_gateway.clone());
    let screen_listener_future =
        start_server_and_handler(num_screen, backup_handler, payments_gateway);
    let wait_input = wait_input(order_reader);
    let _ = tokio::join!(
        screen_communication_future,
        screen_listener_future,
        wait_input
    );
}

/// Waits for the user to press 'p' to start processing the orders.
/// Once the user presses 'p', the orders are read from the file and sent to the order reader.
async fn wait_input(order_reader: Addr<OrderReader>) {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    println!("{}", "Press 'p' to start processing orders...".purple());

    let _ = tokio::spawn(async move {
        let mut input = String::new();
        loop {
            input.clear();
            reader
                .read_line(&mut input)
                .await
                .expect("Failed to read line");
            if input.trim() == "p" {
                let _ = order_reader.send(ReadOrders()).await;
                break;
            }
            println!("{}", "Press 'p' to start processing orders".purple());
        }
    })
    .await;
}

/// Starts the server and the handler for the screen.
/// The server listens for connections from the previous screen, the next screen, and the robots.
/// The handler processes the connections and creates the actors for the connections.
/// The handler also sends the messages to the actors to handle the connections.
pub async fn start_server_and_handler(
    id: usize,
    backup_handler: Addr<BackUpHandler>,
    payments_gateway: Addr<PaymentsGateway>,
) -> Result<(), ScreenError> {
    let port = id_to_screen_addr(id);
    let listener = TcpListener::bind(port.clone())
        .await
        .map_err(|_| ScreenError::TcpListenerError(id))?;
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let (mut read, write_half) = split(stream);
                let mut buf = [0; 1];
                read.read_exact(&mut buf).await.expect("Failed to read");
                if buf[0] as char == SCREEN_PREVIOUS {
                    handle_previous_screen(read, &backup_handler, &payments_gateway);
                } else if buf[0] as char == ROBOT {
                    handle_robot_connection(read, write_half, &payments_gateway);
                } else if buf[0] as char == SCREEN_NEXT {
                    handle_next_screen(id, &payments_gateway).await;
                }
            }
            Err(_) => {
                return Err(ScreenError::TcpListenerError(id));
            }
        }
    }
}

/// Handles the connection from the next screen.
/// The connection is established with the next screen and the actor is created to handle the connection.
async fn handle_next_screen(id: usize, payments_gateway: &Addr<PaymentsGateway>) {
    let id_following = (id + 1) % MAX_NUMBER_OF_SCREENS;
    let _ = connect_to_following_screen(id_following, payments_gateway.clone(), id).await;
}

/// Handles the connection from the robot.
/// The connection is established with the robot and the actor is created to handle the connection.
fn handle_robot_connection(
    read: tokio::io::ReadHalf<TcpStream>,
    write_half: tokio::io::WriteHalf<TcpStream>,
    payments_gateway: &Addr<PaymentsGateway>,
) {
    let _ = RobotConnectionHandler::create(|ctx| {
        RobotConnectionHandler::add_stream(LinesStream::new(BufReader::new(read).lines()), ctx);
        let write = Arc::new(Mutex::new(write_half));
        RobotConnectionHandler::new(write, payments_gateway.clone())
    });
}

/// Handles the connection from the previous screen.
/// The connection is established with the previous screen and the actor is created to handle the connection.
fn handle_previous_screen(
    read: tokio::io::ReadHalf<TcpStream>,
    backup_handler: &Addr<BackUpHandler>,
    payments_gateway: &Addr<PaymentsGateway>,
) {
    let _ = ScreenConnectionListener::create(|ctx| {
        ScreenConnectionListener::add_stream(LinesStream::new(BufReader::new(read).lines()), ctx);
        ScreenConnectionListener::new(backup_handler.clone(), payments_gateway.clone())
    });
}

/// Connects to the following screen.
/// The function tries to connect to the following screen.
/// If the connection is successful, the actor is created to handle the connection.
/// If the connection is not successful, the function tries to connect to the next screen.
/// The function returns the id of the screen that was connected to.
pub async fn connect_to_following_screen(
    id_following: usize,
    payments_gateway: Addr<PaymentsGateway>,
    my_id: usize,
) -> usize {
    let mut next = id_following;
    for i in 0..MAX_NUMBER_OF_SCREENS {
        next = (next + i) % MAX_NUMBER_OF_SCREENS;
        if next == my_id {
            return next;
        }
        let port = id_to_screen_addr(next);
        if let Some(value) = try_connection(port, &payments_gateway, my_id, next).await {
            return value;
        }
    }
    next
}

/// Tries to connect to the following screen.
async fn try_connection(
    port: String,
    payments_gateway: &Addr<PaymentsGateway>,
    my_id: usize,
    next: usize,
) -> Option<usize> {
    if let Ok(mut stream) = TcpStream::connect(port.clone()).await {
        stream
            .write_all(&[SCREEN_PREVIOUS as u8])
            .await
            .expect("Failed to write");
        let _ = ScreenConnectionSender::create(|ctx| {
            let (read_half, write_half) = split(stream);
            ScreenConnectionSender::add_stream(
                LinesStream::new(BufReader::new(read_half).lines()),
                ctx,
            );
            let write = Arc::new(Mutex::new(write_half));
            ScreenConnectionSender::new(write, payments_gateway.clone(), my_id)
        });
        Some(next)
    } else {
        None
    }
}

/// Notifies the previous screens that the screen is connected and it is ready to receive connections.
async fn notify_previous_screens(my_id: usize) {
    let mut previous = my_id;
    for _ in 0..MAX_NUMBER_OF_SCREENS {
        if previous == 0 {
            previous = MAX_NUMBER_OF_SCREENS - 1;
        } else {
            previous -= 1;
        }
        if previous == my_id {
            return;
        }
        let port = id_to_screen_addr(previous);
        if let Ok(mut stream) = TcpStream::connect(port.clone()).await {
            stream
                .write_all(&[SCREEN_NEXT as u8])
                .await
                .expect("Failed to write");
        }
    }
}

/// Connects to the following screen and notifies the previous screens.
/// The function connects to the following screen and notifies the previous screens that the screen is connected.
async fn connect_following_and_notify_previous(
    my_id: usize,
    payments_gateway: Addr<PaymentsGateway>,
) {
    let id_following = (my_id + 1) % MAX_NUMBER_OF_SCREENS;
    let id_connected = connect_to_following_screen(id_following, payments_gateway, my_id).await;
    if id_connected == my_id {
        return;
    }
    let _ = notify_previous_screens(my_id).await;
}
