<script lang="ts">
  import Activity from 'lucide-svelte/icons/activity';
  import Check from 'lucide-svelte/icons/check';
  import Clock from 'lucide-svelte/icons/clock';
  import ExternalLink from 'lucide-svelte/icons/external-link';
  import Gift from 'lucide-svelte/icons/gift';
  import Play from 'lucide-svelte/icons/play';
  import Zap from 'lucide-svelte/icons/zap';
  import ActivityFeed from '../components/ActivityFeed.svelte';
  import Badge from '../components/Badge.svelte';
  import Button from '../components/Button.svelte';
  import Checklist from '../components/Checklist.svelte';
  import Countdown from '../components/Countdown.svelte';
  import ProgressBar from '../components/ProgressBar.svelte';
  import SectionHeader from '../components/SectionHeader.svelte';
  import Sparkline from '../components/Sparkline.svelte';
  import StatTile from '../components/StatTile.svelte';
  import StatusPill from '../components/StatusPill.svelte';
  import { api } from '../lib/api';
  import { nowMs, slowNowMs } from '../lib/clock';
  import {
    formatEndCountdown,
    formatEtaClock,
    formatExactDateTime,
    formatLiveDuration,
    formatLivePercent,
    formatLiveRemaining,
    formatMinutes,
    formatRelativeTime,
  } from '../lib/format';
  import {
    computeDashboardStats,
    computeEndingSoon,
    computeLiveCampaignProgress,
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

  const stats = $derived(computeDashboardStats(snapshot, $sessionMinutes, $slowNowMs));
  const upNext = $derived(computeUpNextQueue(snapshot, settings, $slowNowMs, 6));
  const endingSoon = $derived(computeEndingSoon(snapshot, $slowNowMs));
  const cards = $derived([snapshot.twitch, snapshot.kick]);

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
</script>

<div class="page dash-page">
  <header class="page-header">
    <div class="page-title-wrap">
      <h1>Overview // Active Miners</h1>
    </div>

    <div class="page-actions">
      {#if snapshot.mockActive}
        <Badge tone="neutral">Mock mode</Badge>
      {/if}
      <Button variant="primary" size="sm" onclick={() => activeRoute.set('drops')}>
        <Gift size={13} />
        <span>Browse drops</span>
      </Button>
    </div>
  </header>

  <section class="card console-miners" aria-label="Active miners">
    <div class="cm-head tabular">
      <span>PLATFORM</span>
      <span>STREAM &amp; CAMPAIGN</span>
      <span>ACTIVE REWARD &amp; PROGRESS</span>
      <span class="cm-right">ETA / STATUS</span>
      <span class="cm-right">ACTIONS</span>
    </div>

    {#each cards as platSnap (platSnap.platform)}
      {@const camp = findActiveCampaign(platSnap)}
      {@const drop = findActiveDrop(platSnap, camp)}
      {@const platName = platSnap.platform === 'twitch' ? 'TWITCH' : 'KICK'}
      {@const isMining = platSnap.state === 'Mining'}
      {@const liveProg = platSnap.watching
        ? computeLiveCampaignProgress(camp, drop?.id ?? null, isMining, platSnap.watchingSince, platSnap.progressSyncedAt, $nowMs)
        : null}
      {@const primarySkin = liveProg?.nextSkin ?? null}

      <div class="cm-row">
        <div class="cm-plat">
          <span class="plat-dot dot-{platSnap.platform}" aria-hidden="true"></span>
          <button type="button" class="cm-plat-link" onclick={() => activeRoute.set(platSnap.platform)}>
            <strong>{platName}</strong>
            {#if platSnap.account}
              <span class="cm-acct">@{platSnap.account.login}</span>
            {/if}
          </button>
        </div>

        {#if platSnap.watching}
          <div class="cm-stream">
            <span class="cm-ch">{platSnap.watching.displayName}</span>
            <span class="cm-sub truncate">
              {camp ? `${camp.game.name} — ${camp.name}` : platSnap.watching.gameName}
            </span>
          </div>

          <div class="cm-prog">
            {#if liveProg && primarySkin}
              <div class="cm-prog-top tabular">
                <span class="truncate">
                  {primarySkin.skinName}
                  {#if liveProg.isMultiDrop}
                    <span class="cm-dim">({liveProg.completedDrops}/{liveProg.totalDrops})</span>
                  {/if}
                </span>
                <strong>{formatLivePercent(primarySkin.liveMinutes, primarySkin.requiredMinutes)}</strong>
              </div>
              <ProgressBar
                value={primarySkin.liveMinutes}
                max={primarySkin.requiredMinutes}
                platform={platSnap.platform}
                label={primarySkin.skinName}
              />
            {:else}
              <span class="cm-sub">Syncing reward milestone...</span>
            {/if}
          </div>

          <div class="cm-eta tabular">
            {#if liveProg && primarySkin}
              <strong>{formatLiveRemaining(primarySkin.remainingSeconds)}</strong>
              <span class="cm-dim">
                {formatLiveDuration(primarySkin.liveMinutes)} / {formatMinutes(primarySkin.requiredMinutes)}
                {#if primarySkin.remainingSeconds > 0}
                  · {formatEtaClock(primarySkin.remainingSeconds, $nowMs)}
                {/if}
              </span>
            {:else}
              <StatusPill state={platSnap.state} compact />
            {/if}
          </div>

          <div class="cm-actions">
            <Button variant="secondary" size="sm" onclick={() => platSnap.state === 'Paused' ? api.resume(platSnap.platform) : api.pause(platSnap.platform)}>
              {platSnap.state === 'Paused' ? 'Resume' : 'Pause'}
            </Button>
            <Button variant="ghost" size="sm" onclick={() => api.switchChannel(platSnap.platform)}>Switch</Button>
            <Button variant="ghost" size="sm" onclick={() => openChannelUrl(platSnap.platform, platSnap.watching!.login)} ariaLabel="Open stream">
              <ExternalLink size={12} />
            </Button>
          </div>
        {:else}
          <div class="cm-idle-span">
            <StatusPill state={platSnap.state} compact />
            <span class="cm-idle-reason truncate">
              {#if platSnap.state === 'LoggedOut'}
                Not signed in — connect {platSnap.platform === 'twitch' ? 'Twitch' : 'Kick'} to track and farm drops
              {:else}
                {platSnap.lastError ?? platSnap.stateReason ?? 'Standing by for eligible live channels'}
              {/if}
            </span>
            {#if platSnap.nextActionAt && platSnap.state !== 'LoggedOut'}
              <span class="cm-dim tabular">retry <Countdown targetIso={platSnap.nextActionAt} /></span>
            {/if}
          </div>

          <div class="cm-actions">
            {#if platSnap.state === 'LoggedOut'}
              <Button variant="primary" size="sm" onclick={() => api.login(platSnap.platform)}>Log in</Button>
            {:else}
              <Button variant="secondary" size="sm" onclick={() => api.retryPlatform(platSnap.platform)}>Retry</Button>
              <Button variant="ghost" size="sm" onclick={() => activeRoute.set(platSnap.platform)}>Open</Button>
            {/if}
          </div>
        {/if}
      </div>
    {/each}
  </section>

  <section class="stats-strip" aria-label="Session summary">
    <StatTile label="Active drops" value={stats.activeCampaigns} hint="Twitch & Kick live">
      {#snippet icon()}<Gift size={14} />{/snippet}
    </StatTile>

    <StatTile label="Session farmed" value={`${stats.sessionMinutes}m`} hint="Minutes credited">
      {#snippet icon()}<Activity size={14} />{/snippet}
      {#snippet extra()}<Sparkline values={$sessionHistory} />{/snippet}
    </StatTile>

    <StatTile label="Drops claimed" value={stats.claimedTotal} hint={`${stats.claimedToday} claimed today`}>
      {#snippet icon()}<Check size={14} />{/snippet}
    </StatTile>

    <StatTile
      label="Claimable now"
      value={stats.claimableNow}
      hint={stats.claimableNow > 0 ? 'Ready or awaiting you' : 'All caught up'}
    >
      {#snippet icon()}<Zap size={14} />{/snippet}
    </StatTile>
  </section>

  <Checklist {snapshot} {settings} />

  <div class="bottom-grid">
    <div class="col-stack">
      <section class="card panel">
        <SectionHeader title="Up next" count={upNext.length} uppercase>
          {#snippet icon()}<Play size={14} />{/snippet}
        </SectionHeader>

        {#if upNext.length === 0}
          <div class="panel-empty">
            Queue is empty. Pick campaigns from All Drops to schedule rewards here.
          </div>
        {:else}
          <div class="list-stack">
            {#each upNext as item (item.campaign.id)}
              <div class="mini-row">
                <span class="plat-dot dot-{item.platform}" aria-hidden="true"></span>
                <div class="mini-mid">
                  <div class="mini-top">
                    <span class="mini-title">{item.campaign.name}</span>
                    <div class="mini-badges">
                      {#if item.totalDrops > 0}
                        <Badge tone="neutral">
                          {item.claimedDrops > 0
                            ? `${item.claimedDrops}/${item.totalDrops} drops`
                            : `${item.totalDrops} ${item.totalDrops === 1 ? 'drop' : 'drops'}`}
                        </Badge>
                      {/if}
                      <span class="plat-inline">
                        <span class="plat-dot dot-{item.platform}" aria-hidden="true"></span>
                        <span>{item.campaign.game.name}</span>
                      </span>
                    </div>
                  </div>
                  <div class="mini-sub tabular">
                    <span>{formatMinutes(item.currentMinutes)} / {formatMinutes(item.requiredMinutes)}</span>
                    {#if item.totalDrops > 1}
                      <span class="mini-next-hint">· Next: {item.drop.rewards[0]?.name || item.drop.name}</span>
                    {/if}
                  </div>
                  <ProgressBar
                    value={item.currentMinutes}
                    max={item.requiredMinutes}
                    platform={item.platform}
                    label={item.campaign.name}
                  />
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </section>

      <section class="card panel">
        <SectionHeader title="Ending soon (< 48h)" count={endingSoon.length} uppercase>
          {#snippet icon()}<Clock size={14} />{/snippet}
        </SectionHeader>

        {#if endingSoon.length === 0}
          <div class="panel-empty">No active campaigns expiring in the next 48 hours.</div>
        {:else}
          <div class="list-stack">
            {#each endingSoon as item (item.campaign.id)}
              {@const endInfo = formatEndCountdown(item.campaign.endsAt, $slowNowMs)}
              <button
                type="button"
                class="ending-row"
                title={`Ends ${formatExactDateTime(item.campaign.endsAt)}`}
                onclick={() => activeRoute.set('drops')}
              >
                <span class="plat-dot dot-{item.platform}" aria-hidden="true"></span>
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
      <section class="card panel">
        <SectionHeader title="Recent claims" count={snapshot.recentClaims.length} uppercase>
          {#snippet icon()}<Check size={14} />{/snippet}
        </SectionHeader>

        {#if snapshot.recentClaims.length === 0}
          <div class="panel-empty">
            Completed rewards are claimed automatically and logged here.
          </div>
        {:else}
          <div class="list-stack">
            {#each snapshot.recentClaims.slice(0, 6) as claim (claim.id + claim.claimedAt)}
              <div class="mini-row">
                <span class="plat-dot dot-{claim.platform}" aria-hidden="true"></span>
                <div class="mini-mid">
                  <span class="mini-title">{claim.dropName}</span>
                  <span class="mini-sub">{claim.gameName} · {claim.campaignName}</span>
                </div>
                <Badge tone={claim.platform}>{claim.platform}</Badge>
                <span class="claim-ts tabular">{formatRelativeTime(claim.claimedAt, $slowNowMs)}</span>
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
  .stats-strip {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--grid-gap, 8px);
  }

  @media (max-width: 820px) {
    .stats-strip {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .console-miners {
    padding: 0;
    overflow-x: auto;
  }

  .cm-head,
  .cm-row {
    display: grid;
    grid-template-columns: 118px minmax(120px, 1.1fr) minmax(145px, 1.25fr) 132px auto;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
  }

  .cm-head {
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    color: var(--text-faint);
  }

  .cm-row {
    border-bottom: 1px solid var(--border);
    font-size: 12px;
  }

  .cm-row:last-child {
    border-bottom: none;
  }

  .cm-right { text-align: right; }

  .cm-plat {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
  }

  .cm-plat-link {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    border: none;
    background: transparent;
    padding: 0;
    text-align: left;
    min-width: 0;
  }

  .cm-plat-link strong {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text);
  }

  .cm-plat-link:hover strong { color: var(--accent-text); }

  .cm-acct {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--text-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 110px;
  }

  .cm-stream {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .cm-ch { font-weight: 600; color: var(--text); }
  .cm-sub { font-size: 11px; color: var(--text-muted); }

  .cm-prog {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .cm-prog-top {
    display: flex;
    justify-content: space-between;
    gap: 6px;
    font-size: 11px;
  }

  .cm-eta {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 1px;
    font-size: 11px;
  }

  .cm-dim { color: var(--text-faint); font-size: 10.5px; }

  .cm-idle-span {
    grid-column: span 3;
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .cm-idle-reason {
    flex: 1;
    color: var(--text-muted);
    font-size: 12px;
  }

  .cm-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 4px;
  }

  @media (max-width: 1040px) {
    .cm-head { display: none; }
    .cm-row {
      grid-template-columns: 112px minmax(0, 1fr) auto;
      row-gap: 6px;
    }
    .cm-actions {
      grid-column: 3;
      grid-row: 1;
    }
    .cm-prog {
      grid-column: 1 / 3;
      grid-row: 2;
    }
    .cm-eta {
      grid-column: 3;
      grid-row: 2;
      align-items: flex-end;
    }
    .cm-idle-span {
      grid-column: 2 / 3;
    }
  }

  .bottom-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
    gap: var(--grid-gap, 8px);
    align-items: start;
  }

  .col-stack {
    display: flex;
    flex-direction: column;
    gap: var(--grid-gap, 8px);
  }

  .panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .panel-empty {
    padding: 12px;
    border-radius: var(--radius-xs);
    background: var(--surface-2);
    border: 1px dashed var(--border);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .list-stack {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .mini-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 5px 7px;
    border: 1px solid var(--border);
    border-radius: var(--radius-xs);
    background: var(--surface-2);
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
    gap: 8px;
  }

  .mini-badges {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .plat-inline {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
  }

  .mini-title,
  .mini-sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mini-title { font-size: var(--text-sm); font-weight: 600; color: var(--text); }
  .mini-sub { font-size: 11px; color: var(--text-muted); }
  .mini-next-hint { color: var(--text-faint); }

  .ending-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 8px;
    border-radius: var(--radius-xs);
    border: 1px solid var(--border);
    background: var(--surface-2);
    text-align: left;
    color: inherit;
  }

  .ending-row:hover {
    border-color: var(--border-hover);
    background: var(--surface-hover);
  }

  .claim-ts {
    font-size: 11px;
    color: var(--text-faint);
    white-space: nowrap;
  }
</style>
