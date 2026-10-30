//! Killing Floor Perk Selection and Progression UI Component.
//!
//! Recreates the authentic retail Killing Floor (KF1) Perk Selection screen
//! built on the Unreal Engine 2.5 GUI2K4 architecture:
//! - All 7 iconic Perks: Field Medic, Support Specialist, Sharpshooter,
//!   Commando, Berserker, Firebug, Demolitions.
//! - Retail level progression (Levels 0 through 6) with dual primary/secondary
//!   stat requirements (e.g. Damage Healed, Shotgun Damage & Welding Points,
//!   Headshot Kills, Bullpup Damage & Stalker Kills, Melee Damage, Flame Damage,
//!   Explosive Damage).
//! - Detailed level progression bars, numeric counters, and level-up milestones.
//! - Dynamic stat bonuses matching retail percentages (syringe recharge,
//!   carry capacity, recoil reduction, zed-time extensions, clot immunity, etc.).
//! - Characteristic perk weapon loadouts with tier ranking, weight limits,
//!   and level-based merchant discounts.
//! - Interactive GUIComponent with mouse selection, hover states, tab navigation,
//!   and decoupled perk selection / confirmation callbacks.

use std::any::Any;
use std::collections::HashMap;
use std::fmt::Debug;

use crate::gui::{
    Canvas, ComponentState, GUIComponent, LayoutBox, MouseButton, MouseEvent, Rect,
    TextAlignment,
};
pub use crate::hud::PerkType;

/// The 7 Killing Floor retail perks in standard display order.
pub const ALL_PERKS: [PerkType; 7] = [
    PerkType::FieldMedic,
    PerkType::SupportSpecialist,
    PerkType::Sharpshooter,
    PerkType::Commando,
    PerkType::Berserker,
    PerkType::Firebug,
    PerkType::Demolitions,
];

/// Maximum achievable perk level in Killing Floor 1.
pub const MAX_PERK_LEVEL: u8 = 6;

/// Tab views inside the perk detail inspection panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PerkDetailTab {
    #[default]
    Bonuses,
    Arsenal,
    Requirements,
}

impl PerkDetailTab {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Bonuses => "STAT BONUSES",
            Self::Arsenal => "WEAPON ARSENAL",
            Self::Requirements => "LEVEL REQUIREMENTS",
        }
    }
}

/// Helper function providing the full display title of a perk.
pub fn perk_title(perk: PerkType) -> &'static str {
    match perk {
        PerkType::FieldMedic => "Field Medic",
        PerkType::SupportSpecialist => "Support Specialist",
        PerkType::Sharpshooter => "Sharpshooter",
        PerkType::Commando => "Commando",
        PerkType::Berserker => "Berserker",
        PerkType::Firebug => "Firebug",
        PerkType::Demolitions => "Demolitions",
    }
}

/// Helper function providing the short tag of a perk.
pub fn perk_tag(perk: PerkType) -> &'static str {
    match perk {
        PerkType::FieldMedic => "MEDIC",
        PerkType::SupportSpecialist => "SUPPORT",
        PerkType::Sharpshooter => "SHARPSHOOTER",
        PerkType::Commando => "COMMANDO",
        PerkType::Berserker => "BERSERKER",
        PerkType::Firebug => "FIREBUG",
        PerkType::Demolitions => "DEMO",
    }
}

/// Concise role summary of each perk.
pub fn perk_role_summary(perk: PerkType) -> &'static str {
    match perk {
        PerkType::FieldMedic => {
            "Team healer, survivability anchor, and high-speed defensive tank."
        }
        PerkType::SupportSpecialist => {
            "Shotgun heavy gunner, door welder/fortifier, and ammunition carrier."
        }
        PerkType::Sharpshooter => {
            "Precision long-range marksman specialized in high-priority Large Zed headshots."
        }
        PerkType::Commando => {
            "Assault rifle scout, invisible Stalker spotter, and Zed-Time chain extender."
        }
        PerkType::Berserker => {
            "Frontline melee brawler, crowd-control tank, and fast-moving kiter immune to Clots."
        }
        PerkType::Firebug => {
            "Area-of-effect crowd control and panic inducer with lingering incendiary fire."
        }
        PerkType::Demolitions => {
            "Heavy explosive artillery specialized in burst clearing and Fleshpound chunking."
        }
    }
}

/// Primary thematic RGBA color for perk UI headers and progress bars.
pub fn perk_theme_color(perk: PerkType) -> [f32; 4] {
    match perk {
        PerkType::FieldMedic => [0.15, 0.65, 0.75, 1.0],       // Medical Cyan/Teal
        PerkType::SupportSpecialist => [0.25, 0.45, 0.85, 1.0], // Tactical Blue
        PerkType::Sharpshooter => [0.85, 0.65, 0.15, 1.0],      // Precision Brass/Amber
        PerkType::Commando => [0.35, 0.70, 0.30, 1.0],          // Military Green
        PerkType::Berserker => [0.85, 0.20, 0.20, 1.0],         // Blood Red
        PerkType::Firebug => [0.95, 0.45, 0.10, 1.0],           // Flame Orange
        PerkType::Demolitions => [0.90, 0.75, 0.15, 1.0],       // Hazard Yellow
    }
}

/// Secondary background tint for perk item cards.
pub fn perk_tint_color(perk: PerkType) -> [f32; 4] {
    let mut c = perk_theme_color(perk);
    c[0] *= 0.25;
    c[1] *= 0.25;
    c[2] *= 0.25;
    c[3] = 0.50;
    c
}

/// Individual perk stat bonus entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerkStatBonus {
    pub title: &'static str,
    pub value: String,
    pub description: &'static str,
    pub is_passive: bool,
}

impl PerkStatBonus {
    pub fn new(title: &'static str, value: impl Into<String>, description: &'static str, is_passive: bool) -> Self {
        Self {
            title,
            value: value.into(),
            description,
            is_passive,
        }
    }
}

/// Characteristic weapon associated with a perk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerkWeapon {
    pub name: &'static str,
    pub tier: u8,
    pub weight: u32,
    pub base_price: u32,
    pub ammo_type: &'static str,
    pub description: &'static str,
}

impl PerkWeapon {
    pub const fn new(
        name: &'static str,
        tier: u8,
        weight: u32,
        base_price: u32,
        ammo_type: &'static str,
        description: &'static str,
    ) -> Self {
        Self {
            name,
            tier,
            weight,
            base_price,
            ammo_type,
            description,
        }
    }

    /// Calculates discounted store price given a discount percentage (0..=100).
    pub fn discounted_price(&self, discount_percent: u32) -> u32 {
        let clamped_pct = discount_percent.min(100);
        let factor = (100 - clamped_pct) as f32 / 100.0;
        ((self.base_price as f32) * factor).round() as u32
    }
}

/// Progression milestone requirements for each level (0..=6).
pub mod perk_requirements {
    use super::PerkType;

    /// Primary requirement name for each perk.
    pub fn primary_stat_name(perk: PerkType) -> &'static str {
        match perk {
            PerkType::FieldMedic => "Damage Healed",
            PerkType::SupportSpecialist => "Shotgun Damage",
            PerkType::Sharpshooter => "Headshot Kills",
            PerkType::Commando => "Bullpup / AR Damage",
            PerkType::Berserker => "Melee Damage",
            PerkType::Firebug => "Flame Damage",
            PerkType::Demolitions => "Explosive Damage",
        }
    }

    /// Secondary requirement name for perks that have dual criteria.
    pub fn secondary_stat_name(perk: PerkType) -> Option<&'static str> {
        match perk {
            PerkType::SupportSpecialist => Some("Door Welding Points"),
            PerkType::Commando => Some("Stalkers Killed"),
            _ => None,
        }
    }

    /// Thresholds required to reach levels [0, 1, 2, 3, 4, 5, 6].
    pub fn thresholds(perk: PerkType) -> (&'static [u64; 7], Option<&'static [u64; 7]>) {
        const FIELD_MEDIC_HEAL: [u64; 7] = [0, 200, 750, 2_000, 4_000, 12_000, 25_000];

        const DAMAGE_SCALE_STANDARD: [u64; 7] = [0, 25_000, 100_000, 500_000, 1_500_000, 3_500_000, 5_500_000];

        const SUPPORT_WELDING: [u64; 7] = [0, 2_000, 7_000, 35_000, 100_000, 250_000, 370_000];

        const SHARPSHOOTER_HEADSHOTS: [u64; 7] = [0, 30, 100, 700, 2_500, 5_500, 8_500];

        const COMMANDO_STALKERS: [u64; 7] = [0, 30, 100, 700, 1_600, 2_400, 3_600];

        match perk {
            PerkType::FieldMedic => (&FIELD_MEDIC_HEAL, None),
            PerkType::SupportSpecialist => (&DAMAGE_SCALE_STANDARD, Some(&SUPPORT_WELDING)),
            PerkType::Sharpshooter => (&SHARPSHOOTER_HEADSHOTS, None),
            PerkType::Commando => (&DAMAGE_SCALE_STANDARD, Some(&COMMANDO_STALKERS)),
            PerkType::Berserker => (&DAMAGE_SCALE_STANDARD, None),
            PerkType::Firebug => (&DAMAGE_SCALE_STANDARD, None),
            PerkType::Demolitions => (&DAMAGE_SCALE_STANDARD, None),
        }
    }
}

/// Tracks player progress for an individual perk.
#[derive(Clone, Debug, PartialEq)]
pub struct PerkProgression {
    pub perk: PerkType,
    pub level: u8,
    pub primary_current: u64,
    pub secondary_current: Option<u64>,
}

