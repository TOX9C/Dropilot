<script lang="ts">
  interface Props {
    checked: boolean;
    disabled?: boolean;
    ariaLabel: string;
    onchange: (next: boolean) => void;
  }

  let { checked, disabled = false, ariaLabel, onchange }: Props = $props();

  function toggle() {
    if (!disabled) {
      onchange(!checked);
    }
  }
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={ariaLabel}
  class="toggle"
  class:checked={checked}
  {disabled}
  onclick={toggle}
>
  <span class="thumb"></span>
</button>

<style>
  .toggle {
    position: relative;
    width: 36px;
    height: 20px;
    padding: 0;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border);
    background: var(--surface-2);
    flex-shrink: 0;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      border-color var(--duration-fast) var(--ease-out);
  }

  .toggle.checked {
    background: var(--accent);
    border-color: var(--accent);
  }

  .thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: var(--radius-pill);
    background: #ffffff;
    transition: transform var(--duration-fast) var(--ease-out);
  }

  .toggle.checked .thumb {
    transform: translateX(16px);
  }

  .toggle:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
</style>
