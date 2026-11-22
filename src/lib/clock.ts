import { readable } from 'svelte/store';

function createVisibleClock(intervalMs: number) {
  return readable<number>(Date.now(), (set) => {
    if (typeof document === 'undefined') {
      return () => {};
    }

    let timer: ReturnType<typeof setInterval> | null = null;

    function start() {
      if (timer !== null) return;
      set(Date.now());
      timer = setInterval(() => {
        if (document.visibilityState === 'visible') {
          set(Date.now());
        }
      }, intervalMs);
    }

    function stop() {
      if (timer !== null) {
        clearInterval(timer);
        timer = null;
      }
    }

    function onVisibility() {
      if (document.visibilityState === 'visible') {
        start();
      } else {
        stop();
      }
    }

    if (document.visibilityState === 'visible') {
      start();
    }
    document.addEventListener('visibilitychange', onVisibility);

    return () => {
      stop();
      document.removeEventListener('visibilitychange', onVisibility);
    };
  });
}

/**
 * 1-second clock used ONLY by active live watch progress bars and countdowns.
 * Automatically stops ticking when the window is hidden or minimized.
 */
export const nowMs = createVisibleClock(1000);

/**
 * 30-second low-overhead clock for campaign grids, relative timestamps, and counts.
 * Prevents re-sorting and re-rendering 100+ cards every second (keeps idle CPU near 0%).
 */
export const slowNowMs = createVisibleClock(30_000);
