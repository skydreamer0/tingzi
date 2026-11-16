// The window only shows state; recording, recognition and insertion all happen in the Rust side.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const capsule = document.querySelector('.capsule');
const mic = document.getElementById('mic');
const status = document.getElementById('status');
const level = document.getElementById('level');
let idleText = '按麥克風開始';

function render(state, text) {
  capsule.dataset.state = state;
  status.textContent = text || idleText;
  status.title = state === 'done' ? text : '';
  mic.disabled = state === 'working';
  mic.setAttribute('aria-pressed', String(state === 'recording'));
  mic.setAttribute('aria-label', state === 'recording' ? '停止錄音' : '開始錄音');
  if (state !== 'recording') level.style.width = '0';
}

mic.addEventListener('click', () => invoke('toggle_recording'));
listen('voice', event => render(event.payload.state, event.payload.text));
// Square root keeps quiet speech visible on the meter.
listen('level', event => { level.style.width = Math.round(Math.sqrt(event.payload) * 100) + '%'; });

invoke('shortcut_label').then(label => {
  idleText = `按 ${label} 或麥克風開始`;
  if (capsule.dataset.state === 'idle') render('idle', '');
});
