/* Lex Studio — a small pop-up dictionary for translating (Studio family). Pick the two languages, type (or copy) a word,
   get the translations, meanings and forms. The words come from Wiktionary (Rust: lookup.rs); the window only shows.
   A key combination anywhere brings the window up at the mouse pointer with the copied word looked up (main.rs);
   Escape hides it again. Shared theme, layout and language like the other Studio apps (shared.js). */
'use strict';
const $ = s => document.querySelector(s), $$ = s => [...document.querySelectorAll(s)];
const STORE = 'lex-studio.v1';
const invoke = window.__TAURI__ ? window.__TAURI__.core.invoke : null;
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[c]));
const MAC = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
const SV = p => `<svg class="i" viewBox="0 0 24 24">${p}</svg>`;
const ICON = {
  star: SV('<path d="M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8-5.2-2.7-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z"/>'),
  starOn: '<svg class="i" viewBox="0 0 24 24" style="fill:currentColor"><path d="M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8-5.2-2.7-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z"/></svg>',
  copy: SV('<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5a1 1 0 0 0-1-1H5a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h3"/>'),
  web: SV('<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"/>'),
  reload: SV('<path d="M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7"/>'),
  x: SV('<path d="M6 6l12 12M18 6 6 18"/>'),
  back: SV('<path d="M15 6l-6 6 6 6"/>')
};

/* ================= Texts ================= */
let I18N = {en: {}, de: {}}, lang = 'en';
function t(key, vars) {
  let s = (I18N[lang] && I18N[lang][key]) ?? I18N.en[key] ?? key;
  if (vars) for (const k in vars) s = s.split('{' + k + '}').join(vars[k]);
  return s;
}
async function loadTexts() {
  if (invoke) { I18N = await invoke('texts'); return; }
  const get = l => fetch(`../src-tauri/i18n/${l}.json`).then(r => r.json()).catch(() => ({}));
  I18N = {en: await get('en'), de: await get('de')};
}
const LANGS = ['la', 'grc', 'en', 'de', 'fr', 'es', 'ru', 'it', 'sv', 'ja'];
const langName = c => t('l.' + c);
/* Parts of speech from the English Wiktionary get a name in the window's language; the German ones come in German. */
const posName = p => (I18N.en['pos.' + p] ? t('pos.' + p) : p) || t('pos.unknown');

/* ================= Settings and state ================= */
const THEMES = ['dark', 'light', 'github', 'github-dark', 'cli', 'cli-amber', 'cli-cyan', 'cli-magenta', 'cli-white', 'midnight', 'dusk', 'graphite', 'contrast', 'sand', 'rose', 'proton'];
const DEFAULT_KEY = 'CommandOrControl+Alt+Shift+W';
const DEF = {lang: 'auto', theme: 'dark', from: 'la', to: 'de', hotkey: DEFAULT_KEY, atCursor: true, background: true, hideOnBlur: false,
  clipboard: true, suggest: true, translit: true, autoScript: true, pinned: false};
let settings = {...DEF}, history = [], saved = [], booted = false;
const effectiveLang = () => settings.lang === 'auto' ? ((navigator.language || 'en').toLowerCase().startsWith('de') ? 'de' : 'en') : settings.lang;
function saveState() { try { localStorage.setItem(STORE, JSON.stringify({settings, history, saved})); } catch (e) {} }

let view = 'home', current = null, error = null, busy = false, lastClip = '', token = 0;

/* ================= Looking up ================= */
/* What is actually looked up: Latin letters typed for Greek / Russian / Japanese become that script. */
function queryOf(text, from = settings.from) {
  const conv = settings.translit && window.Translit ? Translit.convert(text, from) : null;
  return conv || String(text).trim();
}
/* The script tells the language: Greek → Ancient Greek, Cyrillic → Russian, kana / kanji → Japanese. */
function scriptOf(s) {
  if (/[Ͱ-Ͽἀ-῿]/.test(s)) return 'grc';
  if (/[Ѐ-ӿ]/.test(s)) return 'ru';
  if (/[぀-ヿ㐀-䶿一-鿿]/.test(s)) return 'ja';
  return null;
}
function followScript(text) {
  const sc = settings.autoScript ? scriptOf(text) : null;
  if (!sc || sc === settings.from) return;
  if (settings.to === sc) settings.to = settings.from;
  settings.from = sc;
  toast(t('toast.fromAuto', {lang: langName(sc)}));
  renderLangs(); saveState();
}

async function look(text, opts = {}) {
  const from = opts.from || settings.from, to = opts.to || settings.to;
  const raw = String(text || '').trim();
  if (!raw) return;
  if (opts.from && opts.to && (from !== settings.from || to !== settings.to)) { settings.from = from; settings.to = to; renderLangs(); }
  if ($('#q').value.trim() !== raw) $('#q').value = raw;
  const word = queryOf(raw, from);
  closeSugg(); showTranslit();
  const my = ++token;
  busy = true; error = null; view = 'result'; render(); status();
  try {
    if (!invoke) throw new Error('offline');
    const r = await invoke('lookup', {word, from, to, fresh: !!opts.fresh});
    if (my !== token) return;
    current = r;
    if (r.sections.length) addHistory(r.title || word, from, to);
  } catch (e) {
    if (my !== token) return;
    current = null; error = {code: String(e && e.message || e), word, from, to};
  }
  busy = false; render(); status(); saveState();
}
function addHistory(w, from, to) {
  history = [{w, from, to}, ...history.filter(h => !(h.w === w && h.from === from && h.to === to))].slice(0, 40);
}
const isSaved = (w, from, to) => saved.some(s => s.w === w && s.from === from && s.to === to);

