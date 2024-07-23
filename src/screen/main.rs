use std::env;
use tp2::{config::MAX_NUMBER_OF_SCREENS, screen::communication::start_actors_and_connections};

/// Entry point of the screen application.
///
/// It receives the number of screen as an argument and starts the actors and connections.
/// The number of screen must be less than MAX_NUMBER_OF_SCREENS, which is set in the utils module.
///

#[actix::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let num_screen = match parse_num_screen(&args) {
        Some(num) => num,
        None => return,
    };
    let order_file = match parse_file_name(&args) {
        Some(file) => file,
        None => return,
    };

    start_actors_and_connections(num_screen, order_file).await;
}

/// Parses the number of screen from the arguments.
///
/// If the number of screen is not provided or is invalid, it prints the usage and returns None.
fn parse_num_screen(args: &[String]) -> Option<usize> {
    args.get(1)
        .and_then(|arg| {
            arg.parse::<usize>()
                .ok()
                .filter(|&num| num < MAX_NUMBER_OF_SCREENS)
        })
        .or_else(|| {
            println!("Usage: {} <num_screen> <file_name>", args[0]);
            println!("num_screen must be less than {}", MAX_NUMBER_OF_SCREENS);
            None
        })
}

/// Parses the file name from the arguments.
///
/// If the file name is not provided, it prints the usage and returns None.
fn parse_file_name(args: &[String]) -> Option<String> {
    args.get(2)
        .map(|arg| format!("./src/orders_samples/{}", arg))
        .or_else(|| {
            println!("Usage: {} <num_screen> <file_name>", args[0]);
            println!("file_name must be a valid file in the orders_samples directory");
            None
        })
}
