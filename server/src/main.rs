use std::sync::{Arc, Mutex};

use bevy::{
    app::Events,
    input::{keyboard::KeyboardInput, ElementState},
    prelude::*,
    render::{
        camera::{Camera, OrthographicProjection, ScalingMode},
        pipeline::{PipelineDescriptor, RenderPipeline},
        shader::{ShaderStage, ShaderStages},
        texture::ImageType,
    },
    window::{WindowMode, WindowResized},
    DefaultPlugins,
};
use bevy_prototype_lyon::plugin::ShapePlugin;
use bevy_rapier2d::prelude::*;
use rand::Rng;
use smol::io;

use skitspel::{
    ActionEvent, ConnectedPlayers, DisconnectedPlayers, GameState, Player, Players, COLORS,
    GAME_HEIGHT, GAME_WIDTH, RAPIER_SCALE_FACTOR,
};
use util_bevy::{Fonts, Game, Games, VoteEvent};

use achtung::AchtungGamePlugin;
use hockey::HockeyGamePlugin;
use network::{
    EventMessage, EventTimer, GeneralEvent, NetworkContext, NetworkEvent, NetworkPlugin,
};
use push::PushGamePlugin;
use selection_menu::GameSelectionPlugin;
use start_menu::MenuPlugin;
use volleyball::VolleyBallGamePlugin;

fn main() -> io::Result<()> {
    std::env::set_var("SMOL_THREADS", num_cpus::get().to_string());
    smol::block_on(async {
        App::build()
            .insert_resource(WindowDescriptor {
                title: "skitspel".to_string(),
                width: GAME_WIDTH,
                height: GAME_HEIGHT,
                ..Default::default()
            })
            .insert_resource(ClearColor(Color::rgb(0.1, 0.1, 0.1)))
            .insert_resource(Msaa { samples: 4 })
            .init_resource::<Players>()
            .init_resource::<ConnectedPlayers>()
            .init_resource::<DisconnectedPlayers>()
            .init_resource::<Games>()
            .init_resource::<Fonts>()
            .add_event::<VoteEvent>()
            .add_plugins(DefaultPlugins)
            .add_plugin(ShapePlugin)
            .add_plugin(RapierPhysicsPlugin::<NoUserData>::default())
            .add_plugin(RapierRenderPlugin)
            .add_plugin(NetworkPlugin)
            .add_plugin(MenuPlugin)
            .add_plugin(GameSelectionPlugin)
            .add_plugin(PushGamePlugin)
            .add_plugin(HockeyGamePlugin)
            .add_plugin(VolleyBallGamePlugin)
            .add_plugin(AchtungGamePlugin)
            .add_state(GameState::StartMenu)
            .add_startup_system(common_setup.system())
            .add_system(camera_scaling_fix.system())
            .add_system(handle_general_message.system())
            .add_system(handle_action_message.system())
            .add_system(handle_fullscreen.system())
            .run();
        Ok(())
    })
}

