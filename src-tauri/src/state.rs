use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Instant;

use tauri::AppHandle;
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tracing::warn;

use crate::events::EventHub;
use crate::history::{history_path, ClaimHistoryEntry, ClaimPlatform, ClaimRecordStatus, HistoryStore};
use crate::logging::{LogBuffer, LogHandle};
use crate::net::client::AllowlistedClient;
use crate::platforms::{
    AppSnapshot, Campaign, PlatformId, PlatformSnapshot, ProfileSnapshot, RecentClaim,
};
use crate::remote_config::RemoteConfigManager;
use crate::rewards::epic::{EpicFreeGame, EpicTracker};
use crate::settings::{save_settings, Settings};
use crate::webhooks::{WebhookDispatcher, WebhookEvent};

const MAX_RECENT_CLAIMS: usize = 50;

pub type BridgeWaiters = Arc<StdMutex<HashMap<String, oneshot::Sender<Result<String, String>>>>>;

#[derive(Debug, Clone)]
pub enum MinerControl {
    LoginComplete,
    Logout,
    Pause,
    Resume,
    SwitchChannel,
    PinChannel(Option<String>),
    RetryNow,
    SettingsUpdated,
    RefreshNow,
    CampaignDetailsLoaded(Box<Campaign>),
    Shutdown,
}

struct MinerRegistration {
    sender: mpsc::Sender<MinerControl>,
    task: tauri::async_runtime::JoinHandle<()>,
}

pub struct AppState {
    pub settings: Arc<RwLock<Settings>>,
    pub http: Arc<AllowlistedClient>,
    pub logs: LogBuffer,
    pub log_handle: StdMutex<Option<LogHandle>>,
    pub events: EventHub,
    pub remote_config: Arc<RemoteConfigManager>,
    pub epic: Arc<EpicTracker>,
    pub webhooks: Arc<WebhookDispatcher>,
    pub history: Arc<RwLock<HistoryStore>>,
    history_path: PathBuf,
    history_save_lock: Mutex<()>,
    pub twitch_snapshot: Arc<RwLock<PlatformSnapshot>>,
    pub kick_snapshot: Arc<RwLock<PlatformSnapshot>>,
    pub profile_snapshots: RwLock<HashMap<String, PlatformSnapshot>>,
    pub recent_claims: Arc<RwLock<VecDeque<RecentClaim>>>,
    profile_miners: RwLock<HashMap<String, MinerRegistration>>,
    shutdown_lock: Mutex<()>,
    shutting_down: AtomicBool,
    pub kick_bridge_waiters: BridgeWaiters,
    pub twitch_bridge_lock: Arc<Mutex<()>>,
    pub allow_exit: AtomicBool,
    pub close_to_tray: AtomicBool,
    pub mock_active: AtomicBool,
}

impl AppState {
    pub fn new(
        initial_settings: Settings,
        http: Arc<AllowlistedClient>,
        logs: LogBuffer,
    ) -> Self {
        Self::new_with_history_path(initial_settings, http, logs, history_path())
    }

    fn new_with_history_path(
        initial_settings: Settings,
        http: Arc<AllowlistedClient>,
        logs: LogBuffer,
        history_path: PathBuf,
    ) -> Self {
        let mock_active = cfg!(feature = "mock") && initial_settings.mock_mode;
        let close_to_tray = initial_settings.close_to_tray;
        let history_store = HistoryStore::load_from_path(&history_path);
        let initial_claims: VecDeque<RecentClaim> =
            history_store.recent_claimed(MAX_RECENT_CLAIMS).into_iter().collect();
        let remote_config = Arc::new(RemoteConfigManager::load_initial());
        let epic = Arc::new(EpicTracker::load_initial());
        let mut initial_profile_snapshots = HashMap::new();
        for prof in &initial_settings.profiles {
            if crate::edition::is_profile_allowed(prof.platform, &prof.id) {
                initial_profile_snapshots
                    .insert(prof.id.clone(), PlatformSnapshot::logged_out(prof.platform));
            }
        }

        Self {
            settings: Arc::new(RwLock::new(initial_settings)),
            http,
            logs,
            log_handle: StdMutex::new(None),
            events: EventHub::new(),
            remote_config,
            epic,
            webhooks: Arc::new(WebhookDispatcher::new()),
            history: Arc::new(RwLock::new(history_store)),
            history_path,
            history_save_lock: Mutex::new(()),
            twitch_snapshot: Arc::new(RwLock::new(PlatformSnapshot::logged_out(PlatformId::Twitch))),
            kick_snapshot: Arc::new(RwLock::new(PlatformSnapshot::logged_out(PlatformId::Kick))),
            profile_snapshots: RwLock::new(initial_profile_snapshots),
            recent_claims: Arc::new(RwLock::new(initial_claims)),
            profile_miners: RwLock::new(HashMap::new()),
            shutdown_lock: Mutex::new(()),
            shutting_down: AtomicBool::new(false),
            kick_bridge_waiters: Arc::new(StdMutex::new(HashMap::new())),
            twitch_bridge_lock: Arc::new(Mutex::new(())),
            allow_exit: AtomicBool::new(false),
            close_to_tray: AtomicBool::new(close_to_tray),
            mock_active: AtomicBool::new(mock_active),
        }
    }

