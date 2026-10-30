//! Killing Floor Trader Shop GUI Component
//!
//! Provides the retail Killing Floor trader pod interface, including:
//! - Ammo purchase per-weapon and global "Fill All Ammo" resupply
//! - Kevlar Combat Armor purchasing with pro-rated costs and Medic perk discounts
//! - Weapon buy catalog with perk filtering and dynamic perk discount calculations (10% to 70%)
//! - Player inventory loadout management with resale value calculations (75% base return)
//! - Weight block capacity meter (default 15/15 kg, expandable with Support Specialist perk)
//! - Cash / Dosh balance display and real-time transaction validation
//! - Full integration with OpenKF 2D UI system implementing [`crate::gui::GUIComponent`]

use std::any::Any;
use std::fmt::Debug;

use crate::gui::{
    Canvas, ComponentState, GUIComponent, LayoutBox, MouseButton, MouseEvent, Rect,
    TextAlignment,
};
use crate::hud::PerkType;

/// Category filter tabs for the Trader Shop catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TraderTab {
    #[default]
    AllWeapons,
    PerkWeapons,
    Berserker,
    Commando,
    SupportSpecialist,
    FieldMedic,
    Demolitions,
    Firebug,
    Sharpshooter,
}

impl TraderTab {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::AllWeapons => "ALL WEAPONS",
            Self::PerkWeapons => "PERK SPECIALS",
            Self::Berserker => "BERSERKER",
            Self::Commando => "COMMANDO",
            Self::SupportSpecialist => "SUPPORT",
            Self::FieldMedic => "MEDIC",
            Self::Demolitions => "DEMO",
            Self::Firebug => "FIREBUG",
            Self::Sharpshooter => "SHARPSHOOTER",
        }
    }

    pub fn matches_perk(&self, perk: PerkType) -> bool {
        match self {
            Self::AllWeapons => true,
            Self::PerkWeapons => true, // Evaluated dynamically against player perk
            Self::Berserker => perk == PerkType::Berserker,
            Self::Commando => perk == PerkType::Commando,
            Self::SupportSpecialist => perk == PerkType::SupportSpecialist,
            Self::FieldMedic => perk == PerkType::FieldMedic,
            Self::Demolitions => perk == PerkType::Demolitions,
            Self::Firebug => perk == PerkType::Firebug,
            Self::Sharpshooter => perk == PerkType::Sharpshooter,
        }
    }
}

/// Errors that can occur during a Trader transaction.
#[derive(Clone, Debug, PartialEq)]
pub enum TraderError {
    WeaponNotFound(String),
    InsufficientFunds { needed: u32, available: u32 },
    WeightLimitExceeded { weapon_weight: f32, current: f32, max: f32 },
    AlreadyOwned(String),
    CannotSellWeapon(String),
    AmmoAlreadyFull(String),
    ArmorAlreadyFull,
    WeaponNotInInventory(String),
}

impl std::fmt::Display for TraderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WeaponNotFound(id) => write!(f, "Weapon not found in catalog: {}", id),
            Self::InsufficientFunds { needed, available } => {
                write!(f, "Insufficient funds: requires £{}, available £{}", needed, available)
            }
            Self::WeightLimitExceeded { weapon_weight, current, max } => {
                write!(
                    f,
                    "Carrying capacity exceeded! Weapon is {:.1}kg (Current: {:.1}/{:.1}kg)",
                    weapon_weight, current, max
                )
            }
            Self::AlreadyOwned(id) => write!(f, "Weapon already owned: {}", id),
            Self::CannotSellWeapon(id) => write!(f, "Default weapon cannot be sold: {}", id),
            Self::AmmoAlreadyFull(id) => write!(f, "Ammunition already full for: {}", id),
            Self::ArmorAlreadyFull => write!(f, "Armor already at maximum (100)"),
            Self::WeaponNotInInventory(id) => write!(f, "Weapon not found in inventory: {}", id),
        }
    }
}

impl std::error::Error for TraderError {}

/// Definition of a weapon or equipment item available in the Trader catalog.
#[derive(Clone, Debug, PartialEq)]
pub struct TraderWeaponItem {
    pub id: String,
    pub name: String,
    pub perk: PerkType,
    pub base_cost: u32,
    pub weight: f32,
    pub ammo_cost_per_mag: u32,
    pub mag_capacity: u32,
    pub max_ammo: u32,
    pub description: String,
    pub tier: u8,
    pub can_sell: bool,
}

impl TraderWeaponItem {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        perk: PerkType,
        base_cost: u32,
        weight: f32,
        ammo_cost_per_mag: u32,
        mag_capacity: u32,
        max_ammo: u32,
        description: impl Into<String>,
        tier: u8,
        can_sell: bool,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            perk,
            base_cost,
            weight,
            ammo_cost_per_mag,
            mag_capacity,
            max_ammo,
            description: description.into(),
            tier,
            can_sell,
        }
    }
}

/// Weapon currently held in player's inventory loadout.
#[derive(Clone, Debug, PartialEq)]
pub struct InventoryWeapon {
    pub id: String,
    pub name: String,
    pub perk: PerkType,
    pub weight: f32,
    pub current_ammo: u32,
    pub max_ammo: u32,
    pub ammo_cost_per_mag: u32,
    pub mag_capacity: u32,
    pub base_cost: u32,
    pub can_sell: bool,
}

impl InventoryWeapon {
    pub fn from_catalog_item(item: &TraderWeaponItem, initial_ammo: u32) -> Self {
        Self {
            id: item.id.clone(),
            name: item.name.clone(),
            perk: item.perk,
            weight: item.weight,
            current_ammo: initial_ammo.min(item.max_ammo),
            max_ammo: item.max_ammo,
            ammo_cost_per_mag: item.ammo_cost_per_mag,
            mag_capacity: item.mag_capacity,
            base_cost: item.base_cost,
            can_sell: item.can_sell,
        }
    }

    #[inline]
    pub fn is_ammo_full(&self) -> bool {
        self.current_ammo >= self.max_ammo
    }

    #[inline]
    pub fn missing_ammo(&self) -> u32 {
        self.max_ammo.saturating_sub(self.current_ammo)
    }
}

/// Killing Floor Trader Shop page component.
pub struct TraderShopPage {
    pub id: String,
    pub bounds: Rect,
    pub layout_box: LayoutBox,
    pub state: ComponentState,
    pub visible: bool,
    pub enabled: bool,

    // Player economy & state
    pub player_cash: u32,
    pub player_armor: f32,
    pub player_max_armor: f32,
    pub player_weight: f32,
    pub player_max_weight: f32,
    pub player_perk: PerkType,
    pub player_perk_level: u8,

    // Shop catalog & player inventory
    pub active_tab: TraderTab,
    pub shop_catalog: Vec<TraderWeaponItem>,
    pub player_inventory: Vec<InventoryWeapon>,

    // UI selection & scrolling
    pub selected_shop_index: Option<usize>,
    pub selected_inventory_index: Option<usize>,
    pub shop_scroll_offset: usize,
    pub inventory_scroll_offset: usize,
    pub row_height: f32,

    // Status / Feedback log
    pub last_status_message: Option<String>,
    pub is_status_error: bool,
}

impl Debug for TraderShopPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TraderShopPage")
            .field("id", &self.id)
            .field("bounds", &self.bounds)
            .field("cash", &self.player_cash)
            .field("armor", &self.player_armor)
            .field("weight", &self.player_weight)
            .field("max_weight", &self.player_max_weight)
            .field("perk", &self.player_perk)
            .field("perk_level", &self.player_perk_level)
            .field("catalog_len", &self.shop_catalog.len())
            .field("inventory_len", &self.player_inventory.len())
            .finish()
    }
}

impl Default for TraderShopPage {
    fn default() -> Self {
        Self::with_bounds(Rect::new(0.0, 0.0, 1024.0, 768.0))
    }
}

impl TraderShopPage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_bounds(bounds: Rect) -> Self {
        let mut page = Self {
            id: "TraderShopPage".to_string(),
            bounds,
            layout_box: LayoutBox::absolute(bounds.x, bounds.y, bounds.width, bounds.height),
            state: ComponentState::Normal,
            visible: true,
            enabled: true,

            player_cash: 1000,
            player_armor: 50.0,
            player_max_armor: 100.0,
            player_weight: 0.0,
            player_max_weight: 15.0,
            player_perk: PerkType::Commando,
            player_perk_level: 5,

            active_tab: TraderTab::AllWeapons,
            shop_catalog: Self::default_catalog(),
            player_inventory: Vec::new(),

            selected_shop_index: None,
            selected_inventory_index: None,
            shop_scroll_offset: 0,
            inventory_scroll_offset: 0,
            row_height: 38.0,

            last_status_message: Some("WELCOME TO HORZINE BIOTECH TRADER STATION".to_string()),
            is_status_error: false,
        };

