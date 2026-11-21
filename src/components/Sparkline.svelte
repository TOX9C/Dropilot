<script lang="ts">
  interface Props {
    values: number[];
    width?: number;
    height?: number;
    color?: string;
  }

  let { values, width = 96, height = 28, color = 'var(--accent)' }: Props = $props();

  const points = $derived.by(() => {
    const slice = values.slice(-60);
    if (slice.length < 2) {
      const y = height - 4;
      return `0,${y} ${width},${y}`;
    }
    const min = Math.min(...slice);
    const max = Math.max(...slice);
    const span = Math.max(1, max - min);
    return slice
      .map((v, i) => {
        const x = (i / (slice.length - 1)) * width;
        const y = height - 3 - ((v - min) / span) * (height - 6);
        return `${x.toFixed(1)},${y.toFixed(1)}`;
      })
      .join(' ');
  });
</script>

<svg {width} {height} viewBox="0 0 {width} {height}" class="spark" aria-hidden="true">
  <polyline
    fill="none"
    stroke={color}
    stroke-width="1.75"
    stroke-linecap="round"
    stroke-linejoin="round"
    {points}
  />
</svg>

<style>
  .spark {
    display: block;
    overflow: visible;
  }
</style>