impl PerkProgression {
    /// Creates a progression record with default 0 progress at specified level.
    pub fn new(perk: PerkType, level: u8) -> Self {
        let (prim_req, sec_req) = perk_requirements::thresholds(perk);
        let lvl = level.min(MAX_PERK_LEVEL) as usize;
        let primary_current = prim_req[lvl];
        let secondary_current = sec_req.map(|sec| sec[lvl]);

        Self {
            perk,
            level: level.min(MAX_PERK_LEVEL),
            primary_current,
            secondary_current,
        }
    }

    /// Creates a progression record with explicit current values.
    pub fn with_values(
        perk: PerkType,
        level: u8,
        primary_current: u64,
        secondary_current: Option<u64>,
    ) -> Self {
        Self {
            perk,
            level: level.min(MAX_PERK_LEVEL),
            primary_current,
            secondary_current,
        }
    }

    pub fn primary_name(&self) -> &'static str {
        perk_requirements::primary_stat_name(self.perk)
    }

    pub fn secondary_name(&self) -> Option<&'static str> {
        perk_requirements::secondary_stat_name(self.perk)
    }

    pub fn is_max_level(&self) -> bool {
        self.level >= MAX_PERK_LEVEL
    }

    /// Target primary value for the next level (or current max requirement if at level 6).
    pub fn primary_target(&self) -> u64 {
        let (prim_thresh, _) = perk_requirements::thresholds(self.perk);
        let target_idx = if self.is_max_level() {
            MAX_PERK_LEVEL as usize
        } else {
            (self.level + 1) as usize
        };
        prim_thresh[target_idx]
    }

    /// Target secondary value for the next level (if perk has dual criteria).
    pub fn secondary_target(&self) -> Option<u64> {
        let (_, sec_thresh) = perk_requirements::thresholds(self.perk);
        sec_thresh.map(|thresh| {
            let target_idx = if self.is_max_level() {
                MAX_PERK_LEVEL as usize
            } else {
                (self.level + 1) as usize
            };
            thresh[target_idx]
        })
    }

    /// Progress ratio (0.0 to 1.0) towards the next primary requirement.
    pub fn primary_ratio(&self) -> f32 {
        if self.is_max_level() {
            return 1.0;
        }
        let (prim_thresh, _) = perk_requirements::thresholds(self.perk);
        let current_lvl_thresh = prim_thresh[self.level as usize];
        let next_lvl_thresh = prim_thresh[(self.level + 1) as usize];

        if next_lvl_thresh <= current_lvl_thresh {
            return 1.0;
        }

        let clamped = self.primary_current.clamp(current_lvl_thresh, next_lvl_thresh);
        let numerator = (clamped - current_lvl_thresh) as f32;
        let denominator = (next_lvl_thresh - current_lvl_thresh) as f32;
        (numerator / denominator).clamp(0.0, 1.0)
    }

    /// Progress ratio (0.0 to 1.0) towards the next secondary requirement.
    pub fn secondary_ratio(&self) -> Option<f32> {
        if self.is_max_level() {
            return Some(1.0);
        }
        let (_, sec_thresh) = perk_requirements::thresholds(self.perk);
        let thresh = sec_thresh?;
        let current_val = self.secondary_current.unwrap_or(0);

        let current_lvl_thresh = thresh[self.level as usize];
        let next_lvl_thresh = thresh[(self.level + 1) as usize];

        if next_lvl_thresh <= current_lvl_thresh {
            return Some(1.0);
        }

        let clamped = current_val.clamp(current_lvl_thresh, next_lvl_thresh);
        let numerator = (clamped - current_lvl_thresh) as f32;
        let denominator = (next_lvl_thresh - current_lvl_thresh) as f32;
        Some((numerator / denominator).clamp(0.0, 1.0))
    }

    /// Combined overall progress ratio (0.0 to 1.0) toward the next level.
    pub fn overall_ratio(&self) -> f32 {
        if self.is_max_level() {
            return 1.0;
        }
        let p = self.primary_ratio();
        if let Some(s) = self.secondary_ratio() {
            (p + s) * 0.5
        } else {
            p
        }
    }

    /// Adds progress points and automatically triggers level-ups when thresholds are satisfied.
    /// Returns true if a level up occurred.
    pub fn add_progress(&mut self, primary_delta: u64, secondary_delta: Option<u64>) -> bool {
        self.primary_current = self.primary_current.saturating_add(primary_delta);
        if let Some(sec_add) = secondary_delta {
            self.secondary_current = Some(self.secondary_current.unwrap_or(0).saturating_add(sec_add));
        }
        self.check_level_up()
    }

    /// Evaluates current stats against level requirements and increments level if eligible.
    pub fn check_level_up(&mut self) -> bool {
        let (prim_thresh, sec_thresh) = perk_requirements::thresholds(self.perk);
        let mut leveled = false;

        while self.level < MAX_PERK_LEVEL {
            let next_lvl = (self.level + 1) as usize;
            let prim_ok = self.primary_current >= prim_thresh[next_lvl];
            let sec_ok = match sec_thresh {
                Some(thresh) => self.secondary_current.unwrap_or(0) >= thresh[next_lvl],
                None => true,
            };

            if prim_ok && sec_ok {
                self.level += 1;
                leveled = true;
            } else {
                break;
            }
        }

        leveled
    }
}

