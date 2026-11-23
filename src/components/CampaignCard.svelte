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
  import { slowNowMs } from '../lib/clock';
  import {
    formatEndCountdown,
    formatExactDateTime,
    formatMinutes,
    formatStartsIn,
    progressPercent,
  } from '../lib/format';
  import { getCampaignTotalMinutes } from '../lib/selectors';
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

  const linkState = $derived.by<'no_link' | 'connected' | 'unlinked'>(() => {
    if (!campaign.requiresLink) return 'no_link';
    if (campaign.linked) return 'connected';
    return 'unlinked';
  });

  const connectTargetUrl = $derived(
    campaign.linkUrl ||
      (campaign.platform === 'twitch'
        ? 'https://www.twitch.tv/settings/connections'
        : 'https://kick.com/drops/inventory'),
  );

  const allClaimed = $derived(
    campaign.drops.length > 0 && campaign.drops.every((d) => d.claimed),
  );
  const claimedCount = $derived(campaign.drops.filter((d) => d.claimed).length);
  const totalMinutes = $derived(getCampaignTotalMinutes(campaign));
  const nextUnclaimedDrop = $derived.by(() => {
    const unclaimed = campaign.drops.filter((d) => !d.claimed);
    if (unclaimed.length === 0) return null;
    return [...unclaimed].sort(
      (a, b) =>
        Math.max(0, a.requiredMinutes - a.currentMinutes) -
        Math.max(0, b.requiredMinutes - b.currentMinutes),
    )[0];
  });
  const wholeRemainingMinutes = $derived.by(() => {
    const unclaimed = campaign.drops.filter((d) => !d.claimed);
    if (unclaimed.length === 0) return 0;
    return Math.max(...unclaimed.map((d) => Math.max(0, d.requiredMinutes - d.currentMinutes)));
  });
  const endInfo = $derived(formatEndCountdown(campaign.endsAt, $slowNowMs));
  const exactEnd = $derived(formatExactDateTime(campaign.endsAt));
  const upcomingLabel = $derived(formatStartsIn(campaign.startsAt, $slowNowMs));
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
    if (campaign.drops.length === 0 || totalMinutes <= 0) return null;
    const watched = Math.max(0, totalMinutes - wholeRemainingMinutes);
    return progressPercent(watched, totalMinutes);
  });

  // Console uses dense single-row layout for maximum visibility
  const useCompact = $derived(compact || true);

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
</script>

