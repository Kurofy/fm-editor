//! Bridge protocol — the `BridgeCmd` commands the UI sends to the worker,
//! the `BridgeResult` responses it receives, and the `ProcessBridge` handle
//! that wraps the mpsc channel pair.

use std::sync::{Arc, mpsc};
use std::thread;

use crate::cache::EntityCache;
use crate::fields::{HasSubRecords, SubRecordId};
use crate::player::PlayerRecord;
use crate::staff::StaffRecord;

use super::worker::worker_loop;

// ---------------------------------------------------------------------------
// Shared view types
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct EntityCounts {
    pub players: usize,
    pub staff: usize,
    pub clubs: usize,
}

/// Full staff data including name and role sub-record.
#[derive(Clone)]
pub struct StaffView {
    pub record: StaffRecord,
    pub name: Option<String>,
    pub roles: Option<[u8; 15]>,
    pub role_ptr: u64,
    /// Display nationality string (e.g. "England"). None until offset is mapped.
    pub nationality: Option<String>,
    /// Display club name (e.g. "Tottenham Hotspur"). None until offset is mapped.
    pub club_name: Option<String>,
    /// Existing schema club chain; only cached object matches are navigable.
    pub club_addr: Option<u64>,
    /// Place of birth (e.g. "San Sebastián"). None until offset is mapped.
    pub birthplace: Option<String>,
    /// Contract sub-record bytes (job, wage, expiry), when mapped and present.
    pub contract_raw: Option<Vec<u8>>,
    /// Address of the contract sub-record, for edits to contract fields.
    pub contract_addr: Option<u64>,
}

/// Full club data with resolved sub-records.
#[derive(Clone)]
pub struct ClubView {
    pub addr: u64,
    pub index: usize,
    pub raw: Vec<u8>,
    pub uid: u32,
    pub full_name: String,
    pub short_name: String,
    pub info_raw: Option<Vec<u8>>,
    pub info_addr: Option<u64>,
    pub finance_raw: Option<Vec<u8>>,
    pub finance_addr: Option<u64>,
    pub stadium_raw: Option<Vec<u8>>,
    /// Senior (first) team object — holds the club reputation on FM27.
    pub senior_team_raw: Option<Vec<u8>>,
    pub senior_team_addr: Option<u64>,
    pub budget_raw: Option<Vec<u8>>,
    pub budget_addr: Option<u64>,
    pub nickname: Option<String>,
    /// Raw bytes for the home stadium entity (flat inline array), loaded via
    /// the schema's `home_stadium` sub-record chain.
    pub stadium_entity_raw: Option<Vec<u8>>,
    /// Home stadium name FM displays (`st_name`), e.g. "Tottenham Hotspur Stadium".
    pub stadium_name: Option<String>,
    /// Official/alternative stadium name (`st_name2`), e.g. "City of
    /// Manchester Stadium" for the Etihad. Null for most stadiums.
    pub stadium_name2: Option<String>,
    /// Affiliated and feeder clubs.
    pub affiliations: Vec<crate::affiliation::AffiliationView>,
}

impl ClubView {
    /// Mutable bytes and in-process address of a named sub-record, for
    /// editing a field whose schema location is `SubRecord { container, .. }`.
    pub fn sub_record_mut(&mut self, id: SubRecordId) -> Option<(&mut Vec<u8>, u64)> {
        match id {
            "info" => Some((self.info_raw.as_mut()?, self.info_addr?)),
            "finance" => Some((self.finance_raw.as_mut()?, self.finance_addr?)),
            "budget" => Some((self.budget_raw.as_mut()?, self.budget_addr?)),
            "senior_team" => Some((self.senior_team_raw.as_mut()?, self.senior_team_addr?)),
            _ => None,
        }
    }
}

impl HasSubRecords for ClubView {
    fn sub_record(&self, id: SubRecordId) -> Option<&[u8]> {
        match id {
            "info" => self.info_raw.as_deref(),
            "finance" => self.finance_raw.as_deref(),
            "budget" => self.budget_raw.as_deref(),
            "stadium" => self.stadium_raw.as_deref(),
            "senior_team" => self.senior_team_raw.as_deref(),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Bridge protocol
// ---------------------------------------------------------------------------

pub enum BridgeCmd {
    /// Attach to the game with this PID, or with `None` to the only running
    /// game. With several games running, `None` answers `ChooseGame`.
    Attach(Option<u32>),
    LoadEntityCache,
    LoadPlayer(u64, usize),
    LoadStaff(u64, usize),
    LoadClub(u64, usize),
    LoadHumanManager,
    WriteBytes { id: u64, addr: u64, data: Vec<u8> },
    Ping,
    Shutdown,
}

pub enum BridgeResult {
    /// fm.exe attached AND a save is loaded (entity arrays populated and
    /// stable across two consecutive probes).
    Connected(u32, EntityCounts),
    /// fm.exe attached but no save is loaded yet, or save creation is
    /// still in progress. The worker keeps pinging; once counts are
    /// stable + plausible, we promote to Connected.
    Attached(u32, crate::game_version::GameBuild),
    /// Several games are running (they all run as `fm.exe`) and no PID was
    /// given. Nothing was attached; the UI asks which one to use.
    ChooseGame(Vec<crate::process::RunningGame>),
    /// fm.exe still alive but the save got unloaded (or returned to
    /// main menu). Caller should wipe entity state and wait.
    SaveUnloaded(u32),
    /// fm.exe exited while the cache was loaded. Cache is preserved;
    /// UI enters ReadOnly mode.
    GameClosed,
    /// Bulk load progress — `done` is cumulative across all entity types.
    CacheProgress {
        done: usize,
        total: usize,
    },
    /// The bulk entity-cache load failed; the app leaves its loading state.
    CacheFailed(String),
    /// Cache load complete. The Arc is cheaply cloned into app state.
    CacheReady(Arc<EntityCache>),
    PlayerLoaded(PlayerRecord),
    StaffLoaded(StaffView),
    ClubLoaded(ClubView),
    HumanManagerLoaded(StaffView),
    WriteOk {
        id: u64,
    },
    WriteFail {
        id: u64,
        error: String,
    },
    PingOk,
    PingFail,
    Error(String),
}

// ---------------------------------------------------------------------------
// ProcessBridge — UI-side handle
// ---------------------------------------------------------------------------

pub struct ProcessBridge {
    cmd_tx: mpsc::Sender<BridgeCmd>,
    result_rx: mpsc::Receiver<BridgeResult>,
}

impl ProcessBridge {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        thread::spawn(move || worker_loop(cmd_rx, result_tx));
        Self { cmd_tx, result_rx }
    }

    pub fn send(&self, cmd: BridgeCmd) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn try_recv(&self) -> Option<BridgeResult> {
        self.result_rx.try_recv().ok()
    }
}
