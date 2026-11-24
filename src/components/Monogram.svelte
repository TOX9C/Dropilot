<script lang="ts">
  import { monogramFor } from '../lib/selectors';

  interface Props {
    src: string | null | undefined;
    name: string;
    width?: number;
    height?: number;
    rounded?: boolean;
  }

  let { src, name, width = 36, height = 36, rounded = false }: Props = $props();

  let failed = $state(false);

  $effect(() => {
    void src;
    failed = false;
  });

  const mono = $derived(monogramFor(name));
</script>

{#if src && !failed}
  <img
    {src}
    alt=""
    {width}
    {height}
    loading="lazy"
    decoding="async"
    referrerpolicy="no-referrer"
    class="mono-img"
    class:rounded
    style="width:{width}px;height:{height}px"
    onerror={() => (failed = true)}
  />
{:else}
  <div
    class="mono-tile"
    class:rounded
    style="width:{width}px;height:{height}px"
    aria-hidden="true"
  >
    {mono.initials}
  </div>
{/if}

<style>
  .mono-img,
  .mono-tile {
    flex-shrink: 0;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    object-fit: cover;
    font-family: var(--font-display);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.03em;
  }

  .mono-tile {
    background: var(--surface-2);
    color: var(--text-muted);
  }

  .rounded {
    border-radius: var(--radius-pill);
  }
</style>
