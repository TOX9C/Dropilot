<script lang="ts">
  import AlertTriangle from 'lucide-svelte/icons/alert-triangle';
  import Clock from 'lucide-svelte/icons/clock';
  import Pause from 'lucide-svelte/icons/pause';
  import Pin from 'lucide-svelte/icons/pin';
  import Play from 'lucide-svelte/icons/play';
  import RefreshCw from 'lucide-svelte/icons/refresh-cw';
  import { nowMs } from '../lib/clock';
  import { formatEndCountdown, formatEta, formatViewerCount, progressPercent } from '../lib/format';
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
  const pct = $derived(drop ? progressPercent(drop.currentMinutes, drop.requiredMinutes) : 0);
  const eta = $derived(
    drop ? formatEta(drop.currentMinutes, drop.requiredMinutes) : stateReason ?? 'Waiting for stream',
  );
  const isPaused = $derived(state === 'Paused');
  const isPinned = $derived(
    Boolean(channel && pinnedChannel && channel.login.toLowerCase() === pinnedChannel.toLowerCase()),
  );
  const campEnd = $derived(campaign ? formatEndCountdown(campaign.endsAt, $nowMs) : null);
</script>

<section class="card now-card now-{platform} stripe-{platform} wash-{platform}" class:error-border={state === 'Error'}>
  {#if channel}
    <div class="top">
      <div class="channel-block">
        <Monogram src={channel.avatarUrl} name={channel.displayName} width={44} height={44} rounded />

        <div class="ch-meta">
          <div class="ch-name-row">
            <span class="ch-name">{channel.displayName}</span>
            <span class="viewers tabular">
              {formatViewerCount(channel.viewerCount)} viewers
            </span>
          </div>
          <div class="ch-sub">
            <span>{campaign ? campaign.game.name : channel.gameName}</span>
            {#if campEnd}
              <Badge tone={campEnd.tone}>
                <Clock size={11} />
                <span>{campEnd.ended ? 'Ended' : `Ends in ${campEnd.text}`}</span>
              </Badge>
            {/if}
            {#if drop}
              <span aria-hidden="true">·</span>
              <span class="drop-title">{drop.name}</span>
              <span class="tabular">({drop.currentMinutes}/{drop.requiredMinutes} min)</span>
            {/if}
          </div>
        </div>
      </div>

      <div class="controls">
        {#if candidates.length > 0}
          <select
            class="pin-select"
            aria-label="Pin candidate channel"
            value={pinnedChannel ?? ''}
            onchange={(e) => {
              const val = (e.currentTarget as HTMLSelectElement).value;
              onPinChannel(val || null);
            }}
          >
            <option value="">Auto channel</option>
            {#each candidates as cand (cand.login)}
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

    <div class="progress-block">
      <div class="pct-row">
        <span class="big-pct tabular">{pct}%</span>
        <span class="eta tabular">{eta}</span>
      </div>
      <ProgressBar
        value={drop ? drop.currentMinutes : 0}
        max={drop ? drop.requiredMinutes : 100}
        {platform}
        tall
        label={drop ? drop.name : 'Watch progress'}
      />
    </div>
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
    gap: var(--space-4);
  }

  .now-twitch {
    border-color: rgba(168, 85, 247, 0.30);
  }

  .now-kick {
    border-color: rgba(83, 252, 24, 0.26);
  }

  .wash-twitch {
    background-image: var(--wash-twitch);
  }

  .wash-kick {
    background-image: var(--wash-kick);
  }

  .error-border {
    border-color: rgba(255, 93, 93, 0.45);
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
    gap: 2px;
    min-width: 0;
  }

  .ch-name-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .ch-name {
    font-size: var(--text-md);
    font-weight: 600;
  }

  .viewers {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .ch-sub {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }

  .drop-title {
    color: var(--text);
    font-weight: 600;
  }

  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .pin-select {
    max-width: 170px;
  }

  .progress-block {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .pct-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }

  .big-pct {
    font-size: 32px;
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1.05;
  }

  .eta {
    font-size: var(--text-sm);
    color: var(--text-muted);
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
