<script lang="ts">
  import Check from 'lucide-svelte/icons/check';
  import ChevronDown from 'lucide-svelte/icons/chevron-down';
  import ChevronUp from 'lucide-svelte/icons/chevron-up';
  import CircleCheck from 'lucide-svelte/icons/circle-check';
  import Clock from 'lucide-svelte/icons/clock';
  import ExternalLink from 'lucide-svelte/icons/external-link';
  import Gift from 'lucide-svelte/icons/gift';
  import Link2 from 'lucide-svelte/icons/link-2';
  import LoaderCircle from 'lucide-svelte/icons/loader-circle';
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
    onFetchDetails?: () => Promise<void>;
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
    onFetchDetails,
  }: Props = $props();

  let expandedDrops = $state(false);
  let skinsOpen = $state(false);
  let loadingDetails = $state(false);
  let detailsError = $state<string | null>(null);
  let brokenImages = $state<Record<string, boolean>>({});

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
  const visibleDrops = $derived(expandedDrops ? campaign.drops : campaign.drops.slice(0, 2));

  const skinTiles = $derived.by(() => {
    const tiles: Array<{
      key: string;
      skinName: string;
      dropName: string;
      imageUrl: string | null;
      requiredMinutes: number;
      currentMinutes: number;
      claimed: boolean;
      claimable: boolean;
      needsManualClaim: boolean;
    }> = [];
    for (const drop of campaign.drops) {
      const rewards = drop.rewards.length > 0 ? drop.rewards : [{ name: drop.name, imageUrl: null }];
      rewards.forEach((reward, idx) => {
        tiles.push({
          key: `${drop.id}-${idx}`,
          skinName: reward.name || drop.name,
          dropName: drop.name,
          imageUrl: reward.imageUrl,
          requiredMinutes: drop.requiredMinutes,
          currentMinutes: drop.currentMinutes,
          claimed: drop.claimed,
          claimable: drop.claimable,
          needsManualClaim: drop.needsManualClaim,
        });
      });
    }
    return tiles;
  });

  const overallPct = $derived.by(() => {
    if (campaign.drops.length === 0) return null;
    const req = campaign.drops.reduce((s, d) => s + d.requiredMinutes, 0);
    const cur = campaign.drops.reduce(
      (s, d) => s + (d.claimed ? d.requiredMinutes : d.currentMinutes),
      0,
    );
    return progressPercent(cur, req);
  });

  async function ensureDetailsLoaded() {
    if (!onFetchDetails || loadingDetails) return;
    if (campaign.platform !== 'twitch') return;
    if (campaign.linkVerified && campaign.drops.length > 0) return;

    loadingDetails = true;
    detailsError = null;
    try {
      await onFetchDetails();
    } catch (err) {
      detailsError = String(err);
    } finally {
      loadingDetails = false;
    }
  }

  async function toggleSkins() {
    skinsOpen = !skinsOpen;
    if (skinsOpen) {
      await ensureDetailsLoaded();
    }
  }

  async function handleCheckLink() {
    if (onFetchDetails) {
      await ensureDetailsLoaded();
    } else if (campaign.linkUrl) {
      onOpenUrl(campaign.linkUrl);
    }
  }
</script>

