<script lang="ts">
  import Check from 'lucide-svelte/icons/check';
  import ChevronDown from 'lucide-svelte/icons/chevron-down';
  import ChevronUp from 'lucide-svelte/icons/chevron-up';
  import CircleCheck from 'lucide-svelte/icons/circle-check';
  import Clock from 'lucide-svelte/icons/clock';
  import ExternalLink from 'lucide-svelte/icons/external-link';
  import Gift from 'lucide-svelte/icons/gift';
  import Link2 from 'lucide-svelte/icons/link-2';
  import Play from 'lucide-svelte/icons/play';
  import Unlink from 'lucide-svelte/icons/unlink';
  import { slowNowMs } from '../lib/clock';
  import {
    formatEndCountdown,
    formatExactDateTime,
    formatStartsIn,
  } from '../lib/format';
  import type { GameDropGroup } from '../lib/selectors';
  import Badge from './Badge.svelte';
  import Monogram from './Monogram.svelte';

  interface Props {
    group: GameDropGroup;
    expanded: boolean;
    showPlatformBadge?: boolean;
    onToggleExpand: () => void;
    onToggleGameFarm: (farm: boolean) => void;
    onOpenUrl: (url: string) => void;
  }

  let {
    group,
    expanded,
    showPlatformBadge = false,
    onToggleExpand,
    onToggleGameFarm,
    onOpenUrl,
  }: Props = $props();

  let imgFailed = $state(false);

  $effect(() => {
    void group.game.boxArtUrl;
    imgFailed = false;
  });

  const endInfo = $derived(formatEndCountdown(group.soonestEndsAt, $slowNowMs));
  const exactEnd = $derived(formatExactDateTime(group.soonestEndsAt));
  const upcomingOnly = $derived(!group.hasActive && group.hasUpcoming);
  const upcomingLabel = $derived(formatStartsIn(group.soonestStartsAt, $slowNowMs));
  const platLabel = $derived(group.platform === 'twitch' ? 'Twitch' : 'Kick');
  const campaignCountLabel = $derived(
    `${group.campaigns.length} ${group.campaigns.length === 1 ? 'campaign' : 'campaigns'}`,
  );
  const rewardSummaryLabel = $derived(
    group.totalDrops > 0
      ? `${group.totalDrops} ${group.totalDrops === 1 ? 'reward' : 'rewards'}`
      : 'Inspect rewards',
  );
</script>

<article
  class="card card-interactive game-card"
  class:farming-active={group.isFarming}
  class:is-expanded={expanded}
