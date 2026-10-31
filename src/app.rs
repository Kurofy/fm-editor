//! FmEditorApp — main application state and eframe::App implementation.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::bridge::{BridgeCmd, BridgeResult, ClubView, EntityCounts, ProcessBridge, StaffView};
use crate::cache::{CachedClub, CachedPlayer, CachedStaff, EntityCache};
use crate::game_version::GameBuild;
use crate::player::PlayerRecord;
use crate::process::RunningGame;
use crate::settings::{AppSettings, Currency, Scale, Theme, WagePeriod, currency_data_id, theme_data_id};
use crate::staff::StaffRecord;
use crate::ui::navigation::{self, BrowserHistory};
use crate::ui::pages::{
    advanced_search_modal, club_search::ClubSearchState, player_search::PlayerSearchState,
    staff_search::StaffSearchState,
};
use crate::ui::{club_panel, field_widget, pages, player_panel, staff_panel};
use crate::undo::{UndoEntry, UndoStack};
use crate::write_tracker::WriteTracker;

// ---------------------------------------------------------------------------
// Shared types used by UI modules
// ---------------------------------------------------------------------------

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Page {
    BestXi,
    PlayerSearch,
    StaffSearch,
    ClubSearch,
    HumanManager,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EntityTabType {
    Player,
    Staff,
    Club,
}

/// Cache load state — tracks whether the bulk entity cache is being built.
#[derive(Clone, Copy, PartialEq)]
pub enum CacheLoadState {
    Idle,
    Loading { done: usize, total: usize },
    Ready,
}

impl EntityTabType {
    /// The search page that hosts a tab of this type.
    fn home_page(self) -> Page {
        match self {
            EntityTabType::Player => Page::PlayerSearch,
            EntityTabType::Staff => Page::StaffSearch,
            EntityTabType::Club => Page::ClubSearch,
        }
    }
}

/// One entry in the browser-style navigation history. Separate from
/// `EntityTab` — tabs are "things currently open", history is "things I've
/// visited in order". The back/forward buttons walk this vector.
#[derive(Clone, PartialEq, Eq)]
struct HistoryEntry {
    entity_type: EntityTabType,
    addr: u64,
    index: usize,
    display_name: String,
}

#[derive(Clone, PartialEq, Eq)]
enum Location {
    Results(Page),
    Entity(HistoryEntry),
}

/// Four-state connection machine.
#[derive(Clone, Copy, PartialEq)]
pub enum ConnState {
    Disconnected,
    /// fm.exe is attached but no save is loaded yet (or save creation is
    /// still in progress). The worker keeps pinging until counts stabilize.
    WaitingForSave(u32),
    /// fm.exe attached and a save is loaded. Entity reads and writes are safe.
    Ready(u32),
    /// fm.exe exited while a save was loaded. The entity cache is preserved
    /// for read-only browsing; writes are disabled until reconnect.
    ReadOnly(u32),
}

impl ConnState {
    pub fn is_ready(self) -> bool {
        matches!(self, ConnState::Ready(_))
    }
    pub fn is_attached(self) -> bool {
        !matches!(self, ConnState::Disconnected)
    }
    /// True only when connected AND the game is running. Used to gate writes.
    pub fn is_writable(self) -> bool {
        matches!(self, ConnState::Ready(_))
    }
}

pub struct EntityTab {
    pub entity_type: EntityTabType,
    pub display_name: String,
    pub addr: u64,
    pub index: usize,
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

pub struct FmEditorApp {
    // Navigation
    active_page: Page,

    // Connection
    bridge: ProcessBridge,
    conn: ConnState,
    attached_build: Option<GameBuild>,
    window_branch: Option<String>,
    /// Games offered by the last `ChooseGame` (several `fm.exe` running and
    /// none chosen yet). Cleared once a game is attached.
    game_choices: Vec<RunningGame>,
    auto_connect: bool,
    last_ping: Instant,
    last_auto_connect: Instant,

    // Per-page filter state. Each struct owns all its filter inputs.
    // Filtering runs every frame against the entity cache — no bridge round-trip.
    player_search: PlayerSearchState,
    best_xi: pages::best_xi::BestXiState,
    staff_search: StaffSearchState,
    club_search: ClubSearchState,

    // Advanced search modal state
    advanced_open: bool,
    /// Snapshot taken when the modal opens; restored on Cancel.
    advanced_snapshot: Option<PlayerSearchState>,

    // Entity cache — built by the worker on connect, preserved across GameClosed.
    entity_cache: Option<Arc<EntityCache>>,
    cache_load_state: CacheLoadState,

    // Entity tabs
    open_tabs: Vec<EntityTab>,
    active_tab: Option<usize>,

    // One history across search pages and related-entity links.
    history: BrowserHistory<Location>,
    // Keep opened detail snapshots available when the game exits.
    club_snapshots: HashMap<u64, ClubView>,
    player_snapshots: HashMap<u64, PlayerRecord>,
    staff_snapshots: HashMap<u64, StaffView>,

    // Selected entity data (refreshed on tab switch)
    selected_player: Option<PlayerRecord>,
    selected_staff: Option<StaffView>,
    selected_club: Option<ClubView>,
    loading_entity: bool,

    // Sub-tabs
    player_tab: player_panel::PlayerTab,
    staff_tab: staff_panel::StaffTab,
    club_tab: club_panel::ClubTab,

    // Human manager
    manager_view: Option<StaffView>,
    manager_loading: bool,

    // Undo
    undo_stack: UndoStack,
    writes: WriteTracker,
    write_error: Option<String>,

    // Status
    status_msg: Option<(String, Instant)>,
    entity_counts: Option<EntityCounts>,
    show_history: bool,

    // User preferences
    settings: AppSettings,
    // Deferred window resize — set when scale changes, applied next frame
    // after set_zoom_factor has taken effect so InnerSize uses the new ppp.
    pending_scale_resize: Option<f32>,

    // Debug-only: handle to the MCP server state — used to show the traffic LED.
    #[cfg(debug_assertions)]
    mcp_state: Option<std::sync::Arc<crate::debug::DebugState>>,
}

impl FmEditorApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = AppSettings::load();
        cc.egui_ctx.set_visuals(settings.theme.visuals());
        cc.egui_ctx.set_zoom_factor(settings.scale.zoom_factor());
        egui_extras::install_image_loaders(&cc.egui_ctx);

        Self {
            active_page: Page::HumanManager,
            bridge: ProcessBridge::new(),
            conn: ConnState::Disconnected,
            attached_build: None,
            window_branch: None,
            game_choices: Vec::new(),
            auto_connect: true,
            last_ping: Instant::now(),
            last_auto_connect: Instant::now() - std::time::Duration::from_secs(10),
            player_search: PlayerSearchState::default(),
            best_xi: pages::best_xi::BestXiState::default(),
            staff_search: StaffSearchState::default(),
            club_search: ClubSearchState::default(),
            advanced_open: false,
            advanced_snapshot: None,
            entity_cache: None,
            cache_load_state: CacheLoadState::Idle,
            open_tabs: Vec::new(),
            active_tab: None,
            history: BrowserHistory::new(Location::Results(Page::HumanManager)),
            club_snapshots: HashMap::new(),
            player_snapshots: HashMap::new(),
            staff_snapshots: HashMap::new(),
            selected_player: None,
            selected_staff: None,
            selected_club: None,
            loading_entity: false,
            player_tab: player_panel::PlayerTab::Overview,
            staff_tab: staff_panel::StaffTab::Overview,
            club_tab: club_panel::ClubTab::Overview,
            manager_view: None,
            manager_loading: false,
            undo_stack: UndoStack::new(),
            writes: WriteTracker::new(),
            write_error: None,
            status_msg: None,
            entity_counts: None,
            show_history: false,
            settings,
            pending_scale_resize: None,
            #[cfg(debug_assertions)]
            mcp_state: None,
        }
    }

    /// Keep the preview/worktree identity when the attached game changes the title.
    pub fn with_branch(mut self, branch: Option<String>) -> Self {
        self.window_branch = branch;
        self
    }

    #[cfg(debug_assertions)]
    pub fn with_mcp_state(mut self, state: std::sync::Arc<crate::debug::DebugState>) -> Self {
        self.mcp_state = Some(state);
        self
    }

    // --------------------------------------------------
    // Bridge result processing
    // --------------------------------------------------

    /// Wipe all entity state on disconnect / save-unload transitions.
    /// Does NOT touch the entity cache — call `clear_cache()` separately
    /// when stale data must be discarded (e.g. save unloaded).
    fn clear_entity_state(&mut self) {
        self.entity_counts = None;
        self.selected_player = None;
        self.selected_staff = None;
        self.selected_club = None;
        self.manager_view = None;
        self.manager_loading = false;
        self.loading_entity = false;
        self.open_tabs.clear();
        self.active_tab = None;
        self.history = BrowserHistory::new(Location::Results(self.active_page));
        self.club_snapshots.clear();
        self.player_snapshots.clear();
        self.staff_snapshots.clear();
        self.undo_stack.clear();
        self.writes.clear();
        self.write_error = None;
    }

    /// Drop the entity cache — called when a save is unloaded or a new one
    /// starts loading. Not called on GameClosed (cache is preserved for
    /// read-only browsing).
    fn clear_cache(&mut self) {
        self.best_xi.clear();
        self.entity_cache = None;
        self.cache_load_state = CacheLoadState::Idle;
    }

    fn process_results(&mut self) {
        while let Some(result) = self.bridge.try_recv() {
            match result {
                BridgeResult::Connected(pid, counts) => {
                    self.clear_entity_state();
                    self.clear_cache();
                    self.conn = ConnState::Ready(pid);
                    let cache_total = counts.players + counts.staff + counts.clubs;
                    self.entity_counts = Some(counts);
                    self.cache_load_state = CacheLoadState::Loading {
                        done: 0,
                        total: cache_total,
                    };
                    let game = self.game_label(pid);
                    self.set_status(format!("Connected to {game}"));
                }
                BridgeResult::Attached(pid, build) => {
                    self.conn = ConnState::WaitingForSave(pid);
                    self.clear_entity_state();
                    self.clear_cache();
                    self.attached_build = Some(build);
                    self.game_choices.clear();
                    let game = self.game_label(pid);
                    self.set_status(format!("Attached to {game} \u{2014} waiting for save"));
                }
                BridgeResult::ChooseGame(games) => {
                    // A stale auto-connect answer after a game was attached.
                    if !matches!(self.conn, ConnState::Disconnected | ConnState::ReadOnly(_)) {
                        continue;
                    }
                    if games != self.game_choices {
                        self.set_status(format!(
                            "{} games running \u{2014} choose one to attach",
                            games.len()
                        ));
                    }
                    self.game_choices = games;
                }
                BridgeResult::SaveUnloaded(pid) => {
                    self.conn = ConnState::WaitingForSave(pid);
                    self.clear_entity_state();
                    self.clear_cache();
                    self.set_status("Save unloaded \u{2014} waiting for next save".into());
                }
                BridgeResult::GameClosed => {
                    // Game exited while save was loaded — enter read-only.
                    // Cache is preserved; entity_counts stays for the header.
                    if let ConnState::Ready(pid) = self.conn {
                        self.conn = ConnState::ReadOnly(pid);
                    }
                    self.set_status("Game not running \u{2014} browse only".into());
                }
                BridgeResult::CacheProgress { done, total } => {
                    self.cache_load_state = CacheLoadState::Loading { done, total };
                }
                BridgeResult::CacheFailed(e) => {
                    // Leave the loading state so the status bar (hidden by the
                    // progress bar) shows why the save could not be loaded.
                    self.cache_load_state = CacheLoadState::Idle;
                    self.set_status(format!("Loading save failed: {e}"));
                }
                BridgeResult::CacheReady(arc) => {
                    self.entity_cache = Some(arc);
                    self.cache_load_state = CacheLoadState::Ready;
                }
                BridgeResult::PingOk => {}
                BridgeResult::PingFail => {
                    self.conn = ConnState::Disconnected;
                    self.clear_entity_state();
                    self.clear_cache();
                    self.attached_build = None;
                    self.set_status("Disconnected".into());
                }
                BridgeResult::PlayerLoaded(rec) => {
                    self.player_snapshots.insert(rec.addr, rec.clone());
                    if self.is_active_entity(EntityTabType::Player, rec.addr) {
                        self.loading_entity = false;
                        self.selected_player = Some(rec);
                    }
                }
                BridgeResult::StaffLoaded(view) => {
                    self.staff_snapshots.insert(view.record.addr, view.clone());
                    if self.is_active_entity(EntityTabType::Staff, view.record.addr) {
                        self.loading_entity = false;
                        self.selected_staff = Some(view);
                    }
                }
                BridgeResult::ClubLoaded(view) => {
                    self.club_snapshots.insert(view.addr, view.clone());
                    if self.is_active_entity(EntityTabType::Club, view.addr) {
                        self.loading_entity = false;
                        self.selected_club = Some(view);
                    }
                }
                BridgeResult::HumanManagerLoaded(view) => {
                    self.manager_loading = false;
                    self.manager_view = Some(view);
                }
                BridgeResult::WriteOk { id } => self.complete_write(id, true, None),
                BridgeResult::WriteFail { id, error } => {
                    self.complete_write(id, false, Some(error))
                }
                BridgeResult::Error(e) => {
                    // While detached, errors come from attach attempts: the
                    // games offered earlier may be gone, so stop offering them.
                    if matches!(self.conn, ConnState::Disconnected | ConnState::ReadOnly(_)) {
                        self.game_choices.clear();
                    }
                    self.loading_entity = false;
                    self.manager_loading = false;
                    self.set_status(format!("Error: {e}"));
                }
            }
        }
    }

    fn is_active_entity(&self, entity_type: EntityTabType, addr: u64) -> bool {
        self.active_tab
            .and_then(|i| self.open_tabs.get(i))
            .is_some_and(|tab| {
                tab.entity_type == entity_type
                    && tab.addr == addr
                    && tab.entity_type.home_page() == self.active_page
            })
    }

    /// "FM27 (PID 1234)" for the attached game; just the PID before the
    /// version is known.
    fn game_label(&self, pid: u32) -> String {
        match self.attached_build {
            Some(build) => RunningGame { pid, build }.to_string(),
            None => format!("PID {pid}"),
        }
    }

    fn set_status(&mut self, msg: String) {
        self.status_msg = Some((msg, Instant::now()));
    }

    fn handle_auto_connect(&mut self) {
        // Retry attach when disconnected OR in read-only mode (game exited).
        // In WaitingForSave, Ping handles the promotion — no need to re-attach.
        if self.auto_connect
            && matches!(self.conn, ConnState::Disconnected | ConnState::ReadOnly(_))
            && self.last_auto_connect.elapsed().as_secs() >= 3
        {
            self.bridge.send(BridgeCmd::Attach(None));
            self.last_auto_connect = Instant::now();
        }
    }

    fn handle_ping(&mut self) {
        // Ping when attached and game is running (Ready or WaitingForSave).
        // Don't ping in ReadOnly — the worker has no FmProcess; it would
        // just return PingFail and we'd wipe the cache unnecessarily.
        if matches!(
            self.conn,
            ConnState::Ready(_) | ConnState::WaitingForSave(_)
        ) && self.last_ping.elapsed().as_secs() >= 2
        {
            self.bridge.send(BridgeCmd::Ping);
            self.last_ping = Instant::now();
        }
    }
    fn reconcile_write_buffers(&mut self, entry: &UndoEntry, was_undo: bool, success: bool) {
        // Failed edits restore the last known bytes. Failed undo never changed the UI.
        if was_undo && !success {
            return;
        }
        let bytes = if was_undo || !success {
            &entry.old_raw
        } else {
            &entry.new_raw
        };
        for p in self
            .player_snapshots
            .values_mut()
            .chain(self.selected_player.iter_mut())
        {
            patch_player_buffer(
                &mut p.raw,
                p.addr,
                entry.addr,
                bytes,
                p.build.schemas().player_person_offset,
                p.person_shift,
            );
            if let (Some(raw), Some(base)) = (&mut p.contract_raw, p.contract_addr) {
                patch_buffer(raw, base, entry.addr, bytes);
            }
        }
        // A human manager's identity edits land `identity_shift` bytes past
        // the schema offset; map them back before patching the buffer.
        let normalized = |s: &StaffView| {
            let schema = s.record.build.schemas();
            let shift = s.record.identity_shift as u64;
            entry
                .addr
                .checked_sub(s.record.addr)
                .and_then(|rel| rel.checked_sub(shift))
                .filter(|rel| shift != 0 && schema.is_manager_shifted_offset(*rel))
                .map(|rel| s.record.addr + rel)
                .unwrap_or(entry.addr)
        };
        for s in self
            .staff_snapshots
            .values_mut()
            .chain(self.selected_staff.iter_mut())
        {
            let at = normalized(s);
            patch_buffer(&mut s.record.raw, s.record.addr, at, bytes);
            if let (Some(raw), Some(base)) = (&mut s.contract_raw, s.contract_addr) {
                patch_buffer(raw, base, entry.addr, bytes);
            }
        }
        if let Some(s) = &mut self.manager_view {
            if let (Some(raw), Some(base)) = (&mut s.contract_raw, s.contract_addr) {
                patch_buffer(raw, base, entry.addr, bytes);
            }
            let at = normalized(s);
            patch_buffer(&mut s.record.raw, s.record.addr, at, bytes);
        }
        for c in self
            .club_snapshots
            .values_mut()
            .chain(self.selected_club.iter_mut())
        {
            patch_buffer(&mut c.raw, c.addr, entry.addr, bytes);
            for (raw, base) in [
                (&mut c.info_raw, c.info_addr),
                (&mut c.finance_raw, c.finance_addr),
                (&mut c.budget_raw, c.budget_addr),
                (&mut c.senior_team_raw, c.senior_team_addr),
            ] {
                if let (Some(raw), Some(base)) = (raw, base) {
                    patch_buffer(raw, base, entry.addr, bytes);
                }
            }
        }
    }

    // Browser navigation intentionally crosses entity and sidebar boundaries.
    fn push_history(&mut self, entry: HistoryEntry) {
        self.history.visit(Location::Entity(entry));
    }

    fn can_nav_back(&self) -> bool {
        self.history.can_back()
    }
    fn can_nav_forward(&self) -> bool {
        self.history.can_forward()
    }

    fn activate_location(&mut self, location: Location) {
        match location {
            Location::Results(page) => {
                self.active_page = page;
                self.active_tab = None;
                self.loading_entity = false;
            }
            Location::Entity(entry) => self.activate_history_entry(entry),
        }
    }

    fn complete_write(&mut self, id: u64, success: bool, error: Option<String>) {
        let Some(completion) = self.writes.complete(id, success) else {
            return;
        };
        self.reconcile_write_buffers(&completion.entry, completion.was_undo, completion.success);
        if completion.success {
            if completion.was_undo {
                self.set_status(format!("Undone: {}", completion.entry.description));
            } else {
                self.set_status(format!("Saved: {}", completion.entry.description));
                self.undo_stack.push(completion.entry);
            }
        } else {
            let message = format!(
                "Write failed: {}. The unsuccessful change was not added to Undo; displayed bytes have been restored.",
                error.as_deref().unwrap_or("Unknown error")
            );
            self.write_error = Some(message);
            if completion.was_undo {
                self.undo_stack.push(completion.entry);
            }
        }
        if !self.writes.is_pending() {
            if let Some(tab_idx) = self.active_tab {
                self.reload_tab(tab_idx);
            } else if self.active_page == Page::HumanManager && self.conn.is_ready() {
                self.bridge.send(BridgeCmd::LoadHumanManager);
                self.manager_loading = true;
            }
        }
    }

    fn restore_tab_snapshot(&mut self, tab_idx: usize) -> bool {
        let Some(build) = self.attached_build else {
            return false;
        };
        let Some(tab) = self.open_tabs.get(tab_idx) else {
            return false;
        };
        match tab.entity_type {
            EntityTabType::Player => {
                self.selected_player =
                    self.player_snapshots.get(&tab.addr).cloned().or_else(|| {
                        self.entity_cache
                            .as_ref()?
                            .players
                            .iter()
                            .find(|p| p.addr == tab.addr)
                            .map(|cached| cached_player_record(cached, build))
                    });
                self.selected_player.is_some()
            }
            EntityTabType::Staff => {
                self.selected_staff = self.staff_snapshots.get(&tab.addr).cloned().or_else(|| {
                    self.entity_cache
                        .as_ref()?
                        .staff
                        .iter()
                        .find(|s| s.addr == tab.addr)
                        .map(|cached| cached_staff_view(cached, build))
                });
                self.selected_staff.is_some()
            }
            EntityTabType::Club => {
                self.selected_club = self.club_snapshots.get(&tab.addr).cloned();
                self.selected_club.is_some()
            }
        }
    }

    /// Dispatch the appropriate Load command for a tab, flipping
    /// `loading_entity` so the UI shows a spinner until the bridge replies.
    fn reload_tab(&mut self, tab_idx: usize) {
        if let Some(tab) = self.open_tabs.get(tab_idx) {
            if !self.conn.is_ready() {
                self.restore_tab_snapshot(tab_idx);
                self.loading_entity = false;
                return;
            }
            let cmd = match tab.entity_type {
                EntityTabType::Player => BridgeCmd::LoadPlayer(tab.addr, tab.index),
                EntityTabType::Staff => BridgeCmd::LoadStaff(tab.addr, tab.index),
                EntityTabType::Club => BridgeCmd::LoadClub(tab.addr, tab.index),
            };
            self.bridge.send(cmd);
            self.loading_entity = true;
        }
    }

    /// Reuse or reopen a visited entity, switching to its owning search page.
    fn activate_history_entry(&mut self, entry: HistoryEntry) {
        self.active_page = entry.entity_type.home_page();
        let existing = self
            .open_tabs
            .iter()
            .position(|t| t.entity_type == entry.entity_type && t.addr == entry.addr);
        let tab_idx = if let Some(idx) = existing {
            idx
        } else {
            self.open_tabs.push(EntityTab {
                entity_type: entry.entity_type,
                display_name: entry.display_name,
                addr: entry.addr,
                index: entry.index,
            });
            self.open_tabs.len() - 1
        };
        self.active_tab = Some(tab_idx);
        self.restore_tab_snapshot(tab_idx);
        self.reload_tab(tab_idx);
    }
}

