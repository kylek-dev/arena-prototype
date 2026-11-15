//! Startup render warmup behind the loading screen. Presentation only: no
//! clocks or state enter the simulation.
//!
//! Bevy uploads meshes/images and compiles render pipelines lazily, for what a
//! camera actually draws, and on the web each new shader variant can stall a
//! frame. The loading overlay therefore stays up while the scene renders behind
//! it. Once assets are loaded the camera sweeps four headings around the player
//! (so scenery outside the first view is prepared too) and the Pulse lightning is
//! drawn. Readiness then needs a full sweep plus a settled home view in which no
//! GPU upload is pending and no new pipeline appears; any new work restarts it.
use std::{collections::HashSet, f32::consts::TAU};

use bevy::{
    light::Skybox,
    prelude::*,
    render::{
        ExtractSchedule, MainWorld, RenderApp, mesh::RenderMesh, render_asset::RenderAssets,
        render_resource::PipelineCache, texture::GpuImage,
    },
};

const SWEEP_VIEWS: u32 = 4;
/// Long enough for a view's new pipelines to show up in the render report
/// (queued during one render frame, seen at the next extraction).
const FRAMES_PER_VIEW: u32 = 6;
const HOME_FRAMES: u32 = 10;
const SETTLE_SECONDS: f32 = 0.35;
/// Never trap the player behind the overlay: an asset that never reaches the
/// GPU, or a driver that keeps compiling, ends the warmup instead.
const MAX_WARMUP_SECONDS: f32 = 15.0;
/// Arena textures and sky are optional decoration; the character is not.
const MAX_SCENERY_SECONDS: f32 = 60.0;

/// What the render world saw at the last extraction.
#[derive(Resource, Default, Clone, Copy)]
pub(crate) struct Renderer {
    frame: u64,
    pending: usize,
    pipelines: usize,
}

#[derive(Resource, Default)]
pub struct State {
    phase: &'static str,
    ready: bool,
    assets_done: bool,
    timed_out: bool,
    stable: u32,
    seconds: f32,
    elapsed: f32,
    warming: f32,
    last_frame: u64,
    last_pipelines: usize,
    pending: usize,
}

impl State {
    pub fn ready(&self) -> bool {
        self.ready
    }

    /// Camera yaw offset while warming up behind the overlay, else `None`.
    pub fn sweep(&self) -> Option<f32> {
        (self.assets_done && !self.ready).then(|| {
            let view = self.stable / FRAMES_PER_VIEW;
            if view < SWEEP_VIEWS {
                view as f32 * TAU / SWEEP_VIEWS as f32
            } else {
                0.0
            }
        })
    }

    pub fn json(&self) -> String {
        format!(
            r#"{{"phase":"{}","ready":{},"pending":{},"timedOut":{}}}"#,
            if self.phase.is_empty() {
                "assets"
            } else {
                self.phase
            },
            self.ready,
            self.pending,
            self.timed_out
        )
    }

    fn advance(&mut self, character_done: bool, scenery_done: bool, render: Renderer, dt: f32) {
        if self.ready {
            return;
        }
        let dt = if dt.is_finite() && (0.0..=0.1).contains(&dt) {
            dt
        } else {
            0.0
        };
        self.elapsed += dt;
        self.pending = render.pending;
        let fresh = render.frame != self.last_frame;
        let compiled = render.pipelines != self.last_pipelines;
        self.last_frame = render.frame;
        self.last_pipelines = render.pipelines;
        self.assets_done = character_done && (scenery_done || self.elapsed > MAX_SCENERY_SECONDS);
        if !self.assets_done {
            self.phase = "assets";
            self.stable = 0;
            self.seconds = 0.0;
            return;
        }
        self.warming += dt;
        if self.warming > MAX_WARMUP_SECONDS {
            self.ready = true;
            self.timed_out = true;
            self.phase = "ready";
            return;
        }
        if !fresh {
            return;
        }
        if render.pending > 0 || compiled || render.pipelines == 0 || dt == 0.0 {
            self.stable = 0;
            self.seconds = 0.0;
            self.phase = "render";
            return;
        }
        self.phase = "warming";
        self.stable += 1;
        self.seconds += dt;
        if self.stable >= SWEEP_VIEWS * FRAMES_PER_VIEW + HOME_FRAMES
            && self.seconds >= SETTLE_SECONDS
        {
            self.ready = true;
            self.phase = "ready";
        }
    }
}

pub struct StartupPlugin;
impl Plugin for StartupPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<State>().init_resource::<Renderer>();
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.add_systems(ExtractSchedule, inspect_renderer);
        }
    }
}

pub(crate) fn update(
    mut state: ResMut<State>,
    render: Res<Renderer>,
    time: Res<Time>,
    character: Res<crate::character::CharacterModel>,
    textures: Res<crate::textures::ArenaTextures>,
    sky: Res<crate::sky::Sky>,
) {
    state.advance(
        character.status() != "loading",
        textures.ready() && sky.ready(),
        *render,
        time.delta_secs(),
    );
}

