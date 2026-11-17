// The window only shows state; recording, recognition and insertion all happen in the Rust side.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const capsule = document.querySelector('.capsule');
const byId = id => document.getElementById(id);
const mic = byId('mic'), status = byId('status'), detail = byId('detail'), level = byId('level'), editor = byId('text');
const micLabels = { recording: '停止錄音', working: '取消辨識', retry: '改用本機辨識' };

let voice = { state: 'idle', message: '', text: '' };
let idleText = '按麥克風開始';
// What the Zhuyin input method is typing, once it has said so.
let mode = '';
let recordingSince = 0, clock = 0, noticeTimer = 0;

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
  status.textContent = textInBar ? text : state === 'recording' ? elapsed() : message || idleText;
  status.title = textInBar ? text : '';
  detail.textContent = textInBar ? message : state === 'idle' && mode ? `注音輸入法：${mode}` : '';
  mic.setAttribute('aria-pressed', String(state === 'recording'));
  mic.setAttribute('aria-label', micLabels[state] || '開始錄音');
  mic.title = mic.getAttribute('aria-label');
  if (state !== 'recording') level.style.width = '0';
}

function receive(next) {
  if (next.state === 'recording' && voice.state !== 'recording') {
    recordingSince = Date.now();
    clock = setInterval(render, 500);
  } else if (next.state !== 'recording') {
    clearInterval(clock);
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

function setLayout(shape) {
  capsule.dataset.layout = shape;
  render();
  if (shape === 'editor') editor.focus();
}

mic.addEventListener('click', () => invoke('toggle_recording'));
// The docked microphone has no other buttons, so a right-click opens the full bar.
mic.addEventListener('contextmenu', event => { event.preventDefault(); invoke('arrange', { shape: 'bar' }); });
byId('copy').addEventListener('click', () => invoke('copy_preview'));
byId('copy-all').addEventListener('click', () => invoke('copy_preview'));
byId('edit').addEventListener('click', () => invoke('arrange', { shape: 'editor' }));
byId('collapse').addEventListener('click', () => invoke('arrange', { shape: 'bar' }));
byId('dock').addEventListener('click', () => invoke('arrange', { shape: 'docked' }));
byId('discard').addEventListener('click', () => invoke('discard_result'));
byId('settings').addEventListener('click', () => invoke('open_settings'));
editor.addEventListener('input', () => { voice.text = editor.value; invoke('set_preview', { text: editor.value }); });
document.addEventListener('keydown', event => { if (event.key === 'Escape' && capsule.dataset.layout === 'editor') invoke('arrange', { shape: 'bar' }); });

listen('voice', event => receive(event.payload));
listen('notice', event => notice(event.payload));
listen('layout', event => setLayout(event.payload));
listen('look', event => applyLook(event.payload));
listen('mode', event => { mode = event.payload; render(); });
// Square root keeps quiet speech visible on the meter.
listen('level', event => { level.style.width = Math.round(Math.sqrt(event.payload) * 100) + '%'; });

function applyLook(look) {
  const root = document.documentElement;
  if (look.appearance === 1) root.dataset.theme = 'light';
  else if (look.appearance === 2) root.dataset.theme = 'dark';
  else delete root.dataset.theme;
  for (const [name, value] of [['--accent', look.accent?.[0]], ['--on-accent', look.accent?.[1]]]) {
    if (value) root.style.setProperty(name, value); else root.style.removeProperty(name);
  }
  root.style.setProperty('--preview-size', look.text_size + 'px');
  idleText = look.shortcut === '停用' ? '按麥克風開始' : `按 ${look.shortcut} 或麥克風開始`;
  render();
}

invoke('facts').then(facts => {
  mode = facts.mode;
  applyLook(facts);
  setLayout(facts.layout);
  receive({ state: facts.state, message: facts.message, text: facts.text });
});