/// Retrieves the comprehensive list of active stat bonuses for a given perk and level.
pub fn get_perk_bonuses(perk: PerkType, level: u8) -> Vec<PerkStatBonus> {
    let lvl = level.min(MAX_PERK_LEVEL);

    match perk {
        PerkType::FieldMedic => {
            let heal_boost = match lvl {
                0 => 10,
                1 => 20,
                2 => 30,
                3 => 40,
                4 => 50,
                5 => 75,
                _ => 100,
            };
            let recharge_speed = match lvl {
                0 => 25,
                1 => 50,
                2 => 75,
                3 => 100,
                4 => 125,
                5 => 150,
                _ => 200,
            };
            let armor_disc = match lvl {
                0 => 10,
                1 => 20,
                2 => 30,
                3 => 40,
                4 => 50,
                5 => 60,
                _ => 75,
            };
            let dmg_resist = match lvl {
                0 => 5,
                1 => 7,
                2 => 10,
                3 => 15,
                4 => 18,
                5 => 20,
                _ => 25,
            };
            let move_spd = match lvl {
                0 => 0,
                1 => 3,
                2 => 5,
                3 => 8,
                4 => 10,
                5 => 12,
                _ => 15,
            };

            vec![
                PerkStatBonus::new("Heal Potency", format!("+{}% Healing", heal_boost), "Increases amount of health restored per syringe dart.", false),
                PerkStatBonus::new("Syringe Recharge", format!("+{}% Recharge Speed", recharge_speed), "Faster medical syringe refill cycle.", false),
                PerkStatBonus::new("Combat Armor Discount", format!("{}% Discount", armor_disc), "Cheaper Kevlar body armor purchases at Trader.", false),
                PerkStatBonus::new("Damage Resistance", format!("+{}% Damage Resistance", dmg_resist), "Passive damage reduction against all incoming hits.", true),
                PerkStatBonus::new("Movement Speed", format!("+{}% Sprint Speed", move_spd), "Base movement velocity enhancement.", true),
                PerkStatBonus::new(
                    "Spawn Loadout",
                    if lvl >= 6 { "MP7M + Body Armor + Dual 9mms" } else if lvl >= 5 { "MP7M + Kevlar Vest" } else { "Medical Syringe" },
                    "Free starting equipment when spawning or respawning.",
                    false,
                ),
            ]
        }
        PerkType::SupportSpecialist => {
            let dmg = match lvl {
                0 => 10,
                1 => 15,
                2 => 20,
                3 => 30,
                4 => 40,
                5 => 50,
                _ => 60,
            };
            let penetration = match lvl {
                0 => 15,
                1 => 25,
                2 => 40,
                3 => 55,
                4 => 70,
                5 => 80,
                _ => 90,
            };
            let extra_weight = match lvl {
                0 => 0,
                1 => 1,
                2 => 2,
                3 => 3,
                4 => 5,
                5 => 7,
                _ => 9,
            };
            let weld_speed = match lvl {
                0 => 25,
                1 => 50,
                2 => 75,
                3 => 100,
                4 => 120,
                5 => 135,
                _ => 150,
            };
            let extra_grenades = match lvl {
                0 => 0,
                1 => 1,
                2 => 2,
                3 => 3,
                4 => 4,
                5 => 5,
                _ => 6,
            };
            let discount = match lvl {
                0 => 10,
                1 => 20,
                2 => 30,
                3 => 40,
                4 => 50,
                5 => 60,
                _ => 70,
            };

            vec![
                PerkStatBonus::new("Shotgun Damage", format!("+{}% Damage", dmg), "Direct damage boost for all shotgun pellets.", false),
                PerkStatBonus::new("Shotgun Penetration", format!("+{}% Penetration", penetration), "Pellets pierce through multiple Zeds with lower damage dropoff.", false),
                PerkStatBonus::new("Carry Weight", format!("+{} Blocks ({} Total)", extra_weight, 15 + extra_weight), "Expanded backpack capacity for multiple primary weapons.", true),
                PerkStatBonus::new("Welding Speed", format!("+{}% Door Repair", weld_speed), "Faster door welding/unwelding speed and increased door HP.", false),
                PerkStatBonus::new("Grenade Capacity", format!("+{} Frag Grenades", extra_grenades), "Expanded throwable inventory pool.", false),
                PerkStatBonus::new("Shotgun Discount", format!("{}% Discount", discount), "Trader price reduction on shotguns and shells.", false),
            ]
        }
        PerkType::Sharpshooter => {
            let hs_mult = match lvl {
                0 => 20,
                1 => 40,
                2 => 60,
                3 => 80,
                4 => 100,
                5 => 120,
                _ => 140,
            };
            let recoil = match lvl {
                0 => 10,
                1 => 20,
                2 => 30,
                3 => 45,
                4 => 60,
                5 => 70,
                _ => 75,
            };
            let reload = match lvl {
                0 => 10,
                1 => 20,
                2 => 30,
                3 => 40,
                4 => 50,
                5 => 55,
                _ => 60,
            };
            let discount = match lvl {
                0 => 10,
                1 => 20,
                2 => 30,
                3 => 40,
                4 => 50,
                5 => 60,
                _ => 70,
            };

            vec![
                PerkStatBonus::new("Headshot Damage", format!("+{}% Headshot Damage", hs_mult), "Massive precision critical damage with pistols and rifles.", false),
                PerkStatBonus::new("Recoil Reduction", format!("-{}% Recoil", recoil), "Reduced muzzle climb for 9mm, Handcannons, and rifles.", false),
                PerkStatBonus::new("Reload Speed", format!("+{}% Reload Speed", reload), "Faster magazine and cylinder reloads.", false),
                PerkStatBonus::new("Precision Discount", format!("{}% Discount", discount), "Cheaper Crossbow, Winchester, Handcannons, and M14 EBR.", false),
                PerkStatBonus::new(
                    "Spawn Loadout",
                    if lvl >= 6 { "Crossbow + Handcannon" } else if lvl >= 5 { "Winchester Lever Action" } else { "9mm Pistol" },
                    "Free starting equipment when spawning or respawning.",
                    false,
                ),
            ]
        }
        PerkType::Commando => {
            let dmg = match lvl {
                0 => 5,
                1 => 10,
                2 => 15,
                3 => 25,
                4 => 35,
                5 => 45,
                _ => 50,
            };
            let recoil = match lvl {
                0 => 10,
                1 => 20,
                2 => 30,
                3 => 40,
                4 => 50,
                5 => 55,
                _ => 60,
            };
            let mag_boost = match lvl {
                0 => 5,
                1 => 8,
                2 => 12,
                3 => 15,
                4 => 20,
                5 => 22,
                _ => 25,
            };
            let stalker_dist = match lvl {
                0 => 4,
                1 => 6,
                2 => 8,
                3 => 10,
                4 => 12,
                5 => 14,
                _ => 16,
            };
            let zed_extensions = match lvl {
                0 => 1,
                1 => 1,
                2 => 2,
                3 => 2,
                4 => 3,
                5 => 3,
                _ => 4,
            };

            vec![
                PerkStatBonus::new("Assault Rifle Damage", format!("+{}% Damage", dmg), "Higher lethality for Bullpup, AK-47, M4, SCAR, and FN FAL.", false),
                PerkStatBonus::new("Recoil Control", format!("-{}% Recoil", recoil), "Tighter burst spread and sustained automatic fire.", false),
                PerkStatBonus::new("Clip Capacity", format!("+{}% Mag Size", mag_boost), "Larger rifle magazine reserves before needing to reload.", false),
                PerkStatBonus::new("Stalker Detection", format!("{}m Cloak Sight", stalker_dist), "Reveals cloaked Stalkers in glowing red for the whole team.", true),
                PerkStatBonus::new("Zed Health Bars", format!("{}m HP Bar Range", stalker_dist), "Displays specimen remaining health bars above their heads.", true),
                PerkStatBonus::new("Zed-Time Extension", format!("{} Chain Extensions", zed_extensions), "Extends bullet-time slow motion on consecutive kills.", false),
            ]
        }
        PerkType::Berserker => {
            let dmg = match lvl {
                0 => 10,
                1 => 20,
                2 => 35,
                3 => 50,
                4 => 65,
                5 => 80,
                _ => 100,
            };
            let atk_spd = match lvl {
                0 => 10,
                1 => 15,
                2 => 20,
                3 => 25,
                4 => 30,
                5 => 32,
                _ => 35,
            };
            let move_spd = match lvl {
                0 => 5,
                1 => 10,
                2 => 15,
                3 => 20,
                4 => 25,
                5 => 28,
                _ => 30,
            };
            let dmg_resist = match lvl {
                0 => 5,
                1 => 10,
                2 => 15,
                3 => 20,
                4 => 25,
                5 => 30,
                _ => 40,
            };
            let bile_resist = match lvl {
                0 => 20,
                1 => 30,
                2 => 40,
                3 => 50,
                4 => 60,
                5 => 70,
                _ => 80,
            };

            vec![
                PerkStatBonus::new("Melee Damage", format!("+{}% Melee Damage", dmg), "Increases damage for Katana, Fire Axe, Chainsaw, and Claymore.", false),
                PerkStatBonus::new("Attack Speed", format!("+{}% Swing Speed", atk_spd), "Faster melee swings and secondary heavy strikes.", false),
                PerkStatBonus::new("Movement Speed", format!("+{}% Speed", move_spd), "Outrun specimens when wielding melee weaponry.", true),
                PerkStatBonus::new("Damage Resistance", format!("+{}% All Resist", dmg_resist), "General damage absorption across all hit types.", true),
                PerkStatBonus::new("Bile Resistance", format!("+{}% Bloat Resist", bile_resist), "Immunity to Bloat vomit screen disruption and acid burn.", true),
                PerkStatBonus::new(
                    "Clot Grab Immunity",
                    if lvl >= 5 { "Active" } else { "Requires Lv. 5" },
                    "Prevents Clots from latching onto you or stopping your movement.",
                    true,
                ),
            ]
        }
        PerkType::Firebug => {
            let dmg = match lvl {
                0 => 10,
                1 => 15,
                2 => 25,
                3 => 35,
                4 => 45,
                5 => 55,
                _ => 60,
            };
            let burn_dur = match lvl {
                0 => 20,
                1 => 35,
                2 => 50,
                3 => 65,
                4 => 80,
                5 => 90,
                _ => 100,
            };
            let reload = match lvl {
                0 => 10,
                1 => 20,
                2 => 30,
                3 => 40,
                4 => 50,
                5 => 55,
                _ => 60,
            };
            let tank_cap = match lvl {
                0 => 10,
                1 => 25,
                2 => 40,
                3 => 60,
                4 => 80,
                5 => 90,
                _ => 100,
            };
            let fire_resist = match lvl {
                0 => 20,
                1 => 35,
                2 => 50,
                3 => 65,
                4 => 80,
                5 => 90,
                _ => 100,
            };

            vec![
                PerkStatBonus::new("Flame Damage", format!("+{}% Fire Damage", dmg), "Higher burning ticks and instant flame impact damage.", false),
                PerkStatBonus::new("Burn Duration", format!("+{}% Burn Duration", burn_dur), "Zeds remain ablaze longer, inciting panic and stagger.", false),
                PerkStatBonus::new("Reload Speed", format!("+{}% Reload Speed", reload), "Faster fuel canister and incendiary clip reloading.", false),
                PerkStatBonus::new("Fuel Capacity", format!("+{}% Tank Size", tank_cap), "Substantially expanded fuel reservoirs.", false),
                PerkStatBonus::new(
                    "Fire Resistance",
                    if fire_resist >= 100 { "100% (Immune)".to_string() } else { format!("+{}% Resistance", fire_resist) },
                    "Protection from Husk fireballs and self-inflicted ground burns.",
                    true,
                ),
                PerkStatBonus::new(
                    "Spawn Loadout",
                    if lvl >= 6 { "Flamethrower + Body Armor" } else if lvl >= 5 { "MAC-10 Incendiary" } else { "9mm Pistol" },
                    "Free starting equipment when spawning or respawning.",
                    false,
                ),
            ]
        }
        PerkType::Demolitions => {
            let dmg = match lvl {
                0 => 10,
                1 => 15,
                2 => 25,
                3 => 35,
                4 => 45,
                5 => 55,
                _ => 60,
            };
            let ammo_boost = match lvl {
                0 => 10,
                1 => 20,
                2 => 30,
                3 => 40,
                4 => 50,
                5 => 55,
                _ => 60,
            };
            let pipe_bombs = match lvl {
                0 => 2,
                1 => 3,
                2 => 4,
                3 => 5,
                4 => 6,
                5 => 7,
                _ => 8,
            };
            let self_resist = match lvl {
                0 => 25,
                1 => 35,
                2 => 45,
                3 => 55,
                4 => 65,
                5 => 70,
                _ => 75,
            };
            let discount = match lvl {
                0 => 10,
                1 => 20,
                2 => 30,
                3 => 40,
                4 => 50,
                5 => 60,
                _ => 70,
            };

            vec![
                PerkStatBonus::new("Explosive Damage", format!("+{}% Damage", dmg), "Boosted explosive blast yield for grenades, M79, and M32.", false),
                PerkStatBonus::new("Ammo Capacity", format!("+{}% Explosive Ammo", ammo_boost), "Higher reserve grenade shell counts.", false),
                PerkStatBonus::new("Pipe Bomb Max", format!("{} Landmines Deployable", pipe_bombs), "Max concurrent proximity pipe bombs placed on the map.", false),
                PerkStatBonus::new("Self-Damage Resistance", format!("+{}% Resistance", self_resist), "Mitigates fatal self-inflicted explosions at point-blank.", true),
                PerkStatBonus::new("Explosives Discount", format!("{}% Discount", discount), "Lowered store costs for rocket launchers and munitions.", false),
            ]
        }
    }
}