/* The short translation of the current lookup (for the word list): the first words in the target language. */
function summaryOf(r) {
  const words = [];
  for (const s of ordered(r)) {
    for (const g of s.translations) for (const w of g.words) if (!words.includes(w.w)) words.push(w.w);
    if (!s.translations.length && s.mlang === r.to) for (const m of s.meanings) { const p = plain(m); if (p && !words.includes(p)) words.push(p); }
    if (words.length >= 4) break;
  }
  return words.slice(0, 4).join(', ');
}
const plain = segs => segs.map(s => s.t).join('');
/* Sections with something in the target language first; then the basic form; English explanations last. */
function ordered(r) {
  const has = s => s.translations.length || (s.mlang === r.to && s.meanings.length);
  const main = r.sections.filter(s => !s.lemma_of), lemma = r.sections.filter(s => s.lemma_of);
  return [...main.filter(has), ...main.filter(s => !has(s)), ...lemma.filter(has), ...lemma.filter(s => !has(s))];
}

/* ================= Drawing ================= */
function render() {
  const out = $('#out');
  $('#listBtn').classList.toggle('active', view === 'list');
  if (view === 'list') { out.innerHTML = listHtml(); return; }
  if (view === 'home' || (!busy && !current && !error)) { out.innerHTML = homeHtml(); return; }
  if (busy) { out.innerHTML = `<div class="empty"><span class="spin"></span>${esc(t('ui.looking'))}</div>`; return; }
  if (error) { out.innerHTML = errorHtml(error); return; }
  out.innerHTML = resultHtml(current);
  out.scrollTop = 0;
}

function homeHtml() {
  const hist = history.filter(h => h.from === settings.from || h.to === settings.from).slice(0, 18);
  const chips = (list) => list.map((h, i) => `<button class="chip" data-hist="${i}" data-w="${esc(h.w)}" data-from="${h.from}" data-to="${h.to}">${esc(h.w)}<small>${h.from}→${h.to}</small></button>`).join('');
  return `<div class="home">
    <div class="big-tile">Lx</div>
    <div class="hello">${esc(t('home.hello', {from: langName(settings.from), to: langName(settings.to)}))}</div>
    ${hist.length ? `<h3>${esc(t('home.recent'))}<button class="btn-sm" data-act="clearHistory">${esc(t('home.clear'))}</button></h3><div class="chips">${chips(hist)}</div>` : ''}
    <div class="tip">${t('home.tip', {key: `<span class="kbd">${esc(keyLabel(settings.hotkey) || '—')}</span>`})}<br>
      ${esc(t('home.tipScript'))}</div>
  </div>`;
}

function errorHtml(e) {
  const msg = {offline: t('err.offline'), 'rate-limit': t('err.rateLimit'), empty: t('err.empty'), language: t('err.language')}[e.code] || e.code;
  return `<div class="empty">${esc(msg)}<div class="btns" style="justify-content:center"><button class="btn-sm primary" data-act="retry">${esc(t('ui.retry'))}</button></div></div>`;
}

function segsHtml(segs, mlang) {
  return segs.map(s => s.l ? `<a class="lk" data-look="${esc(s.l)}" data-lang="${mlang}">${esc(s.t)}</a>` : esc(s.t)).join('');
}

