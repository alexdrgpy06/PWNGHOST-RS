//! Agent actions

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentAction {
    Stay,
    Hop(u8),
    Deauth(String),
    Associate(String),
    Sleep(u64),
    Wait,
}