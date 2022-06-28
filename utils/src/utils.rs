use num_derive::FromPrimitive;

#[derive(Debug, Clone, FromPrimitive)]
pub enum MessageStyle {
    User = 0,
    Yourself = 1,
    Admin = 2,
    Server = 3,
    Client = 4,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub timestamp: i64,
    pub sender: String,
    pub style: MessageStyle,
    pub text: String,
}
