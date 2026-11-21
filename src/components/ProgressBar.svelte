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
  class:tall={tall}
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
    height: 7px;
    border-radius: var(--radius-pill);
    background: var(--surface-2);
    border: 1px solid var(--border);
    overflow: hidden;
  }

  .track.tall {
    height: 11px;
  }

  .fill {
    width: 100%;
    height: 100%;
    border-radius: var(--radius-pill);
    transform-origin: left center;
  }

  .fill-accent {
    background: var(--grad-accent);
  }
  .fill-twitch {
    background: var(--grad-twitch);
  }
  .fill-kick {
    background: var(--grad-kick);
  }

  @media (prefers-reduced-motion: no-preference) {
    .fill {
      transition: transform 200ms var(--ease-out);
    }
  }
</style>
