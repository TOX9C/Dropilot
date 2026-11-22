<script lang="ts">
  import AlertTriangle from 'lucide-svelte/icons/alert-triangle';
  import Check from 'lucide-svelte/icons/check';
  import Clock from 'lucide-svelte/icons/clock';
  import Gift from 'lucide-svelte/icons/gift';
  import Layers from 'lucide-svelte/icons/layers';
  import Pause from 'lucide-svelte/icons/pause';
  import Pin from 'lucide-svelte/icons/pin';
  import Play from 'lucide-svelte/icons/play';
  import RefreshCw from 'lucide-svelte/icons/refresh-cw';
  import { nowMs } from '../lib/clock';
  import {
    formatEndCountdown,
    formatEtaClock,
    formatLiveDuration,
    formatLivePercent,
    formatLiveRemaining,
    formatMinutes,
    formatViewerCount,
  } from '../lib/format';
  import { computeLiveCampaignProgress } from '../lib/selectors';
  import type { Campaign, Channel, Drop, MinerState, PlatformId } from '../lib/types';
  import Badge from './Badge.svelte';
  import Button from './Button.svelte';
  import Countdown from './Countdown.svelte';
  import Monogram from './Monogram.svelte';
  import ProgressBar from './ProgressBar.svelte';
  import StatusPill from './StatusPill.svelte';

  interface Props {
    platform: PlatformId;
    state: MinerState;
    stateReason: string | null;
    lastError: string | null;
    nextActionAt: string | null;
    watchingSince?: string | null;
    progressSyncedAt?: string | null;
    channel: Channel | null;
    campaign: Campaign | null;
    drop: Drop | null;
    candidates: Channel[];
    pinnedChannel: string | null;
    onPauseResume: () => void;
    onSwitchChannel: () => void;
    onPinChannel: (login: string | null) => void;
    onOpenChannel: (login: string) => void;
    onOpenInventory: () => void;
    onRetry: () => void;
  }

  let {
    platform,
    state,
    stateReason,
    lastError,
    nextActionAt,
    watchingSince = null,
    progressSyncedAt = null,
    channel,
    campaign,
    drop,
    candidates,
    pinnedChannel,
    onPauseResume,
    onSwitchChannel,
    onPinChannel,
    onOpenChannel,
    onOpenInventory,
    onRetry,
  }: Props = $props();

  const platName = $derived(platform === 'twitch' ? 'Twitch' : 'Kick');
  const isPaused = $derived(state === 'Paused');
  const isMining = $derived(state === 'Mining' && Boolean(channel));
  const participatingCandidates = $derived(
    candidates.filter((c) => c.isLive && c.dropsEnabled),
  );
  const isPinned = $derived(
    Boolean(channel && pinnedChannel && channel.login.toLowerCase() === pinnedChannel.toLowerCase()),
  );
  const campEnd = $derived(campaign ? formatEndCountdown(campaign.endsAt, $nowMs) : null);

  const liveProg = $derived(
    computeLiveCampaignProgress(
      campaign,
      drop?.id ?? null,
      isMining,
      watchingSince,
      progressSyncedAt,
      $nowMs,
    ),
  );
</script>