{#snippet connectionBadge()}
  {#if linkState === 'no_link'}
    <span class="conn-pill conn-ready" title="No external game account link required">
      <CircleCheck size={11} />
      <span>Ready</span>
    </span>
  {:else if linkState === 'connected'}
    <span class="conn-pill conn-connected" title="Your game account is connected">
      <Link2 size={11} />
      <span>Linked</span>
    </span>
  {:else}
    <button
      type="button"
      class="conn-pill conn-unlinked conn-clickable"
      title="Game account not connected — click to connect"
      onclick={() => onOpenUrl(connectTargetUrl)}
    >
      <Unlink size={11} />
      <span>Not linked</span>
      <ExternalLink size={9} />
    </button>
  {/if}
{/snippet}

{#snippet skinsDrawer()}
  <div class="skins-drawer" class:skins-drawer-wide={useCompact}>
    <div class="skins-drawer-head">
      <div class="skins-drawer-title">
        <Gift size={13} />
        <span>Rewards</span>
        {#if skinTiles.length > 0}
          <Badge>{skinTiles.length}</Badge>
        {/if}
      </div>

      {#if linkState === 'unlinked' && campaign.linkUrl}
        <button type="button" class="drawer-link-cta" onclick={() => onOpenUrl(campaign.linkUrl!)}>
          <Unlink size={11} />
          <span>Link game account</span>
        </button>
      {:else if campaign.linkUrl}
        <button type="button" class="drawer-link-sub" onclick={() => onOpenUrl(campaign.linkUrl!)}>
          <span>Manage link</span>
          <ExternalLink size={10} />
        </button>
      {/if}
    </div>

    {#if loadingDetails}
      <div class="skins-loading">
        <span class="spin-wrap"><LoaderCircle size={14} /></span>
        <span>Loading rewards from Twitch...</span>
      </div>
    {:else if detailsError}
      <div class="skins-error">
        <span>{detailsError}</span>
        <div class="skins-error-actions">
          <button type="button" class="drawer-retry-btn" onclick={ensureDetailsLoaded}>Retry</button>
          {#if campaign.linkUrl}
            <button type="button" class="drawer-link-sub" onclick={() => onOpenUrl(campaign.linkUrl!)}>
              Open link
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
      <div class="skin-grid" class:skin-grid-wide={useCompact}>
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
                <Monogram src={null} name={tile.skinName} width={44} height={44} />
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
                    <button type="button" class="badge-manual" onclick={onOpenInventory}>Claim</button>
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

{#if useCompact}
  <article
    class="card card-interactive cv-compact compact-wrap design-console"
    class:farming-active={activeFarming}
    class:excluded={isExcluded}
    class:skins-expanded={skinsOpen}
  >
    <div class="compact-row">

      <div class="compact-info">
        <div class="compact-top">
          {#if showPlatformBadge}
            <span
              class="plat-dot dot-{campaign.platform}"
              title={campaign.platform === 'twitch' ? 'Twitch' : 'Kick'}
            ></span>
          {/if}
          <span class="compact-game">{campaign.game.name}</span>
          <span class="compact-sep" aria-hidden="true">/</span>
          <button
            type="button"
            class="compact-title-btn"
            aria-expanded={skinsOpen}
            onclick={toggleSkins}
            title="Click to inspect rewards"
          >
            <span class="compact-title">{campaign.name}</span>
          </button>
        </div>

        <div class="compact-sub tabular">
          {@render connectionBadge()}
          {#if campaign.drops.length > 0}
            <span>
              {claimedCount > 0
                ? `${claimedCount}/${campaign.drops.length} drops`
                : `${campaign.drops.length} ${campaign.drops.length === 1 ? 'drop' : 'drops'}`}
              · {formatMinutes(totalMinutes)}
            </span>
            {#if overallPct !== null && overallPct > 0}
              <span class="compact-pct">· {overallPct}%</span>
            {/if}
          {:else}
            <span>Inspect rewards</span>
          {/if}
          {#if allClaimed}
            <Badge tone="success"><Check size={10} /> Done</Badge>
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
          title={skinsOpen ? 'Hide reward preview' : 'Preview drop rewards'}
        >
          <Gift size={11} />
          <span>{skinTiles.length > 0 ? `${skinTiles.length}` : 'Rewards'}</span>
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
          class="farm-btn compact-farm"
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
          <Star size={12} />
        </button>

        {#if campaign.linkUrl}
          <button
            type="button"
            class="icon-btn"
            aria-label={`Open link page for ${campaign.game.name}`}
            title="Open game account link page"
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
    </div>

    {#if skinsOpen}
      {@render skinsDrawer()}
    {/if}
  </article>
{:else}
  <article
    class="card card-interactive cv-card campaign-card design-console"
    class:farming-active={activeFarming}
    class:excluded={isExcluded}
    class:skins-expanded={skinsOpen}
  >
    <div class="banner">
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
          title={skinsOpen ? 'Collapse reward preview' : 'Inspect rewards in this drop'}
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
            <span>
              {campaign.drops.length > 0
                ? (claimedCount > 0 ? `${claimedCount}/${campaign.drops.length} drops` : `${campaign.drops.length} ${campaign.drops.length === 1 ? 'drop' : 'drops'}`)
                : 'Rewards'}
            </span>
          </button>
          {#if campaign.drops.length > 0}
            <span class="sum-time">{formatMinutes(totalMinutes)}</span>
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
        {#if campaign.drops.length > 1 && !allClaimed}
          <div class="multi-eta-bar tabular">
            {#if nextUnclaimedDrop}
              {@const nextRem = Math.max(0, nextUnclaimedDrop.requiredMinutes - nextUnclaimedDrop.currentMinutes)}
              <span class="eta-pair">
                <span class="eta-k">Next:</span>
                <strong>{nextRem === 0 ? 'Ready' : `${formatMinutes(nextRem)}`}</strong>
              </span>
            {/if}
            <span class="eta-pair">
              <span class="eta-k">Total ({overallPct ?? 0}%):</span>
              <strong>{wholeRemainingMinutes === 0 ? 'Done' : `${formatMinutes(wholeRemainingMinutes)} left`}</strong>
            </span>
          </div>
        {/if}

        <div class="drops-list">
          {#each visibleDrops as drop (drop.id)}
            {@const pct = drop.claimed ? 100 : progressPercent(drop.currentMinutes, drop.requiredMinutes)}
            {@const rem = Math.max(0, drop.requiredMinutes - drop.currentMinutes)}
            {@const displayName = drop.rewards[0]?.name || drop.name}
            <div class="drop-row">
              <Monogram
                src={drop.rewards[0]?.imageUrl}
                name={displayName}
                width={24}
                height={24}
              />
              <div class="drop-info">
                <div class="drop-title-row">
                  <span class="drop-name" title={displayName}>{displayName}</span>
                  <span class="drop-stat tabular">
                    {#if drop.claimed}
                      <Badge tone="success"><Check size={10} /> Claimed</Badge>
                    {:else if drop.needsManualClaim}
                      <button type="button" class="badge-manual" onclick={onOpenInventory}>
                        Claim
                      </button>
                    {:else if drop.claimable}
                      <Badge tone="info">Ready</Badge>
                    {:else}
                      <span>{pct}% · {formatMinutes(rem)}</span>
                    {/if}
                  </span>
                </div>
                <ProgressBar
                  value={drop.claimed ? drop.requiredMinutes : drop.currentMinutes}
                  max={drop.requiredMinutes}
                  platform={campaign.platform}
                  label={displayName}
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
        <Gift size={13} />
        <span class="unhydrated-txt">Inspect reward details</span>
        <ChevronDown size={13} />
      </button>
    {/if}

    <footer class="card-footer">
      <button
        type="button"
        class="farm-btn"
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
            title="Open game account link page"
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
    border-color: var(--accent);
  }

  .campaign-card.excluded,
  .compact-wrap.excluded {
    opacity: 0.56;
  }

  .banner {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
  }

  .meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .top-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }

  .game-wrap {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    flex: 1;
  }

  .game-name {
    font-size: 11.5px;
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
    font-family: var(--font-display);
    font-size: 14px;
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
    gap: 6px;
    font-size: 11px;
    color: var(--text-muted);
  }

  .sum-time {
    color: var(--text-faint);
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

  .conn-unlinked {
    background: var(--error-tint);
    color: var(--error-text);
  }

  .conn-clickable:hover:not(:disabled) {
    border-color: var(--error);
  }

  .skins-pill-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 7px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text-muted);
    font-size: 10.5px;
    font-weight: 600;
    line-height: 1.45;
    white-space: nowrap;
  }

  .skins-pill-btn:hover,
  .skins-pill-btn.active {
    background: var(--accent-tint);
    border-color: var(--accent);
    color: var(--accent-text);
  }

  .spin-wrap {
    display: inline-flex;
    animation: cc-spin 1s linear infinite;
  }

  @keyframes cc-spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .skins-drawer {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
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
    gap: 6px;
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
    border-radius: var(--radius-xs);
    font-size: 11px;
    font-weight: 600;
    padding: 2px 8px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
  }

  .drawer-link-cta {
    background: var(--error-tint);
    border-color: var(--error);
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
    font-size: 11.5px;
    color: var(--text-muted);
  }

  .skins-loading { justify-content: flex-start; }
  .skins-error { color: var(--error-text); border-color: var(--error); }
  .skins-error-actions { display: inline-flex; align-items: center; gap: 6px; }

  .skin-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
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
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
  }

  .skin-img {
    max-width: 100%;
    max-height: 62px;
    object-fit: contain;
    border-radius: var(--radius-xs);
  }

  .skin-time-chip {
    position: absolute;
    right: 5px;
    bottom: 5px;
    padding: 1px 5px;
    border-radius: var(--radius-xs);
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text);
    font-size: 10px;
    font-weight: 600;
  }

  .skin-meta {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 7px 8px;
    flex: 1;
  }

  .skin-name {
    font-size: 11.5px;
    font-weight: 600;
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
    font-size: 10.5px;
    color: var(--text-muted);
  }

  .body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
  }

  .unhydrated-stage {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 10px 14px;
    border: none;
    border-top: 1px solid var(--border);
    background: transparent;
    color: var(--text-muted);
    font-size: 12px;
    text-align: left;
  }

  .unhydrated-stage:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .unhydrated-txt {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .drops-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .drop-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .drop-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
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
    font-size: 11.5px;
    font-weight: 600;
    padding: 2px 0;
  }

  .expand-btn:hover {
    text-decoration: underline;
  }

  .multi-eta-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 5px 8px;
    border-radius: var(--radius-xs);
    background: var(--surface-2);
    border: 1px solid var(--border);
    font-size: 11px;
  }

  .eta-pair {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--text);
  }

  .eta-k {
    color: var(--text-faint);
  }

  .badge-manual {
    background: var(--warning-tint);
    color: var(--warning-text);
    border: none;
    border-radius: var(--radius-xs);
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
    padding: 8px 14px;
    background: var(--surface-2);
    border-top: 1px solid var(--border);
  }

  .farm-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 28px;
    padding: 0 11px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    font-size: 12px;
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

  .footer-right {
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    height: 26px;
    min-width: 26px;
    padding: 0 5px;
    border-radius: var(--radius-xs);
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
    background: var(--accent-tint);
    color: var(--accent-text);
  }

  .star-lbl {
    font-size: 11px;
  }

  /* Compact / Console table-row layout */
  .compact-wrap {
    container-type: inline-size;
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
    min-width: 0;
  }

  .design-console .compact-row {
    padding: 6px 10px;
    gap: 8px;
  }

  .compact-info {
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .design-console .compact-info {
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .compact-top {
    flex: 1 1 0;
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
  }

  .compact-game {
    flex: 0 1 auto;
    min-width: 0;
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .compact-sep {
    flex-shrink: 0;
    color: var(--text-faint);
    font-size: var(--text-xs);
  }

  .compact-title-btn {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    border: none;
    background: transparent;
    padding: 0;
    text-align: left;
  }

  .compact-title {
    display: block;
    font-size: var(--text-sm);
    font-weight: 600;
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
    flex-shrink: 0;
    max-width: 100%;
  }

  .compact-pct {
    color: var(--accent-text);
    font-weight: 600;
  }

  .compact-right {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-shrink: 0;
  }

  .compact-farm {
    height: 24px;
    padding: 0 9px;
    font-size: 11px;
  }

  @container (max-width: 780px) {
    .design-console .compact-info {
      flex-direction: column;
      align-items: flex-start;
      gap: 4px;
    }

    .compact-top {
      flex: 0 0 auto;
      width: 100%;
    }
  }

  @container (max-width: 580px) {
    .compact-row {
      flex-wrap: wrap;
      row-gap: 6px;
    }

    .compact-info {
      flex: 1 1 100%;
    }

    .compact-right {
      width: 100%;
      flex-wrap: wrap;
      justify-content: flex-end;
      padding-top: 5px;
      border-top: 1px solid var(--border);
    }
  }
</style>
