// The window only shows state; recording, recognition and insertion all happen in the Rust side.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const capsule = document.querySelector('.capsule');
const byId = id => document.getElementById(id);
const mic = byId('mic'), status = byId('status'), detail = byId('detail'), level = byId('level'), editor = byId('text');
const micLabels = { recording: '停止錄音', working: '取消辨識', retry: '改用本機辨識' };

let voice = { state: 'idle', message: '', text: '' };
let idleText = '點圖示開始錄音';
// What the Zhuyin input method is typing, once it has said so.
let mode = '';
let recordingSince = 0, clock = 0, noticeTimer = 0;
const key = mic.querySelector('.key');
const still = matchMedia('(prefers-reduced-motion: reduce)');
// Latest microphone level (square-rooted) and what the bars currently show.
let heard = 0, shown = 0, frame = 0, tapTimer = 0;

// Fast attack, slower release, so the bars fall back instead of flickering.
function follow() {
  shown = Math.max(heard, shown * 0.85);
  key.style.setProperty('--level', shown.toFixed(3));
  frame = requestAnimationFrame(follow);
}

function startBars() {
  heard = shown = 0;
  if (!still.matches && !frame) frame = requestAnimationFrame(follow);
}

function stopBars() {
  cancelAnimationFrame(frame);
  frame = 0;
  key.style.removeProperty('--level');
}

// The keycap dips once, as if the key had been pressed.
function tap() {
  if (still.matches) return;
  clearTimeout(tapTimer);
  mic.classList.add('tap');
  tapTimer = setTimeout(() => mic.classList.remove('tap'), 140);
}

function elapsed() {
  const seconds = Math.floor((Date.now() - recordingSince) / 1000);
  return `錄音中 ${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`;
}

function render() {
  const { state, message, text } = voice;
  capsule.dataset.state = state;
  // With a result, the text is the main line and what happened to it sits above in small type.
  // The open editor already shows the text, so the bar then says only what happened.
  const textInBar = state === 'done' && capsule.dataset.layout !== 'editor';
  // The pill is short while dictating, so "click again to cancel" moves to the tooltip.
  const said = state === 'working' ? message.split('・')[0] : message;
  status.textContent = textInBar ? text : state === 'recording' ? elapsed() : said || idleText;
  status.title = textInBar ? text : state === 'recording' || state === 'working' ? message : '';
  detail.textContent = textInBar ? message : state === 'idle' && mode ? `注音輸入法：${mode}` : '';
  mic.setAttribute('aria-pressed', String(state === 'recording'));
  mic.setAttribute('aria-label', micLabels[state] || '開始錄音');
  mic.title = mic.getAttribute('aria-label');
  if (state !== 'recording') level.style.width = '0';
}

function receive(next) {
  const was = voice.state;
  if (next.state === 'recording' && was !== 'recording') {
    recordingSince = Date.now();
    clock = setInterval(render, 500);
    startBars();
    tap();
  } else if (next.state !== 'recording') {
    clearInterval(clock);
    if (was === 'recording') { stopBars(); tap(); }
    else if (next.state === 'done' && was !== 'done') tap();
  }
  // A new dictation stops a fold that was under way, and one the page was about to start.
  if (next.state === 'recording' || next.state === 'working') {
    round++;
    capsule.classList.remove('closing', 'card-closing');
  }
  voice = next;
  editor.value = next.text;
  render();
}

// A short confirmation, such as "copied", that gives the line back afterwards.
function notice(text) {
  clearTimeout(noticeTimer);
  (detail.textContent ? detail : status).textContent = text;
  noticeTimer = setTimeout(render, 2000);
}

// Recent dictations, newest first; choosing one brings its text back into the editor. A refined dictation is
// followed by the text as it was before refinement, so the two can be compared.
const recent = byId('history');
const shortened = (text) => (text.length > 18 ? text.slice(0, 18) + '…' : text);
async function loadRecent() {
  const items = await invoke('history_list');
  recent.replaceChildren(new Option('最近紀錄…', ''), ...items.flatMap((item, index) => {
    const time = new Date(item.at * 1000).toLocaleTimeString('zh-TW', { hour: '2-digit', minute: '2-digit', hour12: false });
    const options = [new Option(`${time}${item.inserted ? '' : '（未貼上）'} ${shortened(item.text)}`, String(index))];
    if (item.before) options.push(new Option(`　└ 潤稿前 ${shortened(item.before)}`, `${index}:before`));
    return options;
  }));
}
recent.addEventListener('change', () => {
  if (recent.value !== '') {
    const [index, which] = recent.value.split(':');
    invoke('history_use', { index: Number(index), before: which === 'before' });
  }
  recent.value = '';
});

// Which edge the keycap is on, and whether the editor card opens above the pill.
function setSide(side) {
  capsule.dataset.side = side.left ? 'left' : 'right';
  capsule.dataset.above = String(side.above);
}

// Everything folds back the way it came: the card into the pill, then the pill into the keycap.
// The window only shrinks once the fold has played, so these times match style.css and layout.rs.
const CARD_MS = 140, PILL_MS = 200;
const wait = ms => new Promise(done => setTimeout(done, ms));
const timers = { grow: 0, card: 0 };
// Counts the put-aways asked for; a newer one, a new dictation or typing makes an older one stand down.
let round = 0;

