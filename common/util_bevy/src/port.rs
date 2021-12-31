use std::ops::Deref;

use bevy::{
    app::{AppExit, Events},
    prelude::FromWorld,
};

/// Default port used if unable to parse spcified port.
pub const DEFAULT_PORT: u16 = 8080;

/// Represents a port. This will be used as an resource to indicate which port
/// the server is listening on.
pub struct Port(pub u16);

impl Deref for Port {
    type Target = u16;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<u16> for Port {
    fn from(value: u16) -> Self {
        Port(value)
    }
}

impl FromWorld for Port {
    fn from_world(world: &mut bevy::prelude::World) -> Self {
        let mut app_exit_events = world.get_resource_or_insert_with(Events::<AppExit>::default);
        let port = if let Some(port_str) = std::env::args().nth(1) {
            match port_str.parse::<u16>() {
                Ok(port) => port,
                Err(_) => {
                    eprintln!("Unable to parse port into u16: {}", port_str);
                    app_exit_events.send(AppExit);
                    DEFAULT_PORT
                }
            }
        } else {
            eprintln!("First argument must be port number.");
            app_exit_events.send(AppExit);
            DEFAULT_PORT
        };
        Self(port)
    }
}
