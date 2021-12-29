use std::ops::{Deref, DerefMut};

use bevy::{
    core::{Time, Timer},
    ecs::component::Component,
    prelude::{
        BuildChildren, Children, Color, Commands, Entity, HorizontalAlign, Query, Res, Transform,
        VerticalAlign, With,
    },
    text::{Text, Text2dBundle, TextAlignment, TextStyle},
};

use crate::{despawn_entity, Fonts};

/// Tag used on entities that should only exists during the start time.
/// All entities with this tag will be removed when the game starts.
pub struct StartEntity;

/// Timer used to "pause" the game a short amount of time before it starts.
/// This is done to allow the players to find their position on the screen before
/// the snakes starts moving.
pub struct StartTimer(Timer);

impl StartTimer {
    pub fn new(start_time: usize) -> Self {
        Self(Timer::from_seconds(start_time as f32, false))
    }
}

impl Deref for StartTimer {
    type Target = Timer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for StartTimer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Setup system that is used to setup the StartTimer.
///
/// The `T` type is used to tag the entity so that it can be automatically
/// removed when running the `despawn_system::<T>()` system.
///
/// The `N` is how long the start timer should run in seconds.
///
/// As long as the game uses the `despawn_system`, there is no need to have a
/// special teardown system for the StartTimer.
pub fn setup_start_timer<T, const N: usize>(mut commands: Commands)
where
    T: Component + Default,
{
    commands
        .spawn()
        .insert(StartTimer::new(N))
        .insert(T::default());
}

/// System that handles the start timer.
///
/// The countdown text is shown in the center of the screen. Any entities tagged
/// with `StartEntity` will be removed when the game starts.
pub fn handle_start_timer(
    mut commands: Commands,
    time: Res<Time>,
    fonts: Res<Fonts>,
    mut start_timer_query: Query<(Entity, &mut StartTimer, Option<&Children>)>,
    start_entities_query: Query<Entity, With<StartEntity>>,
) {
    let (entity, mut start_timer, children) = start_timer_query.single_mut().unwrap();

    let elapsed_before = start_timer.elapsed_secs();
    start_timer.tick(time.delta());
    let elapsed_after = start_timer.elapsed_secs();

    if start_timer.just_finished() {
        // StartTimer just finished, remove the StartText & StartEntity's from
        // the screen.
        if let Some(children) = children {
            for entity in children.iter() {
                despawn_entity(&mut commands, *entity);
            }
        }
        for entity in start_entities_query.iter() {
            despawn_entity(&mut commands, entity);
        }
    } else if start_timer.finished() {
        // The StartTimer have finished previously; the game is already rinning,
        // nothing to do here.
    } else if elapsed_after as usize - elapsed_before as usize >= 1 || elapsed_before == 0.0 {
        // The StartTimer is running and just passed a "whole number" second or
        // the timer was just reset. Remove the potential old StartText and create
        // a new one to show the next second.
        if let Some(children) = children {
            for entity in children.iter() {
                despawn_entity(&mut commands, *entity);
            }
        }

        let secs_remainig = start_timer.duration().as_secs() - elapsed_after as u64;

        let start_text = Text::with_section(
            secs_remainig.to_string(),
            TextStyle {
                font: fonts.bold.clone(),
                font_size: 256.0,
                color: Color::rgb(1.0, 1.0, 1.0),
            },
            TextAlignment {
                vertical: VerticalAlign::Center,
                horizontal: HorizontalAlign::Center,
            },
        );

        let start_text_bundle = Text2dBundle {
            text: start_text,
            transform: Transform::from_xyz(0.0, 0.0, 0.0),
            ..Default::default()
        };

        let text_entity = commands
            .spawn_bundle(start_text_bundle)
            .insert(StartEntity)
            .id();

        commands.entity(entity).push_children(&[text_entity]);
    } else {
        // The StartTimer is running but nothing to do here since we only update
        // the StartText every whole second.
    }
}
