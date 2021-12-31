//! # Binary format of messages sent over wire
//!
//! First byte indicates the type:
//!   0 => Action event (move/fire/jump etc.)
//!   1 => Connect event (sent from client when it connects containing name)
//!
//! If first byte is `ActionEvent` (0) then the second byte represents:
//!   0  => UpPressed
//!   1  => UpReleased
//!   2  => RightPressed
//!   3  => RightReleased
//!   4  => DownPressed
//!   5  => DownReleased
//!   6  => LeftPressed
//!   7  => LeftReleased
//!   8  => APressed    (A and B are arbitrary "action" keys)
//!   9  => AReleased
//!   10 => BPressed
//!   11 => BReleased
use async_tungstenite::{tungstenite::Message, WebSocketStream};
use bevy::core::Timer;
use futures_util::stream::SplitSink;
use smol::net::TcpStream;

use skitspel::{ActionEvent, PlayerId};

pub type WebSocketSink = SplitSink<WebSocketStream<TcpStream>, Message>;

/// A timer used to synchronize action events.
///
/// Since the actions of players are updated every tick (which is really frequent)
/// and the clients send `ActionEvent`s over the wire (which is really slow), we
/// don't want to update the action events every tick. Instead we want to "hold"
/// the previosly sent `ActionEvent` from the clients for some time so that it
/// actually gets applied properly (instead of being over after one tick).
///
/// When this timer expires, we will re-evaluate the inputs given from the client
/// and update the `ActionEvent`s accordingly.
pub struct EventTimer(pub Timer);

impl EventTimer {
    /// The time (in seconds) that a `ActionEvent` will be held/applied.
    const HOLD_TIME_SEC: f32 = 1.0 / 10.0;
}

impl Default for EventTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(Self::HOLD_TIME_SEC, true))
    }
}

/// Represents an event that have been triggered by a client.
/// This can ex. be a movement event or a new client have connected.
#[derive(Debug, Clone)]
pub struct EventMessage {
    /// The `PlayerId` of the client that triggered this event.
    pub player_id: PlayerId,
    pub event: NetworkEvent,
}

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    /// An event represeting that something has changed with a connection or
    /// the network (ex. connect/disconnet).
    General(GeneralEvent),
    /// An event representing an action (ex. steering/fire/jump).
    Action(ActionEvent),
    /// An event represeting a message received from a client that was invalid.
    Invalid(Vec<u8>),
}

#[derive(Debug)]
pub enum GeneralEvent {
    /// The String is the name that was specified by the newly connected player.
    ///
    /// The sink can be used to send data to the newly connected client. This
    /// will not be propagated all the way through the "system". The sink will
    /// at one point be moved to its correct place and the value will be set to
    /// None.
    Connected(String, Option<WebSocketSink>),
    Disconnected,
}

impl Clone for GeneralEvent {
    fn clone(&self) -> Self {
        match self {
            Self::Connected(name, _) => Self::Connected(name.clone(), None),
            Self::Disconnected => Self::Disconnected,
        }
    }
}

/// Utility function to decode the given binary `data` into the `Event` that it
/// represents. The `data` will have been sent from one of the clients.
pub fn decode_message(data: &[u8]) -> NetworkEvent {
    if data.is_empty() {
        return NetworkEvent::Invalid(Vec::with_capacity(0));
    }

    // See top-level comment for mapping between values and events.
    match data[0] {
        0 => decode_action_event(data),
        1 => decode_connect_event(data),
        _ => NetworkEvent::Invalid(data.to_vec()),
    }
}

fn decode_action_event(data: &[u8]) -> NetworkEvent {
    if data.len() != 2 {
        return NetworkEvent::Invalid(data.to_vec());
    }

    // See top-level comment for mapping between values and events.
    NetworkEvent::Action(match data[1] {
        0 => ActionEvent::UpPressed,
        1 => ActionEvent::UpReleased,
        2 => ActionEvent::RightPressed,
        3 => ActionEvent::RightReleased,
        4 => ActionEvent::DownPressed,
        5 => ActionEvent::DownReleased,
        6 => ActionEvent::LeftPressed,
        7 => ActionEvent::LeftReleased,
        8 => ActionEvent::APressed,
        9 => ActionEvent::AReleased,
        10 => ActionEvent::BPressed,
        11 => ActionEvent::BReleased,
        _ => return NetworkEvent::Invalid(data.to_vec()),
    })
}

pub fn decode_connect_event(data: &[u8]) -> NetworkEvent {
    match std::str::from_utf8(&data[1..]) {
        Ok(name) => NetworkEvent::General(GeneralEvent::Connected(name.to_string(), None)),
        _ => NetworkEvent::Invalid(data.to_vec()),
    }
}