// ---------------------------------------------------------------------------
// eframe::App
// ---------------------------------------------------------------------------

impl eframe::App for FmEditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply deferred window resize — must happen the frame AFTER
        // set_zoom_factor so the new pixels_per_point is already active.
        // Skip when maximized: the window already fills the screen, so only
        // the zoom matters — no InnerSize needed and it would un-maximize.
        if let Some(f) = self.pending_scale_resize.take() {
            let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
            if !maximized {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                    1280.0 * f,
                    800.0 * f,
                )));
            }
        }

        // Background housekeeping (runs before UI so results are fresh)
        self.process_results();
        self.handle_auto_connect();
        let title = editor_title(self.attached_build, self.window_branch.as_deref());
        if ctx.input(|i| i.viewport().title.as_deref() != Some(title.as_str())) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
        // Let the fm-debug MCP server follow the game this window shows when
        // several are running.
        #[cfg(debug_assertions)]
        if let Some(ref mcp) = self.mcp_state {
            let pid = match self.conn {
                ConnState::Ready(pid) | ConnState::WaitingForSave(pid) => pid,
                ConnState::ReadOnly(_) | ConnState::Disconnected => 0,
            };
            mcp.editor_pid
                .store(pid, std::sync::atomic::Ordering::Relaxed);
        }
        self.handle_ping();
        ctx.request_repaint_after(Duration::from_secs(1));

        // Auto-load human manager when navigating to that page — only
        // when a save is actually loaded, not just when fm.exe is attached.
        if self.active_page == Page::HumanManager
            && self.conn.is_ready()
            && self.manager_view.is_none()
            && !self.manager_loading
        {
            self.bridge.send(BridgeCmd::LoadHumanManager);
            self.manager_loading = true;
        }

        // Publish active theme into egui temp data so widget code can read it
        // without needing an explicit parameter (used by value_color in field_widget).
        // Also enforce the configured theme's visuals so eframe/winit system-theme
        // events after `CreationContext` cannot revert the user's saved theme.
        ctx.set_visuals(self.settings.theme.visuals());
        ctx.data_mut(|d| {
            d.insert_temp(theme_data_id(), self.settings.theme);
            d.insert_temp(currency_data_id(), self.settings.currency);
        });
        navigation::set_result_keyboard_enabled(
            ctx,
            !ctx.wants_keyboard_input() && !self.advanced_open,
        );

        // Collect deferred actions during UI rendering
        let mut attach_requested = false;
        let mut attach_pid_requested: Option<u32> = None;
        let mut undo_requested = !ctx.wants_keyboard_input()
            && ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Z));
        let mut edits: Vec<field_widget::FieldEdit> = Vec::new();
        let mut load_tab: Option<usize> = None;
        let mut clicked_player: Option<usize> = None;
        let mut clicked_xi_player: Option<usize> = None;
        let mut clicked_staff: Option<usize> = None;
        let mut clicked_club: Option<usize> = None;
        // Sidebar nav history actions — handled post-closure so we don't
        // hold `self` borrows across UI rendering.
        let mut nav_back_clicked = false;
        let mut nav_forward_clicked = false;
        let mut nav_reload_clicked = false;
        let mut nav_page_clicked = false;
        let mut reload_save_clicked = false;

        // ===== Menu bar =====
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Load Save").clicked() {
                        reload_save_clicked = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Settings", |ui| {
                    ui.menu_button("Theme", |ui| {
                        for theme in [Theme::Dark, Theme::Light, Theme::DarkBlue] {
                            if ui
                                .selectable_label(self.settings.theme == theme, theme.label())
                                .clicked()
                            {
                                self.settings.theme = theme;
                                ctx.set_visuals(theme.visuals());
                                self.settings.save();
                                ui.close_menu();
                            }
                        }
                    });
                    ui.menu_button("Scale", |ui| {
                        let mut idx = self.settings.scale.to_index() as i32;
                        let labels = ["100%", "125%", "150%"];
                        ui.label(labels[idx as usize]);
                        if ui
                            .add(egui::Slider::new(&mut idx, 0..=2).show_value(false))
                            .changed()
                        {
                            let new_scale = Scale::from_index(idx as u8);
                            self.settings.scale = new_scale;
                            let f = new_scale.zoom_factor();
                            ctx.set_zoom_factor(f);
                            self.pending_scale_resize = Some(f);
                            self.settings.save();
                        }
                    });
                    ui.menu_button("Wage Display", |ui| {
                        for period in [WagePeriod::Weekly, WagePeriod::Monthly, WagePeriod::Yearly]
                        {
                            if ui
                                .selectable_label(
                                    self.settings.wage_period == period,
                                    period.label(),
                                )
                                .clicked()
                            {
                                self.settings.wage_period = period;
                                self.settings.save();
                                ui.close_menu();
                            }
                        }
                    });
                    ui.menu_button("Currency", |ui| {
                        for currency in [
                            Currency::Eur,
                            Currency::Usd,
                            Currency::Gbp,
                            Currency::Idr,
                        ] {
                            if ui
                                .selectable_label(
                                    self.settings.currency == currency,
                                    currency.label(),
                                )
                                .clicked()
                            {
                                self.settings.currency = currency;
                                self.settings.save();
                                ui.close_menu();
                            }
                        }
                    });
                    ui.separator();
                    let mut rts = self.settings.reload_on_tab_switch;
                    if ui.checkbox(&mut rts, "Reload on tab switch").changed() {
                        self.settings.reload_on_tab_switch = rts;
                        self.settings.save();
                    }
                    ui.menu_button("Face Pack", |ui| {
                        let current = self.settings.face_pack_path.as_deref().unwrap_or("(none)");
                        ui.label(current);
                        if ui.button("Set path\u{2026}").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.settings.face_pack_path =
                                    Some(path.to_string_lossy().into_owned());
                                self.settings.save();
                            }
                            ui.close_menu();
                        }
                        if self.settings.face_pack_path.is_some() && ui.button("Clear").clicked() {
                            self.settings.face_pack_path = None;
                            self.settings.save();
                            ui.close_menu();
                        }
                    });
                });
            });
        });

        // Clone the Arc so we can borrow entity data inside egui closures
        // without conflicting with the &mut self borrow the closures hold.
        let cache_arc = self.entity_cache.clone();
        navigation::publish_cache(ctx, cache_arc.clone());
        navigation::set_editing_enabled(ctx, self.conn.is_writable() && !self.writes.is_pending());
        let wage_period = self.settings.wage_period;
        let face_pack_path = self.settings.face_pack_path.clone();
        let schemas = self.attached_build.map(GameBuild::schemas);
        // Only filter the active page — running all three every frame on a
        // large save (50k+ players) causes visible lag and hangs.
        let player_filtered: Vec<&CachedPlayer> = if self.active_page == Page::PlayerSearch {
            if let (Some(c), Some(schemas)) = (&cache_arc, schemas) {
                pages::player_search::filter_with_clubs(
                    schemas, &c.players, &self.player_search, c.my_clubs.as_ref(),
                )
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let staff_filtered: Vec<&CachedStaff> = if self.active_page == Page::StaffSearch {
            if let (Some(c), Some(schemas)) = (&cache_arc, schemas) {
                pages::staff_search::filter(schemas, &c.staff, &self.staff_search)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let club_filtered: Vec<&CachedClub> = if self.active_page == Page::ClubSearch {
            if let Some(c) = &cache_arc {
                pages::club_search::filter(&c.clubs, &self.club_search)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // ===== Sidebar =====
        egui::SidePanel::left("sidebar")
            .default_width(196.0)
            .min_width(176.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.heading(editor_title(self.attached_build, None));
                ui.label(
                    egui::RichText::new("YOUR SAVE · YOUR WORKSPACE")
                        .small()
                        .weak(),
                );
                ui.add_space(12.0);

                // History + reload row — sits right under the title so
                // Back / Forward / Reload are always the first thing the
                // eye hits in the sidebar.
                let can_back = self.can_nav_back();
                let can_forward = self.can_nav_forward();
                let has_current = self.active_tab.is_some()
                    || (self.active_page == Page::HumanManager && self.manager_view.is_some());
                ui.horizontal(|ui| {
                    let back_resp = ui.add_enabled(
                        can_back,
                        egui::Button::new(egui::RichText::new("\u{2B05}").size(14.0))
                            .min_size(egui::vec2(28.0, 22.0)),
                    );
                    if back_resp.clicked() {
                        nav_back_clicked = true;
                    }
                    if !can_back {
                        back_resp.on_disabled_hover_text("No earlier entry");
                    } else {
                        back_resp.on_hover_text("Back");
                    }

                    let fwd_resp = ui.add_enabled(
                        can_forward,
                        egui::Button::new(egui::RichText::new("\u{27A1}").size(14.0))
                            .min_size(egui::vec2(28.0, 22.0)),
                    );
                    if fwd_resp.clicked() {
                        nav_forward_clicked = true;
                    }
                    if !can_forward {
                        fwd_resp.on_disabled_hover_text("No later entry");
                    } else {
                        fwd_resp.on_hover_text("Forward");
                    }

                    let reload_resp = ui.add_enabled(
                        has_current,
                        egui::Button::new(egui::RichText::new("\u{21BB}").size(14.0))
                            .min_size(egui::vec2(28.0, 22.0)),
                    );
                    if reload_resp.clicked() {
                        nav_reload_clicked = true;
                    }
                    if !has_current {
                        reload_resp.on_disabled_hover_text("Nothing to reload");
                    } else {
                        reload_resp.on_hover_text("Reload from memory");
                    }
                });

                ui.separator();
                ui.add_space(8.0);
                ui.label(egui::RichText::new("BROWSE").small().strong().weak());
                for (page, label) in [
                    (Page::BestXi, "Best XI"),
                    (Page::PlayerSearch, "Players"),
                    (Page::StaffSearch, "Staff"),
                    (Page::ClubSearch, "Clubs"),
                ] {
                    let selected = self.active_page == page;
                    if ui
                        .add_sized(
                            [ui.available_width(), 30.0],
                            egui::Button::new(label).selected(selected).frame(selected),
                        )
                        .clicked()
                    {
                        self.active_page = page;
                        nav_page_clicked = true;
                    }
                }
                ui.add_space(12.0);
                ui.label(egui::RichText::new("MANAGEMENT").small().strong().weak());
                let selected = self.active_page == Page::HumanManager;
                if ui
                    .add_sized(
                        [ui.available_width(), 30.0],
                        egui::Button::new("Human Manager")
                            .selected(selected)
                            .frame(selected),
                    )
                    .clicked()
                {
                    self.active_page = Page::HumanManager;
                    nav_page_clicked = true;
                }
                ui.add_space(20.0);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new("GAME CONNECTION")
                            .small()
                            .strong()
                            .weak(),
                    );
                    ui.add_space(6.0);

                    // Connection status
                    match self.conn {
                        ConnState::Ready(_) => {
                            ui.colored_label(
                                egui::Color32::from_rgb(76, 175, 80),
                                "\u{25CF} Connected",
                            );
                        }
                        ConnState::WaitingForSave(_) => {
                            ui.colored_label(
                                egui::Color32::from_rgb(255, 193, 7),
                                "\u{25CF} Waiting for save",
                            );
                        }
                        ConnState::ReadOnly(_) => {
                            ui.colored_label(
                                egui::Color32::from_rgb(255, 152, 0),
                                "\u{25CF} Game not running",
                            );
                            if ui.button("Reconnect").clicked() {
                                attach_requested = true;
                            }
                        }
                        ConnState::Disconnected => {
                            ui.colored_label(
                                egui::Color32::from_rgb(244, 67, 54),
                                "\u{25CF} Disconnected",
                            );
                            if ui.button("Load Game").clicked() {
                                attach_requested = true;
                            }
                        }
                    }
                    // Several fm.exe running: one button per game. Shown while no
                    // game is attached (including read-only after a game exits).
                    if matches!(self.conn, ConnState::Disconnected | ConnState::ReadOnly(_))
                        && !self.game_choices.is_empty()
                    {
                        ui.label("Attach:");
                        for game in &self.game_choices {
                            if ui.button(game.to_string()).clicked() {
                                attach_pid_requested = Some(game.pid);
                            }
                        }
                    }
                    ui.checkbox(&mut self.auto_connect, "Auto-connect");

                    // Reload Save button — available when connected + cache ready
                    if matches!(self.conn, ConnState::Ready(_))
                        && self.cache_load_state == CacheLoadState::Ready
                    {
                        if ui.button("\u{21BB} Reload Save").clicked() {
                            reload_save_clicked = true;
                        }
                    }
                });
            });

        // ===== Status Bar =====
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            // Progress bar — shown while cache is loading
            if let CacheLoadState::Loading { done, total } = self.cache_load_state {
                let pct = if total > 0 {
                    done as f32 / total as f32
                } else {
                    0.0
                };
                ui.horizontal(|ui| {
                    ui.label("Loading save\u{2026}");
                    ui.add(
                        egui::ProgressBar::new(pct)
                            .show_percentage()
                            .desired_width(220.0),
                    );
                });
                return;
            }

            ui.horizontal(|ui| {
                match self.conn {
                    ConnState::Ready(pid) => {
                        ui.colored_label(
                            egui::Color32::from_rgb(76, 175, 80),
                            format!("\u{25CF} Connected to {}", self.game_label(pid)),
                        );
                    }
                    ConnState::WaitingForSave(pid) => {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 193, 7),
                            format!(
                                "\u{25CF} Attached to {} \u{2014} waiting for save",
                                self.game_label(pid)
                            ),
                        );
                    }
                    ConnState::ReadOnly(_) => {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 152, 0),
                            "\u{25CF} Game not running \u{2014} browse only",
                        );
                    }
                    ConnState::Disconnected => {
                        ui.colored_label(
                            egui::Color32::from_rgb(244, 67, 54),
                            "\u{25CF} Disconnected",
                        );
                    }
                }
                ui.separator();
                if let Some(ref c) = self.entity_counts {
                    ui.label(format!(
                        "Players: {}  Staff: {}  Clubs: {}",
                        c.players, c.staff, c.clubs
                    ));
                }
                ui.separator();
                if let Some((ref msg, t)) = self.status_msg {
                    if t.elapsed().as_secs() < 8 {
                        ui.label(msg.as_str());
                    }
                }

                // Debug-only: MCP traffic LED — dim green when idle, bright when
                // a tool call was made in the last 300 ms (HDD-LED style).
                #[cfg(debug_assertions)]
                if let Some(ref mcp) = self.mcp_state {
                    use std::sync::atomic::Ordering;
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    let last_ms = mcp.last_activity_ms.load(Ordering::Relaxed);
                    let active = last_ms > 0 && now_ms.saturating_sub(last_ms) < 300;
                    let color = if active {
                        egui::Color32::from_rgb(0, 230, 100) // bright: recent activity
                    } else {
                        egui::Color32::from_rgb(0, 80, 40) // dim: idle
                    };
                    ui.separator();
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(), 5.0, color);
                    ui.label(format!("MCP :{}", mcp.port));
                }
            });
        });

        // Entity tabs now live inline inside the central panel, below each
        // search page's filter row — filtered to only show tabs matching
        // the active page's entity type, so e.g. a Player tab doesn't leak
        // into the Club Search view.

        // ===== Central Panel =====
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if self.writes.is_pending() {
                    ui.spinner();
                    ui.label("Applying changes to game memory…");
                } else if self.conn.is_writable() {
                    ui.label("Editing live — changes apply immediately. Ctrl+Z to undo.");
                } else {
                    ui.label("Read-only — editing is disabled until a game and save are connected.");
                }
                ui.separator();
                let undo_label = format!("Undo ({})", self.undo_stack.len());
                let undo_hint = self.undo_stack.last()
                    .map(|e| format!("Undo: {}", e.description))
                    .unwrap_or_else(|| "No confirmed edits to undo".to_string());
                if ui.add_enabled(
                    self.conn.is_writable() && !self.writes.is_pending() && !self.undo_stack.is_empty(),
                    egui::Button::new(undo_label),
                ).on_hover_text(undo_hint).clicked() {
                    undo_requested = true;
                }
                if ui.button("Change history").clicked() {
                    self.show_history = !self.show_history;
                }
            });
            if let Some(error) = self.write_error.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    if ui.small_button("Dismiss").clicked() { self.write_error = None; }
                });
            }
            ui.separator();
            let Some(schemas) = schemas else {
                ui.label("Waiting for fm.exe\u{2026}");
                return;
            };
            match self.active_page {
                Page::BestXi => {
                    clicked_xi_player = pages::best_xi::show(
                        ui, &mut self.best_xi, schemas, cache_arc.as_ref(),
                    );
                }
                // ---- Search pages: filter row always visible, results area swaps ----
                Page::PlayerSearch | Page::StaffSearch | Page::ClubSearch => {
                    // Filter row — always rendered, live filtering each frame.
                    match self.active_page {
                        Page::PlayerSearch => {
                            if pages::player_search::filter_row(
                                ui, &mut self.player_search, Some(schemas),
                                cache_arc.as_ref().and_then(|c| c.my_clubs.as_ref()),
                            )
                                && !self.advanced_open
                            {
                                self.advanced_snapshot = Some(self.player_search.clone());
                                self.advanced_open = true;
                            }
                        }
                        Page::StaffSearch  => pages::staff_search::filter_row(ui, &mut self.staff_search),
                        Page::ClubSearch   => pages::club_search::filter_row(ui, &mut self.club_search),
                        Page::HumanManager | Page::BestXi => unreachable!(),
                    }

                    ui.add_space(4.0);

                    // Results badge + inline entity tabs. The badge is
                    // always the leftmost item so the count stays visible
                    // while a detail panel is open. Clicking it clears
                    // the active tab and restores the results grid.
                    // Each open tab for this page renders as "[Name] ×".
                    // The count reflects the CURRENT search results, not
                    // the total entity count in the save.
                    let count_text = match self.active_page {
                        Page::PlayerSearch => format!("Results ({})", player_filtered.len()),
                        Page::StaffSearch  => format!("Results ({})", staff_filtered.len()),
                        Page::ClubSearch   => format!("Results ({})", club_filtered.len()),
                        Page::HumanManager | Page::BestXi => "Results".to_string(),
                    };
                    let mut close_idx: Option<usize> = None;
                    let mut results_badge_clicked = false;
                    ui.horizontal(|ui| {
                        let results_selected = self.active_tab.is_none();
                        if ui.selectable_label(results_selected, count_text).clicked() {
                            self.active_tab = None;
                            results_badge_clicked = true;
                        }
                        for i in 0..self.open_tabs.len() {
                            if self.open_tabs[i].entity_type.home_page()
                                != self.active_page
                            {
                                continue;
                            }
                            let selected = self.active_tab == Some(i);
                            let name = self.open_tabs[i].display_name.clone();
                            // Render name + × as a single Button so the
                            // two halves share one selection background.
                            // Click position tells us which half was hit:
                            // the rightmost ~18px is the close hotspot.
                            let text = format!("{name}   \u{2715}");
                            let resp = ui.add(
                                egui::Button::new(text)
                                    .selected(selected)
                                    .min_size(egui::vec2(0.0, 22.0)),
                            );
                            if resp.clicked() {
                                let close_zone = 18.0;
                                let clicked_close = resp
                                    .interact_pointer_pos()
                                    .map(|p| p.x > resp.rect.right() - close_zone)
                                    .unwrap_or(false);
                                if clicked_close {
                                    close_idx = Some(i);
                                } else if !selected {
                                    // Record the tab switch in history so
                                    // Back walks between the open tabs.
                                    // Snapshot fields before we mutate self
                                    // so the borrow checker stays happy.
                                    let entry = HistoryEntry {
                                        entity_type: self.open_tabs[i].entity_type,
                                        addr: self.open_tabs[i].addr,
                                        index: self.open_tabs[i].index,
                                        display_name: self.open_tabs[i].display_name.clone(),
                                    };
                                    self.active_tab = Some(i);
                                    load_tab = Some(i);
                                    self.push_history(entry);
                                }
                            }
                        }
                    });
                    if let Some(i) = close_idx {
                        self.open_tabs.remove(i);
                        match self.active_tab {
                            Some(idx) if idx == i => self.active_tab = None,
                            Some(idx) if idx > i => self.active_tab = Some(idx - 1),
                            _ => {}
                        }
                    }
                    // Clicking the Results badge should also sync the
                    // page's history idx to None so Back is disabled
                    // (we're now at the implicit "results" entry).
                    if results_badge_clicked || (close_idx.is_some() && self.active_tab.is_none()) {
                        self.loading_entity = false;
                        self.history.visit(Location::Results(self.active_page));
                    }
                    ui.add_space(4.0);

                    // Idle-state short-circuit.
                    let has_cache = matches!(self.cache_load_state, CacheLoadState::Ready);
                    if !self.conn.is_ready() && !has_cache {
                        ui.label(match self.conn {
                            ConnState::Disconnected => "Waiting for fm.exe\u{2026}",
                            ConnState::WaitingForSave(_) =>
                                "Attached to fm.exe \u{2014} load a save to search.",
                            ConnState::ReadOnly(_) => "Game not running \u{2014} loading\u{2026}",
                            ConnState::Ready(_) => "",
                        });
                        return;
                    }

                    // Cache still loading — progress bar shown in status bar;
                    // show a brief note in the results area too.
                    if matches!(self.cache_load_state, CacheLoadState::Loading { .. }) {
                        ui.label("Loading entity cache\u{2026}");
                        return;
                    }

                    // Results area: either a loading spinner (for tab refreshes),
                    // the entity detail panel, or the results grid.
                    if self.loading_entity {
                        ui.centered_and_justified(|ui| {
                            ui.spinner();
                        });
                        return;
                    }

                    // If there's an active tab that matches this page's
                    // entity type, render its detail panel in the results
                    // area. Otherwise fall through to the results grid.
                    let active_tab_matches = self
                        .active_tab
                        .and_then(|i| self.open_tabs.get(i))
                        .map(|t| t.entity_type.home_page() == self.active_page)
                        .unwrap_or(false);

                    if active_tab_matches {
                        let tab_idx = self.active_tab.unwrap();
                        let tab_type = self.open_tabs[tab_idx].entity_type;
                        let tab_addr = self.open_tabs[tab_idx].addr;
                        match tab_type {
                            EntityTabType::Player => {
                                if let Some(p) = self.selected_player.as_mut().filter(|p| p.addr == tab_addr) {
                                    let addr = p.addr;
                                    let clubs = self.entity_cache.as_deref().map(|c| c.clubs.as_slice());
                                    let nations = self.entity_cache.as_deref().map(|c| c.nations.as_slice());
                                    let e = player_panel::show(
                                        schemas, ui, p, &mut self.player_tab, addr,
                                        wage_period, self.settings.currency, face_pack_path.as_deref(),
                                        clubs, nations,
                                    );
                                    edits.extend(e);
                                } else {
                                    ui.label("Player details unavailable. Reload while the game is connected.");
                                }
                            }
                            EntityTabType::Staff => {
                                if let Some(s) = self.selected_staff.as_mut().filter(|s| s.record.addr == tab_addr) {
                                    let e = staff_panel::show(schemas, ui, s, &mut self.staff_tab);
                                    edits.extend(e);
                                } else {
                                    ui.label("Staff details unavailable. Reload while the game is connected.");
                                }
                            }
                            EntityTabType::Club => {
                                if let Some(c) = self.selected_club.as_mut().filter(|c| c.addr == tab_addr) {
                                    let prev_curr = self.settings.currency;
                                    let e = club_panel::show(
                                        schemas,
                                        ui,
                                        c,
                                        &mut self.club_tab,
                                        &mut self.settings.currency,
                                    );
                                    if self.settings.currency != prev_curr {
                                        self.settings.save();
                                    }
                                    edits.extend(e);
                                } else {
                                    ui.label("Club details unavailable. Reload while the game is connected.");
                                }
                            }
                        }
                    } else {
                        match self.active_page {
                            Page::PlayerSearch => {
                                clicked_player = pages::player_search::results_grid(
                                    schemas,
                                    ui,
                                    &mut self.player_search,
                                    &player_filtered,
                                );
                            }
                            Page::StaffSearch => {
                                clicked_staff = pages::staff_search::results_grid(
                                    schemas,
                                    ui,
                                    &mut self.staff_search,
                                    &staff_filtered,
                                );
                            }
                            Page::ClubSearch => {
                                clicked_club = pages::club_search::results_grid(
                                    ui,
                                    &club_filtered,
                                    self.settings.currency,
                                );
                            }
                            Page::HumanManager | Page::BestXi => unreachable!(),
                        }
                    }
                }

                // ---- Human manager: no filter row, dedicated panel ----
                Page::HumanManager => {
                    egui::ScrollArea::vertical().show(ui, |ui| match self.conn {
                        ConnState::Disconnected => {
                            ui.label("Waiting for fm.exe\u{2026}");
                        }
                        ConnState::WaitingForSave(_) => {
                            ui.label(
                                "Attached to fm.exe \u{2014} load a save to view your manager.",
                            );
                        }
                        ConnState::Ready(_) | ConnState::ReadOnly(_) => {
                            if self.manager_loading {
                                ui.spinner();
                            } else if let Some(ref mut mgr) = self.manager_view {
                                let e = staff_panel::show_manager(schemas, ui, mgr);
                                if self.conn.is_writable() {
                                    edits.extend(e);
                                }
                            } else {
                                ui.label("Loading human manager\u{2026}");
                            }
                        }
                    });
                }
            }
        });

        // ===== Edit History window =====
        if self.show_history {
            egui::Window::new("Edit History")
                .open(&mut self.show_history)
                .show(ctx, |ui| {
                    if self.undo_stack.is_empty() {
                        ui.label("No edits yet.");
                    } else {
                        for entry in self.undo_stack.entries().iter().rev() {
                            ui.label(&entry.description);
                        }
                    }
                });
        }

        // ===== Advanced Search modal =====
        if self.advanced_open {
            use advanced_search_modal::ModalResult;
            let result =
                advanced_search_modal::show(ctx, &mut self.player_search, &mut self.advanced_open);
            match result {
                ModalResult::Ok => {
                    self.advanced_snapshot = None;
                }
                ModalResult::Cancel => {
                    if let Some(snapshot) = self.advanced_snapshot.take() {
                        self.player_search = snapshot;
                    }
                }
                ModalResult::ClearAll => {
                    self.player_search.clear_advanced();
                }
                ModalResult::None => {}
            }
        }

        // =================================================================
        // Post-UI action processing (after all closures have completed)
        // =================================================================

        if attach_requested {
            self.bridge.send(BridgeCmd::Attach(None));
        }
        if let Some(pid) = attach_pid_requested {
            self.bridge.send(BridgeCmd::Attach(Some(pid)));
        }

        // A write becomes undoable only after the worker confirms success.
        if undo_requested
            && edits.is_empty()
            && self.conn.is_writable()
            && !self.writes.is_pending()
        {
            if let Some(entry) = self.undo_stack.pop() {
                let cmd = self.writes.submit_undo(entry);
                self.bridge.send(cmd);
            }
        }
        if self.conn.is_writable() && !self.writes.is_pending() && !edits.is_empty() {
            let entity_name = self
                .active_tab
                .and_then(|i| self.open_tabs.get(i))
                .map(|t| t.display_name.as_str())
                .or_else(|| self.manager_view.as_ref().and_then(|m| m.name.as_deref()))
                .unwrap_or("Entity");
            let entries = edits
                .into_iter()
                .map(|edit| UndoEntry {
                    description: format!("{entity_name}: {}", edit.description),
                    addr: edit.addr,
                    old_raw: edit.old_raw,
                    new_raw: edit.new_raw,
                    timestamp: Instant::now(),
                })
                .collect();
            for cmd in self.writes.submit(entries) {
                self.bridge.send(cmd);
            }
        }

        // Handle reload_save — clear stale cache and re-trigger bulk load.
        if reload_save_clicked {
            self.best_xi.clear();
            self.entity_cache = None;
            self.cache_load_state = CacheLoadState::Loading { done: 0, total: 0 };
            self.bridge.send(BridgeCmd::LoadEntityCache);
        }

        if nav_page_clicked {
            self.active_tab = None;
            self.loading_entity = false;
            self.history.visit(Location::Results(self.active_page));
        }

        // (Cache-driven search needs no auto-submit — results appear immediately.)

        if let Some(cached) = clicked_xi_player.and_then(|i| cache_arc.as_ref()?.players.get(i)) {
            let entry = HistoryEntry {
                entity_type: EntityTabType::Player,
                addr: cached.addr,
                index: cached.index,
                display_name: cached.name.clone(),
            };
            self.history.visit(Location::Entity(entry.clone()));
            self.activate_history_entry(entry);
        }

        // Open player from search results — build PlayerRecord from cache (no bridge call)
        if let (Some(i), Some(build)) = (clicked_player, self.attached_build) {
            let cached = player_filtered[i];
            let name = if cached.name.is_empty() {
                "Unknown".to_string()
            } else {
                cached.name.clone()
            };
            let addr = cached.addr;
            let index = cached.index;
            let rec = cached_player_record(cached, build);
            let existing = self
                .open_tabs
                .iter()
                .position(|t| t.addr == addr && t.entity_type == EntityTabType::Player);
            if let Some(idx) = existing {
                self.active_tab = Some(idx);
            } else {
                self.open_tabs.push(EntityTab {
                    entity_type: EntityTabType::Player,
                    display_name: name.clone(),
                    addr,
                    index,
                });
                self.active_tab = Some(self.open_tabs.len() - 1);
            }
            self.selected_player = Some(rec);
            if !self.conn.is_ready() {
                self.restore_tab_snapshot(self.active_tab.unwrap());
            }
            // The cache holds only the struct bytes; load sub-records (contract,
            // club, nationality) so the panel can show them.
            if self.conn.is_ready() {
                self.bridge.send(BridgeCmd::LoadPlayer(addr, index));
            }
            if self.conn.is_ready() {
                self.loading_entity = true;
            }
            self.push_history(HistoryEntry {
                entity_type: EntityTabType::Player,
                addr,
                index,
                display_name: name,
            });
        }

        // Open staff from search results — build StaffView from cache (no bridge call)
        if let (Some(i), Some(build)) = (clicked_staff, self.attached_build) {
            let cached = staff_filtered[i];
            let name = if cached.name.is_empty() {
                "Unknown".to_string()
            } else {
                cached.name.clone()
            };
            let addr = cached.addr;
            let index = cached.index;
            let view = cached_staff_view(cached, build);
            let existing = self
                .open_tabs
                .iter()
                .position(|t| t.addr == addr && t.entity_type == EntityTabType::Staff);
            if let Some(idx) = existing {
                self.active_tab = Some(idx);
            } else {
                self.open_tabs.push(EntityTab {
                    entity_type: EntityTabType::Staff,
                    display_name: name.clone(),
                    addr,
                    index,
                });
                self.active_tab = Some(self.open_tabs.len() - 1);
            }
            self.selected_staff = Some(view);
            if !self.conn.is_ready() {
                self.restore_tab_snapshot(self.active_tab.unwrap());
            }
            if self.conn.is_ready() {
                self.loading_entity = true;
            }
            // The cache holds only the struct bytes; load sub-records
            // (contract, club, nationality) so the panel can show them.
            if self.conn.is_ready() {
                self.bridge.send(BridgeCmd::LoadStaff(addr, index));
            }
            self.push_history(HistoryEntry {
                entity_type: EntityTabType::Staff,
                addr,
                index,
                display_name: name,
            });
        }

        // Open club from search results — still uses bridge (club detail not cached)
        if let Some(i) = clicked_club {
            let cached = club_filtered[i];
            let name = cached.name.clone();
            let addr = cached.addr;
            let index = cached.index;
            let existing = self
                .open_tabs
                .iter()
                .position(|t| t.addr == addr && t.entity_type == EntityTabType::Club);
            if let Some(idx) = existing {
                self.active_tab = Some(idx);
            } else {
                self.open_tabs.push(EntityTab {
                    entity_type: EntityTabType::Club,
                    display_name: name.clone(),
                    addr,
                    index,
                });
                self.active_tab = Some(self.open_tabs.len() - 1);
            }
            if let Some(tab_idx) = self.active_tab {
                self.reload_tab(tab_idx);
            }
            self.push_history(HistoryEntry {
                entity_type: EntityTabType::Club,
                addr,
                index,
                display_name: name,
            });
        }

        // Tab switch — reload entity data from memory (gated on setting)
        if let Some(tab_idx) = load_tab {
            let restored = self.restore_tab_snapshot(tab_idx);
            if self.settings.reload_on_tab_switch || !restored {
                self.reload_tab(tab_idx);
            }
        }

        // Related names open through the same path as tabs and history.
        if let Some(target) = navigation::take_club_target(ctx) {
            let entry = HistoryEntry {
                entity_type: EntityTabType::Club,
                addr: target.addr,
                index: target.index,
                display_name: target.name,
            };
            self.push_history(entry.clone());
            self.activate_history_entry(entry);
        }
        if nav_back_clicked {
            if let Some(location) = self.history.back() {
                self.activate_location(location);
            }
        }
        if nav_forward_clicked {
            if let Some(location) = self.history.forward() {
                self.activate_location(location);
            }
        }
        if nav_reload_clicked {
            if let Some(tab_idx) = self.active_tab {
                self.reload_tab(tab_idx);
            } else if self.active_page == Page::HumanManager && self.conn.is_ready() {
                self.bridge.send(BridgeCmd::LoadHumanManager);
                self.manager_loading = true;
            }
        }
    }
}