// Plays the fold from the current shape towards `to`; resolves when the window may shrink.
async function fold(to) {
  const from = capsule.dataset.layout;
  if (still.matches || from === to || from === 'docked') return;
  capsule.classList.remove('opening', 'card-opening');
  if (from === 'editor') {
    capsule.classList.add('card-closing');
    await wait(CARD_MS);
  }
  if (to === 'docked') {
    capsule.style.setProperty('--shown', '72px');
    capsule.classList.add('closing');
    await wait(PILL_MS);
  }
}

// Folds the window back to `to`, after `delay` so a confirmation such as "copied" can be read.
async function putAway(to, delay = 0) {
  const mine = ++round;
  if (delay) await wait(delay);
  if (mine !== round || capsule.dataset.layout === to) return;
  await fold(to);
  if (mine === round) invoke('arrange', { shape: to });
}

// Back to the keycap or the pill, whichever the user had before the dictation opened the window.
async function goHome(delay = 0) {
  putAway(await invoke('home'), delay);
}

// The window has already grown when this runs, so the pill is clipped back to its old width and opens from there.
let width = innerWidth;
addEventListener('resize', () => {
  const from = width;
  width = innerWidth;
  if (innerWidth <= from || still.matches) return;
  capsule.classList.remove('closing', 'card-closing', 'opening');
  capsule.style.setProperty('--shown', `${from}px`);
  void capsule.offsetWidth;
  capsule.classList.add('opening');
  clearTimeout(timers.grow);
  timers.grow = setTimeout(() => capsule.classList.remove('opening'), PILL_MS + 60);
});

function setLayout(shape) {
  const was = capsule.dataset.layout;
  capsule.classList.remove('closing', 'card-closing');
  // The card unrolls out of the pill, downwards or upwards, as it folds back in.
  if (shape === 'editor' && was !== 'editor' && !still.matches) {
    capsule.classList.add('card-opening');
    clearTimeout(timers.card);
    timers.card = setTimeout(() => capsule.classList.remove('card-opening'), PILL_MS + 60);
  }
  capsule.dataset.layout = shape;
  render();
  if (shape === 'editor') { loadRecent(); editor.focus(); }
}

// Copying is usually the last thing done with the text, so the window then folds back by itself.
async function copyAndPutAway() {
  if (await invoke('copy_preview')) goHome(700);
}

// Dragging the button moves the window; a press that stays put is a click.
const appWindow = window.__TAURI__.window.getCurrentWindow();
let pressAt = null, dragged = false;
mic.addEventListener('pointerdown', event => {
  if (event.button !== 0) return;
  pressAt = { x: event.screenX, y: event.screenY };
  dragged = false;
});
mic.addEventListener('pointermove', event => {
  if (!pressAt || dragged || !(event.buttons & 1)) return;
  if (Math.hypot(event.screenX - pressAt.x, event.screenY - pressAt.y) < 5) return;
  dragged = true;
  pressAt = null;
  appWindow.startDragging();
});
mic.addEventListener('pointerup', () => { pressAt = null; });
mic.addEventListener('click', event => {
  if (dragged) { dragged = false; event.preventDefault(); return; }
  invoke('toggle_recording');
});
// The docked button has no others beside it, so a right-click opens the full bar.
mic.addEventListener('contextmenu', event => { event.preventDefault(); invoke('arrange', { shape: 'bar' }); });
byId('copy').addEventListener('click', copyAndPutAway);
byId('copy-all').addEventListener('click', copyAndPutAway);
byId('edit').addEventListener('click', () => { round++; invoke('arrange', { shape: 'editor' }); });
byId('collapse').addEventListener('click', () => goHome());
byId('dock').addEventListener('click', () => putAway('docked'));
byId('discard').addEventListener('click', () => invoke('discard_result'));
byId('convert').addEventListener('click', () => invoke('convert_clipboard_script'));
byId('settings').addEventListener('click', () => invoke('open_settings'));
editor.addEventListener('input', () => { round++; voice.text = editor.value; invoke('set_preview', { text: editor.value }); });
document.addEventListener('keydown', event => { if (event.key === 'Escape' && capsule.dataset.layout === 'editor') goHome(); });

listen('voice', event => receive(event.payload));
listen('notice', event => notice(event.payload));
listen('layout', event => setLayout(event.payload));
listen('side', event => setSide(event.payload));
listen('closing', () => fold('docked'));
listen('look', event => applyLook(event.payload));
listen('mode', event => { mode = event.payload; render(); });
// Square root keeps quiet speech visible on the meter.
listen('level', event => {
  heard = Math.sqrt(Math.min(1, Math.max(0, Number(event.payload) || 0)));
  level.style.width = Math.round(heard * 100) + '%';
});

function applyLook(look) {
  const root = document.documentElement;
  if (look.appearance === 1) root.dataset.theme = 'light';
  else if (look.appearance === 2) root.dataset.theme = 'dark';
  else delete root.dataset.theme;
  for (const [name, value] of [['--accent', look.accent?.[0]], ['--on-accent', look.accent?.[1]]]) {
    if (value) root.style.setProperty(name, value); else root.style.removeProperty(name);
  }
  root.style.setProperty('--preview-size', look.text_size + 'px');
  idleText = look.shortcut === '停用' ? '點圖示開始錄音' : `按 ${look.shortcut} 或點圖示開始錄音`;
  render();
}

invoke('facts').then(facts => {
  if (facts.platform === 'macos') document.getElementById('paste-last-hint').textContent = 'Option+Shift+Z 再貼上一則';
  mode = facts.mode;
  applyLook(facts);
  setSide(facts.side);
  setLayout(facts.layout);
  receive({ state: facts.state, message: facts.message, text: facts.text });
});