function resultHtml(r) {
  const word = r.title || r.word;
  if (!r.sections.length) {
    const sug = r.suggestions.length ? `<p>${esc(t('res.similar'))}</p><div class="chips" style="justify-content:center">${r.suggestions.map(s => `<button class="chip" data-w="${esc(s)}">${esc(s)}</button>`).join('')}</div>` : '';
    return `<div class="empty">${t('res.none', {word: `<b>${esc(r.word)}</b>`, lang: esc(langName(r.from))})}${sug}
      <div class="btns" style="justify-content:center"><button class="btn-sm" data-act="swapLook">${esc(t('res.otherWay', {from: langName(r.to), to: langName(r.from)}))}</button>
      <button class="btn-sm" data-act="webSearch">${esc(t('res.web'))}</button></div></div>`;
  }
  const on = isSaved(word, r.from, r.to);
  let html = `<div class="wordhead"><h1>${esc(word)}<small>${esc(langName(r.from))} → ${esc(langName(r.to))}${r.word !== word ? ' · ' + esc(t('res.typed', {word: r.word})) : ''}</small></h1>
    <button class="ibtn${on ? ' active' : ''}" data-act="save" title="${esc(t(on ? 'a.unsave' : 'a.save'))}">${on ? ICON.starOn : ICON.star}</button>
    <button class="ibtn" data-act="copyAll" title="${esc(t('a.copyAll'))}">${ICON.copy}</button>
    <button class="ibtn" data-act="refresh" title="${esc(t('a.refresh'))}">${ICON.reload}</button></div>`;
  if (r.cached) html += '';
  const secs = ordered(r);
  const anyTarget = secs.some(s => s.translations.length || (s.mlang === r.to && s.meanings.length));
  if (!anyTarget) html += `<div class="note warn">${esc(t('res.noTarget', {lang: langName(r.to)}))}</div>`;
  let lemmaShown = false;
  for (const s of secs) {
    if (s.lemma_of && !lemmaShown) { lemmaShown = true; html += `<div class="divider">${esc(t('res.lemma', {word: s.title}))}</div>`; }
    const primary = s.translations.length || (s.mlang === r.to && s.meanings.length);
    const tags = [];
    if (s.via) tags.push(`<span class="tag via" title="${esc(t('res.viaHint'))}">${esc(t('res.via', {word: s.via}))}</span>`);
    if (!primary && s.mlang !== r.to && s.meanings.length) tags.push(`<span class="tag">${esc(t('res.explained', {lang: langName(s.mlang)}))}</span>`);
    html += `<div class="card${primary ? ' primary' : ' extra'}"><div class="card-h"><span class="pos">${esc(posName(s.pos))}</span>${s.lemma_of ? `<span class="tag">${esc(s.title)}</span>` : ''}${tags.join('')}</div>`;
    for (const g of s.translations) {
      html += `<div class="grp">${g.gloss ? `<div class="gloss">${esc(g.gloss)}</div>` : ''}<div class="words">` +
        g.words.map(w => `<button class="w" data-word="${esc(w.w)}" title="${esc(t('res.wordHint'))}">${esc(w.w)}${w.note ? `<small>${esc(w.note)}</small>` : ''}</button>`).join('') + '</div></div>';
    }
    if (s.meanings.length) {
      if (s.translations.length) html += `<div class="mlabel">${esc(t('res.meanings', {lang: langName(s.mlang)}))}</div>`;
      const shown = primary ? s.meanings : s.meanings.slice(0, 6);
      html += `<ol class="mean">${shown.map(m => `<li>${segsHtml(m, s.mlang)}</li>`).join('')}</ol>`;
    }
    if (s.forms.length) {
      html += `<details class="forms"><summary>${esc(t('res.forms', {n: s.forms.length}))}</summary><div class="ftable">${s.forms.map(f => `<span>${esc(f[0])}</span><b>${esc(f[1])}</b>`).join('')}</div></details>`;
    }
    html += '</div>';
  }
  html += `<div class="sources">${esc(t('res.source'))} ${r.sources.map(s => `<a data-url="${esc(s.url)}">${esc((s.wiki === 'de' ? 'de' : 'en') + '.wiktionary: ' + s.title)}</a>`).join(' · ')}
    ${r.cached ? '<br>' + esc(t('res.cached')) : ''}${r.partial ? '<br>' + esc(t('res.partial')) : ''}</div>`;
  return html;
}

function listHtml() {
  if (!saved.length) return `<div class="list"><div class="empty">${t('list.empty')}</div></div>`;
  return `<div class="list"><h3>${esc(t('list.title', {n: saved.length}))}</h3>` + saved.map((s, i) =>
    `<div class="row" data-saved="${i}"><div><span class="rw">${esc(s.w)}</span><span class="rl">${s.from}→${s.to}</span><div class="rt">${esc(s.tr || '–')}</div></div>
     <button class="ibtn" data-unsave="${i}" title="${esc(t('a.unsave'))}">${ICON.x}</button></div>`).join('') +
    `<div class="btns"><button class="btn-sm primary" data-act="exportCsv">${esc(t('list.export'))}</button><button class="btn-sm" data-act="copyList">${esc(t('list.copy'))}</button>
     <button class="btn-sm" data-act="clearList">${esc(t('list.clear'))}</button></div><p class="tip">${esc(t('list.hint'))}</p></div>`;
}

function renderLangs() {
  const opts = c => LANGS.map(l => `<option value="${l}"${l === c ? ' selected' : ''}>${esc(langName(l))}</option>`).join('');
  $('#from').innerHTML = opts(settings.from);
  $('#to').innerHTML = opts(settings.to);
  showTranslit();
}
function showTranslit() {
  const raw = $('#q').value, conv = settings.translit && window.Translit ? Translit.convert(raw, settings.from) : null;
  const n = $('#translit');
  n.hidden = !conv;
  if (conv) n.innerHTML = `${esc(t('ui.translit'))} <b>${esc(conv)}</b>`;
  $('#clearQ').hidden = !raw;
}
function status() {
  const st = $('#stState');
  st.className = 'st-dot' + (busy ? ' busy' : error ? ' off' : current && current.cached ? '' : '');
  st.textContent = busy ? t('st.busy') : error ? t('st.error') : current && current.cached ? t('st.cached') : t('st.ready');
  $('#stKey').textContent = settings.hotkey ? t('st.key', {key: keyLabel(settings.hotkey)}) : '';
  const c = $('#listCount'); c.hidden = !saved.length; c.textContent = saved.length;
  $('#pinBtn').classList.toggle('active', settings.pinned);
}
let toastT = null;
function toast(msg) { const n = $('#toast'); n.textContent = msg; n.classList.add('show'); clearTimeout(toastT); toastT = setTimeout(() => n.classList.remove('show'), 2600); }
function applyTitles() {
  $('#q').placeholder = t('ui.search', {lang: langName(settings.from)});
  const tip = {swap: 'a.swap', pinBtn: 'a.pin', listBtn: 'a.list', setBtn: 'a.settings', clearQ: 'a.clear'};
  for (const [id, k] of Object.entries(tip)) { const el = document.getElementById(id); if (el) el.title = t(k) + (KEYS[el.dataset.act] ? ` (${fmtCombo(KEYS[el.dataset.act])})` : ''); }
  $('#from').title = t('ui.from'); $('#to').title = t('ui.to');
}

