use std::{
    collections::{hash_map::Entry, HashMap},
    net::{Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
};

use bevy::{
    app::{AppExit, Events},
    prelude::*,
};
use futures_util::stream::StreamExt;
use smol::{
    channel::{self, Receiver, Sender, TryRecvError},
    net::{TcpListener, TcpStream},
};

use skitspel::{ActionEvent, PlayerId, PlayerIdGenerator};

use crate::{
    event::{decode_message, EventMessage, GeneralEvent, NetworkEvent, WebSocketSink},
    EventTimer,
};

/// The buffer size for the channel containing events.
const EVENT_CHANNEL_BUF_SIZE: usize = 20;

/// This is the "central" struct of the network logic, all messages goes through
/// this struct.
///
/// All messages received from the clients are put into the `channel_tx` channel.
/// These messages are then handled by the `event_message_handler` function
/// which populates the other fields in struct with the data.
#[derive(Debug, Default)]
pub struct NetworkContext {
    /// Internal channel used to create new `EventMessage`s. This sender will be
    /// given to the "websocket client handlers". When a client sends a message
    /// over the websocket, the handler will register that message as a
    /// `EventMessage` and put it into this channel.
    ///
    /// This channel will be created and then set in the `event_message_handler`
    /// function. The corresponding receivers are localy stored in that function.
    channel_tx: Option<Sender<EventMessage>>,

    /// Channel used for messages/events that aren't related to the actions of
    /// client. This can be ex. connection, disconnect or error messages.
    ///
    /// This channel will be created and then set in the `event_message_handler`
    /// function. The corresponding receivers are localy stored in that function.
    common_client_channel: Option<Receiver<EventMessage>>,

    /// These channels will contain all `ActionEvent` messages that have been
    /// generated per client/player. The key is the ID of the player and the
    /// value is the unhandled messages that have been sent by that specific
    /// client.
    ///
    /// These channels will be read by a "bevy system" which acts on the events.
    /// Using one channel per client prevents one client to clog a potential
    /// shared channel. This separation also makes it cleaner to read one message
    /// for every client per game-tick.
    client_channels: HashMap<PlayerId, Receiver<ActionEvent>>,

    /// Contains the sinks for the websocket clients. These are used to send
    /// data to the clients.
    client_websockets: HashMap<PlayerId, WebSocketSink>,

    /// Used to generate new unique player IDs.
    id_generator: Arc<Mutex<PlayerIdGenerator>>,
}

impl NetworkContext {
    pub fn iter_common(&mut self) -> GeneralMessageIter {
        GeneralMessageIter { network_ctx: self }
    }

    /// Only returs an iterator if the `event_timer` have finished the time to
    /// start a new tick.
    pub fn iter_action<'s>(
        &'s mut self,
        time: &Time,
        event_timer: &mut EventTimer,
    ) -> Option<ActionMessageIter> {
        if event_timer.0.tick(time.delta()).just_finished() {
            let player_ids = self.client_websockets.keys().cloned().collect::<Vec<_>>();
            Some(ActionMessageIter {
                network_ctx: self,
                player_ids,
                cur_player_idx: 0,
            })
        } else {
            None
        }
    }
}

pub struct GeneralMessageIter<'a> {
    network_ctx: &'a mut NetworkContext,
}

impl<'a> Iterator for GeneralMessageIter<'a> {
    type Item = EventMessage;

    fn next(&mut self) -> Option<Self::Item> {
        let common_client_rx = self.network_ctx.common_client_channel.as_mut()?;
        match common_client_rx.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Closed) => panic!("Channel closed"),
        }
    }
}

pub struct ActionMessageIter<'a> {
    network_ctx: &'a mut NetworkContext,
    player_ids: Vec<PlayerId>,
    /// The current index of the `player_ids` that we are iterating over.
    cur_player_idx: usize,
}

impl<'a> Iterator for ActionMessageIter<'a> {
    type Item = (PlayerId, ActionEvent);

    fn next(&mut self) -> Option<Self::Item> {
        if self.cur_player_idx >= self.player_ids.len() {
            return None;
        }

        let player_id = self.player_ids.get(self.cur_player_idx)?;
        let client_rx = self.network_ctx.client_channels.get_mut(player_id)?;
        let event = match client_rx.try_recv() {
            Ok(action_event) => action_event,
            Err(TryRecvError::Empty | TryRecvError::Closed) => ActionEvent::None,
        };

        self.cur_player_idx += 1;
        Some((*player_id, event))
    }
}

