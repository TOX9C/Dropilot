<script lang="ts">
  import Activity from 'lucide-svelte/icons/activity';
  import Check from 'lucide-svelte/icons/check';
  import Clock from 'lucide-svelte/icons/clock';
  import Gift from 'lucide-svelte/icons/gift';
  import LayoutDashboard from 'lucide-svelte/icons/layout-dashboard';
  import LogIn from 'lucide-svelte/icons/log-in';
  import Pause from 'lucide-svelte/icons/pause';
  import Play from 'lucide-svelte/icons/play';
  import RefreshCw from 'lucide-svelte/icons/refresh-cw';
  import Zap from 'lucide-svelte/icons/zap';
  import ActivityFeed from '../components/ActivityFeed.svelte';
  import Badge from '../components/Badge.svelte';
  import Button from '../components/Button.svelte';
  import Checklist from '../components/Checklist.svelte';
  import Countdown from '../components/Countdown.svelte';
  import Monogram from '../components/Monogram.svelte';
  import ProgressBar from '../components/ProgressBar.svelte';
  import SectionHeader from '../components/SectionHeader.svelte';
  import Sparkline from '../components/Sparkline.svelte';
  import StatTile from '../components/StatTile.svelte';
  import StatusPill from '../components/StatusPill.svelte';
  import { api } from '../lib/api';
  import { nowMs } from '../lib/clock';
  import {
    formatEndCountdown,
    formatEta,
    formatExactDateTime,
    formatMinutes,
    formatRelativeTime,
    formatViewerCount,
    progressPercent,
  } from '../lib/format';
  import {
    computeDashboardStats,
    computeEndingSoon,
    computeUpNextQueue,
  } from '../lib/selectors';
  import { activeRoute, notifyUi, sessionHistory, sessionMinutes } from '../lib/stores';
  import type {
    AppSnapshot,
    Campaign,
    Drop,
    LogEntry,
    PlatformId,
    PlatformSnapshot,
    Settings,
  } from '../lib/types';

  interface Props {
    snapshot: AppSnapshot;
    settings: Settings;
    logs: LogEntry[];
  }

  let { snapshot, settings, logs }: Props = $props();

  const stats = $derived(computeDashboardStats(snapshot, $sessionMinutes, $nowMs));
  const upNext = $derived(computeUpNextQueue(snapshot, settings, $nowMs, 4));
  const endingSoon = $derived(computeEndingSoon(snapshot, $nowMs));
  const cards = $derived([snapshot.twitch, snapshot.kick]);
  const allPaused = $derived(
    snapshot.twitch.state === 'Paused' && snapshot.kick.state === 'Paused',
  );

  function findActiveCampaign(snap: PlatformSnapshot): Campaign | null {
    if (!snap.currentCampaignId) return null;
    return snap.campaigns.find((c) => c.id === snap.currentCampaignId) ?? null;
  }

  function findActiveDrop(snap: PlatformSnapshot, camp: Campaign | null): Drop | null {
    if (!camp) return null;
    if (snap.currentDropId) {
      const exact = camp.drops.find((d) => d.id === snap.currentDropId);
      if (exact) return exact;
    }
    return camp.drops.find((d) => !d.claimed) ?? null;
  }

  function openChannelUrl(platform: PlatformId, login: string) {
    const url = platform === 'twitch' ? `https://www.twitch.tv/${login}` : `https://kick.com/${login}`;
    void api.openUrl(url).catch((err) => notifyUi(String(err), 'error'));
  }

  async function refreshAll() {
    try {
      await Promise.all([api.retryPlatform('twitch'), api.retryPlatform('kick')]);
      notifyUi('Refreshed Twitch and Kick');
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function togglePauseAll() {
    try {
      if (allPaused) {
        await Promise.all([api.resume('twitch'), api.resume('kick')]);
      } else {
        await Promise.all([api.pause('twitch'), api.pause('kick')]);
      }
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }
</script>

<div class="page">
  <header class="page-header">
    <div class="page-title-wrap">
      <div class="page-icon page-icon-cyan"><LayoutDashboard size={18} /></div>
      <div>
        <h1>Dashboard</h1>
        <p class="page-subtitle">Live farming overview across Twitch and Kick</p>
      </div>
    </div>

    <div class="page-actions">
      {#if snapshot.mockActive}
        <Badge tone="accent">Mock mode</Badge>
      {/if}
      <div class="sync-pill tabular" title="Automatic background check cadence">
        <Clock size={12} />
        <span>Next sync: <Countdown targetIso={stats.nextRefreshIso} /></span>
      </div>
      <Button variant="primary" size="sm" onclick={() => activeRoute.set('drops')}>
        <Gift size={14} />
        <span>Browse drops</span>
      </Button>
      <Button variant="secondary" size="sm" onclick={refreshAll}>
        <RefreshCw size={14} />
        <span>Refresh all</span>
      </Button>
      <Button variant="secondary" size="sm" onclick={togglePauseAll}>
        {#if allPaused}<Play size={14} /><span>Resume all</span>{:else}<Pause size={14} /><span>Pause all</span>{/if}
      </Button>
    </div>
  </header>

  <section class="stats-strip" aria-label="Session summary">
    <StatTile
      label="Active drops"
      value={stats.activeCampaigns}
      hint="Twitch & Kick live"
      tone="twitch"
    >
      {#snippet icon()}<Gift size={15} />{/snippet}
    </StatTile>

    <StatTile
      label="Session farmed"
      value={`${stats.sessionMinutes}m`}
      hint="Minutes watched"
      tone="accent"
    >
      {#snippet icon()}<Activity size={15} />{/snippet}
      {#snippet extra()}<Sparkline values={$sessionHistory} />{/snippet}
    </StatTile>

    <StatTile
      label="Drops claimed"
      value={stats.claimedTotal}
      hint={`${stats.claimedToday} claimed today`}
      tone="success"
    >
      {#snippet icon()}<Check size={15} />{/snippet}
    </StatTile>

    <StatTile
      label="Claimable now"
      value={stats.claimableNow}
      hint={stats.claimableNow > 0 ? 'Ready or awaiting you' : 'All caught up'}
      tone="kick"
    >
      {#snippet icon()}<Zap size={15} />{/snippet}
    </StatTile>
  </section>

  <section class="hero-grid" aria-label="Now farming">
    {#each cards as platSnap (platSnap.platform)}
      {@const camp = findActiveCampaign(platSnap)}
      {@const drop = findActiveDrop(platSnap, camp)}
      {@const queuedTarget = upNext.find((u) => u.platform === platSnap.platform) ?? null}
      {@const platName = platSnap.platform === 'twitch' ? 'Twitch' : 'Kick'}
      <article class="card hero-card hero-{platSnap.platform} stripe-{platSnap.platform} wash-{platSnap.platform}">
        <div class="hero-top">
          <div class="plat-title">
            <span class="plat-dot dot-{platSnap.platform}" aria-hidden="true"></span>
            <h2>{platName}</h2>
            {#if platSnap.account}
              <Badge tone={platSnap.platform}>@{platSnap.account.displayName}</Badge>
            {/if}
          </div>
          <StatusPill state={platSnap.state} />
        </div>

        {#if platSnap.watching}
          {@const pct = drop ? progressPercent(drop.currentMinutes, drop.requiredMinutes) : 0}
          {@const campEnd = camp ? formatEndCountdown(camp.endsAt, $nowMs) : null}
          <div class="hero-watching">
            <div class="watch-row">
              <Monogram
                src={platSnap.watching.avatarUrl}
                name={platSnap.watching.displayName}
                width={44}
                height={44}
                rounded
              />
              <div class="watch-meta">
                <div class="watch-ch">
                  <strong>{platSnap.watching.displayName}</strong>
                  <span class="viewers tabular">
                    {formatViewerCount(platSnap.watching.viewerCount)} viewers
                  </span>
                </div>
                <div class="watch-sub-row">
                  <span class="watch-sub">
                    {camp ? `${camp.game.name} · ${camp.name}` : platSnap.watching.gameName}
                  </span>
                  {#if camp && campEnd}
                    <span title={`Ends ${formatExactDateTime(camp.endsAt)}`}>
                      <Badge tone={campEnd.tone}>
                        <Clock size={10} />
                        <span>{campEnd.ended ? 'Ended' : `Ends in ${campEnd.text}`}</span>
                      </Badge>
                    </span>
                  {/if}
                </div>
              </div>
              <span class="big-pct tabular">{pct}%</span>
            </div>

            {#if drop}
              <div class="drop-meta tabular">
                <span>{drop.name}</span>
                <span>{drop.currentMinutes}/{drop.requiredMinutes} min · {formatEta(drop.currentMinutes, drop.requiredMinutes)}</span>
              </div>
              <ProgressBar
                value={drop.currentMinutes}
                max={drop.requiredMinutes}
                platform={platSnap.platform}
                tall
                label={drop.name}
              />
            {:else}
              <div class="drop-meta tabular">
                <span>Earning drop progress</span>
                <span>Syncing reward milestone...</span>
              </div>
              <ProgressBar value={5} max={100} platform={platSnap.platform} tall label="Watch progress" />
            {/if}
          </div>

          <div class="hero-actions">
            <div class="left-btns">
              <Button variant="secondary" size="sm" onclick={() => platSnap.state === 'Paused' ? api.resume(platSnap.platform) : api.pause(platSnap.platform)}>
                <Pause size={13} /> Pause
              </Button>
              <Button variant="secondary" size="sm" onclick={() => api.switchChannel(platSnap.platform)}>
                <RefreshCw size={13} /> Switch
              </Button>
              <Button variant="ghost" size="sm" onclick={() => openChannelUrl(platSnap.platform, platSnap.watching!.login)}>
                Open ↗
              </Button>
            </div>
            <Button variant="ghost" size="sm" onclick={() => activeRoute.set(platSnap.platform)}>
              Manage →
            </Button>
          </div>
        {:else}
          <div class="hero-idle">
            {#if queuedTarget && platSnap.state !== 'LoggedOut'}
              {@const qEnd = formatEndCountdown(queuedTarget.campaign.endsAt, $nowMs)}
              <div class="idle-target-card">
                <Monogram
                  src={queuedTarget.campaign.game.boxArtUrl}
                  name={queuedTarget.campaign.game.name}
                  width={36}
                  height={48}
                />
                <div class="idle-target-meta">
                  <div class="idle-target-top">
                    <span class="idle-target-game">{queuedTarget.campaign.game.name}</span>
                    <Badge tone={qEnd.tone}>
                      <Clock size={10} />
                      <span>{qEnd.ended ? 'Ended' : qEnd.text}</span>
                    </Badge>
                  </div>
                  <strong class="idle-target-name">{queuedTarget.campaign.name}</strong>
                  <span class="idle-target-status">
                    {platSnap.lastError ?? platSnap.stateReason ?? 'Queued to farm'}
                  </span>
                </div>
              </div>
            {:else}
              <p class="idle-msg">
                {#if platSnap.state === 'LoggedOut'}
                  {#if platSnap.campaigns.length > 0}
                    {platSnap.campaigns.length} public {platName} campaigns available to browse. Sign in to farm drops and track progress.
                  {:else}
                    Sign in with your {platName} account to fetch active campaigns and farm drops.
                  {/if}
                {:else}
                  {platSnap.lastError ?? platSnap.stateReason ?? `No active stream selected for ${platName}.`}
                {/if}
              </p>
            {/if}

            <div class="hero-actions">
              {#if platSnap.state === 'LoggedOut'}
                <Button variant={platSnap.platform} size="sm" onclick={() => api.login(platSnap.platform)}>
                  <LogIn size={14} /> Log in to {platName}
                </Button>
              {:else if platSnap.state === 'Error'}
                <div class="left-btns">
                  <Button variant="secondary" size="sm" onclick={() => api.openDropsInventory(platSnap.platform)}>
                    Open {platName} drops ↗
                  </Button>
                  <Button variant={platSnap.platform} size="sm" onclick={() => api.retryPlatform(platSnap.platform)}>
                    <RefreshCw size={13} /> Retry now
                  </Button>
                </div>
              {:else}
                <div class="left-btns">
                  <Button variant="secondary" size="sm" onclick={() => api.retryPlatform(platSnap.platform)}>
                    <RefreshCw size={13} /> Check now
                  </Button>
                  <Button variant="secondary" size="sm" onclick={() => activeRoute.set('drops')}>
                    Browse drops →
                  </Button>
                </div>
              {/if}
              <Button variant="ghost" size="sm" onclick={() => activeRoute.set(platSnap.platform)}>
                Open {platName} →
              </Button>
            </div>
          </div>
        {/if}
      </article>
    {/each}
  </section>

  <Checklist {snapshot} {settings} />

  <div class="bottom-grid">
    <div class="col-stack">
      <section class="card panel panel-cyan">
        <SectionHeader title="Up next" count={upNext.length} uppercase>
          {#snippet icon()}<span class="sec-ico-cyan"><Play size={14} /></span>{/snippet}
        </SectionHeader>

        {#if upNext.length === 0}
          <p class="empty-note">Queue is empty. Pick campaigns from All Drops or Twitch/Kick to queue rewards here.</p>
        {:else}
          <div class="list-stack">
            {#each upNext as item (item.campaign.id + item.drop.id)}
              <div class="mini-row">
                <Monogram src={item.drop.rewards[0]?.imageUrl} name={item.campaign.game.name} width={30} height={30} />
                <div class="mini-mid">
                  <div class="mini-top">
                    <span class="mini-title">{item.drop.name}</span>
                    <Badge tone={item.platform}>{item.campaign.game.name}</Badge>
                  </div>
                  <div class="mini-sub tabular">
                    <span>{formatMinutes(item.drop.currentMinutes)} / {formatMinutes(item.drop.requiredMinutes)}</span>
                  </div>
                  <ProgressBar
                    value={item.drop.currentMinutes}
                    max={item.drop.requiredMinutes}
                    platform={item.platform}
                    label={item.drop.name}
                  />
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </section>

      <section class="card panel panel-warn">
        <SectionHeader title="Ending soon (< 48h)" count={endingSoon.length} uppercase>
          {#snippet icon()}<span class="sec-ico-warn"><Clock size={15} /></span>{/snippet}
        </SectionHeader>

        {#if endingSoon.length === 0}
          <p class="empty-note">No active campaigns expiring in the next 48 hours.</p>
        {:else}
          <div class="list-stack">
            {#each endingSoon as item (item.campaign.id)}
              {@const endInfo = formatEndCountdown(item.campaign.endsAt, $nowMs)}
              <button
                type="button"
                class="ending-row"
                title={`Ends ${formatExactDateTime(item.campaign.endsAt)}`}
                onclick={() => activeRoute.set('drops')}
              >
                <Monogram src={item.campaign.game.boxArtUrl} name={item.campaign.game.name} width={28} height={36} />
                <div class="mini-mid">
                  <span class="mini-title">{item.campaign.name}</span>
                  <span class="mini-sub">{item.campaign.game.name} · {item.platform === 'twitch' ? 'Twitch' : 'Kick'}</span>
                </div>
                <Badge tone={endInfo.tone}>
                  <Clock size={10} />
                  <span>{endInfo.ended ? 'Ended' : endInfo.text}</span>
                </Badge>
              </button>
            {/each}
          </div>
        {/if}
      </section>
    </div>

    <div class="col-stack">
      <section class="card panel panel-success">
        <SectionHeader title="Recent claims" count={snapshot.recentClaims.length} uppercase>
          {#snippet icon()}<span class="sec-ico-success"><Check size={15} /></span>{/snippet}
        </SectionHeader>

        {#if snapshot.recentClaims.length === 0}
          <div class="claims-empty">
            <Check size={18} />
            <div>
              <strong>No drops claimed yet this session</strong>
              <p>Completed rewards are claimed automatically and logged here.</p>
            </div>
          </div>
        {:else}
          <div class="list-stack">
            {#each snapshot.recentClaims.slice(0, 6) as claim (claim.id + claim.claimedAt)}
              <div class="mini-row">
                <Monogram src={claim.rewardImageUrl} name={claim.dropName} width={32} height={32} />
                <div class="mini-mid">
                  <span class="mini-title">{claim.dropName}</span>
                  <span class="mini-sub">{claim.gameName} · {claim.campaignName}</span>
                </div>
                <Badge tone={claim.platform}>{claim.platform}</Badge>
                <span class="claim-ts tabular">{formatRelativeTime(claim.claimedAt, $nowMs)}</span>
              </div>
            {/each}
          </div>
        {/if}
      </section>

      <ActivityFeed {logs} />
    </div>
  </div>
</div>

<style>
  .sync-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 11px;
    border-radius: var(--radius-pill);
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .stats-strip {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--space-3);
  }

  @media (max-width: 820px) {
    .stats-strip {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .hero-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
    gap: var(--space-4);
  }

  .hero-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .hero-twitch {
    border-color: rgba(168, 85, 247, 0.32);
  }

  .hero-kick {
    border-color: rgba(83, 252, 24, 0.28);
  }

  .sec-ico-cyan { display: flex; color: var(--cyan-text); }
  .sec-ico-warn { display: flex; color: var(--warning-text); }
  .sec-ico-success { display: flex; color: var(--success-text); }

  .wash-twitch {
    background-image: var(--wash-twitch);
  }

  .wash-kick {
    background-image: var(--wash-kick);
  }

  .hero-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .plat-title {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .hero-watching {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .watch-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .watch-meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .watch-ch {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .viewers {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .watch-sub-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .watch-sub,
  .mini-title,
  .mini-sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .watch-sub {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .big-pct {
    font-size: 26px;
    font-weight: 700;
    letter-spacing: -0.02em;
  }

  .drop-meta {
    display: flex;
    justify-content: space-between;
    gap: var(--space-2);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .hero-idle {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    flex: 1;
    gap: var(--space-3);
  }

  .idle-msg {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }

  .idle-target-card {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 11px;
    border-radius: var(--radius-md);
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid var(--border);
  }

  :global(:root[data-theme="light"]) .idle-target-card {
    background: var(--surface-2);
  }

  .idle-target-meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .idle-target-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }

  .idle-target-game {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .idle-target-name {
    font-size: 13px;
    font-weight: 700;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .idle-target-status {
    font-size: 11px;
    color: var(--text-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hero-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding-top: var(--space-2);
    border-top: 1px solid var(--border);
  }

  .left-btns {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .bottom-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
    gap: var(--space-4);
    align-items: start;
  }

  .col-stack {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .empty-note {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }

  .list-stack {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .mini-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: 7px 9px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }

  .ending-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: 7px 9px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid transparent;
    text-align: left;
    width: 100%;
  }

  .ending-row:hover {
    border-color: var(--border-hover);
    background: var(--surface-hover);
  }

  .mini-mid {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .mini-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .mini-title {
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .mini-sub {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .claim-ts {
    font-size: var(--text-xs);
    color: var(--text-faint);
    white-space: nowrap;
  }

  .claims-empty {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    color: var(--text-muted);
    font-size: var(--text-xs);
  }

  .claims-empty strong {
    display: block;
    color: var(--text);
    font-size: var(--text-sm);
    margin-bottom: 2px;
  }
</style>