/// Retrieves the characteristic weapon arsenal associated with a perk.
pub fn get_perk_weapons(perk: PerkType) -> Vec<PerkWeapon> {
    match perk {
        PerkType::FieldMedic => vec![
            PerkWeapon::new("Medical Syringe", 0, 0, 0, "Self-charging", "Standard-issue biometric self & teammate injector."),
            PerkWeapon::new("MP7M Medic Gun", 1, 3, 300, "9mm Submachine", "Light submachine gun equipped with auto-tracking healing darts."),
            PerkWeapon::new("MP5M Medic Gun", 2, 3, 500, "9mm Submachine", "Rapid fire medical PDW with generous dart reservoir."),
            PerkWeapon::new("M7A3 Medic Gun", 3, 6, 1250, "Heavy Dart Rifle", "High-stopping-power carbine with rapid healing recharge."),
            PerkWeapon::new("Schneidzekgova Medical Gun", 4, 8, 2500, "Energy Cells", "State-of-the-art heavy energy rifle capable of instant team stabilization."),
        ],
        PerkType::SupportSpecialist => vec![
            PerkWeapon::new("Welder", 0, 0, 0, "Battery Pack", "Seals and reinforces security doors against incoming Zed hordes."),
            PerkWeapon::new("12 Gauge Pump Shotgun", 1, 8, 200, "12ga Buckshot", "Reliable pump-action shotgun delivering tight pellet spreads."),
            PerkWeapon::new("Vlad 9000 Nailgun", 2, 6, 750, "Industrial Nails", "High-velocity nail projector whose projectiles ricochet off walls."),
            PerkWeapon::new("Hunting Shotgun", 2, 10, 650, "10ga Heavy Shot", "Double-barrel break-action devastating large Zeds at point-blank."),
            PerkWeapon::new("Combat Shotgun", 3, 8, 1000, "12ga Box Mag", "Semi-automatic tactical shotgun with rapid box magazine reloads."),
            PerkWeapon::new("AA-12 Auto Shotgun", 4, 10, 4000, "12ga Drum Mag", "Fully automatic 20-round drum-fed room clearer."),
        ],
        PerkType::Sharpshooter => vec![
            PerkWeapon::new("9mm Tactical Pistol", 1, 0, 100, "9mm Parabellum", "Standard sidearm capable of rapid headshot decaps."),
            PerkWeapon::new("Dual 9mm Pistols", 1, 4, 200, "9mm Parabellum", "Akimbo pistols doubling firepower and sustained volume."),
            PerkWeapon::new("Handcannon (.50 AE)", 2, 2, 500, ".50 Action Express", "Heavy caliber revolver punching through Scrake skulls."),
            PerkWeapon::new("Dual Handcannons", 2, 4, 1000, ".50 Action Express", "Twin hand cannons offering extreme single-target burst."),
            PerkWeapon::new("Winchester Lever-Action", 1, 6, 200, ".44 Magnum", "Iconic lever-action carbine with instant target acquisition."),
            PerkWeapon::new("Crossbow", 3, 9, 800, "Steel Bolts", "Long-range sniper weapon with reusable armor-piercing bolts."),
            PerkWeapon::new("M14 EBR", 3, 8, 2500, "7.62mm NATO", "Laser-sighted battle rifle perfect for continuous headshot chains."),
            PerkWeapon::new("M99 AMR", 4, 13, 3500, ".50 BMG Anti-Materiel", "Massive single-shot anti-materiel cannon capable of dropping a Fleshpound in one shot."),
        ],
        PerkType::Commando => vec![
            PerkWeapon::new("Bullpup", 1, 6, 400, "5.56mm NATO", "Compact assault rifle with integrated reflex optic."),
            PerkWeapon::new("AK-47", 2, 6, 750, "7.62x39mm", "Rugged rifle with substantial stopping power against medium Zeds."),
            PerkWeapon::new("M4 Assault Rifle", 3, 6, 1000, "5.56mm NATO", "Modernized carbine featuring rapid handling and tight burst spread."),
            PerkWeapon::new("SCAR Mk17", 4, 5, 2500, "7.62mm NATO", "Heavy automatic battle rifle boasting high per-bullet damage."),
            PerkWeapon::new("FN FAL ACOG", 4, 6, 2750, "7.62mm NATO", "Magnified scoped rifle effective at both medium and long ranges."),
        ],
        PerkType::Berserker => vec![
            PerkWeapon::new("Combat Knife", 0, 0, 0, "Edge", "Basic survival blade with fast forward thrusts."),
            PerkWeapon::new("Machete", 1, 1, 100, "Heavy Blade", "Broad clearing blade effective for decapitating Clots and Crawlers."),
            PerkWeapon::new("Fire Axe", 2, 5, 250, "Double-Bit Axe", "Heavy two-handed axe with powerful overhead cleaving chops."),
            PerkWeapon::new("Katana", 2, 3, 1000, "Folded Steel", "Swift razor blade slicing cleanly through swarming packs."),
            PerkWeapon::new("Chainsaw", 3, 8, 1000, "Gasoline Motor", "Continuous spinning saw chewing through Gorefasts and Scrakes."),
            PerkWeapon::new("Claymore", 3, 6, 1500, "Greatsword", "Broad reach greatsword staggering heavy Zeds on contact."),
            PerkWeapon::new("Buzzsaw Bow", 4, 7, 2500, "Circular Sawblades", "Shoots spinning metal blades that decapitate and ricochet off surfaces."),
        ],
        PerkType::Firebug => vec![
            PerkWeapon::new("MAC-10 Incendiary", 2, 4, 500, ".45 ACP Dragonsbreath", "High rate of fire machine pistol loading incendiary tracer rounds."),
            PerkWeapon::new("Trench Gun", 2, 8, 1250, "12ga Dragonsbreath", "Slam-fire shotgun spreading intense burning pellets."),
            PerkWeapon::new("Flamethrower", 3, 10, 750, "Napalm Fuel", "Stream of pressurized flame igniting entire corridors."),
            PerkWeapon::new("Flare Revolver", 2, 2, 500, "Magnesium Flares", "Fires lingering flares that stick into targets and illuminate dark zones."),
            PerkWeapon::new("Husk Fireball Launcher", 4, 8, 4000, "Plasma Cells", "Reverse-engineered Zed cannon firing charged plasma fireballs."),
        ],
        PerkType::Demolitions => vec![
            PerkWeapon::new("Pipe Bomb", 2, 1, 1500, "C4 Explosive", "Proximity landmine triggered by approaching Zeds."),
            PerkWeapon::new("M79 Grenade Launcher", 2, 4, 1250, "40mm HE Shells", "Single-shot break-action launching high-explosive rounds."),
            PerkWeapon::new("M4 203", 3, 8, 2500, "5.56mm + 40mm", "Assault rifle integrated with an underslung 40mm grenade launcher."),
            PerkWeapon::new("M32 Multiple Grenade Launcher", 3, 7, 2500, "40mm HE Shells", "6-shot revolving grenade launcher providing devastating burst barrages."),
            PerkWeapon::new("Seeker Six", 4, 8, 3500, "Micro Rockets", "Multi-tube rocket pod firing rapid volleys of guided micro-rockets."),
            PerkWeapon::new("L.A.W. Rocket Launcher", 4, 13, 3000, "Anti-Tank Rocket", "Shoulder-fired anti-tank rocket wiping entire groups in a massive fireball."),
        ],
    }
}

/// Identifiers for clickable buttons within the Perk Selection Page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerkUiButton {
    SelectPerkItem(usize),
    Tab(PerkDetailTab),
    ApplyPerk,
    CancelOrClose,
    WeaponScrollUp,
    WeaponScrollDown,
}

/// Killing Floor Perk Selection Page GUI Component.
///
/// Implements [`GUIComponent`] and manages the full perk progression and
/// selection workflow.
pub struct PerkSelectionPage {
    /// Unique component identifier
    pub id: String,
    /// Absolute bounding box on screen
    pub bounds: Rect,
    /// Layout box specification
    pub layout_box: LayoutBox,
    /// Interaction state
    pub state: ComponentState,
    /// Visibility toggle
    pub visible: bool,
    /// Enabled toggle
    pub enabled: bool,
    /// Currently active perk in-game (equipped at spawn)
    pub current_perk: PerkType,
    /// Perk currently selected / inspected in the UI
    pub selected_perk: PerkType,
    /// Pending perk change requested by player (takes effect next wave / spawn)
    pub pending_perk: Option<PerkType>,
    /// Progression state for each of the 7 perks
    pub progressions: HashMap<PerkType, PerkProgression>,
    /// Active detail sub-tab
    pub active_tab: PerkDetailTab,
    /// Currently hovered button / widget
    pub hovered_button: Option<PerkUiButton>,
    /// Scroll offset for weapons list
    pub weapon_scroll_offset: usize,
    /// Callback invoked when a perk preview is clicked
    #[allow(clippy::type_complexity)]
    pub on_perk_selected: Option<Box<dyn FnMut(PerkType) + Send + Sync>>,
    /// Callback invoked when the "SELECT PERK" action is applied
    #[allow(clippy::type_complexity)]
    pub on_change_applied: Option<Box<dyn FnMut(PerkType) + Send + Sync>>,
}

impl Debug for PerkSelectionPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PerkSelectionPage")
            .field("id", &self.id)
            .field("bounds", &self.bounds)
            .field("visible", &self.visible)
            .field("current_perk", &self.current_perk)
            .field("selected_perk", &self.selected_perk)
            .field("pending_perk", &self.pending_perk)
            .field("active_tab", &self.active_tab)
            .finish()
    }
}

impl Default for PerkSelectionPage {
    fn default() -> Self {
        Self::new()
    }
}

impl PerkSelectionPage {
    /// Creates a new PerkSelectionPage with default retail progression (Level 0).
    pub fn new() -> Self {
        let mut progressions = HashMap::new();
        for &perk in &ALL_PERKS {
            progressions.insert(perk, PerkProgression::new(perk, 0));
        }

        Self {
            id: "PerkSelectionPage".to_string(),
            bounds: Rect::new(50.0, 50.0, 924.0, 668.0),
            layout_box: LayoutBox::centered(924.0, 668.0),
            state: ComponentState::Normal,
            visible: true,
            enabled: true,
            current_perk: PerkType::FieldMedic,
            selected_perk: PerkType::FieldMedic,
            pending_perk: None,
            progressions,
            active_tab: PerkDetailTab::Bonuses,
            hovered_button: None,
            weapon_scroll_offset: 0,
            on_perk_selected: None,
            on_change_applied: None,
        }
    }

