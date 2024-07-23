use std::fmt;

#[derive(Debug, PartialEq)]
pub enum ScreenError {
    TcpListenerError(usize),
}

impl fmt::Display for ScreenError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ScreenError::TcpListenerError(e) => {
                write!(f, "Screen {}: Couldn't initialize listener", e)
            }
        }
    }
}
