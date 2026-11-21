<script lang="ts">
  import { nowMs } from '../lib/clock';
  import { formatCountdown } from '../lib/format';

  interface Props {
    targetIso: string | null | undefined;
    prefix?: string;
    fallback?: string;
  }

  let { targetIso, prefix = '', fallback = '—' }: Props = $props();

  const label = $derived(targetIso ? formatCountdown(targetIso, $nowMs) : fallback);
</script>

<span class="tabular">
  {#if prefix && targetIso}{prefix}{/if}{label}
</span>
