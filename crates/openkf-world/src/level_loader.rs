//! # OpenKF Level Loader & ROM Package Spawner
//!
//! Clean-room, high-level map loader that parses Unreal Engine 2.5 `.rom` packages,
//! extracts Binary Space Partitioning (BSP) world collision geometry (`UModel`),
//! instantiates specimen spawn points (`ZombieVolume`), trader zones (`ShopVolume`),
//! navigation path nodes (`PathNode`), and player starts (`PlayerStart`), registering
//! them directly into the archetype-backed `hecs` ECS simulation world.

use byteorder::{ByteOrder, LittleEndian};
use glam::Vec3;
use hecs::Entity;
use openkf_bsp::{BspError, FBspNode, FBspSurf, UModel, UModelParser};
use openkf_package::{
    read_compact_index, FObjectExport, MmapPackage, PackageError, UnrealPackage,
};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

use crate::components::{ActorState, SpatialIndex, Transform};
use crate::types::Rotator;
use crate::world::World;

/// RF_HasStack (0x02000000) indicates an object has a serialized FStateFrame header.
const RF_HAS_STACK: u32 = 0x02000000;

/// Default maximum connection distance between adjacent navigation PathNodes in Unreal Units.
pub const DEFAULT_MAX_PATH_REACH_DISTANCE: f32 = 1200.0;

/// Errors that can occur during level loading and package parsing.
#[derive(Error, Debug)]
pub enum LevelLoadError {
    #[error("IO error loading level: {0}")]
    Io(#[from] std::io::Error),

    #[error("Unreal Package error: {0}")]
    Package(#[from] PackageError),

    #[error("BSP parsing error: {0}")]
    Bsp(#[from] BspError),

    #[error("Invalid ROM map: {0}")]
    InvalidRom(String),

    #[error("Level property error: {0}")]
    PropertyError(String),
}

/// Metadata and statistics summary for a loaded level.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelSummary {
    /// Level display name or title.
    pub title: String,
    /// Level author or creator.
    pub author: String,
    /// Detailed level description.
    pub description: String,
    /// Recommended minimum player count.
    pub ideal_player_count_min: i32,
    /// Recommended maximum player count.
    pub ideal_player_count_max: i32,
    /// Total count of compiled BSP nodes in the world.
    pub bsp_nodes_count: usize,
    /// Total count of BSP surface polygons.
    pub bsp_surfs_count: usize,
    /// Total count of shared 3D BSP vertex points.
    pub bsp_points_count: usize,
    /// Total count of navigation path nodes spawned.
    pub path_nodes_count: usize,
    /// Total count of specimen / Zed spawn volumes spawned.
    pub specimen_spawns_count: usize,
    /// Total count of trader shop zones spawned.
    pub trader_zones_count: usize,
    /// Total count of player spawn points spawned.
    pub player_starts_count: usize,
    /// Total count of ECS entities instantiated.
    pub total_actors_spawned: usize,
    /// Minimum world-space bounding coordinate.
    pub bounds_min: Vec3,
    /// Maximum world-space bounding coordinate.
    pub bounds_max: Vec3,
}

impl Default for LevelSummary {
    fn default() -> Self {
        Self {
            title: "Untitled Level".to_string(),
            author: "Unknown Author".to_string(),
            description: String::new(),
            ideal_player_count_min: 1,
            ideal_player_count_max: 6,
            bsp_nodes_count: 0,
            bsp_surfs_count: 0,
            bsp_points_count: 0,
            path_nodes_count: 0,
            specimen_spawns_count: 0,
            trader_zones_count: 0,
            player_starts_count: 0,
            total_actors_spawned: 0,
            bounds_min: Vec3::splat(f32::MAX),
            bounds_max: Vec3::splat(f32::MIN),
        }
    }
}

impl LevelSummary {
    /// Returns true if the level has valid geometry and player spawn points.
    #[inline]
    pub fn is_playable(&self) -> bool {
        self.player_starts_count > 0 && (self.bsp_nodes_count > 0 || self.path_nodes_count > 0)
    }

    /// Extends level bounding box with a 3D point.
    pub fn extend_bounds(&mut self, point: Vec3) {
        self.bounds_min = self.bounds_min.min(point);
        self.bounds_max = self.bounds_max.max(point);
    }
}

/// BSP Collision Model component attached to world geometry entities.
#[derive(Debug, Clone, PartialEq)]
pub struct BspCollisionModel {
    /// Name of the model export in the package (e.g. `Model0`, `Model3`).
    pub model_name: String,
    /// Underlying parsed `UModel` containing BSP nodes, surfaces, points, and zones.
    pub umodel: UModel,
    /// World-space bounding box minimum.
    pub bounds_min: Vec3,
    /// World-space bounding box maximum.
    pub bounds_max: Vec3,
    /// Whether this model represents the primary root level BSP geometry.
    pub is_root_level_model: bool,
}

impl BspCollisionModel {
    /// Creates a new BSP collision model component.
    pub fn new(model_name: impl Into<String>, umodel: UModel, is_root: bool) -> Self {
        let (min, max) = Self::compute_bounds(&umodel);
        Self {
            model_name: model_name.into(),
            umodel,
            bounds_min: min,
            bounds_max: max,
            is_root_level_model: is_root,
        }
    }

    /// Computes world-space bounding box from the UModel points and box.
    fn compute_bounds(umodel: &UModel) -> (Vec3, Vec3) {
        if umodel.points.is_empty() {
            let min = Vec3::new(
                umodel.bounding_box.min.x,
                umodel.bounding_box.min.y,
                umodel.bounding_box.min.z,
            );
            let max = Vec3::new(
                umodel.bounding_box.max.x,
                umodel.bounding_box.max.y,
                umodel.bounding_box.max.z,
            );
            return (min, max);
        }

        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for pt in &umodel.points {
            let v = Vec3::new(pt.x, pt.y, pt.z);
            min = min.min(v);
            max = max.max(v);
        }
        (min, max)
    }

    /// Total count of BSP nodes.
    #[inline]
    pub fn node_count(&self) -> usize {
        self.umodel.bsp_nodes.len()
    }

    /// Total count of BSP polygon surfaces.
    #[inline]
    pub fn surf_count(&self) -> usize {
        self.umodel.bsp_surfs.len()
    }

    /// Total count of shared 3D points.
    #[inline]
    pub fn point_count(&self) -> usize {
        self.umodel.points.len()
    }

    /// Checks if a 3D point is within the axis-aligned bounds of the model.
    #[inline]
    pub fn contains_point(&self, p: Vec3) -> bool {
        p.x >= self.bounds_min.x
            && p.x <= self.bounds_max.x
            && p.y >= self.bounds_min.y
            && p.y <= self.bounds_max.y
            && p.z >= self.bounds_min.z
            && p.z <= self.bounds_max.z
    }
}

/// Navigation PathNode component for AI pathfinding and waypoint networks.
#[derive(Debug, Clone, PartialEq)]
pub struct PathNodeComponent {
    /// Object name in the package (e.g. `PathNode25`, `ZombiePathNode3`).
    pub node_name: String,
    /// Maximum reachable distance from this node.
    pub reach_radius: f32,
    /// AI traversal cost factor.
    pub cost: f32,
    /// Indicates whether this node is dedicated to Zed pathfinding.
    pub is_zombie_path_node: bool,
    /// Whether this node is temporarily blocked (e.g. by a welded door).
    pub b_blocked: bool,
    /// Actor tag.
    pub tag: String,
    /// Entities of directly reachable adjacent path nodes.
    pub connected_nodes: Vec<Entity>,
}

impl PathNodeComponent {
    pub fn new(node_name: impl Into<String>, is_zombie: bool) -> Self {
        Self {
            node_name: node_name.into(),
            reach_radius: 50.0,
            cost: 1.0,
            is_zombie_path_node: is_zombie,
            b_blocked: false,
            tag: String::new(),
            connected_nodes: Vec::new(),
        }
    }
}

/// Specimen spawn volume component for wave-based Zed generation.
/// Corresponds to `ZombieVolume`, `KFZombieZoneVolume`, and `KFSpawnArea`.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecimenSpawnPoint {
    /// Name of the volume in the map.
    pub volume_name: String,
    /// Minimum distance to human players required before specimen can spawn here.
    pub min_distance_to_player: f32,
    /// Multiplier applied to number of spawned specimens.
    pub spawn_count_multiplier: f32,
    /// Priority/weight for spawner selection during wave generation.
    pub spawn_desirability: f32,
    /// Allows massive specimens (Scrake, Fleshpound).
    pub b_massive_zeds: bool,
    /// Allows agile/leaping specimens (Crawler).
    pub b_leaping_zeds: bool,
    /// Allows standard specimens (Clot, Gorefast, Bloat, Siren).
    pub b_normal_zeds: bool,
    /// Allows ranged specimens (Husk).
    pub b_ranged_zeds: bool,
    /// Whether this spawn volume is currently active.
    pub b_enabled: bool,
    /// Actor event tag.
    pub tag: String,
}

impl Default for SpecimenSpawnPoint {
    fn default() -> Self {
        Self {
            volume_name: "ZombieVolume".to_string(),
            min_distance_to_player: 400.0,
            spawn_count_multiplier: 1.0,
            spawn_desirability: 1.0,
            b_massive_zeds: true,
            b_leaping_zeds: true,
            b_normal_zeds: true,
            b_ranged_zeds: true,
            b_enabled: true,
            tag: String::new(),
        }
    }
}

/// Trader Zone component representing shop volumes where players purchase weapons and ammo.
/// Corresponds to `ShopVolume`.
#[derive(Debug, Clone, PartialEq)]
pub struct TraderZone {
    /// Name of the shop volume.
    pub zone_name: String,
    /// Shop URL or identifier (e.g. `CloningRoomTrader`, `Shop_WestLondon`).
    pub shop_url: String,
    /// Probability this trader is selected to open between waves (0.0 to 1.0).
    pub enabled_probability: f32,
    /// If true, this trader remains open continuously.
    pub b_always_enabled: bool,
    /// Whether this trader shop is currently open for business.
    pub is_currently_open: bool,
    /// Actor tag.
    pub tag: String,
    /// Door entities associated with this shop volume.
    pub door_entities: Vec<Entity>,
}

impl Default for TraderZone {
    fn default() -> Self {
        Self {
            zone_name: "ShopVolume".to_string(),
            shop_url: "TraderShop".to_string(),
            enabled_probability: 1.0,
            b_always_enabled: false,
            is_currently_open: false,
            tag: String::new(),
            door_entities: Vec::new(),
        }
    }
}

/// Player Start component representing team spawn locations.
/// Corresponds to `PlayerStart`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSpawnPoint {
    /// Name of the spawn actor.
    pub start_name: String,
    /// Team index (0 for humans in standard KF).
    pub team_number: u8,
    /// Whether this start point is enabled.
    pub b_enabled: bool,
    /// Whether this start point can be used in single player.
    pub b_single_player_start: bool,
    /// Actor tag.
    pub tag: String,
}

