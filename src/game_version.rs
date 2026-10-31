//! Game version, storefront and build selector + runtime dispatch.
//!
//! Every schema lookup routes through a `&'static VersionedSchemas` that
//! belongs to the game being read: `FmProcess::schemas()` on the worker side,
//! the attached game's build on the UI side. There is no process-global
//! "active version", so several games open at once cannot cross schemas.
//!
//! A schema is selected per [`GameBuild`] — a release ([`GameVersion`]) as
//! shipped by one storefront ([`Storefront`]). The Game Pass build of a
//! release is a different binary from the Steam one (Microsoft GDK instead
//! of Steamworks), so its module hash and module-relative RVAs can differ
//! even when the struct layouts match.
//!
//! Detection is automatic: FM24, FM26 and FM27 all run as `fm.exe`, so
//! `process::running_games()` lists them and reads each one's on-disk
//! module path — the install directory always contains the release year,
//! and a Game Pass install is recognisable by its install location — to
//! determine which build, and so which schema, it uses.
//!
//! Adding a build (e.g. FM26 on Game Pass) is a new `GameBuild` variant,
//! its arms in the `GameBuild` matches, and its schema static under
//! `crate::fields::`.

use crate::fields::{fm24, fm26, fm27, VersionedSchemas};

/// Which Football Manager release a process is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameVersion {
    Fm24,
    Fm26,
    Fm27,
}

impl GameVersion {
    /// Module that hosts the game logic and the entity-array RVAs.
    /// FM26 and FM27 (Unity) split it into `game_plugin.dll`; FM24 links it
    /// into `fm.exe` itself.
    pub fn module_name(self) -> &'static str {
        match self {
            GameVersion::Fm24 => "fm.exe",
            GameVersion::Fm26 | GameVersion::Fm27 => "game_plugin.dll",
        }
    }

    /// Human-readable name for status text and pickers (e.g. "FM26").
    pub fn display_name(self) -> &'static str {
        match self {
            GameVersion::Fm24 => "FM24",
            GameVersion::Fm26 => "FM26",
            GameVersion::Fm27 => "FM27",
        }
    }

    /// Short lowercase identifier used in source paths (e.g. "fm26").
    pub fn short_name(self) -> &'static str {
        match self {
            GameVersion::Fm24 => "fm24",
            GameVersion::Fm26 => "fm26",
            GameVersion::Fm27 => "fm27",
        }
    }
}

/// Where a game install came from. Anything not recognised as Game Pass or
/// Epic Games Store is treated as the Steam build, which is what every
/// schema was mapped against (on Windows and under Proton).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Storefront {
    Steam,
    /// PC Game Pass / Microsoft Store (Xbox app) install.
    GamePass,
    /// Epic Games Store install.
    Epic,
}

impl Storefront {
    /// Human-readable name for status text (e.g. "Game Pass").
    pub fn display_name(self) -> &'static str {
        match self {
            Storefront::Steam => "Steam",
            Storefront::GamePass => "Game Pass",
            Storefront::Epic => "Epic",
        }
    }
}

/// A release as shipped by one storefront — the unit a schema is selected
/// for. Only builds with a schema exist as variants, so a `GameBuild` always
/// has one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameBuild {
    Fm24Steam,
    /// Starts as a copy of the FM24 Steam schema; unverified against the
    /// Epic Games Store binary (see docs/fm24).
    Fm24Epic,
    Fm26Steam,
    Fm27Steam,
    /// Starts as a copy of the FM27 Steam schema; nothing in it has been
    /// verified against the Game Pass binary yet (see docs/fm27).
    Fm27GamePass,
}

impl GameBuild {
    /// Every build with a schema.
    pub const ALL: [GameBuild; 5] = [
        GameBuild::Fm24Steam,
        GameBuild::Fm24Epic,
        GameBuild::Fm26Steam,
        GameBuild::Fm27Steam,
        GameBuild::Fm27GamePass,
    ];

    /// The build for this release and storefront, or an error when no
    /// schema exists for that combination (e.g. FM26 on Game Pass).
    pub fn resolve(version: GameVersion, storefront: Storefront) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|b| b.version() == version && b.storefront() == storefront)
            .ok_or_else(|| {
                format!(
                    "no schema for the {} build of {} — only the {} build is mapped",
                    storefront.display_name(),
                    version.display_name(),
                    Storefront::Steam.display_name(),
                )
            })
    }

    pub fn version(self) -> GameVersion {
        match self {
            GameBuild::Fm24Steam | GameBuild::Fm24Epic => GameVersion::Fm24,
            GameBuild::Fm26Steam => GameVersion::Fm26,
            GameBuild::Fm27Steam | GameBuild::Fm27GamePass => GameVersion::Fm27,
        }
    }

    pub fn storefront(self) -> Storefront {
        match self {
            GameBuild::Fm24Steam | GameBuild::Fm26Steam | GameBuild::Fm27Steam => {
                Storefront::Steam
            }
            GameBuild::Fm24Epic => Storefront::Epic,
            GameBuild::Fm27GamePass => Storefront::GamePass,
        }
    }

    /// The per-build schema bundle (field lists, RVAs, sizes).
    pub fn schemas(self) -> &'static VersionedSchemas {
        match self {
            GameBuild::Fm24Steam => &fm24::FM24,
            GameBuild::Fm24Epic => &fm24::FM24_EPIC,
            GameBuild::Fm26Steam => &fm26::FM26,
            GameBuild::Fm27Steam => &fm27::FM27,
            GameBuild::Fm27GamePass => &fm27::FM27_GAMEPASS,
        }
    }

    /// Name of the schema static in `src/fields/<version>/mod.rs`, for
    /// messages that tell the developer what to edit.
    pub fn schema_name(self) -> &'static str {
        match self {
            GameBuild::Fm24Steam => "FM24",
            GameBuild::Fm24Epic => "FM24_EPIC",
            GameBuild::Fm26Steam => "FM26",
            GameBuild::Fm27Steam => "FM27",
            GameBuild::Fm27GamePass => "FM27_GAMEPASS",
        }
    }

    /// Human-readable name for status text and pickers. The Steam build is
    /// the default and keeps the bare version name ("FM27"); other
    /// storefronts are named ("FM27 Game Pass").
    pub fn display_name(self) -> &'static str {
        match self {
            GameBuild::Fm24Epic => "FM24 Epic",
            GameBuild::Fm27GamePass => "FM27 Game Pass",
            other => other.version().display_name(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_round_trips_every_build() {
        for build in GameBuild::ALL {
            assert_eq!(GameBuild::resolve(build.version(), build.storefront()), Ok(build));
        }
    }

    #[test]
    fn resolve_rejects_unmapped_storefront() {
        let err = GameBuild::resolve(GameVersion::Fm26, Storefront::GamePass).unwrap_err();
        assert!(err.contains("Game Pass") && err.contains("FM26"), "{err}");
    }

    #[test]
    fn every_build_has_its_own_schema() {
        for (i, a) in GameBuild::ALL.iter().enumerate() {
            for b in &GameBuild::ALL[i + 1..] {
                assert!(!std::ptr::eq(a.schemas(), b.schemas()), "{a:?} / {b:?}");
            }
        }
    }
}
