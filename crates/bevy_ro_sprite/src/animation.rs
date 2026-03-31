use crate::loader::RoAtlas;
use bevy::{
    app::{App, Plugin, PostUpdate, PreUpdate},
    ecs::component::Mutable,
    image::TextureAtlas,
    prelude::*,
    sprite::Sprite,
    sprite_render::Material2d,
};
use std::time::Duration;

pub struct RoAnimationPlugin;
impl Plugin for RoAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, update_ro_animation);
        app.add_systems(PostUpdate, render_animation::<Sprite>);
        app.register_type::<RoAnimation>();
        app.register_type::<RoAnimationState>();
        app.register_type::<AnimationRepeat>();
    }
}

/// Triggered when an animation frame with an ACT event is reached during playback.
///
/// Observe globally with `app.add_observer(|t: On<SpriteFrameEvent>| { ... })`, or
/// on a specific entity with `commands.entity(e).observe(...)`.
///
/// Event strings are set by the RO client toolchain and typically contain either a
/// logical trigger name (e.g. `"atk"`) or a sound file reference (e.g. `"attack.wav"`).
#[derive(EntityEvent, Debug, Clone)]
pub struct SpriteFrameEvent {
    /// The entity whose animation produced this event (also used as the observer target).
    pub entity: Entity,
    /// The ACT event string (e.g. `"atk"`, `"attack.wav"`).
    pub event: String,
    /// The animation tag active when the event fired (e.g. `"attack1_s"`).
    pub tag: Option<String>,
}

/// Anything that implements this trait can be a render target for [`RoAnimation`].
pub trait RenderAnimation {
    type Extra<'e>;
    fn render_animation(
        &mut self,
        atlas: &RoAtlas,
        state: &RoAnimationState,
        extra: &mut Self::Extra<'_>,
    );
}

impl RenderAnimation for Sprite {
    type Extra<'e> = ();
    fn render_animation(&mut self, atlas: &RoAtlas, state: &RoAnimationState, _extra: &mut ()) {
        self.image = atlas.atlas_image.clone();
        self.texture_atlas = Some(TextureAtlas {
            layout: atlas.atlas_layout.clone(),
            index: atlas.get_atlas_index(usize::from(state.current_frame)),
        });
    }
}

impl<M: Material2d + RenderAnimation> RenderAnimation for MeshMaterial2d<M> {
    type Extra<'e> = (ResMut<'e, Assets<M>>, <M as RenderAnimation>::Extra<'e>);
    fn render_animation(
        &mut self,
        atlas: &RoAtlas,
        state: &RoAnimationState,
        extra: &mut Self::Extra<'_>,
    ) {
        let Some(material) = extra.0.get_mut(&*self) else {
            return;
        };
        material.render_animation(atlas, state, &mut extra.1);
    }
}

#[cfg(feature = "3d")]
impl<M: Material + RenderAnimation> RenderAnimation for MeshMaterial3d<M> {
    type Extra<'e> = (ResMut<'e, Assets<M>>, <M as RenderAnimation>::Extra<'e>);
    fn render_animation(
        &mut self,
        atlas: &RoAtlas,
        state: &RoAnimationState,
        extra: &mut Self::Extra<'_>,
    ) {
        let Some(material) = extra.0.get_mut(&*self) else {
            return;
        };
        material.render_animation(atlas, state, &mut extra.1);
    }
}

pub fn render_animation<T: RenderAnimation + Component<Mutability = Mutable>>(
    mut animations: Query<(&RoAnimation, &mut T, &RoAnimationState)>,
    atlases: Res<Assets<RoAtlas>>,
    mut extra: <T as RenderAnimation>::Extra<'_>,
) {
    for (animation, mut target, state) in &mut animations {
        let Some(atlas) = atlases.get(&animation.atlas) else {
            continue;
        };
        target.render_animation(atlas, state, &mut extra);
    }
}

#[derive(Component, Default, Reflect, Clone, Debug)]
#[require(RoAnimationState)]
#[reflect]
pub struct RoAnimation {
    pub atlas: Handle<RoAtlas>,
    pub animation: RoAnimationControl,
}

#[derive(Default, Reflect, Clone, Debug)]
#[reflect]
pub struct RoAnimationControl {
    pub tag: Option<String>,
    pub playing: bool,
    pub speed: f32,
    pub repeat: AnimationRepeat,
}

impl RoAnimationControl {
    pub fn tag(tag: &str) -> Self {
        Self {
            tag: Some(tag.to_string()),
            playing: true,
            speed: 1.0,
            repeat: AnimationRepeat::Loop,
        }
    }
}

#[derive(Component, Default, Reflect, Debug)]
#[reflect]
pub struct RoAnimationState {
    pub current_frame: u16,
    pub elapsed: Duration,
}

#[derive(Default, Debug, Clone, Reflect)]
#[reflect]
pub enum AnimationRepeat {
    #[default]
    Loop,
    Count(u32),
}

pub fn update_ro_animation(
    mut animations: Query<(Entity, &mut RoAnimation, &mut RoAnimationState)>,
    atlases: Res<Assets<RoAtlas>>,
    time: Res<Time>,
    mut commands: Commands,
) {
    for (entity, mut animation, mut state) in animations.iter_mut() {
        let Some(atlas) = atlases.get(&animation.atlas) else {
            continue;
        };

        let range = match animation.animation.tag.as_ref() {
            Some(tag) => match atlas.tags.get(tag) {
                Some(meta) => meta.range.clone(),
                None => continue,
            },
            None => 0..=(atlas.frame_durations.len().saturating_sub(1) as u16),
        };

        if !range.contains(&state.current_frame) {
            state.current_frame = *range.start();
            state.elapsed = Duration::ZERO;
        }

        if !animation.animation.playing {
            continue;
        }

        state.elapsed +=
            Duration::from_secs_f32(time.delta_secs() * animation.animation.speed.max(0.0));

        let Some(frame_dur) = atlas.frame_durations.get(usize::from(state.current_frame)) else {
            continue;
        };

        if state.elapsed >= *frame_dur {
            state.elapsed =
                Duration::from_secs_f32(state.elapsed.as_secs_f32() % frame_dur.as_secs_f32());

            let next = state.current_frame + 1;
            let new_frame = if next > *range.end() {
                match animation.animation.repeat {
                    AnimationRepeat::Loop => {
                        *range.start()
                    }
                    AnimationRepeat::Count(ref mut n) if *n > 0 => {
                        *n -= 1;
                        *range.start()
                    }
                    AnimationRepeat::Count(_) => {
                        animation.animation.playing = false;
                        continue;
                    }
                }
            } else {
                next
            };

            state.current_frame = new_frame;

            if let Some(Some(event)) = atlas.frame_events.get(usize::from(new_frame)) {
                commands.trigger(SpriteFrameEvent {
                    entity,
                    event: event.clone(),
                    tag: animation.animation.tag.clone(),
                });
            }
        }
    }
}
