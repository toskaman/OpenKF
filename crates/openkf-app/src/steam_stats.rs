//! Steam Stats, Achievements, and Cloud Storage Subsystem for Killing Floor
//!
//! Provides full integration with Valve's `ISteamUserStats` and Steam Cloud Storage
//! interfaces for Killing Floor (AppID 1250). Supports reading and writing perk progression,
//! kill counters, difficulty wins, achievement unlocks, and remote cloud profile synchronization.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Retail Killing Floor Steam Application ID.
pub const KILLING_FLOOR_APP_ID: u32 = 1250;

/// Errors that can occur during Steam Stats and Cloud operations.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SteamStatsError {
    #[error("Steam API is not initialized or user is not logged in")]
    NotInitialized,

    #[error("Current stats have not been requested yet; call request_current_stats() first")]
    StatsNotReady,

    #[error("Stat '{0}' was not found in the Steam schema")]
    StatNotFound(String),

    #[error("Achievement '{0}' was not found in the Steam schema")]
    AchievementNotFound(String),

    #[error("Stat '{0}' type mismatch (expected int, found float or vice versa)")]
    TypeMismatch(String),

    #[error("Failed to commit stats to Steam server")]
    StoreStatsFailed,

    #[error("Cloud storage error: {0}")]
    CloudStorageError(String),

    #[error("Profile serialization/deserialization failed: {0}")]
    SerializationError(String),
}

/// Killing Floor standard 7 perk classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PerkType {
    FieldMedic,
    SupportSpecialist,
    Sharpshooter,
    Commando,
    Berserker,
    Firebug,
    Demolitions,
}

impl PerkType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::FieldMedic => "Field Medic",
            Self::SupportSpecialist => "Support Specialist",
            Self::Sharpshooter => "Sharpshooter",
            Self::Commando => "Commando",
            Self::Berserker => "Berserker",
            Self::Firebug => "Firebug",
            Self::Demolitions => "Demolitions",
        }
    }
}

/// Killing Floor specimen types for kill tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpecimenType {
    Clot,
    Crawler,
    Gorefast,
    Stalker,
    Bloat,
    Siren,
    Husk,
    Scrake,
    Fleshpound,
    Patriarch,
}

/// Weapon damage / kill classification for perk progression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WeaponCategory {
    MedicSyringeOrDart,
    Shotgun,
    Welder,
    SharpshooterPistolOrRifle,
    AssaultRifle,
    Melee,
    Flame,
    Explosive,
}

/// Detailed progression report for a specific perk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerkProgress {
    pub perk: PerkType,
    pub current_level: u32,
    pub primary_current: i32,
    pub primary_required: i32,
    pub secondary_current: Option<i32>,
    pub secondary_required: Option<i32>,
    pub is_max_level: bool,
}

/// Known Killing Floor internal Steam stat keys.
pub mod stat_keys {
    // Field Medic
    pub const DAMAGE_HEALED: &str = "DamageHealed";

    // Support Specialist
    pub const SHOTGUN_DAMAGE: &str = "ShotgunDamage";
    pub const WELDING_POINTS: &str = "WeldingPoints";

    // Sharpshooter
    pub const HEADSHOT_KILLS: &str = "HeadshotKills";

    // Commando
    pub const BULLPUP_DAMAGE: &str = "BullpupDamage";
    pub const STALKER_KILLS: &str = "StalkerKills";

    // Berserker
    pub const MELEE_DAMAGE: &str = "MeleeDamage";

    // Firebug
    pub const FLAMETHROWER_DAMAGE: &str = "FlameThrowerDamage";

    // Demolitions
    pub const EXPLOSIVES_DAMAGE: &str = "ExplosivesDamage";

    // Overall / General
    pub const TOTAL_KILLS: &str = "TotalKills";
    pub const CASH_DONATED: &str = "CashDonated";
    pub const GAMES_WON: &str = "GamesWon";
    pub const DEATHS: &str = "Deaths";
}

/// Known Killing Floor internal Steam achievement keys.
pub mod achievement_keys {
    pub const EXPERIMENTILOTTACIDE: &str = "KillXEnemies3";
    pub const PATRIARCH_WITH_LAW: &str = "KillPatriarchWithLAW";
    pub const FLAMING_HELL: &str = "KillHuskWithFlamethrower";
    pub const WIN_BIOTICS_LAB_NORMAL: &str = "WinBioticsLabNormal";
    pub const WIN_FARM_NORMAL: &str = "WinFarmNormal";
    pub const WIN_MANOR_NORMAL: &str = "WinManorNormal";
    pub const GIBLET_MAKER: &str = "TurnXEnemiesIntoGiblets";
    pub const SNEAKERS_OFF_FLOOR: &str = "KillXStalkersWithExplosives";
}

/// State representation of an individual Steam achievement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AchievementState {
    pub api_name: String,
    pub unlocked: bool,
    pub unlock_time: u64,
}

/// Value of a Steam Stat (Integer or Float).
#[derive(Debug, Clone, PartialEq)]
pub enum StatValue {
    Int(i32),
    Float(f32),
}

/// In-memory storage and synchronization engine for Steam User Stats.
///
/// Implements full semantics of `ISteamUserStats`, including dirty tracking,
/// schema validation, local caching, and commitment to Steam Cloud.
pub struct SteamUserStats {
    app_id: u32,
    stats_requested: bool,
    is_dirty: bool,
    stats: HashMap<String, StatValue>,
    achievements: HashMap<String, AchievementState>,
}

impl Default for SteamUserStats {
    fn default() -> Self {
        Self::new(KILLING_FLOOR_APP_ID)
    }
}