/* ================= Suggestions while typing ================= */
let suggT = null, suggList = [], suggSel = -1;
function closeSugg() { $('#sugg').hidden = true; suggList = []; suggSel = -1; }
function drawSugg() {
  const box = $('#sugg');
  if (!suggList.length) { closeSugg(); return; }
  box.innerHTML = suggList.map((s, i) => `<button data-sugg="${i}" class="${i === suggSel ? 'sel' : ''}">${esc(s)}</button>`).join('');
  box.hidden = false;
}
function onType() {
  showTranslit();
  clearTimeout(suggT);
  const raw = $('#q').value.trim();
  if (!raw) { closeSugg(); if (view === 'result') { view = 'home'; current = null; error = null; render(); } return; }
  if (!settings.suggest || !invoke || raw.length < 2) { closeSugg(); return; }
  suggT = setTimeout(async () => {
    const q = queryOf(raw);
    try {
      const list = await invoke('suggest', {prefix: q, from: settings.from});
      if ($('#q').value.trim() !== raw || document.activeElement !== $('#q')) return;
      suggList = list.filter(s => s !== q).slice(0, 8); suggSel = -1; drawSugg();
    } catch (e) { closeSugg(); }
  }, 320);
}

/* ================= Actions ================= */
const A = {
  swap() {
    [settings.from, settings.to] = [settings.to, settings.from];
    renderLangs(); applyTitles(); saveState();
    if (current && current.sections.length) {
      // The other way round: look up the first translation (or the same word, when there is none).
      // canis (la → de) gets its German as meanings („Hund“ as a link) — then the first linked word of those.
      const secs = ordered(current), old = current;
      const first = secs.flatMap(s => s.translations.flatMap(g => g.words.map(w => w.w)))[0] ||
        secs.filter(s => s.mlang === old.to).flatMap(s => s.meanings.flatMap(m => m.filter(x => x.l).map(x => x.l)))[0];
      look(first || $('#q').value);
    } else if ($('#q').value.trim()) look($('#q').value);
    else render();
  },
  pin() { settings.pinned = !settings.pinned; if (invoke) invoke('pin', {on: settings.pinned}).catch(() => {}); toast(t(settings.pinned ? 'toast.pinned' : 'toast.unpinned')); status(); saveState(); },
  list() { view = view === 'list' ? (current || error ? 'result' : 'home') : 'list'; render(); },
  settings() { openSettings(); },
  clear() { $('#q').value = ''; closeSugg(); current = null; error = null; view = 'home'; showTranslit(); render(); status(); $('#q').focus(); },
  focus() { $('#q').focus(); $('#q').select(); },
  retry() { if (error) look($('#q').value || error.word, {from: error.from, to: error.to}); },
  refresh() { if (current) look($('#q').value || current.word, {fresh: true}); },
  swapLook() { if (current) { const w = current.word; [settings.from, settings.to] = [settings.to, settings.from]; renderLangs(); look(w); } },
  webSearch() {
    if (!current) return;
    const wiki = current.from === 'de' ? 'de' : 'en';
    openUrl(`https://${wiki}.wiktionary.org/w/index.php?search=${encodeURIComponent(current.word)}`);
  },
  save() {
    if (!current || !current.sections.length) return;
    const w = current.title || current.word, {from, to} = current;
    if (isSaved(w, from, to)) { saved = saved.filter(s => !(s.w === w && s.from === from && s.to === to)); toast(t('toast.unsaved', {word: w})); }
    else { saved.unshift({w, from, to, tr: summaryOf(current), at: Date.now()}); toast(t('toast.saved', {word: w})); }
    saveState(); render(); status();
  },
  copyAll() {
    if (!current) return;
    const w = current.title || current.word, lines = [w + ' (' + langName(current.from) + ' → ' + langName(current.to) + ')'];
    for (const s of ordered(current)) {
      const parts = [];
      for (const g of s.translations) parts.push((g.gloss ? g.gloss + ': ' : '') + g.words.map(x => x.w).join(', '));
      if (s.mlang === current.to || !s.translations.length) s.meanings.forEach((m, i) => parts.push((i + 1) + '. ' + plain(m)));
      if (parts.length) lines.push('', posName(s.pos) + (s.via ? ' — ' + t('res.via', {word: s.via}) : '') + (s.lemma_of ? ' — ' + s.title : ''), ...parts);
    }
    copy(lines.join('\n'), t('toast.copiedAll'));
  },
  clearHistory() { history = []; saveState(); render(); },
  async exportCsv() {
    if (!saved.length || !invoke) return;
    const q = v => /[;"\n]/.test(v) ? '"' + String(v).replace(/"/g, '""') + '"' : String(v);
    const rows = [[t('csv.word'), t('csv.translation'), t('csv.from'), t('csv.to')], ...saved.map(s => [s.w, s.tr, langName(s.from), langName(s.to)])];
    try {
      const p = await invoke('export_csv', {name: t('csv.name'), label: 'CSV', content: rows.map(r => r.map(q).join(';')).join('\r\n')});
      if (p) toast(t('toast.exported'));
    } catch (e) { toast(String(e)); }
  },
  copyList() { copy(saved.map(s => `${s.w}\t${s.tr}`).join('\n'), t('toast.copiedList')); },
  async clearList() { if (saved.length && await confirmBox(t('list.clearAsk'))) { saved = []; saveState(); render(); status(); } },
  hide() { if (invoke) invoke('hide_window').catch(() => {}); }
};
async function confirmBox(msg) { return window.confirm(msg); }
function copy(text, msg) {
  (invoke ? invoke('clipboard_write', {text}) : navigator.clipboard.writeText(text)).then(() => toast(msg || t('toast.copied', {word: text})), e => toast(String(e)));
  lastClip = text.trim();   // our own copy is not a new word to look up when the window comes back
}
function openUrl(url) { if (invoke) invoke('open_url', {url}).catch(e => toast(String(e))); else window.open(url); }

/* Clicks in the window */
document.addEventListener('click', e => {
  const el = e.target.closest('[data-act],[data-hist],[data-w],[data-word],[data-look],[data-url],[data-sugg],[data-saved],[data-unsave]');
  if (!el || el.closest('.modal')) return;
  if (el.dataset.unsave != null) { saved.splice(+el.dataset.unsave, 1); saveState(); render(); status(); return; }
  if (el.dataset.act) { if (A[el.dataset.act]) A[el.dataset.act](); return; }
  if (el.dataset.sugg != null) { look(suggList[+el.dataset.sugg]); return; }
  if (el.dataset.word) {
    // A translation: copy it; with Ctrl / Cmd look it up the other way round.
    if (e.ctrlKey || e.metaKey) lookBack(el.dataset.word); else copy(el.dataset.word);
    return;
  }
  if (el.dataset.look) { lookLink(el.dataset.look, el.dataset.lang); return; }
  if (el.dataset.url) { openUrl(el.dataset.url); return; }
  if (el.dataset.saved != null) { const s = saved[+el.dataset.saved]; look(s.w, {from: s.from, to: s.to}); return; }
  if (el.dataset.w) look(el.dataset.w, el.dataset.from ? {from: el.dataset.from, to: el.dataset.to} : {});
});
document.addEventListener('dblclick', e => { const el = e.target.closest('[data-word]'); if (el) lookBack(el.dataset.word); });
/* A translation the other way round: canis (la → de) → Hund (de → la). */
function lookBack(word) { const {from, to} = current || settings; look(word, {from: to, to: from}); }
/* A link in a meaning: a word in the meaning's language → back into the source language. */
function lookLink(word, mlang) {
  const {from, to} = current || settings;
  if (mlang === from) look(word, {from, to}); else look(word, {from: mlang, to: from});
}

$('#from').onchange = () => {
  const v = $('#from').value;
  if (v === settings.to) settings.to = settings.from;
  settings.from = v; renderLangs(); applyTitles(); saveState();
  if ($('#q').value.trim()) look($('#q').value); else render();
};
$('#to').onchange = () => {
  const v = $('#to').value;
  if (v === settings.from) settings.from = settings.to;
  settings.to = v; renderLangs(); applyTitles(); saveState();
  if ($('#q').value.trim() && view !== 'home') look($('#q').value); else render();
};
$('#q').addEventListener('input', onType);
$('#q').addEventListener('paste', () => setTimeout(() => { const v = $('#q').value.trim(); if (v) { followScript(v); look(v); } }, 0));
$('#q').addEventListener('keydown', e => {
  if (e.key === 'ArrowDown' && suggList.length) { e.preventDefault(); suggSel = (suggSel + 1) % suggList.length; drawSugg(); return; }
  if (e.key === 'ArrowUp' && suggList.length) { e.preventDefault(); suggSel = (suggSel - 1 + suggList.length) % suggList.length; drawSugg(); return; }
  if (e.key === 'Enter') {
    e.preventDefault();
    const v = suggSel >= 0 ? suggList[suggSel] : $('#q').value;
    followScript(v);
    look(v, {fresh: e.ctrlKey || e.metaKey});
  }
});
$('#q').addEventListener('blur', () => setTimeout(() => { if (document.activeElement !== $('#q')) closeSugg(); }, 150));

/* ================= Keyboard ================= */
/* The window's own keys (fixed; the key that brings the window up is set in the settings). */
const KEYS = {swap: 'Ctrl+K', save: 'Ctrl+D', pin: 'Ctrl+T', list: 'Ctrl+B', settings: 'Ctrl+,', focus: 'Ctrl+L', copyAll: 'Ctrl+Shift+C'};
const fmtCombo = c => MAC ? c.replace('Ctrl+Shift+', '⇧⌘').replace('Ctrl+', '⌘') : (lang === 'de' ? c.replace('Ctrl', 'Strg').replace('Shift', 'Umschalt') : c);
function comboOf(e) {
  let k = e.key.length === 1 ? e.key.toUpperCase() : e.key;
  if (e.code === 'Comma') k = ',';
  return (e.ctrlKey || e.metaKey ? 'Ctrl+' : '') + (e.shiftKey ? 'Shift+' : '') + (e.altKey ? 'Alt+' : '') + k;
}
window.addEventListener('keydown', e => {
  if (recording) return;
  if (e.key === 'Escape') {
    e.preventDefault();
    if (!$('#sugg').hidden) closeSugg();
    else if ($('#modal').classList.contains('show')) closeModal();
    else if (view === 'list') A.list();
    else A.hide();
    return;
  }
  if ($('#modal').classList.contains('show')) return;
  const combo = comboOf(e);
  const act = Object.keys(KEYS).find(a => KEYS[a] === combo);
  if (act) { e.preventDefault(); e.stopPropagation(); A[act](); return; }
  // Typing anywhere goes into the search field.
  if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey && document.activeElement !== $('#q') && !/INPUT|SELECT|TEXTAREA/.test(document.activeElement.tagName)) $('#q').focus();
}, true);

