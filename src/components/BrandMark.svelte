<script lang="ts">
  interface Props {
    size?: number;
    showWordmark?: boolean;
    compact?: boolean;
    color?: string;
  }

  let { size = 20, showWordmark = true, compact = false, color = 'var(--accent)' }: Props = $props();
</script>

<div class="brand-lockup" class:compact>
  <svg
    class="brand-mark"
    viewBox="0 0 256 256"
    width={size}
    height={size}
    fill="none"
    aria-hidden="true"
  >
    <!-- Dropilot — Console: Precision tangent teardrop with compass navigation pointer -->
    <path
      fill={color}
      fill-rule="evenodd"
      clip-rule="evenodd"
      d="M128 22 L190 110 A76 76 0 1 1 66 110 Z M128 96 L162 178 L128 158 L94 178 Z"
    />
  </svg>

  {#if showWordmark}
    <span class="wm-console">dropilot<span class="wm-ver">_0.1</span></span>
  {/if}
</div>

<style>
  .brand-lockup {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .brand-lockup.compact {
    gap: 0;
  }

  .brand-mark {
    flex-shrink: 0;
    display: block;
  }

  .wm-console {
    font-family: var(--font-mono);
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.01em;
    color: var(--text);
  }

  .wm-ver {
    color: var(--text-faint);
    font-weight: 400;
  }
</style>