impl SteamUserStats {
    /// Creates a new `SteamUserStats` instance configured with the specified AppID.
    pub fn new(app_id: u32) -> Self {
        let mut inst = Self {
            app_id,
            stats_requested: false,
            is_dirty: false,
            stats: HashMap::new(),
            achievements: HashMap::new(),
        };
        inst.init_default_kf_schema();
        inst
    }

    /// Initializes default stat names and known achievements for Killing Floor.
    fn init_default_kf_schema(&mut self) {
        use stat_keys::*;
        let default_int_stats = [
            DAMAGE_HEALED,
            SHOTGUN_DAMAGE,
            WELDING_POINTS,
            HEADSHOT_KILLS,
            BULLPUP_DAMAGE,
            STALKER_KILLS,
            MELEE_DAMAGE,
            FLAMETHROWER_DAMAGE,
            EXPLOSIVES_DAMAGE,
            TOTAL_KILLS,
            CASH_DONATED,
            GAMES_WON,
            DEATHS,
        ];

        for key in default_int_stats {
            self.stats.insert(key.to_string(), StatValue::Int(0));
        }

        use achievement_keys::*;
        let default_achievements = [
            EXPERIMENTILOTTACIDE,
            PATRIARCH_WITH_LAW,
            FLAMING_HELL,
            WIN_BIOTICS_LAB_NORMAL,
            WIN_FARM_NORMAL,
            WIN_MANOR_NORMAL,
            GIBLET_MAKER,
            SNEAKERS_OFF_FLOOR,
        ];

        for key in default_achievements {
            self.achievements.insert(
                key.to_string(),
                AchievementState {
                    api_name: key.to_string(),
                    unlocked: false,
                    unlock_time: 0,
                },
            );
        }
    }

    /// AppID associated with this session.
    pub fn app_id(&self) -> u32 {
        self.app_id
    }

    /// Whether local stats have uncommitted changes.
    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    /// Whether stats have been loaded from Steam / Cloud.
    pub fn is_ready(&self) -> bool {
        self.stats_requested
    }

    /// Simulates calling `SteamUserStats()->RequestCurrentStats()`.
    /// Transitions the state to ready so stats can be accessed.
    pub fn request_current_stats(&mut self) -> Result<(), SteamStatsError> {
        self.stats_requested = true;
        Ok(())
    }

    /// Retrieves an integer statistic value (e.g., `DamageHealed`, `HeadshotKills`).
    pub fn get_stat_int(&self, name: &str) -> Result<i32, SteamStatsError> {
        if !self.stats_requested {
            return Err(SteamStatsError::StatsNotReady);
        }

        match self.stats.get(name) {
            Some(StatValue::Int(val)) => Ok(*val),
            Some(StatValue::Float(_)) => Err(SteamStatsError::TypeMismatch(name.to_string())),
            None => Err(SteamStatsError::StatNotFound(name.to_string())),
        }
    }

    /// Sets an integer statistic value in local memory.
    pub fn set_stat_int(&mut self, name: &str, val: i32) -> Result<(), SteamStatsError> {
        if !self.stats_requested {
            return Err(SteamStatsError::StatsNotReady);
        }

        match self.stats.get_mut(name) {
            Some(entry @ StatValue::Int(_)) => {
                *entry = StatValue::Int(val);
                self.is_dirty = true;
                Ok(())
            }
            Some(StatValue::Float(_)) => Err(SteamStatsError::TypeMismatch(name.to_string())),
            None => {
                // If it's a new custom stat key, insert it dynamically
                self.stats.insert(name.to_string(), StatValue::Int(val));
                self.is_dirty = true;
                Ok(())
            }
        }
    }

    /// Adds delta to an integer statistic value and returns the new value.
    pub fn add_stat_int(&mut self, name: &str, delta: i32) -> Result<i32, SteamStatsError> {
        let current = self.get_stat_int(name).unwrap_or(0);
        let next = current.saturating_add(delta);
        self.set_stat_int(name, next)?;
        Ok(next)
    }

    /// Retrieves a floating-point statistic value.
    pub fn get_stat_float(&self, name: &str) -> Result<f32, SteamStatsError> {
        if !self.stats_requested {
            return Err(SteamStatsError::StatsNotReady);
        }

        match self.stats.get(name) {
            Some(StatValue::Float(val)) => Ok(*val),
            Some(StatValue::Int(_)) => Err(SteamStatsError::TypeMismatch(name.to_string())),
            None => Err(SteamStatsError::StatNotFound(name.to_string())),
        }
    }

    /// Sets a floating-point statistic value in local memory.
    pub fn set_stat_float(&mut self, name: &str, val: f32) -> Result<(), SteamStatsError> {
        if !self.stats_requested {
            return Err(SteamStatsError::StatsNotReady);
        }

        match self.stats.get_mut(name) {
            Some(entry @ StatValue::Float(_)) => {
                *entry = StatValue::Float(val);
                self.is_dirty = true;
                Ok(())
            }
            Some(StatValue::Int(_)) => Err(SteamStatsError::TypeMismatch(name.to_string())),
            None => {
                self.stats.insert(name.to_string(), StatValue::Float(val));
                self.is_dirty = true;
                Ok(())
            }
        }
    }

    /// Checks if a specific achievement is unlocked.
    pub fn get_achievement(&self, name: &str) -> Result<bool, SteamStatsError> {
        if !self.stats_requested {
            return Err(SteamStatsError::StatsNotReady);
        }

        match self.achievements.get(name) {
            Some(ach) => Ok(ach.unlocked),
            None => Err(SteamStatsError::AchievementNotFound(name.to_string())),
        }
    }

