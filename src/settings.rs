//! Persistent user preferences: theme, UI scale, wage display, and face packs.
//!
//! Stored at `%APPDATA%/fm-editor/settings.json` on Windows.
//! Falls back to defaults on missing or corrupt file.

use eframe::egui;
use serde::{Deserialize, Serialize};

// ─── Theme ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Theme {
    #[default]
    Dark,
    Light,
    DarkBlue,
}

impl Theme {
    pub fn label(self) -> &'static str {
        match self {
            Theme::Dark     => "Dark",
            Theme::Light    => "Light",
            Theme::DarkBlue => "Dark Blue",
        }
    }

    pub fn visuals(self) -> egui::Visuals {
        match self {
            Theme::Dark     => egui::Visuals::dark(),
            Theme::Light    => egui::Visuals::light(),
            Theme::DarkBlue => dark_blue_visuals(),
        }
    }
}

/// Navy/steel-blue dark theme.
fn dark_blue_visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.window_fill      = egui::Color32::from_rgb(15, 25, 45);
    v.panel_fill       = egui::Color32::from_rgb(20, 32, 56);
    v.faint_bg_color   = egui::Color32::from_rgb(25, 40, 70);
    v.extreme_bg_color = egui::Color32::from_rgb(10, 18, 35);
    v
}

// ─── Scale ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Scale {
    #[default]
    S100,
    S125,
    S150,
}

impl Scale {
    pub fn zoom_factor(self) -> f32 {
        match self {
            Scale::S100 => 1.0,
            Scale::S125 => 1.1,
            Scale::S150 => 1.2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Scale::S100 => "100%",
            Scale::S125 => "125%",
            Scale::S150 => "150%",
        }
    }

    pub fn from_index(i: u8) -> Self {
        match i {
            1 => Scale::S125,
            2 => Scale::S150,
            _ => Scale::S100,
        }
    }

    pub fn to_index(self) -> u8 {
        match self {
            Scale::S100 => 0,
            Scale::S125 => 1,
            Scale::S150 => 2,
        }
    }
}

// ─── Wage Period ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WagePeriod {
    #[default]
    Weekly,
    Monthly,
    Yearly,
}

impl WagePeriod {
    pub fn label(self) -> &'static str {
        match self {
            WagePeriod::Weekly  => "Weekly",
            WagePeriod::Monthly => "Monthly",
            WagePeriod::Yearly  => "Yearly",
        }
    }
}

// ─── Currency ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Currency {
    #[default]
    Eur,
    Usd,
    Gbp,
    Idr,
}

impl Currency {
    pub fn label(self) -> &'static str {
        match self {
            Currency::Eur => "Euro (€)",
            Currency::Usd => "US Dollar ($)",
            Currency::Gbp => "Pound Sterling (£)",
            Currency::Idr => "Indonesian Rupiah (Rp)",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Currency::Eur => "€",
            Currency::Usd => "$",
            Currency::Gbp => "£",
            Currency::Idr => "Rp",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Currency::Eur => "EUR",
            Currency::Usd => "USD",
            Currency::Gbp => "GBP",
            Currency::Idr => "IDR",
        }
    }

    /// Rate relative to British Pound (GBP), which is FM's internal base currency.
    /// 1 GBP = `rate` in this currency.
    pub fn rate_from_gbp(self) -> f64 {
        match self {
            Currency::Gbp => 1.0,
            Currency::Eur => 1.15,
            Currency::Usd => 1.25,
            Currency::Idr => 20_000.0,
        }
    }

    /// Convert a base GBP amount (i64) to this currency (i64).
    pub fn from_gbp(self, gbp: i64) -> i64 {
        match self {
            Currency::Gbp => gbp,
            _ => (gbp as f64 * self.rate_from_gbp()).round() as i64,
        }
    }

    /// Convert an amount in this currency (i64) back to base GBP (i64).
    pub fn to_gbp(self, amount: i64) -> i64 {
        match self {
            Currency::Gbp => amount,
            _ => {
                let rate = self.rate_from_gbp();
                if rate <= 0.0 {
                    amount
                } else {
                    (amount as f64 / rate).round() as i64
                }
            }
        }
    }

    /// Format an amount in this currency with symbol and thousands separators.
    pub fn format_amount(self, amount: i64) -> String {
        let is_neg = amount < 0;
        let abs_val = amount.unsigned_abs();
        let formatted = format_with_commas(abs_val);
        let prefix = match self {
            Currency::Eur => "€",
            Currency::Usd => "$",
            Currency::Gbp => "£",
            Currency::Idr => "Rp ",
        };
        if is_neg {
            format!("-{}{}", prefix, formatted)
        } else {
            format!("{}{}", prefix, formatted)
        }
    }

    /// Format a base GBP amount in this currency.
    pub fn format_gbp(self, gbp: i64) -> String {
        self.format_amount(self.from_gbp(gbp))
    }

    /// Format an amount compactly with K / M / B suffix.
    pub fn format_compact(self, amount: i64) -> String {
        let abs = amount.unsigned_abs();
        let prefix = self.symbol();
        if abs >= 1_000_000_000 {
            format!("{}{:.1}B", prefix, abs as f64 / 1_000_000_000.0)
        } else if abs >= 1_000_000 {
            format!("{}{:.0}M", prefix, abs as f64 / 1_000_000.0)
        } else if abs >= 1_000 {
            format!("{}{:.0}k", prefix, abs as f64 / 1_000.0)
        } else {
            format!("{}{}", prefix, abs)
        }
    }
}