{#snippet connectionBadge()}
  {#if linkState === 'no_link'}
    <span class="conn-pill conn-ready" title="No external game account link required — ready to farm">
      <CircleCheck size={11} />
      <span>No link needed</span>
    </span>
  {:else if linkState === 'connected'}
    <span class="conn-pill conn-connected" title="Your game account is connected">
      <Link2 size={11} />
      <span>Connected</span>
    </span>
  {:else if linkState === 'unlinked'}
    {#if campaign.linkUrl}
      <button
        type="button"
        class="conn-pill conn-unlinked conn-clickable"
        title="Game account not connected — click to open link page"
        onclick={() => onOpenUrl(campaign.linkUrl!)}
      >
        <Unlink size={11} />
        <span>Not connected</span>
      </button>
    {:else}
      <span class="conn-pill conn-unlinked" title="Game account not connected">
        <Unlink size={11} />
        <span>Not connected</span>
      </span>
    {/if}
  {:else}
    <button
      type="button"
      class="conn-pill conn-unverified conn-clickable"
      disabled={loadingDetails}
      title="Click to verify whether this game account is connected & load skins"
      onclick={handleCheckLink}
    >
      {#if loadingDetails}
        <span class="spin-wrap"><LoaderCircle size={11} /></span>
        <span>Checking...</span>
      {:else}
        <Link2 size={11} />
        <span>Check link</span>
      {/if}
    </button>
  {/if}
{/snippet}

{#snippet skinsDrawer()}
  <div class="skins-drawer" class:skins-drawer-wide={compact}>
    <div class="skins-drawer-head">
      <div class="skins-drawer-title">
        <Gift size={13} />
        <span>Skins &amp; Rewards</span>
        {#if skinTiles.length > 0}
          <Badge>{skinTiles.length}</Badge>
        {/if}
      </div>

      {#if linkState === 'unlinked' && campaign.linkUrl}
        <button type="button" class="drawer-link-cta" onclick={() => onOpenUrl(campaign.linkUrl!)}>
          <Unlink size={11} />
          <span>Link game account ↗</span>
        </button>
      {:else if campaign.linkUrl}
        <button type="button" class="drawer-link-sub" onclick={() => onOpenUrl(campaign.linkUrl!)}>
          <span>Manage link ↗</span>
        </button>
      {/if}
    </div>

    {#if loadingDetails}
      <div class="skins-loading">
        <span class="spin-wrap"><LoaderCircle size={15} /></span>
        <span>Loading skins &amp; game connection status from Twitch...</span>
      </div>
    {:else if detailsError}
      <div class="skins-error">
        <span>{detailsError}</span>
        <div class="skins-error-actions">
          <button type="button" class="drawer-retry-btn" onclick={ensureDetailsLoaded}>Retry</button>
          {#if campaign.linkUrl}
            <button type="button" class="drawer-link-sub" onclick={() => onOpenUrl(campaign.linkUrl!)}>
              Open link ↗
            </button>
          {/if}
        </div>
      </div>
    {:else if skinTiles.length === 0}
      <div class="skins-empty">
        <span>No reward previews listed for this campaign.</span>
        {#if onFetchDetails && campaign.platform === 'twitch'}
          <button type="button" class="drawer-retry-btn" onclick={ensureDetailsLoaded}>Reload</button>
        {/if}
      </div>
    {:else}
      <div class="skin-grid" class:skin-grid-wide={compact}>
        {#each skinTiles as tile (tile.key)}
          {@const pct = tile.claimed ? 100 : progressPercent(tile.currentMinutes, tile.requiredMinutes)}
          {@const showImg = Boolean(tile.imageUrl && !brokenImages[tile.key])}
          <div class="skin-tile">
            <div class="skin-stage">
              {#if showImg}
                <img
                  class="skin-img"
                  src={tile.imageUrl!}
                  alt={tile.skinName}
                  loading="lazy"
                  onerror={() => {
                    brokenImages = { ...brokenImages, [tile.key]: true };
                  }}
                />
              {:else}
                <Monogram src={null} name={tile.skinName} width={52} height={52} />
              {/if}

              <span class="skin-time-chip tabular">{formatMinutes(tile.requiredMinutes)}</span>
            </div>

            <div class="skin-meta">
              <div class="skin-name" title={tile.skinName}>{tile.skinName}</div>
              {#if tile.dropName && tile.dropName !== tile.skinName}
                <div class="skin-sub" title={tile.dropName}>{tile.dropName}</div>
              {/if}

              <div class="skin-prog-row tabular">
                <ProgressBar
                  value={tile.claimed ? tile.requiredMinutes : tile.currentMinutes}
                  max={tile.requiredMinutes}
                  platform={campaign.platform}
                  label={tile.skinName}
                />
                <div class="skin-status">
                  {#if tile.claimed}
                    <Badge tone="success"><Check size={10} /> Claimed</Badge>
                  {:else if tile.needsManualClaim}
                    <button type="button" class="badge-manual" onclick={onOpenInventory}>Claim ↗</button>
                  {:else if tile.claimable}
                    <Badge tone="info">Ready</Badge>
                  {:else}
                    <span>{pct}% · {tile.currentMinutes}/{tile.requiredMinutes}m</span>
                  {/if}
                </div>
              </div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/snippet}

{#if compact}
  <article
    class="card card-interactive cv-compact compact-wrap plat-left-{campaign.platform}"
    class:farming-active={activeFarming}
    class:excluded={isExcluded}
    class:skins-expanded={skinsOpen}
  >
    <div class="compact-row">
      <Monogram
        src={campaign.game.boxArtUrl}
        name={campaign.game.name}
        width={34}
        height={44}
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
          <button
            type="button"
            class="compact-title-btn"
            aria-expanded={skinsOpen}
            onclick={toggleSkins}
            title="Click to inspect skins and rewards"
          >
            <span class="compact-title">{campaign.name}</span>
          </button>
        </div>

        <div class="compact-sub tabular">
          {@render connectionBadge()}
          {#if campaign.drops.length > 0}
            <span>{campaign.drops.length} {campaign.drops.length === 1 ? 'drop' : 'drops'} · {formatMinutes(totalMinutes)}</span>
            {#if overallPct !== null && overallPct > 0}
              <span class="compact-pct">· {overallPct}%</span>
            {/if}
          {:else}
            <span>Click to preview skins</span>
          {/if}
          {#if allClaimed}
            <Badge tone="success"><Check size={10} /> Completed</Badge>
          {/if}
        </div>
      </div>

      <div class="compact-right">
        <button
          type="button"
          class="skins-pill-btn"
          class:active={skinsOpen}
          aria-expanded={skinsOpen}
          onclick={toggleSkins}
          title={skinsOpen ? 'Hide skin preview' : 'Preview drop skins'}
        >
          <Gift size={11} />
          <span>{skinTiles.length > 0 ? `Skins (${skinTiles.length})` : 'Skins'}</span>
          {#if skinsOpen}<ChevronUp size={11} />{:else}<ChevronDown size={11} />{/if}
        </button>

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
    </div>

    {#if skinsOpen}
      {@render skinsDrawer()}
    {/if}
  </article>
{:else}
  <article
    class="card card-interactive cv-card campaign-card stripe-{campaign.platform}"
    class:farming-active={activeFarming}
    class:excluded={isExcluded}
    class:skins-expanded={skinsOpen}
  >
    <div class="banner wash-{campaign.platform}">
      <Monogram
        src={campaign.game.boxArtUrl}
        name={campaign.game.name}
        width={40}
        height={52}
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

        <button
          type="button"
          class="camp-title-btn"
          aria-expanded={skinsOpen}
          onclick={toggleSkins}
          title={skinsOpen ? 'Collapse skin preview' : 'Click to see skins & rewards in this drop'}
        >
          <h3 class="camp-name" title={campaign.name}>{campaign.name}</h3>
          <span class="title-chevron" aria-hidden="true">
            {#if skinsOpen}<ChevronUp size={13} />{:else}<ChevronDown size={13} />{/if}
          </span>
        </button>

        <div class="summary-line tabular">
          {@render connectionBadge()}
          <button
            type="button"
            class="skins-pill-btn"
            class:active={skinsOpen}
            aria-expanded={skinsOpen}
            onclick={toggleSkins}
          >
            <Gift size={10} />
            <span>{skinTiles.length > 0 ? `Skins (${skinTiles.length})` : 'View skins'}</span>
            {#if skinsOpen}<ChevronUp size={10} />{:else}<ChevronDown size={10} />{/if}
          </button>
          {#if campaign.drops.length > 0}
            <span>{formatMinutes(totalMinutes)}</span>
          {/if}
          {#if allClaimed}
            <Badge tone="success"><Check size={10} /> Done</Badge>
          {/if}
        </div>
      </div>
    </div>

    {#if skinsOpen}
      {@render skinsDrawer()}
    {:else if campaign.drops.length > 0}
      <div class="body">
        <div class="drops-list">
          {#each visibleDrops as drop (drop.id)}
            {@const pct = drop.claimed ? 100 : progressPercent(drop.currentMinutes, drop.requiredMinutes)}
            <div class="drop-row">
              <Monogram
                src={drop.rewards[0]?.imageUrl}
                name={drop.name}
                width={24}
                height={24}
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
              onclick={() => (expandedDrops = !expandedDrops)}
            >
              {expandedDrops ? 'Show fewer drops' : `+${campaign.drops.length - 2} more drops`}
            </button>
          {/if}
        </div>
      </div>
    {:else}
      <button type="button" class="unhydrated-stage" onclick={toggleSkins}>
        <span class="unhydrated-ico"><Gift size={13} /></span>
        <span class="unhydrated-txt">Click to preview reward skins &amp; verify link</span>
        <span class="unhydrated-cta">Preview ▾</span>
      </button>
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
  .compact-wrap.farming-active {
    border-color: rgba(129, 140, 248, 0.55);
    box-shadow:
      0 8px 24px rgba(99, 102, 241, 0.16),
      inset 0 1px 0 rgba(255, 255, 255, 0.09);
  }

  .campaign-card.skins-expanded,
  .compact-wrap.skins-expanded {
    border-color: rgba(167, 139, 250, 0.45);
  }

  .campaign-card.excluded,
  .compact-wrap.excluded {
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
    gap: 4px;
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

  .camp-title-btn {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 6px;
    width: 100%;
    border: none;
    background: transparent;
    padding: 0;
    color: inherit;
    text-align: left;
  }

  .camp-title-btn:hover .camp-name,
  .compact-title-btn:hover .compact-title {
    color: var(--accent-text);
  }

  .title-chevron {
    color: var(--text-faint);
    flex-shrink: 0;
    margin-top: 2px;
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
    gap: 4px;
    padding: 1px 7px;
    border-radius: var(--radius-pill);
    font-size: 10px;
    font-weight: 700;
    line-height: 1.45;
    white-space: nowrap;
    border: 1px solid transparent;
  }

  .conn-ready {
    background: rgba(56, 189, 248, 0.14);
    border-color: rgba(56, 189, 248, 0.36);
    color: var(--cyan-text);
  }

  .conn-connected {
    background: rgba(52, 211, 153, 0.16);
    border-color: rgba(52, 211, 153, 0.42);
    color: var(--success-text);
  }

  .conn-unlinked {
    background: rgba(248, 113, 113, 0.16);
    border-color: rgba(248, 113, 113, 0.45);
    color: var(--error-text);
  }

  .conn-unverified {
    background: rgba(251, 191, 36, 0.13);
    border-color: rgba(251, 191, 36, 0.36);
    color: var(--warning-text);
  }

  .conn-clickable:hover:not(:disabled) {
    filter: brightness(1.14);
    border-color: var(--border-hover);
  }

  .skins-pill-btn {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 7px;
    border-radius: var(--radius-pill);
    border: 1px solid rgba(167, 139, 250, 0.28);
    background: rgba(129, 140, 248, 0.10);
    color: var(--accent-text);
    font-size: 10px;
    font-weight: 700;
    line-height: 1.45;
    white-space: nowrap;
  }

  .skins-pill-btn:hover,
  .skins-pill-btn.active {
    background: var(--accent-tint);
    border-color: var(--accent);
  }

  .spin-wrap {
    display: inline-flex;
    animation: cc-spin 1s linear infinite;
  }

  @keyframes cc-spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  /* Expanded Skin & Reward Showcase Drawer */
  .skins-drawer {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 9px 10px;
    background: var(--surface-2);
    border-top: 1px solid var(--border);
  }

  .skins-drawer-wide {
    padding: 10px 14px;
  }

  .skins-drawer-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .skins-drawer-title {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    font-weight: 700;
    color: var(--text);
  }

  .drawer-link-cta,
  .drawer-link-sub,
  .drawer-retry-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border-radius: var(--radius-pill);
    font-size: 10px;
    font-weight: 700;
    padding: 2px 8px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--accent-text);
  }

  .drawer-link-cta {
    background: rgba(248, 113, 113, 0.16);
    border-color: rgba(248, 113, 113, 0.42);
    color: var(--error-text);
  }

  .skins-loading,
  .skins-error,
  .skins-empty {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 10px;
    border-radius: var(--radius-sm);
    background: var(--surface);
    border: 1px solid var(--border);
    font-size: 11px;
    color: var(--text-muted);
  }

  .skins-loading {
    justify-content: flex-start;
  }

  .skins-error {
    color: var(--error-text);
    border-color: rgba(248, 113, 113, 0.35);
  }

  .skins-error-actions {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .skin-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 7px;
  }

  .skin-grid-wide {
    grid-template-columns: repeat(auto-fill, minmax(156px, 1fr));
  }

  .skin-tile {
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-sm);
    background: var(--surface);
    border: 1px solid var(--border);
    overflow: hidden;
  }

  .skin-stage {
    position: relative;
    height: 76px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 6px;
    background: radial-gradient(circle at 50% 45%, rgba(129, 140, 248, 0.18), rgba(11, 14, 26, 0.55) 75%);
    border-bottom: 1px solid var(--border);
  }

  :global(:root[data-theme="light"]) .skin-stage {
    background: radial-gradient(circle at 50% 45%, rgba(99, 102, 241, 0.12), #edf2fb 78%);
  }

  :global(:root[data-theme="light"]) .skin-time-chip {
    background: rgba(255, 255, 255, 0.92);
  }

  .skin-img {
    max-width: 100%;
    max-height: 62px;
    object-fit: contain;
    border-radius: 6px;
  }

  .skin-time-chip {
    position: absolute;
    right: 5px;
    bottom: 5px;
    padding: 1px 5px;
    border-radius: var(--radius-pill);
    background: rgba(11, 14, 26, 0.86);
    border: 1px solid var(--border);
    color: var(--text);
    font-size: 10px;
    font-weight: 700;
  }

  .skin-meta {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 7px;
    flex: 1;
  }

  .skin-name {
    font-size: 11px;
    font-weight: 700;
    color: var(--text);
    line-height: 1.25;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .skin-sub {
    font-size: 10px;
    color: var(--text-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skin-prog-row {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .skin-status {
    font-size: 10px;
    color: var(--text-muted);
  }

  .body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 7px 10px;
    border-top: 1px solid var(--border);
  }

  .unhydrated-stage {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 10px;
    border: none;
    border-top: 1px solid var(--border);
    background: rgba(255, 255, 255, 0.015);
    color: var(--text-muted);
    font-size: 11px;
    text-align: left;
  }

  :global(:root[data-theme="light"]) .unhydrated-stage {
    background: rgba(91, 108, 243, 0.03);
  }

  .unhydrated-stage:hover {
    background: rgba(129, 140, 248, 0.08);
    color: var(--text);
  }

  .unhydrated-ico {
    width: 24px;
    height: 24px;
    border-radius: 7px;
    background: var(--accent-tint);
    color: var(--accent-text);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .unhydrated-txt {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .unhydrated-cta {
    font-size: 10px;
    font-weight: 700;
    color: var(--accent-text);
    flex-shrink: 0;
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
  .compact-wrap {
    display: flex;
    flex-direction: column;
    padding: 0;
    overflow: hidden;
    border-radius: var(--radius-md);
  }

  .compact-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
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
    gap: 3px;
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

  .compact-title-btn {
    min-width: 0;
    border: none;
    background: transparent;
    padding: 0;
    text-align: left;
  }

  .compact-title {
    display: block;
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
