import { readable } from 'svelte/store';

/**
 * Single shared 1-second clock store for every countdown and relative timer.
 * Automatically stops ticking when the document is hidden or unmounted.
 */
export const nowMs = readable<number>(Date.now(), (set) => {
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
    }, 1000);
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
