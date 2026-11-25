use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use tauri::AppHandle;
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tracing::warn;

use crate::events::EventHub;
use crate::history::{ClaimHistoryEntry, ClaimPlatform, ClaimRecordStatus, HistoryStore};
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
    pub twitch_snapshot: Arc<RwLock<PlatformSnapshot>>,
    pub kick_snapshot: Arc<RwLock<PlatformSnapshot>>,
    pub profile_snapshots: RwLock<HashMap<String, PlatformSnapshot>>,
    pub recent_claims: Arc<RwLock<VecDeque<RecentClaim>>>,
    pub profile_miners: RwLock<HashMap<String, mpsc::Sender<MinerControl>>>,
    pub kick_bridge_waiters: BridgeWaiters,
    pub twitch_bridge_lock: Arc<Mutex<()>>,
    pub allow_exit: AtomicBool,
    pub close_to_tray: AtomicBool,
    pub mock_active: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(
        initial_settings: Settings,
        http: Arc<AllowlistedClient>,
        logs: LogBuffer,
    ) -> Self {
        let mock_active = cfg!(feature = "mock") && initial_settings.mock_mode;
        let close_to_tray = initial_settings.close_to_tray;
        let history_store = HistoryStore::load();
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
            twitch_snapshot: Arc::new(RwLock::new(PlatformSnapshot::logged_out(PlatformId::Twitch))),
            kick_snapshot: Arc::new(RwLock::new(PlatformSnapshot::logged_out(PlatformId::Kick))),
            profile_snapshots: RwLock::new(initial_profile_snapshots),
            recent_claims: Arc::new(RwLock::new(initial_claims)),
            profile_miners: RwLock::new(HashMap::new()),
            kick_bridge_waiters: Arc::new(StdMutex::new(HashMap::new())),
            twitch_bridge_lock: Arc::new(Mutex::new(())),
            allow_exit: AtomicBool::new(false),
            close_to_tray: AtomicBool::new(close_to_tray),
            mock_active: Arc::new(AtomicBool::new(mock_active)),
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
                let snap = profile_map
                    .get(&prof.id)
                    .cloned()
                    .unwrap_or_else(|| PlatformSnapshot::logged_out(prof.platform));
                ProfileSnapshot {
                    profile_id: prof.id.clone(),
                    label: prof.label.clone(),
                    platform: prof.platform,
                    snapshot: snap,
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
            let mut hist = self.history.write().await;
            hist.record_claim(hist_entry);
            if let Err(err) = hist.save() {
                warn!(error = %err, "Failed persisting claim history");
            }
        }
    }

    pub async fn record_manual_claim(&self, entry: ClaimHistoryEntry) {
        if !self.mock_active.load(Ordering::Relaxed) {
            let mut hist = self.history.write().await;
            hist.record_claim(entry);
            if let Err(err) = hist.save() {
                warn!(error = %err, "Failed persisting manual claim record");
            }
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
        let mut hist = self.history.write().await;
        hist.record_claim(entry);
        if let Err(err) = hist.save() {
            warn!(error = %err, "Failed persisting Epic claim history");
        }
    }

    pub async fn remove_epic_claim(&self, game_id: &str) {
        let entry_id = format!("epic-{game_id}");
        let mut hist = self.history.write().await;
        hist.remove_claim(&entry_id);
        if let Err(err) = hist.save() {
            warn!(error = %err, "Failed updating Epic claim history");
        }
    }

    pub async fn record_watch_minutes(&self, delta_minutes: u32) {
        if delta_minutes == 0 || self.mock_active.load(Ordering::Relaxed) {
            return;
        }
        let mut hist = self.history.write().await;
        hist.record_watch_minutes(delta_minutes);
        if let Err(err) = hist.save() {
            warn!(error = %err, "Failed persisting watch minutes");
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

    pub async fn register_miner_for_profile(
        &self,
        profile_id: &str,
        tx: mpsc::Sender<MinerControl>,
    ) {
        self.profile_miners
            .write()
            .await
            .insert(profile_id.to_string(), tx);
    }

    pub async fn unregister_miner_for_profile(&self, profile_id: &str) {
        self.profile_miners.write().await.remove(profile_id);
        self.profile_snapshots.write().await.remove(profile_id);
    }

    pub async fn send_profile_miner(&self, profile_id: &str, cmd: MinerControl) {
        let tx = self.profile_miners.read().await.get(profile_id).cloned();
        if let Some(sender) = tx {
            let _ = sender.send(cmd).await;
        }
    }

    pub async fn register_miner(&self, platform: PlatformId, tx: mpsc::Sender<MinerControl>) {
        let active_id = self.settings.read().await.active_profile_id(platform).to_string();
        self.register_miner_for_profile(&active_id, tx).await;
    }

    pub async fn send_miner(&self, platform: PlatformId, cmd: MinerControl) {
        let active_id = self.settings.read().await.active_profile_id(platform).to_string();
        self.send_profile_miner(&active_id, cmd).await;
    }

    pub async fn send_all_miners(&self, cmd: MinerControl) {
        let senders: Vec<mpsc::Sender<MinerControl>> =
            self.profile_miners.read().await.values().cloned().collect();
        for sender in senders {
            let _ = sender.send(cmd.clone()).await;
        }
    }

    pub async fn persist_settings(&self) -> Result<(), String> {
        let snap = self.settings.read().await.clone();
        save_settings(&snap)
    }

    pub async fn publish_snapshot(&self, app: &AppHandle, force: bool) {
        let snap = self.full_snapshot().await;
        self.events.emit_snapshot_throttled(app, &snap, force).await;
    }
}