>
  <button
    type="button"
    class="art-stage"
    aria-expanded={expanded}
    aria-label={expanded ? `Collapse ${group.game.name} campaigns` : `Expand ${group.game.name} campaigns`}
    onclick={onToggleExpand}
  >
    {#if group.game.boxArtUrl && !imgFailed}
      <img
        class="box-art"
        src={group.game.boxArtUrl}
        alt={group.game.name}
        loading="lazy"
        onerror={() => (imgFailed = true)}
      />
    {:else}
      <div class="art-fallback">
        <Monogram src={null} name={group.game.name} width={56} height={56} />
      </div>
    {/if}

    <div class="art-top-strip">
      {#if showPlatformBadge}
        <span class="plat-chip" title={platLabel}>
          <span class="plat-dot dot-{group.platform}" aria-hidden="true"></span>
          <span>{platLabel}</span>
        </span>
      {/if}

      {#if group.claimableCount > 0}
        <Badge tone="info">{group.claimableCount} ready</Badge>
      {:else if group.allCompleted}
        <Badge tone="success"><Check size={10} /> Done</Badge>
      {/if}
    </div>
  </button>

  <div class="card-body">
    <div class="title-row">
      <button
        type="button"
        class="game-title-btn"
        aria-expanded={expanded}
        onclick={onToggleExpand}
        title={group.game.name}
      >
        <span class="game-title">{group.game.name}</span>
      </button>
    </div>

    <div class="status-row">
      {#if group.linkState === 'ready'}
        <span class="conn-pill conn-ready" title="No external game account link required">
          <CircleCheck size={11} />
          <span>Ready</span>
        </span>
      {:else if group.linkState === 'linked'}
        <span class="conn-pill conn-connected" title="Your game account is connected">
          <Link2 size={11} />
          <span>Linked</span>
        </span>
      {:else if group.linkState === 'partial'}
        <button
          type="button"
          class="conn-pill conn-warn conn-clickable"
          title={`${group.linkedCount}/${group.linkRequiredCount} campaigns linked — click to connect account`}
          onclick={() => onOpenUrl(group.connectTargetUrl)}
        >
          <Unlink size={11} />
          <span>{group.linkedCount}/{group.linkRequiredCount} linked</span>
          <ExternalLink size={9} />
        </button>
      {:else}
        <button
          type="button"
          class="conn-pill conn-unlinked conn-clickable"
          title="Game account not linked — click to connect before claiming drops"
          onclick={() => onOpenUrl(group.connectTargetUrl)}
        >
          <Unlink size={11} />
          <span>Not linked</span>
          <ExternalLink size={9} />
        </button>
      {/if}

      <span class="camp-badge tabular" title={`${campaignCountLabel} · ${rewardSummaryLabel}`}>
        {campaignCountLabel}
        {#if group.totalDrops > 0}
          <span class="sep" aria-hidden="true">·</span>
          <span>{group.totalDrops} {group.totalDrops === 1 ? 'reward' : 'rewards'}</span>
        {/if}
      </span>
    </div>

    <div class="timing-row tabular">
      {#if upcomingOnly}
        <span class="timing-pill" title={`Starts ${formatExactDateTime(group.soonestStartsAt)} · Ends ${exactEnd}`}>
          <Clock size={11} />
          <span>{upcomingLabel}</span>
        </span>
      {:else}
        <span class="timing-pill" class:urgent={endInfo.tone === 'error'} class:soon={endInfo.tone === 'warning'} title={`Soonest campaign ends ${exactEnd}`}>
          <Clock size={11} />
          <span>{endInfo.ended ? 'Ended' : `Ends in ${endInfo.text}`}</span>
        </span>
      {/if}
    </div>
  </div>

  <footer class="card-footer">
    <button
      type="button"
      class="farm-btn"
      class:selected={group.isFarming}
      aria-pressed={group.isFarming}
      onclick={() => onToggleGameFarm(!group.isFarming)}
      title={group.isFarming ? `Stop farming ${group.game.name}` : `Auto-farm all ${group.game.name} campaigns`}
    >
      {#if group.isFarming}
        <Check size={12} />
        <span>Farming</span>
      {:else}
        <Play size={11} />
        <span>Farm</span>
      {/if}
    </button>

    <button
      type="button"
      class="expand-btn"
      class:active={expanded}
      aria-expanded={expanded}
      onclick={onToggleExpand}
      title={expanded ? 'Hide campaigns and rewards' : 'Inspect campaigns and rewards'}
    >
      <Gift size={12} />
      <span>{expanded ? 'Hide' : 'Rewards'}</span>
      {#if expanded}<ChevronUp size={12} />{:else}<ChevronDown size={12} />{/if}
    </button>
  </footer>
</article>

<style>
  .game-card {
    display: flex;
    flex-direction: column;
    padding: 0;
    overflow: hidden;
    border-radius: var(--radius-md);
  }

  .game-card.farming-active {
    border-color: var(--accent);
  }

  .game-card.is-expanded {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .art-stage {
    position: relative;
    width: 100%;
    height: 144px;
    border: none;
    border-bottom: 1px solid var(--border);
    padding: 0;
    background: var(--surface-2);
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .box-art {
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: center 22%;
    display: block;
  }

  .art-fallback {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--surface-2);
  }

  .art-top-strip {
    position: absolute;
    top: 7px;
    left: 7px;
    right: 7px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    pointer-events: none;
  }

  .plat-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 7px;
    border-radius: var(--radius-xs);
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
  }

  .card-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 10px 11px;
    min-width: 0;
  }

  .title-row {
    display: flex;
    align-items: flex-start;
    gap: 6px;
  }

  .game-title-btn {
    border: none;
    background: transparent;
    padding: 0;
    text-align: left;
    color: var(--text);
    width: 100%;
  }

  .game-title-btn:hover .game-title {
    color: var(--accent-text);
  }

  .game-title {
    font-family: var(--font-display);
    font-size: var(--text-md);
    font-weight: 700;
    line-height: 1.25;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .status-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .conn-pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 7px;
    border-radius: var(--radius-pill);
    font-size: 10.5px;
    font-weight: 600;
    line-height: 1.45;
    white-space: nowrap;
    border: 1px solid transparent;
  }

  .conn-ready {
    background: var(--surface-2);
    border-color: var(--border);
    color: var(--text-muted);
  }

  .conn-connected {
    background: var(--success-tint);
    color: var(--success-text);
  }

  .conn-warn {
    background: var(--warning-tint);
    color: var(--warning-text);
  }

  .conn-unlinked {
    background: var(--error-tint);
    color: var(--error-text);
  }

  .conn-clickable:hover {
    border-color: currentColor;
  }

  .camp-badge {
    font-size: 11px;
    color: var(--text-muted);
    display: inline-flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }

  .sep {
    color: var(--text-faint);
  }

  .timing-row {
    margin-top: auto;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .timing-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text-muted);
  }

  .timing-pill.soon {
    color: var(--warning-text);
    font-weight: 600;
  }

  .timing-pill.urgent {
    color: var(--error-text);
    font-weight: 600;
  }

  .card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 7px 10px;
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
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
  }

  .farm-btn:hover {
    border-color: var(--accent);
    color: var(--accent-text);
  }

  .farm-btn.selected {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-on);
  }

  .farm-btn.selected:hover {
    background: var(--accent-hover);
    color: var(--accent-on);
  }

  .expand-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 26px;
    padding: 0 8px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
  }

  .expand-btn:hover,
  .expand-btn.active {
    background: var(--accent-tint);
    border-color: var(--accent);
    color: var(--accent-text);
  }
</style>
