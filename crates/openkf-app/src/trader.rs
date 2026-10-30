use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

pub const TRADER_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TraderMode {
    RetailVerified,
    AdapterVerified,
    #[default]
    GenericObservable,
    UnknownCustom,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Availability {
    Available,
    Locked,
    Hidden,
    OutOfStock,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NativeAction {
    BuyWeapon,
    BuyAmmo,
    BuyKevlar,
    BuyFirstAid,
    SellWeapon,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataSource {
    Live,
    NativeGui,
    Adapter,
    LocalMetadata,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Confidence {
    Verified,
    Observed,
    Indicative,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FieldProvenance {
    pub source: DataSource,
    pub confidence: Confidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemSource {
    Retail,
    Mod,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatDirection {
    HigherBetter,
    LowerBetter,
    Neutral,
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssetRef {
    pub package_id: String,
    pub object_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StatEntry {
    pub key: String,
    pub localized_label: Option<String>,
    pub raw_value: Option<String>,
    pub formatted_value: Option<String>,
    pub unit: Option<String>,
    pub direction: StatDirection,
    pub normalized_min: Option<f64>,
    pub normalized_max: Option<f64>,
    pub provenance: Option<FieldProvenance>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Discount {
    pub amount: Option<i64>,
    pub percent: Option<f64>,
    pub verified: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraderItem {
    pub item_id: String,
    pub pickup_class_id: Option<String>,
    pub weapon_class_id: Option<String>,
    pub display_name: String,
    pub verified_aliases: Vec<String>,
    pub item_type: Option<String>,
    pub description: Option<String>,
    pub primary_category_id: Option<String>,
    pub additional_category_ids: Vec<String>,
    pub perk_ids: Vec<String>,
    pub perk_association_known: bool,
    pub icon_ref: Option<AssetRef>,
    pub preview_ref: Option<AssetRef>,
    pub base_price: Option<i64>,
    pub effective_price: Option<i64>,
    pub ammo_price: Option<i64>,
    pub weight: Option<f64>,
    pub max_per_purchase: Option<u32>,
    pub owned_count: Option<u32>,
    pub availability: Availability,
    pub restriction_reasons: Vec<String>,
    pub discount: Option<Discount>,
    pub custom_stats: Vec<StatEntry>,
    pub tags: Vec<String>,
    pub item_source: ItemSource,
    pub provenance: BTreeMap<String, FieldProvenance>,
    pub action_binding_key: Option<String>,
    pub original_server_order: u32,
}

impl TraderItem {
    /// Constructs an observation row. Unknown game facts and actions remain absent.
    pub fn minimal(
        item_id: impl Into<String>,
        display_name: impl Into<String>,
        order: u32,
    ) -> Self {
        Self {
            item_id: item_id.into(),
            pickup_class_id: None,
            weapon_class_id: None,
            display_name: display_name.into(),
            verified_aliases: Vec::new(),
            item_type: None,
            description: None,
            primary_category_id: None,
            additional_category_ids: Vec::new(),
            perk_ids: Vec::new(),
            perk_association_known: false,
            icon_ref: None,
            preview_ref: None,
            base_price: None,
            effective_price: None,
            ammo_price: None,
            weight: None,
            max_per_purchase: None,
            owned_count: None,
            availability: Availability::Unknown,
            restriction_reasons: Vec::new(),
            discount: None,
            custom_stats: Vec::new(),
            tags: Vec::new(),
            item_source: ItemSource::Unknown,
            provenance: BTreeMap::new(),
            action_binding_key: None,
            original_server_order: order,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct InventoryEntry {
    pub instance_id: Option<String>,
    pub item_id: Option<String>,
    pub display_name: String,
    pub ammo_loaded: Option<u32>,
    pub ammo_reserve: Option<u32>,
    pub quantity: Option<u32>,
    pub actual_weight: Option<f64>,
    pub sell_value: Option<i64>,
    pub can_sell: Option<bool>,
    pub is_equipped: Option<bool>,
    pub custom_fields: Vec<StatEntry>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraderPerk {
    pub perk_id: String,
    pub class_id: Option<String>,
    pub name: String,
    pub icon_ref: Option<AssetRef>,
    pub current_level: Option<u32>,
    pub max_level: Option<u32>,
    pub level_progress: Option<f64>,
    pub description: Option<String>,
    pub provenance: BTreeMap<String, FieldProvenance>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraderCategory {
    pub category_id: String,
    pub display_name: String,
    pub icon_ref: Option<AssetRef>,
    pub parent_id: Option<String>,
    pub order_index: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wallet {
    pub amount: Option<i64>,
    pub currency_label: Option<String>,
    pub provenance: Option<FieldProvenance>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Capacity {
    pub used: Option<f64>,
    pub maximum: Option<f64>,
    pub unit: Option<String>,
    pub provenance: Option<FieldProvenance>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CapabilityReport {
    pub can_read_catalog: bool,
    pub can_observe_inventory: bool,
    pub can_open_native_fallback: bool,
    pub supported_actions: BTreeSet<NativeAction>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraderSnapshot {
    pub schema_version: u32,
    pub session_id: String,
    pub generation: u64,
    pub observed_at_monotonic_ms: u64,
    pub mode: TraderMode,
    pub trader_open: bool,
    pub trader_closes_in_seconds: Option<f64>,
    pub catalog_items: Vec<TraderItem>,
    pub categories: Vec<TraderCategory>,
    pub perks: Vec<TraderPerk>,
    pub inventory: Vec<InventoryEntry>,
    pub wallet: Wallet,
    pub capacity: Capacity,
    pub capabilities: CapabilityReport,
    pub source_version: Option<String>,
}

impl TraderSnapshot {
    pub fn empty(session_id: impl Into<String>, generation: u64, mode: TraderMode) -> Self {
        Self {
            schema_version: TRADER_SCHEMA_VERSION,
            session_id: session_id.into(),
            generation,
            observed_at_monotonic_ms: 0,
            mode,
            trader_open: false,
            trader_closes_in_seconds: None,
            catalog_items: Vec::new(),
            categories: Vec::new(),
            perks: Vec::new(),
            inventory: Vec::new(),
            wallet: Wallet::default(),
            capacity: Capacity::default(),
            capabilities: CapabilityReport::default(),
            source_version: None,
        }
    }

    pub fn validate(&self, limits: &ValidationLimits) -> Result<(), SnapshotError> {
        if self.schema_version != TRADER_SCHEMA_VERSION {
            return Err(SnapshotError::UnsupportedSchema(self.schema_version));
        }
        if self.session_id.trim().is_empty() {
            return Err(SnapshotError::EmptySessionId);
        }
        check_text_len("session_id", &self.session_id, limits.max_session_id_chars)?;
        check_collection("catalog_items", self.catalog_items.len(), limits.max_items)?;
        check_collection("categories", self.categories.len(), limits.max_categories)?;
        check_collection("perks", self.perks.len(), limits.max_perks)?;
        check_collection("inventory", self.inventory.len(), limits.max_inventory)?;

        if let Some(seconds) = self.trader_closes_in_seconds {
            check_finite("trader_closes_in_seconds", seconds)?;
            if seconds < 0.0 {
                return Err(SnapshotError::NegativeNumber("trader_closes_in_seconds"));
            }
        }
        if let Some(amount) = self.wallet.amount {
            if amount < 0 {
                return Err(SnapshotError::NegativePrice("wallet.amount".to_string()));
            }
        }
        for (name, value) in [
            ("capacity.used", self.capacity.used),
            ("capacity.maximum", self.capacity.maximum),
        ] {
            if let Some(value) = value {
                check_finite(name, value)?;
                if value < 0.0 {
                    return Err(SnapshotError::NegativeNumber(name));
                }
            }
        }
        let category_ids = unique_ids(
            self.categories
                .iter()
                .map(|category| category.category_id.as_str()),
            "category",
        )?;
        let perk_ids = unique_ids(self.perks.iter().map(|perk| perk.perk_id.as_str()), "perk")?;
        unique_ids(
            self.catalog_items.iter().map(|item| item.item_id.as_str()),
            "item",
        )?;

        for item in &self.catalog_items {
            check_text_len("display_name", &item.display_name, limits.max_name_chars)?;
            for alias in &item.verified_aliases {
                check_text_len("verified_aliases", alias, limits.max_name_chars)?;
            }
            if let Some(item_type) = &item.item_type {
                check_text_len("item_type", item_type, limits.max_name_chars)?;
            }
            for tag in &item.tags {
                check_text_len("tags", tag, limits.max_name_chars)?;
            }
            if let Some(description) = &item.description {
                check_text_len("description", description, limits.max_description_chars)?;
            }
            check_optional_id("pickup_class_id", item.pickup_class_id.as_deref(), limits)?;
            check_optional_id("weapon_class_id", item.weapon_class_id.as_deref(), limits)?;
            for (name, price) in [
                ("base_price", item.base_price),
                ("effective_price", item.effective_price),
                ("ammo_price", item.ammo_price),
            ] {
                if let Some(price) = price {
                    if price < 0 {
                        return Err(SnapshotError::NegativePrice(format!(
                            "{}.{}",
                            item.item_id, name
                        )));
                    }
                }
            }
            if let Some(weight) = item.weight {
                check_finite("item.weight", weight)?;
                if weight < 0.0 {
                    return Err(SnapshotError::NegativeNumber("item.weight"));
                }
            }
            if let Some(discount) = &item.discount {
                if let Some(amount) = discount.amount {
                    if amount < 0 {
                        return Err(SnapshotError::NegativePrice("discount.amount".to_string()));
                    }
                }
                if let Some(percent) = discount.percent {
                    check_finite("discount.percent", percent)?;
                }
            }
            if let Some(category_id) = &item.primary_category_id {
                if !category_ids.contains(category_id.as_str()) {
                    return Err(SnapshotError::InvalidReference("primary_category_id"));
                }
            }
            if item
                .additional_category_ids
                .iter()
                .any(|id| !category_ids.contains(id.as_str()))
            {
                return Err(SnapshotError::InvalidReference("additional_category_ids"));
            }
            if item
                .perk_ids
                .iter()
                .any(|id| !perk_ids.contains(id.as_str()))
            {
                return Err(SnapshotError::InvalidReference("perk_ids"));
            }
            check_stats(&item.custom_stats, limits)?;
        }

        for category in &self.categories {
            check_text_len(
                "category.display_name",
                &category.display_name,
                limits.max_name_chars,
            )?;
            if let Some(parent_id) = &category.parent_id {
                if !category_ids.contains(parent_id.as_str()) || parent_id == &category.category_id
                {
                    return Err(SnapshotError::InvalidReference("category.parent_id"));
                }
            }
        }
        for perk in &self.perks {
            check_text_len("perk.name", &perk.name, limits.max_name_chars)?;
            check_optional_id("perk.class_id", perk.class_id.as_deref(), limits)?;
            if let (Some(current), Some(maximum)) = (perk.current_level, perk.max_level) {
                if current > maximum {
                    return Err(SnapshotError::InvalidReference("perk.level"));
                }
            }
            if let Some(progress) = perk.level_progress {
                check_finite("perk.level_progress", progress)?;
            }
        }
        for entry in &self.inventory {
            check_text_len(
                "inventory.display_name",
                &entry.display_name,
                limits.max_name_chars,
            )?;
            check_optional_id(
                "inventory.instance_id",
                entry.instance_id.as_deref(),
                limits,
            )?;
            if let Some(weight) = entry.actual_weight {
                check_finite("inventory.actual_weight", weight)?;
                if weight < 0.0 {
                    return Err(SnapshotError::NegativeNumber("inventory.actual_weight"));
                }
            }
            if let Some(value) = entry.sell_value {
                if value < 0 {
                    return Err(SnapshotError::NegativePrice(
                        "inventory.sell_value".to_string(),
                    ));
                }
            }
            check_stats(&entry.custom_fields, limits)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    UnsupportedSchema(u32),
    EmptySessionId,
    EmptyIdentifier(&'static str),
    DuplicateItemId(String),
    DuplicateIdentifier(&'static str, String),
    CollectionLimit(&'static str),
    TextLimit(&'static str),
    NegativePrice(String),
    NegativeNumber(&'static str),
    NonFiniteNumber(&'static str),
    InvalidReference(&'static str),
    InvalidCapacity,
    StaleGeneration { current: u64, incoming: u64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidationLimits {
    pub max_items: usize,
    pub max_categories: usize,
    pub max_perks: usize,
    pub max_inventory: usize,
    pub max_stats_per_item: usize,
    pub max_session_id_chars: usize,
    pub max_name_chars: usize,
    pub max_description_chars: usize,
    pub max_identifier_chars: usize,
}

impl Default for ValidationLimits {
    fn default() -> Self {
        Self {
            max_items: 5_000,
            max_categories: 256,
            max_perks: 256,
            max_inventory: 512,
            max_stats_per_item: 128,
            max_session_id_chars: 128,
            max_name_chars: 512,
            max_description_chars: 4_096,
            max_identifier_chars: 1_024,
        }
    }
}

fn check_text_len(name: &'static str, value: &str, max: usize) -> Result<(), SnapshotError> {
    if value.chars().count() > max {
        Err(SnapshotError::TextLimit(name))
    } else {
        Ok(())
    }
}

fn check_optional_id(
    name: &'static str,
    value: Option<&str>,
    limits: &ValidationLimits,
) -> Result<(), SnapshotError> {
    if let Some(value) = value {
        check_text_len(name, value, limits.max_identifier_chars)?;
    }
    Ok(())
}

fn check_collection(name: &'static str, size: usize, max: usize) -> Result<(), SnapshotError> {
    if size > max {
        Err(SnapshotError::CollectionLimit(name))
    } else {
        Ok(())
    }
}

fn check_finite(name: &'static str, value: f64) -> Result<(), SnapshotError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(SnapshotError::NonFiniteNumber(name))
    }
}

fn is_trusted(provenance: FieldProvenance) -> bool {
    matches!(
        provenance.source,
        DataSource::Live | DataSource::NativeGui | DataSource::Adapter
    ) && matches!(
        provenance.confidence,
        Confidence::Verified | Confidence::Observed
    )
}

fn has_trusted_field(item: &TraderItem, field: &str) -> bool {
    item.provenance.get(field).copied().is_some_and(is_trusted)
}

fn unique_ids<'a>(
    ids: impl Iterator<Item = &'a str>,
    kind: &'static str,
) -> Result<BTreeSet<&'a str>, SnapshotError> {
    let mut unique = BTreeSet::new();
    for id in ids {
        if id.trim().is_empty() {
            return Err(SnapshotError::EmptyIdentifier(kind));
        }
        if !unique.insert(id) {
            if kind == "item" {
                return Err(SnapshotError::DuplicateItemId(id.to_string()));
            }
            return Err(SnapshotError::DuplicateIdentifier(kind, id.to_string()));
        }
    }
    Ok(unique)
}

fn check_stats(stats: &[StatEntry], limits: &ValidationLimits) -> Result<(), SnapshotError> {
    check_collection("custom_stats", stats.len(), limits.max_stats_per_item)?;
    for stat in stats {
        check_text_len("stat.key", &stat.key, limits.max_identifier_chars)?;
        for (name, value) in [
            ("stat.normalized_min", stat.normalized_min),
            ("stat.normalized_max", stat.normalized_max),
        ] {
            if let Some(value) = value {
                check_finite(name, value)?;
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Default)]
pub struct NormalizedCatalogStore {
    current: Option<Arc<TraderSnapshot>>,
    normalized_search_index: Vec<String>,
    normalized_technical_index: Vec<String>,
}

impl NormalizedCatalogStore {
    pub fn current(&self) -> Option<&TraderSnapshot> {
        self.current.as_deref()
    }

    pub fn current_shared(&self) -> Option<Arc<TraderSnapshot>> {
        self.current.as_ref().map(Arc::clone)
    }

    pub fn replace(&mut self, snapshot: TraderSnapshot) -> Result<(), SnapshotError> {
        self.replace_with_limits(snapshot, &ValidationLimits::default())
    }

    pub fn replace_with_limits(
        &mut self,
        snapshot: TraderSnapshot,
        limits: &ValidationLimits,
    ) -> Result<(), SnapshotError> {
        snapshot.validate(limits)?;
        if let Some(current) = &self.current {
            if current.session_id == snapshot.session_id
                && snapshot.generation <= current.generation
            {
                return Err(SnapshotError::StaleGeneration {
                    current: current.generation,
                    incoming: snapshot.generation,
                });
            }
        }
        self.normalized_search_index = snapshot
            .catalog_items
            .iter()
            .map(|item| {
                let mut terms = Vec::new();
                terms.push(item.display_name.as_str());
                terms.extend(item.verified_aliases.iter().map(String::as_str));
                terms.extend(item.tags.iter().map(String::as_str));
                if let Some(item_type) = item.item_type.as_deref() {
                    terms.push(item_type);
                }
                normalize_for_search(&terms.join(" "))
            })
            .collect();
        self.normalized_technical_index = snapshot
            .catalog_items
            .iter()
            .map(|item| {
                normalize_for_search(
                    &[
                        item.pickup_class_id.as_deref().unwrap_or_default(),
                        item.weapon_class_id.as_deref().unwrap_or_default(),
                    ]
                    .join(" "),
                )
            })
            .collect();
        self.current = Some(Arc::new(snapshot));
        Ok(())
    }

    pub fn query<'a>(&'a self, query: &CatalogQuery) -> Vec<&'a TraderItem> {
        self.query_report(query).items
    }

    pub fn query_report<'a>(&'a self, query: &CatalogQuery) -> CatalogQueryResult<'a> {
        let Some(snapshot) = self.current.as_deref() else {
            return CatalogQueryResult::default();
        };
        let mut inactive_filters = Vec::new();
        let wallet = if query.affordable_only {
            match (snapshot.wallet.amount, snapshot.wallet.provenance) {
                (Some(amount), Some(provenance)) if is_trusted(provenance) => Some(amount),
                _ => {
                    inactive_filters.push(InactiveFilter::AffordableWalletUnverified);
                    None
                }
            }
        } else {
            None
        };
        let capacity = if query.fits_weight_only {
            match (
                snapshot.capacity.used,
                snapshot.capacity.maximum,
                snapshot.capacity.provenance,
            ) {
                (Some(used), Some(maximum), Some(provenance)) if is_trusted(provenance) => {
                    Some((used, maximum))
                }
                _ => {
                    inactive_filters.push(InactiveFilter::CapacityUnverified);
                    None
                }
            }
        } else {
            None
        };
        if (query.affordable_only && wallet.is_none())
            || (query.fits_weight_only && capacity.is_none())
        {
            return CatalogQueryResult {
                items: Vec::new(),
                inactive_filters,
            };
        }
        let needle = normalize_for_search(&query.search_text);
        let mut matched = snapshot
            .catalog_items
            .iter()
            .enumerate()
            .filter(|(index, item)| {
                if !needle.is_empty()
                    && !self.normalized_search_index[*index].contains(&needle)
                    && !(query.include_technical_fields
                        && self.normalized_technical_index[*index].contains(&needle))
                {
                    return false;
                }
                if query.category_id.is_some() || !query.category_ids.is_empty() {
                    let category_matches = query
                        .category_id
                        .iter()
                        .chain(query.category_ids.iter())
                        .any(|category_id| {
                            item.primary_category_id.as_ref() == Some(category_id)
                                || item.additional_category_ids.contains(category_id)
                        });
                    if !category_matches {
                        return false;
                    }
                }
                if let Some(perk_id) = &query.perk_id {
                    if !item.perk_association_known || !item.perk_ids.contains(perk_id) {
                        return false;
                    }
                }
                let perk_matches = match &query.perk_filter {
                    PerkFilter::Any => true,
                    PerkFilter::ForPerk(perk_id) => {
                        item.perk_association_known && item.perk_ids.contains(perk_id)
                    }
                    PerkFilter::CrossPerk => item.perk_association_known && item.perk_ids.len() > 1,
                    PerkFilter::UnknownAssociation => !item.perk_association_known,
                };
                if !perk_matches {
                    return false;
                }
                if query.available_only
                    && (item.availability != Availability::Available
                        || !has_trusted_field(item, "availability"))
                {
                    return false;
                }
                if query.custom_only
                    && (item.item_source != ItemSource::Mod
                        || !has_trusted_field(item, "item_source"))
                {
                    return false;
                }
                if query.official_only
                    && (item.item_source != ItemSource::Retail
                        || !has_trusted_field(item, "item_source"))
                {
                    return false;
                }
                if query.owned_only
                    && (item.owned_count.unwrap_or(0) == 0
                        || !has_trusted_field(item, "owned_count"))
                {
                    return false;
                }
                if query.favorite_only && !query.favorite_item_ids.contains(&item.item_id) {
                    return false;
                }
                if let Some(wallet_amount) = wallet {
                    if !has_trusted_field(item, "effective_price")
                        || item
                            .effective_price
                            .is_none_or(|price| price > wallet_amount)
                    {
                        return false;
                    }
                }
                if let Some((used, maximum)) = capacity {
                    if !has_trusted_field(item, "weight")
                        || item.weight.is_none_or(|weight| weight > maximum - used)
                    {
                        return false;
                    }
                }
                if query.available_only && item.availability != Availability::Available {
                    return false;
                }
                true
            })
            .map(|(_, item)| item)
            .collect::<Vec<_>>();
        matched.sort_by(|left, right| compare_items(left, right, query.sort));
        CatalogQueryResult {
            items: matched,
            inactive_filters,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InactiveFilter {
    AffordableWalletUnverified,
    CapacityUnverified,
}

#[derive(Debug)]
#[derive(Default)]
pub struct CatalogQueryResult<'a> {
    pub items: Vec<&'a TraderItem>,
    pub inactive_filters: Vec<InactiveFilter>,
}


#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CatalogQuery {
    pub search_text: String,
    pub category_id: Option<String>,
    pub category_ids: Vec<String>,
    pub perk_id: Option<String>,
    pub perk_filter: PerkFilter,
    pub available_only: bool,
    pub include_technical_fields: bool,
    pub custom_only: bool,
    pub official_only: bool,
    pub owned_only: bool,
    pub favorite_only: bool,
    pub favorite_item_ids: BTreeSet<String>,
    pub affordable_only: bool,
    pub fits_weight_only: bool,
    pub sort: CatalogSort,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum PerkFilter {
    #[default]
    Any,
    ForPerk(String),
    CrossPerk,
    UnknownAssociation,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CatalogSort {
    #[default]
    ServerOrder,
    NameAscending,
    NameDescending,
    PriceAscending,
    PriceDescending,
    WeightAscending,
    WeightDescending,
    AvailableFirst,
    OwnedFirst,
}

fn compare_items(left: &TraderItem, right: &TraderItem, sort: CatalogSort) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let primary = match sort {
        CatalogSort::ServerOrder => Ordering::Equal,
        CatalogSort::NameAscending => left
            .display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase()),
        CatalogSort::NameDescending => right
            .display_name
            .to_lowercase()
            .cmp(&left.display_name.to_lowercase()),
        CatalogSort::PriceAscending => {
            compare_optional(left.effective_price, right.effective_price, false)
        }
        CatalogSort::PriceDescending => {
            compare_optional(left.effective_price, right.effective_price, true)
        }
        CatalogSort::WeightAscending => compare_optional(left.weight, right.weight, false),
        CatalogSort::WeightDescending => compare_optional(left.weight, right.weight, true),
        CatalogSort::AvailableFirst => {
            availability_rank(left.availability).cmp(&availability_rank(right.availability))
        }
        CatalogSort::OwnedFirst => right
            .owned_count
            .unwrap_or(0)
            .cmp(&left.owned_count.unwrap_or(0)),
    };
    primary
        .then_with(|| left.original_server_order.cmp(&right.original_server_order))
        .then_with(|| left.item_id.cmp(&right.item_id))
}

fn compare_optional<T: PartialOrd + Copy>(
    left: Option<T>,
    right: Option<T>,
    descending: bool,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (left, right) {
        (Some(left), Some(right)) => {
            let ordering = left.partial_cmp(&right).unwrap_or(Ordering::Equal);
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn availability_rank(value: Availability) -> u8 {
    match value {
        Availability::Available => 0,
        Availability::Unknown => 1,
        Availability::OutOfStock => 2,
        Availability::Locked => 3,
        Availability::Hidden => 4,
    }
}

fn normalize_for_search(value: &str) -> String {
    value
        .nfd()
        .filter(|character| !is_combining_mark(*character))
        .flat_map(char::to_lowercase)
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionRequest {
    pub session_id: String,
    pub generation: u64,
    pub item_id: String,
    pub action: NativeAction,
    pub quantity: u32,
}

impl ActionRequest {
    pub fn new(
        session_id: impl Into<String>,
        generation: u64,
        item_id: impl Into<String>,
        action: NativeAction,
        quantity: u32,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            generation,
            item_id: item_id.into(),
            action,
            quantity,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionPermit {
    pub command_id: u64,
    pub session_id: String,
    pub generation: u64,
    pub item_id: String,
    pub action: NativeAction,
    pub quantity: u32,
    pub native_binding_key: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionStatus {
    Pending,
    Confirmed,
    Rejected,
    Uncertain,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeObservation {
    AuthoritativeSuccess,
    AuthoritativeRejection,
    NoReliableResult,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GatewayError {
    StaleSelection,
    TraderClosed,
    UnknownCustomUi,
    ItemNotFound,
    UnsupportedAction,
    MissingNativeBinding,
    ItemUnavailable,
    InvalidQuantity,
    DuplicateAction,
}

#[derive(Clone, Debug)]
struct CommandRecord {
    request: ActionRequest,
    status: ActionStatus,
    created_at_ms: u64,
}

#[derive(Clone, Debug, Default)]
pub struct TraderCommandGateway {
    next_command_id: u64,
    commands: BTreeMap<u64, CommandRecord>,
}

impl TraderCommandGateway {
    /// Issues permission for an already-known native binding; it never dispatches gameplay code.
    pub fn prepare(
        &mut self,
        snapshot: &TraderSnapshot,
        request: &ActionRequest,
        now_ms: u64,
    ) -> Result<ActionPermit, GatewayError> {
        if request.session_id != snapshot.session_id || request.generation != snapshot.generation {
            return Err(GatewayError::StaleSelection);
        }
        if !snapshot.trader_open {
            return Err(GatewayError::TraderClosed);
        }
        if snapshot.mode == TraderMode::UnknownCustom {
            return Err(GatewayError::UnknownCustomUi);
        }
        if request.quantity == 0 {
            return Err(GatewayError::InvalidQuantity);
        }
        if !snapshot
            .capabilities
            .supported_actions
            .contains(&request.action)
        {
            return Err(GatewayError::UnsupportedAction);
        }
        let item = snapshot
            .catalog_items
            .iter()
            .find(|item| item.item_id == request.item_id)
            .ok_or(GatewayError::ItemNotFound)?;
        if matches!(
            item.availability,
            Availability::Locked | Availability::Hidden | Availability::OutOfStock
        ) {
            return Err(GatewayError::ItemUnavailable);
        }
        if item
            .max_per_purchase
            .is_some_and(|maximum| request.quantity > maximum)
        {
            return Err(GatewayError::InvalidQuantity);
        }
        let binding_key = item
            .action_binding_key
            .as_ref()
            .filter(|key| !key.trim().is_empty())
            .cloned()
            .ok_or(GatewayError::MissingNativeBinding)?;
        if self.commands.values().any(|record| {
            record.request.session_id == request.session_id
                && record.request.item_id == request.item_id
                && record.request.action == request.action
                && matches!(
                    record.status,
                    ActionStatus::Pending | ActionStatus::Uncertain
                )
        }) {
            return Err(GatewayError::DuplicateAction);
        }

        self.next_command_id = self.next_command_id.wrapping_add(1).max(1);
        let command_id = self.next_command_id;
        self.commands.insert(
            command_id,
            CommandRecord {
                request: request.clone(),
                status: ActionStatus::Pending,
                created_at_ms: now_ms,
            },
        );
        Ok(ActionPermit {
            command_id,
            session_id: request.session_id.clone(),
            generation: request.generation,
            item_id: request.item_id.clone(),
            action: request.action,
            quantity: request.quantity,
            native_binding_key: binding_key,
        })
    }

    /// Accepts only an explicit authoritative result; weak state deltas stay uncertain.
    pub fn reconcile(
        &mut self,
        command_id: u64,
        observation: NativeObservation,
    ) -> Option<ActionStatus> {
        let record = self.commands.get_mut(&command_id)?;
        if matches!(
            record.status,
            ActionStatus::Confirmed | ActionStatus::Rejected
        ) {
            return Some(record.status);
        }
        record.status = match observation {
            NativeObservation::AuthoritativeSuccess => ActionStatus::Confirmed,
            NativeObservation::AuthoritativeRejection => ActionStatus::Rejected,
            NativeObservation::NoReliableResult => ActionStatus::Uncertain,
        };
        Some(record.status)
    }

    /// Moves expired requests to UNCERTAIN. No request is retried automatically.
    pub fn expire(&mut self, now_ms: u64, timeout_ms: u64) {
        for record in self.commands.values_mut() {
            if record.status == ActionStatus::Pending
                && now_ms.saturating_sub(record.created_at_ms) >= timeout_ms
            {
                record.status = ActionStatus::Uncertain;
            }
        }
    }

    /// Closing a session leaves unresolved work uncertain and blocks re-clicks in that session.
    pub fn close_session(&mut self, session_id: &str) {
        for record in self.commands.values_mut() {
            if record.request.session_id == session_id && record.status == ActionStatus::Pending {
                record.status = ActionStatus::Uncertain;
            }
        }
    }

    pub fn status(&self, command_id: u64) -> Option<ActionStatus> {
        self.commands.get(&command_id).map(|record| record.status)
    }

    pub fn pending_count(&self) -> usize {
        self.commands
            .values()
            .filter(|record| record.status == ActionStatus::Pending)
            .count()
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TraderField {
    Catalog,
    Categories,
    Perks,
    Inventory,
    Wallet,
    Capacity,
    Prices,
    Weights,
    Availability,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionProbe {
    pub game_build: String,
    pub loaded_mod_packages: BTreeMap<String, String>,
    pub observed_class_signatures: BTreeSet<String>,
    pub stable_catalog_observed: bool,
    pub observed_fields: BTreeSet<TraderField>,
    pub custom_trader_ui: bool,
    pub native_fallback_available: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdapterDescriptor {
    pub adapter_id: String,
    pub tested_game_build: String,
    pub package_id: String,
    pub supported_package_versions: BTreeSet<String>,
    pub required_class_signatures: BTreeSet<String>,
    pub mapping_evidence: Vec<String>,
    pub supported_fields: BTreeSet<TraderField>,
    pub supported_actions: BTreeSet<NativeAction>,
    pub callback_provenance: BTreeMap<NativeAction, String>,
    pub can_open_native_fallback: bool,
}

impl AdapterDescriptor {
    pub fn new(
        adapter_id: impl Into<String>,
        tested_game_build: impl Into<String>,
        package_id: impl Into<String>,
    ) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            tested_game_build: tested_game_build.into(),
            package_id: package_id.into(),
            supported_package_versions: BTreeSet::new(),
            required_class_signatures: BTreeSet::new(),
            mapping_evidence: Vec::new(),
            supported_fields: BTreeSet::new(),
            supported_actions: BTreeSet::new(),
            callback_provenance: BTreeMap::new(),
            can_open_native_fallback: false,
        }
    }

    fn matches(&self, probe: &SessionProbe) -> bool {
        probe.game_build == self.tested_game_build
            && probe
                .loaded_mod_packages
                .get(&self.package_id)
                .is_some_and(|version| self.supported_package_versions.contains(version))
            && self
                .required_class_signatures
                .iter()
                .all(|signature| probe.observed_class_signatures.contains(signature))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibilityMode {
    RetailVerified,
    AdapterVerified,
    GenericObservable,
    UnknownCustom,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibilitySelection {
    pub mode: CompatibilityMode,
    pub adapter_id: Option<String>,
    pub supported_fields: BTreeSet<TraderField>,
    pub supported_actions: BTreeSet<NativeAction>,
    pub native_fallback_available: bool,
}

impl CompatibilitySelection {
    fn unavailable() -> Self {
        Self {
            mode: CompatibilityMode::Unavailable,
            adapter_id: None,
            supported_fields: BTreeSet::new(),
            supported_actions: BTreeSet::new(),
            native_fallback_available: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdapterError {
    MissingEvidence,
    DuplicateAdapter,
}

#[derive(Clone, Debug, Default)]
pub struct CompatibilityRegistry {
    retail_game_builds: BTreeSet<String>,
    adapters: Vec<AdapterDescriptor>,
}

impl CompatibilityRegistry {
    pub fn register_retail_build(&mut self, game_build: impl Into<String>) {
        self.retail_game_builds.insert(game_build.into());
    }

    pub fn register_adapter(&mut self, descriptor: AdapterDescriptor) -> Result<(), AdapterError> {
        let has_complete_signature = !descriptor.adapter_id.trim().is_empty()
            && !descriptor.tested_game_build.trim().is_empty()
            && !descriptor.package_id.trim().is_empty()
            && !descriptor.supported_package_versions.is_empty()
            && !descriptor.required_class_signatures.is_empty();
        let has_evidence = !descriptor.mapping_evidence.is_empty()
            && descriptor
                .mapping_evidence
                .iter()
                .all(|entry| !entry.trim().is_empty());
        let actions_have_provenance = descriptor.supported_actions.iter().all(|action| {
            descriptor
                .callback_provenance
                .get(action)
                .is_some_and(|evidence| !evidence.trim().is_empty())
        });
        if !has_complete_signature || !has_evidence || !actions_have_provenance {
            return Err(AdapterError::MissingEvidence);
        }
        if self
            .adapters
            .iter()
            .any(|adapter| adapter.adapter_id == descriptor.adapter_id)
        {
            return Err(AdapterError::DuplicateAdapter);
        }
        self.adapters.push(descriptor);
        Ok(())
    }

    pub fn detect(&self, probe: &SessionProbe) -> CompatibilitySelection {
        if let Some(adapter) = self.adapters.iter().find(|adapter| adapter.matches(probe)) {
            return CompatibilitySelection {
                mode: CompatibilityMode::AdapterVerified,
                adapter_id: Some(adapter.adapter_id.clone()),
                supported_fields: adapter.supported_fields.clone(),
                supported_actions: adapter.supported_actions.clone(),
                native_fallback_available: adapter.can_open_native_fallback
                    && probe.native_fallback_available,
            };
        }
        if probe.custom_trader_ui {
            if probe.native_fallback_available {
                return CompatibilitySelection {
                    mode: CompatibilityMode::UnknownCustom,
                    adapter_id: None,
                    supported_fields: BTreeSet::new(),
                    supported_actions: BTreeSet::new(),
                    native_fallback_available: true,
                };
            }
            return CompatibilitySelection::unavailable();
        }
        if probe.loaded_mod_packages.is_empty()
            && self.retail_game_builds.contains(&probe.game_build)
        {
            let fields = [
                TraderField::Catalog,
                TraderField::Categories,
                TraderField::Perks,
                TraderField::Inventory,
                TraderField::Wallet,
                TraderField::Capacity,
                TraderField::Prices,
                TraderField::Weights,
                TraderField::Availability,
            ]
            .into_iter()
            .collect();
            let actions = [
                NativeAction::BuyWeapon,
                NativeAction::BuyAmmo,
                NativeAction::BuyKevlar,
                NativeAction::BuyFirstAid,
                NativeAction::SellWeapon,
            ]
            .into_iter()
            .collect();
            return CompatibilitySelection {
                mode: CompatibilityMode::RetailVerified,
                adapter_id: Some("retail-kf1".to_string()),
                supported_fields: fields,
                supported_actions: actions,
                native_fallback_available: probe.native_fallback_available,
            };
        }
        if probe.stable_catalog_observed {
            return CompatibilitySelection {
                mode: CompatibilityMode::GenericObservable,
                adapter_id: None,
                supported_fields: probe.observed_fields.clone(),
                supported_actions: BTreeSet::new(),
                native_fallback_available: probe.native_fallback_available,
            };
        }
        CompatibilitySelection::unavailable()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionOutcome {
    NoSelection,
    Preserved,
    TemporarilyHidden,
    NeighborSelected {
        removed_item_id: String,
        selected_item_id: String,
    },
    Cleared {
        removed_item_id: String,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TraderSelectionModel {
    selected_item_id: Option<String>,
    last_visible_index: usize,
}

impl TraderSelectionModel {
    pub fn selected_item_id(&self) -> Option<&str> {
        self.selected_item_id.as_deref()
    }

    pub fn select(&mut self, item_id: &str, visible_items: &[&TraderItem]) -> bool {
        let Some(index) = visible_items
            .iter()
            .position(|item| item.item_id == item_id)
        else {
            return false;
        };
        self.selected_item_id = Some(item_id.to_string());
        self.last_visible_index = index;
        true
    }

    pub fn reconcile(
        &mut self,
        snapshot: &TraderSnapshot,
        visible_items: &[&TraderItem],
    ) -> SelectionOutcome {
        let Some(selected_item_id) = self.selected_item_id.clone() else {
            return SelectionOutcome::NoSelection;
        };
        if snapshot
            .catalog_items
            .iter()
            .any(|item| item.item_id == selected_item_id)
        {
            if let Some(index) = visible_items
                .iter()
                .position(|item| item.item_id == selected_item_id)
            {
                self.last_visible_index = index;
                return SelectionOutcome::Preserved;
            }
            return SelectionOutcome::TemporarilyHidden;
        }

        if visible_items.is_empty() {
            self.selected_item_id = None;
            return SelectionOutcome::Cleared {
                removed_item_id: selected_item_id,
            };
        }

        let index = self.last_visible_index.min(visible_items.len() - 1);
        let neighbor_id = visible_items[index].item_id.clone();
        self.selected_item_id = Some(neighbor_id.clone());
        self.last_visible_index = index;
        SelectionOutcome::NeighborSelected {
            removed_item_id: selected_item_id,
            selected_item_id: neighbor_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(item_id: &str, display_name: &str, order: u32) -> TraderItem {
        TraderItem::minimal(item_id, display_name, order)
    }

    fn purchasable_snapshot(session_id: &str, generation: u64) -> TraderSnapshot {
        let mut weapon = item("retail:KFMod.AK47Pickup", "AK-47", 0);
        weapon.availability = Availability::Available;
        weapon.effective_price = Some(200);
        weapon.action_binding_key = Some("retail:ServerBuyWeapon".to_string());
        let mut snapshot = snapshot(vec![weapon], session_id, generation);
        snapshot.mode = TraderMode::RetailVerified;
        snapshot.trader_open = true;
        snapshot
            .capabilities
            .supported_actions
            .insert(NativeAction::BuyWeapon);
        snapshot
    }

    fn snapshot(items: Vec<TraderItem>, session_id: &str, generation: u64) -> TraderSnapshot {
        let mut snapshot =
            TraderSnapshot::empty(session_id, generation, TraderMode::GenericObservable);
        snapshot.catalog_items = items;
        snapshot
    }

    #[test]
    fn unknown_fields_stay_unknown_and_do_not_gain_an_action() {
        let unknown = item("mod:SomePack.Pickup", "Workshop weapon", 0);
        assert_eq!(unknown.effective_price, None);
        assert_eq!(unknown.weight, None);
        assert_eq!(unknown.action_binding_key, None);
        assert_eq!(unknown.availability, Availability::Unknown);
        assert!(snapshot(vec![unknown], "session-a", 1)
            .validate(&ValidationLimits::default())
            .is_ok());
    }

    #[test]
    fn identical_names_with_distinct_ids_remain_separate_items() {
        let catalog = snapshot(
            vec![
                item("pack-a:Weapon.Pickup", "Repeater", 0),
                item("pack-b:Weapon.Pickup", "Repeater", 1),
            ],
            "session-a",
            1,
        );
        let mut store = NormalizedCatalogStore::default();
        store.replace(catalog).unwrap();
        assert_eq!(store.query(&CatalogQuery::default()).len(), 2);
    }

    #[test]
    fn duplicate_item_ids_are_rejected() {
        let catalog = snapshot(
            vec![
                item("retail:KFMod.AK47Pickup", "AK-47", 0),
                item("retail:KFMod.AK47Pickup", "AK-47 variant", 1),
            ],
            "session-a",
            1,
        );
        assert_eq!(
            catalog.validate(&ValidationLimits::default()),
            Err(SnapshotError::DuplicateItemId(
                "retail:KFMod.AK47Pickup".to_string()
            ))
        );
    }

    #[test]
    fn invalid_prices_and_non_finite_weights_are_rejected() {
        let mut negative_price = item("retail:Weapon.Pickup", "Weapon", 0);
        negative_price.effective_price = Some(-1);
        assert!(matches!(
            snapshot(vec![negative_price], "session-a", 1).validate(&ValidationLimits::default()),
            Err(SnapshotError::NegativePrice(_))
        ));

        let mut invalid_weight = item("retail:Weapon.Pickup", "Weapon", 0);
        invalid_weight.weight = Some(f64::NAN);
        assert!(matches!(
            snapshot(vec![invalid_weight], "session-a", 1).validate(&ValidationLimits::default()),
            Err(SnapshotError::NonFiniteNumber(_))
        ));
    }

    #[test]
    fn configurable_collection_caps_reject_oversized_snapshot() {
        let catalog = snapshot(
            vec![item("a:One", "One", 0), item("b:Two", "Two", 1)],
            "session-a",
            1,
        );
        let limits = ValidationLimits {
            max_items: 1,
            ..ValidationLimits::default()
        };
        assert!(matches!(
            catalog.validate(&limits),
            Err(SnapshotError::CollectionLimit("catalog_items"))
        ));
    }

    #[test]
    fn store_rejects_stale_generation_without_replacing_current_snapshot() {
        let mut store = NormalizedCatalogStore::default();
        store.replace(snapshot(vec![], "session-a", 4)).unwrap();
        assert!(matches!(
            store.replace(snapshot(vec![], "session-a", 3)),
            Err(SnapshotError::StaleGeneration { .. })
        ));
        assert_eq!(store.current().unwrap().generation, 4);
    }

    #[test]
    fn a_new_session_can_restart_its_generation_counter() {
        let mut store = NormalizedCatalogStore::default();
        store.replace(snapshot(vec![], "server-a", 12)).unwrap();
        store.replace(snapshot(vec![], "server-b", 1)).unwrap();
        assert_eq!(store.current().unwrap().session_id, "server-b");
        assert_eq!(store.current().unwrap().generation, 1);
    }

    #[test]
    fn search_is_case_insensitive_and_preserves_distinct_ids() {
        let mut store = NormalizedCatalogStore::default();
        store
            .replace(snapshot(
                vec![
                    item("pack-a:Weapon.One", "Éclair Rifle", 0),
                    item("pack-b:Weapon.Two", "ÉCLAIR Pistol", 1),
                    item("pack-c:Weapon.Three", "Other", 2),
                ],
                "session-a",
                1,
            ))
            .unwrap();
        let query = CatalogQuery {
            search_text: "eclair".to_string(),
            ..CatalogQuery::default()
        };
        assert_eq!(store.query(&query).len(), 2);
    }

    #[test]
    fn unknown_prices_sort_last_in_both_directions() {
        let mut store = NormalizedCatalogStore::default();
        let mut known_low = item("retail:Low", "Low", 0);
        known_low.effective_price = Some(100);
        let mut unknown = item("mod:Unknown", "Unknown", 1);
        unknown.effective_price = None;
        let mut known_high = item("retail:High", "High", 2);
        known_high.effective_price = Some(900);
        store
            .replace(snapshot(
                vec![known_low, unknown, known_high],
                "session-a",
                1,
            ))
            .unwrap();

        for sort in [CatalogSort::PriceAscending, CatalogSort::PriceDescending] {
            let ids = store
                .query(&CatalogQuery {
                    sort,
                    ..CatalogQuery::default()
                })
                .iter()
                .map(|item| item.item_id.as_str())
                .collect::<Vec<_>>();
            assert_eq!(ids.last(), Some(&"mod:Unknown"));
        }
    }

    #[test]
    fn equal_sort_keys_use_server_order_then_item_id() {
        let mut store = NormalizedCatalogStore::default();
        let mut z = item("pack-z:Weapon", "Same", 1);
        z.effective_price = Some(200);
        let mut a_late = item("pack-a:Weapon", "Same", 2);
        a_late.effective_price = Some(200);
        let mut a_early = item("pack-b:Weapon", "Same", 0);
        a_early.effective_price = Some(200);
        store
            .replace(snapshot(vec![z, a_late, a_early], "session-a", 1))
            .unwrap();
        let ids = store
            .query(&CatalogQuery {
                sort: CatalogSort::PriceAscending,
                ..CatalogQuery::default()
            })
            .iter()
            .map(|item| item.item_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["pack-b:Weapon", "pack-z:Weapon", "pack-a:Weapon"]);
    }

    #[test]
    fn search_uses_verified_aliases_and_tags_and_hides_class_names_by_default() {
        let mut store = NormalizedCatalogStore::default();
        let mut weapon = item("mod:Fire.Pickup", "Fire Weapon", 0);
        weapon.verified_aliases.push("M1 Flamethrower".to_string());
        weapon.tags.push("Firebug".to_string());
        weapon.pickup_class_id = Some("FirePack.M1Pickup".to_string());
        store
            .replace(snapshot(vec![weapon], "session-a", 1))
            .unwrap();

        for search_text in ["M1 Flame", "Firebug"] {
            assert_eq!(
                store
                    .query(&CatalogQuery {
                        search_text: search_text.to_string(),
                        ..CatalogQuery::default()
                    })
                    .len(),
                1
            );
        }
        assert!(store
            .query(&CatalogQuery {
                search_text: "FirePack.M1Pickup".to_string(),
                ..CatalogQuery::default()
            })
            .is_empty());
        assert_eq!(
            store
                .query(&CatalogQuery {
                    search_text: "FirePack.M1Pickup".to_string(),
                    include_technical_fields: true,
                    ..CatalogQuery::default()
                })
                .len(),
            1
        );
    }

    #[test]
    fn action_requires_open_trader_exact_selection_and_verified_native_binding() {
        let mut gateway = TraderCommandGateway::default();
        let mut closed = purchasable_snapshot("server-a", 4);
        closed.trader_open = false;
        let request = ActionRequest::new(
            "server-a",
            4,
            "retail:KFMod.AK47Pickup",
            NativeAction::BuyWeapon,
            1,
        );
        assert_eq!(
            gateway.prepare(&closed, &request, 100),
            Err(GatewayError::TraderClosed)
        );

        let mut unknown = purchasable_snapshot("server-a", 4);
        unknown.mode = TraderMode::UnknownCustom;
        assert_eq!(
            gateway.prepare(&unknown, &request, 100),
            Err(GatewayError::UnknownCustomUi)
        );

        let mut no_binding = purchasable_snapshot("server-a", 4);
        no_binding.catalog_items[0].action_binding_key = None;
        assert_eq!(
            gateway.prepare(&no_binding, &request, 100),
            Err(GatewayError::MissingNativeBinding)
        );

        let stale = ActionRequest::new(
            "server-a",
            3,
            "retail:KFMod.AK47Pickup",
            NativeAction::BuyWeapon,
            1,
        );
        assert_eq!(
            gateway.prepare(&purchasable_snapshot("server-a", 4), &stale, 100),
            Err(GatewayError::StaleSelection)
        );
    }

    #[test]
    fn one_pending_intent_blocks_duplicate_click_without_dispatching_it() {
        let mut gateway = TraderCommandGateway::default();
        let snapshot = purchasable_snapshot("server-a", 4);
        let request = ActionRequest::new(
            "server-a",
            4,
            "retail:KFMod.AK47Pickup",
            NativeAction::BuyWeapon,
            1,
        );
        let permit = gateway.prepare(&snapshot, &request, 100).unwrap();
        assert_eq!(permit.native_binding_key, "retail:ServerBuyWeapon");
        assert_eq!(gateway.pending_count(), 1);
        assert_eq!(
            gateway.prepare(&snapshot, &request, 101),
            Err(GatewayError::DuplicateAction)
        );
    }

    #[test]
    fn timeout_becomes_uncertain_and_is_never_automatically_retried() {
        let mut gateway = TraderCommandGateway::default();
        let snapshot = purchasable_snapshot("server-a", 4);
        let request = ActionRequest::new(
            "server-a",
            4,
            "retail:KFMod.AK47Pickup",
            NativeAction::BuyWeapon,
            1,
        );
        let permit = gateway.prepare(&snapshot, &request, 100).unwrap();
        gateway.expire(1_000, 500);
        assert_eq!(
            gateway.status(permit.command_id),
            Some(ActionStatus::Uncertain)
        );
        assert_eq!(
            gateway.prepare(&snapshot, &request, 1_001),
            Err(GatewayError::DuplicateAction)
        );
    }

    #[test]
    fn only_authoritative_observation_can_confirm_a_transaction() {
        let mut gateway = TraderCommandGateway::default();
        let snapshot = purchasable_snapshot("server-a", 4);
        let request = ActionRequest::new(
            "server-a",
            4,
            "retail:KFMod.AK47Pickup",
            NativeAction::BuyWeapon,
            1,
        );
        let permit = gateway.prepare(&snapshot, &request, 100).unwrap();
        assert_eq!(
            gateway.reconcile(permit.command_id, NativeObservation::NoReliableResult),
            Some(ActionStatus::Uncertain)
        );
        assert_eq!(
            gateway.status(permit.command_id),
            Some(ActionStatus::Uncertain)
        );
    }

    #[test]
    fn action_from_an_old_session_cannot_be_reconciled_as_a_new_session_action() {
        let mut gateway = TraderCommandGateway::default();
        let snapshot = purchasable_snapshot("server-a", 4);
        let request = ActionRequest::new(
            "server-a",
            4,
            "retail:KFMod.AK47Pickup",
            NativeAction::BuyWeapon,
            1,
        );
        let permit = gateway.prepare(&snapshot, &request, 100).unwrap();
        let new_snapshot = purchasable_snapshot("server-b", 1);
        gateway.close_session("server-a");
        assert_eq!(
            gateway.status(permit.command_id),
            Some(ActionStatus::Uncertain)
        );
        let new_request = ActionRequest::new(
            "server-b",
            1,
            "retail:KFMod.AK47Pickup",
            NativeAction::BuyWeapon,
            1,
        );
        assert!(gateway.prepare(&new_snapshot, &new_request, 200).is_ok());
    }

    fn probe() -> SessionProbe {
        SessionProbe {
            game_build: "KF1-1065".to_string(),
            loaded_mod_packages: BTreeMap::new(),
            observed_class_signatures: BTreeSet::new(),
            stable_catalog_observed: true,
            observed_fields: BTreeSet::new(),
            custom_trader_ui: false,
            native_fallback_available: true,
        }
    }

    fn verified_adapter() -> AdapterDescriptor {
        let mut adapter = AdapterDescriptor::new("scrn-9.75.02", "KF1-1065", "ScrnBalanceSrv");
        adapter
            .supported_package_versions
            .insert("9.75.02".to_string());
        adapter
            .required_class_signatures
            .insert("ScrnBalance.ScrnBalance".to_string());
        adapter
            .mapping_evidence
            .push("Retail test server receipt and source trace".to_string());
        adapter.supported_fields.insert(TraderField::Perks);
        adapter.supported_actions.insert(NativeAction::BuyWeapon);
        adapter.callback_provenance.insert(
            NativeAction::BuyWeapon,
            "Scrn callback observed in tested build".to_string(),
        );
        adapter
    }

    #[test]
    fn unknown_mod_is_read_only_or_falls_back_to_its_native_trader() {
        let registry = CompatibilityRegistry::default();
        let mut unknown = probe();
        unknown
            .loaded_mod_packages
            .insert("CustomTrader".to_string(), "2".to_string());
        let generic = registry.detect(&unknown);
        assert_eq!(generic.mode, CompatibilityMode::GenericObservable);
        assert!(generic.supported_actions.is_empty());

        unknown.custom_trader_ui = true;
        assert_eq!(
            registry.detect(&unknown).mode,
            CompatibilityMode::UnknownCustom
        );
        unknown.native_fallback_available = false;
        assert_eq!(
            registry.detect(&unknown).mode,
            CompatibilityMode::Unavailable
        );
    }

    #[test]
    fn adapter_match_requires_build_package_version_and_class_signature() {
        let mut registry = CompatibilityRegistry::default();
        registry.register_adapter(verified_adapter()).unwrap();
        let mut session = probe();
        session
            .loaded_mod_packages
            .insert("ScrnBalanceSrv".to_string(), "9.75.02".to_string());
        session
            .observed_class_signatures
            .insert("ScrnBalance.ScrnBalance".to_string());
        assert_eq!(
            registry.detect(&session).mode,
            CompatibilityMode::AdapterVerified
        );

        session
            .loaded_mod_packages
            .insert("ScrnBalanceSrv".to_string(), "9.75.01".to_string());
        assert_eq!(
            registry.detect(&session).mode,
            CompatibilityMode::GenericObservable
        );
        session
            .loaded_mod_packages
            .insert("ScrnBalanceSrv".to_string(), "9.75.02".to_string());
        session.game_build = "KF1-1064".to_string();
        assert_eq!(
            registry.detect(&session).mode,
            CompatibilityMode::GenericObservable
        );
    }

    #[test]
    fn retail_recognition_requires_exact_build_and_no_loaded_mod_packages() {
        let mut registry = CompatibilityRegistry::default();
        registry.register_retail_build("KF1-1065");
        let retail = probe();
        let result = registry.detect(&retail);
        assert_eq!(result.mode, CompatibilityMode::RetailVerified);
        assert!(result.supported_actions.contains(&NativeAction::BuyWeapon));

        let mut modded = probe();
        modded
            .loaded_mod_packages
            .insert("UnknownMod".to_string(), "1".to_string());
        assert_eq!(
            registry.detect(&modded).mode,
            CompatibilityMode::GenericObservable
        );
    }

    #[test]
    fn facet_families_use_or_within_family_and_and_between_families() {
        let mut store = NormalizedCatalogStore::default();
        let mut first = item("mod:first", "First", 0);
        first.primary_category_id = Some("cat-a".to_string());
        first.perk_ids.push("perk-a".to_string());
        first.perk_association_known = true;
        let mut second = item("mod:second", "Second", 1);
        second.primary_category_id = Some("cat-b".to_string());
        second.perk_ids.push("perk-b".to_string());
        second.perk_association_known = true;
        let mut unknown = item("mod:unknown", "Unknown perk", 2);
        unknown.perk_association_known = false;
        let mut catalog = snapshot(vec![first, second, unknown], "server-a", 1);
        catalog.categories = vec![
            TraderCategory {
                category_id: "cat-a".to_string(),
                display_name: "A".to_string(),
                icon_ref: None,
                parent_id: None,
                order_index: 0,
            },
            TraderCategory {
                category_id: "cat-b".to_string(),
                display_name: "B".to_string(),
                icon_ref: None,
                parent_id: None,
                order_index: 1,
            },
        ];
        catalog.perks = vec![
            TraderPerk {
                perk_id: "perk-a".to_string(),
                class_id: None,
                name: "A".to_string(),
                icon_ref: None,
                current_level: None,
                max_level: None,
                level_progress: None,
                description: None,
                provenance: BTreeMap::new(),
            },
            TraderPerk {
                perk_id: "perk-b".to_string(),
                class_id: None,
                name: "B".to_string(),
                icon_ref: None,
                current_level: None,
                max_level: None,
                level_progress: None,
                description: None,
                provenance: BTreeMap::new(),
            },
        ];
        store.replace(catalog).unwrap();

        let matching = store.query(&CatalogQuery {
            category_ids: vec!["cat-a".to_string(), "cat-b".to_string()],
            perk_filter: PerkFilter::ForPerk("perk-b".to_string()),
            ..CatalogQuery::default()
        });
        assert_eq!(
            matching
                .iter()
                .map(|item| item.item_id.as_str())
                .collect::<Vec<_>>(),
            vec!["mod:second"]
        );
        let unknown_perks = store.query(&CatalogQuery {
            perk_filter: PerkFilter::UnknownAssociation,
            ..CatalogQuery::default()
        });
        assert_eq!(
            unknown_perks
                .iter()
                .map(|item| item.item_id.as_str())
                .collect::<Vec<_>>(),
            vec!["mod:unknown"]
        );
    }

    #[test]
    fn custom_official_owned_available_and_favorite_facets_use_explicit_facts() {
        let mut store = NormalizedCatalogStore::default();
        let mut retail = item("retail:One", "One", 0);
        retail.item_source = ItemSource::Retail;
        retail.owned_count = Some(1);
        retail.availability = Availability::Available;
        let verified = FieldProvenance {
            source: DataSource::NativeGui,
            confidence: Confidence::Verified,
        };
        retail
            .provenance
            .insert("item_source".to_string(), verified);
        retail
            .provenance
            .insert("owned_count".to_string(), verified);
        retail
            .provenance
            .insert("availability".to_string(), verified);
        let mut custom = item("mod:Two", "Two", 1);
        custom.item_source = ItemSource::Mod;
        custom.owned_count = Some(0);
        custom.availability = Availability::Unknown;
        custom
            .provenance
            .insert("item_source".to_string(), verified);
        store
            .replace(snapshot(vec![retail, custom], "server-a", 1))
            .unwrap();
        let query = CatalogQuery {
            custom_only: true,
            favorite_only: true,
            favorite_item_ids: ["mod:Two".to_string()].into_iter().collect(),
            ..CatalogQuery::default()
        };
        assert_eq!(
            store
                .query(&query)
                .iter()
                .map(|item| item.item_id.as_str())
                .collect::<Vec<_>>(),
            vec!["mod:Two"]
        );
        assert_eq!(
            store
                .query(&CatalogQuery {
                    official_only: true,
                    owned_only: true,
                    available_only: true,
                    ..CatalogQuery::default()
                })
                .len(),
            1
        );
        assert!(store
            .query(&CatalogQuery {
                custom_only: true,
                official_only: true,
                ..CatalogQuery::default()
            })
            .is_empty());
    }

    #[test]
    fn affordability_and_weight_facets_require_trusted_context_and_values() {
        let verified = FieldProvenance {
            source: DataSource::Adapter,
            confidence: Confidence::Verified,
        };
        let local = FieldProvenance {
            source: DataSource::LocalMetadata,
            confidence: Confidence::Indicative,
        };
        let mut store = NormalizedCatalogStore::default();
        let mut trusted = item("mod:trusted", "Trusted", 0);
        trusted.effective_price = Some(50);
        trusted.weight = Some(1.0);
        trusted
            .provenance
            .insert("effective_price".to_string(), verified);
        trusted.provenance.insert("weight".to_string(), verified);
        let mut local_only = item("mod:local", "Local metadata", 1);
        local_only.effective_price = Some(10);
        local_only.weight = Some(0.1);
        local_only
            .provenance
            .insert("effective_price".to_string(), local);
        local_only.provenance.insert("weight".to_string(), local);
        let mut expensive = item("mod:expensive", "Expensive", 2);
        expensive.effective_price = Some(500);
        expensive.weight = Some(10.0);
        expensive
            .provenance
            .insert("effective_price".to_string(), verified);
        expensive.provenance.insert("weight".to_string(), verified);
        let mut catalog = snapshot(vec![trusted, local_only, expensive], "server-a", 1);
        catalog.wallet.amount = Some(100);
        catalog.capacity.used = Some(4.0);
        catalog.capacity.maximum = Some(10.0);
        store.replace(catalog.clone()).unwrap();
        let unknown_context = store.query_report(&CatalogQuery {
            affordable_only: true,
            fits_weight_only: true,
            ..CatalogQuery::default()
        });
        assert!(unknown_context.items.is_empty());
        assert!(unknown_context
            .inactive_filters
            .contains(&InactiveFilter::AffordableWalletUnverified));
        assert!(unknown_context
            .inactive_filters
            .contains(&InactiveFilter::CapacityUnverified));

        catalog.generation = 2;
        catalog.wallet.provenance = Some(verified);
        catalog.capacity.provenance = Some(verified);
        store.replace(catalog).unwrap();
        let trusted_context = store.query_report(&CatalogQuery {
            affordable_only: true,
            fits_weight_only: true,
            ..CatalogQuery::default()
        });
        assert_eq!(
            trusted_context
                .items
                .iter()
                .map(|item| item.item_id.as_str())
                .collect::<Vec<_>>(),
            vec!["mod:trusted"]
        );
        assert!(trusted_context.inactive_filters.is_empty());
    }

    #[test]
    fn adapter_registration_rejects_missing_test_evidence() {
        let mut registry = CompatibilityRegistry::default();
        let mut descriptor = AdapterDescriptor::new("unverified", "KF1-1065", "SomeMod");
        descriptor
            .supported_package_versions
            .insert("1.0".to_string());
        assert_eq!(
            registry.register_adapter(descriptor),
            Err(AdapterError::MissingEvidence)
        );
    }

    #[test]
    fn selection_survives_sorting_by_stable_item_id() {
        let catalog = snapshot(
            vec![
                item("id-a", "A", 0),
                item("id-b", "B", 1),
                item("id-c", "C", 2),
            ],
            "server-a",
            1,
        );
        let initial = catalog.catalog_items.iter().collect::<Vec<_>>();
        let mut selection = TraderSelectionModel::default();
        assert!(selection.select("id-b", &initial));
        let reordered = vec![
            &catalog.catalog_items[2],
            &catalog.catalog_items[1],
            &catalog.catalog_items[0],
        ];
        assert_eq!(
            selection.reconcile(&catalog, &reordered),
            SelectionOutcome::Preserved
        );
        assert_eq!(selection.selected_item_id(), Some("id-b"));
    }

    #[test]
    fn filtered_selection_is_retained_when_item_still_exists() {
        let catalog = snapshot(
            vec![item("id-a", "A", 0), item("id-b", "B", 1)],
            "server-a",
            1,
        );
        let all = catalog.catalog_items.iter().collect::<Vec<_>>();
        let mut selection = TraderSelectionModel::default();
        selection.select("id-b", &all);
        let filtered = vec![&catalog.catalog_items[0]];
        assert_eq!(
            selection.reconcile(&catalog, &filtered),
            SelectionOutcome::TemporarilyHidden
        );
        assert_eq!(selection.selected_item_id(), Some("id-b"));
    }

    #[test]
    fn removed_selection_moves_to_neighbor_and_reports_removed_id() {
        let previous = snapshot(
            vec![
                item("id-a", "A", 0),
                item("id-b", "B", 1),
                item("id-c", "C", 2),
            ],
            "server-a",
            1,
        );
        let all = previous.catalog_items.iter().collect::<Vec<_>>();
        let mut selection = TraderSelectionModel::default();
        selection.select("id-b", &all);
        let current = snapshot(
            vec![item("id-a", "A", 0), item("id-c", "C", 1)],
            "server-a",
            2,
        );
        let visible = current.catalog_items.iter().collect::<Vec<_>>();
        assert_eq!(
            selection.reconcile(&current, &visible),
            SelectionOutcome::NeighborSelected {
                removed_item_id: "id-b".to_string(),
                selected_item_id: "id-c".to_string(),
            }
        );
    }
}