    /// Sets the currently active perk (the perk the player spawns as).
    pub fn set_current_perk(&mut self, perk: PerkType) {
        self.current_perk = perk;
        if self.pending_perk == Some(perk) {
            self.pending_perk = None;
        }
    }

    /// Selects a perk for inspection / viewing in the UI.
    pub fn select_perk(&mut self, perk: PerkType) {
        self.selected_perk = perk;
        self.weapon_scroll_offset = 0;
        if let Some(ref mut cb) = self.on_perk_selected {
            cb(perk);
        }
    }

    /// Confirms and commits the selected perk as the player's active choice.
    /// In Killing Floor 1, perk changes typically take effect at trader or next wave.
    pub fn apply_selected_perk(&mut self) {
        let selected = self.selected_perk;
        self.pending_perk = Some(selected);
        self.current_perk = selected;
        if let Some(ref mut cb) = self.on_change_applied {
            cb(selected);
        }
    }

    /// Attaches a callback hook when a perk is previewed / selected.
    pub fn set_on_perk_selected<F>(&mut self, callback: F)
    where
        F: FnMut(PerkType) + Send + Sync + 'static,
    {
        self.on_perk_selected = Some(Box::new(callback));
    }

    /// Attaches a callback hook when a perk change is confirmed / applied.
    pub fn set_on_change_applied<F>(&mut self, callback: F)
    where
        F: FnMut(PerkType) + Send + Sync + 'static,
    {
        self.on_change_applied = Some(Box::new(callback));
    }

    /// Gets the player's currently active perk.
    pub fn current_perk(&self) -> PerkType {
        self.current_perk
    }

    /// Gets the perk currently being inspected in the page.
    pub fn selected_perk(&self) -> PerkType {
        self.selected_perk
    }

    /// Checks if a perk change has been requested that differs from the active perk.
    pub fn is_perk_pending_change(&self) -> bool {
        self.pending_perk.is_some() && self.pending_perk != Some(self.current_perk)
    }

    /// Returns a reference to the progression record of a perk.
    pub fn get_progression(&self, perk: PerkType) -> &PerkProgression {
        self.progressions.get(&perk).expect("All 7 perks must exist in progression map")
    }

    /// Returns a mutable reference to the progression record of a perk.
    pub fn get_progression_mut(&mut self, perk: PerkType) -> &mut PerkProgression {
        self.progressions.get_mut(&perk).expect("All 7 perks must exist in progression map")
    }

    /// Replaces the progression record for a given perk.
    pub fn set_progression(&mut self, perk: PerkType, progression: PerkProgression) {
        self.progressions.insert(perk, progression);
    }

    // --- Layout Geometry Helpers ---

    /// Inner content boundary padding from the window edges.
    pub fn inner_bounds(&self) -> Rect {
        Rect::new(
            self.bounds.x + 8.0,
            self.bounds.y + 8.0,
            self.bounds.width - 16.0,
            self.bounds.height - 16.0,
        )
    }

    /// Title bar rectangle.
    pub fn title_bar_rect(&self) -> Rect {
        let inner = self.inner_bounds();
        Rect::new(inner.x, inner.y, inner.width, 36.0)
    }

    /// Left sidebar containing the 7 perk selection cards.
    pub fn perk_list_rect(&self) -> Rect {
        let inner = self.inner_bounds();
        let top = inner.y + 44.0;
        let height = inner.height - 44.0 - 52.0; // leave room for bottom action bar
        Rect::new(inner.x, top, 280.0, height)
    }

    /// Individual perk card rectangle for index 0..6.
    pub fn perk_card_rect(&self, index: usize) -> Rect {
        let list = self.perk_list_rect();
        let card_h = (list.height - (6.0 * 6.0)) / 7.0;
        let y = list.y + (index as f32) * (card_h + 6.0);
        Rect::new(list.x, y, list.width, card_h)
    }

    /// Right detail inspection panel.
    pub fn detail_panel_rect(&self) -> Rect {
        let inner = self.inner_bounds();
        let list = self.perk_list_rect();
        let left = list.x + list.width + 12.0;
        let width = inner.x + inner.width - left;
        Rect::new(left, list.y, width, list.height)
    }

    /// Header within the right detail panel (perk name, level star, overview).
    pub fn detail_header_rect(&self) -> Rect {
        let p = self.detail_panel_rect();
        Rect::new(p.x, p.y, p.width, 68.0)
    }

    /// Progression bars container inside the right detail panel.
    pub fn progress_container_rect(&self) -> Rect {
        let p = self.detail_panel_rect();
        Rect::new(p.x, p.y + 74.0, p.width, 82.0)
    }

    /// Tab bar rectangle for sub-tab selection.
    pub fn tab_bar_rect(&self) -> Rect {
        let p = self.detail_panel_rect();
        Rect::new(p.x, p.y + 162.0, p.width, 32.0)
    }

    /// Individual tab button rectangle.
    pub fn tab_button_rect(&self, tab: PerkDetailTab) -> Rect {
        let bar = self.tab_bar_rect();
        let tab_w = (bar.width - 8.0) / 3.0;
        let x = match tab {
            PerkDetailTab::Bonuses => bar.x,
            PerkDetailTab::Arsenal => bar.x + tab_w + 4.0,
            PerkDetailTab::Requirements => bar.x + (tab_w * 2.0) + 8.0,
        };
        Rect::new(x, bar.y, tab_w, bar.height)
    }

    /// Content viewport for the active tab.
    pub fn tab_content_rect(&self) -> Rect {
        let p = self.detail_panel_rect();
        let top = p.y + 198.0;
        let h = p.y + p.height - top;
        Rect::new(p.x, top, p.width, h)
    }

    /// Bottom action bar spanning the full inner width.
    pub fn action_bar_rect(&self) -> Rect {
        let inner = self.inner_bounds();
        let h = 44.0;
        let y = inner.y + inner.height - h;
        Rect::new(inner.x, y, inner.width, h)
    }

    /// "SELECT / APPLY PERK" button rectangle.
    pub fn apply_button_rect(&self) -> Rect {
        let bar = self.action_bar_rect();
        let btn_w = 180.0;
        let x = bar.x + bar.width - btn_w - 110.0;
        Rect::new(x, bar.y + 4.0, btn_w, bar.height - 8.0)
    }

    /// "CLOSE" button rectangle.
    pub fn close_button_rect(&self) -> Rect {
        let bar = self.action_bar_rect();
        let btn_w = 95.0;
        let x = bar.x + bar.width - btn_w;
        Rect::new(x, bar.y + 4.0, btn_w, bar.height - 8.0)
    }

    // --- Mouse & Event Handling ---

    fn hit_test(&self, x: f32, y: f32) -> Option<PerkUiButton> {
        // Test perk cards
        for i in 0..ALL_PERKS.len() {
            if self.perk_card_rect(i).contains(x, y) {
                return Some(PerkUiButton::SelectPerkItem(i));
            }
        }

        // Test tabs
        if self.tab_button_rect(PerkDetailTab::Bonuses).contains(x, y) {
            return Some(PerkUiButton::Tab(PerkDetailTab::Bonuses));
        }
        if self.tab_button_rect(PerkDetailTab::Arsenal).contains(x, y) {
            return Some(PerkUiButton::Tab(PerkDetailTab::Arsenal));
        }
        if self.tab_button_rect(PerkDetailTab::Requirements).contains(x, y) {
            return Some(PerkUiButton::Tab(PerkDetailTab::Requirements));
        }

        // Test action buttons
        if self.apply_button_rect().contains(x, y) {
            return Some(PerkUiButton::ApplyPerk);
        }
        if self.close_button_rect().contains(x, y) {
            return Some(PerkUiButton::CancelOrClose);
        }

        None
    }

    fn handle_click(&mut self, x: f32, y: f32) -> bool {
        if let Some(btn) = self.hit_test(x, y) {
            match btn {
                PerkUiButton::SelectPerkItem(idx) => {
                    if idx < ALL_PERKS.len() {
                        self.select_perk(ALL_PERKS[idx]);
                    }
                    true
                }
                PerkUiButton::Tab(tab) => {
                    self.active_tab = tab;
                    self.weapon_scroll_offset = 0;
                    true
                }
                PerkUiButton::ApplyPerk => {
                    self.apply_selected_perk();
                    true
                }
                PerkUiButton::CancelOrClose => {
                    self.visible = false;
                    true
                }
                _ => false,
            }
        } else {
            false
        }
    }

    // --- Canvas Rendering ---

    /// Primary render dispatch drawing all widgets onto the Canvas.
    pub fn render_canvas(&self, canvas: &mut Canvas) {
        if !self.visible {
            return;
        }

        // 1. Main outer window frame (Dark industrial steel / grunge)
        canvas.draw_filled_rect(self.bounds, [0.08, 0.08, 0.10, 0.96]);
        canvas.draw_border(self.bounds, 2.0, [0.45, 0.18, 0.16, 1.0]);

        // 2. Title bar
        let title_rect = self.title_bar_rect();
        canvas.draw_filled_rect(title_rect, [0.14, 0.08, 0.08, 0.95]);
        canvas.draw_border(title_rect, 1.0, [0.55, 0.15, 0.15, 1.0]);
        canvas.draw_text(
            "KILLING FLOOR — PERK SELECTION & PROGRESSION",
            [title_rect.x + 12.0, title_rect.y + 9.0],
            16.0,
            [0.95, 0.92, 0.88, 1.0],
            TextAlignment::Left,
        );

        // Active perk indicator in title bar
        let active_title = format!("EQUIPPED: {} (LV. {})", perk_tag(self.current_perk), self.get_progression(self.current_perk).level);
        canvas.draw_text(
            active_title,
            [title_rect.x + title_rect.width - 12.0, title_rect.y + 10.0],
            13.0,
            perk_theme_color(self.current_perk),
            TextAlignment::Right,
        );

        // 3. Left sidebar: 7 Perk selection cards
        self.render_perk_list(canvas);

        // 4. Right side: Perk inspection detail panel
        self.render_detail_panel(canvas);

        // 5. Bottom action bar
        self.render_action_bar(canvas);
    }