        // Initialize default starter loadout: Knife and 9mm Tactical Pistol
        page.init_starter_loadout();
        page.update_max_weight();
        page
    }

    /// Sets up default Killing Floor starter loadout (Knife + 9mm Pistol).
    pub fn init_starter_loadout(&mut self) {
        self.player_inventory.clear();
        self.player_inventory.push(InventoryWeapon {
            id: "knife".to_string(),
            name: "Combat Knife".to_string(),
            perk: PerkType::Berserker,
            weight: 0.0,
            current_ammo: 0,
            max_ammo: 0,
            ammo_cost_per_mag: 0,
            mag_capacity: 0,
            base_cost: 0,
            can_sell: false,
        });

        self.player_inventory.push(InventoryWeapon {
            id: "pistol_9mm".to_string(),
            name: "9mm Tactical Pistol".to_string(),
            perk: PerkType::Sharpshooter,
            weight: 0.0,
            current_ammo: 120,
            max_ammo: 240,
            ammo_cost_per_mag: 10,
            mag_capacity: 15,
            base_cost: 150,
            can_sell: false,
        });

        self.recalculate_carried_weight();
    }

    /// Default authentic Killing Floor retail weapon catalog.
    pub fn default_catalog() -> Vec<TraderWeaponItem> {
        vec![
            // Berserker
            TraderWeaponItem::new(
                "machete",
                "Machete",
                PerkType::Berserker,
                100,
                1.0,
                0,
                0,
                0,
                "Sharp high-carbon steel blade for swift cleaving.",
                1,
                true,
            ),
            TraderWeaponItem::new(
                "axe",
                "Fire Axe",
                PerkType::Berserker,
                250,
                5.0,
                0,
                0,
                0,
                "Heavy woodchopper axe delivering devastating overhead chops.",
                1,
                true,
            ),
            TraderWeaponItem::new(
                "katana",
                "Katana",
                PerkType::Berserker,
                1000,
                3.0,
                0,
                0,
                0,
                "Folded steel samurai blade. Extremely fast and lethal.",
                2,
                true,
            ),
            TraderWeaponItem::new(
                "chainsaw",
                "Chainsaw",
                PerkType::Berserker,
                1000,
                8.0,
                0,
                0,
                0,
                "Gas-powered industrial saw. Stuns Scrakes and butchers mobs.",
                2,
                true,
            ),
            TraderWeaponItem::new(
                "claymore",
                "Claymore Sword",
                PerkType::Berserker,
                1500,
                6.0,
                0,
                0,
                0,
                "Highland two-handed greatsword with immense reach.",
                3,
                true,
            ),

            // Commando
            TraderWeaponItem::new(
                "bullpup",
                "Bullpup",
                PerkType::Commando,
                400,
                6.0,
                20,
                40,
                400,
                "British L85A1 bullpup assault rifle with holographic reflex optic.",
                1,
                true,
            ),
            TraderWeaponItem::new(
                "ak47",
                "AK-47",
                PerkType::Commando,
                1000,
                6.0,
                30,
                30,
                300,
                "Rugged 7.62x39mm assault rifle with immense stopping power.",
                2,
                true,
            ),
            TraderWeaponItem::new(
                "m4_assault",
                "M4 Carbine",
                PerkType::Commando,
                1250,
                6.0,
                25,
                30,
                360,
                "Versatile 5.56mm carbine with optical sight and tight grouping.",
                2,
                true,
            ),
            TraderWeaponItem::new(
                "scar",
                "SCAR-H Battle Rifle",
                PerkType::Commando,
                2500,
                5.0,
                40,
                20,
                200,
                "Elite special forces 7.62 NATO battle rifle. Extreme lethality.",
                3,
                true,
            ),
            TraderWeaponItem::new(
                "fnfal",
                "FN FAL ACOG",
                PerkType::Commando,
                2750,
                6.0,
                45,
                20,
                240,
                "Select-fire battle rifle mounted with 4x ACOG combat optic.",
                3,
                true,
            ),

            // Support Specialist
            TraderWeaponItem::new(
                "shotgun_pump",
                "Combat Shotgun",
                PerkType::SupportSpecialist,
                500,
                8.0,
                30,
                8,
                64,
                "12-gauge tactical pump shotgun fitted with high-power flashlight.",
                1,
                true,
            ),
            TraderWeaponItem::new(
                "hunting_shotgun",
                "Hunting Shotgun",
                PerkType::SupportSpecialist,
                750,
                10.0,
                35,
                2,
                48,
                "Double-barrel sawed-off shotgun. Both barrels can be fired at once.",
                2,
                true,
            ),
            TraderWeaponItem::new(
                "aa12",
                "AA-12 Auto Shotgun",
                PerkType::SupportSpecialist,
                4000,
                10.0,
                60,
                20,
                80,
                "Fully automatic 20-round drum magazine shotgun.",
                3,
                true,
            ),

            // Field Medic
            TraderWeaponItem::new(
                "mp7m",
                "MP7M Medic Gun",
                PerkType::FieldMedic,
                300,
                3.0,
                15,
                20,
                280,
                "Compact submachine gun equipped with remote medical dart injector.",
                1,
                true,
            ),
            TraderWeaponItem::new(
                "mp5m",
                "MP5M Medic Gun",
                PerkType::FieldMedic,
                1000,
                3.0,
                25,
                32,
                320,
                "Tactical 9mm SMG firing rapid regenerative healing darts.",
                2,
                true,
            ),
            TraderWeaponItem::new(
                "m7a3m",
                "M7A3 Medic Gun",
                PerkType::FieldMedic,
                2000,
                6.0,
                35,
                15,
                225,
                "Heavy carbine with high-velocity healing projectiles.",
                3,
                true,
            ),

            // Demolitions
            TraderWeaponItem::new(
                "pipebomb",
                "Pipe Bomb",
                PerkType::Demolitions,
                1500,
                1.0,
                500,
                1,
                2,
                "Proximity-detonated high explosive device.",
                1,
                true,
            ),
            TraderWeaponItem::new(
                "m79",
                "M79 Grenade Launcher",
                PerkType::Demolitions,
                1250,
                4.0,
                40,
                1,
                24,
                "Break-action 40mm explosive projectile launcher.",
                2,
                true,
            ),
            TraderWeaponItem::new(
                "m32",
                "M32 Multiple GL",
                PerkType::Demolitions,
                2500,
                7.0,
                100,
                6,
                36,
                "6-round revolving semi-automatic 40mm grenade launcher.",
                3,
                true,
            ),
            TraderWeaponItem::new(
                "law",
                "L.A.W. Rocket Launcher",
                PerkType::Demolitions,
                3000,
                11.0,
                150,
                1,
                10,
                "Light Anti-Tank shoulder-fired rocket system. Massive blast radius.",
                3,
                true,
            ),

            // Firebug
            TraderWeaponItem::new(
                "mac10",
                "MAC-10 Incendiary",
                PerkType::Firebug,
                500,
                4.0,
                20,
                30,
                300,
                "Submachine gun firing armor-piercing incendiary incendiary bullets.",
                1,
                true,
            ),
            TraderWeaponItem::new(
                "flamethrower",
                "Flamethrower",
                PerkType::Firebug,
                750,
                10.0,
                40,
                100,
                400,
                "Liquid fuel projector spewing continuous streams of burning napalm.",
                2,
                true,
            ),
            TraderWeaponItem::new(
                "flare_revolver",
                "Flare Revolver",
                PerkType::Firebug,
                500,
                2.0,
                25,
                6,
                72,
                "Heavy-caliber revolver launching combustible flare projectiles.",
                2,
                true,
            ),

            // Sharpshooter
            TraderWeaponItem::new(
                "dual_9mm",
                "Dual 9mm Pistols",
                PerkType::Sharpshooter,
                150,
                4.0,
                15,
                30,
                240,
                "Akimbo matched 9mm tactical pistols for high fire rate.",
                1,
                true,
            ),
            TraderWeaponItem::new(
                "lever_action",
                "Lever-Action Rifle",
                PerkType::Sharpshooter,
                200,
                5.0,
                20,
                10,
                100,
                "Classic .44 Magnum rifle delivering crisp headshot damage.",
                1,
                true,
            ),
            TraderWeaponItem::new(
                "handcannon",
                "Handcannon (.50 AE)",
                PerkType::Sharpshooter,
                500,
                4.0,
                30,
                8,
                80,
                "Massive semi-automatic pistol chambered in .50 Action Express.",
                2,
                true,
            ),
            TraderWeaponItem::new(
                "dual_handcannons",
                "Dual Handcannons",
                PerkType::Sharpshooter,
                1000,
                8.0,
                60,
                16,
                80,
                "Twin .50 caliber heavy pistols capable of dropping giant ZEDs.",
                2,
                true,
            ),
            TraderWeaponItem::new(
                "crossbow",
                "Crossbow",
                PerkType::Sharpshooter,
                800,
                9.0,
                35,
                1,
                36,
                "Compound hunting bow with precision optical scope and steel rebar bolts.",
                2,
                true,
            ),
            TraderWeaponItem::new(
                "m99",
                "M99 AMR .50 BMG",
                PerkType::Sharpshooter,
                3500,
                13.0,
                250,
                1,
                25,
                "Single-shot anti-materiel sniper rifle firing .50 BMG rounds.",
                3,
                true,
            ),
        ]
    }

    /// Updates max weight according to perk (Support Specialist gets weight bonuses in KF1).
    pub fn update_max_weight(&mut self) {
        if self.player_perk == PerkType::SupportSpecialist {
            // Level 0: 15kg, Level 1: 16kg, Level 2: 17kg, Level 3: 18kg, Level 4: 20kg, Level 5: 22kg, Level 6: 24kg
            let bonus = match self.player_perk_level {
                0 => 0.0,
                1 => 1.0,
                2 => 2.0,
                3 => 3.0,
                4 => 5.0,
                5 => 7.0,
                _ => 9.0,
            };
            self.player_max_weight = 15.0 + bonus;
        } else {
            self.player_max_weight = 15.0;
        }
    }

    /// Recalculates total carried weight from inventory items.
    pub fn recalculate_carried_weight(&mut self) {
        let total: f32 = self.player_inventory.iter().map(|w| w.weight).sum();
        self.player_weight = total;
    }

    /// Returns the perk discount percentage for weapons of the given perk.
    pub fn get_perk_discount_percent(&self, perk: PerkType) -> u32 {
        if self.player_perk != perk {
            return 0;
        }

        match self.player_perk_level {
            0 => 0,
            1 => 10,
            2 => 20,
            3 => 30,
            4 => 40,
            5 => 50,
            _ => 70, // Level 6+ gets 70% retail discount
        }
    }

    /// Calculates discounted purchase price of a weapon from the catalog.
    pub fn calculate_weapon_cost(&self, weapon: &TraderWeaponItem) -> u32 {
        let discount = self.get_perk_discount_percent(weapon.perk);
        if discount == 0 {
            weapon.base_cost
        } else {
            let discounted = (weapon.base_cost as f32) * (1.0 - (discount as f32) / 100.0);
            discounted.round() as u32
        }
    }

    /// Calculates resale value of an inventory weapon (Killing Floor standard is 75% of base price).
    pub fn calculate_sell_value(&self, weapon: &InventoryWeapon) -> u32 {
        ((weapon.base_cost as f32) * 0.75).floor() as u32
    }

    /// Calculates ammo magazine refill cost with perk discount if applicable.
    pub fn calculate_ammo_cost(&self, weapon: &InventoryWeapon) -> u32 {
        let discount = self.get_perk_discount_percent(weapon.perk);
        if discount == 0 {
            weapon.ammo_cost_per_mag
        } else {
            let discounted = (weapon.ammo_cost_per_mag as f32) * (1.0 - (discount as f32) / 100.0);
            (discounted.round() as u32).max(1)
        }
    }

    /// Calculates cost to fully refill ammo for a single inventory weapon.
    pub fn calculate_fill_ammo_cost(&self, weapon: &InventoryWeapon) -> u32 {
        let missing = weapon.missing_ammo();
        if missing == 0 || weapon.mag_capacity == 0 {
            return 0;
        }

        let mags_needed = missing.div_ceil(weapon.mag_capacity);
        let per_mag = self.calculate_ammo_cost(weapon);
        mags_needed * per_mag
    }

    /// Calculates total cost to fill ammo for ALL carried weapons.
    pub fn calculate_total_fill_ammo_cost(&self) -> u32 {
        self.player_inventory
            .iter()
            .map(|w| self.calculate_fill_ammo_cost(w))
            .sum()
    }

    /// Calculates cost to replenish armor up to 100.
    /// Base full armor cost is £300. Field Medic lvl 5 gets 50% discount, lvl 6 gets 70% discount.
    pub fn calculate_armor_cost(&self) -> u32 {
        let missing = (self.player_max_armor - self.player_armor).max(0.0);
        if missing <= 0.001 {
            return 0;
        }

        let base_full_cost: f32 = 300.0;
        let discount = if self.player_perk == PerkType::FieldMedic {
            match self.player_perk_level {
                0..=4 => 0.0,
                5 => 0.50,
                _ => 0.70,
            }
        } else {
            0.0
        };

        let discounted_full = base_full_cost * (1.0 - discount);
        let cost = (missing / self.player_max_armor) * discounted_full;
        (cost.ceil() as u32).max(1)
    }

    // =========================================================================
    // Core Transaction Contract Methods
    // =========================================================================

    /// Buys a weapon from the catalog.
    pub fn buy_weapon(&mut self, weapon_id: &str) -> Result<u32, TraderError> {
        let weapon_opt = self.shop_catalog.iter().find(|w| w.id == weapon_id).cloned();
        let weapon = match weapon_opt {
            Some(w) => w,
            None => {
                let err = TraderError::WeaponNotFound(weapon_id.to_string());
                self.set_status(&err.to_string(), true);
                return Err(err);
            }
        };

        // Check if already in inventory
        if self.player_inventory.iter().any(|w| w.id == weapon.id) {
            let err = TraderError::AlreadyOwned(weapon.name.clone());
            self.set_status(&err.to_string(), true);
            return Err(err);
        }

        // Check weight capacity
        if self.player_weight + weapon.weight > self.player_max_weight {
            let err = TraderError::WeightLimitExceeded {
                weapon_weight: weapon.weight,
                current: self.player_weight,
                max: self.player_max_weight,
            };
            self.set_status(&err.to_string(), true);
            return Err(err);
        }

        // Check funds
        let cost = self.calculate_weapon_cost(&weapon);
        if self.player_cash < cost {
            let err = TraderError::InsufficientFunds {
                needed: cost,
                available: self.player_cash,
            };
            self.set_status(&err.to_string(), true);
            return Err(err);
        }

        // Execute transaction
        self.player_cash -= cost;
        let initial_ammo = weapon.mag_capacity * 2; // In KF1, weapons come with 2 magazines
        let inv_weapon = InventoryWeapon::from_catalog_item(&weapon, initial_ammo);
        self.player_inventory.push(inv_weapon);
        self.recalculate_carried_weight();

        let msg = format!("Purchased {} for £{} (Weight: +{:.1}kg)", weapon.name, cost, weapon.weight);
        self.set_status(&msg, false);
        Ok(cost)
    }

    /// Sells an item from player's inventory back to the Trader.
    pub fn sell_weapon(&mut self, weapon_id: &str) -> Result<u32, TraderError> {
        let index_opt = self.player_inventory.iter().position(|w| w.id == weapon_id);
        let index = match index_opt {
            Some(i) => i,
            None => {
                let err = TraderError::WeaponNotInInventory(weapon_id.to_string());
                self.set_status(&err.to_string(), true);
                return Err(err);
            }
        };

        let weapon = &self.player_inventory[index];
        if !weapon.can_sell {
            let err = TraderError::CannotSellWeapon(weapon.name.clone());
            self.set_status(&err.to_string(), true);
            return Err(err);
        }

        let resale_value = self.calculate_sell_value(weapon);
        let weapon_name = weapon.name.clone();
        let weapon_weight = weapon.weight;

        self.player_inventory.remove(index);
        self.player_cash += resale_value;
        self.recalculate_carried_weight();

        // Adjust selection
        if let Some(sel) = self.selected_inventory_index {
            if sel >= self.player_inventory.len() {
                self.selected_inventory_index = if !self.player_inventory.is_empty() {
                    Some(self.player_inventory.len() - 1)
                } else {
                    None
                };
            }
        }

        let msg = format!("Sold {} for £{} (Freed {:.1}kg)", weapon_name, resale_value, weapon_weight);
        self.set_status(&msg, false);
        Ok(resale_value)
    }

    /// Buys 1 magazine of ammunition for a specific inventory weapon.
    pub fn buy_ammo(&mut self, weapon_id: &str) -> Result<u32, TraderError> {
        let index_opt = self.player_inventory.iter().position(|w| w.id == weapon_id);
        let index = match index_opt {
            Some(i) => i,
            None => {
                let err = TraderError::WeaponNotInInventory(weapon_id.to_string());
                self.set_status(&err.to_string(), true);
                return Err(err);
            }
        };

        let weapon = &self.player_inventory[index];
        if weapon.is_ammo_full() {
            let err = TraderError::AmmoAlreadyFull(weapon.name.clone());
            self.set_status(&err.to_string(), true);
            return Err(err);
        }

        let cost = self.calculate_ammo_cost(weapon);
        if self.player_cash < cost {
            let err = TraderError::InsufficientFunds {
                needed: cost,
                available: self.player_cash,
            };
            self.set_status(&err.to_string(), true);
            return Err(err);
        }

        let weapon_name = weapon.name.clone();
        let mag = weapon.mag_capacity;

        let w_mut = &mut self.player_inventory[index];
        w_mut.current_ammo = (w_mut.current_ammo + mag).min(w_mut.max_ammo);
        let new_ammo = w_mut.current_ammo;
        let max_ammo = w_mut.max_ammo;
        self.player_cash -= cost;

        let msg = format!("Purchased ammo for {} (+{} rounds: {}/{}) - £{}", weapon_name, mag, new_ammo, max_ammo, cost);
        self.set_status(&msg, false);
        Ok(cost)
    }

    /// Buys Kevlar combat armor.
    pub fn buy_armor(&mut self) -> Result<u32, TraderError> {
        if self.player_armor >= self.player_max_armor {
            let err = TraderError::ArmorAlreadyFull;
            self.set_status(&err.to_string(), true);
            return Err(err);
        }

        let full_cost = self.calculate_armor_cost();
        if full_cost == 0 {
            let err = TraderError::ArmorAlreadyFull;
            self.set_status(&err.to_string(), true);
            return Err(err);
        }

        if self.player_cash >= full_cost {
            // Full purchase
            self.player_cash -= full_cost;
            self.player_armor = self.player_max_armor;
            let msg = format!("Kevlar Combat Armor fully replenished (100/100) - £{}", full_cost);
            self.set_status(&msg, false);
            Ok(full_cost)
        } else if self.player_cash > 0 {
            // Partial purchase with whatever cash is available
            let spent = self.player_cash;
            let missing = self.player_max_armor - self.player_armor;
            let fraction = (spent as f32) / (full_cost as f32);
            let armor_gained = (missing * fraction).min(missing);

            self.player_cash = 0;
            self.player_armor = (self.player_armor + armor_gained).min(self.player_max_armor);
            let msg = format!("Purchased partial Kevlar Combat Armor (+{:.0} armor) - £{}", armor_gained, spent);
            self.set_status(&msg, false);
            Ok(spent)
        } else {
            let err = TraderError::InsufficientFunds {
                needed: full_cost,
                available: 0,
            };
            self.set_status(&err.to_string(), true);
            Err(err)
        }
    }

    /// Quick auto-resupply: Fills ammunition for all carried weapons up to cash limit.
    pub fn fill_all_ammo(&mut self) -> Result<u32, TraderError> {
        let total_cost = self.calculate_total_fill_ammo_cost();
        if total_cost == 0 {
            let msg = "All weapons already fully stocked with ammunition.";
            self.set_status(msg, false);
            return Ok(0);
        }

        let mut total_spent = 0;
        let mut filled_count = 0;

        // Loop through each weapon and top off
        for i in 0..self.player_inventory.len() {
            let w = &self.player_inventory[i];
            let needed_cost = self.calculate_fill_ammo_cost(w);
            if needed_cost == 0 {
                continue;
            }

            if self.player_cash >= needed_cost {
                self.player_cash -= needed_cost;
                total_spent += needed_cost;
                let w_mut = &mut self.player_inventory[i];
                w_mut.current_ammo = w_mut.max_ammo;
                filled_count += 1;
            } else {
                // Buy as many magazines as cash allows
                let per_mag = self.calculate_ammo_cost(w);
                if per_mag > 0 {
                    while self.player_cash >= per_mag && !self.player_inventory[i].is_ammo_full() {
                        self.player_cash -= per_mag;
                        total_spent += per_mag;
                        let mag = self.player_inventory[i].mag_capacity;
                        let max_a = self.player_inventory[i].max_ammo;
                        self.player_inventory[i].current_ammo =
                            (self.player_inventory[i].current_ammo + mag).min(max_a);
                        filled_count += 1;
                    }
                }
            }
        }

        let msg = format!("Auto-fill resupply complete: £{} spent across {} weapons.", total_spent, filled_count);
        self.set_status(&msg, false);
        Ok(total_spent)
    }

    /// Sets the player's active perk and level, recalculating bonuses and limits.
    pub fn set_perk(&mut self, perk: PerkType, level: u8) {
        self.player_perk = perk;
        self.player_perk_level = level;
        self.update_max_weight();
    }

    /// Sets status feedback line.
    pub fn set_status(&mut self, msg: &str, is_error: bool) {
        self.last_status_message = Some(msg.to_string());
        self.is_status_error = is_error;
    }

    /// Returns weapons matching the currently active catalog tab.
    pub fn filtered_catalog(&self) -> Vec<&TraderWeaponItem> {
        self.shop_catalog
            .iter()
            .filter(|item| match self.active_tab {
                TraderTab::AllWeapons => true,
                TraderTab::PerkWeapons => item.perk == self.player_perk,
                tab => tab.matches_perk(item.perk),
            })
            .collect()
    }

    // =========================================================================
    // UI Layout Geometry Helpers
    // =========================================================================

    #[inline]
    pub fn header_rect(&self) -> Rect {
        Rect::new(self.bounds.x + 16.0, self.bounds.y + 12.0, self.bounds.width - 32.0, 50.0)
    }

    #[inline]
    pub fn stats_bar_rect(&self) -> Rect {
        Rect::new(self.bounds.x + 16.0, self.bounds.y + 68.0, self.bounds.width - 32.0, 48.0)
    }

    #[inline]
    pub fn tabs_bar_rect(&self) -> Rect {
        Rect::new(self.bounds.x + 16.0, self.bounds.y + 122.0, self.bounds.width - 32.0, 32.0)
    }

    #[inline]
    pub fn shop_panel_rect(&self) -> Rect {
        let width = (self.bounds.width - 44.0) * 0.55;
        let y = self.bounds.y + 160.0;
        let height = self.bounds.height - 230.0;
        Rect::new(self.bounds.x + 16.0, y, width, height)
    }

    #[inline]
    pub fn inventory_panel_rect(&self) -> Rect {
        let shop_w = (self.bounds.width - 44.0) * 0.55;
        let x = self.bounds.x + 16.0 + shop_w + 12.0;
        let width = (self.bounds.width - 44.0) * 0.45;
        let y = self.bounds.y + 160.0;
        let height = self.bounds.height - 230.0;
        Rect::new(x, y, width, height)
    }

    #[inline]
    pub fn footer_rect(&self) -> Rect {
        let height = 54.0;
        let y = self.bounds.y + self.bounds.height - height - 12.0;
        Rect::new(self.bounds.x + 16.0, y, self.bounds.width - 32.0, height)
    }

    pub fn max_visible_shop_rows(&self) -> usize {
        let panel = self.shop_panel_rect();
        let body_height = (panel.height - 30.0).max(0.0);
        (body_height / self.row_height).floor() as usize
    }

    pub fn max_visible_inventory_rows(&self) -> usize {
        let panel = self.inventory_panel_rect();
        let body_height = (panel.height - 30.0).max(0.0);
        (body_height / self.row_height).floor() as usize
    }
}

