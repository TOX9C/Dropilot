<script lang="ts">
  import Check from 'lucide-svelte/icons/check';
  import CircleCheck from 'lucide-svelte/icons/circle-check';
  import Clock from 'lucide-svelte/icons/clock';
  import ExternalLink from 'lucide-svelte/icons/external-link';
  import Link2 from 'lucide-svelte/icons/link-2';
  import LoaderCircle from 'lucide-svelte/icons/loader-circle';
  import Maximize2 from 'lucide-svelte/icons/maximize-2';
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
    upgradeArtworkUrl,
  } from '../lib/format';
  import {
    getCampaignTotalMinutes,
    isCampaignCompleted,
    isCampaignFarmed,
    type GameDropGroup,
  } from '../lib/selectors';
  import { openLightbox } from '../lib/stores';
  import type { Campaign, PlatformId, Settings } from '../lib/types';
  import Badge from './Badge.svelte';
  import Button from './Button.svelte';
  import GameDropCard from './GameDropCard.svelte';
  import Monogram from './Monogram.svelte';
  import ProgressBar from './ProgressBar.svelte';

  interface Props {
    groups: GameDropGroup[];
    settings: Settings;
    showPlatformBadge?: boolean;
    onToggleGameFarm: (group: GameDropGroup, farm: boolean) => void;
    onToggleCampaignFarm: (camp: Campaign, farm: boolean) => void;
    onExcludeCampaign: (camp: Campaign) => void;
    onOpenUrl: (url: string) => void;
    onOpenInventory: (platform: PlatformId) => void;
    onFetchDetails?: (camp: Campaign) => Promise<void>;
  }

  let {
    groups,
    settings,
    showPlatformBadge = false,
    onToggleGameFarm,
    onToggleCampaignFarm,
    onExcludeCampaign,
    onOpenUrl,
    onOpenInventory,
    onFetchDetails,
  }: Props = $props();

  let gridEl = $state<HTMLDivElement | null>(null);
  let colCount = $state(3);
  let expandedKey = $state<string | null>(null);
  let loadingIds = $state<Record<string, boolean>>({});
  let errorIds = $state<Record<string, string>>({});
  let brokenImages = $state<Record<string, boolean>>({});

  function measureCols(el: HTMLElement) {
    const raw = getComputedStyle(el).gridTemplateColumns;
    if (!raw || raw === 'none') return;
    const tracks = raw.trim().split(/\s+/).filter(Boolean);
    if (tracks.length > 0) {
      colCount = tracks.length;
    }
  }

  $effect(() => {
    const el = gridEl;
    if (!el) return;
    measureCols(el);
    const ro = new ResizeObserver(() => measureCols(el));
    ro.observe(el);
    return () => ro.disconnect();
  });

  // Reset expansion if the expanded game disappears under new filters.
  $effect(() => {
    if (expandedKey && !groups.some((g) => g.key === expandedKey)) {
      expandedKey = null;
    }
  });

  const expandedIndex = $derived(
    expandedKey ? groups.findIndex((g) => g.key === expandedKey) : -1,
  );
  const expandedGroup = $derived(expandedIndex >= 0 ? groups[expandedIndex] : null);
  const expandedRowEndIndex = $derived.by(() => {
    if (expandedIndex < 0) return -1;
    const cols = Math.max(1, colCount);
    const row = Math.floor(expandedIndex / cols);
    return Math.min(groups.length - 1, (row + 1) * cols - 1);
  });

  async function hydrateCampaign(camp: Campaign) {
    if (!onFetchDetails || loadingIds[camp.id]) return;
    if (camp.platform !== 'twitch') return;
    if (camp.linkVerified && camp.drops.length > 0) return;

    loadingIds = { ...loadingIds, [camp.id]: true };
    const nextErrs = { ...errorIds };
    delete nextErrs[camp.id];
    errorIds = nextErrs;
    try {
      await onFetchDetails(camp);
    } catch (err) {
      errorIds = { ...errorIds, [camp.id]: String(err) };
    } finally {
      const nextLoading = { ...loadingIds };
      delete nextLoading[camp.id];
      loadingIds = nextLoading;
    }
  }

  // Automatically hydrate Twitch campaign reward details whenever a game card is expanded.
  $effect(() => {
    const grp = expandedGroup;
    if (!grp || !onFetchDetails) return;
    for (const camp of grp.campaigns) {
      if (
        camp.platform === 'twitch' &&
        camp.drops.length === 0 &&
        !loadingIds[camp.id] &&
        !errorIds[camp.id]
      ) {
        void hydrateCampaign(camp);
      }
    }
  });

  function toggleExpand(key: string) {
    expandedKey = expandedKey === key ? null : key;
  }

  function campOverallPct(camp: Campaign): number | null {
    const total = getCampaignTotalMinutes(camp);
    if (camp.drops.length === 0 || total <= 0) return null;
    const unclaimed = camp.drops.filter((d) => !d.claimed);
    const rem =
      unclaimed.length === 0
        ? 0
        : Math.max(...unclaimed.map((d) => Math.max(0, d.requiredMinutes - d.currentMinutes)));
    return progressPercent(Math.max(0, total - rem), total);
  }

  function campWatchProgress(camp: Campaign): { watched: number; total: number } {
    const total = getCampaignTotalMinutes(camp);
    if (camp.drops.length === 0 || total <= 0) return { watched: 0, total: 0 };
    const unclaimed = camp.drops.filter((d) => !d.claimed);
    const rem =
      unclaimed.length === 0
        ? 0
        : Math.max(...unclaimed.map((d) => Math.max(0, d.requiredMinutes - d.currentMinutes)));
    return { watched: Math.max(0, total - rem), total };
  }

  function campaignTiles(camp: Campaign) {
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
    for (const drop of camp.drops) {
      const rewards =
        drop.rewards.length > 0 ? drop.rewards : [{ name: drop.name, imageUrl: null }];
      rewards.forEach((reward, idx) => {
        tiles.push({
          key: `${camp.id}:${drop.id}-${idx}`,
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
  }
</script>

<div class="game-grid" bind:this={gridEl}>
  {#each groups as group, idx (group.key)}
    <GameDropCard
      {group}
      expanded={expandedKey === group.key}
      {showPlatformBadge}
      onToggleExpand={() => toggleExpand(group.key)}
      onToggleGameFarm={(farm) => onToggleGameFarm(group, farm)}
      {onOpenUrl}
    />

    {#if expandedGroup && idx === expandedRowEndIndex}
      {@const headerThumb = upgradeArtworkUrl(expandedGroup.game.boxArtUrl, 420, 560)}
      {@const headerFull = upgradeArtworkUrl(expandedGroup.game.boxArtUrl, 600, 800)}
      {@const headerImgKey = `header:${expandedGroup.key}`}
      <section
        class="card game-detail-panel"
        aria-label={`${expandedGroup.game.name} drop campaigns and rewards`}
      >
        <header class="detail-head">
          <div class="detail-identity">
            {#if headerThumb && !brokenImages[headerImgKey]}
              <button
                type="button"
                class="detail-art-btn"
                title="Click to view full artwork"
                aria-label={`View full artwork for ${expandedGroup.game.name}`}
                onclick={() =>
                  openLightbox({
                    src: headerFull!,
                    title: expandedGroup.game.name,
                    subtitle: `${expandedGroup.platform === 'twitch' ? 'Twitch' : 'Kick'} · ${expandedGroup.campaigns.length} ${expandedGroup.campaigns.length === 1 ? 'campaign' : 'campaigns'}`,
                  })}
              >
                <img
                  src={headerThumb}
                  alt={expandedGroup.game.name}
                  class="detail-art-img"
                  decoding="async"
                  onerror={() => {
                    brokenImages = { ...brokenImages, [headerImgKey]: true };
                  }}
                />
                <span class="detail-art-zoom" aria-hidden="true">
                  <Maximize2 size={10} />
                </span>
              </button>
            {:else}
              <Monogram
                src={null}
                name={expandedGroup.game.name}
                width={45}
                height={60}
              />
            {/if}
            <div class="detail-title-col">
              <div class="detail-title-row">
                {#if showPlatformBadge}
                  <span
                    class="plat-dot dot-{expandedGroup.platform}"
                    title={expandedGroup.platform === 'twitch' ? 'Twitch' : 'Kick'}
                  ></span>
                {/if}
                <h2>{expandedGroup.game.name}</h2>
                <Badge tone="neutral">
                  {expandedGroup.campaigns.length}
                  {expandedGroup.campaigns.length === 1 ? 'campaign' : 'campaigns'}
                </Badge>
                {#if expandedGroup.totalDrops > 0}
                  <Badge tone="neutral">
                    {expandedGroup.claimedDrops > 0
                      ? `${expandedGroup.claimedDrops}/${expandedGroup.totalDrops} claimed`
                      : `${expandedGroup.totalDrops} rewards`}
                  </Badge>
                {/if}
              </div>
              <p class="detail-sub">
                {expandedGroup.platform === 'twitch' ? 'Twitch Drops' : 'Kick Drops'}
                · Prioritize the entire game or manage individual campaigns below.
              </p>
            </div>
          </div>

          <div class="detail-head-actions">
            <Button
              variant={expandedGroup.isFarming ? 'primary' : 'secondary'}
              size="sm"
              onclick={() => onToggleGameFarm(expandedGroup, !expandedGroup.isFarming)}
            >
              {#if expandedGroup.isFarming}
                <Star size={12} />
                <span>Farming game</span>
              {:else}
                <Play size={12} />
                <span>Farm all {expandedGroup.game.name}</span>
              {/if}
            </Button>
            <button
              type="button"
              class="close-detail-btn"
              aria-label={`Close ${expandedGroup.game.name} details`}
              onclick={() => (expandedKey = null)}
            >
              <X size={14} />
            </button>
          </div>
        </header>

        <div class="detail-campaigns">
          {#each expandedGroup.campaigns as camp (camp.id)}
            {@const isExcluded = settings[camp.platform].excludedCampaigns.includes(camp.id)}
            {@const activeFarming = isCampaignFarmed(camp, settings) && !isExcluded}
            {@const allClaimed = isCampaignCompleted(camp)}
            {@const endInfo = formatEndCountdown(camp.endsAt, $slowNowMs)}
            {@const exactEnd = formatExactDateTime(camp.endsAt)}
            {@const campProg = campWatchProgress(camp)}
            {@const pct = campOverallPct(camp)}
            {@const tiles = campaignTiles(camp)}
            {@const isLoading = Boolean(loadingIds[camp.id])}
            {@const loadErr = errorIds[camp.id]}
            {@const connectUrl =
              camp.linkUrl ||
              (camp.platform === 'twitch'
                ? 'https://www.twitch.tv/settings/connections'
                : 'https://kick.com/drops/inventory')}

            <div
              class="camp-block"
              class:camp-farming={activeFarming}
              class:camp-excluded={isExcluded}
            >
              <div class="camp-block-head">
                <div class="camp-block-info">
                  <div class="camp-block-title-row">
                    <h3>{camp.name}</h3>
                    {#if camp.status === 'upcoming'}
                      <Badge tone="info">
                        <Clock size={10} />
                        <span>{formatStartsIn(camp.startsAt, $slowNowMs)}</span>
                      </Badge>
                    {:else}
                      <span title={`Ends ${exactEnd}`}>
                        <Badge tone={endInfo.tone}>
                          <Clock size={10} />
                          <span>{endInfo.ended ? 'Ended' : `Ends in ${endInfo.text}`}</span>
                        </Badge>
                      </span>
                    {/if}
                    {#if allClaimed}
                      <Badge tone="success"><Check size={10} /> Done</Badge>
                    {/if}
                  </div>

                  <div class="camp-block-meta tabular">
                    {#if !camp.requiresLink}
                      <span class="conn-pill conn-ready" title="No external game account link required">
                        <CircleCheck size={11} />
                        <span>Ready</span>
                      </span>
                    {:else if camp.linked}
                      <span class="conn-pill conn-connected" title="Your game account is connected">
                        <Link2 size={11} />
                        <span>Linked</span>
                      </span>
                    {:else}
                      <button
                        type="button"
                        class="conn-pill conn-unlinked conn-clickable"
                        title="Game account not linked — click to connect"
                        onclick={() => onOpenUrl(connectUrl)}
                      >
                        <Unlink size={11} />
                        <span>Not linked</span>
                        <ExternalLink size={9} />
                      </button>
                    {/if}

                    {#if campProg.total > 0}
                      <span>
                        {campProg.watched}m / {formatMinutes(campProg.total)}
                        {#if pct !== null}
                          · <strong>{pct}%</strong>
                        {/if}
                      </span>
                    {/if}
                    {#if camp.linkUrl}
                      <button
                        type="button"
                        class="inline-link-btn"
                        onclick={() => onOpenUrl(camp.linkUrl!)}
                      >
                        <span>Manage link</span>
                        <ExternalLink size={10} />
                      </button>
                    {/if}
                  </div>
                </div>

                <div class="camp-block-actions">
                  <button
                    type="button"
                    class="camp-farm-btn"
                    class:selected={activeFarming}
                    aria-pressed={activeFarming}
                    onclick={() => onToggleCampaignFarm(camp, !activeFarming)}
                  >
                    {#if activeFarming}
                      <Check size={12} />
                      <span>Farming</span>
                    {:else}
                      <Play size={11} />
                      <span>{isExcluded ? 'Include' : 'Farm'}</span>
                    {/if}
                  </button>

                  {#if !isExcluded}
                    <button
                      type="button"
                      class="camp-excl-btn"
                      title="Exclude this campaign"
                      aria-label={`Exclude ${camp.name}`}
                      onclick={() => onExcludeCampaign(camp)}
                    >
                      <X size={12} />
                    </button>
                  {/if}
                </div>
              </div>

              {#if campProg.total > 0}
                <ProgressBar
                  value={campProg.watched}
                  max={campProg.total}
                  platform={camp.platform}
                  label={camp.name}
                />
              {/if}

              {#if isLoading}
                <div class="rewards-state">
                  <span class="spin-wrap"><LoaderCircle size={14} /></span>
                  <span>Loading rewards from {camp.platform === 'twitch' ? 'Twitch' : 'Kick'}...</span>
                </div>
              {:else if loadErr}
                <div class="rewards-state rewards-err">
                  <span>{loadErr}</span>
                  <button
                    type="button"
                    class="retry-btn"
                    onclick={() => void hydrateCampaign(camp)}
                  >
                    Retry
                  </button>
                </div>
              {:else if tiles.length === 0}
                <div class="rewards-state">
                  <span>No reward previews listed for this campaign yet.</span>
                  {#if onFetchDetails && camp.platform === 'twitch'}
                    <button
                      type="button"
                      class="retry-btn"
                      onclick={() => void hydrateCampaign(camp)}
                    >
                      Load rewards
                    </button>
                  {/if}
                </div>
              {:else}
                <div class="rewards-grid">
                  {#each tiles as tile (tile.key)}
                    {@const tilePct = tile.claimed
                      ? 100
                      : progressPercent(tile.currentMinutes, tile.requiredMinutes)}
                    {@const showImg = Boolean(tile.imageUrl && !brokenImages[tile.key])}
                    <div class="reward-tile">
                      {#if showImg}
                        <button
                          type="button"
                          class="reward-stage reward-stage-btn"
                          title="Click to view full artwork"
                          aria-label={`View full artwork for ${tile.skinName}`}
                          onclick={() =>
                            openLightbox({
                              src: tile.imageUrl!,
                              title: tile.skinName,
                              subtitle: `${expandedGroup.game.name} · ${camp.name} · ${formatMinutes(tile.requiredMinutes)}`,
                            })}
                        >
                          <img
                            class="reward-img"
                            src={tile.imageUrl!}
                            alt={tile.skinName}
                            loading="lazy"
                            decoding="async"
                            onerror={() => {
                              brokenImages = { ...brokenImages, [tile.key]: true };
                            }}
                          />
                          <span class="stage-zoom-badge" aria-hidden="true">
                            <Maximize2 size={11} />
                          </span>
                          <span class="reward-time tabular">{formatMinutes(tile.requiredMinutes)}</span>
                        </button>
                      {:else}
                        <div class="reward-stage">
                          <Monogram src={null} name={tile.skinName} width={44} height={44} />
                          <span class="reward-time tabular">{formatMinutes(tile.requiredMinutes)}</span>
                        </div>
                      {/if}

                      <div class="reward-meta">
                        <div class="reward-name" title={tile.skinName}>{tile.skinName}</div>
                        <div class="reward-prog tabular">
                          <ProgressBar
                            value={tile.claimed ? tile.requiredMinutes : tile.currentMinutes}
                            max={tile.requiredMinutes}
                            platform={camp.platform}
                            label={tile.skinName}
                          />
                          <div class="reward-status">
                            {#if tile.claimed}
                              <Badge tone="success"><Check size={10} /> Claimed</Badge>
                            {:else if tile.needsManualClaim}
                              <button
                                type="button"
                                class="badge-manual"
                                onclick={() => onOpenInventory(camp.platform)}
                              >
                                Claim
                              </button>
                            {:else if tile.claimable}
                              <Badge tone="info">Ready</Badge>
                            {:else}
                              <span>{tilePct}% · {tile.currentMinutes}/{tile.requiredMinutes}m</span>
                            {/if}
                          </div>
                        </div>
                      </div>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      </section>
    {/if}
  {/each}
</div>

<style>
  .game-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(268px, 1fr));
    gap: var(--grid-gap, 10px);
    align-items: stretch;
  }

  .game-detail-panel {
    grid-column: 1 / -1;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    border-color: var(--accent);
    background: var(--surface);
  }

  .detail-head {
    display: flex;
    flex-wrap: nowrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding-bottom: 10px;
    border-bottom: 1px solid var(--border);
  }

  .detail-identity {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    flex: 1;
  }

  .detail-art-btn {
    position: relative;
    width: 45px;
    height: 60px;
    padding: 0;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--sidebar-bg);
    overflow: hidden;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .detail-art-btn:hover,
  .detail-art-btn:focus-visible {
    border-color: var(--accent);
  }

  .detail-art-img {
    width: 100%;
    height: 100%;
    object-fit: contain;
    display: block;
  }

  .detail-art-zoom {
    position: absolute;
    right: 2px;
    bottom: 2px;
    width: 16px;
    height: 16px;
    border-radius: var(--radius-xs);
    background: rgba(9, 12, 16, 0.88);
    border: 1px solid var(--border);
    color: var(--accent-text);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    pointer-events: none;
  }

  .detail-art-btn:hover .detail-art-zoom,
  .detail-art-btn:focus-visible .detail-art-zoom {
    opacity: 1;
  }

  .detail-title-col {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  .detail-title-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .detail-title-row h2 {
    font-size: var(--text-lg);
  }

  .detail-sub {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .detail-head-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .close-detail-btn {
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text-muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .close-detail-btn:hover {
    border-color: var(--border-hover);
    color: var(--text);
    background: var(--surface-hover);
  }

  .detail-campaigns {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
    gap: 10px;
    align-items: stretch;
  }

  @media (max-width: 760px) {
    .detail-head {
      flex-wrap: wrap;
    }

    .detail-campaigns {
      grid-template-columns: 1fr;
    }
  }

  .camp-block {
    display: flex;
    flex-direction: column;
    gap: 9px;
    min-width: 0;
    height: 100%;
    padding: 12px;
    border-radius: var(--radius-md);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }

  .camp-block.camp-farming {
    border-color: var(--accent);
  }

  .camp-block.camp-excluded {
    opacity: 0.6;
  }

  .camp-block-head {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
  }

  .camp-block-info {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
    flex: 1;
  }

  .camp-block-title-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 7px;
  }

  .camp-block-title-row h3 {
    font-size: var(--text-md);
    color: var(--text);
  }

  .camp-block-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: var(--text-muted);
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
    background: var(--surface);
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

  .conn-clickable:hover {
    border-color: currentColor;
  }

  .inline-link-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 7px;
    border-radius: var(--radius-xs);
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 600;
  }

  .inline-link-btn:hover {
    border-color: var(--border-hover);
    color: var(--text);
  }

  .camp-block-actions {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-shrink: 0;
  }

  .camp-farm-btn {
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
  }

  .camp-farm-btn:hover {
    border-color: var(--accent);
    color: var(--accent-text);
  }

  .camp-farm-btn.selected {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-on);
  }

  .camp-excl-btn {
    width: 26px;
    height: 26px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text-muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .camp-excl-btn:hover {
    border-color: var(--border-hover);
    color: var(--text);
  }

  .rewards-state {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 9px 11px;
    border-radius: var(--radius-sm);
    background: var(--surface);
    border: 1px solid var(--border);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .rewards-err {
    border-color: var(--error);
    color: var(--error-text);
  }

  .spin-wrap {
    display: inline-flex;
    animation: gdg-spin 1s linear infinite;
  }

  @keyframes gdg-spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .retry-btn {
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text);
    border-radius: var(--radius-xs);
    padding: 2px 8px;
    font-size: 11px;
    font-weight: 600;
  }

  .rewards-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 8px;
  }

  .reward-tile {
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-sm);
    background: var(--surface);
    border: 1px solid var(--border);
    overflow: hidden;
  }

  .reward-stage {
    position: relative;
    width: 100%;
    height: 112px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 8px;
    background: var(--surface-2);
    border: none;
    border-bottom: 1px solid var(--border);
  }

  .reward-stage-btn:hover,
  .reward-stage-btn:focus-visible {
    background: var(--surface-hover);
  }

  .reward-img {
    max-width: 100%;
    max-height: 94px;
    width: auto;
    height: auto;
    object-fit: contain;
    border-radius: var(--radius-xs);
  }

  .stage-zoom-badge {
    position: absolute;
    left: 5px;
    bottom: 5px;
    width: 20px;
    height: 20px;
    border-radius: var(--radius-xs);
    border: 1px solid var(--border);
    background: rgba(9, 12, 16, 0.88);
    color: var(--accent-text);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    pointer-events: none;
  }

  .reward-stage-btn:hover .stage-zoom-badge,
  .reward-stage-btn:focus-visible .stage-zoom-badge {
    opacity: 1;
  }

  .reward-time {
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

  .reward-meta {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 7px 8px;
    flex: 1;
  }

  .reward-name {
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

  .reward-prog {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .reward-status {
    font-size: 10.5px;
    color: var(--text-muted);
  }

  .badge-manual {
    background: var(--warning-tint);
    color: var(--warning-text);
    border: 1px solid var(--warning);
    border-radius: var(--radius-xs);
    padding: 1px 7px;
    font-size: 11px;
    font-weight: 600;
  }
</style>
