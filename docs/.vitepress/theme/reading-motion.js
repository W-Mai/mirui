import { nextTick, ref, watch } from 'vue';

export const pageMotion = ref(true);

export function togglePageMotion() {
  pageMotion.value = !pageMotion.value;
  try { localStorage.setItem('mirui-motion', pageMotion.value ? 'on' : 'off'); } catch {}
}

// The router owns history, scrolling and page lifetimes. Capture the old
// article only once the incoming module is loaded, just before Vue commits it.
export function installReadingMotion(router) {
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  let turn;
  let fade;

  function interrupt() {
    turn?.skipTransition();
    fade?.cancel();
  }
  function syncMotion() {
    let choice;
    try { choice = localStorage.getItem('mirui-motion'); } catch {}
    pageMotion.value = choice === 'on' || (choice !== 'off' && !reduced.matches);
  }
  syncMotion();
  watch(pageMotion, enabled => {
    document.documentElement.classList.toggle('site-motion-off', !enabled);
    if (!enabled) interrupt();
  }, { immediate: true, flush: 'sync' });
  reduced.addEventListener('change', syncMotion);
  window.addEventListener('storage', event => { if (!event.key || event.key === 'mirui-motion') syncMotion(); });
  router.onBeforePageLoad = interrupt;
  router.onAfterPageLoad = () => {
    const content = document.getElementById('VPContent');
    if (!router.route.component || !content || document.hidden || !pageMotion.value) return;
    if (!document.startViewTransition) {
      const leaving = content.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 90, fill: 'forwards' });
      fade = leaving;
      return leaving.finished.then(() => {
        leaving.cancel();
        fade = content.animate([{ opacity: 0, transform: 'translateY(5px)' }, { opacity: 1, transform: 'none' }],
          { duration: 180, easing: 'ease-out' });
      }, () => {});
    }
    const root = document.documentElement;
    root.classList.add('reading-turn');
    return new Promise(captured => {
      const current = document.startViewTransition(async () => {
        captured();
        // The router resumes at its awaiting hook and queues Vue's patch.
        await Promise.resolve();
        await nextTick();
      });
      turn = current;
      // A newer navigation can supersede this capture before its first frame.
      current.ready.catch(error => {
        if (error.name !== 'AbortError') console.warn(error);
      });
      current.finished.finally(() => {
        if (turn === current) {
          root.classList.remove('reading-turn');
          turn = undefined;
        }
      });
    });
  };
}