    fn render_perk_list(&self, canvas: &mut Canvas) {
        for (i, &perk) in ALL_PERKS.iter().enumerate() {
            let card_rect = self.perk_card_rect(i);
            let is_selected = perk == self.selected_perk;
            let is_current = perk == self.current_perk;
            let is_hovered = self.hovered_button == Some(PerkUiButton::SelectPerkItem(i));
            let prog = self.get_progression(perk);

            // Background fill
            let bg_color = if is_selected {
                [0.26, 0.12, 0.12, 0.95]
            } else if is_hovered {
                [0.20, 0.14, 0.14, 0.90]
            } else {
                [0.12, 0.12, 0.14, 0.85]
            };
            canvas.draw_filled_rect(card_rect, bg_color);

            // Border
            let border_color = if is_selected {
                perk_theme_color(perk)
            } else if is_hovered {
                [0.65, 0.35, 0.30, 1.0]
            } else {
                [0.25, 0.25, 0.28, 0.70]
            };
            canvas.draw_border(card_rect, if is_selected { 2.0 } else { 1.0 }, border_color);

            // Left color accent stripe
            let stripe_rect = Rect::new(card_rect.x, card_rect.y, 6.0, card_rect.height);
            canvas.draw_filled_rect(stripe_rect, perk_theme_color(perk));

            // Perk full name & level
            let name_x = card_rect.x + 14.0;
            let name_y = card_rect.y + 8.0;
            canvas.draw_text(
                perk_title(perk),
                [name_x, name_y],
                14.0,
                if is_selected { [1.0, 1.0, 1.0, 1.0] } else { [0.85, 0.85, 0.88, 1.0] },
                TextAlignment::Left,
            );

            // Level badge / text
            let lvl_text = if prog.is_max_level() {
                "LV. 6 [MAX]".to_string()
            } else {
                format!("LEVEL {}", prog.level)
            };
            let lvl_color = if prog.is_max_level() {
                [0.95, 0.80, 0.20, 1.0] // Gold
            } else {
                [0.75, 0.75, 0.80, 1.0]
            };
            canvas.draw_text(
                lvl_text,
                [card_rect.x + card_rect.width - 12.0, name_y + 1.0],
                12.0,
                lvl_color,
                TextAlignment::Right,
            );

            // Mini progress bar on card
            let bar_y = card_rect.y + card_rect.height - 14.0;
            let bar_w = card_rect.width - 24.0;
            let bar_bg = Rect::new(name_x, bar_y, bar_w, 6.0);
            canvas.draw_filled_rect(bar_bg, [0.05, 0.05, 0.06, 1.0]);

            let fill_ratio = prog.overall_ratio();
            if fill_ratio > 0.0 {
                let fill_w = bar_w * fill_ratio;
                canvas.draw_filled_rect(Rect::new(name_x, bar_y, fill_w, 6.0), perk_theme_color(perk));
            }

            // Equipped tag if this is the currently active perk
            if is_current {
                let tag_rect = Rect::new(card_rect.x + card_rect.width - 70.0, card_rect.y + 24.0, 58.0, 14.0);
                canvas.draw_filled_rect(tag_rect, [0.15, 0.45, 0.20, 0.85]);
                canvas.draw_text("EQUIPPED", [tag_rect.x + 4.0, tag_rect.y + 2.0], 9.0, [1.0, 1.0, 1.0, 1.0], TextAlignment::Left);
            }
        }
    }

    fn render_detail_panel(&self, canvas: &mut Canvas) {
        let p_rect = self.detail_panel_rect();
        canvas.draw_filled_rect(p_rect, [0.10, 0.10, 0.12, 0.90]);
        canvas.draw_border(p_rect, 1.0, [0.28, 0.28, 0.32, 0.80]);

        let perk = self.selected_perk;
        let prog = self.get_progression(perk);

        // Header: Name, level, role summary
        let hdr = self.detail_header_rect();
        canvas.draw_filled_rect(hdr, [0.15, 0.12, 0.12, 0.75]);
        canvas.draw_border(hdr, 1.0, [0.35, 0.22, 0.22, 0.70]);

        canvas.draw_text(
            perk_title(perk),
            [hdr.x + 14.0, hdr.y + 10.0],
            22.0,
            perk_theme_color(perk),
            TextAlignment::Left,
        );

        let lvl_banner = if prog.is_max_level() {
            "PERK LEVEL: 6 [MASTER RANK]".to_string()
        } else {
            format!("PERK LEVEL: {} / 6", prog.level)
        };
        canvas.draw_text(
            lvl_banner,
            [hdr.x + hdr.width - 14.0, hdr.y + 14.0],
            14.0,
            [0.95, 0.85, 0.30, 1.0],
            TextAlignment::Right,
        );

        canvas.draw_text(
            perk_role_summary(perk),
            [hdr.x + 14.0, hdr.y + 40.0],
            11.5,
            [0.78, 0.78, 0.82, 1.0],
            TextAlignment::Left,
        );

        // Progression Bars Section
        self.render_progress_bars(canvas, prog);

        // Sub-Tabs bar
        self.render_tab_bar(canvas);

        // Sub-Tab contents
        let content_rect = self.tab_content_rect();
        match self.active_tab {
            PerkDetailTab::Bonuses => self.render_bonuses_tab(canvas, content_rect, perk, prog),
            PerkDetailTab::Arsenal => self.render_arsenal_tab(canvas, content_rect, perk, prog),
            PerkDetailTab::Requirements => self.render_requirements_tab(canvas, content_rect, perk, prog),
        }
    }

    fn render_progress_bars(&self, canvas: &mut Canvas, prog: &PerkProgression) {
        let prog_rect = self.progress_container_rect();
        canvas.draw_filled_rect(prog_rect, [0.08, 0.08, 0.10, 0.85]);
        canvas.draw_border(prog_rect, 1.0, [0.22, 0.22, 0.26, 0.80]);

        let has_secondary = prog.secondary_name().is_some();
        let bar_h = if has_secondary { 12.0 } else { 18.0 };
        let mut curr_y = prog_rect.y + 8.0;

        // Primary requirement
        let prim_ratio = prog.primary_ratio();
        let prim_label = format!("{}: {} / {} ({}%)",
            prog.primary_name(),
            format_number(prog.primary_current),
            format_number(prog.primary_target()),
            (prim_ratio * 100.0) as u32
        );
        canvas.draw_text(prim_label, [prog_rect.x + 12.0, curr_y], 11.0, [0.90, 0.90, 0.92, 1.0], TextAlignment::Left);

        let bar_w = prog_rect.width - 24.0;
        let bar_bg = Rect::new(prog_rect.x + 12.0, curr_y + 16.0, bar_w, bar_h);
        canvas.draw_filled_rect(bar_bg, [0.04, 0.04, 0.05, 1.0]);
        canvas.draw_border(bar_bg, 1.0, [0.25, 0.25, 0.30, 0.80]);

        let fill_w = bar_w * prim_ratio;
        if fill_w > 0.0 {
            canvas.draw_filled_rect(Rect::new(bar_bg.x, bar_bg.y, fill_w, bar_h), perk_theme_color(prog.perk));
        }

        // Secondary requirement (if any)
        if let Some(sec_name) = prog.secondary_name() {
            curr_y += 34.0;
            let sec_cur = prog.secondary_current.unwrap_or(0);
            let sec_target = prog.secondary_target().unwrap_or(1);
            let sec_ratio = prog.secondary_ratio().unwrap_or(0.0);

            let sec_label = format!("{}: {} / {} ({}%)",
                sec_name,
                format_number(sec_cur),
                format_number(sec_target),
                (sec_ratio * 100.0) as u32
            );
            canvas.draw_text(sec_label, [prog_rect.x + 12.0, curr_y], 11.0, [0.90, 0.90, 0.92, 1.0], TextAlignment::Left);

            let sec_bg = Rect::new(prog_rect.x + 12.0, curr_y + 16.0, bar_w, bar_h);
            canvas.draw_filled_rect(sec_bg, [0.04, 0.04, 0.05, 1.0]);
            canvas.draw_border(sec_bg, 1.0, [0.25, 0.25, 0.30, 0.80]);

            let sec_fill_w = bar_w * sec_ratio;
            if sec_fill_w > 0.0 {
                canvas.draw_filled_rect(Rect::new(sec_bg.x, sec_bg.y, sec_fill_w, bar_h), [0.30, 0.65, 0.85, 1.0]);
            }
        }
    }

