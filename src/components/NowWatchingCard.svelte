<script lang="ts">
  import AlertTriangle from 'lucide-svelte/icons/alert-triangle';
  import Check from 'lucide-svelte/icons/check';
  import Clock from 'lucide-svelte/icons/clock';
  import ExternalLink from 'lucide-svelte/icons/external-link';
  import Gift from 'lucide-svelte/icons/gift';
  import Layers from 'lucide-svelte/icons/layers';
  import Pause from 'lucide-svelte/icons/pause';
  import Pin from 'lucide-svelte/icons/pin';
  import Play from 'lucide-svelte/icons/play';
  import Radio from 'lucide-svelte/icons/radio';
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
  import {
    computeLiveCampaignProgress,
  } from '../lib/selectors';
  import { openLightbox } from '../lib/stores';
  import type { Campaign, Channel, Drop, MinerState, PlatformId } from '../lib/types';
  import Badge from './Badge.svelte';
  import Button from './Button.svelte';
  import Countdown from './Countdown.svelte';
  import CustomSelect, { type SelectOption } from './CustomSelect.svelte';
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
  const channelOptions = $derived.by((): SelectOption[] => {
    const opts: SelectOption[] = [
      {
        value: '',
        label: 'Auto channel',
        subtitle: 'Best live participating streamer',
        dotTone: 'accent',
      },
    ];
    if (
      pinnedChannel &&
      !participatingCandidates.some((c) => c.login.toLowerCase() === pinnedChannel.toLowerCase())
    ) {
      opts.push({
        value: pinnedChannel,
        label: pinnedChannel,
        subtitle: 'Currently offline',
        dotTone: 'offline',
      });
    }
    for (const cand of participatingCandidates) {
      opts.push({
        value: cand.login,
        label: cand.displayName,
        meta: formatViewerCount(cand.viewerCount),
        subtitle: 'Drops enabled · Live',
        avatarUrl: cand.avatarUrl,
        dotTone: 'live',
      });
    }
    return opts;
  });
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

