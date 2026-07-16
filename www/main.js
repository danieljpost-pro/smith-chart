// JS shell for the Rust/WASM Smith chart: DOM events, sidebar UI, sharing.
// All math and rendering happen inside the wasm module.

import init, { SmithChart } from './pkg/smith_chart.js';

let chart;
const $ = (id) => document.getElementById(id);
const canvas = $('chart');
const wrap = $('chart-wrap');

// ---------- rendering ----------

let renderQueued = false;
function schedule() {
  if (renderQueued) return;
  renderQueued = true;
  requestAnimationFrame(() => {
    renderQueued = false;
    chart.render();
    $('zoom').textContent = `zoom ${fmtNum(chart.zoom_level(), 3)}×`;
  });
}

// ---------- formatting ----------

function fmtNum(v, sig = 4) {
  if (v === null || v === undefined || !isFinite(v)) return '∞';
  if (v === 0) return '0';
  const a = Math.abs(v);
  if (a >= 1e6 || a < 1e-4) return v.toExponential(sig - 1);
  return parseFloat(v.toPrecision(sig)).toString();
}

function fmtComplex(re, im, unit = '') {
  if (re === null || im === null) return '∞';
  const sign = im < 0 ? '−' : '+';
  return `${fmtNum(re)} ${sign} j${fmtNum(Math.abs(im))}${unit ? ' ' + unit : ''}`;
}

function fmtFreq(hz) {
  if (hz === null || hz === undefined) return '';
  const units = [[1e9, 'GHz'], [1e6, 'MHz'], [1e3, 'kHz'], [1, 'Hz']];
  for (const [m, u] of units) if (hz >= m) return `${fmtNum(hz / m)} ${u}`;
  return `${fmtNum(hz)} Hz`;
}

// ---------- readout panel ----------

function updateReadout(json) {
  const r = JSON.parse(json);
  $('ro-gamma').textContent = fmtComplex(r.gamma_re, r.gamma_im);
  $('ro-polar').textContent = `${fmtNum(r.gamma_mag)} ∠ ${fmtNum(r.gamma_deg)}°`;
  $('ro-z').textContent = fmtComplex(r.z_re, r.z_im, 'Ω');
  $('ro-zn').textContent = fmtComplex(r.r, r.x);
  $('ro-y').textContent = fmtComplex(r.y_re_ms, r.y_im_ms, 'mS');
  $('ro-vswr').textContent = fmtNum(r.vswr);
  $('ro-rl').textContent = `${fmtNum(r.return_loss_db)} dB / ${fmtNum(r.mismatch_loss_db)} dB`;
  $('ro-wl').textContent = `${fmtNum(r.wtg)} λ / ${fmtNum(r.wtl)} λ`;
  $('snapline').textContent = r.snap
    ? `▸ ${r.snap.name} ${r.snap.param} @ ${fmtFreq(r.snap.freq_hz)}`
    : '';
}

// ---------- pointer interaction ----------

const pointers = new Map();
let dragDist = 0;

canvas.addEventListener('pointerdown', (e) => {
  canvas.setPointerCapture(e.pointerId);
  pointers.set(e.pointerId, { x: e.offsetX, y: e.offsetY });
  dragDist = 0;
});

canvas.addEventListener('pointermove', (e) => {
  const prev = pointers.get(e.pointerId);
  if (prev) {
    const dx = e.offsetX - prev.x;
    const dy = e.offsetY - prev.y;
    if (pointers.size === 1) {
      chart.pan(dx, dy);
      dragDist += Math.hypot(dx, dy);
    } else if (pointers.size === 2) {
      // Pinch zoom: scale about the midpoint, then follow it.
      const pts = [...pointers.values()];
      const before = Math.hypot(pts[0].x - pts[1].x, pts[0].y - pts[1].y);
      pointers.set(e.pointerId, { x: e.offsetX, y: e.offsetY });
      const now = [...pointers.values()];
      const after = Math.hypot(now[0].x - now[1].x, now[0].y - now[1].y);
      const mx = (now[0].x + now[1].x) / 2;
      const my = (now[0].y + now[1].y) / 2;
      if (before > 0) chart.zoom_at(mx, my, after / before);
      chart.pan(dx / 2, dy / 2);
      dragDist += 10;
      schedule();
      return;
    }
    pointers.set(e.pointerId, { x: e.offsetX, y: e.offsetY });
    schedule();
  } else {
    updateReadout(chart.set_hover(e.offsetX, e.offsetY));
    schedule();
  }
});

canvas.addEventListener('pointerup', (e) => {
  pointers.delete(e.pointerId);
  if (dragDist < 5 && e.button === 0) {
    refreshMarkers(chart.add_marker(e.offsetX, e.offsetY));
    schedule();
  }
});
canvas.addEventListener('pointercancel', (e) => pointers.delete(e.pointerId));
canvas.addEventListener('pointerleave', () => {
  chart.clear_hover();
  $('snapline').textContent = '';
  schedule();
});

