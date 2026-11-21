<script lang="ts">
</script>

<div class="skel-page" aria-busy="true" aria-label="Loading DropPilot">
  <div class="shimmer skel-title"></div>
  <div class="skel-stats">
    <div class="card shimmer skel-stat"></div>
    <div class="card shimmer skel-stat"></div>
    <div class="card shimmer skel-stat"></div>
    <div class="card shimmer skel-stat"></div>
  </div>
  <div class="skel-grid">
    <div class="card shimmer skel-hero"></div>
    <div class="card shimmer skel-hero"></div>
  </div>
</div>

<style>
  .skel-page {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .skel-title {
    width: 180px;
    height: 28px;
  }

  .skel-stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: var(--space-3);
  }

  .skel-stat {
    height: 84px;
  }

  .skel-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: var(--space-4);
  }

  .skel-hero {
    height: 170px;
  }
</style>
