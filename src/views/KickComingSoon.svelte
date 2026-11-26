<script lang="ts">
  import Gift from 'lucide-svelte/icons/gift';
  import Layers from 'lucide-svelte/icons/layers';
  import Sparkles from 'lucide-svelte/icons/sparkles';
  import Tv from 'lucide-svelte/icons/tv';
  import Zap from 'lucide-svelte/icons/zap';
  import Badge from '../components/Badge.svelte';
  import Button from '../components/Button.svelte';
  import { activeRoute } from '../lib/stores';
</script>

<div class="page soon-page">
  <header class="page-header">
    <div class="page-title-wrap">
      <div class="title-inline">
        <span class="plat-dot dot-kick" aria-hidden="true"></span>
        <h1>Kick Drops</h1>
        <Badge tone="kick">Coming soon</Badge>
      </div>
    </div>
    <div class="page-actions">
      <Button variant="secondary" size="sm" onclick={() => activeRoute.set('twitch')}>
        <Tv size={13} />
        <span>Open Twitch</span>
      </Button>
      <Button variant="primary" size="sm" onclick={() => activeRoute.set('drops')}>
        <Gift size={13} />
        <span>Browse Drops</span>
      </Button>
    </div>
  </header>

  <section class="card hero-card">
    <div class="hero-top">
      <div class="hero-icon" aria-hidden="true">
        <Zap size={20} />
      </div>
      <div class="hero-copy">
        <div class="hero-kicker mono">UPCOMING DROPILOT RELEASE</div>
        <h2>Kick Drops &amp; Multi-Account Farming</h2>
        <p>
          You're running <strong>Dropilot Free Edition</strong> (Twitch Drops + Epic Games Free Weekly).
          Kick Drops automation and multi-account profiles are arriving in an upcoming release — Dropilot
          will deliver it automatically through the built-in signed updater with zero reinstall required.
        </p>
      </div>
    </div>

    <div class="feature-grid">
      <div class="feat-card">
        <div class="feat-head">
          <span class="feat-ico"><Zap size={14} /></span>
          <strong>Zero-Video Kick Mining</strong>
        </div>
        <p>
          Automatic Kick Drops campaign discovery, live channel selection, viewer presence heartbeats,
          and automatic reward claiming with the same tiny tray footprint.
        </p>
      </div>

      <div class="feat-card">
        <div class="feat-head">
          <span class="feat-ico"><Sparkles size={14} /></span>
          <strong>Watch-Time Reward Cases</strong>
        </div>
        <p>
          Track Kick gamification challenges and time-limited reward boxes automatically alongside
          standard game drop campaigns.
        </p>
      </div>

      <div class="feat-card">
        <div class="feat-head">
          <span class="feat-ico"><Layers size={14} /></span>
          <strong>Multi-Account Profiles</strong>
        </div>
        <p>
          Run multiple isolated Twitch and Kick accounts concurrently with independent priority queues
          and separate WebView2 sessions.
        </p>
      </div>
    </div>

    <div class="hero-foot">
      <span class="foot-note">
        Keep <strong>Check for signed updates automatically</strong> enabled in Settings so you get the
        update the moment it ships.
      </span>
      <Button variant="secondary" size="sm" onclick={() => activeRoute.set('settings')}>
        Update Settings
      </Button>
    </div>
  </section>
</div>

<style>
  .soon-page {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .title-inline {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .hero-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
    padding: var(--space-6);
  }

  .hero-top {
    display: flex;
    align-items: flex-start;
    gap: var(--space-4);
  }

  .hero-icon {
    width: 44px;
    height: 44px;
    border-radius: var(--radius-md);
    background: var(--kick-soft, rgba(83, 252, 24, 0.12));
    color: var(--kick, #53fc18);
    border: 1px solid rgba(83, 252, 24, 0.25);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .hero-copy {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-width: 68ch;
  }

  .hero-kicker {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--kick, #53fc18);
  }

  .hero-copy h2 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--text);
  }

  .hero-copy p {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-muted);
    line-height: 1.55;
  }

  .feature-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: var(--space-3);
  }

  .feat-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-4);
    border-radius: var(--radius-md);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }

  .feat-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    color: var(--text);
  }

  .feat-ico {
    width: 24px;
    height: 24px;
    border-radius: var(--radius-sm);
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--kick, #53fc18);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .feat-card p {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-muted);
    line-height: 1.5;
  }

  .hero-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--space-3);
    padding-top: var(--space-4);
    border-top: 1px solid var(--border);
  }

  .foot-note {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
</style>