canvas.addEventListener('wheel', (e) => {
  e.preventDefault();
  const factor = Math.exp(-e.deltaY * (e.deltaMode === 1 ? 0.05 : 0.0016));
  chart.zoom_at(e.offsetX, e.offsetY, factor);
  updateReadout(chart.set_hover(e.offsetX, e.offsetY));
  schedule();
}, { passive: false });

canvas.addEventListener('dblclick', (e) => {
  chart.zoom_at(e.offsetX, e.offsetY, 2.2);
  schedule();
});

window.addEventListener('keydown', (e) => {
  if (e.target.tagName === 'INPUT') return;
  const cx = canvas.clientWidth / 2, cy = canvas.clientHeight / 2;
  if (e.key === '+' || e.key === '=') chart.zoom_at(cx, cy, 1.4);
  else if (e.key === '-' || e.key === '_') chart.zoom_at(cx, cy, 1 / 1.4);
  else if (e.key === '0' || e.key === 'Home') chart.reset_view();
  else return;
  schedule();
});

// ---------- markers ----------

function refreshMarkers(json) {
  const items = JSON.parse(json ?? chart.markers_json());
  const ul = $('marker-list');
  ul.innerHTML = '';
  for (const m of items) {
    const li = document.createElement('li');
    const label = document.createElement('span');
    label.className = 'grow mono';
    const f = m.freq_hz ? ` @ ${fmtFreq(m.freq_hz)}` : '';
    label.textContent = `M${m.index + 1}  ${fmtComplex(m.z_re, m.z_im, 'Ω')}${f}  VSWR ${fmtNum(m.vswr, 3)}`;
    const del = document.createElement('button');
    del.textContent = '✕';
    del.title = 'Remove marker';
    del.onclick = () => { refreshMarkers(chart.remove_marker(m.index)); schedule(); };
    li.append(label, del);
    ul.append(li);
  }
}

$('btn-addmarker').onclick = () => {
  const r = parseFloat($('mk-r').value) || 0;
  const x = parseFloat($('mk-x').value) || 0;
  refreshMarkers(chart.add_marker_impedance(r, x));
  schedule();
};
$('btn-clearmarkers').onclick = () => {
  chart.clear_markers();
  refreshMarkers();
  schedule();
};

// ---------- traces ----------

function refreshTraces(json) {
  const items = JSON.parse(json ?? chart.traces_json());
  const ul = $('trace-list');
  ul.innerHTML = '';
  for (const t of items) {
    const li = document.createElement('li');

    const vis = document.createElement('input');
    vis.type = 'checkbox';
    vis.checked = t.visible;
    vis.title = 'Show/hide trace';
    vis.onchange = () => { chart.set_trace_visible(t.index, vis.checked); schedule(); };

    const chip = document.createElement('span');
    chip.className = 'chip';
    chip.style.background = chart.trace_color(t.index);

    const name = document.createElement('span');
    name.className = 'grow';
    name.title = `${t.points} points · ${fmtFreq(t.f_min_hz)} – ${fmtFreq(t.f_max_hz)} · Z₀ ${t.z0} Ω`;
    name.textContent = t.name;

    li.append(vis, chip, name);

    if (t.params.length > 1) {
      const sel = document.createElement('select');
      t.params.forEach((p, i) => {
        const o = document.createElement('option');
        o.value = i; o.textContent = p;
        if (i === t.param) o.selected = true;
        sel.append(o);
      });
      sel.onchange = () => { chart.set_trace_param(t.index, +sel.value); schedule(); };
      li.append(sel);
    }

    const del = document.createElement('button');
    del.textContent = '✕';
    del.title = 'Remove trace';
    del.onclick = () => { refreshTraces(chart.remove_trace(t.index)); schedule(); };
    li.append(del);
    ul.append(li);
  }
}

function setStatus(msg, isError = false) {
  const el = $('status');
  el.textContent = msg;
  el.style.color = isError ? '#d03b3b' : '';
  if (isError) setTimeout(() => setStatus('drag to pan · scroll to zoom · click to mark · 0 resets'), 6000);
}

async function loadFiles(files) {
  for (const file of files) {
    try {
      const text = await file.text();
      refreshTraces(chart.add_touchstone(file.name, text));
      setStatus(`loaded ${file.name}`);
    } catch (err) {
      setStatus(`${file.name}: ${err}`, true);
    }
  }
  schedule();
}

$('btn-load').onclick = () => $('file-input').click();
$('file-input').onchange = (e) => { loadFiles(e.target.files); e.target.value = ''; };
$('btn-demo').onclick = async () => {
  try {
    const resp = await fetch('examples/series-rlc.s1p');
    refreshTraces(chart.add_touchstone('series-rlc.s1p', await resp.text()));
    schedule();
  } catch (err) {
    setStatus(`demo failed: ${err}`, true);
  }
};

