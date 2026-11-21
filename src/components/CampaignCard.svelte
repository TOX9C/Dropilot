<script lang="ts">
  import Check from 'lucide-svelte/icons/check';
  import CircleCheck from 'lucide-svelte/icons/circle-check';
  import Clock from 'lucide-svelte/icons/clock';
  import ExternalLink from 'lucide-svelte/icons/external-link';
  import Link2 from 'lucide-svelte/icons/link-2';
  import Play from 'lucide-svelte/icons/play';
  import Star from 'lucide-svelte/icons/star';
  import Unlink from 'lucide-svelte/icons/unlink';
  import X from 'lucide-svelte/icons/x';
  import { nowMs } from '../lib/clock';
  import {
    formatEndCountdown,
    formatExactDateTime,
    formatMinutes,
    formatStartsIn,
    progressPercent,
  } from '../lib/format';
  import type { Campaign } from '../lib/types';
  import Badge from './Badge.svelte';
  import Monogram from './Monogram.svelte';
  import ProgressBar from './ProgressBar.svelte';

  interface Props {
    campaign: Campaign;
    isFarmSelected: boolean;
    isExcluded: boolean;
    isGamePrioritized: boolean;
    showPlatformBadge?: boolean;
    compact?: boolean;
    onToggleFarm: (farm: boolean) => void;
    onToggleGameFarm: (farm: boolean) => void;
    onExclude: () => void;
    onOpenUrl: (url: string) => void;
    onOpenInventory: () => void;
  }

  let {
    campaign,
    isFarmSelected,
    isExcluded,
    isGamePrioritized,
    showPlatformBadge = false,
    compact = false,
    onToggleFarm,
    onToggleGameFarm,
    onExclude,
    onOpenUrl,
    onOpenInventory,
  }: Props = $props();

  let expanded = $state(false);

  const linkState = $derived.by<'no_link' | 'connected' | 'unlinked' | 'unverified'>(() => {
    if (!campaign.requiresLink) return 'no_link';
    if (campaign.linkVerified && campaign.linked) return 'connected';
    if (campaign.linkVerified !== false && !campaign.linked) return 'unlinked';
    return 'unverified';
  });

  const allClaimed = $derived(
    campaign.drops.length > 0 && campaign.drops.every((d) => d.claimed),
  );
  const totalMinutes = $derived(campaign.drops.reduce((s, d) => s + d.requiredMinutes, 0));
  const endInfo = $derived(formatEndCountdown(campaign.endsAt, $nowMs));
  const exactEnd = $derived(formatExactDateTime(campaign.endsAt));
  const upcomingLabel = $derived(formatStartsIn(campaign.startsAt, $nowMs));
  const activeFarming = $derived(isFarmSelected && !isExcluded);
  const visibleDrops = $derived(expanded ? campaign.drops : campaign.drops.slice(0, 2));
  const overallPct = $derived.by(() => {
    if (campaign.drops.length === 0) return null;
    const req = campaign.drops.reduce((s, d) => s + d.requiredMinutes, 0);
    const cur = campaign.drops.reduce(
      (s, d) => s + (d.claimed ? d.requiredMinutes : d.currentMinutes),
      0,
    );
    return progressPercent(cur, req);
  });
</script>