    pub fn set_log_handle(&self, handle: LogHandle) {
        if let Ok(mut guard) = self.log_handle.lock() {
            *guard = Some(handle);
        }
    }

    pub fn snapshot_lock(&self, platform: PlatformId) -> &Arc<RwLock<PlatformSnapshot>> {
        match platform {
            PlatformId::Twitch => &self.twitch_snapshot,
            PlatformId::Kick => &self.kick_snapshot,
        }
    }

    pub async fn update_profile_snapshot(
        &self,
        profile_id: &str,
        platform: PlatformId,
        snap: PlatformSnapshot,
    ) {
        {
            let mut map = self.profile_snapshots.write().await;
            map.insert(profile_id.to_string(), snap.clone());
        }
        let is_active = self.settings.read().await.active_profile_id(platform) == profile_id;
        if is_active {
            let mut guard = self.snapshot_lock(platform).write().await;
            *guard = snap;
        }
    }

    pub async fn sync_active_platform_snapshot(&self, platform: PlatformId) {
        let active_id = self.settings.read().await.active_profile_id(platform).to_string();
        let active_snap = {
            let map = self.profile_snapshots.read().await;
            map.get(&active_id)
                .cloned()
                .unwrap_or_else(|| PlatformSnapshot::logged_out(platform))
        };
        let mut guard = self.snapshot_lock(platform).write().await;
        *guard = active_snap;
    }

    pub async fn build_snapshot(&self) -> AppSnapshot {
        let (profiles_cfg, active_tw_id, active_kk_id) = {
            let s = self.settings.read().await;
            (
                s.profiles.clone(),
                s.active_profile_id(PlatformId::Twitch).to_string(),
                s.active_profile_id(PlatformId::Kick).to_string(),
            )
        };
        let profile_map = self.profile_snapshots.read().await;
        let profiles: Vec<ProfileSnapshot> = profiles_cfg
            .iter()
            .filter(|prof| crate::edition::is_profile_allowed(prof.platform, &prof.id))
            .map(|prof| {
                let is_active = prof.id == active_tw_id || prof.id == active_kk_id;
                let snapshot = (!is_active).then(|| {
                    profile_map
                        .get(&prof.id)
                        .cloned()
                        .unwrap_or_else(|| PlatformSnapshot::logged_out(prof.platform))
                });
                ProfileSnapshot {
                    profile_id: prof.id.clone(),
                    label: prof.label.clone(),
                    platform: prof.platform,
                    snapshot,
                }
            })
            .collect();

        let twitch = profile_map
            .get(&active_tw_id)
            .cloned()
            .unwrap_or_else(|| PlatformSnapshot::logged_out(PlatformId::Twitch));
        let kick = profile_map
            .get(&active_kk_id)
            .cloned()
            .unwrap_or_else(|| PlatformSnapshot::logged_out(PlatformId::Kick));
        drop(profile_map);

        let claims = self.recent_claims.read().await.iter().cloned().collect();
        AppSnapshot {
            twitch,
            kick,
            profiles,
            recent_claims: claims,
            mock_active: self.mock_active.load(Ordering::Relaxed),
        }
    }

    pub async fn full_snapshot(&self) -> AppSnapshot {
        self.build_snapshot().await
    }