/* ================= The key combination that brings the window up ================= */
/* Stored like Tauri wants it ("CommandOrControl+Alt+Shift+W"); shown like the system does (⌘⌥⇧W / Ctrl+Alt+Shift+W). */
function keyLabel(acc) {
  if (!acc) return '';
  const parts = acc.split('+'), key = parts.pop();
  const mods = parts.map(p => {
    const m = p.toLowerCase();
    if (m === 'commandorcontrol' || m === 'cmdorctrl') return MAC ? '⌘' : (lang === 'de' ? 'Strg' : 'Ctrl');
    if (m === 'command' || m === 'cmd' || m === 'super' || m === 'meta') return MAC ? '⌘' : 'Win';
    if (m === 'control' || m === 'ctrl') return MAC ? '⌃' : (lang === 'de' ? 'Strg' : 'Ctrl');
    if (m === 'alt' || m === 'option') return MAC ? '⌥' : 'Alt';
    if (m === 'shift') return MAC ? '⇧' : (lang === 'de' ? 'Umschalt' : 'Shift');
    return p;
  });
  const k = key === 'Space' ? (lang === 'de' ? 'Leertaste' : 'Space') : key;
  return MAC ? mods.join('') + k : [...mods, k].join('+');
}
/* A pressed key → accelerator; null while only modifiers are held or without a modifier (except F-keys). */
function accelOf(e) {
  let key = null;
  if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3);
  else if (/^Digit\d$/.test(e.code)) key = e.code.slice(5);
  else if (/^F\d{1,2}$/.test(e.code)) key = e.code;
  else if (e.code === 'Space') key = 'Space';
  if (!key) return null;
  const mods = [];
  if (e.metaKey) mods.push(MAC ? 'CommandOrControl' : 'Super');
  if (e.ctrlKey) mods.push(MAC ? 'Control' : 'CommandOrControl');
  if (e.altKey) mods.push('Alt');
  if (e.shiftKey) mods.push('Shift');
  if (!mods.length && !/^F\d/.test(key)) return null;
  if (mods.length === 1 && mods[0] === 'Shift' && !/^F\d/.test(key)) return null;
  return [...mods, key].join('+');
}
let recording = false;
async function applyPopup(showError) {
  if (!invoke) return true;
  try {
    await invoke('popup_config', {hotkey: settings.hotkey || '', atCursor: settings.atCursor, background: settings.background});
    return true;
  } catch (e) {
    const s = String(e);
    if (showError !== false) toast(s.startsWith('taken') ? t('key.taken', {key: keyLabel(settings.hotkey)}) : t('key.invalid'));
    return false;
  }
}