/// Patch only complete, address-checked ranges; never truncate or wrap a write.
fn patch_buffer(raw: &mut [u8], base: u64, addr: u64, bytes: &[u8]) {
    let Some(start) = addr.checked_sub(base).and_then(|n| usize::try_from(n).ok()) else {
        return;
    };
    let Some(end) = start.checked_add(bytes.len()) else {
        return;
    };
    if let Some(dst) = raw.get_mut(start..end) {
        dst.copy_from_slice(bytes);
    }
}

fn patch_player_buffer(
    raw: &mut [u8],
    base: u64,
    addr: u64,
    bytes: &[u8],
    person: usize,
    shift: usize,
) {
    if shift == 0 {
        patch_buffer(raw, base, addr, bytes);
    } else if person <= raw.len() {
        let (prefix, person_raw) = raw.split_at_mut(person);
        patch_buffer(prefix, base, addr, bytes);
        if let Some(person_base) = base
            .checked_add(person as u64)
            .and_then(|p| p.checked_add(shift as u64))
        {
            patch_buffer(person_raw, person_base, addr, bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acknowledgement_patches_only_complete_address_ranges() {
        let mut raw = [1, 2, 3, 4];
        patch_buffer(&mut raw, 100_000, 100_001, &[8, 9]);
        assert_eq!(raw, [1, 8, 9, 4]);
        patch_buffer(&mut raw, 100_000, 99_999, &[0]);
        patch_buffer(&mut raw, 100_000, 100_003, &[0, 0]);
        patch_buffer(&mut raw, 100_000, u64::MAX, &[0]);
        assert_eq!(raw, [1, 8, 9, 4]);
    }
    #[test]
    fn acknowledgements_preserve_shifted_player_coach_layout() {
        let mut raw = [1, 2, 3, 4, 5, 6];
        patch_player_buffer(&mut raw, 100_000, 100_001, &[8], 3, 10);
        patch_player_buffer(&mut raw, 100_000, 100_014, &[9], 3, 10);
        assert_eq!(raw, [1, 8, 3, 4, 9, 6]);
        // A physical address in the inserted block cannot patch normalized person bytes.
        patch_player_buffer(&mut raw, 100_000, 100_004, &[0], 3, 10);
        assert_eq!(raw, [1, 8, 3, 4, 9, 6]);
        patch_player_buffer(&mut raw, 100_000, 100_004, &[7], 3, 0);
        assert_eq!(raw, [1, 8, 3, 4, 7, 6]);
    }
}

fn cached_player_record(cached: &CachedPlayer, build: GameBuild) -> PlayerRecord {
    PlayerRecord {
        build,
        addr: cached.addr,
        index: cached.index,
        name: Some(cached.name.clone()),
        raw: cached.raw.clone(),
        person_shift: cached.person_shift,
        contract_raw: None,
        contract_addr: None,
        club_name: None,
        club_addr: None,
        nationality: None,
        birthplace: None,
    }
}

fn cached_staff_view(cached: &CachedStaff, build: GameBuild) -> StaffView {
    StaffView {
        record: StaffRecord {
            build,
            addr: cached.addr,
            index: cached.index,
            raw: cached.raw.clone(),
            identity_shift: cached.identity_shift,
        },
        name: Some(cached.name.clone()),
        roles: Some(cached.roles),
        role_ptr: cached.role_ptr,
        nationality: None,
        club_name: None,
        club_addr: None,
        birthplace: None,
        contract_raw: None,
        contract_addr: None,
    }
}

fn editor_title(build: Option<GameBuild>, branch: Option<&str>) -> String {
    let base = build.map_or_else(
        || "FM Editor".to_string(),
        |v| format!("{} Editor", v.display_name()),
    );
    match branch.filter(|b| !b.is_empty()) {
        Some(branch) => format!("{base} [{branch}]"),
        None => base,
    }
}

#[cfg(test)]
mod title_tests {
    use super::*;

    #[test]
    fn attached_version_and_worktree_are_both_visible() {
        assert_eq!(editor_title(None, None), "FM Editor");
        for version in GameBuild::ALL {
            assert_eq!(
                editor_title(Some(version), None),
                format!("{} Editor", version.display_name())
            );
            assert_eq!(
                editor_title(Some(version), Some("preview")),
                format!("{} Editor [preview]", version.display_name())
            );
        }
        assert_eq!(editor_title(None, Some("preview")), "FM Editor [preview]");
    }
}