<section class="card now-card design-console" class:error-border={state === 'Error'}>
  {#if channel}
    <div class="top">
      <div class="channel-block">
        <Monogram
          src={channel.avatarUrl}
          name={channel.displayName}
          width={32}
          height={32}
        />

        <div class="ch-meta">
          <div class="ch-name-row">
            <span class="plat-dot dot-{platform}" aria-hidden="true"></span>
            <span class="ch-name">{channel.displayName}</span>
            <span class="viewers tabular">
              {formatViewerCount(channel.viewerCount)} viewers
            </span>
            {#if isMining}
              <Badge tone="success">LIVE</Badge>
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
          <CustomSelect
            options={channelOptions}
            value={pinnedChannel ?? ''}
            onchange={(val) => onPinChannel(val || null)}
            ariaLabel="Select participating channel"
            title="Auto channel by default — only channels participating in this drop are listed"
            searchable={participatingCandidates.length >= 6}
            searchPlaceholder="Search participating streamers..."
            size="sm"
            minMenuWidth={256}
            maxWidth="200px"
          />
        {/if}

        <Button
          variant={isPinned ? 'primary' : 'secondary'}
          size="sm"
          onclick={() => onPinChannel(isPinned ? null : channel.login)}
          ariaLabel={isPinned ? 'Unpin channel' : 'Pin channel'}
          title={isPinned ? 'Unpin channel' : 'Pin current channel'}
        >
          <Pin size={13} />
        </Button>

        <Button variant="secondary" size="sm" onclick={() => onOpenChannel(channel.login)}>
          <ExternalLink size={13} />
          <span>Stream</span>
        </Button>

        <Button variant="secondary" size="sm" onclick={onSwitchChannel}>
          <RefreshCw size={13} />
          <span>Switch</span>
        </Button>

        <Button variant="secondary" size="sm" onclick={onPauseResume}>
          {#if isPaused}<Play size={13} /><span>Resume</span>{:else}<Pause size={13} /><span>Pause</span>{/if}
        </Button>
      </div>
    </div>

    {#if liveProg && liveProg.nextSkin}
      {@const skin = liveProg.nextSkin}
      <div class="progress-grid" class:dual={liveProg.isMultiDrop}>
        <div class="prog-panel">
          <div class="prog-head">
            <div class="prog-identity">
              {#if skin.imageUrl}
                <button
                  type="button"
                  class="skin-thumb-btn"
                  title="Click to view full artwork"
                  aria-label={`View full artwork for ${skin.skinName}`}
                  onclick={() =>
                    openLightbox({
                      src: skin.imageUrl!,
                      title: skin.skinName,
                      subtitle: campaign ? `${campaign.game.name} · ${campaign.name}` : undefined,
                    })}
                >
                  <img src={skin.imageUrl} alt={skin.skinName} class="skin-thumb" decoding="async" />
                </button>
              {:else}
                <span class="prog-icon"><Gift size={15} /></span>
              {/if}
              <div class="prog-titles">
                <span class="prog-eyebrow">
                  {liveProg.isMultiDrop ? 'Next reward' : 'Drop reward'}
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
                <span class="eta-clock">· ETA {formatEtaClock(skin.remainingSeconds, $nowMs)}</span>
              {/if}
            </span>
          </div>
        </div>

        {#if liveProg.isMultiDrop}
          <div class="prog-panel">
            <div class="prog-head">
              <div class="prog-identity">
                <span class="prog-icon"><Layers size={15} /></span>
                <div class="prog-titles">
                  <span class="prog-eyebrow">
                    Whole campaign · {liveProg.completedDrops}/{liveProg.totalDrops} drops
                  </span>
                  <strong class="prog-name truncate">
                    All {liveProg.totalDrops} rewards
                  </strong>
                </div>
              </div>
              <span class="big-pct big-pct-muted tabular">
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
                  <span class="eta-clock">· ETA {formatEtaClock(liveProg.wholeRemainingSeconds, $nowMs)}</span>
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
              title={`${m.skinName} — ${formatLiveDuration(m.liveMinutes)} / ${formatMinutes(m.requiredMinutes)} (${formatLiveRemaining(m.remainingSeconds)})`}
            >
              {#if m.claimed}
                <Check size={11} />
              {:else if m.imageUrl}
                <img src={m.imageUrl} alt="" class="ms-thumb" />
              {:else}
                <Gift size={11} />
              {/if}
              <span class="ms-name truncate">{m.skinName}</span>
              <span class="ms-time tabular">
                {#if m.claimed}
                  Claimed
                {:else if m.claimable}
                  Ready
                {:else}
                  {formatLiveRemaining(m.remainingSeconds)} / {formatMinutes(m.requiredMinutes)}
                {/if}
              </span>
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
      <div class="status-icon-wrap" class:is-err={state === 'Error'} aria-hidden="true">
        {#if state === 'Error'}
          <AlertTriangle size={17} />
        {:else}
          <Radio size={17} />
        {/if}
      </div>

      <div class="status-main">
        <div class="status-title-row">
          <h2>
            {#if state === 'Error'}
              {stateReason ?? `${platName} needs attention`}
            {:else if isPaused}
              Mining paused
            {:else}
              Standing by on {platName}
            {/if}
          </h2>
          <StatusPill {state} />
        </div>
        <p class="status-detail">
          {lastError ?? stateReason ?? `Select a campaign or switch idle behavior to farm ${platName} drops.`}
          {#if nextActionAt}
            <span class="next-inline tabular"> · Retrying in <Countdown targetIso={nextActionAt} /></span>
          {/if}
        </p>
      </div>

      <div class="controls">
        {#if state === 'Error'}
          <Button variant="secondary" size="sm" onclick={onOpenInventory}>
            <span>Open {platName} drops</span>
          </Button>
          <Button variant="primary" size="sm" onclick={onRetry}>
            <RefreshCw size={13} />
            <span>Retry now</span>
          </Button>
        {:else}
          <Button variant="secondary" size="sm" onclick={onRetry}>
            <RefreshCw size={13} />
            <span>Check now</span>
          </Button>
          <Button variant="secondary" size="sm" onclick={onPauseResume}>
            {#if isPaused}<Play size={13} /><span>Resume</span>{:else}<Pause size={13} /><span>Pause</span>{/if}
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

  .now-card.design-console {
    gap: 10px;
  }

  .error-border {
    border-color: var(--error);
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
    gap: 12px;
    min-width: 0;
  }

  .ch-meta {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .ch-name-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
  }

  .ch-name {
    font-family: var(--font-display);
    font-size: var(--text-md);
    font-weight: 700;
    color: var(--text);
  }

  .viewers {
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

  .camp-game {
    color: var(--text);
    font-weight: 600;
  }

  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
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
    background: var(--surface-2);
    border: 1px solid var(--border);
  }

  .design-console .prog-panel {
    padding: 8px 10px;
    gap: 6px;
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

  .skin-thumb-btn {
    width: 36px;
    height: 36px;
    padding: 2px;
    border-radius: var(--radius-sm);
    background: var(--surface);
    border: 1px solid var(--border);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    overflow: hidden;
  }

  .skin-thumb-btn:hover,
  .skin-thumb-btn:focus-visible {
    border-color: var(--accent);
  }

  .skin-thumb {
    width: 100%;
    height: 100%;
    object-fit: contain;
    display: block;
  }

  .prog-icon {
    width: 32px;
    height: 32px;
    border-radius: var(--radius-sm);
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text-muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .prog-titles {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .prog-eyebrow {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-faint);
  }

  .prog-name {
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
  }

  .big-pct {
    font-family: var(--font-display);
    font-size: 26px;
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1;
    color: var(--text);
    flex-shrink: 0;
  }

  .design-console .big-pct {
    font-family: var(--font-mono);
    font-size: 18px;
  }

  .big-pct-muted {
    color: var(--text-muted);
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
    font-weight: 500;
  }

  .eta-clock {
    color: var(--text-faint);
    margin-left: 3px;
  }

  .milestones-strip {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
    gap: 6px;
  }

  .ms-chip {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    padding: 5px 8px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border: 1px solid var(--border);
    font-size: 11px;
    color: var(--text-muted);
  }

  .ms-thumb {
    width: 18px;
    height: 18px;
    border-radius: 3px;
    object-fit: contain;
    background: var(--surface);
    flex-shrink: 0;
  }

  .ms-chip.ms-next {
    background: var(--accent-tint);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .ms-chip.ms-done {
    background: var(--success-tint);
    border-color: transparent;
    color: var(--success-text);
  }

  .ms-name {
    flex: 1;
    min-width: 0;
    color: var(--text);
  }

  .ms-chip.ms-done .ms-name {
    color: var(--success-text);
  }

  .ms-time {
    color: var(--text-faint);
    font-family: var(--font-mono);
    font-size: 10.5px;
    font-weight: 600;
    flex-shrink: 0;
    white-space: nowrap;
  }

  .ms-chip.ms-next .ms-time {
    color: var(--accent-text);
  }

  .status-icon-wrap {
    width: 36px;
    height: 36px;
    border-radius: var(--radius-md);
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--text-muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .status-icon-wrap.is-err {
    background: var(--error-tint);
    color: var(--error-text);
    border-color: transparent;
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

  .status-detail {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }

  .next-inline {
    color: var(--text-faint);
  }
</style>