impl Default for PlayerSpawnPoint {
    fn default() -> Self {
        Self {
            start_name: "PlayerStart".to_string(),
            team_number: 0,
            b_enabled: true,
            b_single_player_start: true,
            tag: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Internal Tagged Property Representation
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Debug)]
struct RawProperty<'a> {
    name: &'a str,
    type_code: u8,
    size: usize,
    struct_name: Option<&'a str>,
    payload: &'a [u8],
    bool_val: bool,
}

impl<'a> RawProperty<'a> {
    #[inline]
    fn as_f32(&self) -> Option<f32> {
        if self.payload.len() >= 4 {
            Some(LittleEndian::read_f32(&self.payload[0..4]))
        } else {
            None
        }
    }

    #[inline]
    fn as_i32(&self) -> Option<i32> {
        if self.payload.len() >= 4 {
            Some(LittleEndian::read_i32(&self.payload[0..4]))
        } else if self.payload.len() >= 2 {
            Some(LittleEndian::read_i16(&self.payload[0..2]) as i32)
        } else if !self.payload.is_empty() {
            Some(self.payload[0] as i32)
        } else {
            None
        }
    }

    #[inline]
    fn as_bool(&self) -> bool {
        self.bool_val
    }

    #[inline]
    fn as_vector(&self) -> Option<Vec3> {
        if self.payload.len() >= 12 {
            let x = LittleEndian::read_f32(&self.payload[0..4]);
            let y = LittleEndian::read_f32(&self.payload[4..8]);
            let z = LittleEndian::read_f32(&self.payload[8..12]);
            Some(Vec3::new(x, y, z))
        } else {
            None
        }
    }

    #[inline]
    fn as_rotator(&self) -> Option<Rotator> {
        if self.payload.len() >= 12 {
            let pitch = LittleEndian::read_i32(&self.payload[0..4]);
            let yaw = LittleEndian::read_i32(&self.payload[4..8]);
            let roll = LittleEndian::read_i32(&self.payload[8..12]);
            Some(Rotator::new(pitch, yaw, roll))
        } else {
            None
        }
    }