    fn render_tab_bar(&self, canvas: &mut Canvas) {
        for &tab in &[PerkDetailTab::Bonuses, PerkDetailTab::Arsenal, PerkDetailTab::Requirements] {
            let r = self.tab_button_rect(tab);
            let is_active = self.active_tab == tab;
            let is_hovered = self.hovered_button == Some(PerkUiButton::Tab(tab));

            let bg = if is_active {
                [0.25, 0.15, 0.15, 1.0]
            } else if is_hovered {
                [0.18, 0.18, 0.22, 0.95]
            } else {
                [0.12, 0.12, 0.14, 0.90]
            };
            canvas.draw_filled_rect(r, bg);

            let border_c = if is_active {
                [0.75, 0.25, 0.25, 1.0]
            } else {
                [0.30, 0.30, 0.35, 0.70]
            };
            canvas.draw_border(r, 1.0, border_c);

            let text_c = if is_active {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.75, 0.75, 0.80, 1.0]
            };
            let (cx, cy) = r.center();
            canvas.draw_text(tab.display_name(), [cx, cy - 6.0], 12.0, text_c, TextAlignment::Center);
        }
    }

    fn render_bonuses_tab(&self, canvas: &mut Canvas, bounds: Rect, perk: PerkType, prog: &PerkProgression) {
        canvas.draw_filled_rect(bounds, [0.07, 0.07, 0.09, 0.95]);
        canvas.draw_border(bounds, 1.0, [0.20, 0.20, 0.25, 0.70]);

        let bonuses = get_perk_bonuses(perk, prog.level);
        let row_h = (bounds.height - 16.0) / 6.0;

        for (i, bonus) in bonuses.iter().enumerate().take(6) {
            let y = bounds.y + 8.0 + (i as f32) * row_h;
            let row_r = Rect::new(bounds.x + 8.0, y, bounds.width - 16.0, row_h - 4.0);

            let bg = if i % 2 == 0 {
                [0.12, 0.12, 0.15, 0.60]
            } else {
                [0.09, 0.09, 0.11, 0.60]
            };
            canvas.draw_filled_rect(row_r, bg);

            // Title
            canvas.draw_text(bonus.title, [row_r.x + 10.0, row_r.y + 6.0], 13.0, [0.95, 0.95, 0.98, 1.0], TextAlignment::Left);

            // Value badge
            canvas.draw_text(&bonus.value, [row_r.x + row_r.width - 10.0, row_r.y + 6.0], 13.0, perk_theme_color(perk), TextAlignment::Right);

            // Description
            canvas.draw_text(bonus.description, [row_r.x + 10.0, row_r.y + 24.0], 10.5, [0.65, 0.65, 0.70, 1.0], TextAlignment::Left);
        }
    }

    fn render_arsenal_tab(&self, canvas: &mut Canvas, bounds: Rect, perk: PerkType, prog: &PerkProgression) {
        canvas.draw_filled_rect(bounds, [0.07, 0.07, 0.09, 0.95]);
        canvas.draw_border(bounds, 1.0, [0.20, 0.20, 0.25, 0.70]);

        let weapons = get_perk_weapons(perk);
        let discount_pct = (prog.level as u32) * 10; // rough baseline discount for display
        let max_visible = 5;
        let visible_weapons = weapons.iter().skip(self.weapon_scroll_offset).take(max_visible);
        let row_h = (bounds.height - 16.0) / 5.0;

        for (i, weapon) in visible_weapons.enumerate() {
            let y = bounds.y + 8.0 + (i as f32) * row_h;
            let row_r = Rect::new(bounds.x + 8.0, y, bounds.width - 16.0, row_h - 4.0);

            let bg = if i % 2 == 0 {
                [0.12, 0.12, 0.15, 0.60]
            } else {
                [0.09, 0.09, 0.11, 0.60]
            };
            canvas.draw_filled_rect(row_r, bg);

            // Weapon Name and Tier
            let name_str = format!("{} (Tier {})", weapon.name, weapon.tier);
            canvas.draw_text(name_str, [row_r.x + 10.0, row_r.y + 6.0], 13.0, [0.95, 0.95, 0.98, 1.0], TextAlignment::Left);

            // Weight and Ammo
            let specs_str = format!("Weight: {} kg | Ammo: {}", weapon.weight, weapon.ammo_type);
            canvas.draw_text(specs_str, [row_r.x + 10.0, row_r.y + 24.0], 10.5, [0.65, 0.65, 0.70, 1.0], TextAlignment::Left);

            // Price / Discounted Price
            let disc_price = weapon.discounted_price(discount_pct);
            let price_str = if weapon.base_price == 0 {
                "FREE".to_string()
            } else if disc_price < weapon.base_price {
                format!("£{} (Base: £{})", disc_price, weapon.base_price)
            } else {
                format!("£{}", weapon.base_price)
            };
            canvas.draw_text(price_str, [row_r.x + row_r.width - 10.0, row_r.y + 12.0], 13.0, [0.95, 0.85, 0.30, 1.0], TextAlignment::Right);
        }
    }

    fn render_requirements_tab(&self, canvas: &mut Canvas, bounds: Rect, perk: PerkType, prog: &PerkProgression) {
        canvas.draw_filled_rect(bounds, [0.07, 0.07, 0.09, 0.95]);
        canvas.draw_border(bounds, 1.0, [0.20, 0.20, 0.25, 0.70]);

        let (prim_thresh, sec_thresh) = perk_requirements::thresholds(perk);
        let row_h = (bounds.height - 16.0) / 7.0;

        for lvl in 0..=6 {
            let y = bounds.y + 8.0 + (lvl as f32) * row_h;
            let row_r = Rect::new(bounds.x + 8.0, y, bounds.width - 16.0, row_h - 4.0);

            let is_unlocked = prog.level >= (lvl as u8);
            let bg = if is_unlocked {
                [0.10, 0.18, 0.12, 0.70]
            } else {
                [0.10, 0.10, 0.12, 0.50]
            };
            canvas.draw_filled_rect(row_r, bg);

            let status_marker = if is_unlocked { "[UNLOCKED]" } else { "[LOCKED]" };
            let marker_c = if is_unlocked { [0.35, 0.85, 0.35, 1.0] } else { [0.60, 0.60, 0.65, 1.0] };

            let lvl_label = format!("Level {} {}", lvl, status_marker);
            canvas.draw_text(lvl_label, [row_r.x + 10.0, row_r.y + 6.0], 12.0, marker_c, TextAlignment::Left);

            let prim_val = format_number(prim_thresh[lvl]);
            let req_summary = match sec_thresh {
                Some(thresh) => format!("{}: {} | {}: {}", prog.primary_name(), prim_val, prog.secondary_name().unwrap_or(""), format_number(thresh[lvl])),
                None => format!("{}: {}", prog.primary_name(), prim_val),
            };
            canvas.draw_text(req_summary, [row_r.x + row_r.width - 10.0, row_r.y + 6.0], 11.0, [0.85, 0.85, 0.90, 1.0], TextAlignment::Right);
        }
    }

    fn render_action_bar(&self, canvas: &mut Canvas) {
        let bar = self.action_bar_rect();
        canvas.draw_filled_rect(bar, [0.12, 0.12, 0.15, 0.95]);
        canvas.draw_border(bar, 1.0, [0.30, 0.30, 0.35, 0.80]);

        // Status banner
        let is_current = self.selected_perk == self.current_perk;
        let status_text = if is_current {
            format!("ACTIVE PERK: {} (Takes effect immediately)", perk_title(self.current_perk))
        } else {
            format!("PENDING CHANGE: {} (Will take effect at Trader / next spawn)", perk_title(self.selected_perk))
        };
        let status_color = if is_current {
            [0.35, 0.85, 0.45, 1.0]
        } else {
            [0.95, 0.75, 0.20, 1.0]
        };
        canvas.draw_text(status_text, [bar.x + 14.0, bar.y + 14.0], 12.0, status_color, TextAlignment::Left);

        // "SELECT / APPLY PERK" button
        let apply_btn = self.apply_button_rect();
        let is_apply_hovered = self.hovered_button == Some(PerkUiButton::ApplyPerk);
        let apply_bg = if is_current {
            [0.20, 0.20, 0.25, 0.80]
        } else if is_apply_hovered {
            [0.75, 0.22, 0.22, 1.0]
        } else {
            [0.55, 0.16, 0.16, 1.0]
        };
        canvas.draw_filled_rect(apply_btn, apply_bg);
        canvas.draw_border(apply_btn, 1.0, if is_current { [0.35, 0.35, 0.40, 1.0] } else { [0.85, 0.35, 0.35, 1.0] });

        let apply_label = if is_current { "CURRENT PERK" } else { "SELECT PERK" };
        let (ax, ay) = apply_btn.center();
        canvas.draw_text(apply_label, [ax, ay - 6.0], 13.0, [1.0, 1.0, 1.0, 1.0], TextAlignment::Center);

        // "CLOSE" button
        let close_btn = self.close_button_rect();
        let is_close_hovered = self.hovered_button == Some(PerkUiButton::CancelOrClose);
        let close_bg = if is_close_hovered {
            [0.35, 0.35, 0.40, 1.0]
        } else {
            [0.22, 0.22, 0.25, 0.90]
        };
        canvas.draw_filled_rect(close_btn, close_bg);
        canvas.draw_border(close_btn, 1.0, [0.45, 0.45, 0.50, 1.0]);
        let (cx, cy) = close_btn.center();
        canvas.draw_text("CLOSE", [cx, cy - 6.0], 12.5, [0.90, 0.90, 0.95, 1.0], TextAlignment::Center);
    }
}

// --- Implementation of GUIComponent Trait ---

impl GUIComponent for PerkSelectionPage {
    fn id(&self) -> &str {
        &self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.layout_box = LayoutBox::absolute(bounds.x, bounds.y, bounds.width, bounds.height);
    }

    fn layout_box(&self) -> LayoutBox {
        self.layout_box
    }

    fn state(&self) -> ComponentState {
        self.state
    }