/* ================= Settings ================= */
let prefsSection = 'general', settingsOpen = false;
function openSettings(section) {
  if (section) prefsSection = section;
  settingsOpen = true;
  $('#modalTitle').textContent = t('set.title');
  $('#modal').classList.add('show');
  renderSettings();
}
function closeModal() { $('#modal').classList.remove('show'); settingsOpen = false; recording = false; $('#modalBody').innerHTML = ''; $('#q').focus(); }
$('#modalClose').onclick = () => closeModal();
$('#modal').onmousedown = e => { if (e.target.id === 'modal') closeModal(); };
async function renderSettings() {
  const opt = (v, label, c) => `<option value="${v}"${String(v) === String(c) ? ' selected' : ''}>${esc(label)}</option>`;
  const sw = (name, label, hint) => `<label class="switch"><input type="checkbox" data-sw="${name}"${settings[name] ? ' checked' : ''}><span></span>${esc(label)}</label>${hint ? `<p class="hint">${esc(hint)}</p>` : ''}`;
  const mock = th => `<div class="tc-mock" data-theme="${th}"><div class="tc-bar"><i></i><i></i><b></b></div><div class="tc-ed"><i></i><i style="width:70%"></i><i style="width:50%"></i></div></div>`;
  const cacheN = invoke ? await invoke('cache_info').catch(() => 0) : 0;
  const body = {
    general: `<h3>${esc(t('set.language'))}</h3><p class="hint">${esc(t('set.languageHint'))}</p>
      <select class="wide-sel" data-sel="lang">${opt('auto', t('set.langAuto'), settings.lang)}${opt('en', 'English', settings.lang)}${opt('de', 'Deutsch', settings.lang)}</select>
      <h3>${esc(t('set.typing'))}</h3>
      ${sw('suggest', t('set.suggest'))}${sw('translit', t('set.translit'), t('set.translitHint'))}${sw('autoScript', t('set.autoScript'))}`,
    popup: `<h3>${esc(t('set.hotkey'))}</h3><p class="hint">${esc(t('set.hotkeyHint'))}</p>
      <div class="hotkey"><span class="kbd${recording ? ' rec' : ''}" id="hkShow">${esc(recording ? t('set.hotkeyPress') : (keyLabel(settings.hotkey) || t('set.hotkeyNone')))}</span>
        <button class="btn-sm" data-pref="recordKey">${esc(t(recording ? 'set.cancel' : 'set.hotkeyChange'))}</button>
        <button class="btn-sm" data-pref="defaultKey">${esc(t('set.hotkeyDefault'))}</button><button class="btn-sm" data-pref="noKey">${esc(t('set.hotkeyOff'))}</button></div>
      <h3>${esc(t('set.popup'))}</h3>
      ${sw('clipboard', t('set.clipboard'), t('set.clipboardHint'))}${sw('atCursor', t('set.atCursor'))}${sw('hideOnBlur', t('set.hideOnBlur'))}
      ${sw('background', t('set.background'), t('set.backgroundHint'))}
      <p><button class="btn-sm" data-pref="quit">${esc(t('set.quit'))}</button></p>`,
    appearance: (window.Shared ? Shared.scopePicker() : '') + `<h3>${esc(t('set.theme'))}</h3><div class="theme-grid">` + THEMES.map(th =>
      `<button class="theme-card${settings.theme === th ? ' active' : ''}" data-theme-pick="${th}">${mock(th)}
       <div class="theme-name">${esc(t('theme.' + th))}<span>${settings.theme === th ? '&#10003;' : ''}</span></div></button>`).join('') + '</div>' + (window.Shared ? Shared.layoutHeading() : ''),
    data: `<h3>${esc(t('set.source'))}</h3><p class="hint">${t('set.sourceHint')}</p>
      <h3>${esc(t('set.cache'))}</h3><p class="hint">${esc(t('set.cacheHint', {n: cacheN}))}</p>
      <p><button class="btn-sm" data-pref="clearCache">${esc(t('set.clearCache'))}</button> <button class="btn-sm" data-pref="clearHistory">${esc(t('set.clearHistory'))}</button></p>
      <h3>${esc(t('set.keys'))}</h3><p class="hint">${Object.entries(KEYS).map(([a, k]) => `<span class="kbd">${esc(fmtCombo(k))}</span> ${esc(t('a.' + a))}`).join('<br>')}<br><span class="kbd">Esc</span> ${esc(t('a.hide'))}<br><span class="kbd">${esc(fmtCombo('Ctrl+'))}Enter</span> ${esc(t('a.refresh'))}</p>`
  };
  const sections = ['general', 'popup', 'appearance', 'data'];
  $('#modalBody').innerHTML = `<div class="prefs"><nav>${sections.map(k =>
    `<button data-sec="${k}" class="${k === prefsSection ? 'active' : ''}">${esc(t('set.' + k))}</button>`).join('')}</nav><section>${body[prefsSection]}</section></div>`;
}
$('#modal').addEventListener('click', async e => {
  if (!settingsOpen) return;
  const el = e.target.closest('[data-sec],[data-theme-pick],[data-pref],a[data-url]'); if (!el) return;
  if (el.dataset.url) { openUrl(el.dataset.url); return; }
  if (el.dataset.sec) { prefsSection = el.dataset.sec; recording = false; }
  else if (el.dataset.themePick) { settings.theme = el.dataset.themePick; applySettings(); }
  else {
    const p = el.dataset.pref;
    if (p === 'recordKey') recording = !recording;
    else if (p === 'defaultKey' || p === 'noKey') {
      const old = settings.hotkey; settings.hotkey = p === 'noKey' ? '' : DEFAULT_KEY; recording = false;
      if (!await applyPopup()) settings.hotkey = old;
      saveState(); status();
    }
    else if (p === 'clearCache') { if (invoke) await invoke('cache_clear').catch(() => {}); toast(t('toast.cacheCleared')); }
    else if (p === 'clearHistory') { history = []; saveState(); toast(t('toast.historyCleared')); }
    else if (p === 'quit') { saveState(); if (invoke) invoke('quit'); }
  }
  const sec = $('.prefs section'), top = sec ? sec.scrollTop : 0;
  await renderSettings();
  if (!el.dataset.sec && $('.prefs section')) $('.prefs section').scrollTop = top;
});
$('#modal').addEventListener('change', async e => {
  if (!settingsOpen) return;
  const el = e.target;
  if (el.dataset.sw) settings[el.dataset.sw] = el.checked;
  else if (el.dataset.sel) settings[el.dataset.sel] = el.value;
  else return;
  if (['atCursor', 'background'].includes(el.dataset.sw)) await applyPopup();
  applySettings(); renderSettings();
});
/* Recording the key: the next combination pressed. */
window.addEventListener('keydown', async e => {
  if (!recording) return;
  e.preventDefault(); e.stopPropagation();
  if (e.key === 'Escape') { recording = false; renderSettings(); return; }
  const acc = accelOf(e);
  if (!acc) return;
  recording = false;
  const old = settings.hotkey; settings.hotkey = acc;
  if (await applyPopup()) toast(t('toast.keySet', {key: keyLabel(acc)})); else settings.hotkey = old;
  saveState(); status(); renderSettings();
}, true);

