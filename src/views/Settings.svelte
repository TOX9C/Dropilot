<script lang="ts">
  import { onMount } from 'svelte';
  import Bell from 'lucide-svelte/icons/bell';
  import Check from 'lucide-svelte/icons/check';
  import Cloud from 'lucide-svelte/icons/cloud';
  import ExternalLink from 'lucide-svelte/icons/external-link';
  import FolderOpen from 'lucide-svelte/icons/folder-open';
  import Info from 'lucide-svelte/icons/info';
  import Link2 from 'lucide-svelte/icons/link-2';
  import LogIn from 'lucide-svelte/icons/log-in';
  import LogOut from 'lucide-svelte/icons/log-out';
  import Monitor from 'lucide-svelte/icons/monitor';
  import Moon from 'lucide-svelte/icons/moon';
  import Palette from 'lucide-svelte/icons/palette';
  import RefreshCw from 'lucide-svelte/icons/refresh-cw';
  import Send from 'lucide-svelte/icons/send';
  import Shield from 'lucide-svelte/icons/shield';
  import Sliders from 'lucide-svelte/icons/sliders';
  import Trash2 from 'lucide-svelte/icons/trash-2';
  import TypeIcon from 'lucide-svelte/icons/type';
  import Zap from 'lucide-svelte/icons/zap';
  import Badge from '../components/Badge.svelte';
  import Button from '../components/Button.svelte';
  import CustomSelect, { type SelectOption } from '../components/CustomSelect.svelte';
  import Modal from '../components/Modal.svelte';
  import Monogram from '../components/Monogram.svelte';
  import ProgressBar from '../components/ProgressBar.svelte';
  import StatusPill from '../components/StatusPill.svelte';
  import Toggle from '../components/Toggle.svelte';
  import { api } from '../lib/api';
  import { formatExactDateTime } from '../lib/format';
  import {
    appSettings,
    checkForAppUpdate,
    checkingForUpdate,
    installAppUpdate,
    installingUpdate,
    notifyUi,
    remoteConfigStatus,
    updateInfo,
  } from '../lib/stores';
  import type {
    AppSnapshot,
    FontFamily,
    FontScale,
    Settings,
    ThemeMode,
    WebhookSecretsStatus,
  } from '../lib/types';

  interface Props {
    settings: Settings;
    snapshot: AppSnapshot;
    mockAvailable: boolean;
  }

  let { settings, snapshot, mockAvailable }: Props = $props();

  type SettingsTab = 'appearance' | 'farming' | 'system' | 'data';

  let activeTab = $state<SettingsTab>('appearance');
  let syncingRemote = $state(false);
  let clearModalOpen = $state(false);

  let webhookSecrets = $state<WebhookSecretsStatus>({
    hasDiscordUrl: false,
    hasTelegramToken: false,
  });
  let discordUrlInput = $state('');
  let telegramTokenInput = $state('');
  let telegramChatIdInput = $state('');
  let savingWebhook = $state(false);
  let testingChannel = $state<'discord' | 'telegram' | null>(null);
  let webhookFeedback = $state<{ channel: 'discord' | 'telegram'; ok: boolean; text: string } | null>(null);

  const timeOptions: SelectOption[] = Array.from({ length: 48 }, (_, idx) => {
    const mins = idx * 30;
    const h = String(Math.floor(mins / 60)).padStart(2, '0');
    const m = String(mins % 60).padStart(2, '0');
    return {
      value: String(mins),
      label: `${h}:${m}`,
    };
  });

  onMount(() => {
    telegramChatIdInput = settings.webhooks?.telegramChatId ?? '';
    void api
      .getWebhookSecretsStatus()
      .then((st) => {
        webhookSecrets = st;
      })
      .catch(() => {});
  });

  $effect(() => {
    const cur = settings.webhooks?.telegramChatId ?? '';
    if (cur && !telegramChatIdInput) {
      telegramChatIdInput = cur;
    }
  });

  const tabs: { id: SettingsTab; label: string; desc: string }[] = [
    { id: 'appearance', label: 'Appearance', desc: 'Color mode & typography' },
    { id: 'farming', label: 'Farming', desc: 'Drop rules & sync' },
    { id: 'system', label: 'System', desc: 'Startup, tray & webhooks' },
    { id: 'data', label: 'Data & About', desc: 'Folders, updates & reset' },
  ];

  const themeOptions: {
    id: ThemeMode;
    label: string;
    hint: string;
  }[] = [
    { id: 'dark', label: 'Dark', hint: 'Low-glare terminal surfaces' },
    { id: 'light', label: 'Light', hint: 'Crisp high-contrast light surfaces' },
    { id: 'system', label: 'Match Windows', hint: 'Follows OS dark/light mode' },
  ];

  const fontOptions: {
    id: FontFamily;
    label: string;
    hint: string;
    sampleSans: string;
    sampleMono: string;
  }[] = [
    {
      id: 'plex',
      label: 'IBM Plex',
      hint: 'Default Console pairing (Sans + Mono)',
      sampleSans: '"IBM Plex Sans", "Segoe UI", sans-serif',
      sampleMono: '"IBM Plex Mono", Consolas, monospace',
    },
    {
      id: 'inter',
      label: 'Inter + JetBrains',
      hint: 'Clean geometric UI + sharp tabular mono',
      sampleSans: '"Inter", "Segoe UI", sans-serif',
      sampleMono: '"JetBrains Mono", Consolas, monospace',
    },
    {
      id: 'system',
      label: 'Windows System',
      hint: 'Native Segoe UI Variable + Cascadia Mono',
      sampleSans: '"Segoe UI Variable", "Segoe UI", system-ui, sans-serif',
      sampleMono: '"Cascadia Mono", Consolas, monospace',
    },
    {
      id: 'mono',
      label: 'JetBrains Mono',
      hint: 'Full terminal monospace typography',
      sampleSans: '"JetBrains Mono", Consolas, monospace',
      sampleMono: '"JetBrains Mono", Consolas, monospace',
    },
  ];

  const fontScaleOptions: { id: FontScale; label: string; pct: string }[] = [
    { id: 'compact', label: 'Compact', pct: '95%' },
    { id: 'default', label: 'Default', pct: '100%' },
    { id: 'comfortable', label: 'Comfortable', pct: '108%' },
    { id: 'large', label: 'Large', pct: '115%' },
  ];

  const activeFontLabel = $derived(
    fontOptions.find((f) => f.id === (settings.fontFamily ?? 'plex'))?.label ?? 'IBM Plex',
  );
  const activeScaleLabel = $derived(
    fontScaleOptions.find((s) => s.id === (settings.fontScale ?? 'default'))?.pct ?? '100%',
  );

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

  function stepRefresh(delta: number) {
    const next = Math.max(10, Math.min(240, (settings.refreshMinutes || 30) + delta));
    void savePatch((d) => (d.refreshMinutes = next));
  }

  function handleCheckUpdates() {
    void checkForAppUpdate(false);
  }

  async function handleClearData() {
    try {
      await api.clearAppData();
      const fresh = await api.getSettings();
      appSettings.set(fresh);
      webhookSecrets = { hasDiscordUrl: false, hasTelegramToken: false };
      discordUrlInput = '';
      telegramTokenInput = '';
      telegramChatIdInput = '';
      clearModalOpen = false;
      notifyUi('Cleared Dropilot accounts and local WebView2 data');
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleRefreshRemoteConfig() {
    syncingRemote = true;
    try {
      const next = await api.refreshRemoteConfig();
      remoteConfigStatus.set(next);
      if (next.lastError) {
        notifyUi(`Remote config sync issue: ${next.lastError}`, 'error');
      } else {
        notifyUi(`Remote config synced (${next.overriddenHashes} active GQL hashes)`);
      }
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      syncingRemote = false;
    }
  }

  async function handleSaveDiscordUrl(clear = false) {
    savingWebhook = true;
    try {
      const val = clear ? '' : discordUrlInput.trim();
      webhookSecrets = await api.saveWebhookSecrets(val, null);
      discordUrlInput = '';
      if (clear) {
        await savePatch((d) => (d.webhooks.discordEnabled = false));
        notifyUi('Discord webhook removed from Credential Manager');
      } else {
        await savePatch((d) => (d.webhooks.discordEnabled = true));
        notifyUi('Discord webhook saved securely in Credential Manager');
      }
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      savingWebhook = false;
    }
  }

  async function handleSaveTelegram(clear = false) {
    savingWebhook = true;
    try {
      if (clear) {
        webhookSecrets = await api.saveWebhookSecrets(null, '');
        telegramTokenInput = '';
        telegramChatIdInput = '';
        await savePatch((d) => {
          d.webhooks.telegramEnabled = false;
          d.webhooks.telegramChatId = '';
        });
        notifyUi('Telegram credentials removed from Credential Manager');
      } else {
        if (telegramTokenInput.trim()) {
          webhookSecrets = await api.saveWebhookSecrets(null, telegramTokenInput.trim());
          telegramTokenInput = '';
        }
        await savePatch((d) => {
          d.webhooks.telegramChatId = telegramChatIdInput.trim();
          if (webhookSecrets.hasTelegramToken && telegramChatIdInput.trim()) {
            d.webhooks.telegramEnabled = true;
          }
        });
        notifyUi('Telegram alert settings saved');
      }
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      savingWebhook = false;
    }
  }

  async function handleTestWebhook(channel: 'discord' | 'telegram') {
    testingChannel = channel;
    webhookFeedback = null;
    try {
      if (channel === 'discord' && discordUrlInput.trim()) {
        webhookSecrets = await api.saveWebhookSecrets(discordUrlInput.trim(), null);
        discordUrlInput = '';
        await savePatch((d) => (d.webhooks.discordEnabled = true));
      }
      const resultMsg = await api.testWebhook(
        channel,
        channel === 'telegram' ? telegramChatIdInput.trim() : undefined,
      );
      const okMsg =
        resultMsg || `Test ${channel === 'discord' ? 'Discord' : 'Telegram'} notification sent`;
      webhookFeedback = { channel, ok: true, text: okMsg };
      notifyUi(okMsg);
    } catch (err) {
      const errMsg = String(err);
      webhookFeedback = { channel, ok: false, text: errMsg };
      notifyUi(errMsg, 'error');
    } finally {
      testingChannel = null;
    }
  }

  function snapToHalfHour(mins: number): string {
    const snapped = Math.floor((mins % 1440) / 30) * 30;
    return String(snapped);
  }
</script>

<div class="page settings-page">
  <header class="page-header">
    <div class="page-title-wrap">
      <h1>Settings</h1>
    </div>
    <div class="page-actions">
      <Badge tone="neutral">v0.1.0</Badge>
    </div>
  </header>

  <!-- Connected Accounts Top Strip -->
  <section class="accounts-wrap" aria-label="Connected accounts">
    <div class="accounts-grid">
      {#each [snapshot.twitch, snapshot.kick] as platSnap (platSnap.platform)}
        {@const isTwitch = platSnap.platform === 'twitch'}
        {@const label = isTwitch ? 'Twitch' : 'Kick'}
        {@const browserUrl = isTwitch
          ? 'https://www.twitch.tv/drops/campaigns'
          : 'https://kick.com/drops/campaigns'}
        <div class="card acct-card">
          <div class="acct-left">
            <Monogram
              src={platSnap.account?.avatarUrl}
              name={platSnap.account?.displayName ?? label}
              width={34}
              height={34}
            />
            <div class="acct-meta">
              <div class="acct-top">
                <span class="plat-dot dot-{platSnap.platform}" aria-hidden="true"></span>
                <strong>{label}</strong>
                <StatusPill state={platSnap.state} compact />
              </div>
              <span class="acct-sub">
                {#if platSnap.account}
                  Signed in as <strong>@{platSnap.account.displayName}</strong>
                {:else}
                  Not connected — sign in via Dropilot's isolated window
                {/if}
              </span>
            </div>
          </div>
          <div class="acct-action">
            <Button
              variant="ghost"
              size="sm"
              title={`Open ${label} Drops in your default browser`}
              onclick={() => api.openUrl(browserUrl).catch((e) => notifyUi(String(e), 'error'))}
            >
              <ExternalLink size={13} />
              <span>Browser</span>
            </Button>
            {#if platSnap.account}
              <Button variant="secondary" size="sm" onclick={() => api.logout(platSnap.platform)}>
                <LogOut size={13} />
                <span>Log out</span>
              </Button>
            {:else}
              <Button
                variant="primary"
                size="sm"
                title={`Opens Dropilot's isolated ${label} sign-in window (required for Drops sync; saved once in Windows Credential Manager)`}
                onclick={() => api.login(platSnap.platform)}
              >
                <LogIn size={13} />
                <span>Log in</span>
              </Button>
            {/if}
          </div>
        </div>
      {/each}
    </div>

    <div class="session-info-bar">
      <Info size={13} />
      <span>
        <strong>Isolated session sign-in:</strong> Dropilot opens a dedicated sign-in window instead of reading your Chrome, Edge, or Firefox profiles. This isolated session is required for Twitch &amp; Kick Drops sync and integrity checks, and your token is saved securely in Windows Credential Manager so you only sign in once.
      </span>
    </div>
  </section>

  <!-- Segmented Tab Bar -->
  <div class="seg-tabs" role="tablist" aria-label="Settings categories">
    {#each tabs as tab (tab.id)}
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === tab.id}
        class="seg-tab"
        class:active={activeTab === tab.id}
        onclick={() => (activeTab = tab.id)}
      >
        <span class="seg-ico" aria-hidden="true">
          {#if tab.id === 'appearance'}
            <Palette size={14} />
          {:else if tab.id === 'farming'}
            <Zap size={14} />
          {:else if tab.id === 'system'}
            <Sliders size={14} />
          {:else}
            <Shield size={14} />
          {/if}
        </span>
        <span class="seg-text">
          <span class="seg-label">{tab.label}</span>
        </span>
      </button>
    {/each}
  </div>

  <!-- Tab Panel: Appearance -->
  {#if activeTab === 'appearance'}
    <div class="appearance-split">
      <div class="panel-stack">
        <section class="card studio-card">
          <div class="studio-head">
            <div>
              <h2>Color Mode</h2>
              <p class="studio-sub">Applies dark or light surfaces and matches the native Windows title bar.</p>
            </div>
          </div>

          <div class="mode-grid" role="radiogroup" aria-label="Color mode">
            {#each themeOptions as opt (opt.id)}
              {@const active = settings.theme === opt.id}
              <button
                type="button"
                role="radio"
                aria-checked={active}
                class="mode-btn"
                class:active
                onclick={() => savePatch((d) => (d.theme = opt.id))}
              >
                <div class="mode-text">
                  <strong>{opt.label}</strong>
                  <span>{opt.hint}</span>
                </div>
                {#if active}
                  <span class="mode-check" aria-hidden="true"><Check size={14} /></span>
                {/if}
              </button>
            {/each}
          </div>
        </section>

        <section class="card studio-card">
          <div class="studio-head">
            <div>
              <div class="title-with-ico">
                <TypeIcon size={15} />
                <h2>Typography &amp; Font</h2>
              </div>
              <p class="studio-sub">Zero-CDN locally bundled &amp; native Windows font pairings saved across restarts.</p>
            </div>
          </div>

          <div class="font-grid" role="radiogroup" aria-label="Font family preset">
            {#each fontOptions as fopt (fopt.id)}
              {@const active = (settings.fontFamily ?? 'plex') === fopt.id}
              <button
                type="button"
                role="radio"
                aria-checked={active}
                class="font-btn"
                class:active
                onclick={() => savePatch((d) => (d.fontFamily = fopt.id))}
              >
                <div class="font-btn-top">
                  <strong>{fopt.label}</strong>
                  {#if active}
                    <span class="mode-check" aria-hidden="true"><Check size={14} /></span>
                  {/if}
                </div>
                <span class="font-hint">{fopt.hint}</span>
                <div class="font-sample-row">
                  <span class="sample-sans" style:font-family={fopt.sampleSans}>Rust Drops</span>
                  <span class="sample-mono" style:font-family={fopt.sampleMono}>01:45 · 63%</span>
                </div>
              </button>
            {/each}
          </div>

          <div class="scale-row">
            <div class="scale-meta">
              <strong>Text size</strong>
              <span>Scales UI text, timers, and card metadata cleanly across resolutions.</span>
            </div>
            <div class="scale-pills" role="radiogroup" aria-label="Text size scale">
              {#each fontScaleOptions as sopt (sopt.id)}
                {@const active = (settings.fontScale ?? 'default') === sopt.id}
                <button
                  type="button"
                  role="radio"
                  aria-checked={active}
                  class="scale-pill"
                  class:active
                  onclick={() => savePatch((d) => (d.fontScale = sopt.id))}
                >
                  <span>{sopt.label}</span>
                  <span class="scale-pct tabular">{sopt.pct}</span>
                </button>
              {/each}
            </div>
          </div>
        </section>
      </div>

      <section class="card studio-card preview-card" aria-label="Typography live preview">
        <div class="studio-head">
          <div>
            <h2>Live Preview</h2>
            <p class="studio-sub">Reflects your selected font family and text scale immediately.</p>
          </div>
          <Badge tone="accent">{activeFontLabel} · {activeScaleLabel}</Badge>
        </div>

        <div class="preview-surface">
          <div class="pv-top">
            <div class="pv-plat">
              <span class="plat-dot dot-twitch" aria-hidden="true"></span>
              <span class="pv-plat-name tabular">TWITCH</span>
              <span class="pv-acct tabular">@demo_pilot</span>
            </div>
            <span class="pv-status tabular">MINING</span>
          </div>

          <div class="pv-camp">
            <div class="pv-camp-head">
              <strong>Rust — Twitch Rivals Drops</strong>
              <span class="pv-link">
                <Link2 size={11} />
                <span>Linked</span>
              </span>
            </div>
            <span class="pv-reward">Reward: Cobalt Garage Door</span>
          </div>

          <div class="pv-prog">
            <div class="pv-prog-row tabular">
              <span>19m / 30m</span>
              <strong>63% · 11m 00s left</strong>
            </div>
            <ProgressBar value={19} max={30} platform="twitch" tall label="Preview progress" />
          </div>

          <div class="pv-log mono">
            <span class="pv-log-ts">[10:15:42]</span>
            <span class="pv-log-lvl">INFO</span>
            <span>Credited +1m watch time for Rust</span>
          </div>
        </div>
      </section>
    </div>
  {:else if activeTab === 'farming'}
    <div class="pref-grid">
      <div class="card pref-card">
        <div class="pref-top">
          <div class="pref-ico"><Zap size={15} /></div>
          <Toggle
            checked={settings.autoClaim}
            ariaLabel="Auto-claim completed drops"
            onchange={(v) => savePatch((d) => (d.autoClaim = v))}
          />
        </div>
        <h3>Auto-claim completed drops</h3>
        <p>Automatically claims finished Twitch &amp; Kick rewards as soon as watch time reaches 100%.</p>
      </div>

      <div class="card pref-card">
        <div class="pref-top">
          <div class="pref-ico"><Shield size={15} /></div>
          <Toggle
            checked={settings.farmUnlinked}
            ariaLabel="Farm even if game account is not linked"
            onchange={(v) => savePatch((d) => (d.farmUnlinked = v))}
          />
        </div>
        <h3>Farm unlinked campaigns</h3>
        <p>Keep off to skip games whose publisher account isn't linked yet during automatic idle fallback.</p>
      </div>

      <div class="card pref-card">
        <div class="pref-top">
          <div class="pref-ico"><Monitor size={15} /></div>
          <Toggle
            checked={settings.compatWatchMode}
            ariaLabel="Compatibility watch mode"
            onchange={(v) => savePatch((d) => (d.compatWatchMode = v))}
          />
        </div>
        <h3>Browser player fallback</h3>
        <p>Ultra-light stream-less mode is active by default (0 Mbps video, ~12 MB RAM in tray). Turn on only if a platform stops crediting drops.</p>
      </div>

      <div class="card pref-card">
        <div class="pref-top">
          <div class="pref-ico"><RefreshCw size={15} /></div>
          <div class="stepper tabular">
            <button type="button" class="step-btn" onclick={() => stepRefresh(-5)} aria-label="Decrease refresh minutes">−</button>
            <span class="step-val">{settings.refreshMinutes}m</span>
            <button type="button" class="step-btn" onclick={() => stepRefresh(5)} aria-label="Increase refresh minutes">+</button>
          </div>
        </div>
        <h3>Campaign refresh cadence</h3>
        <p>How often Dropilot reloads the full drops catalog (10–240 min). Progress itself syncs every 60s.</p>
      </div>

      <div class="card pref-card pref-wide">
        <div class="pref-wide-row">
          <div class="pref-wide-text">
            <h3>When nothing in priority list is live</h3>
            <p>Choose whether to stay idle or automatically farm any eligible active campaign while waiting.</p>
          </div>
          <CustomSelect
            options={[
              {
                value: 'stay_idle',
                label: 'Stay idle (default)',
                subtitle: 'Wait quietly until a priority drop goes live',
                dotTone: 'muted',
              },
              {
                value: 'farm_any',
                label: 'Farm any available campaign',
                subtitle: 'Auto-mine other active drops while waiting',
                dotTone: 'live',
              },
            ] satisfies SelectOption[]}
            value={settings.idleBehavior}
            onchange={(val) =>
              savePatch((d) => (d.idleBehavior = val as Settings['idleBehavior']))}
            ariaLabel="Idle fallback behavior"
            size="md"
            minMenuWidth={270}
            align="right"
          />
        </div>
      </div>

      <div class="card pref-card pref-wide">
        <div class="pref-wide-row">
          <div class="pref-wide-text">
            <div class="title-with-badge">
              <span class="pref-ico"><Moon size={15} /></span>
              <div>
                <h3>Quiet hours schedule</h3>
                <p>Automatically pause drop farming during local hours and resume when the window ends.</p>
              </div>
            </div>
          </div>
          <Toggle
            checked={settings.schedule.quietHoursEnabled}
            ariaLabel="Enable quiet hours schedule"
            onchange={(v) => savePatch((d) => (d.schedule.quietHoursEnabled = v))}
          />
        </div>

        {#if settings.schedule.quietHoursEnabled}
          <div class="schedule-row">
            <div class="sched-field">
              <span class="sched-label">Pause at</span>
              <CustomSelect
                options={timeOptions}
                value={snapToHalfHour(settings.schedule.quietStartMinutes)}
                onchange={(val) =>
                  savePatch((d) => (d.schedule.quietStartMinutes = Number(val)))}
                ariaLabel="Quiet hours start time"
                size="sm"
              />
            </div>
            <div class="sched-field">
              <span class="sched-label">Resume at</span>
              <CustomSelect
                options={timeOptions}
                value={snapToHalfHour(settings.schedule.quietEndMinutes)}
                onchange={(val) =>
                  savePatch((d) => (d.schedule.quietEndMinutes = Number(val)))}
                ariaLabel="Quiet hours end time"
                size="sm"
              />
            </div>
            {#if settings.schedule.quietStartMinutes > settings.schedule.quietEndMinutes}
              <Badge tone="accent">Overnight window</Badge>
            {:else if settings.schedule.quietStartMinutes === settings.schedule.quietEndMinutes}
              <Badge tone="warning">Start &amp; end match (inactive)</Badge>
            {/if}
          </div>
        {/if}

        <div class="sub-divider"></div>

        <div class="pref-wide-row">
          <div class="pref-wide-text">
            <h3>Humanized polling jitter (±25%)</h3>
            <p>Randomizes watch ticks and background sync intervals instead of rigid 60-second pulses.</p>
          </div>
          <Toggle
            checked={settings.schedule.humanizedJitter}
            ariaLabel="Humanized polling jitter"
            onchange={(v) => savePatch((d) => (d.schedule.humanizedJitter = v))}
          />
        </div>
      </div>
    </div>
  {:else if activeTab === 'system'}
    <div class="pref-grid">
      <div class="card pref-card">
        <div class="pref-top">
          <div class="pref-ico"><Monitor size={15} /></div>
          <Toggle
            checked={settings.autostart}
            ariaLabel="Start with Windows"
            onchange={(v) => savePatch((d) => (d.autostart = v))}
          />
        </div>
        <h3>Start with Windows</h3>
        <p>Launches Dropilot automatically at Windows sign-in via standard HKCU Run entry.</p>
      </div>

      <div class="card pref-card">
        <div class="pref-top">
          <div class="pref-ico"><Sliders size={15} /></div>
          <Toggle
            checked={settings.startMinimized}
            ariaLabel="Start minimized to tray"
            onchange={(v) => savePatch((d) => (d.startMinimized = v))}
          />
        </div>
        <h3>Start minimized to tray</h3>
        <p>Starts quietly in the Windows notification tray when launched automatically on boot.</p>
      </div>

      <div class="card pref-card">
        <div class="pref-top">
          <div class="pref-ico"><Sliders size={15} /></div>
          <Toggle
            checked={settings.closeToTray}
            ariaLabel="Close button minimizes to tray"
            onchange={(v) => savePatch((d) => (d.closeToTray = v))}
          />
        </div>
        <h3>Close to system tray</h3>
        <p>Closes the UI window while keeping lightweight Rust drop miners running in the tray.</p>
      </div>

      <div class="card pref-card">
        <div class="pref-top">
          <div class="pref-ico"><Bell size={15} /></div>
          <Toggle
            checked={settings.notifications}
            ariaLabel="Desktop notifications"
            onchange={(v) => savePatch((d) => (d.notifications = v))}
          />
        </div>
        <h3>Desktop toast notifications</h3>
        <p>Notifies you when a reward is claimed, needs manual action, or a session expires.</p>
      </div>

      <div class="card pref-card pref-wide">
        <div class="pref-wide-row">
          <div class="pref-wide-text">
            <h3>Diagnostic log verbosity</h3>
            <p>Keep on Info for normal use; switch to Debug when troubleshooting platform responses.</p>
          </div>
          <CustomSelect
            options={[
              {
                value: 'info',
                label: 'Info (Recommended)',
                subtitle: 'Clean activity and claim summary',
                dotTone: 'info',
              },
              {
                value: 'debug',
                label: 'Debug (Verbose)',
                subtitle: 'Detailed diagnostic trace for troubleshooting',
                dotTone: 'warn',
              },
            ] satisfies SelectOption[]}
            value={settings.logLevel}
            onchange={(val) => savePatch((d) => (d.logLevel = val as Settings['logLevel']))}
            ariaLabel="Log level"
            size="md"
            minMenuWidth={250}
            align="right"
          />
        </div>
      </div>

      <div class="card pref-card pref-wide webhook-card">
        <div class="studio-head">
          <div>
            <h3>Discord &amp; Telegram Webhook Alerts</h3>
            <p class="studio-sub">Get pinged when drops are claimed or require verification. Secrets stay encrypted in Windows Credential Manager.</p>
          </div>
          <Badge tone="neutral">Encrypted</Badge>
        </div>

        <div class="webhook-grid">
          <div class="webhook-box">
            <div class="webhook-box-head">
              <div class="webhook-title-row">
                <strong>Discord Webhook</strong>
                {#if webhookSecrets.hasDiscordUrl}
                  <Badge tone="success">Saved</Badge>
                {:else}
                  <Badge tone="neutral">Not set</Badge>
                {/if}
              </div>
              <Toggle
                checked={settings.webhooks.discordEnabled}
                ariaLabel="Enable Discord webhook alerts"
                onchange={(v) => savePatch((d) => (d.webhooks.discordEnabled = v))}
              />
            </div>
            <div class="webhook-input-row">
              <input
                type="password"
                class="secret-input"
                bind:value={discordUrlInput}
                placeholder={webhookSecrets.hasDiscordUrl
                  ? 'Webhook URL stored securely (paste new URL to replace)'
                  : 'https://discord.com/api/webhooks/...'}
                aria-label="Discord webhook URL"
              />
              <Button
                variant="primary"
                size="sm"
                disabled={savingWebhook || !discordUrlInput.trim()}
                onclick={() => handleSaveDiscordUrl(false)}
              >
                Save
              </Button>
              {#if webhookSecrets.hasDiscordUrl || discordUrlInput.trim()}
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={testingChannel === 'discord' || savingWebhook}
                  onclick={() => handleTestWebhook('discord')}
                >
                  <Send size={12} />
                  <span>{testingChannel === 'discord' ? 'Sending...' : 'Test'}</span>
                </Button>
              {/if}
              {#if webhookSecrets.hasDiscordUrl}
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={savingWebhook}
                  onclick={() => handleSaveDiscordUrl(true)}
                >
                  Remove
                </Button>
              {/if}
            </div>
            {#if webhookFeedback?.channel === 'discord'}
              {@const urlMatch = webhookFeedback.text.match(/https:\/\/discord\.com\/channels\/\d+\/\d+\/\d+/)}
              <div class="webhook-feedback-row">
                <p class="webhook-feedback" class:fb-ok={webhookFeedback.ok} class:fb-err={!webhookFeedback.ok}>
                  {urlMatch ? webhookFeedback.text.replace(/\s*·\s*https:\/\/discord\.com\/channels\/\d+\/\d+\/\d+/, '') : webhookFeedback.text}
                </p>
                {#if urlMatch}
                  <button
                    type="button"
                    class="fb-open-link"
                    onclick={() => api.openUrl(urlMatch[0]).catch((e) => notifyUi(String(e), 'error'))}
                  >
                    Open in Discord
                  </button>
                {/if}
              </div>
            {/if}
          </div>

          <div class="webhook-box">
            <div class="webhook-box-head">
              <div class="webhook-title-row">
                <strong>Telegram Bot</strong>
                {#if webhookSecrets.hasTelegramToken}
                  <Badge tone="success">Token saved</Badge>
                {:else}
                  <Badge tone="neutral">Not set</Badge>
                {/if}
              </div>
              <Toggle
                checked={settings.webhooks.telegramEnabled}
                ariaLabel="Enable Telegram bot alerts"
                onchange={(v) => savePatch((d) => (d.webhooks.telegramEnabled = v))}
              />
            </div>
            <div class="webhook-input-row">
              <input
                type="password"
                class="secret-input"
                bind:value={telegramTokenInput}
                placeholder={webhookSecrets.hasTelegramToken
                  ? 'Bot token stored securely (paste to replace)'
                  : 'Bot token from @BotFather (123456:ABC...)'}
                aria-label="Telegram bot token"
              />
              <input
                type="text"
                class="secret-input chat-id-input"
                bind:value={telegramChatIdInput}
                placeholder="Chat ID"
                aria-label="Telegram chat ID"
              />
              <Button
                variant="primary"
                size="sm"
                disabled={savingWebhook || (!telegramTokenInput.trim() && !telegramChatIdInput.trim())}
                onclick={() => handleSaveTelegram(false)}
              >
                Save
              </Button>
              {#if webhookSecrets.hasTelegramToken}
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={testingChannel === 'telegram'}
                  onclick={() => handleTestWebhook('telegram')}
                >
                  <Send size={12} />
                  <span>{testingChannel === 'telegram' ? 'Sending...' : 'Test'}</span>
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={savingWebhook}
                  onclick={() => handleSaveTelegram(true)}
                >
                  Remove
                </Button>
              {/if}
            </div>
          </div>
        </div>

        <div class="event-toggles">
          <label class="ev-chip">
            <span>Drop Claimed</span>
            <Toggle
              checked={settings.webhooks.onDropClaimed}
              ariaLabel="Notify on drop claimed"
              onchange={(v) => savePatch((d) => (d.webhooks.onDropClaimed = v))}
            />
          </label>
          <label class="ev-chip">
            <span>Action Required / Session Expired</span>
            <Toggle
              checked={settings.webhooks.onActionRequired}
              ariaLabel="Notify on action required"
              onchange={(v) => savePatch((d) => (d.webhooks.onActionRequired = v))}
            />
          </label>
          <label class="ev-chip">
            <span>Campaign Completed</span>
            <Toggle
              checked={settings.webhooks.onCampaignCompleted}
              ariaLabel="Notify on campaign completed"
              onchange={(v) => savePatch((d) => (d.webhooks.onCampaignCompleted = v))}
            />
          </label>
        </div>
      </div>

      {#if mockAvailable}
        <div class="card pref-card pref-wide">
          <div class="pref-wide-row">
            <div class="pref-wide-text">
              <h3>Mock platform mode</h3>
              <p>Simulates campaigns, live channels, progress, and claims offline with zero network calls.</p>
            </div>
            <Toggle
              checked={settings.mockMode}
              ariaLabel="Mock platform mode"
              onchange={(v) => savePatch((d) => (d.mockMode = v))}
            />
          </div>
        </div>
      {/if}
    </div>
  {:else}
    <div class="panel-stack">
      <div class="pref-grid">
        <div class="card pref-card">
          <div class="pref-top">
            <div class="pref-ico"><FolderOpen size={15} /></div>
            <div class="inline-actions">
              <Button variant="secondary" size="sm" onclick={() => api.openDataFolder().catch((e) => notifyUi(String(e), 'error'))}>
                Data
              </Button>
              <Button variant="secondary" size="sm" onclick={() => api.openLogFolder().catch((e) => notifyUi(String(e), 'error'))}>
                Logs
              </Button>
            </div>
          </div>
          <h3>Local app folders</h3>
          <p>Inspect saved settings (%APPDATA%\Dropilot) or redacted rolling logs (%LOCALAPPDATA%\Dropilot\logs).</p>
        </div>

        <div class="card pref-card">
          <div class="pref-top">
            <div class="pref-ico"><RefreshCw size={15} /></div>
            <div class="inline-actions">
              {#if $updateInfo?.available}
                {#if $updateInfo.signedUpdateReady}
                  <Button
                    variant="primary"
                    size="sm"
                    disabled={$installingUpdate}
                    onclick={() => void installAppUpdate()}
                  >
                    {$installingUpdate ? 'Updating...' : `Install v${$updateInfo.latestVersion}`}
                  </Button>
                {:else}
                  <Button variant="primary" size="sm" onclick={() => api.openUrl($updateInfo!.releaseUrl)}>
                    v{$updateInfo.latestVersion}
                  </Button>
                {/if}
              {/if}
              <Button variant="secondary" size="sm" disabled={$checkingForUpdate} onclick={handleCheckUpdates}>
                {$checkingForUpdate ? 'Checking...' : 'Check'}
              </Button>
              <Toggle
                checked={settings.updateCheck}
                ariaLabel="Check for signed updates on startup"
                onchange={(v) => savePatch((d) => (d.updateCheck = v))}
              />
            </div>
          </div>
          <h3>Signed auto-updater</h3>
          <p>Checks cryptographically verified releases on startup and lets you install with one click.</p>
        </div>

        <div class="card pref-card pref-wide">
          <div class="pref-wide-row">
            <div class="pref-wide-text">
              <div class="title-with-badge">
                <span class="pref-ico"><Cloud size={15} /></span>
                <div>
                  <div class="webhook-title-row">
                    <h3>Remote hotfix config</h3>
                    <Badge
                      tone={$remoteConfigStatus?.source === 'remote'
                        ? 'success'
                        : $remoteConfigStatus?.source === 'cache'
                          ? 'accent'
                          : 'neutral'}
                    >
                      {$remoteConfigStatus?.source ?? 'bundled'}
                    </Badge>
                    {#if $remoteConfigStatus && $remoteConfigStatus.overriddenHashes > 0}
                      <Badge tone="neutral">{$remoteConfigStatus.overriddenHashes} GQL hashes</Badge>
                    {/if}
                  </div>
                  <p>
                    Syncs verified Twitch GQL query hashes so Dropilot recovers from platform rotations without waiting for a new installer.
                    {#if $remoteConfigStatus?.fetchedAt}
                      Last synced {formatExactDateTime($remoteConfigStatus.fetchedAt)}.
                    {/if}
                  </p>
                </div>
              </div>
            </div>
            <div class="inline-actions">
              <Button
                variant="secondary"
                size="sm"
                disabled={syncingRemote || !settings.remoteConfigEnabled}
                onclick={handleRefreshRemoteConfig}
              >
                <RefreshCw size={13} />
                <span>{syncingRemote ? 'Syncing...' : 'Sync now'}</span>
              </Button>
              <Toggle
                checked={settings.remoteConfigEnabled}
                ariaLabel="Enable remote hotfix config"
                onchange={(v) => savePatch((d) => (d.remoteConfigEnabled = v))}
              />
            </div>
          </div>
        </div>
      </div>

      <section class="card about-card">
        <div class="about-head">
          <div class="pref-ico"><Info size={15} /></div>
          <div>
            <h2>About Dropilot</h2>
            <p class="studio-sub">Version 0.1.0 · MIT License · Standard user privileges (asInvoker)</p>
          </div>
        </div>
        <p class="about-note">
          <strong>Disclaimer:</strong> Using automation tools may violate Twitch or Kick Terms of Service. You are solely responsible for your accounts. Dropilot is not affiliated with, sponsored by, or endorsed by Twitch Interactive, Inc. or Kick Streaming Pty Ltd.
        </p>
        <div class="about-links">
          <Button variant="secondary" size="sm" onclick={() => api.openUrl('https://github.com/theflash1211/dropilot#readme')}>
            README
          </Button>
          <Button variant="secondary" size="sm" onclick={() => api.openUrl('https://github.com/theflash1211/dropilot/blob/main/docs/PRIVACY.md')}>
            Privacy
          </Button>
          <Button variant="secondary" size="sm" onclick={() => api.openUrl('https://github.com/theflash1211/dropilot/blob/main/docs/ENDPOINTS.md')}>
            Endpoints
          </Button>
        </div>
      </section>

      <section class="card danger-card">
        <div class="danger-row">
          <div>
            <h3>Reset &amp; clear all Dropilot data</h3>
            <p>Deletes Credential Manager tokens, Dropilot's isolated WebView2 cookies, and resets preferences to defaults.</p>
          </div>
          <Button variant="danger" size="sm" onclick={() => (clearModalOpen = true)}>
            <Trash2 size={13} />
            <span>Clear app data</span>
          </Button>
        </div>
      </section>
    </div>
  {/if}
</div>

<Modal open={clearModalOpen} title="Clear all Dropilot data?" onclose={() => (clearModalOpen = false)}>
  <p>
    This signs out of Twitch and Kick, deletes stored tokens from Windows Credential Manager, clears Dropilot's isolated WebView2 cookies, and resets settings to defaults.
  </p>
  <p style="margin-top: 8px; color: var(--text-muted); font-size: 13px;">
    Your personal web browsers (Chrome, Edge, Firefox) are never touched.
  </p>
  {#snippet footer()}
    <Button variant="secondary" size="sm" onclick={() => (clearModalOpen = false)}>Cancel</Button>
    <Button variant="danger" size="sm" onclick={handleClearData}>Clear app data</Button>
  {/snippet}
</Modal>

<style>
  .accounts-wrap {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .accounts-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--grid-gap, 14px);
  }

  @media (max-width: 780px) {
    .accounts-grid {
      grid-template-columns: 1fr;
    }
  }

  .acct-action {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .session-info-bar {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    background: var(--surface);
    border: 1px solid var(--border);
    font-size: var(--text-xs);
    color: var(--text-muted);
    line-height: 1.45;
  }

  .session-info-bar :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
    color: var(--accent-text);
  }

  .session-info-bar strong {
    color: var(--text);
  }

  .acct-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 14px 16px;
  }

  .acct-left {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .acct-meta {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  .acct-top {
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .acct-sub {
    font-size: 12px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .acct-sub strong { color: var(--text); }

  .seg-tabs {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 6px;
    padding: 5px;
    border-radius: var(--radius-md);
    background: var(--surface);
    border: 1px solid var(--border);
  }

  @media (max-width: 760px) {
    .seg-tabs {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .seg-tab {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 8px 11px;
    border-radius: var(--radius-sm);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-muted);
    text-align: left;
  }

  .seg-tab:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .seg-tab.active {
    background: var(--surface-2);
    border-color: var(--border-hover);
    color: var(--text);
  }

  .seg-ico {
    width: 26px;
    height: 26px;
    border-radius: var(--radius-xs);
    background: var(--surface-2);
    border: 1px solid var(--border);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    color: var(--text-muted);
  }

  .seg-tab.active .seg-ico {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-on);
  }

  .seg-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .seg-label {
    font-size: var(--text-sm);
    font-weight: 600;
    line-height: 1.2;
  }

  .seg-desc {
    font-size: 11px;
    color: var(--text-faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .panel-stack {
    display: flex;
    flex-direction: column;
    gap: var(--grid-gap, 14px);
  }

  .studio-card {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .studio-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .studio-sub {
    margin-top: 3px;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  /* Color Mode */
  .mode-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
  }

  @media (max-width: 720px) {
    .mode-grid {
      grid-template-columns: 1fr;
    }
  }

  .mode-btn {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 11px 13px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface-2);
    text-align: left;
  }

  .mode-btn:hover {
    border-color: var(--border-hover);
  }

  .mode-btn.active {
    border-color: var(--accent);
    background: var(--surface);
  }

  .mode-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .mode-text strong {
    font-size: var(--text-sm);
    color: var(--text);
  }

  .mode-text span {
    font-size: 11.5px;
    color: var(--text-muted);
  }

  .mode-check {
    color: var(--accent-text);
    display: inline-flex;
  }

  .appearance-split {
    display: grid;
    grid-template-columns: minmax(0, 1.35fr) minmax(320px, 0.9fr);
    gap: var(--grid-gap, 14px);
    align-items: start;
  }

  @media (max-width: 1180px) {
    .appearance-split {
      grid-template-columns: 1fr;
    }
  }

  .title-with-ico {
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .font-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }

  @media (max-width: 680px) {
    .font-grid {
      grid-template-columns: 1fr;
    }
  }

  .font-btn {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 11px 13px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface-2);
    text-align: left;
  }

  .font-btn:hover {
    border-color: var(--border-hover);
  }

  .font-btn.active {
    border-color: var(--accent);
    background: var(--surface);
  }

  .font-btn-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .font-btn-top strong {
    font-size: var(--text-sm);
    color: var(--text);
  }

  .font-hint {
    font-size: 11px;
    color: var(--text-muted);
  }

  .font-sample-row {
    margin-top: 4px;
    padding: 5px 8px;
    border-radius: var(--radius-xs);
    background: var(--bg);
    border: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .sample-sans {
    font-size: 12px;
    font-weight: 600;
    color: var(--text);
  }

  .sample-mono {
    font-size: 11px;
    color: var(--accent-text);
  }

  .scale-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }

  .scale-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .scale-meta strong {
    font-size: var(--text-sm);
    color: var(--text);
  }

  .scale-meta span {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .scale-pills {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 3px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }

  .scale-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 10px;
    border: 1px solid transparent;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--text-muted);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .scale-pill:hover {
    color: var(--text);
  }

  .scale-pill.active {
    background: var(--accent-tint);
    border-color: var(--accent);
    color: var(--accent-text);
  }

  .scale-pct {
    font-size: 10.5px;
    opacity: 0.85;
  }

  .preview-card {
    gap: 12px;
  }

  .preview-surface {
    display: flex;
    flex-direction: column;
    gap: 11px;
    padding: 12px;
    border-radius: var(--radius-sm);
    background: var(--bg);
    border: 1px solid var(--border);
  }

  .pv-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .pv-plat {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .pv-plat-name {
    font-size: var(--text-xs);
    font-weight: 700;
    color: var(--text);
  }

  .pv-acct {
    font-size: var(--text-xs);
    color: var(--text-faint);
  }

  .pv-status {
    padding: 2px 7px;
    border-radius: var(--radius-pill);
    background: var(--success-tint);
    color: var(--success-text);
    font-size: 10px;
    font-weight: 600;
  }

  .pv-camp {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .pv-camp-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .pv-camp-head strong {
    font-family: var(--font-display);
    font-size: var(--text-md);
    color: var(--text);
  }

  .pv-link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 7px;
    border-radius: var(--radius-pill);
    background: var(--success-tint);
    color: var(--success-text);
    font-size: 10.5px;
    font-weight: 600;
  }

  .pv-reward {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }

  .pv-prog {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .pv-prog-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .pv-prog-row strong {
    color: var(--text);
  }

  .pv-log {
    display: flex;
    align-items: baseline;
    gap: 7px;
    padding: 6px 8px;
    border-radius: var(--radius-xs);
    background: var(--surface);
    border: 1px solid var(--border);
    border-left: 3px solid var(--info);
    font-size: var(--text-xs);
    color: var(--text);
  }

  .pv-log-ts {
    color: var(--text-faint);
  }

  .pv-log-lvl {
    color: var(--info-text);
    font-weight: 600;
  }

  /* Preferences Grid */
  .pref-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--grid-gap, 14px);
  }

  @media (min-width: 1360px) {
    .pref-grid {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
  }

  @media (min-width: 1840px) {
    .pref-grid {
      grid-template-columns: repeat(4, minmax(0, 1fr));
    }
  }

  @media (max-width: 780px) {
    .pref-grid {
      grid-template-columns: 1fr;
    }
  }

  .pref-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .pref-wide {
    grid-column: 1 / -1;
  }

  .pref-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 2px;
  }

  .pref-ico {
    width: 30px;
    height: 30px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--text-muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .pref-card h3 {
    font-size: var(--text-base);
    font-weight: 600;
    color: var(--text);
  }

  .pref-card p {
    font-size: var(--text-xs);
    color: var(--text-muted);
    line-height: 1.45;
    max-width: 72ch;
  }

  .pref-wide-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 14px;
  }

  .pref-wide-text {
    flex: 1;
    min-width: 240px;
  }

  .title-with-badge {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }

  .sub-divider {
    height: 1px;
    background: var(--border);
    margin: 4px 0;
  }

  .schedule-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 14px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }

  .sched-field {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  .sched-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
  }

  .stepper {
    display: inline-flex;
    align-items: center;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    overflow: hidden;
  }

  .step-btn {
    width: 28px;
    height: 28px;
    border: none;
    background: transparent;
    color: var(--text);
    font-weight: 600;
  }

  .step-btn:hover {
    background: var(--surface-hover);
  }

  .step-val {
    min-width: 44px;
    text-align: center;
    font-size: 12px;
    font-weight: 600;
  }

  .inline-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .webhook-card {
    gap: 14px;
  }

  .webhook-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  @media (max-width: 860px) {
    .webhook-grid {
      grid-template-columns: 1fr;
    }
  }

  .webhook-box {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }

  .webhook-box-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .webhook-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .webhook-input-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .secret-input {
    flex: 1;
    min-width: 160px;
    background: var(--surface);
  }

  .chat-id-input {
    flex: 0 0 110px;
    min-width: 90px;
  }

  .event-toggles {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .ev-chip {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
    font-size: 12px;
    color: var(--text);
    cursor: pointer;
  }

  .about-card {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .about-head {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .about-note {
    font-size: var(--text-xs);
    color: var(--text-muted);
    line-height: 1.55;
  }

  .about-links {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .danger-card {
    border-color: var(--error);
  }

  .danger-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
  }

  .danger-row h3 {
    font-size: var(--text-base);
    color: var(--error-text);
  }

  .danger-row p {
    margin-top: 2px;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .webhook-feedback-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .webhook-feedback {
    margin: 0;
    font-family: var(--font-mono);
    font-size: 11px;
    line-height: 1.4;
    word-break: break-word;
  }

  .fb-open-link {
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--accent-text);
    border-radius: var(--radius-xs);
    padding: 2px 8px;
    font-family: var(--font-mono);
    font-size: 10.5px;
    font-weight: 600;
    white-space: nowrap;
  }

  .fb-open-link:hover {
    border-color: var(--accent);
  }

  .webhook-feedback.fb-ok {
    color: var(--success-text);
  }

  .webhook-feedback.fb-err {
    color: var(--error-text);
  }
</style>