    async fn save_history_snapshot(&self, hist: HistoryStore, operation: &str) {
        let path = self.history_path.clone();
        match tokio::task::spawn_blocking(move || hist.save_to_path(&path)).await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => warn!(error = %err, operation, "Failed persisting history"),
            Err(err) => warn!(error = %err, operation, "History save task failed"),
        }
    }

    async fn persist_history(&self, operation: &str) {
        let _save_guard = self.history_save_lock.lock().await;
        let hist = self.history.read().await.clone();
        self.save_history_snapshot(hist, operation).await;
    }

    pub async fn flush_history(&self) {
        self.persist_history("shutdown").await;
    }

    pub async fn clear_history(&self) {
        let _save_guard = self.history_save_lock.lock().await;
        *self.history.write().await = HistoryStore::default();
        self.recent_claims.write().await.clear();
        self.save_history_snapshot(HistoryStore::default(), "clear").await;
    }

    pub async fn delete_history(&self) {
        let _save_guard = self.history_save_lock.lock().await;
        *self.history.write().await = HistoryStore::default();
        self.recent_claims.write().await.clear();
        let path = self.history_path.clone();
        match tokio::task::spawn_blocking(move || std::fs::remove_file(path)).await {
            Ok(Ok(())) => {}
            Ok(Err(err)) if err.kind() == std::io::ErrorKind::NotFound => {}
            Ok(Err(err)) => warn!(error = %err, "Failed deleting history"),
            Err(err) => warn!(error = %err, "History delete task failed"),
        }
    }

    pub async fn record_claim(&self, claim: RecentClaim) {
        let hist_entry = ClaimHistoryEntry::from(&claim);
        {
            let mut guard = self.recent_claims.write().await;
            guard.retain(|existing| existing.id != claim.id);
            if guard.len() >= MAX_RECENT_CLAIMS {
                guard.pop_back();
            }
            guard.push_front(claim);
        }
        if !self.mock_active.load(Ordering::Relaxed) {
            self.history.write().await.record_claim(hist_entry);
            self.persist_history("claim").await;
        }
    }

    pub async fn record_manual_claim(&self, entry: ClaimHistoryEntry) {
        if !self.mock_active.load(Ordering::Relaxed) {
            self.history.write().await.record_claim(entry);
            self.persist_history("manual claim").await;
        }
    }

    pub async fn record_epic_claim(&self, game: &EpicFreeGame) {
        let entry = ClaimHistoryEntry {
            id: format!("epic-{}", game.id),
            platform: ClaimPlatform::Epic,
            game_name: game.title.clone(),
            campaign_name: "Epic Free Weekly".to_string(),
            drop_name: game.title.clone(),
            reward_image_url: game.box_art_url.clone().or_else(|| game.banner_url.clone()),
            claimed_at: game.claimed_at.unwrap_or_else(chrono::Utc::now),
            status: ClaimRecordStatus::Claimed,
        };
        self.history.write().await.record_claim(entry);
        self.persist_history("Epic claim").await;
    }

    pub async fn remove_epic_claim(&self, game_id: &str) {
        let entry_id = format!("epic-{game_id}");
        self.history.write().await.remove_claim(&entry_id);
        self.persist_history("Epic claim removal").await;
    }

    pub async fn record_watch_minutes(&self, delta_minutes: u32) {
        if delta_minutes == 0 || self.mock_active.load(Ordering::Relaxed) {
            return;
        }
        let save_due = {
            let mut hist = self.history.write().await;
            hist.record_watch_minutes(delta_minutes);
            hist.begin_watch_save(Instant::now())
        };
        if save_due {
            self.persist_history("watch minutes").await;
        }
    }

    pub async fn dispatch_webhook(&self, event: WebhookEvent) {
        if !crate::edition::current_edition().webhooks_enabled {
            return;
        }
        let webhook_settings = self.settings.read().await.webhooks.clone();
        self.webhooks
            .dispatch_async(self.http.clone(), webhook_settings, event);
    }

    async fn join_miner(profile_id: &str, task: tauri::async_runtime::JoinHandle<()>) {
        if let Err(err) = task.await {
            warn!(error = %err, profile_id, "Miner task did not shut down cleanly");
        }
    }

    pub async fn register_miner_for_profile(
        &self,
        profile_id: &str,
        tx: mpsc::Sender<MinerControl>,
        task: tauri::async_runtime::JoinHandle<()>,
    ) {
        let _shutdown_guard = self.shutdown_lock.lock().await;
        if self.shutting_down.load(Ordering::SeqCst) {
            let _ = tx.send(MinerControl::Shutdown).await;
            Self::join_miner(profile_id, task).await;
            self.flush_history().await;
            return;
        }
        self.profile_miners.write().await.insert(
            profile_id.to_string(),
            MinerRegistration { sender: tx, task },
        );
    }

    pub async fn unregister_miner_for_profile(&self, profile_id: &str) {
        let _shutdown_guard = self.shutdown_lock.lock().await;
        let registration = self.profile_miners.write().await.remove(profile_id);
        if let Some(registration) = registration {
            Self::join_miner(profile_id, registration.task).await;
        }
        self.profile_snapshots.write().await.remove(profile_id);
    }

    pub async fn send_profile_miner(&self, profile_id: &str, cmd: MinerControl) {
        let tx = self
            .profile_miners
            .read()
            .await
            .get(profile_id)
            .map(|registration| registration.sender.clone());
        if let Some(sender) = tx {
            let _ = sender.send(cmd).await;
        }
    }

    pub async fn send_miner(&self, platform: PlatformId, cmd: MinerControl) {
        let active_id = self.settings.read().await.active_profile_id(platform).to_string();
        self.send_profile_miner(&active_id, cmd).await;
    }

    pub async fn send_all_miners(&self, cmd: MinerControl) {
        let senders: Vec<mpsc::Sender<MinerControl>> = self
            .profile_miners
            .read()
            .await
            .values()
            .map(|registration| registration.sender.clone())
            .collect();
        for sender in senders {
            let _ = sender.send(cmd.clone()).await;
        }
    }

    pub async fn shutdown(&self) {
        let _shutdown_guard = self.shutdown_lock.lock().await;
        self.shutting_down.store(true, Ordering::SeqCst);
        self.send_all_miners(MinerControl::Shutdown).await;
        let registrations = std::mem::take(&mut *self.profile_miners.write().await);
        for (profile_id, registration) in registrations {
            Self::join_miner(&profile_id, registration.task).await;
        }
        self.flush_history().await;
    }

    pub async fn persist_settings(&self) -> Result<(), String> {
        let snap = self.settings.read().await.clone();
        save_settings(&snap)
    }

    pub async fn publish_snapshot(&self, app: &AppHandle, force: bool) {
        let snap = Arc::new(self.full_snapshot().await);
        self.events.emit_snapshot_throttled(app, snap, force).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state(path: &std::path::Path) -> Arc<AppState> {
        let mut settings = Settings::default();
        settings.mock_mode = false;
        Arc::new(AppState::new_with_history_path(
            settings,
            Arc::new(AllowlistedClient::new().unwrap()),
            LogBuffer::new(),
            path.to_path_buf(),
        ))
    }

    fn test_claim() -> RecentClaim {
        RecentClaim {
            id: "claim-1".to_string(),
            platform: PlatformId::Twitch,
            game_name: "Game".to_string(),
            campaign_name: "Campaign".to_string(),
            drop_name: "Drop".to_string(),
            reward_image_url: None,
            claimed_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn clear_history_waits_for_an_older_save() {
        let path = std::env::temp_dir().join(format!(
            "dropilot-clear-race-{}-{}.json",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let state = test_state(&path);
        state.record_claim(test_claim()).await;

        let (started_tx, started_rx) = oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let old_save = tokio::spawn({
            let state = state.clone();
            let path = path.clone();
            async move {
                let _save_guard = state.history_save_lock.lock().await;
                let hist = state.history.read().await.clone();
                tokio::task::spawn_blocking(move || {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    hist.save_to_path(&path).unwrap();
                })
                .await
                .unwrap();
            }
        });
        started_rx.await.unwrap();

        let clear = state.clear_history();
        tokio::pin!(clear);
        let completed_early = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            &mut clear,
        )
        .await
        .is_ok();
        release_tx.send(()).unwrap();
        assert!(!completed_early, "clear bypassed the in-flight save");
        clear.await;
        old_save.await.unwrap();

        assert!(HistoryStore::load_from_path(&path).claims.is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn shutdown_joins_miners_before_flushing_history() {
        let path = std::env::temp_dir().join(format!(
            "dropilot-shutdown-flush-{}-{}.json",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let state = test_state(&path);
        state.history.write().await.begin_watch_save(Instant::now());
        let (tx, mut rx) = mpsc::channel(1);
        let task = tauri::async_runtime::spawn({
            let state = state.clone();
            async move {
                if matches!(rx.recv().await, Some(MinerControl::Shutdown)) {
                    state.record_watch_minutes(1).await;
                }
            }
        });
        state
            .register_miner_for_profile("test-profile", tx, task)
            .await;

        state.shutdown().await;

        assert_eq!(HistoryStore::load_from_path(&path).total_watch_minutes, 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn concurrent_shutdowns_wait_for_the_same_miners() {
        let path = std::env::temp_dir().join(format!(
            "dropilot-concurrent-shutdown-{}-{}.json",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let state = test_state(&path);
        state.history.write().await.begin_watch_save(Instant::now());
        let (tx, mut rx) = mpsc::channel(1);
        let (started_tx, started_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let task = tauri::async_runtime::spawn({
            let state = state.clone();
            async move {
                if matches!(rx.recv().await, Some(MinerControl::Shutdown)) {
                    started_tx.send(()).unwrap();
                    release_rx.await.unwrap();
                    state.record_watch_minutes(1).await;
                }
            }
        });
        state
            .register_miner_for_profile("test-profile", tx, task)
            .await;

        let first = tauri::async_runtime::spawn({
            let state = state.clone();
            async move { state.shutdown().await }
        });
        started_rx.await.unwrap();
        let second = tauri::async_runtime::spawn({
            let state = state.clone();
            async move { state.shutdown().await }
        });
        tokio::pin!(second);
        let second_finished_early =
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut second)
                .await
                .is_ok();
        release_tx.send(()).unwrap();
        assert!(!second_finished_early, "second shutdown bypassed the miner");
        first.await.unwrap();
        second.await.unwrap();

        assert_eq!(HistoryStore::load_from_path(&path).total_watch_minutes, 1);
        let _ = std::fs::remove_file(path);
    }
}