pub fn format_with_commas(mut n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let mut parts = Vec::new();
    while n > 0 {
        let rem = n % 1000;
        n /= 1000;
        if n > 0 {
            parts.push(format!("{:03}", rem));
        } else {
            parts.push(format!("{}", rem));
        }
    }
    parts.reverse();
    parts.join(",")
}

/// Format a weekly wage value according to the chosen display period and currency.
/// FM stores wages as weekly integers in GBP. Monthly ≈ ×4.333, yearly ≈ ×52.
pub fn format_wage(weekly_gbp: u32, period: WagePeriod, currency: Currency) -> String {
    let weekly_curr = currency.from_gbp(weekly_gbp as i64).max(0) as u64;
    let amount = match period {
        WagePeriod::Weekly  => weekly_curr,
        WagePeriod::Monthly => (weekly_curr as f64 * 4.333).round() as u64,
        WagePeriod::Yearly  => weekly_curr * 52,
    };
    let suffix = match period {
        WagePeriod::Weekly  => "/wk",
        WagePeriod::Monthly => "/mo",
        WagePeriod::Yearly  => "/yr",
    };
    format!("{}{}", currency.format_amount(amount as i64), suffix)
}

// ─── AppSettings ─────────────────────────────────────────────────────────────

fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    pub theme:  Theme,
    pub scale:  Scale,
    #[serde(default)]
    pub wage_period: WagePeriod,
    #[serde(default)]
    pub currency: Currency,
    /// Directory containing `<uid>.png` face pack images.
    /// None = face images disabled.
    #[serde(default)]
    pub face_pack_path: Option<String>,
    /// Re-read entity data from memory each time a tab is switched to.
    /// Default on — disable if the spinner is distracting on a slow save.
    #[serde(default = "default_true")]
    pub reload_on_tab_switch: bool,
}

impl AppSettings {
    /// Load from disk, falling back to defaults on any error.
    pub fn load() -> Self {
        let Some(path) = settings_path() else { return Self::default() };
        let Ok(text) = std::fs::read_to_string(&path) else { return Self::default() };
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// Save to disk. Silently ignores I/O errors.
    pub fn save(&self) {
        let Some(path) = settings_path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, text);
        }
    }
}

fn settings_path() -> Option<std::path::PathBuf> {
    Some(dirs::config_dir()?.join("fm-editor").join("settings.json"))
}

// ─── Egui data key ────────────────────────────────────────────────────────────

///Key used to store the active `Theme` in egui's per-frame temp data so
/// widget code can read it without needing an explicit parameter.
pub fn theme_data_id() -> egui::Id {
    egui::Id::new("fm_active_theme")
}

pub fn currency_data_id() -> egui::Id {
    egui::Id::new("fm_active_currency")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_visuals_override_system_theme_on_every_frame() {
        let ctx = egui::Context::default();
        for theme in [Theme::Dark, Theme::Light, Theme::DarkBlue] {
            ctx.set_visuals(egui::Visuals::light());
            ctx.set_visuals(theme.visuals());
            assert_eq!(ctx.style().visuals.window_fill, theme.visuals().window_fill);
        }
    }

    #[test]
    fn settings_round_trip_preserves_theme() {
        for theme in [Theme::Dark, Theme::Light, Theme::DarkBlue] {
            let s = AppSettings {
                theme,
                ..AppSettings::default()
            };
            let json = serde_json::to_string(&s).unwrap();
            let loaded: AppSettings = serde_json::from_str(&json).unwrap();
            assert_eq!(loaded.theme, theme);
        }
    }

    #[test]
    fn default_currency_is_euro() {
        assert_eq!(Currency::default(), Currency::Eur);
        assert_eq!(AppSettings::default().currency, Currency::Eur);
    }

    #[test]
    fn settings_round_trip_preserves_currency() {
        for currency in [Currency::Eur, Currency::Usd, Currency::Gbp, Currency::Idr] {
            let s = AppSettings {
                currency,
                ..AppSettings::default()
            };
            let json = serde_json::to_string(&s).unwrap();
            let loaded: AppSettings = serde_json::from_str(&json).unwrap();
            assert_eq!(loaded.currency, currency);
        }
    }

    #[test]
    fn currency_conversion_and_formatting() {
        let eur = Currency::Eur;
        assert_eq!(eur.from_gbp(10_000_000), 11_500_000);
        assert_eq!(eur.to_gbp(11_500_000), 10_000_000);
        assert_eq!(eur.format_amount(11_500_000), "€11,500,000");
        assert_eq!(eur.format_amount(-500_000), "-€500,000");

        let idr = Currency::Idr;
        assert_eq!(idr.from_gbp(1_000_000), 20_000_000_000);
        assert_eq!(idr.to_gbp(20_000_000_000), 1_000_000);
        assert_eq!(idr.format_amount(20_000_000_000), "Rp 20,000,000,000");

        let wage_str = format_wage(100_000, WagePeriod::Weekly, Currency::Eur);
        assert_eq!(wage_str, "€115,000/wk");
    }
}