function applyLanguage() {
  document.documentElement.lang = lang;
  renderLangs(); applyTitles();
  if (invoke) invoke('tray_labels', {show: t('tray.show'), quit: t('tray.quit')}).catch(() => {});
  if (booted) { render(); status(); }
}
function applySettings() {
  if (!THEMES.includes(settings.theme)) settings.theme = 'dark';
  if (window.Shared) Shared.push({theme: settings.theme, lang: settings.lang});
  document.documentElement.dataset.theme = settings.theme;
  const nl = effectiveLang();
  if (nl !== lang) { lang = nl; applyLanguage(); }
  if (booted) { status(); showTranslit(); }
  saveState();
}

/* ================= Coming up from anywhere ================= */
/* The key combination (or the tray icon) brought the window up: look up the copied word, ready to type. */
async function summoned() {
  if (settings.clipboard && invoke) {
    const text = String(await invoke('clipboard_read').catch(() => '') || '').trim();
    // A word or a short phrase — not a whole paragraph — and only when it is new.
    if (text && text !== lastClip && text.length <= 60 && !/[\n\r\t]/.test(text) && text.split(/\s+/).length <= 4) {
      lastClip = text;
      followScript(text);
      $('#q').value = text;
      look(text);
    }
  }
  $('#q').focus(); $('#q').select();
}
window.addEventListener('blur', () => {
  if (!settings.hideOnBlur || settings.pinned || settingsOpen || !invoke) return;
  setTimeout(() => { if (!document.hasFocus()) invoke('hide_window').catch(() => {}); }, 200);
});