/// The entry call to this plugin which sets up all the logic.
///
/// This function spawns two persistant "tasks"/"processes" that will run
/// until the whole program exists.
///
/// The first "task" spawned is the `event_message_handler` which handles all
/// state in the `NetworkContext`. It creates new channels/sockets when new
/// clients are connected and removes channels/sockets when they disconnect.
/// It also filters/sorts the messages received from the clients (all stored in
/// a single channel) and puts them into more "descriptive" channels so that the
/// messages are easily read by other components.
///
/// The second "task" is the `websocket_listener` which accepts new connections
/// from clients. For every client, a new "task" is spawned that handles all
/// communication with that specific client.
pub(crate) fn setup_network(
    network_ctx: ResMut<Arc<Mutex<NetworkContext>>>,
    mut app_exit_events: ResMut<Events<AppExit>>,
) {
    let port = if let Some(port_str) = std::env::args().nth(1) {
        match port_str.parse::<u16>() {
            Ok(port) => port,
            Err(_) => {
                println!("Unable to parse port into u16: {}", port_str);
                app_exit_events.send(AppExit);
                return;
            }
        }
    } else {
        8080
    };

    let (channel_tx, channel_rx) = channel::unbounded();
    let (common_client_tx, common_client_rx) = channel::bounded(EVENT_CHANNEL_BUF_SIZE);
    network_ctx.lock().unwrap().channel_tx = Some(channel_tx.clone());
    network_ctx.lock().unwrap().common_client_channel = Some(common_client_rx);
    smol::spawn(event_message_handler(
        Arc::clone(&network_ctx),
        channel_rx,
        common_client_tx,
    ))
    .detach();

    let server_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port);
    let id_generator = Arc::clone(&network_ctx.lock().unwrap().id_generator);
    smol::spawn(websocket_listener(server_addr, id_generator, channel_tx)).detach();
}

/// A function to handle all the structure/logic inside the one and only
/// `EventMessageContext`. This function will locally store the receiver
/// corresponding to `event_ctx.channel_tx` and the senders corresponding to
/// the receivers in `event_ctx.client_channels`.
///
/// This function is the only function that is allowed to create or removed
/// information from the `EventMessageContext`. The only other interaction done
/// with the object is by a "bevy system" that pops the latest message from
/// `event_ctx.client_channels` for every client (one time every game-tick).
///
/// This function will:
///  * Create/remove `event_ctx.client_channels` when a client connects/disconnects.
///  * Create/remove `event_ctx.client_websockets` when a client connects/disconnects.
///  * Propagate messages from the `event_ctx.channel_tx` channel into the
///    corresponding `event_ctx.client_channels`/`event_ctx.common_client_channel`.
async fn event_message_handler(
    event_ctx: Arc<Mutex<NetworkContext>>,
    channel_rx: Receiver<EventMessage>,
    common_client_tx: Sender<EventMessage>,
) {
    println!("Started the EventMessageContext-handler.");

    // Will contain the senders for the corresponding receivers stored in
    // `event_ctx.client_channels`.
    let mut client_channels_tx = HashMap::new();

    while let Ok(EventMessage {
        player_id,
        mut event,
    }) = channel_rx.recv().await
    {
        // Edge-case to handle new connection. Need to setup all the structures
        // before starting the "processing" of the message/event.
        if let NetworkEvent::General(GeneralEvent::Connected(ref mut sink_opt)) = event {
            println!(
                "EventMessageContext-handler :: Received connect from player with ID: {}",
                player_id
            );

            let mut event_ctx_guard = event_ctx.lock().unwrap();
            if let Some(sink) = sink_opt.take() {
                let (tx, rx) = channel::bounded(EVENT_CHANNEL_BUF_SIZE);
                event_ctx_guard.client_channels.insert(player_id, rx);
                client_channels_tx.insert(player_id, tx);
                event_ctx_guard.client_websockets.insert(player_id, sink);
            } else {
                unreachable!("Received connect with no sink. Player ID: {}", player_id)
            }
        }

        if let NetworkEvent::Action(car_event) = event {
            // TODO: Probably shouldn't drop them now. Should sent messages on
            //       pressed/release, so is important to read all messages.
            match client_channels_tx.entry(player_id) {
                Entry::Occupied(mut entry) => {
                    // Drop any event that doesn't fit into the channel. We don't
                    // want to buffer old, delayed, inputs.
                    let _ = entry.get_mut().try_send(car_event);
                }
                Entry::Vacant(_) => unreachable!(
                    "Received message from non-existing player with ID: {}",
                    player_id
                ),
            }
        } else if let NetworkEvent::Invalid(_) = event {
            // `Invalid` messages aren't that important to save. So if the channel
            // is full, we will just drop this event instead of waiting on a free
            // slot in the channel.
            let _ = common_client_tx.try_send(EventMessage {
                player_id,
                event: event.clone(),
            });
        } else if let Err(err) = common_client_tx
            .send(EventMessage {
                player_id,
                event: event.clone(),
            })
            .await
        {
            println!("Unable to send msg to common client: {}", err);
        }

        // Edge-case to handle disconnects. Need to handle the message/event
        // before starting to remove the now unnused structures.
        if let NetworkEvent::General(GeneralEvent::Disconnected) = event {
            println!(
                "EventMessageContext-handler :: Received disconnect from player with ID: {}",
                player_id
            );

            let mut event_ctx_guard = event_ctx.lock().unwrap();
            client_channels_tx.remove(&player_id);
            if let Some(rx) = event_ctx_guard.client_channels.remove(&player_id) {
                rx.close();
            }
            event_ctx_guard.client_websockets.remove(&player_id);
        }
    }

    println!("Stopped the EventMessageContext-handler.");
}