#[allow(clippy::too_many_arguments)]
pub fn common_setup(
    mut commands: Commands,
    mut font_assets: ResMut<Assets<Font>>,
    mut textures: ResMut<Assets<Texture>>,
    mut pipelines: ResMut<Assets<PipelineDescriptor>>,
    mut shaders: ResMut<Assets<Shader>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut configuration: ResMut<RapierConfiguration>,
    mut fonts: ResMut<Fonts>,
    mut games: ResMut<Games>,
) {
    fonts.bold = font_assets.add(
        Font::try_from_bytes(include_bytes!("..\\assets\\FiraSans-Bold.ttf").to_vec()).unwrap(),
    );
    fonts.regular = font_assets.add(
        Font::try_from_bytes(include_bytes!("..\\assets\\FiraMono-Medium.ttf").to_vec()).unwrap(),
    );

    games.push(Game {
        name: "Push",
        game_state: GameState::PushGame,
        screenshot: materials.add(
            textures
                .add(
                    Texture::from_buffer(
                        include_bytes!("..\\assets\\push.png"),
                        ImageType::Extension("png"),
                    )
                    .unwrap(),
                )
                .into(),
        ),
    });
    games.push(Game {
        name: "Hockey",
        game_state: GameState::HockeyGame,
        screenshot: materials.add(
            textures
                .add(
                    Texture::from_buffer(
                        include_bytes!("..\\assets\\hockey.png"),
                        ImageType::Extension("png"),
                    )
                    .unwrap(),
                )
                .into(),
        ),
    });
    games.push(Game {
        name: "Volleyball",
        game_state: GameState::VolleyBallGame,
        screenshot: materials.add(
            textures
                .add(
                    Texture::from_buffer(
                        include_bytes!("..\\assets\\volleyball.png"),
                        ImageType::Extension("png"),
                    )
                    .unwrap(),
                )
                .into(),
        ),
    });
    games.push(Game {
        name: "Achtung die Kurve",
        game_state: GameState::AchtungGame,
        screenshot: materials.add(
            textures
                .add(
                    Texture::from_buffer(
                        include_bytes!("..\\assets\\achtung.png"),
                        ImageType::Extension("png"),
                    )
                    .unwrap(),
                )
                .into(),
        ),
    });

    let pipeline_handle = pipelines.add(PipelineDescriptor::default_config(ShaderStages {
        vertex: shaders.add(Shader::from_glsl(ShaderStage::Vertex, VERTEX_SHADER)),
        fragment: Some(shaders.add(Shader::from_glsl(ShaderStage::Fragment, FRAGMENT_SHADER))),
    }));
    commands.insert_resource(RenderPipelines::from_pipelines(vec![RenderPipeline::new(
        pipeline_handle,
    )]));

    configuration.scale = RAPIER_SCALE_FACTOR;
    configuration.gravity = [0.0, 0.0].into();

    let mut camera = OrthographicCameraBundle::new_2d();
    camera.orthographic_projection.scaling_mode = ScalingMode::FixedVertical;
    camera.orthographic_projection.scale = GAME_HEIGHT / 2.0;

    commands.spawn_bundle(camera);
}

/// Takes actions according to new "general" messages that aren't related to a
/// specific  players input action. Example of messages that this function
/// handles are:
///  - Player connect.
///  - Player disconnect.
///
/// Player will be added/removed from the `Players` resource. Newly connected/
/// disconnected players will be temporary stored in `ConnectedPlayers` &
/// `DisconnectedPlayers` respectively for one tick. This will allow any game
/// running to easily see the changes and handle them if they want to.
fn handle_general_message(
    event_ctx: Res<Arc<Mutex<NetworkContext>>>,
    mut players: ResMut<Players>,
    mut connected_players: ResMut<ConnectedPlayers>,
    mut disconnected_players: ResMut<DisconnectedPlayers>,
    mut game_state: ResMut<State<GameState>>,
) {
    // The structures containing newly connected/disconnected players are cleared
    // after every tick.
    connected_players.clear();
    disconnected_players.clear();

    let mut event_ctx_guard = event_ctx.lock().unwrap();
    for EventMessage { player_id, event } in event_ctx_guard.iter_common() {
        match event {
            NetworkEvent::General(GeneralEvent::Connected(_)) => {
                let color_idx = rand::thread_rng().gen_range(0..COLORS.len());
                let color = COLORS[color_idx];

                let new_player = Player::new(player_id, color);
                players.insert(player_id, new_player.clone());
                connected_players.insert(player_id, new_player);

                println!("Added new player with ID: {}", player_id);
            }

            NetworkEvent::General(GeneralEvent::Disconnected) => {
                players.remove(&player_id);
                disconnected_players.insert(player_id);

                println!("Removed player with ID: {}", player_id);

                if players.len() < 2 && *game_state.current() != GameState::StartMenu {
                    println!("Less than two people connected, go back to start menu!");
                    game_state.set(GameState::StartMenu).unwrap();
                }
            }

            NetworkEvent::Invalid(data) => {
                println!(
                    "Received invalid message from player with ID {}: {:#?}",
                    player_id, data
                );
            }

            _ => (),
        }
    }
}

