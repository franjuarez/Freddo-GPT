pub const SCREEN_PREVIOUS: char = 's';
pub const ROBOT: char = 'r';
pub const SCREEN_NEXT: char = 'n';

pub fn id_to_leader_addr(id: usize) -> String {
    "127.0.0.1:369".to_owned() + &*id.to_string()
}

pub fn id_to_screen_addr(id: usize) -> String {
    "127.0.0.1:700".to_owned() + &*id.to_string()
}