/// Listens and accepts new websocket connections.
///
/// When a new connection is established, this function spawns a
/// `websocket_client_handler()` which handles all communication with the
/// specific client.
async fn websocket_listener(
    server_addr: SocketAddr,
    id_generator: Arc<Mutex<PlayerIdGenerator>>,
    channel_tx: Sender<EventMessage>,
) {
    let listener = match TcpListener::bind(&server_addr).await {
        Ok(listener) => listener,
        Err(err) => {
            println!(
                "Unable to create listener on addr {}: {:#?}",
                server_addr, err
            );
            return;
        }
    };

    println!("Listening on addr: {}", server_addr);

    loop {
        match listener.accept().await {
            Ok((stream, client_addr)) => {
                let player_id = id_generator.lock().unwrap().generate();
                smol::spawn(websocket_client_handler(
                    player_id,
                    channel_tx.clone(),
                    stream,
                    client_addr,
                ))
                .detach();
            }
            Err(err) => {
                println!(
                    "Error accepting connection on addr {}: {:#?}",
                    server_addr, err
                );
                break;
            }
        };
    }

    println!("Stopped listening on addr: {}", server_addr);
}

/// Handles all communication with one specific client.
///
/// When a new client connects to the server, one of these function will be
/// spawned which will work as a proxy to read/write data between the client and
/// the variables in the `NetworkContext`.
async fn websocket_client_handler(
    player_id: PlayerId,
    channel_tx: Sender<EventMessage>,
    client_stream: TcpStream,
    client_addr: SocketAddr,
) {
    println!("Started client handler for player with ID: {}.", player_id);

    let (client_tx, mut client_rx) = match async_tungstenite::accept_async(client_stream).await {
        Ok(websocket_stream) => websocket_stream.split(),
        Err(err) => {
            println!(
                "Unable to create websocket connection on addr {}: {:#?}",
                client_addr, err
            );
            return;
        }
    };

    if let Err(err) = channel_tx
        .send(EventMessage {
            player_id,
            event: NetworkEvent::General(GeneralEvent::Connected(Some(client_tx))),
        })
        .await
    {
        println!(
            "Unable to put connect message into internal channel: {:#?}",
            err
        );
    }

    loop {
        let msg_result = match client_rx.next().await {
            Some(msg_result) => msg_result,
            None => {
                println!("Channel closed from client with player id {}.", player_id);
                break;
            }
        };

        let msg = match msg_result {
            Ok(msg) => msg,
            Err(err) => {
                println!(
                    "Received error from client with player id {}: {}",
                    player_id, err
                );
                break;
            }
        };

        let event = decode_message(&msg.into_data());
        if let Err(err) = channel_tx.send(EventMessage { player_id, event }).await {
            println!("Unable to put message into internal channel: {:#?}", err);
        }
    }

    if let Err(err) = channel_tx
        .send(EventMessage {
            player_id,
            event: NetworkEvent::General(GeneralEvent::Disconnected),
        })
        .await
    {
        println!(
            "Unable to put disconnect message into internal channel: {:#?}",
            err
        );
    }

    println!("Stopped client handler for player with ID: {}.", player_id);
}
