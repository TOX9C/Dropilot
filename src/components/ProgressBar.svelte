<script lang="ts">
  import type { PlatformId } from '../lib/types';

  interface Props {
    value: number;
    max: number;
    platform?: PlatformId | 'accent';
    tall?: boolean;
    label?: string;
  }

  let { value, max, platform = 'accent', tall = false, label = 'Progress' }: Props = $props();

  const ratio = $derived(max > 0 ? Math.max(0, Math.min(1, value / max)) : 0);
</script>

<div
  class="track"
  class:tall
  role="progressbar"
  aria-label={label}
  aria-valuenow={Math.round(ratio * 100)}
  aria-valuemin={0}
  aria-valuemax={100}
>
  <div class="fill fill-{platform}" style="transform: scaleX({ratio})"></div>
</div>

<style>
  .track {
    width: 100%;
    height: var(--progress-h, 7px);
    border-radius: var(--radius-pill);
    background: var(--surface-2);
    overflow: hidden;
  }

  .track.tall {
    height: var(--progress-tall-h, 10px);
  }

  :global(:root[data-design="console"]) .track {
    border: 1px solid var(--border);
  }

  .fill {
    width: 100%;
    height: 100%;
    border-radius: var(--radius-pill);
    transform-origin: left center;
    background: var(--accent);
  }

  /* Single-accent progress across all designs — no neon rainbow bars */
  .fill-accent,
  .fill-twitch,
  .fill-kick {
    background: var(--accent);
  }

  @media (prefers-reduced-motion: no-preference) {
    .fill {
      transition: transform 200ms var(--ease-out);
    }
  }
</style>
