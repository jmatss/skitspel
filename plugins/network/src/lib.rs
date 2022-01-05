mod event;
mod network;
mod wsstream;

use std::sync::{Arc, Mutex};

use bevy::prelude::{AppBuilder, IntoSystem, Plugin};

pub use event::{EventMessage, EventTimer, GeneralEvent, NetworkEvent};
use network::setup_network;
pub use network::{ActionMessageIter, GeneralMessageIter, NetworkContext};

/// Plugin that handles all network logic for the game.
///
/// The game communicates over websockets with the clients. The struct containing
/// the "synchronizing" state is the `NetworkContext`.
pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.init_resource::<EventTimer>()
            .init_resource::<Arc<Mutex<NetworkContext>>>()
            .add_startup_system(setup_network.system());
    }
}