// =============================================================================
// GUIComponent Trait Implementation
// =============================================================================

impl GUIComponent for TraderShopPage {
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
                // 1. Check tab clicks
                let tabs_rect = self.tabs_bar_rect();
                if tabs_rect.contains(x, y) {
                    let tabs = [
                        TraderTab::AllWeapons,
                        TraderTab::PerkWeapons,
                        TraderTab::Berserker,
                        TraderTab::Commando,
                        TraderTab::SupportSpecialist,
                        TraderTab::FieldMedic,
                        TraderTab::Demolitions,
                        TraderTab::Firebug,
                        TraderTab::Sharpshooter,
                    ];
                    let tab_w = tabs_rect.width / (tabs.len() as f32);
                    let tab_idx = ((x - tabs_rect.x) / tab_w).floor() as usize;
                    if tab_idx < tabs.len() {
                        self.active_tab = tabs[tab_idx];
                        self.shop_scroll_offset = 0;
                        self.selected_shop_index = None;
                        return true;
                    }
                }

                // 2. Check Armor Buy button in stats bar
                let stats = self.stats_bar_rect();
                let armor_btn_rect = Rect::new(stats.x + 360.0, stats.y + 6.0, 110.0, 36.0);
                if armor_btn_rect.contains(x, y) {
                    let _ = self.buy_armor();
                    return true;
                }

