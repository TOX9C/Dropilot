use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use tauri::AppHandle;
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};

use crate::events::EventHub;
use crate::logging::{LogBuffer, LogHandle};
use crate::net::client::AllowlistedClient;
use crate::platforms::{AppSnapshot, Campaign, PlatformId, PlatformSnapshot, RecentClaim};
use crate::settings::{save_settings, Settings};

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
    pub twitch_snapshot: Arc<RwLock<PlatformSnapshot>>,
    pub kick_snapshot: Arc<RwLock<PlatformSnapshot>>,
    pub recent_claims: Arc<RwLock<VecDeque<RecentClaim>>>,
    pub miner_channels: RwLock<HashMap<PlatformId, mpsc::Sender<MinerControl>>>,
    pub kick_bridge_waiters: BridgeWaiters,
    pub twitch_bridge_lock: Arc<Mutex<()>>,
    pub allow_exit: AtomicBool,
    pub mock_active: AtomicBool,
}

impl AppState {
    pub fn new(
        initial_settings: Settings,
        http: Arc<AllowlistedClient>,
        logs: LogBuffer,
    ) -> Self {
        let mock_active = cfg!(feature = "mock") && initial_settings.mock_mode;
        Self {
            settings: Arc::new(RwLock::new(initial_settings)),
            http,
            logs,
            log_handle: StdMutex::new(None),
            events: EventHub::new(),
            twitch_snapshot: Arc::new(RwLock::new(PlatformSnapshot::logged_out(PlatformId::Twitch))),
            kick_snapshot: Arc::new(RwLock::new(PlatformSnapshot::logged_out(PlatformId::Kick))),
            recent_claims: Arc::new(RwLock::new(VecDeque::with_capacity(MAX_RECENT_CLAIMS))),
            miner_channels: RwLock::new(HashMap::new()),
            kick_bridge_waiters: Arc::new(StdMutex::new(HashMap::new())),
            twitch_bridge_lock: Arc::new(Mutex::new(())),
            allow_exit: AtomicBool::new(false),
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

    pub async fn full_snapshot(&self) -> AppSnapshot {
        let twitch = self.twitch_snapshot.read().await.clone();
        let kick = self.kick_snapshot.read().await.clone();
        let claims = self.recent_claims.read().await.iter().cloned().collect();
        AppSnapshot {
            twitch,
            kick,
            recent_claims: claims,
            mock_active: self.mock_active.load(Ordering::Relaxed),
        }
    }

    pub async fn record_claim(&self, claim: RecentClaim) {
        let mut guard = self.recent_claims.write().await;
        if guard.len() >= MAX_RECENT_CLAIMS {
            guard.pop_back();
        }
        guard.push_front(claim);
    }

    pub async fn register_miner(&self, platform: PlatformId, tx: mpsc::Sender<MinerControl>) {
        self.miner_channels.write().await.insert(platform, tx);
    }

    pub async fn send_miner(&self, platform: PlatformId, cmd: MinerControl) {
        let tx = self.miner_channels.read().await.get(&platform).cloned();
        if let Some(sender) = tx {
            let _ = sender.send(cmd).await;
        }
    }

    pub async fn send_all_miners(&self, cmd: MinerControl) {
        let senders: Vec<mpsc::Sender<MinerControl>> =
            self.miner_channels.read().await.values().cloned().collect();
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
