use bevy::{
    ecs::component::Component,
    prelude::{Commands, DespawnRecursiveExt, Entity, Query, With},
};

/// This system can be used to despawn every enitity that contains the component `T`.
///
/// When creating a game, one should add a specific component/tag to all things
/// that are spawned. For example the "push game" will tag all its entities with
/// the component `PushGamePlugin`. We can then register this system to run with
/// `T = PushGamePlugin` in the "push games" `on_exit` which automatically despawn
/// everything related to the game.
pub fn despawn_system<T: Component>(mut commands: Commands, query: Query<Entity, With<T>>) {
    for entity in query.iter() {
        despawn_entity(&mut commands, entity);
    }
}

pub fn despawn_entity(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).despawn_recursive();
}