                // 3. Check Shop Item clicks / Buy button clicks
                let shop_rect = self.shop_panel_rect();
                let shop_body = Rect::new(shop_rect.x, shop_rect.y + 28.0, shop_rect.width, shop_rect.height - 28.0);
                if shop_body.contains(x, y) {
                    let row_idx = ((y - shop_body.y) / self.row_height).floor() as usize;
                    let target_idx = self.shop_scroll_offset + row_idx;
                    let weapon_info = {
                        let filtered = self.filtered_catalog();
                        if target_idx < filtered.len() {
                            Some((filtered[target_idx].id.clone(), target_idx))
                        } else {
                            None
                        }
                    };
                    if let Some((weapon_id, idx)) = weapon_info {
                        self.selected_shop_index = Some(idx);

                        // Check if click was on the "BUY" button for that row (right side of row)
                        let row_y = shop_body.y + (row_idx as f32) * self.row_height;
                        let buy_btn = Rect::new(shop_body.x + shop_body.width - 64.0, row_y + 4.0, 58.0, self.row_height - 8.0);
                        if buy_btn.contains(x, y) {
                            let _ = self.buy_weapon(&weapon_id);
                        }
                        return true;
                    }
                }

                // 4. Check Inventory Item clicks / Sell / Ammo button clicks
                let inv_rect = self.inventory_panel_rect();
                let inv_body = Rect::new(inv_rect.x, inv_rect.y + 28.0, inv_rect.width, inv_rect.height - 28.0);
                if inv_body.contains(x, y) {
                    let row_idx = ((y - inv_body.y) / self.row_height).floor() as usize;
                    let target_idx = self.inventory_scroll_offset + row_idx;
                    let item_info = {
                        if target_idx < self.player_inventory.len() {
                            let row_y = inv_body.y + (row_idx as f32) * self.row_height;
                            let ammo_btn = Rect::new(inv_body.x + inv_body.width - 120.0, row_y + 4.0, 54.0, self.row_height - 8.0);
                            let sell_btn = Rect::new(inv_body.x + inv_body.width - 62.0, row_y + 4.0, 56.0, self.row_height - 8.0);
                            Some((
                                self.player_inventory[target_idx].id.clone(),
                                target_idx,
                                ammo_btn.contains(x, y),
                                sell_btn.contains(x, y),
                            ))
                        } else {
                            None
                        }
                    };

                    if let Some((wid, idx, is_ammo_click, is_sell_click)) = item_info {
                        self.selected_inventory_index = Some(idx);

                        if is_ammo_click {
                            let _ = self.buy_ammo(&wid);
                            return true;
                        }

                        if is_sell_click {
                            let _ = self.sell_weapon(&wid);
                            return true;
                        }
                        return true;
                    }
                }

