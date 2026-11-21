<script lang="ts">
  import type { Snippet } from 'svelte';
  import Badge from './Badge.svelte';

  interface Props {
    title: string;
    count?: number | null;
    uppercase?: boolean;
    icon?: Snippet;
    actions?: Snippet;
  }

  let { title, count = null, uppercase = false, icon, actions }: Props = $props();
</script>

<div class="sec-head">
  <div class="sec-left">
    {#if icon}
      <span class="sec-icon" aria-hidden="true">{@render icon()}</span>
    {/if}
    <h2 class:section-label={uppercase} class:sec-title={!uppercase}>{title}</h2>
    {#if count !== null}
      <Badge>{count}</Badge>
    {/if}
  </div>
  {#if actions}
    <div class="sec-actions">
      {@render actions()}
    </div>
  {/if}
</div>

<style>
  .sec-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .sec-left,
  .sec-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .sec-icon {
    color: var(--text-muted);
    display: flex;
  }

  .sec-title {
    font-size: var(--text-md);
    font-weight: 600;
  }
</style>