    /// Unlocks an achievement locally. Returns `true` if it was newly unlocked.
    pub fn set_achievement(&mut self, name: &str) -> Result<bool, SteamStatsError> {
        if !self.stats_requested {
            return Err(SteamStatsError::StatsNotReady);
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        match self.achievements.get_mut(name) {
            Some(ach) => {
                if !ach.unlocked {
                    ach.unlocked = true;
                    ach.unlock_time = now;
                    self.is_dirty = true;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            None => {
                // Register dynamically and unlock
                self.achievements.insert(
                    name.to_string(),
                    AchievementState {
                        api_name: name.to_string(),
                        unlocked: true,
                        unlock_time: now,
                    },
                );
                self.is_dirty = true;
                Ok(true)
            }
        }
    }

    /// Clears an achievement locally.
    pub fn clear_achievement(&mut self, name: &str) -> Result<bool, SteamStatsError> {
        if !self.stats_requested {
            return Err(SteamStatsError::StatsNotReady);
        }

        match self.achievements.get_mut(name) {
            Some(ach) => {
                if ach.unlocked {
                    ach.unlocked = false;
                    ach.unlock_time = 0;
                    self.is_dirty = true;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            None => Err(SteamStatsError::AchievementNotFound(name.to_string())),
        }
    }

    /// Commits all pending stat and achievement modifications to Steam / Cloud.
    pub fn store_stats(&mut self) -> Result<bool, SteamStatsError> {
        if !self.stats_requested {
            return Err(SteamStatsError::StatsNotReady);
        }

        let was_dirty = self.is_dirty;
        self.is_dirty = false;
        Ok(was_dirty)
    }

    /// Resets all statistics and optionally achievements to default zero states.
    pub fn reset_all_stats(&mut self, achievements_too: bool) -> Result<(), SteamStatsError> {
        if !self.stats_requested {
            return Err(SteamStatsError::StatsNotReady);
        }

        for val in self.stats.values_mut() {
            match val {
                StatValue::Int(ref mut v) => *v = 0,
                StatValue::Float(ref mut v) => *v = 0.0,
            }
        }

        if achievements_too {
            for ach in self.achievements.values_mut() {
                ach.unlocked = false;
                ach.unlock_time = 0;
            }
        }

        self.is_dirty = true;
        Ok(())
    }

    // =========================================================================
    // KILLING FLOOR PERK ENGINE & GAMEPLAY HOOKS
    // =========================================================================

    /// Calculates current perk level (0 through 6) based on Killing Floor v1065 thresholds.
    pub fn calculate_perk_level(&self, perk: PerkType) -> u32 {
        if !self.stats_requested {
            return 0;
        }

        match perk {
            PerkType::FieldMedic => {
                let healed = self.get_stat_int(stat_keys::DAMAGE_HEALED).unwrap_or(0);
                if healed >= 25_000 {
                    6
                } else if healed >= 12_000 {
                    5
                } else if healed >= 4_000 {
                    4
                } else if healed >= 2_000 {
                    3
                } else if healed >= 750 {
                    2
                } else if healed >= 200 {
                    1
                } else {
                    0
                }
            }
            PerkType::SupportSpecialist => {
                let dmg = self.get_stat_int(stat_keys::SHOTGUN_DAMAGE).unwrap_or(0);
                let weld = self.get_stat_int(stat_keys::WELDING_POINTS).unwrap_or(0);

                let lvl_dmg = if dmg >= 5_500_000 {
                    6
                } else if dmg >= 3_500_000 {
                    5
                } else if dmg >= 1_500_000 {
                    4
                } else if dmg >= 500_000 {
                    3
                } else if dmg >= 100_000 {
                    2
                } else if dmg >= 25_000 {
                    1
                } else {
                    0
                };

                let lvl_weld = if weld >= 25_000 {
                    6
                } else if weld >= 8_000 {
                    5
                } else if weld >= 2_000 {
                    4
                } else if weld >= 750 {
                    3
                } else if weld >= 250 {
                    2
                } else if weld >= 100 {
                    1
                } else {
                    0
                };

                lvl_dmg.min(lvl_weld)
            }
            PerkType::Sharpshooter => {
                let hs = self.get_stat_int(stat_keys::HEADSHOT_KILLS).unwrap_or(0);
                if hs >= 8_500 {
                    6
                } else if hs >= 5_500 {
                    5
                } else if hs >= 2_500 {
                    4
                } else if hs >= 700 {
                    3
                } else if hs >= 100 {
                    2
                } else if hs >= 30 {
                    1
                } else {
                    0
                }
            }
            PerkType::Commando => {
                let dmg = self.get_stat_int(stat_keys::BULLPUP_DAMAGE).unwrap_or(0);
                let stalkers = self.get_stat_int(stat_keys::STALKER_KILLS).unwrap_or(0);

                let lvl_dmg = if dmg >= 5_500_000 {
                    6
                } else if dmg >= 3_500_000 {
                    5
                } else if dmg >= 1_500_000 {
                    4
                } else if dmg >= 500_000 {
                    3
                } else if dmg >= 100_000 {
                    2
                } else if dmg >= 25_000 {
                    1
                } else {
                    0
                };

                let lvl_stalkers = if stalkers >= 4_000 {
                    6
                } else if stalkers >= 2_400 {
                    5
                } else if stalkers >= 1_200 {
                    4
                } else if stalkers >= 350 {
                    3
                } else if stalkers >= 100 {
                    2
                } else if stalkers >= 30 {
                    1
                } else {
                    0
                };

                lvl_dmg.min(lvl_stalkers)
            }
            PerkType::Berserker => {
                let dmg = self.get_stat_int(stat_keys::MELEE_DAMAGE).unwrap_or(0);
                if dmg >= 5_500_000 {
                    6
                } else if dmg >= 3_500_000 {
                    5
                } else if dmg >= 1_500_000 {
                    4
                } else if dmg >= 500_000 {
                    3
                } else if dmg >= 100_000 {
                    2
                } else if dmg >= 25_000 {
                    1
                } else {
                    0
                }
            }
            PerkType::Firebug => {
                let dmg = self.get_stat_int(stat_keys::FLAMETHROWER_DAMAGE).unwrap_or(0);
                if dmg >= 5_500_000 {
                    6
                } else if dmg >= 3_500_000 {
                    5
                } else if dmg >= 1_500_000 {
                    4
                } else if dmg >= 500_000 {
                    3
                } else if dmg >= 100_000 {
                    2
                } else if dmg >= 25_000 {
                    1
                } else {
                    0
                }
            }
            PerkType::Demolitions => {
                let dmg = self.get_stat_int(stat_keys::EXPLOSIVES_DAMAGE).unwrap_or(0);
                if dmg >= 5_500_000 {
                    6
                } else if dmg >= 3_500_000 {
                    5
                } else if dmg >= 1_500_000 {
                    4
                } else if dmg >= 500_000 {
                    3
                } else if dmg >= 100_000 {
                    2
                } else if dmg >= 25_000 {
                    1
                } else {
                    0
                }
            }
        }
    }

    /// Obtains detailed progression info including target requirements for next level.
    pub fn get_perk_progress(&self, perk: PerkType) -> PerkProgress {
        let level = self.calculate_perk_level(perk);
        let is_max = level >= 6;

        match perk {
            PerkType::FieldMedic => {
                let reqs = [200, 750, 2000, 4000, 12000, 25000];
                let current = self.get_stat_int(stat_keys::DAMAGE_HEALED).unwrap_or(0);
                let target = if is_max {
                    reqs[5]
                } else {
                    reqs[level as usize]
                };

                PerkProgress {
                    perk,
                    current_level: level,
                    primary_current: current,
                    primary_required: target,
                    secondary_current: None,
                    secondary_required: None,
                    is_max_level: is_max,
                }
            }
            PerkType::SupportSpecialist => {
                let dmg_reqs = [25_000, 100_000, 500_000, 1_500_000, 3_500_000, 5_500_000];
                let weld_reqs = [100, 250, 750, 2000, 8000, 25000];
                let cur_dmg = self.get_stat_int(stat_keys::SHOTGUN_DAMAGE).unwrap_or(0);
                let cur_weld = self.get_stat_int(stat_keys::WELDING_POINTS).unwrap_or(0);

                let target_dmg = if is_max {
                    dmg_reqs[5]
                } else {
                    dmg_reqs[level as usize]
                };
                let target_weld = if is_max {
                    weld_reqs[5]
                } else {
                    weld_reqs[level as usize]
                };

                PerkProgress {
                    perk,
                    current_level: level,
                    primary_current: cur_dmg,
                    primary_required: target_dmg,
                    secondary_current: Some(cur_weld),
                    secondary_required: Some(target_weld),
                    is_max_level: is_max,
                }
            }
            PerkType::Sharpshooter => {
                let reqs = [30, 100, 700, 2500, 5500, 8500];
                let cur = self.get_stat_int(stat_keys::HEADSHOT_KILLS).unwrap_or(0);
                let target = if is_max {
                    reqs[5]
                } else {
                    reqs[level as usize]
                };

                PerkProgress {
                    perk,
                    current_level: level,
                    primary_current: cur,
                    primary_required: target,
                    secondary_current: None,
                    secondary_required: None,
                    is_max_level: is_max,
                }
            }
            PerkType::Commando => {
                let dmg_reqs = [25_000, 100_000, 500_000, 1_500_000, 3_500_000, 5_500_000];
                let stalker_reqs = [30, 100, 350, 1200, 2400, 4000];
                let cur_dmg = self.get_stat_int(stat_keys::BULLPUP_DAMAGE).unwrap_or(0);
                let cur_stalkers = self.get_stat_int(stat_keys::STALKER_KILLS).unwrap_or(0);

                let target_dmg = if is_max {
                    dmg_reqs[5]
                } else {
                    dmg_reqs[level as usize]
                };
                let target_stalker = if is_max {
                    stalker_reqs[5]
                } else {
                    stalker_reqs[level as usize]
                };

                PerkProgress {
                    perk,
                    current_level: level,
                    primary_current: cur_dmg,
                    primary_required: target_dmg,
                    secondary_current: Some(cur_stalkers),
                    secondary_required: Some(target_stalker),
                    is_max_level: is_max,
                }
            }
            PerkType::Berserker => {
                let reqs = [25_000, 100_000, 500_000, 1_500_000, 3_500_000, 5_500_000];
                let cur = self.get_stat_int(stat_keys::MELEE_DAMAGE).unwrap_or(0);
                let target = if is_max {
                    reqs[5]
                } else {
                    reqs[level as usize]
                };

                PerkProgress {
                    perk,
                    current_level: level,
                    primary_current: cur,
                    primary_required: target,
                    secondary_current: None,
                    secondary_required: None,
                    is_max_level: is_max,
                }
            }
            PerkType::Firebug => {
                let reqs = [25_000, 100_000, 500_000, 1_500_000, 3_500_000, 5_500_000];
                let cur = self.get_stat_int(stat_keys::FLAMETHROWER_DAMAGE).unwrap_or(0);
                let target = if is_max {
                    reqs[5]
                } else {
                    reqs[level as usize]
                };

                PerkProgress {
                    perk,
                    current_level: level,
                    primary_current: cur,
                    primary_required: target,
                    secondary_current: None,
                    secondary_required: None,
                    is_max_level: is_max,
                }
            }
            PerkType::Demolitions => {
                let reqs = [25_000, 100_000, 500_000, 1_500_000, 3_500_000, 5_500_000];
                let cur = self.get_stat_int(stat_keys::EXPLOSIVES_DAMAGE).unwrap_or(0);
                let target = if is_max {
                    reqs[5]
                } else {
                    reqs[level as usize]
                };

                PerkProgress {
                    perk,
                    current_level: level,
                    primary_current: cur,
                    primary_required: target,
                    secondary_current: None,
                    secondary_required: None,
                    is_max_level: is_max,
                }
            }
        }
    }

    /// Records an in-game kill event, updating total kills, perk counters, and achievements.
    pub fn record_kill(
        &mut self,
        victim: SpecimenType,
        weapon: WeaponCategory,
        headshot: bool,
    ) -> Result<(), SteamStatsError> {
        let total = self.add_stat_int(stat_keys::TOTAL_KILLS, 1)?;

        // Achievement check: 10,000 specimens killed
        if total >= 10_000 {
            self.set_achievement(achievement_keys::EXPERIMENTILOTTACIDE)?;
        }

        // Sharpshooter headshot tracking
        if headshot && weapon == WeaponCategory::SharpshooterPistolOrRifle {
            self.add_stat_int(stat_keys::HEADSHOT_KILLS, 1)?;
        }

        // Commando stalker tracking
        if victim == SpecimenType::Stalker {
            if weapon == WeaponCategory::AssaultRifle {
                self.add_stat_int(stat_keys::STALKER_KILLS, 1)?;
            } else if weapon == WeaponCategory::Explosive {
                // Secret achievement: kill stalkers with explosives
                self.set_achievement(achievement_keys::SNEAKERS_OFF_FLOOR)?;
            }
        }

        // Special specimen kill achievements
        if victim == SpecimenType::Patriarch && weapon == WeaponCategory::Explosive {
            self.set_achievement(achievement_keys::PATRIARCH_WITH_LAW)?;
        } else if victim == SpecimenType::Husk && weapon == WeaponCategory::Flame {
            self.set_achievement(achievement_keys::FLAMING_HELL)?;
        }

        Ok(())
    }

    /// Records weapon damage dealt to specimens, updating the corresponding perk stat.
    pub fn record_damage(
        &mut self,
        category: WeaponCategory,
        damage: i32,
    ) -> Result<i32, SteamStatsError> {
        if damage <= 0 {
            return Ok(0);
        }

        match category {
            WeaponCategory::Shotgun => self.add_stat_int(stat_keys::SHOTGUN_DAMAGE, damage),
            WeaponCategory::AssaultRifle => self.add_stat_int(stat_keys::BULLPUP_DAMAGE, damage),
            WeaponCategory::Melee => self.add_stat_int(stat_keys::MELEE_DAMAGE, damage),
            WeaponCategory::Flame => self.add_stat_int(stat_keys::FLAMETHROWER_DAMAGE, damage),
            WeaponCategory::Explosive => self.add_stat_int(stat_keys::EXPLOSIVES_DAMAGE, damage),
            _ => Ok(0),
        }
    }

    /// Records teammate healing, updating the Field Medic perk stat.
    pub fn record_heal(&mut self, amount: i32) -> Result<i32, SteamStatsError> {
        if amount <= 0 {
            return Ok(0);
        }
        self.add_stat_int(stat_keys::DAMAGE_HEALED, amount)
    }

    /// Records door welding points, updating the Support Specialist perk stat.
    pub fn record_weld(&mut self, points: i32) -> Result<i32, SteamStatsError> {
        if points <= 0 {
            return Ok(0);
        }
        self.add_stat_int(stat_keys::WELDING_POINTS, points)
    }
}

// =========================================================================
// STEAM CLOUD STORAGE & REMOTE PROFILE SYNC
// =========================================================================

/// Emulated Steam Cloud Remote Storage (`ISteamRemoteStorage`) provider.
#[derive(Debug, Clone, Default)]
pub struct SteamCloudStorage {
    files: HashMap<String, Vec<u8>>,
    quota_bytes: usize,
    used_bytes: usize,
}

impl SteamCloudStorage {
    /// Creates a cloud storage instance with 100MB quota.
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            quota_bytes: 100 * 1024 * 1024,
            used_bytes: 0,
        }
    }

    /// Writes a file into Steam Cloud storage.
    pub fn file_write(&mut self, file_name: &str, data: &[u8]) -> Result<(), SteamStatsError> {
        let prev_len = self.files.get(file_name).map(|v| v.len()).unwrap_or(0);
        let new_len = data.len();

        if self.used_bytes - prev_len + new_len > self.quota_bytes {
            return Err(SteamStatsError::CloudStorageError(
                "Steam Cloud quota exceeded".to_string(),
            ));
        }

        self.used_bytes = self.used_bytes - prev_len + new_len;
        self.files.insert(file_name.to_string(), data.to_vec());
        Ok(())
    }

    /// Reads a file from Steam Cloud storage.
    pub fn file_read(&self, file_name: &str) -> Result<Vec<u8>, SteamStatsError> {
        match self.files.get(file_name) {
            Some(bytes) => Ok(bytes.clone()),
            None => Err(SteamStatsError::CloudStorageError(format!(
                "Cloud file '{}' not found",
                file_name
            ))),
        }
    }

    /// Checks if a file exists in Steam Cloud storage.
    pub fn file_exists(&self, file_name: &str) -> bool {
        self.files.contains_key(file_name)
    }

    /// Gets the size in bytes of a cloud file.
    pub fn file_size(&self, file_name: &str) -> usize {
        self.files.get(file_name).map(|v| v.len()).unwrap_or(0)
    }

    /// Deletes a file from Steam Cloud storage.
    pub fn file_delete(&mut self, file_name: &str) -> Result<bool, SteamStatsError> {
        if let Some(removed) = self.files.remove(file_name) {
            self.used_bytes -= removed.len();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Returns the number of files stored in cloud.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

/// Profile progression payload serialized to Steam Cloud or disk.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudProfilePayload {
    pub app_id: u32,
    pub timestamp: u64,
    pub stats: HashMap<String, i32>,
    pub achievements: Vec<String>,
}

impl CloudProfilePayload {
    /// Serializes profile to a portable string key-value format.
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        out.push_str("[OpenKF_Profile]\n");
        out.push_str(&format!("AppID={}\n", self.app_id));
        out.push_str(&format!("Timestamp={}\n", self.timestamp));

        out.push_str("\n[Stats]\n");
        let mut sorted_stats: Vec<_> = self.stats.iter().collect();
        sorted_stats.sort_by_key(|(k, _)| *k);
        for (k, v) in sorted_stats {
            out.push_str(&format!("{}={}\n", k, v));
        }

        out.push_str("\n[Achievements]\n");
        let mut sorted_ach = self.achievements.clone();
        sorted_ach.sort();
        for ach in sorted_ach {
            out.push_str(&format!("Unlocked={}\n", ach));
        }

        out
    }

    /// Deserializes profile from string key-value format.
    pub fn deserialize(text: &str) -> Result<Self, SteamStatsError> {
        let mut app_id = KILLING_FLOOR_APP_ID;
        let mut timestamp = 0;
        let mut stats = HashMap::new();
        let mut achievements = Vec::new();

        let mut section = "";

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                section = &line[1..line.len() - 1];
                continue;
            }

            if let Some((key, val)) = line.split_once('=') {
                let key = key.trim();
                let val = val.trim();

                match section {
                    "OpenKF_Profile" => {
                        if key == "AppID" {
                            app_id = val.parse().unwrap_or(KILLING_FLOOR_APP_ID);
                        } else if key == "Timestamp" {
                            timestamp = val.parse().unwrap_or(0);
                        }
                    }
                    "Stats" => {
                        if let Ok(num) = val.parse::<i32>() {
                            stats.insert(key.to_string(), num);
                        }
                    }
                    "Achievements"
                        if key == "Unlocked" => {
                            achievements.push(val.to_string());
                        }
                    _ => {}
                }
            }
        }

        Ok(Self {
            app_id,
            timestamp,
            stats,
            achievements,
        })
    }
}

impl SteamUserStats {
    /// Exports current profile to a `CloudProfilePayload`.
    pub fn export_cloud_profile(&self) -> CloudProfilePayload {
        let mut stats_map = HashMap::new();
        for (k, v) in &self.stats {
            if let StatValue::Int(val) = v {
                stats_map.insert(k.clone(), *val);
            }
        }

        let mut ach_list = Vec::new();
        for (k, v) in &self.achievements {
            if v.unlocked {
                ach_list.push(k.clone());
            }
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        CloudProfilePayload {
            app_id: self.app_id,
            timestamp: now,
            stats: stats_map,
            achievements: ach_list,
        }
    }

    /// Synchronizes and commits the profile to Steam Cloud under `UserStats.dat`.
    pub fn sync_cloud_profile(
        &mut self,
        cloud: &mut SteamCloudStorage,
    ) -> Result<(), SteamStatsError> {
        let payload = self.export_cloud_profile();
        let serialized = payload.serialize();
        cloud.file_write("UserStats.dat", serialized.as_bytes())?;
        self.store_stats()?;
        Ok(())
    }

    /// Loads and merges profile from Steam Cloud under `UserStats.dat`.
    /// Merges stats by taking the maximum progression value (conflict resolution).
    pub fn load_cloud_profile(
        &mut self,
        cloud: &SteamCloudStorage,
    ) -> Result<bool, SteamStatsError> {
        if !cloud.file_exists("UserStats.dat") {
            return Ok(false);
        }

        let bytes = cloud.file_read("UserStats.dat")?;
        let text = String::from_utf8(bytes).map_err(|e| {
            SteamStatsError::SerializationError(format!("Invalid UTF-8 in cloud profile: {}", e))
        })?;

        let payload = CloudProfilePayload::deserialize(&text)?;

        // Ensure stats are ready
        if !self.stats_requested {
            self.request_current_stats()?;
        }

        // Merge stats with highest value
        for (k, v) in payload.stats {
            let current = self.get_stat_int(&k).unwrap_or(0);
            if v > current {
                let _ = self.set_stat_int(&k, v);
            }
        }

        // Unlock achievements
        for ach in payload.achievements {
            let _ = self.set_achievement(&ach);
        }

        self.is_dirty = false;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_steam_stats_initialization_and_request() {
        let mut stats = SteamUserStats::new(KILLING_FLOOR_APP_ID);
        assert_eq!(stats.app_id(), 1250);
        assert!(!stats.is_ready());

        // Accessing stats before requesting returns StatsNotReady
        assert_eq!(
            stats.get_stat_int(stat_keys::DAMAGE_HEALED),
            Err(SteamStatsError::StatsNotReady)
        );

        // Request stats
        assert!(stats.request_current_stats().is_ok());
        assert!(stats.is_ready());

        // Default zero
        assert_eq!(stats.get_stat_int(stat_keys::DAMAGE_HEALED).unwrap(), 0);
    }

    #[test]
    fn test_steam_stats_get_set_int_and_dirty_tracking() {
        let mut stats = SteamUserStats::default();
        stats.request_current_stats().unwrap();
        assert!(!stats.is_dirty());

        stats.set_stat_int(stat_keys::DAMAGE_HEALED, 1500).unwrap();
        assert!(stats.is_dirty());
        assert_eq!(stats.get_stat_int(stat_keys::DAMAGE_HEALED).unwrap(), 1500);

        // Add delta
        let updated = stats.add_stat_int(stat_keys::DAMAGE_HEALED, 500).unwrap();
        assert_eq!(updated, 2000);
        assert_eq!(stats.get_stat_int(stat_keys::DAMAGE_HEALED).unwrap(), 2000);

        // Store stats commits changes
        let was_dirty = stats.store_stats().unwrap();
        assert!(was_dirty);
        assert!(!stats.is_dirty());

        // Store again when clean
        let was_dirty2 = stats.store_stats().unwrap();
        assert!(!was_dirty2);
    }

    #[test]
    fn test_steam_stats_float() {
        let mut stats = SteamUserStats::default();
        stats.request_current_stats().unwrap();

        stats.set_stat_float("AccuracyRate", 0.85).unwrap();
        let val = stats.get_stat_float("AccuracyRate").unwrap();
        assert!((val - 0.85).abs() < 1e-5);

        // Type mismatch check
        assert_eq!(
            stats.get_stat_int("AccuracyRate"),
            Err(SteamStatsError::TypeMismatch("AccuracyRate".to_string()))
        );
    }

    #[test]
    fn test_achievements_unlock_and_clear() {
        let mut stats = SteamUserStats::default();
        stats.request_current_stats().unwrap();

        let ach = achievement_keys::EXPERIMENTILOTTACIDE;
        assert_eq!(stats.get_achievement(ach).unwrap(), false);

        // Unlock
        let newly_unlocked = stats.set_achievement(ach).unwrap();
        assert!(newly_unlocked);
        assert_eq!(stats.get_achievement(ach).unwrap(), true);

        // Subsequent unlock call returns false
        let newly_unlocked_again = stats.set_achievement(ach).unwrap();
        assert!(!newly_unlocked_again);

        // Clear achievement
        let cleared = stats.clear_achievement(ach).unwrap();
        assert!(cleared);
        assert_eq!(stats.get_achievement(ach).unwrap(), false);
    }

    #[test]
    fn test_field_medic_perk_leveling() {
        let mut stats = SteamUserStats::default();
        stats.request_current_stats().unwrap();

        // Level 0
        assert_eq!(stats.calculate_perk_level(PerkType::FieldMedic), 0);

        // Level 1: 200
        stats.record_heal(200).unwrap();
        assert_eq!(stats.calculate_perk_level(PerkType::FieldMedic), 1);

        // Level 2: 750
        stats.record_heal(550).unwrap();
        assert_eq!(stats.calculate_perk_level(PerkType::FieldMedic), 2);

        // Level 3: 2000
        stats.record_heal(1250).unwrap();
        assert_eq!(stats.calculate_perk_level(PerkType::FieldMedic), 3);

        // Level 4: 4000
        stats.record_heal(2000).unwrap();
        assert_eq!(stats.calculate_perk_level(PerkType::FieldMedic), 4);

        // Level 5: 12000
        stats.record_heal(8000).unwrap();
        assert_eq!(stats.calculate_perk_level(PerkType::FieldMedic), 5);

        // Level 6: 25000
        stats.record_heal(13000).unwrap();
        assert_eq!(stats.calculate_perk_level(PerkType::FieldMedic), 6);

        let prog = stats.get_perk_progress(PerkType::FieldMedic);
        assert_eq!(prog.current_level, 6);
        assert!(prog.is_max_level);
        assert_eq!(prog.primary_current, 25000);
    }

    #[test]
    fn test_support_specialist_dual_requirement() {
        let mut stats = SteamUserStats::default();
        stats.request_current_stats().unwrap();

        // Shotgun damage high, but no welding -> Level 0
        stats
            .record_damage(WeaponCategory::Shotgun, 6_000_000)
            .unwrap();
        assert_eq!(stats.calculate_perk_level(PerkType::SupportSpecialist), 0);

        // Weld to Level 1 (100)
        stats.record_weld(100).unwrap();
        assert_eq!(stats.calculate_perk_level(PerkType::SupportSpecialist), 1);

        // Weld to Level 6 (25000)
        stats.record_weld(25000).unwrap();
        assert_eq!(stats.calculate_perk_level(PerkType::SupportSpecialist), 6);

        let prog = stats.get_perk_progress(PerkType::SupportSpecialist);
        assert_eq!(prog.current_level, 6);
        assert!(prog.is_max_level);
    }

    #[test]
    fn test_sharpshooter_headshots() {
        let mut stats = SteamUserStats::default();
        stats.request_current_stats().unwrap();

        for _ in 0..30 {
            stats
                .record_kill(
                    SpecimenType::Clot,
                    WeaponCategory::SharpshooterPistolOrRifle,
                    true,
                )
                .unwrap();
        }
        assert_eq!(stats.calculate_perk_level(PerkType::Sharpshooter), 1);
        assert_eq!(stats.get_stat_int(stat_keys::TOTAL_KILLS).unwrap(), 30);
    }

    #[test]
    fn test_commando_stalker_and_bullpup_damage() {
        let mut stats = SteamUserStats::default();
        stats.request_current_stats().unwrap();

        // Only damage, no stalkers -> 0
        stats
            .record_damage(WeaponCategory::AssaultRifle, 500_000)
            .unwrap();
        assert_eq!(stats.calculate_perk_level(PerkType::Commando), 0);

        // Add 30 stalkers -> Level 1
        for _ in 0..30 {
            stats
                .record_kill(SpecimenType::Stalker, WeaponCategory::AssaultRifle, false)
                .unwrap();
        }
        assert_eq!(stats.calculate_perk_level(PerkType::Commando), 1);
    }

    #[test]
    fn test_berserker_firebug_demolitions() {
        let mut stats = SteamUserStats::default();
        stats.request_current_stats().unwrap();

        stats
            .record_damage(WeaponCategory::Melee, 100_000)
            .unwrap();
        assert_eq!(stats.calculate_perk_level(PerkType::Berserker), 2);

        stats
            .record_damage(WeaponCategory::Flame, 1_500_000)
            .unwrap();
        assert_eq!(stats.calculate_perk_level(PerkType::Firebug), 4);

        stats
            .record_damage(WeaponCategory::Explosive, 5_500_000)
            .unwrap();
        assert_eq!(stats.calculate_perk_level(PerkType::Demolitions), 6);
    }

    #[test]
    fn test_achievement_kill_triggers() {
        let mut stats = SteamUserStats::default();
        stats.request_current_stats().unwrap();

        // Patriarch with LAW
        stats
            .record_kill(SpecimenType::Patriarch, WeaponCategory::Explosive, false)
            .unwrap();
        assert!(stats
            .get_achievement(achievement_keys::PATRIARCH_WITH_LAW)
            .unwrap());

        // Husk with Flamethrower
        stats
            .record_kill(SpecimenType::Husk, WeaponCategory::Flame, false)
            .unwrap();
        assert!(stats
            .get_achievement(achievement_keys::FLAMING_HELL)
            .unwrap());

        // Stalker with Explosives
        stats
            .record_kill(SpecimenType::Stalker, WeaponCategory::Explosive, false)
            .unwrap();
        assert!(stats
            .get_achievement(achievement_keys::SNEAKERS_OFF_FLOOR)
            .unwrap());
    }

    #[test]
    fn test_cloud_storage_and_sync() {
        let mut stats = SteamUserStats::default();
        stats.request_current_stats().unwrap();

        stats.record_heal(12000).unwrap();
        stats
            .set_achievement(achievement_keys::WIN_BIOTICS_LAB_NORMAL)
            .unwrap();

        let mut cloud = SteamCloudStorage::new();
        stats.sync_cloud_profile(&mut cloud).unwrap();

        assert!(cloud.file_exists("UserStats.dat"));
        assert!(cloud.file_size("UserStats.dat") > 0);

        // Load into a fresh SteamUserStats instance
        let mut fresh = SteamUserStats::default();
        let loaded = fresh.load_cloud_profile(&cloud).unwrap();
        assert!(loaded);
        assert_eq!(
            fresh.get_stat_int(stat_keys::DAMAGE_HEALED).unwrap(),
            12000
        );
        assert!(fresh
            .get_achievement(achievement_keys::WIN_BIOTICS_LAB_NORMAL)
            .unwrap());
        assert_eq!(fresh.calculate_perk_level(PerkType::FieldMedic), 5);
    }

    #[test]
    fn test_cloud_conflict_resolution_max_progress() {
        let mut local = SteamUserStats::default();
        local.request_current_stats().unwrap();
        local.record_heal(5000).unwrap();

        let mut cloud = SteamCloudStorage::new();

        // Create remote cloud profile with higher heal but lower something else
        let mut remote_stats = HashMap::new();
        remote_stats.insert(stat_keys::DAMAGE_HEALED.to_string(), 10000);
        remote_stats.insert(stat_keys::HEADSHOT_KILLS.to_string(), 25);

        let remote_payload = CloudProfilePayload {
            app_id: KILLING_FLOOR_APP_ID,
            timestamp: 100000,
            stats: remote_stats,
            achievements: vec![achievement_keys::EXPERIMENTILOTTACIDE.to_string()],
        };

        cloud
            .file_write("UserStats.dat", remote_payload.serialize().as_bytes())
            .unwrap();

        // Merge cloud into local
        local.load_cloud_profile(&cloud).unwrap();

        // Should take maximum (10000 > 5000)
        assert_eq!(
            local.get_stat_int(stat_keys::DAMAGE_HEALED).unwrap(),
            10000
        );
        assert_eq!(local.get_stat_int(stat_keys::HEADSHOT_KILLS).unwrap(), 25);
        assert!(local
            .get_achievement(achievement_keys::EXPERIMENTILOTTACIDE)
            .unwrap());
    }

    #[test]
    fn test_reset_all_stats() {
        let mut stats = SteamUserStats::default();
        stats.request_current_stats().unwrap();

        stats.record_heal(3000).unwrap();
        stats
            .set_achievement(achievement_keys::WIN_BIOTICS_LAB_NORMAL)
            .unwrap();

        stats.reset_all_stats(true).unwrap();
        assert_eq!(stats.get_stat_int(stat_keys::DAMAGE_HEALED).unwrap(), 0);
        assert_eq!(
            stats
                .get_achievement(achievement_keys::WIN_BIOTICS_LAB_NORMAL)
                .unwrap(),
            false
        );
    }
}