{#snippet connectionBadge()}
  {#if linkState === 'no_link'}
    <span class="conn-pill conn-ready" title="No external game account link required">
      <CircleCheck size={11} />
      <span>Ready</span>
    </span>
  {:else if linkState === 'connected'}
    <span class="conn-pill conn-connected" title="Game account is linked">
      <Link2 size={11} />
      <span>Connected</span>
    </span>
  {:else if linkState === 'unlinked'}
    {#if campaign.linkUrl}
      <button
        type="button"
        class="conn-pill conn-unlinked conn-clickable"
        title="Game account not linked — click to open link page"
        onclick={() => onOpenUrl(campaign.linkUrl!)}
      >
        <Unlink size={11} />
        <span>Not linked</span>
      </button>
    {:else}
      <span class="conn-pill conn-unlinked" title="Game account not linked">
        <Unlink size={11} />
        <span>Not linked</span>
      </span>
    {/if}
  {:else if campaign.linkUrl}
    <button
      type="button"
      class="conn-pill conn-unverified conn-clickable"
      title="Open game account link page to check or connect"
      onclick={() => onOpenUrl(campaign.linkUrl!)}
    >
      <Link2 size={11} />
      <span>Link</span>
      <ExternalLink size={9} />
    </button>
  {/if}
{/snippet}

{#if compact}
  <article
    class="card card-interactive cv-compact compact-row plat-left-{campaign.platform}"
    class:farming-active={activeFarming}
    class:excluded={isExcluded}
  >
    <Monogram
      src={campaign.game.boxArtUrl}
      name={campaign.game.name}
      width={32}
      height={42}
    />

    <div class="compact-info">
      <div class="compact-top">
        {#if showPlatformBadge}
          <Badge tone={campaign.platform}>
            <span class="plat-dot dot-{campaign.platform}" aria-hidden="true"></span>
            {campaign.platform === 'twitch' ? 'Twitch' : 'Kick'}
          </Badge>
        {/if}
        <span class="compact-game">{campaign.game.name}</span>
        <span class="compact-sep" aria-hidden="true">·</span>
        <span class="compact-title" title={campaign.name}>{campaign.name}</span>
      </div>

      <div class="compact-sub tabular">
        {@render connectionBadge()}
        {#if campaign.drops.length > 0}
          <span>{campaign.drops.length} {campaign.drops.length === 1 ? 'drop' : 'drops'} · {formatMinutes(totalMinutes)}</span>
          {#if overallPct !== null && overallPct > 0}
            <span class="compact-pct">· {overallPct}%</span>
          {/if}
        {:else}
          <span>Rewards sync on farm</span>
        {/if}
        {#if allClaimed}
          <Badge tone="success"><Check size={10} /> Completed</Badge>
        {/if}
      </div>
    </div>

    <div class="compact-right">
      {#if campaign.status === 'upcoming'}
        <span class="end-pill" title={`Starts ${formatExactDateTime(campaign.startsAt)} · Ends ${exactEnd}`}>
          <Badge tone="info"><Clock size={10} /><span>{upcomingLabel}</span></Badge>
        </span>
      {:else}
        <span class="end-pill" title={`Ends ${exactEnd}`}>
          <Badge tone={endInfo.tone}><Clock size={10} /><span>{endInfo.ended ? 'Ended' : endInfo.text}</span></Badge>
        </span>
      {/if}

      <button
        type="button"
        class="farm-btn compact-farm farm-{campaign.platform}"
        class:selected={activeFarming}
        aria-pressed={activeFarming}
        onclick={() => onToggleFarm(!activeFarming)}
      >
        {#if activeFarming}
          <Check size={12} />
          <span>Farming</span>
        {:else}
          <Play size={11} />
          <span>{isExcluded ? 'Include' : 'Farm'}</span>
        {/if}
      </button>

      <button
        type="button"
        class="icon-btn"
        class:star-active={isGamePrioritized}
        aria-pressed={isGamePrioritized}
        aria-label={isGamePrioritized ? `Unpin game ${campaign.game.name}` : `Prioritize game ${campaign.game.name}`}
        title={isGamePrioritized ? 'Game prioritized' : 'Prioritize all campaigns for this game'}
        onclick={() => onToggleGameFarm(!isGamePrioritized)}
      >
        <Star size={13} />
      </button>

      {#if campaign.linkUrl}
        <button
          type="button"
          class="icon-btn"
          aria-label={`Open link page for ${campaign.game.name}`}
          title="Open game account link page ↗"
          onclick={() => onOpenUrl(campaign.linkUrl!)}
        >
          <ExternalLink size={13} />
        </button>
      {/if}

      {#if !isExcluded}
        <button
          type="button"
          class="icon-btn"
          aria-label={`Exclude ${campaign.name}`}
          title="Exclude campaign"
          onclick={onExclude}
        >
          <X size={13} />
        </button>
      {/if}
    </div>
  </article>
{:else}
  <article
    class="card card-interactive cv-card campaign-card stripe-{campaign.platform}"
    class:farming-active={activeFarming}
    class:excluded={isExcluded}
  >
    <div class="banner wash-{campaign.platform}">
      <Monogram
        src={campaign.game.boxArtUrl}
        name={campaign.game.name}
        width={38}
        height={50}
      />

      <div class="meta">
        <div class="top-row">
          <div class="game-wrap">
            {#if showPlatformBadge}
              <span
                class="plat-dot dot-{campaign.platform}"
                title={campaign.platform === 'twitch' ? 'Twitch' : 'Kick'}
                aria-label={campaign.platform === 'twitch' ? 'Twitch' : 'Kick'}
              ></span>
            {/if}
            <span class="game-name" title={campaign.game.name}>{campaign.game.name}</span>
          </div>

          {#if campaign.status === 'upcoming'}
            <span class="end-pill" title={`Starts ${formatExactDateTime(campaign.startsAt)} · Ends ${exactEnd}`}>
              <Badge tone="info">
                <Clock size={10} />
                <span>{upcomingLabel}</span>
              </Badge>
            </span>
          {:else}
            <span class="end-pill" title={`Ends ${exactEnd}`}>
              <Badge tone={endInfo.tone}>
                <Clock size={10} />
                <span>{endInfo.ended ? 'Ended' : endInfo.text}</span>
              </Badge>
            </span>
          {/if}
        </div>

        <h3 class="camp-name" title={campaign.name}>{campaign.name}</h3>

        <div class="summary-line tabular">
          {@render connectionBadge()}
          {#if campaign.drops.length > 0}
            <span>{campaign.drops.length} {campaign.drops.length === 1 ? 'drop' : 'drops'} · {formatMinutes(totalMinutes)}</span>
          {:else}
            <span>Rewards sync on farm</span>
          {/if}
          {#if allClaimed}
            <Badge tone="success"><Check size={10} /> Done</Badge>
          {/if}
        </div>
      </div>
    </div>

    {#if campaign.drops.length > 0}
      <div class="body">
        <div class="drops-list">
          {#each visibleDrops as drop (drop.id)}
            {@const pct = drop.claimed ? 100 : progressPercent(drop.currentMinutes, drop.requiredMinutes)}
            <div class="drop-row">
              <Monogram
                src={drop.rewards[0]?.imageUrl}
                name={drop.name}
                width={22}
                height={22}
              />
              <div class="drop-info">
                <div class="drop-title-row">
                  <span class="drop-name" title={drop.name}>{drop.name}</span>
                  <span class="drop-stat tabular">
                    {#if drop.claimed}
                      <Badge tone="success"><Check size={10} /> Claimed</Badge>
                    {:else if drop.needsManualClaim}
                      <button type="button" class="badge-manual" onclick={onOpenInventory}>
                        Claim ↗
                      </button>
                    {:else if drop.claimable}
                      <Badge tone="info">Ready</Badge>
                    {:else}
                      <span>{pct}% · {drop.currentMinutes}/{drop.requiredMinutes}m</span>
                    {/if}
                  </span>
                </div>
                <ProgressBar
                  value={drop.claimed ? drop.requiredMinutes : drop.currentMinutes}
                  max={drop.requiredMinutes}
                  platform={campaign.platform}
                  label={drop.name}
                />
              </div>
            </div>
          {/each}

          {#if campaign.drops.length > 2}
            <button
              type="button"
              class="expand-btn"
              onclick={() => (expanded = !expanded)}
            >
              {expanded ? 'Show fewer drops' : `+${campaign.drops.length - 2} more drops`}
            </button>
          {/if}
        </div>
      </div>
    {/if}

    <footer class="card-footer">
      <button
        type="button"
        class="farm-btn farm-{campaign.platform}"
        class:selected={activeFarming}
        aria-pressed={activeFarming}
        onclick={() => onToggleFarm(!activeFarming)}
      >
        {#if activeFarming}
          <Check size={12} />
          <span>Farming</span>
        {:else}
          <Play size={11} />
          <span>{isExcluded ? 'Include' : 'Farm'}</span>
        {/if}
      </button>

      <div class="footer-right">
        <button
          type="button"
          class="icon-btn"
          class:star-active={isGamePrioritized}
          aria-pressed={isGamePrioritized}
          aria-label={isGamePrioritized ? `Unpin game ${campaign.game.name}` : `Prioritize game ${campaign.game.name}`}
          title={isGamePrioritized ? 'Game prioritized' : 'Prioritize all campaigns for this game'}
          onclick={() => onToggleGameFarm(!isGamePrioritized)}
        >
          <Star size={12} />
          {#if isGamePrioritized}
            <span class="star-lbl">Game</span>
          {/if}
        </button>

        {#if campaign.linkUrl}
          <button
            type="button"
            class="icon-btn"
            aria-label={`Open link page for ${campaign.game.name}`}
            title="Open game account link page ↗"
            onclick={() => onOpenUrl(campaign.linkUrl!)}
          >
            <ExternalLink size={12} />
          </button>
        {/if}

        {#if !isExcluded}
          <button
            type="button"
            class="icon-btn"
            aria-label={`Exclude ${campaign.name}`}
            title="Exclude campaign"
            onclick={onExclude}
          >
            <X size={12} />
          </button>
        {/if}
      </div>
    </footer>
  </article>
{/if}

<style>
  .campaign-card {
    display: flex;
    flex-direction: column;
    padding: 0;
    overflow: hidden;
  }

  .campaign-card.farming-active,
  .compact-row.farming-active {
    border-color: rgba(129, 140, 248, 0.55);
    box-shadow:
      0 8px 24px rgba(99, 102, 241, 0.16),
      inset 0 1px 0 rgba(255, 255, 255, 0.09);
  }

  .campaign-card.excluded,
  .compact-row.excluded {
    opacity: 0.6;
  }

  .banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 10px;
  }

  .wash-twitch {
    background: var(--wash-twitch);
  }

  .wash-kick {
    background: var(--wash-kick);
  }

  .meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .top-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 5px;
  }

  .game-wrap {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    flex: 1;
  }

  .game-name {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .end-pill {
    flex-shrink: 0;
  }

  .camp-name {
    font-size: 13px;
    font-weight: 700;
    letter-spacing: -0.01em;
    line-height: 1.25;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .summary-line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text-muted);
  }

  .conn-pill {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 6px;
    border-radius: var(--radius-pill);
    font-size: 10px;
    font-weight: 700;
    line-height: 1.4;
    white-space: nowrap;
    border: 1px solid transparent;
  }

  .conn-ready {
    background: rgba(52, 211, 153, 0.12);
    color: var(--success-text);
  }

  .conn-connected {
    background: var(--success-tint);
    border-color: rgba(52, 211, 153, 0.35);
    color: var(--success-text);
  }

  .conn-unlinked {
    background: var(--warning-tint);
    border-color: rgba(251, 191, 36, 0.36);
    color: var(--warning-text);
  }

  .conn-unverified {
    background: var(--surface-2);
    border-color: var(--border);
    color: var(--text-muted);
  }

  .conn-clickable:hover {
    filter: brightness(1.12);
    border-color: var(--border-hover);
  }

  .body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 7px 10px;
    border-top: 1px solid var(--border);
  }

  .drops-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .drop-row {
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .drop-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .drop-title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }

  .drop-name {
    font-size: var(--text-xs);
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .drop-stat {
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
  }

  .expand-btn {
    align-self: flex-start;
    border: none;
    background: transparent;
    color: var(--accent-text);
    font-size: 11px;
    font-weight: 600;
    padding: 2px 0;
  }

  .expand-btn:hover {
    text-decoration: underline;
  }

  .badge-manual {
    background: var(--warning-tint);
    color: var(--warning-text);
    border: none;
    border-radius: var(--radius-pill);
    padding: 1px 7px;
    font-size: 11px;
    font-weight: 600;
  }

  .card-footer {
    margin-top: auto;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 6px 10px;
    background: var(--surface-2);
    border-top: 1px solid var(--border);
  }

  .farm-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
  }

  .farm-twitch:hover {
    border-color: rgba(168, 85, 247, 0.5);
    background: var(--twitch-tint);
    color: var(--twitch-text);
  }

  .farm-kick:hover {
    border-color: rgba(83, 252, 24, 0.45);
    background: var(--kick-tint);
    color: var(--kick-text);
  }

  .farm-btn.selected {
    background: var(--grad-twitch);
    border-color: transparent;
    color: var(--twitch-on);
  }

  .farm-btn.farm-kick.selected {
    background: var(--grad-kick);
    color: var(--kick-on);
  }

  .footer-right {
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .icon-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 26px;
    min-width: 26px;
    padding: 0 5px;
    border-radius: var(--radius-sm);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 600;
  }

  .icon-btn:hover {
    background: var(--surface-hover);
    border-color: var(--border);
    color: var(--text);
  }

  .icon-btn.star-active {
    background: var(--warning-tint);
    border-color: rgba(251, 191, 36, 0.35);
    color: var(--warning-text);
  }

  .star-lbl {
    font-size: 11px;
  }

  /* Compact horizontal row layout */
  .compact-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-radius: var(--radius-md);
  }

  .plat-left-twitch {
    border-left: 3px solid var(--twitch);
  }

  .plat-left-kick {
    border-left: 3px solid var(--kick);
  }

  .compact-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .compact-top {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .compact-game {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-muted);
    white-space: nowrap;
  }

  .compact-sep {
    color: var(--text-faint);
    font-size: var(--text-xs);
  }

  .compact-title {
    font-size: var(--text-sm);
    font-weight: 700;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .compact-sub {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-muted);
  }

  .compact-pct {
    color: var(--accent-text);
    font-weight: 600;
  }

  .compact-right {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .compact-farm {
    height: 26px;
    padding: 0 9px;
  }
</style>