let dragDepth = 0;
wrap.addEventListener('dragenter', (e) => { e.preventDefault(); if (++dragDepth) wrap.classList.add('dragging'); });
wrap.addEventListener('dragover', (e) => e.preventDefault());
wrap.addEventListener('dragleave', () => { if (--dragDepth <= 0) { dragDepth = 0; wrap.classList.remove('dragging'); } });
wrap.addEventListener('drop', (e) => {
  e.preventDefault();
  dragDepth = 0;
  wrap.classList.remove('dragging');
  loadFiles(e.dataTransfer.files);
});

// ---------- display options ----------

function bindOption(id, setter, isNumber = false) {
  $(id).addEventListener('change', (e) => {
    setter(isNumber ? parseFloat(e.target.value) || 0 : e.target.checked);
    refreshMarkers(); // readouts depend on Z0
    schedule();
  });
}
bindOption('opt-imp', (v) => chart.set_show_impedance(v));
bindOption('opt-adm', (v) => chart.set_show_admittance(v));
bindOption('opt-labels', (v) => chart.set_show_labels(v));
bindOption('opt-vswr', (v) => chart.set_show_vswr(v));
bindOption('opt-q', (v) => chart.set_q(v), true);
bindOption('opt-z0', (v) => chart.set_z0(v), true);

function applyTheme(dark) {
  document.documentElement.dataset.theme = dark ? 'dark' : 'light';
  chart.set_dark(dark);
}
$('btn-theme').onclick = () => {
  applyTheme(document.documentElement.dataset.theme !== 'dark');
  refreshTraces();
  schedule();
};

function syncControls() {
  const o = JSON.parse(chart.options_json());
  $('opt-imp').checked = o.show_impedance;
  $('opt-adm').checked = o.show_admittance;
  $('opt-labels').checked = o.show_labels;
  $('opt-vswr').checked = o.show_vswr;
  $('opt-q').value = o.q > 0 ? o.q : '';
  $('opt-z0').value = o.z0;
  document.documentElement.dataset.theme = o.dark ? 'dark' : 'light';
}

// ---------- toolbar ----------

$('btn-reset').onclick = () => { chart.reset_view(); schedule(); };

$('btn-png').onclick = () => {
  canvas.toBlob((blob) => {
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = 'smith-chart.png';
    a.click();
    URL.revokeObjectURL(a.href);
  });
};

// ---------- sharing (URL fragment, no server) ----------

function bytesToB64url(bytes) {
  let bin = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(bin).replaceAll('+', '-').replaceAll('/', '_').replace(/=+$/, '');
}

function b64urlToBytes(s) {
  const bin = atob(s.replaceAll('-', '+').replaceAll('_', '/'));
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
}

async function makeShareUrl() {
  const bytes = new TextEncoder().encode(chart.state_json());
  let payload = bytes, tag = 'r';
  if (typeof CompressionStream !== 'undefined') {
    const stream = new Blob([bytes]).stream().pipeThrough(new CompressionStream('deflate-raw'));
    payload = new Uint8Array(await new Response(stream).arrayBuffer());
    tag = 'd';
  }
  return `${location.origin}${location.pathname}#${tag}${bytesToB64url(payload)}`;
}

async function loadFromHash() {
  const h = location.hash.slice(1);
  if (h.length < 2) return;
  try {
    const tag = h[0];
    let bytes = b64urlToBytes(h.slice(1));
    if (tag === 'd') {
      const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('deflate-raw'));
      bytes = new Uint8Array(await new Response(stream).arrayBuffer());
    } else if (tag !== 'r') {
      return;
    }
    chart.load_state(new TextDecoder().decode(bytes));
    setStatus('loaded shared chart from URL');
  } catch (err) {
    setStatus(`could not load shared state: ${err}`, true);
  }
}

$('btn-share').onclick = async () => {
  try {
    const url = await makeShareUrl();
    history.replaceState(null, '', url);
    await navigator.clipboard.writeText(url);
    setStatus(url.length > 60000
      ? 'link copied — very large; consider fewer trace points'
      : 'share link copied to clipboard');
  } catch {
    setStatus('link is in the address bar (clipboard unavailable)');
  }
};

// ---------- boot ----------

async function main() {
  await init();
  chart = new SmithChart('chart');

  const resize = () => {
    const r = wrap.getBoundingClientRect();
    chart.resize(r.width, r.height, window.devicePixelRatio || 1);
    schedule();
  };
  new ResizeObserver(resize).observe(wrap);
  resize();

  await loadFromHash();
  syncControls();
  refreshMarkers();
  refreshTraces();
  schedule();
}

main().catch((err) => setStatus(`failed to start: ${err}`, true));
