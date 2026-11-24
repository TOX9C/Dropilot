<script lang="ts">
  import { onMount } from 'svelte';
  import Bell from 'lucide-svelte/icons/bell';
  import CalendarClock from 'lucide-svelte/icons/calendar-clock';
  import Check from 'lucide-svelte/icons/check';
  import Clock from 'lucide-svelte/icons/clock';
  import ExternalLink from 'lucide-svelte/icons/external-link';
  import Gamepad2 from 'lucide-svelte/icons/gamepad-2';
  import Gift from 'lucide-svelte/icons/gift';
  import Maximize2 from 'lucide-svelte/icons/maximize-2';
  import RefreshCw from 'lucide-svelte/icons/refresh-cw';
  import Sparkles from 'lucide-svelte/icons/sparkles';
  import Badge from '../components/Badge.svelte';
  import Button from '../components/Button.svelte';
  import Monogram from '../components/Monogram.svelte';
  import { api } from '../lib/api';
  import { slowNowMs } from '../lib/clock';
  import {
    formatEndCountdown,
    formatExactDateTime,
    formatRelativeTime,
    formatStartsIn,
  } from '../lib/format';
  import { epicGameArtCandidates } from '../lib/selectors';
  import { epicFeed, notifyUi, openLightbox } from '../lib/stores';
  import type { EpicFreeGame } from '../lib/types';

  let loading = $state(false);
  let claimingId = $state<string | null>(null);
  let togglingId = $state<string | null>(null);
  let artStage = $state<Record<string, number>>({});

  $effect(() => {
    void $epicFeed?.fetchedAt;
    artStage = {};
  });

  function currentArtUrl(game: EpicFreeGame): string | null {
    const candidates = epicGameArtCandidates(game);
    const stage = artStage[game.id] ?? 0;
    return candidates[stage] ?? null;
  }

  function advanceArtStage(gameId: string) {
    artStage = { ...artStage, [gameId]: (artStage[gameId] ?? 0) + 1 };
  }

  async function loadEpic(force = false) {
    loading = true;
    try {
      const next = force ? await api.refreshEpicFreeGames() : await api.getEpicFreeGames();
      epicFeed.set(next);
      if (force) {
        if (next.lastError) {
          notifyUi(`Epic refresh failed: ${next.lastError}`, 'error');
        } else {
          notifyUi(`Synced ${next.games.length} Epic free game offers`);
        }
      }
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void loadEpic(false);
  });

  const activeGames = $derived(
    ($epicFeed?.games ?? []).filter((g) => g.status === 'active'),
  );
  const upcomingGames = $derived(
    ($epicFeed?.games ?? []).filter((g) => g.status === 'upcoming'),
  );
  const unclaimedActiveCount = $derived(activeGames.filter((g) => !g.claimed).length);

  async function handleClaimInDropilot(game: EpicFreeGame) {
    claimingId = game.id;
    try {
      await api.openEpicClaim(game.id);
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      claimingId = null;
    }
  }

  async function handleToggleClaimed(game: EpicFreeGame) {
    togglingId = game.id;
    try {
      const next = await api.markEpicGameClaimed(game.id, !game.claimed);
      epicFeed.set(next);
      notifyUi(
        !game.claimed
          ? `Marked "${game.title}" as claimed and saved to History`
          : `Removed "${game.title}" claimed mark`,
      );
    } catch (err) {
      notifyUi(String(err), 'error');
    } finally {
      togglingId = null;
    }
  }

  async function handleOpenStore(url: string) {
    try {
      await api.openUrl(url);
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }

  async function handleOpenLauncher(game?: EpicFreeGame) {
    try {
      const openedLauncher = await api.openEpicLauncher(game?.id ?? null);
      if (!openedLauncher) {
        notifyUi('Epic Games Launcher was not detected — opened store page in browser instead');
      }
    } catch (err) {
      notifyUi(String(err), 'error');
    }
  }
</script>

{#snippet gameBanner(game: EpicFreeGame, lightboxSubtitle: string)}
  {@const artUrl = currentArtUrl(game)}
  {#if artUrl}
    <button
      type="button"
      class="banner-btn has-art"
      title="Click to view full artwork"
      aria-label={`View full artwork for ${game.title}`}
      onclick={() => {
        openLightbox({
          src: artUrl,
          title: game.title,
          subtitle: lightboxSubtitle,
        });
      }}
    >
      <img
        class="banner-img"
        src={artUrl}
        alt={game.title}
        loading="lazy"
        decoding="async"
        referrerpolicy="no-referrer"
        onerror={() => advanceArtStage(game.id)}
      />
      <span class="zoom-badge" aria-hidden="true"><Maximize2 size={12} /></span>
    </button>
  {:else}
    <div class="banner-btn banner-fallback" aria-hidden="true">
      <Monogram src={null} name={game.title} width={56} height={56} />
    </div>
  {/if}
{/snippet}

<div class="page epic-page">
  <header class="page-header">
    <div class="page-title-wrap">
      <div class="title-inline">
        <span class="plat-dot dot-epic" aria-hidden="true"></span>
        <h1>Epic Free Weekly</h1>
        {#if activeGames.length > 0}
          <Badge tone={unclaimedActiveCount > 0 ? 'accent' : 'success'}>
            {unclaimedActiveCount > 0
              ? `${unclaimedActiveCount} FREE NOW`
              : `${activeGames.length} CLAIMED`}
          </Badge>
        {/if}
        {#if upcomingGames.length > 0}
          <Badge tone="neutral">{upcomingGames.length} NEXT WEEK</Badge>
        {/if}
      </div>
      <p class="page-subtitle tabular">
        {#if $epicFeed?.fetchedAt}
          Synced {formatRelativeTime($epicFeed.fetchedAt, $slowNowMs)} · Official Epic Games Store rotation
        {:else}
          Official Epic Games Store weekly free games rotation
        {/if}
      </p>
    </div>

    <div class="page-actions">
      <Button
        variant="secondary"
        size="sm"
        onclick={() => handleOpenLauncher()}
        title="Open Epic Games Launcher to Free Games"
      >
        <Gamepad2 size={13} />
        <span>Open Launcher</span>
      </Button>
      <Button
        variant="secondary"
        size="sm"
        onclick={() => handleOpenStore('https://store.epicgames.com/en-US/free-games')}
      >
        <ExternalLink size={13} />
        <span>Epic Store</span>
      </Button>
      <Button variant="primary" size="sm" disabled={loading} onclick={() => loadEpic(true)}>
        <RefreshCw size={13} />
        <span>{loading ? 'Syncing...' : 'Refresh'}</span>
      </Button>
    </div>
  </header>

  {#if $epicFeed?.lastError}
    <div class="card error-strip" role="alert">
      <div class="err-copy">
        <strong>Could not reach Epic Games Store</strong>
        <span>{$epicFeed.lastError}</span>
      </div>
      <Button variant="secondary" size="sm" disabled={loading} onclick={() => loadEpic(true)}>
        <RefreshCw size={12} />
        <span>Retry</span>
      </Button>
    </div>
  {/if}

  <div class="card assist-note">
    <Sparkles size={14} />
    <span>
      <strong>1-Click Assisted Claim</strong> opens the official Epic Games Store page inside Dropilot's isolated session window so you stay signed in across weeks and handle checkout or anti-bot checks directly.
    </span>
  </div>

  {#if loading && !$epicFeed?.games.length}
    <div class="designed-empty">
      <span class="empty-ico" aria-hidden="true"><Gift size={18} /></span>
      <strong>Loading Epic Games weekly free catalog...</strong>
      <p>Checking Epic Games Store for active and upcoming weekly free titles.</p>
    </div>
  {:else if activeGames.length === 0 && upcomingGames.length === 0}
    <div class="designed-empty">
      <span class="empty-ico" aria-hidden="true"><Gift size={18} /></span>
      <strong>No Epic free game offers found right now</strong>
      <p>Click Refresh to check Epic Games Store again.</p>
      <Button variant="secondary" size="sm" onclick={() => loadEpic(true)}>
        <RefreshCw size={13} />
        <span>Refresh now</span>
      </Button>
    </div>
  {:else}
    <!-- Active Free Now -->
    <section class="epic-section" aria-label="Free to claim now">
      <div class="sec-head">
        <div class="sec-left">
          <Gift size={14} />
          <span class="section-label">FREE NOW</span>
          <Badge tone="neutral">{activeGames.length}</Badge>
        </div>
        <span class="sec-hint">Claim before the weekly Thursday rotation ends</span>
      </div>

      {#if activeGames.length === 0}
        <div class="card empty-inline">
          No active free titles detected right now — see upcoming titles below.
        </div>
      {:else}
        <div class="epic-grid">
          {#each activeGames as game (game.id)}
            {@const endInfo = formatEndCountdown(game.endsAt, $slowNowMs)}
            {@const lightboxSub = `${game.seller} · ${game.originalPrice !== 'Free' ? `Usually ${game.originalPrice}` : 'Free on Epic Games Store'}`}
            <article class="card epic-card" class:is-claimed={game.claimed}>
              {@render gameBanner(game, lightboxSub)}

              <div class="card-meta">
                <div class="price-row">
                  {#if game.claimed}
                    <Badge tone="success"><Check size={10} /> CLAIMED</Badge>
                  {:else}
                    <Badge tone="accent">FREE NOW</Badge>
                  {/if}
                  {#if game.originalPrice && game.originalPrice !== 'Free'}
                    <span class="orig-price tabular">{game.originalPrice}</span>
                  {/if}
                </div>

                <h2 class="game-title" title={game.title}>{game.title}</h2>
                <span class="game-seller">{game.seller}</span>

                {#if game.description}
                  <p class="game-desc">{game.description}</p>
                {/if}

                <div class="countdown-row tabular" title={`Ends ${formatExactDateTime(game.endsAt)}`}>
                  <Badge tone={endInfo.tone}>
                    <Clock size={10} />
                    <span>{endInfo.ended ? 'Ended' : `Ends in ${endInfo.text}`}</span>
                  </Badge>
                </div>
              </div>

              <footer class="card-actions">
                <Button
                  variant={game.claimed ? 'secondary' : 'primary'}
                  size="sm"
                  disabled={claimingId === game.id}
                  onclick={() => handleClaimInDropilot(game)}
                >
                  <Gift size={12} />
                  <span>{claimingId === game.id ? 'Opening...' : 'Claim in Dropilot'}</span>
                </Button>

                <Button
                  variant={game.claimed ? 'ghost' : 'secondary'}
                  size="sm"
                  disabled={togglingId === game.id}
                  onclick={() => handleToggleClaimed(game)}
                >
                  <Check size={12} />
                  <span>{game.claimed ? 'Unmark' : 'Mark claimed'}</span>
                </Button>

                <Button
                  variant="ghost"
                  size="sm"
                  onclick={() => handleOpenLauncher(game)}
                  title="Open this game in Epic Games Launcher"
                >
                  <Gamepad2 size={12} />
                  <span>Launcher</span>
                </Button>

                <Button
                  variant="ghost"
                  size="sm"
                  onclick={() => handleOpenStore(game.storeUrl)}
                  title="Open store page in default browser"
                >
                  <ExternalLink size={12} />
                  <span>Browser</span>
                </Button>
              </footer>
            </article>
          {/each}
        </div>
      {/if}
    </section>

    <!-- Coming Next Week -->
    {#if upcomingGames.length > 0}
      <section class="epic-section" aria-label="Coming next week">
        <div class="sec-head">
          <div class="sec-left">
            <CalendarClock size={14} />
            <span class="section-label">COMING NEXT WEEK</span>
            <Badge tone="neutral">{upcomingGames.length}</Badge>
          </div>
          <span class="sec-hint">
            <Bell size={11} />
            <span>Dropilot notifies you via desktop toast &amp; Discord when these unlock</span>
          </span>
        </div>

        <div class="epic-grid">
          {#each upcomingGames as game (game.id)}
            {@const startsLabel = formatStartsIn(game.startsAt, $slowNowMs)}
            <article class="card epic-card upcoming-card">
              {@render gameBanner(game, `Unlocks ${formatExactDateTime(game.startsAt)}`)}

              <div class="card-meta">
                <div class="price-row">
                  <Badge tone="info">UPCOMING</Badge>
                  {#if game.originalPrice && game.originalPrice !== 'Free'}
                    <span class="orig-price tabular">{game.originalPrice}</span>
                  {/if}
                </div>

                <h2 class="game-title" title={game.title}>{game.title}</h2>
                <span class="game-seller">{game.seller}</span>

                {#if game.description}
                  <p class="game-desc">{game.description}</p>
                {/if}

                <div class="countdown-row tabular" title={`Free ${formatExactDateTime(game.startsAt)} – ${formatExactDateTime(game.endsAt)}`}>
                  <Badge tone="neutral">
                    <Clock size={10} />
                    <span>{startsLabel}</span>
                  </Badge>
                </div>
              </div>

              <footer class="card-actions">
                <Button
                  variant="secondary"
                  size="sm"
                  onclick={() => handleOpenLauncher(game)}
                  title="Open this game in Epic Games Launcher"
                >
                  <Gamepad2 size={12} />
                  <span>Open in Launcher</span>
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onclick={() => handleOpenStore(game.storeUrl)}
                >
                  <ExternalLink size={12} />
                  <span>Browser</span>
                </Button>
              </footer>
            </article>
          {/each}
        </div>
      </section>
    {/if}
  {/if}
</div>

<style>
  .title-inline {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .error-strip {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    border-color: var(--error);
    background: var(--error-tint);
  }

  .err-copy {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: var(--text-xs);
    color: var(--error-text);
  }

  .assist-note {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    padding: 9px 12px;
    font-size: var(--text-xs);
    color: var(--text-muted);
    background: var(--surface-2);
    line-height: 1.45;
  }

  .assist-note strong {
    color: var(--text);
  }

  .epic-section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .sec-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .sec-left {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    color: var(--text-muted);
  }

  .sec-hint {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text-faint);
  }

  .empty-inline {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }

  .epic-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(300px, 100%), 1fr));
    gap: var(--grid-gap, 12px);
  }

  .epic-card {
    display: flex;
    flex-direction: column;
    padding: 0;
    overflow: hidden;
  }

  .epic-card.is-claimed {
    border-color: var(--success);
  }

  .upcoming-card {
    opacity: 0.9;
  }

  .banner-btn {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
    border: none;
    border-bottom: 1px solid var(--border);
    background: var(--surface-2);
    padding: 0;
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .banner-btn.has-art:hover .banner-img,
  .banner-btn.has-art:focus-visible .banner-img {
    transform: scale(1.02);
  }

  .banner-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: center;
    display: block;
    transition: transform var(--duration-base) var(--ease-out);
  }

  .zoom-badge {
    position: absolute;
    right: 8px;
    bottom: 8px;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-xs);
    border: 1px solid rgba(255, 255, 255, 0.14);
    background: rgba(9, 12, 16, 0.82);
    color: #e6edf3;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transition: opacity var(--duration-fast) var(--ease-out);
  }

  .banner-btn.has-art:hover .zoom-badge,
  .banner-btn.has-art:focus-visible .zoom-badge {
    opacity: 1;
  }

  .card-meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 12px;
  }

  .price-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .orig-price {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-faint);
    text-decoration: line-through;
  }

  .game-title {
    font-size: var(--text-base);
    font-weight: 700;
    color: var(--text);
    line-height: 1.25;
  }

  .game-seller {
    font-size: 11px;
    color: var(--text-faint);
  }

  .game-desc {
    font-size: var(--text-xs);
    color: var(--text-muted);
    line-height: 1.45;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .countdown-row {
    margin-top: auto;
    padding-top: 4px;
  }

  .card-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    background: var(--surface-2);
    border-top: 1px solid var(--border);
  }
</style>