<section class="card now-card now-{platform} stripe-{platform} wash-{platform}" class:error-border={state === 'Error'}>
  {#if channel}
    <div class="top">
      <div class="channel-block">
        <Monogram src={channel.avatarUrl} name={channel.displayName} width={46} height={46} rounded />

        <div class="ch-meta">
          <div class="ch-name-row">
            <span class="ch-name">{channel.displayName}</span>
            <span class="viewers tabular">
              {formatViewerCount(channel.viewerCount)} viewers
            </span>
            {#if isMining}
              <span class="live-chip" title="Stream-less live watch timer (0 Mbps video)">
                <span class="live-dot" aria-hidden="true"></span>
                LIVE
              </span>
            {/if}
          </div>
          <div class="ch-sub">
            <span class="camp-game">{campaign ? campaign.game.name : channel.gameName}</span>
            {#if campaign}
              <span aria-hidden="true">·</span>
              <span class="camp-title">{campaign.name}</span>
            {/if}
            {#if campEnd}
              <Badge tone={campEnd.tone}>
                <Clock size={11} />
                <span>{campEnd.ended ? 'Ended' : `Ends in ${campEnd.text}`}</span>
              </Badge>
            {/if}
          </div>
        </div>
      </div>

      <div class="controls">
        {#if participatingCandidates.length > 0 || pinnedChannel}
          <select
            class="pin-select"
            aria-label="Select participating channel"
            title="Auto channel by default — only channels participating in this drop are listed"
            value={pinnedChannel ?? ''}
            onchange={(e) => {
              const val = (e.currentTarget as HTMLSelectElement).value;
              onPinChannel(val || null);
            }}
          >
            <option value="">Auto channel</option>
            {#if pinnedChannel && !participatingCandidates.some((c) => c.login.toLowerCase() === pinnedChannel.toLowerCase())}
              <option value={pinnedChannel}>{pinnedChannel} (offline)</option>
            {/if}
            {#each participatingCandidates as cand (cand.login)}
              <option value={cand.login}>
                {cand.displayName} ({formatViewerCount(cand.viewerCount)})
              </option>
            {/each}
          </select>
        {/if}

        <Button
          variant={isPinned ? 'primary' : 'secondary'}
          size="sm"
          onclick={() => onPinChannel(isPinned ? null : channel.login)}
          ariaLabel={isPinned ? 'Unpin channel' : 'Pin channel'}
          title={isPinned ? 'Unpin channel' : 'Pin current channel'}
        >
          <Pin size={14} />
        </Button>

        <Button variant="secondary" size="sm" onclick={() => onOpenChannel(channel.login)}>
          <span>Open stream ↗</span>
        </Button>

        <Button variant="secondary" size="sm" onclick={onSwitchChannel}>
          <RefreshCw size={14} />
          <span>Switch</span>
        </Button>

        <Button variant="secondary" size="sm" onclick={onPauseResume}>
          {#if isPaused}<Play size={14} /><span>Resume</span>{:else}<Pause size={14} /><span>Pause</span>{/if}
        </Button>
      </div>
    </div>

    {#if liveProg && liveProg.nextSkin}
      {@const skin = liveProg.nextSkin}
      <div class="progress-grid" class:dual={liveProg.isMultiDrop}>
        <!-- Next Skin / Current Drop Progress -->
        <div class="prog-panel prog-primary">
          <div class="prog-head">
            <div class="prog-identity">
              {#if skin.imageUrl}
                <img src={skin.imageUrl} alt={skin.skinName} class="skin-thumb" />
              {:else}
                <span class="prog-icon"><Gift size={15} /></span>
              {/if}
              <div class="prog-titles">
                <span class="prog-eyebrow">
                  {liveProg.isMultiDrop ? 'Next skin reward' : 'Drop reward'}
                </span>
                <strong class="prog-name truncate" title={skin.skinName}>{skin.skinName}</strong>
              </div>
            </div>
            <span class="big-pct tabular">
              {formatLivePercent(skin.liveMinutes, skin.requiredMinutes)}
            </span>
          </div>

          <ProgressBar
            value={skin.liveMinutes}
            max={skin.requiredMinutes}
            {platform}
            tall
            label={skin.skinName}
          />

          <div class="prog-foot tabular">
            <span class="prog-watched">
              {formatLiveDuration(skin.liveMinutes)} / {formatMinutes(skin.requiredMinutes)}
            </span>
            <span class="prog-eta">
              {formatLiveRemaining(skin.remainingSeconds)}
              {#if skin.remainingSeconds > 0}
                <span class="eta-clock">· {formatEtaClock(skin.remainingSeconds, $nowMs)}</span>
              {/if}
            </span>
          </div>
        </div>

        <!-- Whole Campaign Progress (for Multi-Drop Campaigns) -->
        {#if liveProg.isMultiDrop}
          <div class="prog-panel prog-whole">
            <div class="prog-head">
              <div class="prog-identity">
                <span class="prog-icon prog-icon-whole"><Layers size={15} /></span>
                <div class="prog-titles">
                  <span class="prog-eyebrow">
                    Whole drop · {liveProg.completedDrops}/{liveProg.totalDrops} skins
                  </span>
                  <strong class="prog-name truncate">
                    All {liveProg.totalDrops} rewards
                  </strong>
                </div>
              </div>
              <span class="big-pct big-pct-whole tabular">
                {formatLivePercent(liveProg.wholeLiveMinutes, liveProg.wholeRequiredMinutes)}
              </span>
            </div>

            <ProgressBar
              value={liveProg.wholeLiveMinutes}
              max={liveProg.wholeRequiredMinutes}
              platform="accent"
              tall
              label="Whole drop campaign progress"
            />

            <div class="prog-foot tabular">
              <span class="prog-watched">
                {formatLiveDuration(liveProg.wholeLiveMinutes)} / {formatMinutes(liveProg.wholeRequiredMinutes)}
              </span>
              <span class="prog-eta">
                {formatLiveRemaining(liveProg.wholeRemainingSeconds)}
                {#if liveProg.wholeRemainingSeconds > 0}
                  <span class="eta-clock">· {formatEtaClock(liveProg.wholeRemainingSeconds, $nowMs)}</span>
                {/if}
              </span>
            </div>
          </div>
        {/if}
      </div>

      {#if liveProg.isMultiDrop}
        <div class="milestones-strip" aria-label="Campaign reward milestones">
          {#each liveProg.milestones as m (m.id)}
            {@const isNext = liveProg.nextSkin?.id === m.id}
            <div
              class="ms-chip"
              class:ms-done={m.claimed || m.claimable}
              class:ms-next={isNext && !m.claimed}
              title={`${m.skinName} — ${formatMinutes(m.requiredMinutes)} (${formatLiveRemaining(m.remainingSeconds)})`}
            >
              {#if m.claimed}
                <Check size={11} />
              {/if}
              <span class="ms-name truncate">{m.skinName}</span>
              <span class="ms-time tabular">{formatMinutes(m.requiredMinutes)}</span>
            </div>
          {/each}
        </div>
      {/if}
    {:else}
      <div class="prog-panel">
        <div class="prog-head">
          <div class="prog-titles">
            <span class="prog-eyebrow">Active stream</span>
            <strong class="prog-name">{stateReason ?? 'Syncing drop progress...'}</strong>
          </div>
          <span class="big-pct tabular">LIVE</span>
        </div>
        <ProgressBar value={12} max={100} {platform} tall label="Watch progress" />
      </div>
    {/if}
  {:else}
    <div class="status-merged">
      <div class="status-main">
        <div class="status-title-row">
          {#if state === 'Error'}
            <span class="err-icon" aria-hidden="true"><AlertTriangle size={17} /></span>
          {/if}
          <h2>
            {#if state === 'Error'}
              {stateReason ?? `${platName} needs attention`}
            {:else if isPaused}
              Mining paused
            {:else}
              Not watching a stream
            {/if}
          </h2>
          <StatusPill {state} />
        </div>
        <p class="status-detail">
          {lastError ?? stateReason ?? `Select a campaign or switch idle behavior to farm ${platName} drops.`}
          {#if nextActionAt}
            <span class="next-inline tabular"> · Next check in <Countdown targetIso={nextActionAt} /></span>
          {/if}
        </p>
      </div>

      <div class="controls">
        {#if state === 'Error'}
          <Button variant="secondary" size="sm" onclick={onOpenInventory}>
            <span>Open {platName} drops ↗</span>
          </Button>
          <Button variant="primary" size="sm" onclick={onRetry}>
            <RefreshCw size={14} />
            <span>Retry now</span>
          </Button>
        {:else}
          <Button variant="secondary" size="sm" onclick={onPauseResume}>
            {#if isPaused}<Play size={14} /><span>Resume</span>{:else}<Pause size={14} /><span>Pause</span>{/if}
          </Button>
        {/if}
      </div>
    </div>
  {/if}
</section>

<style>
  .now-card {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .now-twitch {
    border-color: rgba(168, 85, 247, 0.28);
  }

  .now-kick {
    border-color: rgba(83, 252, 24, 0.24);
  }

  .wash-twitch {
    background-image: var(--wash-twitch);
  }

  .wash-kick {
    background-image: var(--wash-kick);
  }

  .error-border {
    border-color: rgba(244, 63, 94, 0.48);
  }

  .top,
  .status-merged {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .channel-block {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }

  .ch-meta {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  .ch-name-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
  }

  .ch-name {
    font-size: var(--text-md);
    font-weight: 700;
  }

  .viewers {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .live-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 7px;
    border-radius: var(--radius-pill);
    background: var(--success-tint);
    border: 1px solid rgba(16, 185, 129, 0.34);
    color: var(--success-text);
    font-size: 10px;
    font-weight: 800;
    letter-spacing: 0.06em;
  }

  .live-dot {
    width: 6px;
    height: 6px;
    border-radius: var(--radius-pill);
    background: var(--success);
  }

  .ch-sub {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }

  .camp-game {
    color: var(--text);
    font-weight: 600;
  }

  .camp-title {
    color: var(--text-muted);
  }

  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .pin-select {
    max-width: 175px;
  }

  .progress-grid {
    display: grid;
    grid-template-columns: 1fr;
    gap: 10px;
  }

  .progress-grid.dual {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  @media (max-width: 780px) {
    .progress-grid.dual {
      grid-template-columns: 1fr;
    }
  }

  .prog-panel {
    display: flex;
    flex-direction: column;
    gap: 9px;
    padding: 12px 14px;
    border-radius: var(--radius-md);
    background: rgba(8, 10, 15, 0.42);
    border: 1px solid var(--border);
  }

  :global(:root[data-theme="light"]) .prog-panel {
    background: var(--surface-2);
  }

  .prog-whole {
    border-color: var(--border-hover);
  }

  .prog-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .prog-identity {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .skin-thumb {
    width: 34px;
    height: 34px;
    border-radius: 8px;
    object-fit: cover;
    background: var(--surface-2);
    border: 1px solid var(--border);
    flex-shrink: 0;
  }

  .prog-icon {
    width: 34px;
    height: 34px;
    border-radius: 8px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--text-muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .prog-icon-whole {
    background: var(--accent-tint);
    border-color: var(--border-hover);
    color: var(--accent-text);
  }

  .prog-titles {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .prog-eyebrow {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-faint);
  }

  .prog-name {
    font-size: 14px;
    font-weight: 700;
    color: var(--text);
  }

  .big-pct {
    font-size: 26px;
    font-weight: 800;
    letter-spacing: -0.02em;
    line-height: 1;
    flex-shrink: 0;
  }

  .big-pct-whole {
    color: var(--accent-text);
  }

  .prog-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
  }

  .prog-watched {
    color: var(--text);
    font-weight: 600;
  }

  .prog-eta {
    color: var(--text-muted);
    font-weight: 600;
  }

  .eta-clock {
    color: var(--text-faint);
    font-weight: 500;
    margin-left: 3px;
  }

  .milestones-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .ms-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 175px;
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
    font-size: 11px;
    color: var(--text-muted);
  }

  .ms-chip.ms-next {
    background: var(--accent-tint);
    border-color: var(--border-hover);
    color: var(--accent-text);
    font-weight: 600;
  }

  .ms-chip.ms-done {
    background: var(--success-tint);
    border-color: rgba(16, 185, 129, 0.32);
    color: var(--success-text);
  }

  .ms-name {
    min-width: 0;
  }

  .ms-time {
    color: var(--text-faint);
    font-weight: 700;
    flex-shrink: 0;
  }

  .ms-chip.ms-next .ms-time {
    color: var(--accent-text);
  }

  .status-main {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 240px;
    flex: 1;
  }

  .status-title-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .err-icon {
    color: var(--error-text);
    display: flex;
  }

  .status-detail {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }

  .next-inline {
    color: var(--text-faint);
  }
</style>
