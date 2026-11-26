/** Bind the same responsive viewport in an article and in the standalone host. */
export function mountPreviewFrame(frame, zh = false) {
  const grip = document.createElement('button');
  grip.type = 'button';
  grip.className = 'device-grip';
  grip.innerHTML = '<svg width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true"><path d="M4.5 10.5h6v-6M10 10 2 2M1.5 7.5v-6h6" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>';
  const reset = document.createElement('button');
  reset.type = 'button';
  reset.className = 'frame-dimensions';
  function setLocale(isZh) {
    grip.title = isZh ? '拖动调整尺寸，方向键微调，Home 复位' : 'Drag to resize. Arrow keys adjust, Home resets.';
    grip.setAttribute('aria-label', grip.title);
    reset.title = isZh ? '恢复自适应尺寸' : 'Restore responsive size';
    reset.setAttribute('aria-label', reset.title);
  }
  setLocale(zh);
  frame.append(reset, grip);
  const bounds = frame.parentElement;
  const observer = new ResizeObserver(([entry]) => {
    const { width, height } = entry.contentRect;
    reset.textContent = Math.round(width) + ' × ' + Math.round(height) + ' ↺';
  });
  observer.observe(frame);
  const scope = frame.closest('.lesson-workspace') || bounds;
  let drag;
  function resize(w, h) {
    const width = Math.min(bounds.clientWidth, Math.max(220, w));
    scope.style.setProperty('--preview-width', Math.round(width) + 'px');
    scope.style.setProperty('--preview-height', Math.round(Math.max(260, Math.min(900, h))) + 'px');
  }
  function restore() {
    frame.classList.add('restoring');
    scope.style.removeProperty('--preview-width');
    scope.style.removeProperty('--preview-height');
  }
  reset.onclick = restore;
  grip.onpointerdown = event => {
    if (event.button !== 0 || drag) return;
    event.preventDefault();
    const box = frame.getBoundingClientRect();
    frame.classList.remove('restoring');
    drag = { id: event.pointerId, x: event.clientX, y: event.clientY, w: box.width, h: box.height };
    grip.setPointerCapture(event.pointerId);
    scope.classList.add('resizing');
  };
  grip.onpointermove = event => {
    if (drag?.id === event.pointerId) resize(drag.w + 2 * (event.clientX - drag.x), drag.h + event.clientY - drag.y);
  };
  function end(event) {
    if (drag?.id !== event.pointerId) return;
    drag = undefined;
    scope.classList.remove('resizing');
    if (grip.hasPointerCapture(event.pointerId)) grip.releasePointerCapture(event.pointerId);
  }
  grip.onpointerup = grip.onpointercancel = grip.onlostpointercapture = end;
  grip.onkeydown = event => {
    if (event.key === 'Home' || event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      restore();
      return;
    }
    const step = event.shiftKey ? 40 : 8;
    const delta = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] }[event.key];
    if (!delta) return;
    event.preventDefault();
    frame.classList.remove('restoring');
    resize(frame.clientWidth + delta[0], frame.clientHeight + delta[1]);
  };
  const cleanup = () => {
    observer.disconnect();
    scope.classList.remove('resizing');
    grip.remove();
    reset.remove();
  };
  cleanup.setLocale = setLocale;
  return cleanup;
}
