use bevy::{
    ecs::{component::Component, system::EntityCommands},
    prelude::{Assets, BuildChildren, Color, Commands, Mesh, MeshBundle, RenderPipelines, Shader},
    render::{
        pipeline::{PipelineDescriptor, RenderPipeline},
        shader::{ShaderStage, ShaderStages},
    },
};
use bevy_rapier2d::prelude::{ActiveEvents, ColliderType};

use skitspel::{GAME_HEIGHT, GAME_WIDTH};

use crate::{create_path_with_thickness, FRAGMENT_SHADER, VERTEX_SHADER};

/// Spawns a border around the screen with collision.
///
/// If the given `collider_type` is `ColliderType::Sensor`, the spawned colliders
/// will be assigned the tag `collider_tag`. If this is a `ColliderType::Solid`,
/// the `collider_tag` will be ignored.
#[allow(clippy::too_many_arguments)]
pub fn spawn_border_walls<'a, 'b, T>(
    commands: &'b mut Commands<'a>,
    meshes: &mut Assets<Mesh>,
    pipelines: &mut Assets<PipelineDescriptor>,
    shaders: &mut Assets<Shader>,
    color: Color,
    thickness: f32,
    collider_type: ColliderType,
    collider_tag: Option<T>,
) -> EntityCommands<'a, 'b>
where
    T: Component + Clone,
{
    let pipeline_handle = pipelines.add(PipelineDescriptor::default_config(ShaderStages {
        vertex: shaders.add(Shader::from_glsl(ShaderStage::Vertex, VERTEX_SHADER)),
        fragment: Some(shaders.add(Shader::from_glsl(ShaderStage::Fragment, FRAGMENT_SHADER))),
    }));

    let render_pipelines =
        RenderPipelines::from_pipelines(vec![RenderPipeline::new(pipeline_handle)]);

    // Since these walls should be as a "border" around the screen, need to draw
    // them `thickness / 2` units away from the screen border so that the whole
    // path is shown inside the screen.
    let ht = thickness / 2.0;
    let vertices = [
        ((-GAME_WIDTH / 2.0) + ht, (GAME_HEIGHT / 2.0) - ht).into(),
        ((GAME_WIDTH / 2.0) - ht, (GAME_HEIGHT / 2.0) - ht).into(),
        ((GAME_WIDTH / 2.0) - ht, (-GAME_HEIGHT / 2.0) + ht).into(),
        ((-GAME_WIDTH / 2.0) + ht, (-GAME_HEIGHT / 2.0) + ht).into(),
    ];

    let active_events = if let ColliderType::Sensor = collider_type {
        ActiveEvents::INTERSECTION_EVENTS
    } else {
        ActiveEvents::empty()
    };

    let (mesh, colliders) = create_path_with_thickness(
        &vertices,
        color,
        thickness,
        collider_type,
        active_events,
        true,
    );

    let mut entity_commands = commands.spawn_bundle(MeshBundle {
        mesh: meshes.add(mesh),
        render_pipelines,
        ..Default::default()
    });

    colliders.into_iter().for_each(|collider| {
        entity_commands.with_children(|parent| {
            let mut collider_commands = parent.spawn_bundle(collider);
            if let ColliderType::Sensor = collider_type {
                if let Some(collider_tag) = collider_tag.clone() {
                    collider_commands.insert(collider_tag);
                }
            }
        });
    });

    entity_commands
}
