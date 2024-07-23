use std::sync::Arc;

use actix::{Actor, Addr, StreamHandler};
use colored::Colorize;
use tokio::{io::{split, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader}, net::{TcpListener, TcpStream}, sync::Mutex};
use tokio_stream::wrappers::LinesStream;

use crate::{common::utils::{id_to_screen_addr, ROBOT, SCREEN_NEXT, SCREEN_PREVIOUS}, config::MAX_NUMBER_OF_SCREENS, screen::{order_reader::ReadOrders, payments_gateway::RegisterScreenConnection, robot_connection_handler::RobotConnectionHandler, screen_connection_listener::ScreenConnectionListener, screen_connection_sender::ScreenConnectionSender, screen_error::ScreenError}};

use super::{backup_handler::{self, BackUpHandler, SetPaymentsGateway}, order_reader::OrderReader, payments_gateway::PaymentsGateway};
use tokio::sync::mpsc;



pub async fn start_actors_and_connections(num_screen: usize, order_file: String){
    let backup_handler = backup_handler::BackUpHandler::new().start();
    let payments_gateway = PaymentsGateway::new(num_screen).start();
    let _ = backup_handler
        .send(SetPaymentsGateway::new(payments_gateway.clone().recipient()))
        .await;
    let order_reader = OrderReader::new(
        order_file,
        payments_gateway.clone().recipient(),
    )
    .start();
    setup_connections(num_screen, payments_gateway, order_reader, backup_handler.clone()).await;
}

async fn setup_connections(num_screen: usize, payments_gateway: Addr<PaymentsGateway>, order_reader: Addr<OrderReader>, backup_handler: Addr<BackUpHandler>) {
    let screen_communication_future = connect_following_and_notify_previous(num_screen, payments_gateway.clone());
    let screen_listener_future = start_server_and_handler(num_screen, backup_handler, payments_gateway);
    order_reader.do_send(ReadOrders());

    let _ = tokio::join!(
        screen_communication_future,
        screen_listener_future
    );
}

async fn start_server_and_handler(id: usize,backup_handler: Addr<BackUpHandler>, payments_gateway: Addr<PaymentsGateway>) -> Result<(), ScreenError>{
    let port = id_to_screen_addr(id);
    let listener = TcpListener::bind(port).await.map_err(|_| ScreenError::TcpListenerError(id))?;
    while let Ok((stream, addr)) = listener.accept().await {
        let (mut read, write_half) = split(stream);
        let mut buf = [0; 1];
        read.read_exact(buf.as_mut_slice()).await.expect("Failed to read");
        if buf[0] as char == SCREEN_PREVIOUS{
            handle_previous_screen(addr, read, write_half, &backup_handler, &payments_gateway, id);
        } else if buf[0] as char == ROBOT {
            handle_robot_connection(addr, read, write_half, &payments_gateway);
        } else if buf[0] as char == SCREEN_NEXT{
            handle_next_screen(id, &payments_gateway).await;
        }
}
    Ok(())

}

async fn handle_next_screen(id: usize, payments_gateway: &Addr<PaymentsGateway>) {
    let id_following = (id + 1) % MAX_NUMBER_OF_SCREENS;
    let _ = connect_to_following_screen(id_following, payments_gateway.clone(), id).await;
}

fn handle_robot_connection(addr: std::net::SocketAddr, read: tokio::io::ReadHalf<TcpStream>, write_half: tokio::io::WriteHalf<TcpStream>, payments_gateway: &Addr<PaymentsGateway>) {
    let _ = RobotConnectionHandler::create(|ctx| {
        RobotConnectionHandler::add_stream(
            LinesStream::new(BufReader::new(read).lines()),
            ctx,
        );
        let write = Arc::new(Mutex::new(write_half));
        RobotConnectionHandler::new(write, addr, payments_gateway.clone())
    });
}

fn handle_previous_screen(addr: std::net::SocketAddr, read: tokio::io::ReadHalf<TcpStream>, write_half: tokio::io::WriteHalf<TcpStream>, backup_handler: &Addr<BackUpHandler>, payments_gateway: &Addr<PaymentsGateway>, id: usize) {
    let _ = ScreenConnectionListener::create(|ctx| {
        ScreenConnectionListener::add_stream(LinesStream::new(BufReader::new(read).lines()), ctx);
        let write =  Arc::new(Mutex::new(write_half));
        ScreenConnectionListener::new(write, addr, backup_handler.clone(), payments_gateway.clone(), id)
    });
}


pub async fn connect_to_following_screen(id_following: usize, payments_gateway: Addr<PaymentsGateway>, my_id: usize)->usize{
    let mut next = id_following;
    for i in 0..MAX_NUMBER_OF_SCREENS-1 {
        next = (next + i) % MAX_NUMBER_OF_SCREENS;
        let port = id_to_screen_addr(id_following);
        match TcpStream::connect(port.clone()).await {
            Ok(mut stream) => {
                stream.write_all(&[SCREEN_PREVIOUS as u8]).await.expect("Failed to write");
                let actor = ScreenConnectionSender::create(|ctx| {
                    let (read_half, write_half) = split(stream);
                    ScreenConnectionSender::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);
                    let write =  Arc::new(Mutex::new(write_half));
                    ScreenConnectionSender::new(write, payments_gateway.clone(), my_id)
                });
                payments_gateway.do_send(RegisterScreenConnection::new(actor));
                return next;
            },
            Err(_) => {
            }
        }
    }
    return next;

}


async fn notify_previous_screens(my_id: usize, id_already_connected: usize){
    let mut previous = my_id;
    for _ in 0..MAX_NUMBER_OF_SCREENS {
        if previous == 0 {
            previous = MAX_NUMBER_OF_SCREENS - 1;
        } else {
            previous -= 1;
        }
        let port = id_to_screen_addr(previous);
        match TcpStream::connect(port.clone()).await {
            Ok(mut stream) => {
                
                stream.write_all(&[SCREEN_NEXT as u8]).await.expect("Failed to write");
            },
            Err(_) => {
            }
        }
    }
}

async fn connect_following_and_notify_previous(my_id: usize, payments_gateway: Addr<PaymentsGateway>){
    let id_following = (my_id + 1) % MAX_NUMBER_OF_SCREENS;
    let id_connected = connect_to_following_screen(id_following, payments_gateway, my_id).await;
    if id_connected == my_id {
        return;
    }
    let _ = notify_previous_screens(my_id, id_connected).await;
}