/// Read render state during extraction, when MainWorld is accessible: GPU
/// meshes for every spawned mesh, GPU images for every material/sky texture,
/// and the pipeline cache, including pipelines still waiting to compile.
fn inspect_renderer(
    mut main: ResMut<MainWorld>,
    pipelines: Res<PipelineCache>,
    images: Res<RenderAssets<GpuImage>>,
    meshes: Res<RenderAssets<RenderMesh>>,
) {
    if main.get_resource::<State>().is_none_or(State::ready) {
        return;
    }
    let mut mesh_query = main.query::<&Mesh3d>();
    let required_meshes: HashSet<_> = mesh_query.iter(&main).map(|m| m.0.id()).collect();
    let mut required_images = HashSet::new();
    for (_, material) in main.resource::<Assets<StandardMaterial>>().iter() {
        for handle in [
            &material.base_color_texture,
            &material.normal_map_texture,
            &material.metallic_roughness_texture,
            &material.occlusion_texture,
            &material.emissive_texture,
        ]
        .into_iter()
        .flatten()
        {
            required_images.insert(handle.id());
        }
    }
    let mut sky_query = main.query::<&Skybox>();
    for sky in sky_query.iter(&main) {
        if let Some(image) = &sky.image {
            required_images.insert(image.id());
        }
    }
    let pending = pipelines.waiting_pipelines().count()
        + required_meshes
            .iter()
            .filter(|id| meshes.get(**id).is_none())
            .count()
        + required_images
            .iter()
            .filter(|id| images.get(**id).is_none())
            .count();
    let mut report = main.resource_mut::<Renderer>();
    report.frame += 1;
    report.pending = pending;
    report.pipelines = pipelines.pipelines().count();
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn run(state: &mut State, render: &mut Renderer, frames: u32) {
        for _ in 0..frames {
            render.frame += 1;
            state.advance(true, true, *render, DT);
        }
    }

    #[test]
    fn waits_for_assets_gpu_uploads_and_a_full_quiet_sweep() {
        let mut state = State::default();
        let mut render = Renderer {
            pipelines: 3,
            ..default()
        };
        for _ in 0..100 {
            render.frame += 1;
            state.advance(false, true, render, DT);
        }
        assert!(!state.ready());
        assert_eq!(state.sweep(), None, "no sweep before the character loads");
        render.pending = 1;
        run(&mut state, &mut render, 100);
        assert!(!state.ready());
        render.pending = 0;
        // The sweep visits four headings, then settles on the home view.
        let mut headings = Vec::new();
        while !state.ready() {
            headings.push(state.sweep().unwrap());
            run(&mut state, &mut render, 1);
        }
        assert_eq!(state.sweep(), None);
        let mut distinct = headings.clone();
        distinct.dedup();
        assert_eq!(
            distinct,
            vec![0.0, TAU / 4.0, TAU / 2.0, 3.0 * TAU / 4.0, 0.0]
        );
        assert!(state.json().contains(r#""ready":true"#));
        assert!(!state.timed_out);
        state.advance(false, false, Renderer::default(), DT);
        assert!(
            state.ready(),
            "later graphics changes must not re-lock startup"
        );
    }

    #[test]
    fn a_new_pipeline_restarts_the_sweep_and_stale_reports_do_not_count() {
        let mut state = State::default();
        let mut render = Renderer {
            pipelines: 3,
            ..default()
        };
        run(&mut state, &mut render, 2 * FRAMES_PER_VIEW + 1);
        assert_eq!(state.sweep(), Some(TAU / 2.0));
        // A view revealed a new shader variant: start over from the first view.
        render.pipelines += 1;
        run(&mut state, &mut render, 1);
        assert_eq!(state.sweep(), Some(0.0));
        assert_eq!(state.stable, 0);
        // Repeated reports of the same render frame cannot advance readiness.
        for _ in 0..200 {
            state.advance(true, true, render, DT);
        }
        assert!(!state.ready());
        assert_eq!(state.stable, 0);
    }

    #[test]
    fn stuck_gpu_work_or_missing_scenery_cannot_trap_the_player() {
        let mut state = State::default();
        let mut render = Renderer {
            pipelines: 3,
            pending: 1,
            ..default()
        };
        let frames = (MAX_WARMUP_SECONDS / DT) as u32 + 2;
        run(&mut state, &mut render, frames);
        assert!(state.ready());
        assert!(state.json().contains(r#""timedOut":true"#));
        // Scenery that never loads stops blocking after its own limit.
        let mut state = State::default();
        let mut render = Renderer {
            pipelines: 3,
            ..default()
        };
        for _ in 0..((MAX_SCENERY_SECONDS / 0.1) as u32 + 2) {
            render.frame += 1;
            state.advance(true, false, render, 0.1);
        }
        assert!(state.sweep().is_some());
        // The character, unlike scenery, is always waited for.
        let mut state = State::default();
        for _ in 0..2000 {
            render.frame += 1;
            state.advance(false, true, render, 0.1);
        }
        assert!(!state.ready());
    }
}