                // 5. Check Footer Buttons (Fill All Ammo / Buy Selected)
                let footer = self.footer_rect();
                let fill_all_rect = Rect::new(footer.x + 12.0, footer.y + 10.0, 200.0, 34.0);
                if fill_all_rect.contains(x, y) {
                    let _ = self.fill_all_ammo();
                    return true;
                }

                let buy_sel_rect = Rect::new(footer.x + 220.0, footer.y + 10.0, 160.0, 34.0);
                if buy_sel_rect.contains(x, y) {
                    let wid_opt = if let Some(idx) = self.selected_shop_index {
                        self.filtered_catalog().get(idx).map(|w| w.id.clone())
                    } else {
                        None
                    };
                    if let Some(wid) = wid_opt {
                        let _ = self.buy_weapon(&wid);
                        return true;
                    }
                }

                let sell_sel_rect = Rect::new(footer.x + 390.0, footer.y + 10.0, 160.0, 34.0);
                if sell_sel_rect.contains(x, y) {
                    let wid_opt = if let Some(idx) = self.selected_inventory_index {
                        self.player_inventory.get(idx).map(|w| w.id.clone())
                    } else {
                        None
                    };
                    if let Some(wid) = wid_opt {
                        let _ = self.sell_weapon(&wid);
                        return true;
                    }
                }

