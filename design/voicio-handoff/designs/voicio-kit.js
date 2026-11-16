(function () {
  if (window.VK) return;
  const P = {
    mic: '<path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><path d="M12 19v3"/>',
    home: '<path d="M3 10 12 3l9 7v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><path d="M9 22V12h6v10"/>',
    history: '<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/><path d="M12 7v5l4 2"/>',
    models: '<path d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z"/><path d="m3.3 7 8.7 5 8.7-5"/><path d="M12 22V12"/>',
    book: '<path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1 0-5H20"/>',
    settings: '<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>',
    info: '<circle cx="12" cy="12" r="10"/><path d="M12 16v-4M12 8h.01"/>',
    copy: '<rect width="14" height="14" x="8" y="8" rx="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>',
    trash: '<path d="M3 6h18M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>',
    search: '<circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/>',
    x: '<path d="M18 6 6 18M6 6l12 12"/>',
    check: '<path d="M20 6 9 17l-5-5"/>',
    download: '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/><path d="M12 15V3"/>',
    pause: '<rect x="6" y="5" width="4" height="14" rx="1"/><rect x="14" y="5" width="4" height="14" rx="1"/>',
    play: '<path d="M7 4v16l13-8z"/>',
    alert: '<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4M12 17h.01"/>',
    alertCircle: '<circle cx="12" cy="12" r="10"/><path d="M12 8v4M12 16h.01"/>',
    shield: '<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/><path d="m9 12 2 2 4-4"/>',
    drive: '<path d="M22 12H2"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/><path d="M6 16h.01M10 16h.01"/>',
    retry: '<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/>',
    lock: '<rect width="18" height="11" x="3" y="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>',
    plus: '<path d="M5 12h14M12 5v14"/>',
    minus: '<path d="M5 12h14"/>',
    chevDown: '<path d="m6 9 6 6 6-6"/>',
    chevRight: '<path d="m9 18 6-6-6-6"/>',
    arrowRight: '<path d="M5 12h14M13 5l7 7-7 7"/>',
    arrowLeft: '<path d="M19 12H5M11 19l-7-7 7-7"/>',
    terminal: '<path d="m4 17 6-6-6-6"/><path d="M12 19h8"/>',
    keyboard: '<rect width="20" height="16" x="2" y="4" rx="2"/><path d="M6 8h.01M10 8h.01M14 8h.01M18 8h.01M8 12h.01M12 12h.01M16 12h.01M7 16h10"/>',
    power: '<path d="M12 2v10"/><path d="M18.4 6.6a9 9 0 1 1-12.77.04"/>',
    ext: '<path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>',
    file: '<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4M16 13H8M16 17H8"/>',
    max: '<rect x="5" y="5" width="14" height="14" rx="2"/>',
    enter: '<path d="m9 10-5 5 5 5"/><path d="M20 4v7a4 4 0 0 1-4 4H4"/>',
    undo: '<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/>',
    pilcrow: '<path d="M13 4v16M17 4v16M19 4H9.5a4.5 4.5 0 0 0 0 9H13"/>',
    stop: '<rect x="6" y="6" width="12" height="12" rx="2"/>',
    circleCheck: '<circle cx="12" cy="12" r="10"/><path d="m9 12 2 2 4-4"/>',
    wand: '<path d="m21.64 3.64-1.28-1.28a1.21 1.21 0 0 0-1.72 0L2.36 18.64a1.21 1.21 0 0 0 0 1.72l1.28 1.28a1.2 1.2 0 0 0 1.72 0L21.64 5.36a1.2 1.2 0 0 0 0-1.72"/><path d="m14 7 3 3"/><path d="M5 6v4M19 14v4M10 2v2M7 8H3M21 16h-4M11 3H9"/>',
    activity: '<path d="M22 12h-4l-3 9L9 3l-3 9H2"/>',
    gauge: '<path d="m12 14 4-4"/><path d="M3.34 19a10 10 0 1 1 17.32 0"/>',
    globe: '<circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20M2 12h20"/>',
    type: '<path d="M4 7V4h16v3M9 20h6M12 4v16"/>',
    clipboard: '<rect width="8" height="4" x="8" y="2" rx="1"/><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/>',
    grip: '<circle cx="9" cy="6" r="1"/><circle cx="15" cy="6" r="1"/><circle cx="9" cy="12" r="1"/><circle cx="15" cy="12" r="1"/><circle cx="9" cy="18" r="1"/><circle cx="15" cy="18" r="1"/>',
    cpu: '<rect x="4" y="4" width="16" height="16" rx="2"/><rect x="9" y="9" width="6" height="6" rx="1"/><path d="M15 2v2M15 20v2M2 15h2M2 9h2M20 15h2M20 9h2M9 2v2M9 20v2"/>',
    micOff: '<path d="M2 2l20 20"/><path d="M18.89 13.23A7 7 0 0 0 19 12v-2"/><path d="M5 10v2a7 7 0 0 0 12 5"/><path d="M15 9.34V5a3 3 0 0 0-5.68-1.33"/><path d="M9 9v3a3 3 0 0 0 5.12 2.12"/><path d="M12 19v3"/>',
    sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/>',
    moon: '<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>',
    monitor: '<rect width="20" height="14" x="2" y="3" rx="2"/><path d="M8 21h8M12 17v4"/>',
    plug: '<path d="M12 22v-5M9 8V2M15 8V2"/><path d="M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8Z"/>',
    code: '<path d="m16 18 6-6-6-6M8 6l-6 6 6 6"/>',
    timer: '<path d="M10 2h4M12 14l3-3"/><circle cx="12" cy="14" r="8"/>',
    eraser: '<path d="m7 21-4.3-4.3c-1-1-1-2.5 0-3.4l9.6-9.6c1-1 2.5-1 3.4 0l5.6 5.6c1 1 1 2.5 0 3.4L13 21"/><path d="M22 21H7M5 11l9 9"/>',
    ban: '<circle cx="12" cy="12" r="10"/><path d="m4.9 4.9 14.2 14.2"/>',
    layout: '<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 15h18"/>',
    image: '<rect width="18" height="18" x="3" y="3" rx="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.1-3.1a2 2 0 0 0-2.8 0L6 21"/>',
    clock: '<circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/>',
    move: '<path d="M12 2v20M2 12h20M5 9l-3 3 3 3M19 9l3 3-3 3M9 5l3-3 3 3M9 19l3 3 3-3"/>'
  };
  const css = `@keyframes vk-wave{0%{transform:scaleY(.22)}100%{transform:scaleY(1)}}
@keyframes vk-pulse{0%{transform:scale(1);opacity:.55}100%{transform:scale(1.3);opacity:0}}
@keyframes vk-spin{to{transform:rotate(360deg)}}
@keyframes vk-level{0%{clip-path:inset(0 88% 0 0)}18%{clip-path:inset(0 38% 0 0)}40%{clip-path:inset(0 64% 0 0)}62%{clip-path:inset(0 22% 0 0)}82%{clip-path:inset(0 52% 0 0)}100%{clip-path:inset(0 30% 0 0)}}
@keyframes vk-shimmer{0%,100%{opacity:.28}50%{opacity:1}}
@keyframes vk-blink{50%{opacity:0}}
@keyframes vk-breathe{0%,100%{opacity:1}50%{opacity:.3}}
@keyframes vk-pop{0%{transform:scale(.6);opacity:0}100%{transform:scale(1);opacity:1}}
:focus-visible{outline:2px solid var(--focus);outline-offset:2px}
button:disabled{cursor:not-allowed}
@media (prefers-reduced-motion: reduce){*,*::before,*::after{animation:none!important;transition:none!important}}`;
  const st = document.createElement('style');
  st.textContent = css;
  document.head.appendChild(st);

  const rnd = (i) => { const x = Math.sin(i * 127.1 + 311.7) * 43758.5453; return x - Math.floor(x); };
  const cache = {};
  function icons(R, s = 16, sw = 1.75) {
    const k = s + '_' + sw;
    if (cache[k]) return cache[k];
    const o = {};
    for (const n in P) o[n] = R.createElement('svg', { width: s, height: s, viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', strokeWidth: sw, strokeLinecap: 'round', strokeLinejoin: 'round', 'aria-hidden': true, style: { flex: 'none', display: 'block' }, dangerouslySetInnerHTML: { __html: P[n] } });
    return (cache[k] = o);
  }
  function wave(R, o = {}) {
    const n = o.n || 40, h = o.h || 48, w = o.w || 4, g = o.g ?? 4, mode = o.mode || 'live', c = o.color || 'var(--ac)';
    const bars = [];
    for (let i = 0; i < n; i++) {
      const env = Math.pow(Math.sin(Math.PI * (i + 0.5) / n), 0.7);
      const v = Math.max(0.14, env * (0.35 + 0.65 * rnd(i + (o.seed || 0))));
      const pc = Math.round(Math.abs(i - (n - 1) / 2) / ((n - 1) / 2) * 100);
      const s = { width: w, borderRadius: w, background: o.grad ? `color-mix(in oklch, var(--ac) ${100 - pc}%, var(--ac2) ${pc}%)` : c, flex: 'none', transformOrigin: 'center' };
      if (mode === 'idle') { s.height = w; s.opacity = 0.6; }
      else if (mode === 'processing') { s.height = Math.max(w, h * v * 0.45); s.animation = `vk-shimmer 1.2s ease-in-out ${(i / n * 1.2).toFixed(2)}s infinite`; }
      else if (mode === 'static') { s.height = Math.max(w, h * v); }
      else { s.height = Math.max(w, h * v); s.animation = `vk-wave ${(0.42 + rnd(i * 3 + 7) * 0.5).toFixed(2)}s ease-in-out ${(-rnd(i * 5 + 1)).toFixed(2)}s infinite alternate`; }
      bars.push(R.createElement('span', { key: i, style: s }));
    }
    return R.createElement('div', { 'aria-hidden': true, style: { display: 'flex', alignItems: 'center', gap: g, height: h } }, bars);
  }
  function meter(R, o = {}) {
    const n = o.n || 24, h = o.h || 8;
    const row = (fill, extra) => R.createElement('div', { style: Object.assign({ display: 'flex', gap: 3 }, extra || {}) },
      Array.from({ length: n }, (_, i) => R.createElement('span', { key: i, style: { flex: 1, height: h, borderRadius: 2, background: fill(i) } })));
    return R.createElement('div', { 'aria-hidden': true, style: { position: 'relative', width: '100%' } },
      row(() => 'var(--line2)'),
      row((i) => (i >= n * 0.84 ? 'var(--warn)' : 'var(--ac)'), { position: 'absolute', inset: 0, clipPath: 'inset(0 40% 0 0)', animation: `vk-level ${o.dur || 2.6}s ease-in-out infinite alternate` }));
  }
  function logo(R, s = 24, tile = true) {
    const bars = [[0.06, 0.40], [0.25, 0.64], [0.47, 0.94], [0.25, 0.64], [0.06, 0.40]];
    const inner = tile ? s * 0.58 : s;
    const bw = inner * 0.135, gap = (inner - 5 * bw) / 4;
    const kids = bars.map((b, i) => R.createElement('span', { key: i, style: { position: 'absolute', left: i * (bw + gap), top: b[0] * inner, height: (b[1] - b[0]) * inner, width: bw, borderRadius: bw, background: tile ? 'var(--on-ac)' : 'var(--ac)' } }));
    const mark = R.createElement('div', { style: { position: 'relative', width: inner, height: inner, flex: 'none' } }, kids);
    if (!tile) return R.createElement('div', { 'aria-hidden': true, style: { flex: 'none', display: 'flex' } }, mark);
    return R.createElement('div', { 'aria-hidden': true, style: { width: s, height: s, borderRadius: s * 0.28, background: 'var(--ac)', display: 'flex', alignItems: 'center', justifyContent: 'center', flex: 'none' } }, mark);
  }
  function rings(R, inset = 20) {
    const r = (d) => R.createElement('span', { key: d, style: { position: 'absolute', inset, borderRadius: '50%', border: '2px solid var(--ac)', opacity: 0.5, animation: `vk-pulse 1.8s cubic-bezier(.2,.8,.2,1) ${d}s infinite` } });
    return R.createElement(R.Fragment, null, r(0), r(0.9));
  }
  function spinner(R, inset = 12, w = 2) {
    return R.createElement('span', { style: { position: 'absolute', inset, borderRadius: '50%', border: `${w}px solid var(--line2)`, borderTopColor: 'var(--ac)', animation: 'vk-spin .9s linear infinite' } });
  }
  window.VK = { P, icons, wave, meter, logo, rings, spinner };
  window.dispatchEvent(new Event('vk-ready'));
})();
