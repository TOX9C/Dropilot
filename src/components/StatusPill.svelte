<script lang="ts">
  import type { MinerState } from '../lib/types';

  interface Props {
    state: MinerState;
    compact?: boolean;
    labelPrefix?: string;
  }

  let { state, compact = false, labelPrefix = '' }: Props = $props();

  const tone = $derived.by(() => {
    switch (state) {
      case 'Mining':
        return 'success';
      case 'Claiming':
      case 'Selecting':
        return 'info';
      case 'Paused':
      case 'Idle':
        return 'warning';
      case 'Error':
        return 'error';
      case 'LoggedOut':
      default:
        return 'neutral';
    }
  });

  const displayState = $derived(state === 'LoggedOut' ? 'Logged out' : state);
</script>

<span class="pill tone-{tone}" class:compact={compact}>
  <span class="dot" class:pulse={state === 'Mining'} aria-hidden="true"></span>
  <span class="label">
    {#if labelPrefix}{labelPrefix}: {/if}{displayState}
  </span>
</span>

<style>
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
    font-weight: 600;
    line-height: 1.2;
    white-space: nowrap;
  }

  .pill.compact {
    padding: 3px 8px;
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: var(--radius-pill);
    flex-shrink: 0;
  }

  .tone-success {
    background: var(--success-tint);
    color: var(--success-text);
  }
  .tone-success .dot {
    background: var(--success);
  }

  .tone-info {
    background: var(--info-tint);
    color: var(--info-text);
  }
  .tone-info .dot {
    background: var(--info);
  }

  .tone-warning {
    background: var(--warning-tint);
    color: var(--warning-text);
  }
  .tone-warning .dot {
    background: var(--warning);
  }

  .tone-error {
    background: var(--error-tint);
    color: var(--error-text);
  }
  .tone-error .dot {
    background: var(--error);
  }

  .tone-neutral {
    background: var(--neutral-tint);
    color: var(--neutral-text);
  }
  .tone-neutral .dot {
    background: var(--neutral-dot);
  }

  @media (prefers-reduced-motion: no-preference) {
    .dot.pulse {
      animation: mining-pulse 2s ease-in-out infinite;
    }

    @keyframes mining-pulse {
      0%,
      100% {
        opacity: 1;
        transform: scale(1);
      }
      50% {
        opacity: 0.45;
        transform: scale(0.85);
      }
    }
  }
</style>