                // If clicked inside page bounds, consume event
                self.bounds.contains(x, y)
            }

            MouseEvent::Scroll { delta, x, y } => {
                let shop_rect = self.shop_panel_rect();
                if shop_rect.contains(x, y) {
                    let max_vis = self.max_visible_shop_rows();
                    let count = self.filtered_catalog().len();
                    if delta > 0.0 && self.shop_scroll_offset > 0 {
                        self.shop_scroll_offset -= 1;
                    } else if delta < 0.0 && (self.shop_scroll_offset + max_vis) < count {
                        self.shop_scroll_offset += 1;
                    }
                    return true;
                }

                let inv_rect = self.inventory_panel_rect();
                if inv_rect.contains(x, y) {
                    let max_vis = self.max_visible_inventory_rows();
                    let count = self.player_inventory.len();
                    if delta > 0.0 && self.inventory_scroll_offset > 0 {
                        self.inventory_scroll_offset -= 1;
                    } else if delta < 0.0 && (self.inventory_scroll_offset + max_vis) < count {
                        self.inventory_scroll_offset += 1;
                    }
                    return true;
                }

                false
            }

            _ => false,
        }
    }

    fn render(&self, canvas: &mut Canvas) {
        if !self.visible {
            return;
        }

        // 1. Master Background Quad (Dark industrial metallic with red combat accents)
        canvas.draw_filled_rect(self.bounds, [0.06, 0.07, 0.08, 0.98]);
        canvas.draw_border(self.bounds, 2.0, [0.45, 0.12, 0.12, 1.0]);

        // 2. Header Bar
        let hdr = self.header_rect();
        canvas.draw_filled_rect(hdr, [0.12, 0.08, 0.08, 1.0]);
        canvas.draw_border(hdr, 1.0, [0.65, 0.18, 0.18, 1.0]);

        canvas.draw_text(
            "HORZINE BIOTECH // AUTOMATED RESUPPLY STATION",
            [hdr.x + 16.0, hdr.y + 18.0],
            16.0,
            [1.0, 0.85, 0.25, 1.0], // Golden warning yellow
            TextAlignment::Left,
        );

        canvas.draw_text(
            "TRADER POD V1065 // EMERGENCY PROTOCOL ACTIVE",
            [hdr.x + 16.0, hdr.y + 36.0],
            11.0,
            [0.75, 0.75, 0.75, 1.0],
            TextAlignment::Left,
        );

        // Cash / Dosh display (bold green/gold)
        let cash_str = format!("DOSH: £{}", self.player_cash);
        canvas.draw_text(
            &cash_str,
            [hdr.x + hdr.width - 20.0, hdr.y + 26.0],
            22.0,
            [0.35, 1.0, 0.35, 1.0], // Bright Dosh Green
            TextAlignment::Right,
        );

        // 3. Stats Bar: Perk Banner, Weight Capacity Bar, Armor Status
        let stats = self.stats_bar_rect();
        canvas.draw_filled_rect(stats, [0.10, 0.11, 0.13, 1.0]);
        canvas.draw_border(stats, 1.0, [0.25, 0.28, 0.32, 1.0]);

        // 3a. Perk Status & Discount badge
        let perk_discount = self.get_perk_discount_percent(self.player_perk);
        let perk_text = format!(
            "PERK: {} (LVL {}) [-{}% DISCOUNT]",
            self.player_perk.display_name(),
            self.player_perk_level,
            perk_discount
        );
        canvas.draw_text(
            &perk_text,
            [stats.x + 14.0, stats.y + 24.0],
            13.0,
            [0.4, 0.85, 1.0, 1.0], // Perk Cyan
            TextAlignment::Left,
        );

        // 3b. Armor status and Buy Armor button
        let armor_cost = self.calculate_armor_cost();
        let armor_text = format!("ARMOR: {:.0}/100", self.player_armor);
        canvas.draw_text(
            &armor_text,
            [stats.x + 290.0, stats.y + 24.0],
            12.0,
            [0.7, 0.9, 1.0, 1.0],
            TextAlignment::Left,
        );

        // Armor gauge bar
        let armor_bar_bg = Rect::new(stats.x + 290.0, stats.y + 32.0, 60.0, 6.0);
        canvas.draw_filled_rect(armor_bar_bg, [0.2, 0.2, 0.2, 1.0]);
        let armor_ratio = (self.player_armor / self.player_max_armor).clamp(0.0, 1.0);
        let armor_bar_fill = Rect::new(stats.x + 290.0, stats.y + 32.0, 60.0 * armor_ratio, 6.0);
        canvas.draw_filled_rect(armor_bar_fill, [0.2, 0.6, 1.0, 1.0]);

        // Buy Armor button
        let armor_btn = Rect::new(stats.x + 360.0, stats.y + 8.0, 110.0, 32.0);
        let armor_btn_color = if self.player_armor >= 100.0 {
            [0.18, 0.18, 0.20, 0.6] // Disabled
        } else if self.player_cash >= armor_cost {
            [0.20, 0.40, 0.22, 1.0] // Green buy
        } else {
            [0.35, 0.25, 0.15, 1.0] // Orange partial
        };
        canvas.draw_filled_rect(armor_btn, armor_btn_color);
        canvas.draw_border(armor_btn, 1.0, [0.4, 0.6, 0.5, 1.0]);
        let armor_lbl = if self.player_armor >= 100.0 {
            "ARMOR FULL".to_string()
        } else {
            format!("BUY ARMOR (£{})", armor_cost)
        };
        canvas.draw_text(
            &armor_lbl,
            [armor_btn.x + armor_btn.width * 0.5, armor_btn.y + armor_btn.height * 0.5],
            11.0,
            [1.0, 1.0, 1.0, 1.0],
            TextAlignment::Center,
        );

        // 3c. Weight Capacity Bar (Max 15/15 kg, or up to 24 with Support perk)
        let weight_x = stats.x + stats.width - 260.0;
        let weight_text = format!(
            "WEIGHT CAPACITY: {:.1} / {:.1} BLOCKS",
            self.player_weight, self.player_max_weight
        );
        let weight_ratio = (self.player_weight / self.player_max_weight).clamp(0.0, 1.0);

        // Color coding: Green (<10), Yellow (10..14), Red (14..15)
        let weight_color = if self.player_weight >= self.player_max_weight {
            [0.95, 0.2, 0.2, 1.0] // Maxed / Red
        } else if self.player_weight >= 10.0 {
            [1.0, 0.8, 0.2, 1.0] // Heavy / Yellow
        } else {
            [0.3, 0.9, 0.3, 1.0] // Safe / Green
        };

        canvas.draw_text(
            &weight_text,
            [weight_x, stats.y + 16.0],
            11.0,
            weight_color,
            TextAlignment::Left,
        );

        // Progress bar background & fill
        let weight_bg = Rect::new(weight_x, stats.y + 26.0, 240.0, 12.0);
        canvas.draw_filled_rect(weight_bg, [0.15, 0.16, 0.18, 1.0]);
        canvas.draw_border(weight_bg, 1.0, [0.35, 0.35, 0.40, 1.0]);

        let fill_w = 238.0 * weight_ratio;
        if fill_w > 0.0 {
            let weight_fill = Rect::new(weight_x + 1.0, stats.y + 27.0, fill_w, 10.0);
            canvas.draw_filled_rect(weight_fill, weight_color);
        }

        // 4. Category Tabs Bar
        let tabs_rect = self.tabs_bar_rect();
        let tabs = [
            TraderTab::AllWeapons,
            TraderTab::PerkWeapons,
            TraderTab::Berserker,
            TraderTab::Commando,
            TraderTab::SupportSpecialist,
            TraderTab::FieldMedic,
            TraderTab::Demolitions,
            TraderTab::Firebug,
            TraderTab::Sharpshooter,
        ];
        let tab_w = tabs_rect.width / (tabs.len() as f32);

        for (i, tab) in tabs.iter().enumerate() {
            let tab_r = Rect::new(tabs_rect.x + (i as f32) * tab_w, tabs_rect.y, tab_w - 2.0, tabs_rect.height);
            let is_active = self.active_tab == *tab;
            let tab_color = if is_active {
                [0.45, 0.15, 0.15, 1.0] // Crimson active
            } else {
                [0.14, 0.15, 0.18, 1.0] // Dark idle
            };
            canvas.draw_filled_rect(tab_r, tab_color);
            canvas.draw_border(tab_r, 1.0, if is_active { [0.75, 0.3, 0.3, 1.0] } else { [0.25, 0.28, 0.30, 1.0] });

            canvas.draw_text(
                tab.display_name(),
                [tab_r.x + tab_r.width * 0.5, tab_r.y + tab_r.height * 0.5],
                11.0,
                if is_active { [1.0, 1.0, 1.0, 1.0] } else { [0.75, 0.75, 0.75, 1.0] },
                TextAlignment::Center,
            );
        }

        // 5. Left Panel: Shop Buy Catalog
        let shop_rect = self.shop_panel_rect();
        canvas.draw_filled_rect(shop_rect, [0.08, 0.09, 0.11, 1.0]);
        canvas.draw_border(shop_rect, 1.0, [0.35, 0.38, 0.42, 1.0]);

        // Shop panel header
        let shop_hdr = Rect::new(shop_rect.x, shop_rect.y, shop_rect.width, 28.0);
        canvas.draw_filled_rect(shop_hdr, [0.14, 0.16, 0.19, 1.0]);
        canvas.draw_border(shop_hdr, 1.0, [0.3, 0.32, 0.36, 1.0]);
        canvas.draw_text(
            "TRADER WEAPONS CATALOG (BUY)",
            [shop_hdr.x + 12.0, shop_hdr.y + 14.0],
            12.0,
            [0.9, 0.9, 0.9, 1.0],
            TextAlignment::Left,
        );

        // Shop rows
        let filtered_shop = self.filtered_catalog();
        let max_shop_rows = self.max_visible_shop_rows();
        let shop_start = self.shop_scroll_offset;
        let shop_end = (shop_start + max_shop_rows).min(filtered_shop.len());

        for i in shop_start..shop_end {
            let row_idx = i - shop_start;
            let row_y = shop_rect.y + 28.0 + (row_idx as f32) * self.row_height;
            let row_rect = Rect::new(shop_rect.x + 2.0, row_y, shop_rect.width - 4.0, self.row_height - 2.0);

            let is_selected = self.selected_shop_index == Some(i);
            let item = filtered_shop[i];
            let is_owned = self.player_inventory.iter().any(|inv| inv.id == item.id);
            let discounted_cost = self.calculate_weapon_cost(item);
            let discount_pct = self.get_perk_discount_percent(item.perk);
            let can_afford = self.player_cash >= discounted_cost;
            let can_carry = (self.player_weight + item.weight) <= self.player_max_weight;

            let row_bg = if is_selected {
                [0.35, 0.18, 0.18, 0.9] // Selected highlight
            } else if is_owned {
                [0.10, 0.13, 0.10, 0.7] // Owned greenish dark
            } else if i % 2 == 0 {
                [0.11, 0.12, 0.14, 0.8]
            } else {
                [0.09, 0.10, 0.12, 0.8]
            };
            canvas.draw_filled_rect(row_rect, row_bg);

            // Item Name + Perk tag
            let name_str = format!("{} [{}]", item.name, item.perk.display_name());
            canvas.draw_text(
                &name_str,
                [row_rect.x + 8.0, row_rect.y + 12.0],
                12.0,
                if is_owned { [0.6, 0.8, 0.6, 1.0] } else { [0.95, 0.95, 0.95, 1.0] },
                TextAlignment::Left,
            );

            // Weight info
            let weight_str = format!("{:.1} kg", item.weight);
            canvas.draw_text(
                &weight_str,
                [row_rect.x + 8.0, row_rect.y + 26.0],
                10.0,
                if can_carry { [0.75, 0.75, 0.75, 1.0] } else { [0.9, 0.3, 0.3, 1.0] },
                TextAlignment::Left,
            );

            // Price display (discount badge if applicable)
            let price_x = row_rect.x + row_rect.width - 70.0;
            if discount_pct > 0 {
                let disc_text = format!("£{} (-{}%)", discounted_cost, discount_pct);
                canvas.draw_text(
                    &disc_text,
                    [price_x, row_rect.y + 12.0],
                    11.0,
                    [0.3, 0.9, 0.3, 1.0], // Green discount
                    TextAlignment::Right,
                );
                let orig_text = format!("orig £{}", item.base_cost);
                canvas.draw_text(
                    &orig_text,
                    [price_x, row_rect.y + 25.0],
                    9.0,
                    [0.6, 0.6, 0.6, 1.0],
                    TextAlignment::Right,
                );
            } else {
                let price_text = format!("£{}", discounted_cost);
                canvas.draw_text(
                    &price_text,
                    [price_x, row_rect.y + 18.0],
                    12.0,
                    if can_afford { [1.0, 0.9, 0.4, 1.0] } else { [0.8, 0.3, 0.3, 1.0] },
                    TextAlignment::Right,
                );
            }

            // BUY button
            let buy_btn_rect = Rect::new(row_rect.x + row_rect.width - 64.0, row_rect.y + 4.0, 58.0, row_rect.height - 8.0);
            let buy_bg = if is_owned {
                [0.15, 0.15, 0.15, 0.5]
            } else if can_afford && can_carry {
                [0.18, 0.45, 0.22, 1.0]
            } else {
                [0.3, 0.15, 0.15, 0.6]
            };
            canvas.draw_filled_rect(buy_btn_rect, buy_bg);
            canvas.draw_border(buy_btn_rect, 1.0, [0.4, 0.45, 0.5, 1.0]);

            let buy_label = if is_owned {
                "OWNED"
            } else if !can_carry {
                "HEAVY"
            } else if !can_afford {
                "NO £"
            } else {
                "BUY"
            };
            canvas.draw_text(
                buy_label,
                [buy_btn_rect.x + buy_btn_rect.width * 0.5, buy_btn_rect.y + buy_btn_rect.height * 0.5],
                10.0,
                [1.0, 1.0, 1.0, 1.0],
                TextAlignment::Center,
            );
        }

        // 6. Right Panel: Player Inventory / Sell List & Ammo refill
        let inv_rect = self.inventory_panel_rect();
        canvas.draw_filled_rect(inv_rect, [0.08, 0.09, 0.11, 1.0]);
        canvas.draw_border(inv_rect, 1.0, [0.35, 0.38, 0.42, 1.0]);

        // Inventory panel header
        let inv_hdr = Rect::new(inv_rect.x, inv_rect.y, inv_rect.width, 28.0);
        canvas.draw_filled_rect(inv_hdr, [0.14, 0.16, 0.19, 1.0]);
        canvas.draw_border(inv_hdr, 1.0, [0.3, 0.32, 0.36, 1.0]);
        canvas.draw_text(
            "CURRENT LOADOUT (SELL / AMMO)",
            [inv_hdr.x + 12.0, inv_hdr.y + 14.0],
            12.0,
            [0.9, 0.9, 0.9, 1.0],
            TextAlignment::Left,
        );

        // Inventory rows
        let max_inv_rows = self.max_visible_inventory_rows();
        let inv_start = self.inventory_scroll_offset;
        let inv_end = (inv_start + max_inv_rows).min(self.player_inventory.len());

        for i in inv_start..inv_end {
            let row_idx = i - inv_start;
            let row_y = inv_rect.y + 28.0 + (row_idx as f32) * self.row_height;
            let row_rect = Rect::new(inv_rect.x + 2.0, row_y, inv_rect.width - 4.0, self.row_height - 2.0);

            let is_selected = self.selected_inventory_index == Some(i);
            let item = &self.player_inventory[i];
            let sell_val = self.calculate_sell_value(item);
            let ammo_cost = self.calculate_ammo_cost(item);

            let row_bg = if is_selected {
                [0.35, 0.18, 0.18, 0.9]
            } else if i % 2 == 0 {
                [0.11, 0.12, 0.14, 0.8]
            } else {
                [0.09, 0.10, 0.12, 0.8]
            };
            canvas.draw_filled_rect(row_rect, row_bg);

            // Item Name & weight
            let item_line = format!("{} ({:.1}kg)", item.name, item.weight);
            canvas.draw_text(
                &item_line,
                [row_rect.x + 8.0, row_rect.y + 12.0],
                12.0,
                [0.95, 0.95, 0.95, 1.0],
                TextAlignment::Left,
            );

            // Ammo status line
            let ammo_status = if item.max_ammo > 0 {
                format!("Ammo: {}/{}", item.current_ammo, item.max_ammo)
            } else {
                "Melee / Infinite".to_string()
            };
            canvas.draw_text(
                &ammo_status,
                [row_rect.x + 8.0, row_rect.y + 26.0],
                10.0,
                if item.is_ammo_full() { [0.5, 0.85, 0.5, 1.0] } else { [0.9, 0.7, 0.2, 1.0] },
                TextAlignment::Left,
            );

            // AMMO button (if weapon uses ammo)
            let ammo_btn_rect = Rect::new(row_rect.x + row_rect.width - 120.0, row_rect.y + 4.0, 54.0, row_rect.height - 8.0);
            if item.max_ammo > 0 {
                let ammo_bg = if item.is_ammo_full() {
                    [0.15, 0.15, 0.15, 0.5]
                } else if self.player_cash >= ammo_cost {
                    [0.18, 0.35, 0.50, 1.0]
                } else {
                    [0.35, 0.20, 0.20, 0.6]
                };
                canvas.draw_filled_rect(ammo_btn_rect, ammo_bg);
                canvas.draw_border(ammo_btn_rect, 1.0, [0.35, 0.45, 0.6, 1.0]);

                let ammo_lbl = if item.is_ammo_full() {
                    "FULL".to_string()
                } else {
                    format!("+£{}", ammo_cost)
                };
                canvas.draw_text(
                    &ammo_lbl,
                    [ammo_btn_rect.x + ammo_btn_rect.width * 0.5, ammo_btn_rect.y + ammo_btn_rect.height * 0.5],
                    9.0,
                    [1.0, 1.0, 1.0, 1.0],
                    TextAlignment::Center,
                );
            }

            // SELL button
            let sell_btn_rect = Rect::new(row_rect.x + row_rect.width - 62.0, row_rect.y + 4.0, 56.0, row_rect.height - 8.0);
            let sell_bg = if item.can_sell {
                [0.55, 0.22, 0.18, 1.0] // Red sell
            } else {
                [0.15, 0.15, 0.15, 0.5] // Locked
            };
            canvas.draw_filled_rect(sell_btn_rect, sell_bg);
            canvas.draw_border(sell_btn_rect, 1.0, [0.65, 0.3, 0.3, 1.0]);

            let sell_lbl = if item.can_sell {
                format!("£{}", sell_val)
            } else {
                "LOCK".to_string()
            };
            canvas.draw_text(
                &sell_lbl,
                [sell_btn_rect.x + sell_btn_rect.width * 0.5, sell_btn_rect.y + sell_btn_rect.height * 0.5],
                10.0,
                [1.0, 1.0, 1.0, 1.0],
                TextAlignment::Center,
            );
        }

        // 7. Footer Toolbar: Action Buttons & Transaction Status Log
        let footer = self.footer_rect();
        canvas.draw_filled_rect(footer, [0.10, 0.11, 0.13, 1.0]);
        canvas.draw_border(footer, 1.0, [0.35, 0.38, 0.42, 1.0]);

        // Fill All Ammo Button
        let total_fill_cost = self.calculate_total_fill_ammo_cost();
        let fill_all_rect = Rect::new(footer.x + 12.0, footer.y + 10.0, 190.0, 34.0);
        let fill_all_bg = if total_fill_cost == 0 {
            [0.15, 0.16, 0.18, 0.6]
        } else if self.player_cash >= total_fill_cost {
            [0.18, 0.45, 0.25, 1.0]
        } else {
            [0.45, 0.30, 0.15, 1.0]
        };
        canvas.draw_filled_rect(fill_all_rect, fill_all_bg);
        canvas.draw_border(fill_all_rect, 1.0, [0.4, 0.6, 0.45, 1.0]);

        let fill_all_text = if total_fill_cost == 0 {
            "ALL AMMO FULL".to_string()
        } else {
            format!("FILL ALL AMMO (£{})", total_fill_cost)
        };
        canvas.draw_text(
            &fill_all_text,
            [fill_all_rect.x + fill_all_rect.width * 0.5, fill_all_rect.y + fill_all_rect.height * 0.5],
            12.0,
            [1.0, 1.0, 1.0, 1.0],
            TextAlignment::Center,
        );

        // Buy Selected Button
        let buy_sel_rect = Rect::new(footer.x + 210.0, footer.y + 10.0, 140.0, 34.0);
        canvas.draw_filled_rect(buy_sel_rect, [0.22, 0.28, 0.36, 1.0]);
        canvas.draw_border(buy_sel_rect, 1.0, [0.45, 0.55, 0.7, 1.0]);
        canvas.draw_text(
            "BUY SELECTED",
            [buy_sel_rect.x + buy_sel_rect.width * 0.5, buy_sel_rect.y + buy_sel_rect.height * 0.5],
            12.0,
            [1.0, 1.0, 1.0, 1.0],
            TextAlignment::Center,
        );

        // Sell Selected Button
        let sell_sel_rect = Rect::new(footer.x + 360.0, footer.y + 10.0, 140.0, 34.0);
        canvas.draw_filled_rect(sell_sel_rect, [0.45, 0.22, 0.20, 1.0]);
        canvas.draw_border(sell_sel_rect, 1.0, [0.7, 0.35, 0.35, 1.0]);
        canvas.draw_text(
            "SELL SELECTED",
            [sell_sel_rect.x + sell_sel_rect.width * 0.5, sell_sel_rect.y + sell_sel_rect.height * 0.5],
            12.0,
            [1.0, 1.0, 1.0, 1.0],
            TextAlignment::Center,
        );

        // Transaction status log line
        if let Some(ref msg) = self.last_status_message {
            let status_x = footer.x + 515.0;
            let status_color = if self.is_status_error {
                [1.0, 0.35, 0.35, 1.0] // Error Red
            } else {
                [0.85, 0.85, 0.40, 1.0] // Warning / Info Gold
            };
            canvas.draw_text(
                msg,
                [status_x, footer.y + footer.height * 0.5],
                11.0,
                status_color,
                TextAlignment::Left,
            );
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trader_shop_initial_state() {
        let page = TraderShopPage::new();
        assert_eq!(page.id(), "TraderShopPage");
        assert_eq!(page.player_cash, 1000);
        assert_eq!(page.player_armor, 50.0);
        assert_eq!(page.player_max_armor, 100.0);
        assert_eq!(page.player_perk, PerkType::Commando);
        assert_eq!(page.player_perk_level, 5);
        assert_eq!(page.player_max_weight, 15.0);
        // Default starter loadout: knife + 9mm (weight 0.0)
        assert_eq!(page.player_inventory.len(), 2);
        assert_eq!(page.player_weight, 0.0);
        assert!(!page.shop_catalog.is_empty());
    }

    #[test]
    fn test_perk_discount_calculations() {
        let mut page = TraderShopPage::new();
        page.set_perk(PerkType::Commando, 0);
        assert_eq!(page.get_perk_discount_percent(PerkType::Commando), 0);

        page.set_perk(PerkType::Commando, 1);
        assert_eq!(page.get_perk_discount_percent(PerkType::Commando), 10);

        page.set_perk(PerkType::Commando, 2);
        assert_eq!(page.get_perk_discount_percent(PerkType::Commando), 20);

        page.set_perk(PerkType::Commando, 3);
        assert_eq!(page.get_perk_discount_percent(PerkType::Commando), 30);

        page.set_perk(PerkType::Commando, 4);
        assert_eq!(page.get_perk_discount_percent(PerkType::Commando), 40);

        page.set_perk(PerkType::Commando, 5);
        assert_eq!(page.get_perk_discount_percent(PerkType::Commando), 50);

        page.set_perk(PerkType::Commando, 6);
        assert_eq!(page.get_perk_discount_percent(PerkType::Commando), 70);

        // Non-matching perk gets 0% discount
        assert_eq!(page.get_perk_discount_percent(PerkType::Berserker), 0);
        assert_eq!(page.get_perk_discount_percent(PerkType::Sharpshooter), 0);

        // Price check with Bullpup (£400 base)
        let bullpup = page.shop_catalog.iter().find(|w| w.id == "bullpup").unwrap();
        // At level 6 (70% off), Bullpup is £120
        assert_eq!(page.calculate_weapon_cost(bullpup), 120);

        // Non-perk weapon check (Katana £1000 base -> stays £1000)
        let katana = page.shop_catalog.iter().find(|w| w.id == "katana").unwrap();
        assert_eq!(page.calculate_weapon_cost(katana), 1000);
    }

    #[test]
    fn test_support_specialist_weight_bonus() {
        let mut page = TraderShopPage::new();
        page.set_perk(PerkType::Commando, 5);
        assert_eq!(page.player_max_weight, 15.0);

        page.set_perk(PerkType::SupportSpecialist, 0);
        assert_eq!(page.player_max_weight, 15.0);

        page.set_perk(PerkType::SupportSpecialist, 1);
        assert_eq!(page.player_max_weight, 16.0);

        page.set_perk(PerkType::SupportSpecialist, 6);
        assert_eq!(page.player_max_weight, 24.0);
    }

    #[test]
    fn test_buy_weapon_success_and_weight() {
        let mut page = TraderShopPage::new();
        page.player_cash = 1000;
        page.set_perk(PerkType::Commando, 5); // 50% discount

        // Bullpup: base £400 -> £200, weight 6.0kg
        let res = page.buy_weapon("bullpup");
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 200);
        assert_eq!(page.player_cash, 800);
        assert_eq!(page.player_weight, 6.0);
        assert_eq!(page.player_inventory.len(), 3);

        let inv_bullpup = page.player_inventory.iter().find(|w| w.id == "bullpup").unwrap();
        assert_eq!(inv_bullpup.name, "Bullpup");
        assert_eq!(inv_bullpup.current_ammo, 80); // 2 mags
    }

    #[test]
    fn test_buy_weapon_already_owned() {
        let mut page = TraderShopPage::new();
        page.player_cash = 2000;
        assert!(page.buy_weapon("bullpup").is_ok());

        // Attempting to buy again fails
        let res2 = page.buy_weapon("bullpup");
        assert!(matches!(res2, Err(TraderError::AlreadyOwned(_))));
    }

    #[test]
    fn test_buy_weapon_insufficient_funds() {
        let mut page = TraderShopPage::new();
        page.player_cash = 50;
        let res = page.buy_weapon("katana");
        assert!(matches!(res, Err(TraderError::InsufficientFunds { .. })));
    }

    #[test]
    fn test_buy_weapon_weight_limit_exceeded() {
        let mut page = TraderShopPage::new();
        page.player_cash = 10000;
        page.player_weight = 10.0;
        page.player_max_weight = 15.0;

        // AA-12 is 10.0kg -> 10.0 + 10.0 = 20.0 > 15.0
        let res = page.buy_weapon("aa12");
        assert!(matches!(res, Err(TraderError::WeightLimitExceeded { .. })));
    }

    #[test]
    fn test_sell_weapon_and_refund() {
        let mut page = TraderShopPage::new();
        page.player_cash = 2000;
        let _ = page.buy_weapon("bullpup").unwrap();
        assert_eq!(page.player_weight, 6.0);
        let cash_before = page.player_cash;

        // Bullpup base cost = 400. Resale at 75% = 300
        let res = page.sell_weapon("bullpup");
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 300);
        assert_eq!(page.player_cash, cash_before + 300);
        assert_eq!(page.player_weight, 0.0);
        assert!(!page.player_inventory.iter().any(|w| w.id == "bullpup"));
    }

    #[test]
    fn test_cannot_sell_starter_weapons() {
        let mut page = TraderShopPage::new();
        let res_knife = page.sell_weapon("knife");
        assert!(matches!(res_knife, Err(TraderError::CannotSellWeapon(_))));

        let res_pistol = page.sell_weapon("pistol_9mm");
        assert!(matches!(res_pistol, Err(TraderError::CannotSellWeapon(_))));
    }

    #[test]
    fn test_buy_ammo_and_fill() {
        let mut page = TraderShopPage::new();
        page.player_cash = 1000;
        page.set_perk(PerkType::Commando, 5); // 50% discount
        let _ = page.buy_weapon("bullpup").unwrap();

        // Bullpup current ammo is 80 / 400. Mag is 40. Base cost per mag is 20 -> 10 with 50% off
        let cash_before = page.player_cash;
        let ammo_res = page.buy_ammo("bullpup");
        assert!(ammo_res.is_ok());
        assert_eq!(ammo_res.unwrap(), 10);
        assert_eq!(page.player_cash, cash_before - 10);

        let inv_w = page.player_inventory.iter().find(|w| w.id == "bullpup").unwrap();
        assert_eq!(inv_w.current_ammo, 120);

        // Fill all ammo
        let fill_res = page.fill_all_ammo();
        assert!(fill_res.is_ok());
        let inv_w2 = page.player_inventory.iter().find(|w| w.id == "bullpup").unwrap();
        assert_eq!(inv_w2.current_ammo, 400);
    }

    #[test]
    fn test_buy_armor_and_medic_discount() {
        let mut page = TraderShopPage::new();
        page.player_armor = 50.0;
        page.player_cash = 500;
        page.set_perk(PerkType::Commando, 5);

        // 50 missing armor -> (50/100) * 300 = £150
        assert_eq!(page.calculate_armor_cost(), 150);
        let res = page.buy_armor();
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 150);
        assert_eq!(page.player_armor, 100.0);
        assert_eq!(page.player_cash, 350);

        // Full armor rejection
        let res_full = page.buy_armor();
        assert!(matches!(res_full, Err(TraderError::ArmorAlreadyFull)));

        // Field Medic discount test (lvl 6 gets 70% discount)
        page.player_armor = 0.0;
        page.set_perk(PerkType::FieldMedic, 6);
        // 100 missing armor -> 300 * 0.30 = £90
        assert_eq!(page.calculate_armor_cost(), 90);
    }

    #[test]
    fn test_gui_component_render_and_events() {
        let mut page = TraderShopPage::with_bounds(Rect::new(0.0, 0.0, 1024.0, 768.0));
        let mut canvas = Canvas::default();

        // Test render without crashing
        page.render(&mut canvas);
        assert!(!canvas.commands.is_empty());
        assert!(!canvas.vertices.is_empty());
        assert!(!canvas.indices.is_empty());

        // Test mouse event clicks on tab
        let tabs_rect = page.tabs_bar_rect();
        let click_tab_event = MouseEvent::Down {
            button: MouseButton::Left,
            x: tabs_rect.x + 10.0,
            y: tabs_rect.y + 10.0,
        };
        assert!(page.handle_mouse_event(&click_tab_event));

        // Test mouse wheel scroll event
        let shop_rect = page.shop_panel_rect();
        let scroll_event = MouseEvent::Scroll {
            delta: -1.0,
            x: shop_rect.x + 50.0,
            y: shop_rect.y + 50.0,
        };
        assert!(page.handle_mouse_event(&scroll_event));

        // Test downcasting
        assert!(page.as_any().is::<TraderShopPage>());
        assert!(page.as_any_mut().is::<TraderShopPage>());
    }
}