/// Takes actions according to new messages from a player related to a specific
/// input action. Example of messages that this function handles are:
///  - Button presses.
///  - Button releases.
///
/// This function parses these inputs from the players and updates the
/// `PlayerAction` stored inside the `Players`.
fn handle_action_message(
    time: Res<Time>,
    mut event_timer: ResMut<EventTimer>,
    mut players: ResMut<Players>,
    event_ctx: Res<Arc<Mutex<NetworkContext>>>,
) {
    let mut event_ctx = event_ctx.lock().unwrap();
    if let Some(action_iter) = event_ctx.iter_action(&time, &mut event_timer) {
        for (player_id, action_event) in action_iter {
            // Accessing the player "mutably" will trigger a change event.
            // Prevent that to happen in the normal case when no action have
            // been performed by the player two ticks in a row. Only access it
            // mutually if something of possible interest have happened.
            if let Some(player) = players.get(&player_id) {
                if player.has_no_action() && matches!(action_event, ActionEvent::None) {
                    continue;
                }
            }
            if let Some(player) = players.get_mut(&player_id) {
                player.update_action(&action_event);
            }
        }
    }
}

/// Toggle fullscreen with F11 or escape.
fn handle_fullscreen(mut key_events: EventReader<KeyboardInput>, mut windows: ResMut<Windows>) {
    for key_event in key_events.iter() {
        if matches!(key_event.key_code, Some(KeyCode::F11 | KeyCode::Escape))
            && matches!(key_event.state, ElementState::Pressed)
        {
            let window = windows.get_primary_mut().unwrap();
            let new_mode = if let WindowMode::Windowed = window.mode() {
                WindowMode::Fullscreen { use_size: false }
            } else {
                WindowMode::Windowed
            };
            window.set_mode(new_mode);
        }
    }
}

/// Updates the scaling of the game viewport according to the size of the window.
/// This ensures that the game is always fully visible in the window.
/// When/if this merge requests get pushed:
///   https://github.com/bevyengine/bevy/pull/3253
/// the `ScalingMode::Auto` should be used and this system should be removed.
fn camera_scaling_fix(
    resize_event: Res<Events<WindowResized>>,
    mut camera_query: Query<&mut OrthographicProjection, With<Camera>>,
) {
    for event in resize_event.get_reader().iter(&resize_event) {
        let game_aspect_ratio = GAME_WIDTH / GAME_HEIGHT;
        let window_aspect_ratio = event.width / event.height;

        for mut projection in camera_query.iter_mut() {
            if window_aspect_ratio > game_aspect_ratio {
                projection.scaling_mode = ScalingMode::FixedVertical;
                projection.scale = GAME_HEIGHT / 2.0;
            } else {
                projection.scaling_mode = ScalingMode::FixedHorizontal;
                projection.scale = GAME_WIDTH / 2.0;
            }
        }
    }
}

pub const VERTEX_SHADER: &str = r"
#version 450
layout(location = 0) in vec2 Vertex_Position;
layout(location = 1) in vec4 Vertex_Color;
layout(location = 1) out vec4 v_Color;
layout(set = 0, binding = 0) uniform CameraViewProj {
    mat4 ViewProj;
};
layout(set = 1, binding = 0) uniform Transform {
    mat4 Model;
};
void main() {
    v_Color = Vertex_Color;
    gl_Position = ViewProj * Model * vec4(Vertex_Position, 0.0, 1.0);
}
";

pub const FRAGMENT_SHADER: &str = r"
#version 450
layout(location = 1) in vec4 v_Color;
layout(location = 0) out vec4 o_Target;
void main() {
    o_Target = v_Color;
}
";
