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

  const displayState = $derived(state === 'LoggedOut' ? 'Offline' : state);
</script>

<span class="pill tone-{tone}" class:compact>
  <span class="dot" aria-hidden="true"></span>
  <span class="label">
    {#if labelPrefix}{labelPrefix}: {/if}{displayState}
  </span>
</span>

<style>
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 6px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border);
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    line-height: 1.25;
    white-space: nowrap;
  }

  .pill.compact {
    padding: 2px 7px;
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 999px;
    flex-shrink: 0;
  }

  .tone-success {
    background: var(--success-tint);
    color: var(--success-text);
  }
  .tone-success .dot { background: var(--success); }

  .tone-info {
    background: var(--info-tint);
    color: var(--info-text);
  }
  .tone-info .dot { background: var(--info); }

  .tone-warning {
    background: var(--warning-tint);
    color: var(--warning-text);
  }
  .tone-warning .dot { background: var(--warning); }

  .tone-error {
    background: var(--error-tint);
    color: var(--error-text);
  }
  .tone-error .dot { background: var(--error); }

  .tone-neutral {
    background: var(--neutral-tint);
    color: var(--neutral-text);
  }
  .tone-neutral .dot { background: var(--neutral-dot); }
</style>