/* Right-click: native menu on translations and words. */
if (window.Native) {
  let target = null;
  Native.setProvider(e => {
    const w = e.target.closest('[data-word],[data-look]');
    if (!w) return null;
    target = w.dataset.word ? {word: w.dataset.word} : {link: w.dataset.look, lang: w.dataset.lang};
    const word = target.word || target.link;
    return [{id: 'ctx:copy', label: t('ctx.copy', {word})}, {id: 'ctx:look', label: t('ctx.look', {word})}];
  });
  Native.setHandler(id => {
    if (!target) return;
    if (id === 'ctx:copy') copy(target.word || target.link);
    else if (id === 'ctx:look') target.word ? lookBack(target.word) : lookLink(target.link, target.lang);
  });
}

/* Test hook (like the other apps' __md / __pw). */
window.__lx = {A, look, render, get current() { return current; }, set current(v) { current = v; view = 'result'; render(); }, settings: () => settings,
  get saved() { return saved; }, keyLabel, accelOf, summaryOf, ordered, summoned, queryOf, get view() { return view; }};

/* ================= Boot ================= */
(async function boot() {
  await loadTexts();
  let st = null;
  try { st = JSON.parse(localStorage.getItem(STORE)); } catch (e) {}
  if (st) { settings = {...DEF, ...(st.settings || {})}; history = st.history || []; saved = st.saved || []; }
  if (!LANGS.includes(settings.from)) settings.from = DEF.from;
  if (!LANGS.includes(settings.to) || settings.to === settings.from) settings.to = settings.from === 'de' ? 'la' : 'de';
  if (window.Shared) await Shared.init({app: 'lx', apply: sh => {
    if (sh.theme) settings.theme = sh.theme;
    if (sh.lang) settings.lang = sh.lang;
    if (booted) applySettings();
  }});
  lang = effectiveLang();
  applyLanguage();
  booted = true;
  applySettings();
  render(); status();
  if (invoke) {
    await applyPopup();
    if (settings.pinned) invoke('pin', {on: true}).catch(() => {});
    window.__TAURI__.event.listen('summoned', () => summoned());
  }
  $('#q').focus();
})();
