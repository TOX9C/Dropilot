<script lang="ts">
  import Bell from 'lucide-svelte/icons/bell';
  import Info from 'lucide-svelte/icons/info';
  import SettingsIcon from 'lucide-svelte/icons/settings';
  import Shield from 'lucide-svelte/icons/shield';
  import Terminal from 'lucide-svelte/icons/terminal';
  import X from 'lucide-svelte/icons/x';
  import Zap from 'lucide-svelte/icons/zap';
  import Button from '../components/Button.svelte';
  import Modal from '../components/Modal.svelte';
  import Toggle from '../components/Toggle.svelte';
  import { api } from '../lib/api';
  import { appSettings, notifyUi } from '../lib/stores';
  import type { AppSnapshot, Settings, ThemeMode, UpdateStatus } from '../lib/types';

  interface Props {
    settings: Settings;
    snapshot: AppSnapshot;
    mockAvailable: boolean;
  }

  let { settings, snapshot, mockAvailable }: Props = $props();

  let updateStatus = $state<UpdateStatus | null>(null);
  let checkingUpdate = $state(false);
  let clearModalOpen = $state(false);

  const navSections = [
    { id: 'sec-general', label: 'General' },
    { id: 'sec-farming', label: 'Farming' },
    { id: 'sec-notifications', label: 'Notifications' },
    { id: 'sec-privacy', label: 'Privacy & Data' },
    { id: 'sec-advanced', label: 'Advanced' },
    { id: 'sec-about', label: 'About' },
  ];

  const themeOptions: { id: ThemeMode; label: string; bg: string; card: string; fg: string }[] = [
    { id: 'dark', label: 'Dark', bg: '#0B0E1A', card: '#151A30', fg: '#EEF2FF' },
    { id: 'light', label: 'Light', bg: '#F1F4FC', card: '#FFFFFF', fg: '#111629' },
    { id: 'system', label: 'System', bg: 'linear-gradient(135deg,#0B0E1A 50%,#F1F4FC 50%)', card: '#818CF8', fg: '#EEF2FF' },
  ];

  async function savePatch(mutator: (draft: Settings) => void) {
    const draft: Settings = structuredClone(settings);
    mutator(draft);
    draft.refreshMinutes = Math.max(10, Math.min(240, Math.round(draft.refreshMinutes || 30)));
    try {
      const saved = await api.updateSettings(draft);
      appSettings.set(saved);
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleCheckUpdates() {
    checkingUpdate = true;
    try {
      updateStatus = await api.checkUpdate();
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      checkingUpdate = false;
    }
  }

  async function handleClearData() {
    try {
      await api.clearAppData();
      const fresh = await api.getSettings();
      appSettings.set(fresh);
      clearModalOpen = false;
      notifyUi('Cleared DropPilot accounts and local WebView2 data');
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  function jumpTo(id: string) {
    document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }
</script>

<div class="page">
  <header class="page-header">
    <div class="page-title-wrap">
      <div class="page-icon page-icon-warning"><SettingsIcon size={18} /></div>
      <div>
        <h1>Settings</h1>
        <p class="page-subtitle">Configure appearance, farming behavior, accounts, and privacy options</p>
      </div>
    </div>
  </header>

  <div class="settings-layout">
    <nav class="mini-nav card" aria-label="Settings sections">
      {#each navSections as sec (sec.id)}
        <button type="button" class="mini-link" onclick={() => jumpTo(sec.id)}>
          {sec.label}
        </button>
      {/each}
    </nav>

    <div class="sections-col">
      <section id="sec-general" class="card section">
        <div class="sec-title-row">
          <SettingsIcon size={16} />
          <h2>General</h2>
        </div>

        <div class="row row-col">
          <div class="row-text">
            <div class="row-title">Theme</div>
            <div class="row-desc">Matches the native Windows title bar automatically.</div>
          </div>
          <div class="swatch-grid" role="radiogroup" aria-label="Theme">
            {#each themeOptions as opt (opt.id)}
              <button
                type="button"
                role="radio"
                aria-checked={settings.theme === opt.id}
                class="swatch-btn"
                class:active={settings.theme === opt.id}
                onclick={() => savePatch((d) => (d.theme = opt.id))}
              >
                <span class="swatch-box" style="background:{opt.bg}">
                  <span class="swatch-bar" style="background:{opt.card}"></span>
                </span>
                <span>{opt.label}</span>
              </button>
            {/each}
          </div>
        </div>

        <div class="row">
          <div class="row-text">
            <div class="row-title">Start with Windows</div>
            <div class="row-desc">Uses standard HKCU Run key. Off by default.</div>
          </div>
          <Toggle
            checked={settings.autostart}
            ariaLabel="Start with Windows"
            onchange={(v) => savePatch((d) => (d.autostart = v))}
          />
        </div>

        <div class="row">
          <div class="row-text">
            <div class="row-title">Start minimized to tray</div>
            <div class="row-desc">Applies when launched automatically at Windows sign-in.</div>
          </div>
          <Toggle
            checked={settings.startMinimized}
            ariaLabel="Start minimized to tray"
            onchange={(v) => savePatch((d) => (d.startMinimized = v))}
          />
        </div>

        <div class="row">
          <div class="row-text">
            <div class="row-title">Close button minimizes to tray</div>
            <div class="row-desc">Destroys the UI WebView2 window while keeping Rust miners running lightly in tray.</div>
          </div>
          <Toggle
            checked={settings.closeToTray}
            ariaLabel="Close button minimizes to tray"
            onchange={(v) => savePatch((d) => (d.closeToTray = v))}
          />
        </div>
      </section>

      <section id="sec-farming" class="card section">
        <div class="sec-title-row">
          <Zap size={16} />
          <h2>Farming</h2>
        </div>

        <div class="row">
          <div class="row-text">
            <div class="row-title">Auto-claim completed drops</div>
            <div class="row-desc">Claims drops shortly after watch-time requirements are met.</div>
          </div>
          <Toggle
            checked={settings.autoClaim}
            ariaLabel="Auto-claim completed drops"
            onchange={(v) => savePatch((d) => (d.autoClaim = v))}
          />
        </div>

        <div class="row">
          <div class="row-text">
            <div class="row-title">When nothing in priority list is available</div>
            <div class="row-desc">Choose whether to wait idle or farm any eligible active campaign.</div>
          </div>
          <select
            aria-label="Idle fallback behavior"
            value={settings.idleBehavior}
            onchange={(e) => {
              const val = (e.currentTarget as HTMLSelectElement).value as Settings['idleBehavior'];
              savePatch((d) => (d.idleBehavior = val));
            }}
          >
            <option value="stay_idle">Stay idle (default)</option>
            <option value="farm_any">Farm any available campaign</option>
          </select>
        </div>

        <div class="row">
          <div class="row-text">
            <div class="row-title">Campaign refresh interval (minutes)</div>
            <div class="row-desc">Minimum 10 minutes to stay gentle on platform APIs.</div>
          </div>
          <input
            type="number"
            min={10}
            max={240}
            class="num-input tabular"
            aria-label="Campaign refresh interval in minutes"
            value={settings.refreshMinutes}
            onchange={(e) => {
              const val = Number((e.currentTarget as HTMLInputElement).value);
              savePatch((d) => (d.refreshMinutes = val));
            }}
          />
        </div>

        <div class="row">
          <div class="row-text">
            <div class="row-title">Farm even if game account is not linked</div>
            <div class="row-desc">Off by default so unlinked campaigns are skipped until linked.</div>
          </div>
          <Toggle
            checked={settings.farmUnlinked}
            ariaLabel="Farm even if game account is not linked"
            onchange={(v) => savePatch((d) => (d.farmUnlinked = v))}
          />
        </div>

        <div class="row">
          <div class="row-text">
            <div class="row-title">Compatibility watch mode</div>
            <div class="row-desc">Opens a hidden, muted, lowest-quality WebView2 stream window (uses more CPU/RAM).</div>
          </div>
          <Toggle
            checked={settings.compatWatchMode}
            ariaLabel="Compatibility watch mode"
            onchange={(v) => savePatch((d) => (d.compatWatchMode = v))}
          />
        </div>
      </section>

      <section id="sec-notifications" class="card section">
        <div class="sec-title-row">
          <Bell size={16} />
          <h2>Notifications</h2>
        </div>

        <div class="row">
          <div class="row-text">
            <div class="row-title">Desktop toast notifications</div>
            <div class="row-desc">Notify when drops are claimed, need manual action, or sessions expire.</div>
          </div>
          <Toggle
            checked={settings.notifications}
            ariaLabel="Desktop notifications"
            onchange={(v) => savePatch((d) => (d.notifications = v))}
          />
        </div>
      </section>

      <section id="sec-privacy" class="card section">
        <div class="sec-title-row">
          <Shield size={16} />
          <h2>Privacy &amp; Data</h2>
        </div>

        <div class="row">
          <div class="row-text">
            <div class="row-title">Twitch account</div>
            <div class="row-desc">{snapshot.twitch.account ? `Signed in as ${snapshot.twitch.account.displayName}` : 'Not signed in'}</div>
          </div>
          {#if snapshot.twitch.account}
            <Button variant="secondary" size="sm" onclick={() => api.logout('twitch')}>Log out</Button>
          {:else}
            <Button variant="primary" size="sm" onclick={() => api.login('twitch')}>Log in</Button>
          {/if}
        </div>

        <div class="row">
          <div class="row-text">
            <div class="row-title">Kick account</div>
            <div class="row-desc">{snapshot.kick.account ? `Signed in as ${snapshot.kick.account.displayName}` : 'Not signed in'}</div>
          </div>
          {#if snapshot.kick.account}
            <Button variant="secondary" size="sm" onclick={() => api.logout('kick')}>Log out</Button>
          {:else}
            <Button variant="primary" size="sm" onclick={() => api.login('kick')}>Log in</Button>
          {/if}
        </div>

        <div class="row">
          <div class="row-text">
            <div class="row-title">App folders</div>
            <div class="row-desc">Inspect local settings (`%APPDATA%\DropPilot`) or rolling logs (`%LOCALAPPDATA%\DropPilot\logs`).</div>
          </div>
          <div class="inline-actions">
            <Button variant="secondary" size="sm" onclick={() => api.openDataFolder().catch((e) => notifyUi(String(e), 'error'))}>
              Data folder ↗
            </Button>
            <Button variant="secondary" size="sm" onclick={() => api.openLogFolder().catch((e) => notifyUi(String(e), 'error'))}>
              Log folder ↗
            </Button>
          </div>
        </div>

        <div class="row">
          <div class="row-text">
            <div class="row-title">Check for updates on GitHub</div>
            <div class="row-desc">Off by default. Only checks the release API and opens the release page; never downloads binaries.</div>
          </div>
          <div class="inline-actions">
            {#if updateStatus?.available}
              <Button variant="primary" size="sm" onclick={() => api.openUrl(updateStatus!.releaseUrl)}>
                Open v{updateStatus.latestVersion} ↗
              </Button>
            {/if}
            <Button variant="secondary" size="sm" disabled={checkingUpdate} onclick={handleCheckUpdates}>
              {checkingUpdate ? 'Checking...' : 'Check now'}
            </Button>
            <Toggle
              checked={settings.updateCheck}
              ariaLabel="Check for updates on GitHub"
              onchange={(v) => savePatch((d) => (d.updateCheck = v))}
            />
          </div>
        </div>

        <div class="row">
          <div class="row-text">
            <div class="row-title">Clear app data</div>
            <div class="row-desc">Deletes Credential Manager tokens, DropPilot WebView2 cookies, and resets settings.</div>
          </div>
          <Button variant="danger" size="sm" onclick={() => (clearModalOpen = true)}>
            <X size={14} /> Clear app data
          </Button>
        </div>
      </section>

      <section id="sec-advanced" class="card section">
        <div class="sec-title-row">
          <Terminal size={16} />
          <h2>Advanced</h2>
        </div>

        <div class="row">
          <div class="row-text">
            <div class="row-title">Diagnostic log level</div>
            <div class="row-desc">Use Info normally; switch to Debug when troubleshooting platform calls.</div>
          </div>
          <select
            aria-label="Log level"
            value={settings.logLevel}
            onchange={(e) => {
              const val = (e.currentTarget as HTMLSelectElement).value as Settings['logLevel'];
              savePatch((d) => (d.logLevel = val));
            }}
          >
            <option value="info">Info</option>
            <option value="debug">Debug</option>
          </select>
        </div>

        {#if mockAvailable}
          <div class="row">
            <div class="row-text">
              <div class="row-title">Mock platform mode</div>
              <div class="row-desc">Simulates campaigns, live channels, progress, and claims offline with zero network calls.</div>
            </div>
            <Toggle
              checked={settings.mockMode}
              ariaLabel="Mock platform mode"
              onchange={(v) => savePatch((d) => (d.mockMode = v))}
            />
          </div>
        {/if}
      </section>

      <section id="sec-about" class="card section about">
        <div class="sec-title-row">
          <Info size={16} />
          <h2>About DropPilot</h2>
        </div>
        <p class="about-line">
          <strong>Version 0.1.0</strong> · Licensed under the MIT License · Runs strictly as standard user (`asInvoker`).
        </p>
        <p class="about-note">
          <strong>Disclaimer:</strong> Using automation tools may violate Twitch or Kick Terms of Service. You are solely responsible for your accounts. DropPilot is not affiliated with, sponsored by, or endorsed by Twitch Interactive, Inc. or Kick Streaming Pty Ltd.
        </p>
        <div class="about-links">
          <Button variant="ghost" size="sm" onclick={() => api.openUrl('https://github.com/droppilot/droppilot#readme')}>
            README ↗
          </Button>
          <Button variant="ghost" size="sm" onclick={() => api.openUrl('https://github.com/droppilot/droppilot/blob/main/docs/PRIVACY.md')}>
            PRIVACY ↗
          </Button>
          <Button variant="ghost" size="sm" onclick={() => api.openUrl('https://github.com/droppilot/droppilot/blob/main/SECURITY.md')}>
            SECURITY ↗
          </Button>
        </div>
      </section>
    </div>
  </div>
</div>

<Modal
  open={clearModalOpen}
  title="Clear all DropPilot data?"
  onclose={() => (clearModalOpen = false)}
>
  <p>
    This logs out both Twitch and Kick, deletes stored tokens from Windows Credential Manager, clears DropPilot's isolated WebView2 data, and resets your settings to defaults.
  </p>
  {#snippet footer()}
    <Button variant="secondary" size="sm" onclick={() => (clearModalOpen = false)}>Cancel</Button>
    <Button variant="danger" size="sm" onclick={handleClearData}>Clear app data</Button>
  {/snippet}
</Modal>

<style>
  .settings-layout {
    display: grid;
    grid-template-columns: 180px 1fr;
    gap: var(--space-4);
    align-items: start;
  }

  .mini-nav {
    position: sticky;
    top: 0;
    padding: var(--space-2);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .mini-link {
    border: none;
    background: transparent;
    color: var(--text-muted);
    text-align: left;
    padding: 7px 10px;
    border-radius: var(--radius-sm);
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .mini-link:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .sections-col {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    min-width: 0;
  }

  .section {
    display: flex;
    flex-direction: column;
    padding: 0;
    overflow: hidden;
  }

  .sec-title-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-4) var(--space-4) var(--space-3);
    color: var(--text);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--border);
  }

  .row-col {
    align-items: flex-start;
    flex-direction: column;
  }

  .row-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 220px;
  }

  .row-title {
    font-size: var(--text-base);
    font-weight: 600;
  }

  .row-desc {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .swatch-grid {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .swatch-btn {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: 6px 12px 6px 6px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--surface-2);
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .swatch-btn.active {
    border-color: var(--accent);
    background: var(--accent-tint);
    color: var(--accent-text);
  }

  .swatch-box {
    width: 24px;
    height: 18px;
    border-radius: 4px;
    border: 1px solid var(--border);
    padding: 3px;
    display: flex;
    align-items: flex-end;
  }

  .swatch-bar {
    width: 100%;
    height: 6px;
    border-radius: 2px;
  }

  .num-input {
    width: 88px;
    text-align: right;
  }

  .inline-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .about {
    padding: var(--space-4);
    gap: var(--space-2);
  }

  .about .sec-title-row {
    padding: 0 0 var(--space-1);
  }

  .about-line {
    font-size: var(--text-sm);
  }

  .about-note {
    font-size: var(--text-xs);
    color: var(--text-muted);
    line-height: 1.5;
  }

  .about-links {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-top: var(--space-1);
  }

  @media (max-width: 900px) {
    .settings-layout {
      grid-template-columns: 1fr;
    }
    .mini-nav {
      display: none;
    }
  }
</style>