    fn as_string(&self) -> String {
        if self.payload.is_empty() {
            return String::new();
        }
        let mut off = 0;
        if let Ok(count) = read_compact_index(self.payload, &mut off) {
            if count > 0 {
                let len = (count as usize).saturating_sub(1);
                let end = off + len;
                if end <= self.payload.len() {
                    return String::from_utf8_lossy(&self.payload[off..end]).to_string();
                }
            } else if count < 0 {
                let len = (-count as usize).saturating_sub(1);
                let byte_len = len * 2;
                let end = off + byte_len;
                if end <= self.payload.len() {
                    let u16_slice: Vec<u16> = self.payload[off..end]
                        .chunks_exact(2)
                        .map(LittleEndian::read_u16)
                        .collect();
                    return String::from_utf16_lossy(&u16_slice);
                }
            }
        }
        String::from_utf8_lossy(self.payload)
            .trim_end_matches('\0')
            .to_string()
    }
}

// ---------------------------------------------------------------------------
// High-Level Level Loader
// ---------------------------------------------------------------------------

/// High-level loader for Unreal Engine 2.5 `.rom` packages and map levels.
pub struct LevelLoader;

impl LevelLoader {
    /// Loads an Unreal Engine 2 `.rom` map file into an ECS `World`.
    ///
    /// Reads the memory-mapped package, parses BSP world models, specimen spawns,
    /// trader zones, path nodes, and player starts, instantiating them into `world`.
    pub fn load_level_from_rom(
        rom_path: impl AsRef<Path>,
        world: &mut World,
    ) -> Result<LevelSummary, LevelLoadError> {
        let path = rom_path.as_ref();
        if !path.exists() {
            return Err(LevelLoadError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("ROM file not found: {}", path.display()),
            )));
        }

        let mmap_pkg = MmapPackage::open(path)?;
        let pkg = mmap_pkg.as_package();
        Self::load_level_from_package(&pkg, world)
    }

    /// Loads a level from raw in-memory `.rom` package bytes.
    pub fn load_level_from_bytes(
        data: &[u8],
        world: &mut World,
    ) -> Result<LevelSummary, LevelLoadError> {
        let pkg = UnrealPackage::from_bytes(data)?;
        Self::load_level_from_package(&pkg, world)
    }

    /// Parses an `UnrealPackage` view and populates the given `World`.
    pub fn load_level_from_package(
        package: &UnrealPackage<'_>,
        world: &mut World,
    ) -> Result<LevelSummary, LevelLoadError> {
        let mut summary = LevelSummary::default();
        let mut path_node_entities: Vec<(Entity, Vec3, f32)> = Vec::new();

        // Pass 1: Parse all exports and spawn corresponding entities
        for export in package.exports.entries.iter() {
            let class_name = Self::resolve_class_name(package, export.class_index);
            let object_name = package
                .resolve_name(export.object_name)
                .unwrap_or("UnknownObject");
            let payload = package.get_export_slice(export)?;

            match class_name.as_str() {
                "LevelSummary" => {
                    Self::parse_level_summary_export(package, payload, export, &mut summary);
                }
                "LevelInfo" => {
                    Self::parse_level_info_export(package, payload, export, &mut summary, world);
                }
                "Model" => {
                    if let Some(bsp_model) = Self::parse_model_export(object_name, payload) {
                        summary.bsp_nodes_count += bsp_model.node_count();
                        summary.bsp_surfs_count += bsp_model.surf_count();
                        summary.bsp_points_count += bsp_model.point_count();
                        summary.extend_bounds(bsp_model.bounds_min);
                        summary.extend_bounds(bsp_model.bounds_max);

                        let mut spatial = SpatialIndex::cylinder(
                            (bsp_model.bounds_max.x - bsp_model.bounds_min.x).max(100.0) * 0.5,
                            (bsp_model.bounds_max.z - bsp_model.bounds_min.z).max(100.0) * 0.5,
                        );
                        let center = (bsp_model.bounds_min + bsp_model.bounds_max) * 0.5;
                        spatial.update_from_position(center);

                        world.ecs_mut().spawn((
                            bsp_model,
                            Transform::from_position(center),
                            ActorState::static_actor(),
                            spatial,
                        ));
                        summary.total_actors_spawned += 1;
                    }
                }
                "PathNode" | "ZombiePathNode" | "RoadPathNode" | "FlyingPathNode" => {
                    let is_zombie = class_name == "ZombiePathNode";
                    let (props, _) = Self::parse_properties(package, payload, export.object_flags);
                    let pos = props
                        .get("Location")
                        .and_then(|p| p.as_vector())
                        .unwrap_or(Vec3::ZERO);
                    let rot = props
                        .get("Rotation")
                        .and_then(|p| p.as_rotator())
                        .unwrap_or_default();
                    let reach = props
                        .get("CollisionRadius")
                        .and_then(|p| p.as_f32())
                        .unwrap_or(50.0);
                    let tag = props
                        .get("Tag")
                        .map(|p| package.resolve_name(p.as_i32().unwrap_or(0)).unwrap_or(""))
                        .unwrap_or("")
                        .to_string();

                    let mut node_comp = PathNodeComponent::new(object_name, is_zombie);
                    node_comp.reach_radius = reach;
                    node_comp.tag = tag;

                    let mut spatial = SpatialIndex::cylinder(reach, 50.0);
                    spatial.update_from_position(pos);

                    let entity = world.ecs_mut().spawn((
                        node_comp,
                        Transform::from_pos_rotator(pos, rot),
                        ActorState::static_actor(),
                        spatial,
                    ));

                    summary.path_nodes_count += 1;
                    summary.total_actors_spawned += 1;
                    summary.extend_bounds(pos);
                    path_node_entities.push((entity, pos, reach));
                }
                "ZombieVolume" | "KFZombieZoneVolume" | "KFSpawnArea" => {
                    let (props, _) = Self::parse_properties(package, payload, export.object_flags);
                    let pos = props
                        .get("Location")
                        .and_then(|p| p.as_vector())
                        .unwrap_or(Vec3::ZERO);
                    let rot = props
                        .get("Rotation")
                        .and_then(|p| p.as_rotator())
                        .unwrap_or_default();

                    let mut sp = SpecimenSpawnPoint {
                        volume_name: object_name.to_string(),
                        ..Default::default()
                    };

                    if let Some(d) = props.get("MinDistanceToPlayer").and_then(|p| p.as_f32()) {
                        sp.min_distance_to_player = d;
                    }
                    if let Some(m) = props.get("ZombieCountMulti").and_then(|p| p.as_f32()) {
                        sp.spawn_count_multiplier = m;
                    }
                    if let Some(des) = props.get("SpawnDesirability").and_then(|p| p.as_f32()) {
                        sp.spawn_desirability = des;
                    }
                    if let Some(b) = props.get("bMassiveZeds").map(|p| p.as_bool()) {
                        sp.b_massive_zeds = b;
                    }
                    if let Some(b) = props.get("bLeapingZeds").map(|p| p.as_bool()) {
                        sp.b_leaping_zeds = b;
                    }
                    if let Some(b) = props.get("bNormalZeds").map(|p| p.as_bool()) {
                        sp.b_normal_zeds = b;
                    }
                    if let Some(b) = props.get("bRangedZeds").map(|p| p.as_bool()) {
                        sp.b_ranged_zeds = b;
                    }
                    if let Some(b) = props.get("bVolumeIsEnabled").map(|p| p.as_bool()) {
                        sp.b_enabled = b;
                    }
                    if let Some(t) = props.get("Tag") {
                        sp.tag = package
                            .resolve_name(t.as_i32().unwrap_or(0))
                            .unwrap_or("")
                            .to_string();
                    }

                    let mut spatial = SpatialIndex::cylinder(200.0, 100.0);
                    spatial.update_from_position(pos);

                    world.ecs_mut().spawn((
                        sp,
                        Transform::from_pos_rotator(pos, rot),
                        ActorState::static_actor(),
                        spatial,
                    ));

                    summary.specimen_spawns_count += 1;
                    summary.total_actors_spawned += 1;
                    summary.extend_bounds(pos);
                }
                "ShopVolume" => {
                    let (props, _) = Self::parse_properties(package, payload, export.object_flags);
                    let pos = props
                        .get("Location")
                        .and_then(|p| p.as_vector())
                        .unwrap_or(Vec3::ZERO);
                    let rot = props
                        .get("Rotation")
                        .and_then(|p| p.as_rotator())
                        .unwrap_or_default();

                    let mut shop = TraderZone {
                        zone_name: object_name.to_string(),
                        ..Default::default()
                    };

                    if let Some(u) = props.get("URL") {
                        let url_str = u.as_string();
                        if !url_str.is_empty() {
                            shop.shop_url = url_str;
                        }
                    }
                    if let Some(prob) = props.get("EnabledProbability").and_then(|p| p.as_f32()) {
                        shop.enabled_probability = prob;
                    }
                    if let Some(always) = props.get("bAlwaysEnabled").map(|p| p.as_bool()) {
                        shop.b_always_enabled = always;
                    }
                    if let Some(t) = props.get("Tag") {
                        shop.tag = package
                            .resolve_name(t.as_i32().unwrap_or(0))
                            .unwrap_or("")
                            .to_string();
                    }

                    let mut spatial = SpatialIndex::cylinder(300.0, 120.0);
                    spatial.update_from_position(pos);

                    world.ecs_mut().spawn((
                        shop,
                        Transform::from_pos_rotator(pos, rot),
                        ActorState::static_actor(),
                        spatial,
                    ));

                    summary.trader_zones_count += 1;
                    summary.total_actors_spawned += 1;
                    summary.extend_bounds(pos);
                }
                "PlayerStart" => {
                    let (props, _) = Self::parse_properties(package, payload, export.object_flags);
                    let pos = props
                        .get("Location")
                        .and_then(|p| p.as_vector())
                        .unwrap_or(Vec3::ZERO);
                    let rot = props
                        .get("Rotation")
                        .and_then(|p| p.as_rotator())
                        .unwrap_or_default();

                    let mut ps = PlayerSpawnPoint {
                        start_name: object_name.to_string(),
                        ..Default::default()
                    };

                    if let Some(team) = props.get("TeamNumber").and_then(|p| p.as_i32()) {
                        ps.team_number = team as u8;
                    }
                    if let Some(b) = props.get("bEnabled").map(|p| p.as_bool()) {
                        ps.b_enabled = b;
                    }
                    if let Some(b) = props.get("bSinglePlayerStart").map(|p| p.as_bool()) {
                        ps.b_single_player_start = b;
                    }
                    if let Some(t) = props.get("Tag") {
                        ps.tag = package
                            .resolve_name(t.as_i32().unwrap_or(0))
                            .unwrap_or("")
                            .to_string();
                    }

                    let mut spatial = SpatialIndex::cylinder(17.0, 48.0);
                    spatial.update_from_position(pos);

                    world.ecs_mut().spawn((
                        ps,
                        Transform::from_pos_rotator(pos, rot),
                        ActorState::static_actor(),
                        spatial,
                    ));

                    summary.player_starts_count += 1;
                    summary.total_actors_spawned += 1;
                    summary.extend_bounds(pos);
                }
                _ => {}
            }
        }

        // Pass 2: Connect path nodes by reachability distance into the navigation network
        Self::connect_path_nodes(world, &path_node_entities, DEFAULT_MAX_PATH_REACH_DISTANCE);

        // Sanitize bounds if no bounds were collected
        if summary.bounds_min.x > summary.bounds_max.x {
            summary.bounds_min = Vec3::ZERO;
            summary.bounds_max = Vec3::ZERO;
        }

        // Synchronize world level info clock and level name
        world.level_info_mut().level_name = summary.title.clone();

        Ok(summary)
    }

    /// Builds a reachability adjacency graph between PathNodes within maximum reach distance.
    pub fn connect_path_nodes(
        world: &mut World,
        nodes: &[(Entity, Vec3, f32)],
        max_dist: f32,
    ) {
        let max_dist_sq = max_dist * max_dist;
        for i in 0..nodes.len() {
            let (entity_a, pos_a, _) = nodes[i];
            let mut neighbors = Vec::new();
            for j in 0..nodes.len() {
                if i == j {
                    continue;
                }
                let (entity_b, pos_b, _) = nodes[j];
                let dist_sq = pos_a.distance_squared(pos_b);
                if dist_sq <= max_dist_sq {
                    neighbors.push(entity_b);
                }
            }
            if let Ok(mut comp) = world.ecs_mut().get::<&mut PathNodeComponent>(entity_a) {
                comp.connected_nodes = neighbors;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Export Parsers & Helpers
    // -----------------------------------------------------------------------

    fn resolve_class_name(package: &UnrealPackage<'_>, class_index: i32) -> String {
        if class_index < 0 {
            let imp_idx = (-class_index - 1) as usize;
            if let Some(import) = package.imports.get(imp_idx) {
                return package
                    .resolve_name(import.object_name)
                    .unwrap_or("UnknownClass")
                    .to_string();
            }
        } else if class_index > 0 {
            let exp_idx = (class_index - 1) as usize;
            if let Some(export) = package.exports.get(exp_idx) {
                return package
                    .resolve_name(export.object_name)
                    .unwrap_or("UnknownClass")
                    .to_string();
            }
        }
        "Class".to_string()
    }

    fn parse_level_summary_export(
        package: &UnrealPackage<'_>,
        payload: &[u8],
        export: &FObjectExport,
        summary: &mut LevelSummary,
    ) {
        let (props, _) = Self::parse_properties(package, payload, export.object_flags);
        if let Some(t) = props.get("Title") {
            let s = t.as_string();
            if !s.is_empty() {
                summary.title = s;
            }
        }
        if let Some(a) = props.get("Author") {
            let s = a.as_string();
            if !s.is_empty() {
                summary.author = s;
            }
        }
        if let Some(d) = props.get("Description") {
            let s = d.as_string();
            if !s.is_empty() {
                summary.description = s;
            }
        }
        if let Some(min_p) = props.get("IdealPlayerCountMin").and_then(|p| p.as_i32()) {
            summary.ideal_player_count_min = min_p;
        }
        if let Some(max_p) = props.get("IdealPlayerCountMax").and_then(|p| p.as_i32()) {
            summary.ideal_player_count_max = max_p;
        }
    }

    fn parse_level_info_export(
        package: &UnrealPackage<'_>,
        payload: &[u8],
        export: &FObjectExport,
        summary: &mut LevelSummary,
        world: &mut World,
    ) {
        let (props, _) = Self::parse_properties(package, payload, export.object_flags);
        if summary.title == "Untitled Level" {
            if let Some(t) = props.get("Title") {
                let s = t.as_string();
                if !s.is_empty() {
                    summary.title = s;
                }
            }
        }
        if summary.author == "Unknown Author" {
            if let Some(a) = props.get("Author") {
                let s = a.as_string();
                if !s.is_empty() {
                    summary.author = s;
                }
            }
        }
        if summary.description.is_empty() {
            if let Some(d) = props.get("Description") {
                let s = d.as_string();
                if !s.is_empty() {
                    summary.description = s;
                }
            }
        }
        if let Some(time) = props.get("TimeSeconds").and_then(|p| p.as_f32()) {
            world.level_info_mut().time_seconds = time as f64;
        }
    }

    fn parse_model_export(name: &str, payload: &[u8]) -> Option<BspCollisionModel> {
        if payload.is_empty() {
            return None;
        }

        let bsp_slice = if payload.len() > 25 && (payload[0] == 0x00 || payload[0] == 0x01) {
            &payload[1..]
        } else {
            payload
        };

        // Attempt 1: Raw fixed uncompressed layout
        if let Ok(umodel) = UModelParser::parse_raw(bsp_slice) {
            let is_root = name.eq_ignore_ascii_case("Model0")
                || name.eq_ignore_ascii_case("Model3")
                || name.eq_ignore_ascii_case("Model")
                || umodel.bsp_nodes.len() > 100;
            return Some(BspCollisionModel::new(name, umodel, is_root));
        }

        // Attempt 2: Unreal package serialized layout with compact index nodes
        if let Some(umodel) = Self::parse_package_umodel(bsp_slice) {
            let is_root = name.eq_ignore_ascii_case("Model0")
                || name.eq_ignore_ascii_case("Model3")
                || name.eq_ignore_ascii_case("Model")
                || umodel.bsp_nodes.len() > 100;
            return Some(BspCollisionModel::new(name, umodel, is_root));
        }

        None
    }

    /// Parses a UModel serialized within an Unreal package using variable-length compact indices.
    fn parse_package_umodel(data: &[u8]) -> Option<UModel> {
        if data.len() < 41 {
            return None;
        }
        let mut offset = 0;

        let bounding_box = openkf_bsp::parser::read_fbox(data, &mut offset).ok()?;
        let bounding_sphere = openkf_bsp::parser::read_fsphere(data, &mut offset).ok()?;

        // Vectors
        let num_vectors = read_compact_index(data, &mut offset).ok()?;
        if num_vectors < 0 {
            return None;
        }
        let mut vectors = Vec::with_capacity(num_vectors as usize);
        for _ in 0..num_vectors {
            vectors.push(openkf_bsp::parser::read_fvector(data, &mut offset).ok()?);
        }

        // Points
        let num_points = read_compact_index(data, &mut offset).ok()?;
        if num_points < 0 {
            return None;
        }
        let mut points = Vec::with_capacity(num_points as usize);
        for _ in 0..num_points {
            points.push(openkf_bsp::parser::read_fvector(data, &mut offset).ok()?);
        }

        // Nodes
        let num_nodes = read_compact_index(data, &mut offset).ok()?;
        if num_nodes < 0 {
            return None;
        }
        let mut bsp_nodes = Vec::with_capacity(num_nodes as usize);
        for _ in 0..num_nodes {
            if offset + 25 > data.len() {
                return None;
            }
            let plane = openkf_bsp::parser::read_fplane(data, &mut offset).ok()?;
            let zone_mask = LittleEndian::read_u64(&data[offset..offset + 8]);
            offset += 8;
            let node_flags = data[offset];
            offset += 1;

            let i_vert_pool = read_compact_index(data, &mut offset).ok()?;
            let i_surf = read_compact_index(data, &mut offset).ok()?;
            let child0 = read_compact_index(data, &mut offset).ok()?;
            let child1 = read_compact_index(data, &mut offset).ok()?;
            let coll_bound = read_compact_index(data, &mut offset).ok()?;
            let zone0 = read_compact_index(data, &mut offset).ok()?;
            let zone1 = read_compact_index(data, &mut offset).ok()?;

            // FSphere (16 bytes)
            if offset + 16 > data.len() {
                return None;
            }
            let _sphere = openkf_bsp::parser::read_fsphere(data, &mut offset).ok()?;

            if offset + 3 > data.len() {
                return None;
            }
            let _z0 = data[offset];
            offset += 1;
            let _z1 = data[offset];
            offset += 1;
            let num_vertices = data[offset];
            offset += 1;

            // leaves (8 bytes) + render data (12 bytes)
            if offset + 20 > data.len() {
                return None;
            }
            offset += 20;

            let node = FBspNode {
                plane,
                zone_mask,
                node_flags,
                i_vert_pool,
                i_surf,
                i_child: [child0, child1],
                i_collision_bound: coll_bound,
                i_zone: [zone0 as u8, zone1 as u8],
                num_vertices,
                ..Default::default()
            };
            bsp_nodes.push(node);
        }

        // Surfs count
        let mut bsp_surfs = Vec::new();
        if let Ok(num_surfs) = read_compact_index(data, &mut offset) {
            if num_surfs > 0 {
                bsp_surfs.resize(num_surfs as usize, FBspSurf::default());
            }
        }

        Some(UModel {
            bounding_box,
            bounding_sphere,
            vectors,
            points,
            bsp_nodes,
            bsp_surfs,
            verts: Vec::new(),
            num_shared_sides: 0,
            num_zones: 0,
            zones: Vec::new(),
            leaves: Vec::new(),
            root_outside: 0,
            linked: 0,
        })
    }

    /// Parses Unreal Engine 2 tagged properties from an export's binary payload.
    fn parse_properties<'a>(
        package: &'a UnrealPackage<'_>,
        payload: &'a [u8],
        object_flags: u32,
    ) -> (HashMap<&'a str, RawProperty<'a>>, usize) {
        let mut props = HashMap::new();
        let mut offset = 0;

        // If RF_HasStack is set, skip FStateFrame header
        if (object_flags & RF_HAS_STACK) != 0 {
            if let Ok(node) = read_compact_index(payload, &mut offset) {
                let _ = read_compact_index(payload, &mut offset); // state_node
                if offset + 12 <= payload.len() {
                    offset += 12; // probe_mask (8) + latent_action (4)
                    if node != 0 {
                        let _ = read_compact_index(payload, &mut offset); // code_offset
                    }
                }
            }
        }

        // Tagged properties loop until None terminator
        while offset < payload.len() {
            let start_off = offset;
            let name_idx = match read_compact_index(payload, &mut offset) {
                Ok(idx) => idx,
                Err(_) => break,
            };

            if name_idx == 0 {
                // 'None' terminator (name table index 0 is always None in Unreal Engine packages)
                break;
            }

            let prop_name = match package.resolve_name(name_idx) {
                Ok(n) => n,
                Err(_) => break,
            };

            if prop_name == "None" {
                break;
            }

            if offset >= payload.len() {
                break;
            }

            let info_byte = payload[offset];
            offset += 1;

            let b_array = (info_byte & 0x80) != 0;
            let size_code = (info_byte >> 4) & 0x07;
            let type_code = info_byte & 0x0F;

            // Non-boolean array properties have an explicit compact index array index
            if b_array && type_code != 3 {
                let _ = read_compact_index(payload, &mut offset);
            }

            let mut struct_name = None;
            if type_code == 10 {
                // StructProperty: struct type name follows as compact index
                if let Ok(st_idx) = read_compact_index(payload, &mut offset) {
                    struct_name = package.resolve_name(st_idx).ok();
                }
            }

            let (data_size, bool_val) = if type_code == 3 {
                // BoolProperty: boolean value is stored directly in bit 7 of info_byte
                (0, b_array)
            } else {
                match size_code {
                    0 => (1, false),
                    1 => (2, false),
                    2 => (4, false),
                    3 => (12, false),
                    4 => (16, false),
                    5 => {
                        if offset >= payload.len() {
                            break;
                        }
                        let sz = payload[offset] as usize;
                        offset += 1;
                        (sz, false)
                    }
                    6 => {
                        if offset + 2 > payload.len() {
                            break;
                        }
                        let sz = LittleEndian::read_u16(&payload[offset..offset + 2]) as usize;
                        offset += 2;
                        (sz, false)
                    }
                    7 => {
                        if offset + 4 > payload.len() {
                            break;
                        }
                        let sz = LittleEndian::read_u32(&payload[offset..offset + 4]) as usize;
                        offset += 4;
                        (sz, false)
                    }
                    _ => (0, false),
                }
            };

            if offset + data_size > payload.len() {
                break;
            }

            let val_bytes = &payload[offset..offset + data_size];
            offset += data_size;

            props.insert(
                prop_name,
                RawProperty {
                    name: prop_name,
                    type_code,
                    size: data_size,
                    struct_name,
                    payload: val_bytes,
                    bool_val,
                },
            );

            // Guard against infinite loop
            if offset <= start_off {
                break;
            }
        }

        (props, offset)
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use openkf_bsp::{FBspNode, FBspSurf, FBox, FVector};
    use openkf_package::{FObjectExport, FObjectImport, PackageWriter, write_compact_index};

    fn make_test_vector_payload(name_idx: i32, vec: Vec3) -> Vec<u8> {
        let mut buf = Vec::new();
        write_compact_index(name_idx, &mut buf);
        // info_byte: type=10 (Struct), size_code=3 (12 bytes), b_array=0 -> 0x3A
        buf.push(0x3A);
        // struct name index: assume index 2 is "Vector"
        write_compact_index(2, &mut buf);
        // 3 floats
        let mut float_buf = [0u8; 12];
        LittleEndian::write_f32(&mut float_buf[0..4], vec.x);
        LittleEndian::write_f32(&mut float_buf[4..8], vec.y);
        LittleEndian::write_f32(&mut float_buf[8..12], vec.z);
        buf.extend_from_slice(&float_buf);
        // None terminator (0x00)
        buf.push(0x00);
        buf
    }

    #[test]
    fn test_synthetic_level_loader_pipeline() {
        let mut writer = PackageWriter::new();

        // Name Table:
        // 0: None
        // 1: Class
        // 2: Vector
        // 3: Rotator
        // 4: Location
        // 5: Rotation
        // 6: Model
        // 7: Model0
        // 8: PathNode
        // 9: PathNode0
        // 10: PathNode1
        // 11: ZombieVolume
        // 12: ZombieVolume0
        // 13: ShopVolume
        // 14: ShopVolume0
        // 15: PlayerStart
        // 16: PlayerStart0
        // 17: LevelSummary
        // 18: LevelSummary0
        // 19: Title
        // 20: Author
        let names = [
            "None", "Class", "Vector", "Rotator", "Location", "Rotation", "Model", "Model0",
            "PathNode", "PathNode0", "PathNode1", "ZombieVolume", "ZombieVolume0", "ShopVolume",
            "ShopVolume0", "PlayerStart", "PlayerStart0", "LevelSummary", "LevelSummary0", "Title",
            "Author",
        ];
        for n in &names {
            writer.add_name(*n, 0);
        }

        // Imports (Classes):
        // Import 0: Model (-1)
        writer.add_import(FObjectImport {
            class_package: 0,
            class_name: 1, // Class
            package: 0,
            object_name: 6, // Model
        });
        // Import 1: PathNode (-2)
        writer.add_import(FObjectImport {
            class_package: 0,
            class_name: 1,
            package: 0,
            object_name: 8, // PathNode
        });
        // Import 2: ZombieVolume (-3)
        writer.add_import(FObjectImport {
            class_package: 0,
            class_name: 1,
            package: 0,
            object_name: 11, // ZombieVolume
        });
        // Import 3: ShopVolume (-4)
        writer.add_import(FObjectImport {
            class_package: 0,
            class_name: 1,
            package: 0,
            object_name: 13, // ShopVolume
        });
        // Import 4: PlayerStart (-5)
        writer.add_import(FObjectImport {
            class_package: 0,
            class_name: 1,
            package: 0,
            object_name: 15, // PlayerStart
        });
        // Import 5: LevelSummary (-6)
        writer.add_import(FObjectImport {
            class_package: 0,
            class_name: 1,
            package: 0,
            object_name: 17, // LevelSummary
        });

        // 1. Build a synthetic BSP Model
        let mut umodel = UModel::new();
        umodel.bounding_box = FBox {
            min: FVector::new(-1000.0, -1000.0, -100.0),
            max: FVector::new(1000.0, 1000.0, 500.0),
            is_valid: 1,
        };
        umodel.points.push(FVector::new(-500.0, -500.0, 0.0));
        umodel.points.push(FVector::new(500.0, -500.0, 0.0));
        umodel.points.push(FVector::new(500.0, 500.0, 0.0));
        umodel.points.push(FVector::new(-500.0, 500.0, 0.0));
        umodel.bsp_nodes.push(FBspNode::default());
        umodel.bsp_surfs.push(FBspSurf::default());

        let raw_bsp = UModelParser::serialize_raw(&umodel);
        let mut model_payload = vec![0x00]; // None property terminator
        model_payload.extend_from_slice(&raw_bsp);

        writer.add_export(
            FObjectExport {
                class_index: -1, // Model
                super_index: 0,
                package: 0,
                object_name: 7, // Model0
                object_flags: 0,
                serial_size: 0,
                serial_offset: 0,
            },
            model_payload,
        );

        // 2. Add PathNode0 at (100.0, 0.0, 20.0)
        let pn0_payload = make_test_vector_payload(4, Vec3::new(100.0, 0.0, 20.0));
        writer.add_export(
            FObjectExport {
                class_index: -2, // PathNode
                super_index: 0,
                package: 0,
                object_name: 9, // PathNode0
                object_flags: 0,
                serial_size: 0,
                serial_offset: 0,
            },
            pn0_payload,
        );

        // 3. Add PathNode1 at (300.0, 0.0, 20.0)
        let pn1_payload = make_test_vector_payload(4, Vec3::new(300.0, 0.0, 20.0));
        writer.add_export(
            FObjectExport {
                class_index: -2, // PathNode
                super_index: 0,
                package: 0,
                object_name: 10, // PathNode1
                object_flags: 0,
                serial_size: 0,
                serial_offset: 0,
            },
            pn1_payload,
        );

        // 4. Add ZombieVolume0 at (-200.0, 400.0, 0.0)
        let zv_payload = make_test_vector_payload(4, Vec3::new(-200.0, 400.0, 0.0));
        writer.add_export(
            FObjectExport {
                class_index: -3, // ZombieVolume
                super_index: 0,
                package: 0,
                object_name: 12, // ZombieVolume0
                object_flags: 0,
                serial_size: 0,
                serial_offset: 0,
            },
            zv_payload,
        );

        // 5. Add ShopVolume0 at (50.0, -300.0, 0.0)
        let sv_payload = make_test_vector_payload(4, Vec3::new(50.0, -300.0, 0.0));
        writer.add_export(
            FObjectExport {
                class_index: -4, // ShopVolume
                super_index: 0,
                package: 0,
                object_name: 14, // ShopVolume0
                object_flags: 0,
                serial_size: 0,
                serial_offset: 0,
            },
            sv_payload,
        );

        // 6. Add PlayerStart0 at (0.0, 0.0, 50.0)
        let ps_payload = make_test_vector_payload(4, Vec3::new(0.0, 0.0, 50.0));
        writer.add_export(
            FObjectExport {
                class_index: -5, // PlayerStart
                super_index: 0,
                package: 0,
                object_name: 16, // PlayerStart0
                object_flags: 0,
                serial_size: 0,
                serial_offset: 0,
            },
            ps_payload,
        );

        // 7. Add LevelSummary
        let mut ls_payload = Vec::new();
        // Title property (type=13 Str, name=19)
        write_compact_index(19, &mut ls_payload);
        let title_bytes = b"KF-SyntheticTest\0";
        // info byte: size_code=5 (1-byte size) + type 13 -> 0x5D
        ls_payload.push(0x5D);
        ls_payload.push((title_bytes.len() + 1) as u8); // +1 for compact index
        write_compact_index(title_bytes.len() as i32, &mut ls_payload);
        ls_payload.extend_from_slice(title_bytes);
        ls_payload.push(0x00); // None terminator

        writer.add_export(
            FObjectExport {
                class_index: -6, // LevelSummary
                super_index: 0,
                package: 0,
                object_name: 18, // LevelSummary0
                object_flags: 0,
                serial_size: 0,
                serial_offset: 0,
            },
            ls_payload,
        );

        // Build binary package
        let package_bytes = writer.build();

        // Test LevelLoader
        let mut world = World::new();
        let summary = LevelLoader::load_level_from_bytes(&package_bytes, &mut world)
            .expect("load_level_from_bytes should succeed");

        // Verify summary
        assert_eq!(summary.title, "KF-SyntheticTest");
        assert_eq!(summary.bsp_nodes_count, 1);
        assert_eq!(summary.bsp_surfs_count, 1);
        assert_eq!(summary.bsp_points_count, 4);
        assert_eq!(summary.path_nodes_count, 2);
        assert_eq!(summary.specimen_spawns_count, 1);
        assert_eq!(summary.trader_zones_count, 1);
        assert_eq!(summary.player_starts_count, 1);
        assert_eq!(summary.total_actors_spawned, 6);
        assert!(summary.is_playable());

        // Verify ECS entities and components
        let mut bsp_found = 0;
        for (_, (bsp, _transform, state)) in world
            .ecs()
            .query::<(&BspCollisionModel, &Transform, &ActorState)>()
            .iter()
        {
            assert_eq!(bsp.model_name, "Model0");
            assert!(bsp.is_root_level_model);
            assert!(state.b_static);
            bsp_found += 1;
        }
        assert_eq!(bsp_found, 1);

        let mut path_nodes = Vec::new();
        for (entity, (pn, transform)) in world
            .ecs()
            .query::<(&PathNodeComponent, &Transform)>()
            .iter()
        {
            path_nodes.push((entity, pn.clone(), transform.position));
        }
        assert_eq!(path_nodes.len(), 2);
        // Verify path nodes are connected to each other (distance 200.0 <= 1200.0)
        assert_eq!(path_nodes[0].1.connected_nodes.len(), 1);
        assert_eq!(path_nodes[1].1.connected_nodes.len(), 1);
        assert_eq!(path_nodes[0].1.connected_nodes[0], path_nodes[1].0);
        assert_eq!(path_nodes[1].1.connected_nodes[0], path_nodes[0].0);

        let mut specimen_spawns = 0;
        for (_, (spawn, transform)) in world
            .ecs()
            .query::<(&SpecimenSpawnPoint, &Transform)>()
            .iter()
        {
            assert_eq!(spawn.volume_name, "ZombieVolume0");
            assert_eq!(transform.position, Vec3::new(-200.0, 400.0, 0.0));
            assert!(spawn.b_enabled);
            specimen_spawns += 1;
        }
        assert_eq!(specimen_spawns, 1);

        let mut trader_zones = 0;
        for (_, (trader, transform)) in world
            .ecs()
            .query::<(&TraderZone, &Transform)>()
            .iter()
        {
            assert_eq!(trader.zone_name, "ShopVolume0");
            assert_eq!(transform.position, Vec3::new(50.0, -300.0, 0.0));
            trader_zones += 1;
        }
        assert_eq!(trader_zones, 1);

        let mut player_starts = 0;
        for (_, (ps, transform)) in world
            .ecs()
            .query::<(&PlayerSpawnPoint, &Transform)>()
            .iter()
        {
            assert_eq!(ps.start_name, "PlayerStart0");
            assert_eq!(transform.position, Vec3::new(0.0, 0.0, 50.0));
            assert_eq!(ps.team_number, 0);
            player_starts += 1;
        }
        assert_eq!(player_starts, 1);

        // Verify World LevelInfo synchronization
        assert_eq!(world.level_info().level_name, "KF-SyntheticTest");
    }

    #[test]
    fn test_retail_entry_rom_loading() {
        let entry_rom_path = r"E:\SteamLibrary\steamapps\common\KillingFloor\Maps\Entry.rom";
        if !std::path::Path::new(entry_rom_path).exists() {
            eprintln!("Skipping retail Entry.rom test: file not found on disk");
            return;
        }

        let mut world = World::new();
        let summary = LevelLoader::load_level_from_rom(entry_rom_path, &mut world)
            .expect("Entry.rom load should succeed");

        assert_eq!(summary.player_starts_count, 1);
        assert!(summary.bsp_nodes_count > 0);
        assert!(summary.total_actors_spawned >= 2);
    }

    #[test]
    fn test_retail_bioticslab_rom_loading() {
        let biotics_path = r"E:\SteamLibrary\steamapps\common\KillingFloor\Maps\KF-BioticsLab.rom";
        if !std::path::Path::new(biotics_path).exists() {
            eprintln!("Skipping retail KF-BioticsLab.rom test: file not found on disk");
            return;
        }

        let mut world = World::new();
        let summary = LevelLoader::load_level_from_rom(biotics_path, &mut world)
            .expect("KF-BioticsLab.rom load should succeed");

        assert!(summary.path_nodes_count >= 150);
        assert!(summary.specimen_spawns_count >= 30);
        assert!(summary.trader_zones_count >= 1);
        assert!(summary.player_starts_count >= 4);
        assert!(summary.is_playable());
    }

    #[test]
    fn test_level_load_error_file_not_found() {
        let mut world = World::new();
        let res = LevelLoader::load_level_from_rom(
            r"E:\Definitely\NonExistent\Path\Map.rom",
            &mut world,
        );
        assert!(res.is_err());
        match res.err().unwrap() {
            LevelLoadError::Io(err) => {
                assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
            }
            other => panic!("Expected IO error, got {:?}", other),
        }
    }

    #[test]
    fn test_level_load_error_invalid_package_data() {
        let mut world = World::new();
        let corrupt_bytes = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01, 0x02, 0x03];
        let res = LevelLoader::load_level_from_bytes(&corrupt_bytes, &mut world);
        assert!(res.is_err());
        match res.err().unwrap() {
            LevelLoadError::Package(_) => {}
            other => panic!("Expected Package error, got {:?}", other),
        }
    }

    #[test]
    fn test_bsp_collision_model_bounds_and_contains_point() {
        let mut umodel = UModel::new();
        umodel.points.push(FVector::new(-100.0, -100.0, 0.0));
        umodel.points.push(FVector::new(100.0, 100.0, 200.0));
        let bsp = BspCollisionModel::new("TestModel", umodel, false);

        assert_eq!(bsp.node_count(), 0);
        assert_eq!(bsp.surf_count(), 0);
        assert_eq!(bsp.point_count(), 2);
        assert!(bsp.contains_point(Vec3::new(0.0, 0.0, 100.0)));
        assert!(!bsp.contains_point(Vec3::new(500.0, 0.0, 100.0)));
    }

    #[test]
    fn test_path_node_distance_isolation() {
        let mut world = World::new();
        let e1 = world.ecs_mut().spawn((
            PathNodeComponent::new("Node1", false),
            Transform::from_position(Vec3::new(0.0, 0.0, 0.0)),
        ));
        let e2 = world.ecs_mut().spawn((
            PathNodeComponent::new("Node2", false),
            Transform::from_position(Vec3::new(2000.0, 0.0, 0.0)),
        ));

        let nodes = vec![
            (e1, Vec3::new(0.0, 0.0, 0.0), 50.0),
            (e2, Vec3::new(2000.0, 0.0, 0.0), 50.0),
        ];
        LevelLoader::connect_path_nodes(&mut world, &nodes, 1000.0);

        let n1 = world.ecs().get::<&PathNodeComponent>(e1).unwrap();
        assert!(n1.connected_nodes.is_empty());
        let n2 = world.ecs().get::<&PathNodeComponent>(e2).unwrap();
        assert!(n2.connected_nodes.is_empty());
    }
}