    fn set_state(&mut self, state: ComponentState) {
        self.state = state;
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn handle_mouse_event(&mut self, event: &MouseEvent) -> bool {
        if !self.visible || !self.enabled {
            return false;
        }

        match *event {
            MouseEvent::Down { button: MouseButton::Left, x, y } => {
                self.handle_click(x, y)
            }
            MouseEvent::Move { x, y } => {
                if self.bounds.contains(x, y) {
                    self.hovered_button = self.hit_test(x, y);
                    self.state = ComponentState::Hovered;
                    true
                } else {
                    self.hovered_button = None;
                    if self.state == ComponentState::Hovered {
                        self.state = ComponentState::Normal;
                    }
                    false
                }
            }
            MouseEvent::Scroll { delta, x, y }
                if self.tab_content_rect().contains(x, y) && self.active_tab == PerkDetailTab::Arsenal => {
                    let total_weapons = get_perk_weapons(self.selected_perk).len();
                    if delta < 0.0 {
                        if self.weapon_scroll_offset + 5 < total_weapons {
                            self.weapon_scroll_offset += 1;
                        }
                    } else if delta > 0.0 && self.weapon_scroll_offset > 0 {
                        self.weapon_scroll_offset -= 1;
                    }
                    true
                }
            _ => false,
        }
    }

    fn render(&self, canvas: &mut Canvas) {
        if !self.visible {
            return;
        }
        self.render_canvas(canvas);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Helper function to format integers with thousands commas (e.g. 5500000 -> "5,500,000").
pub fn format_number(val: u64) -> String {
    let s = val.to_string();
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();

    for (i, &ch) in chars.iter().enumerate() {
        result.push(ch);
        let remaining = len - 1 - i;
        if remaining > 0 && remaining.is_multiple_of(3) {
            result.push(',');
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_7_perks_enumerated_and_titles() {
        assert_eq!(ALL_PERKS.len(), 7);
        assert_eq!(ALL_PERKS[0], PerkType::FieldMedic);
        assert_eq!(ALL_PERKS[1], PerkType::SupportSpecialist);
        assert_eq!(ALL_PERKS[2], PerkType::Sharpshooter);
        assert_eq!(ALL_PERKS[3], PerkType::Commando);
        assert_eq!(ALL_PERKS[4], PerkType::Berserker);
        assert_eq!(ALL_PERKS[5], PerkType::Firebug);
        assert_eq!(ALL_PERKS[6], PerkType::Demolitions);

        for &perk in &ALL_PERKS {
            assert!(!perk_title(perk).is_empty());
            assert!(!perk_tag(perk).is_empty());
            assert!(!perk_role_summary(perk).is_empty());
        }
    }

    #[test]
    fn test_perk_selection_page_initial_state() {
        let page = PerkSelectionPage::new();
        assert_eq!(page.id(), "PerkSelectionPage");
        assert!(page.is_visible());
        assert!(page.is_enabled());
        assert_eq!(page.current_perk(), PerkType::FieldMedic);
        assert_eq!(page.selected_perk(), PerkType::FieldMedic);
        assert!(!page.is_perk_pending_change());
        assert_eq!(page.progressions.len(), 7);

        for &perk in &ALL_PERKS {
            let prog = page.get_progression(perk);
            assert_eq!(prog.level, 0);
            assert!(!prog.is_max_level());
        }
    }

    #[test]
    fn test_select_and_set_current_perk() {
        let mut page = PerkSelectionPage::new();
        page.select_perk(PerkType::Sharpshooter);
        assert_eq!(page.selected_perk(), PerkType::Sharpshooter);
        assert_eq!(page.current_perk(), PerkType::FieldMedic);

        page.set_current_perk(PerkType::Commando);
        assert_eq!(page.current_perk(), PerkType::Commando);
        assert_eq!(page.selected_perk(), PerkType::Sharpshooter);
    }

    #[test]
    fn test_apply_selected_perk_and_callbacks() {
        use std::sync::{Arc, Mutex};

        let mut page = PerkSelectionPage::new();
        let selected_callback_called = Arc::new(Mutex::new(None));
        let change_applied_called = Arc::new(Mutex::new(None));

        let sel_ref = Arc::clone(&selected_callback_called);
        page.set_on_perk_selected(move |p| {
            *sel_ref.lock().unwrap() = Some(p);
        });

        page.select_perk(PerkType::Berserker);
        assert_eq!(page.selected_perk(), PerkType::Berserker);
        assert_eq!(*selected_callback_called.lock().unwrap(), Some(PerkType::Berserker));

        let app_ref = Arc::clone(&change_applied_called);
        page.set_on_change_applied(move |p| {
            *app_ref.lock().unwrap() = Some(p);
        });

        page.apply_selected_perk();
        assert_eq!(page.current_perk(), PerkType::Berserker);
        assert_eq!(page.pending_perk, Some(PerkType::Berserker));
        assert_eq!(*change_applied_called.lock().unwrap(), Some(PerkType::Berserker));
    }

    #[test]
    fn test_perk_progression_ratios_and_level_up() {
        let mut prog = PerkProgression::new(PerkType::FieldMedic, 0);
        assert_eq!(prog.primary_current, 0);
        assert_eq!(prog.primary_target(), 200);
        assert_eq!(prog.primary_ratio(), 0.0);

        // Add 100 heal -> 50% ratio
        prog.add_progress(100, None);
        assert_eq!(prog.level, 0);
        assert!((prog.primary_ratio() - 0.50).abs() < 1e-4);

        // Add another 100 heal -> total 200 -> level up to Level 1!
        let leveled = prog.add_progress(100, None);
        assert!(leveled);
        assert_eq!(prog.level, 1);
        assert_eq!(prog.primary_target(), 750);
        assert_eq!(prog.primary_ratio(), 0.0); // At exact threshold for Lv 1
    }

    #[test]
    fn test_dual_requirement_progression_support() {
        let mut prog = PerkProgression::new(PerkType::SupportSpecialist, 0);
        assert!(prog.secondary_name().is_some());
        assert_eq!(prog.primary_name(), "Shotgun Damage");
        assert_eq!(prog.secondary_name(), Some("Door Welding Points"));

        // Meet primary threshold (25,000) but not secondary (2,000)
        prog.add_progress(30_000, Some(500));
        assert_eq!(prog.level, 0); // Should not level up yet

        // Now satisfy secondary threshold
        prog.add_progress(0, Some(1_500));
        assert_eq!(prog.level, 1); // Both satisfied!
    }

    #[test]
    fn test_max_level_handling() {
        let mut prog = PerkProgression::new(PerkType::Demolitions, 6);
        assert!(prog.is_max_level());
        assert_eq!(prog.primary_ratio(), 1.0);
        assert_eq!(prog.overall_ratio(), 1.0);

        let leveled = prog.add_progress(1_000_000, None);
        assert!(!leveled);
        assert_eq!(prog.level, 6);
    }

    #[test]
    fn test_stat_bonuses_all_7_perks_levels_0_and_6() {
        for &perk in &ALL_PERKS {
            let bonuses_l0 = get_perk_bonuses(perk, 0);
            assert!(!bonuses_l0.is_empty());
            for b in &bonuses_l0 {
                assert!(!b.title.is_empty());
                assert!(!b.value.is_empty());
            }

            let bonuses_l6 = get_perk_bonuses(perk, 6);
            assert!(!bonuses_l6.is_empty());
            for b in &bonuses_l6 {
                assert!(!b.title.is_empty());
                assert!(!b.value.is_empty());
            }
        }
    }

    #[test]
    fn test_loadout_weapons_and_discounts() {
        for &perk in &ALL_PERKS {
            let weapons = get_perk_weapons(perk);
            assert!(!weapons.is_empty(), "Perk {:?} must have weapons", perk);
            for w in weapons {
                assert!(!w.name.is_empty());
                let discounted = w.discounted_price(50);
                assert!(discounted <= w.base_price);
            }
        }
    }

    #[test]
    fn test_mouse_events_and_interaction() {
        let mut page = PerkSelectionPage::new();
        page.set_bounds(Rect::new(0.0, 0.0, 1000.0, 700.0));

        // Test hover
        let inside_card = page.perk_card_rect(3).center();
        let consumed = page.handle_mouse_event(&MouseEvent::Move { x: inside_card.0, y: inside_card.1 });
        assert!(consumed);
        assert_eq!(page.hovered_button, Some(PerkUiButton::SelectPerkItem(3)));

        // Test click on Commando card (index 3)
        let clicked = page.handle_mouse_event(&MouseEvent::Down {
            button: MouseButton::Left,
            x: inside_card.0,
            y: inside_card.1,
        });
        assert!(clicked);
        assert_eq!(page.selected_perk(), PerkType::Commando);

        // Test tab click
        let tab_rect = page.tab_button_rect(PerkDetailTab::Arsenal);
        let (tx, ty) = tab_rect.center();
        let tab_clicked = page.handle_mouse_event(&MouseEvent::Down {
            button: MouseButton::Left,
            x: tx,
            y: ty,
        });
        assert!(tab_clicked);
        assert_eq!(page.active_tab, PerkDetailTab::Arsenal);

        // Test Apply button click
        let apply_rect = page.apply_button_rect();
        let (ax, ay) = apply_rect.center();
        let apply_clicked = page.handle_mouse_event(&MouseEvent::Down {
            button: MouseButton::Left,
            x: ax,
            y: ay,
        });
        assert!(apply_clicked);
        assert_eq!(page.current_perk(), PerkType::Commando);
    }

    #[test]
    fn test_canvas_rendering_generates_quads_and_text() {
        let mut page = PerkSelectionPage::new();
        page.set_bounds(Rect::new(0.0, 0.0, 1024.0, 768.0));
        let mut canvas = Canvas::new(1024.0, 768.0);

        page.render(&mut canvas);

        assert!(!canvas.commands.is_empty(), "Rendering must emit draw commands");
        assert!(!canvas.vertices.is_empty(), "Rendering must produce UI vertices");
        assert!(!canvas.indices.is_empty(), "Rendering must produce UI indices");
    }

    #[test]
    fn test_format_number_helper() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(750), "750");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(25000), "25,000");
        assert_eq!(format_number(5500000), "5,500,000");
    }

    #[test]
    fn test_downcasting_as_any() {
        let mut page = PerkSelectionPage::new();
        let comp: &mut dyn GUIComponent = &mut page;
        assert!(comp.as_any().downcast_ref::<PerkSelectionPage>().is_some());
        assert!(comp.as_any_mut().downcast_mut::<PerkSelectionPage>().is_some());
    }
}
