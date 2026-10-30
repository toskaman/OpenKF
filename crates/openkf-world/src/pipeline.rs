use std::time::Duration;

/// Minimum and maximum delta seconds decompiled from `Engine.dll:0x1044fffe`.
pub const MIN_DELTA_SECONDS: f32 = 0.0005; // Prevents division by zero in collision/physics
pub const MAX_DELTA_SECONDS: f32 = 0.20;   // Prevents spiral of death on framerate lag spikes

/// Tick update mode corresponding to UE2 `ELevelTick`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TickType {
    /// Update all actors and world subsystems.
    #[default]
    All,
    /// Update only players/viewports.
    ViewportsOnly,
    /// Synchronize only (pause or catchup).
    Synchronize,
}

/// Simulation metrics recorded during a single `ULevel::Tick` execution.
#[derive(Debug, Clone, Default)]
pub struct SimulationTickReport {
    pub raw_delta_seconds: f32,
    pub effective_delta_seconds: f32,
    pub total_time_seconds: f64,
    pub dynamic_actors_ticked: usize,
    pub static_actors_skipped: usize,
    pub deleted_actors_skipped: usize,
    pub actors_despawned_cleanup: usize,
    pub tick_duration: Duration,
}

/// Simulation tick pipeline manager executing `ULevel::Tick` flow.
pub struct TickPipeline;

impl TickPipeline {
    /// Calculates the clamped effective delta time matching `0x1044fffe`:
    /// `EffectiveDelta = clamp(RawDeltaSeconds * TimeDilation, 0.0005, 0.20)`.
    #[inline]
    pub fn compute_effective_dt(raw_dt: f32, time_dilation: f32) -> f32 {
        let dilated = raw_dt * time_dilation;
        dilated.clamp(MIN_DELTA_SECONDS, MAX_DELTA_SECONDS)
    }
}
