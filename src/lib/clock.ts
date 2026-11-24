import { readable } from 'svelte/store';

function isWindowInteractive(): boolean {
  if (typeof document === 'undefined') return true;
  if (document.visibilityState !== 'visible') return false;
  return typeof document.hasFocus === 'function' ? document.hasFocus() : true;
}

/**
 * Reactive store indicating whether Dropilot's window is currently visible AND focused.
 * False whenever minimized, hidden to tray, or Alt-Tabbed away into a game/another app.
 */
export const windowActive = readable<boolean>(isWindowInteractive(), (set) => {
  if (typeof document === 'undefined' || typeof window === 'undefined') {
    return () => {};
  }

  const update = () => {
    set(isWindowInteractive());
  };

  document.addEventListener('visibilitychange', update);
  window.addEventListener('focus', update);
  window.addEventListener('blur', update);

  return () => {
    document.removeEventListener('visibilitychange', update);
    window.removeEventListener('focus', update);
    window.removeEventListener('blur', update);
  };
});

function createVisibleClock(intervalMs: number) {
  return readable<number>(Date.now(), (set) => {
    if (typeof document === 'undefined' || typeof window === 'undefined') {
      return () => {};
    }

    let timer: ReturnType<typeof setInterval> | null = null;

    function start() {
      set(Date.now());
      if (timer !== null) return;
      timer = setInterval(() => {
        if (isWindowInteractive()) {
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

    function syncActivity() {
      if (isWindowInteractive()) {
        start();
      } else {
        stop();
      }
    }

    syncActivity();
    document.addEventListener('visibilitychange', syncActivity);
    window.addEventListener('focus', syncActivity);
    window.addEventListener('blur', syncActivity);

    return () => {
      stop();
      document.removeEventListener('visibilitychange', syncActivity);
      window.removeEventListener('focus', syncActivity);
      window.removeEventListener('blur', syncActivity);
    };
  });
}

/**
 * 1-second clock used ONLY by active live watch progress bars and countdowns.
 * Automatically stops ticking when hidden, minimized, or blurred (Alt-Tabbed away)
 * and snaps immediately to Date.now() on focus.
 */
export const nowMs = createVisibleClock(1000);

/**
 * 30-second low-overhead clock for campaign grids, relative timestamps, and counts.
 * Stops ticking while backgrounded or Alt-Tabbed so idle CPU stays near 0%.
 */
export const slowNowMs = createVisibleClock(30_000);
