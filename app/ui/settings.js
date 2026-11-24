// The settings window: shows the saved choices and stores each change as it is made.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const byId = id => document.getElementById(id);
const status = byId('status');

let panel = null;
// The local recogniser's models and programs: what is here and what can be downloaded.
let local = null;
// The saved microphone when it is not plugged in right now; kept so saving does not lose the choice.
let missingMicrophone = null;

function say(text, fine = false) {
  status.textContent = text;
  status.classList.toggle('fine', fine);
  // The first-run steps cover the footer, so they show the same message.
  if (byId('welcome').open) byId('welcome-status').textContent = text;
}

function applyLook(look) {
  const root = document.documentElement;
  if (look.appearance === 1) root.dataset.theme = 'light';
  else if (look.appearance === 2) root.dataset.theme = 'dark';
  else delete root.dataset.theme;
  for (const [name, value] of [['--accent', look.accent?.[0]], ['--on-accent', look.accent?.[1]]]) {
    if (value) root.style.setProperty(name, value); else root.style.removeProperty(name);
  }
}

// Pages
const tabs = [...document.querySelectorAll('[role="tab"]')];
function openPage(tab) {
  for (const other of tabs) {
    const chosen = other === tab;
    other.setAttribute('aria-selected', String(chosen));
    other.tabIndex = chosen ? 0 : -1;
    byId(other.getAttribute('aria-controls')).hidden = !chosen;
  }
  document.querySelector('.pages').scrollTop = 0;
}
tabs.forEach((tab, index) => {
  tab.addEventListener('click', () => openPage(tab));
  tab.addEventListener('keydown', event => {
    const step = { ArrowDown: 1, ArrowUp: -1 }[event.key];
    if (!step) return;
    event.preventDefault();
    const next = tabs[(index + step + tabs.length) % tabs.length];
    openPage(next);
    next.focus();
  });
});

function option(value, label) {
  const item = document.createElement('option');
  item.value = value;
  item.textContent = label;
  return item;
}

// The page is written for Windows. On macOS the Zhuyin input method's parts are hidden, since
// that input method is Windows only, and the system's own names take the place of Windows' ones.
const onMac = () => panel.platform === 'macos';
function wordForSystem() {
  if (!onMac()) return;
  document.title = '聽字・語音輸入 · 設定';
  for (const part of document.querySelectorAll('[data-windows]')) part.hidden = true;
  for (const part of document.querySelectorAll('[data-mac]')) part.textContent = part.dataset.mac;
  for (const part of document.querySelectorAll('[data-mac-placeholder]')) part.placeholder = part.dataset.macPlaceholder;
}

function fill() {
  const { prefs, zhuyin } = panel;
  const microphone = byId('microphone');
  microphone.replaceChildren(option('', onMac() ? '跟隨 macOS 預設麥克風' : '跟隨 Windows 預設麥克風'), ...panel.microphones.map(name => option(name, name)));
  if (prefs.MicrophoneName && !panel.microphones.includes(prefs.MicrophoneName)) {
    missingMicrophone = prefs.MicrophoneName;
    microphone.append(option(missingMicrophone, missingMicrophone + '（未連接）'));
  }
  microphone.value = prefs.MicrophoneName || '';
  byId('noise-suppression').checked = Boolean(prefs.NoiseSuppression);
  byId('engine').value = prefs.Engine || (prefs.Online ? 'codex' : 'local');
  byId('streaming-asr').checked = Boolean(prefs.StreamingAsr);
  byId('dictation-stream').checked = Boolean(prefs.DictationStream);
  showLocal();
  byId('api-url').value = prefs.ApiUrl;
  byId('api-model').value = prefs.ApiModel;
  showKey();
  showEngine();
  byId('script').value = prefs.Simplified ? '1' : '0';
  byId('use-phrases').checked = prefs.UsePhrases;
  byId('use-replacements').checked = prefs.UseReplacements;
  byId('tidy').checked = prefs.Tidy;
  byId('refine').checked = Boolean(prefs.Refine);
  byId('refine-url').value = prefs.RefineUrl ?? '';
  byId('refine-model').value = prefs.RefineModel ?? '';
  byId('refine-note').value = prefs.RefineNote ?? '';
  byId('refine-language').value = prefs.RefineLanguage ?? '';
  byId('rewrite-selection').checked = Boolean(prefs.RewriteSelection);
  showRefineKey();
  byId('hold').value = prefs.HoldToTalk ? '1' : '0';
  byId('keep-history').checked = prefs.KeepHistory;
  byId('check-updates').checked = prefs.CheckUpdates !== false;
  byId('learn-fixes').checked = prefs.LearnFixes;
  byId('use-phrases').disabled = !panel.has_word_list;
  byId('delivery').value = String(prefs.DeliveryMode ?? (prefs.AutoInsert ? 0 : 2));
  const hotkey = prefs.Hotkey ?? { Modifiers: prefs.Modifiers, Key: prefs.RecordKey };
  byId('hotkey').value = hotkey.Modifiers + ',' + hotkey.Key;
  showHotkey();
  byId('text-size').value = String(prefs.TextSize);
  for (const item of byId('text-size').options) item.textContent = item.value + ' px';

  byId('key-layout').value = String(zhuyin.layout);
  byId('segment').value = zhuyin.staged ? '1' : '0';
  byId('candidate-size').value = String(zhuyin.candidate_size);
  byId('line-size').value = String(zhuyin.line_size);
  byId('appearance').value = String(prefs.Appearance);
  byId('accent').value = prefs.SystemAccent ? '1' : '0';
  byId('startup').checked = panel.startup;
  byId('version').textContent = '版本 ' + panel.version;

  for (const id of ['manage-words', 'manage-templates']) byId(id).disabled = !panel.has_word_list;
  if (!panel.has_word_list) byId('words-note').textContent = '找不到注音詞庫，請重新安裝完整套件。';
  byId('open-keyboard').disabled = !panel.has_keyboard;
  if (!panel.has_keyboard) byId('keyboard-note').textContent = '找不到注音切換工具，請重新安裝完整套件。';
  byId('learning-data').hidden = !panel.has_learning;
  byId('learned-row').hidden = !panel.has_learning;
  if (!panel.shortcut_bound) say('目前錄音快捷鍵被占用，請選其他組合。');
}

const savedEngine = () => panel.prefs.Engine || (panel.prefs.Online ? 'codex' : 'local');

// The choices on the page. While the user's own service is still incomplete, `engineFromPage` false
// keeps the engine the app last accepted so the other choices can still be stored.
function collect(engineFromPage) {
  const number = id => Number(byId(id).value);
  const name = byId('microphone').value || null;
  const engine = engineFromPage ? byId('engine').value : savedEngine();
  return {
    prefs: {
      ...panel.prefs,
      Engine: engine,
      Online: engine === 'codex',
      StreamingAsr: byId('streaming-asr').checked,
      DictationStream: byId('dictation-stream').checked,
      LocalModel: byId('local-model').disabled ? panel.prefs.LocalModel : byId('local-model').value,
      Gpu: byId('gpu').disabled ? panel.prefs.Gpu : byId('gpu').checked,
      ApiUrl: engineFromPage ? byId('api-url').value.trim() : panel.prefs.ApiUrl,
      ApiModel: engineFromPage ? byId('api-model').value.trim() : panel.prefs.ApiModel,
      Hotkey: readHotkey(),
      TextSize: number('text-size'),
      MicrophoneName: name,
      NoiseSuppression: byId('noise-suppression').checked,
      AutoInsert: number('delivery') === 0,
      Simplified: byId('script').value === '1',
      UsePhrases: byId('use-phrases').checked,
      UseReplacements: byId('use-replacements').checked,
      Tidy: byId('tidy').checked,
      Refine: byId('refine').checked,
      RefineUrl: byId('refine-url').value.trim(),
      RefineModel: byId('refine-model').value.trim(),
      RefineNote: byId('refine-note').value.trim(),
      RefineLanguage: byId('refine-language').value,
      RewriteSelection: byId('rewrite-selection').checked,
      HoldToTalk: byId('hold').value === '1',
      KeepHistory: byId('keep-history').checked,
      CheckUpdates: byId('check-updates').checked,
      LearnFixes: byId('learn-fixes').checked,
      Appearance: number('appearance'),
      SystemAccent: byId('accent').value === '1',
      DeliveryMode: number('delivery'),
    },
    zhuyin: { layout: number('key-layout'), staged: byId('segment').value === '1', candidate_size: number('candidate-size'), line_size: number('line-size') },
    startup: byId('startup').checked,
  };
}

// Recognition engine
const engineNotes = {
  local: () => local.ready ? '在這台電腦辨識，錄音不會離開電腦。' : '這台電腦還沒有模型，請在下面下載一個；下載後錄音不會離開電腦。',
  api: () => '錄音會傳到你填的服務，費用依該服務計算；網址是 localhost 時錄音留在這台電腦。',
  codex: () => '錄音會經第三方的 codex-asr 傳到 ChatGPT，需要這台電腦上有效的 Codex 登入。',
};
function showEngine() {
  const engine = byId('engine').value;
  byId('engine-note').textContent = engineNotes[engine]();
  const streaming = byId('streaming-asr');
  streaming.disabled = engine !== 'codex';
  const available = !streaming.disabled && streaming.checked;
  byId('dictation-stream-row').hidden = !available;
  byId('dictation-stream').disabled = !available;
  byId('streaming-asr-note').textContent = streaming.disabled
    ? '實驗功能。使用 Codex 線上辨識時可開啟。'
    : '實驗功能。每次停頓後，剛說完的句子會直接打進錄音開始時游標所在的欄位。開著 AI 潤稿，或「辨識完成後」不是「自動插入並複製文字」時，句子先顯示在浮窗，停錄後才處理全文。錄音片段會經 Codex 傳到 ChatGPT 辨識。';
  byId('local-model-row').hidden = engine !== 'local';
  byId('gpu-row').hidden = engine !== 'local' || !(local.gpu_ready || local.gpu_offer);
  byId('update-row').hidden = engine !== 'local' || !local.updates.length;
  for (const row of document.querySelectorAll('.model-row')) row.hidden = engine !== 'local';
  for (const id of ['api-url-row', 'api-model-row', 'api-key-row']) byId(id).hidden = engine !== 'api';
}
function showKey() {
  byId('api-key').value = '';
  const store = onMac() ? 'macOS 鑰匙圈' : 'Windows 認證管理員';
  byId('api-key-note').textContent = panel.api_key_set
    ? `已存一把金鑰，存在 ${store}。要換新的就貼上；留空會沿用。`
    : `尚未存金鑰。金鑰存在 ${store}，不會寫進設定檔；服務在 localhost 時可以不填。`;
  byId('api-key-clear').disabled = !panel.api_key_set;
}
byId('engine').addEventListener('change', showEngine);
byId('streaming-asr').addEventListener('change', showEngine);

// AI refinement's own key, kept apart from the recognition service's key; like that one, it is never shown back.
function showRefineKey() {
  byId('refine-key').value = '';
  const store = onMac() ? 'macOS 鑰匙圈' : 'Windows 認證管理員';
  byId('refine-key-note').textContent = panel.refine_key_set
    ? `已存一把金鑰，存在 ${store}。要換新的就貼上；留空會沿用。`
    : `尚未存金鑰。金鑰存在 ${store}，和辨識服務的金鑰分開；服務在 localhost 時可以不填。`;
  byId('refine-key-clear').disabled = !panel.refine_key_set;
}

// Local models and the graphics card build. Downloads report on the `local-fetch` event.
function showLocal() {
  const titles = new Map(local.models.map(model => [model.file, model.title]));
  const select = byId('local-model');
  const files = local.files;
  select.replaceChildren(...(files.length ? files : ['']).map(name => option(name, name ? titles.get(name) || name : '尚未下載')));
  const saved = panel.prefs.LocalModel;
  select.value = saved && files.includes(saved) ? saved : (files.includes('ggml-base.bin') ? 'ggml-base.bin' : files[0] || '');
  select.disabled = files.length === 0;

  const gpu = byId('gpu');
  gpu.disabled = !local.gpu_ready;
  gpu.checked = local.gpu_ready && panel.prefs.Gpu;
  byId('gpu-get').hidden = !local.gpu_offer;
  byId('gpu-get').textContent = `下載（${local.gpu_offer}）`;
  byId('gpu-get').disabled = Boolean(local.busy);
  byId('gpu-note').textContent = local.gpu_ready
    ? '用顯示卡辨識，大的模型也不用久等。顯示卡出問題時，那一句會自動改用 CPU。'
    : '這台電腦有 NVIDIA 顯示卡。下載加速元件後，大的模型也不用久等。';

  // One newer program at a time; the next one shows once this one is done.
  const update = local.updates[0];
  if (update) {
    byId('update-note').textContent = `「${update.title}」目前是 ${update.from}，可以更新到 ${update.to}。已下載的模型照常使用，不用重新下載。`;
    byId('update-get').textContent = `更新（${update.size}）`;
    byId('update-get').disabled = Boolean(local.busy);
  }

  for (const row of document.querySelectorAll('.model-row')) row.remove();
  for (const model of local.models) {
    const row = document.createElement('div');
    row.className = 'row sub model-row';
    const what = document.createElement('div');
    what.className = 'what';
    const name = document.createElement('span');
    name.className = 'name';
    name.textContent = `${model.title}・${model.size}`;
    const note = document.createElement('p');
    note.textContent = model.note;
    what.append(name, note);
    const get = document.createElement('button');
    get.type = 'button';
    get.className = 'button';
    // A downloaded model can be deleted from here; one the user placed elsewhere is only marked as there.
    const removable = model.installed && model.removable;
    get.textContent = removable ? '刪除' : model.installed ? '已下載' : '下載';
    get.disabled = (model.installed && !removable) || Boolean(local.busy);
    get.setAttribute('aria-label', `${get.textContent} ${model.title}`);
    get.addEventListener('click', () => removable ? deleteModel(model) : fetchItem(model.id));
    row.append(what, get);
    byId('api-url-row').before(row);
  }
  byId('fetch-row').hidden = !local.busy;
}
async function readLocal() {
  local = await invoke('local_state');
  showLocal();
  showEngine();
}
async function deleteModel(model) {
  try {
    if (!await invoke('local_delete', { file: model.file })) return;
    panel = await invoke('load_settings');
    await readLocal();
    say(`已刪除「${model.title}」。`, true);
  } catch (reason) {
    say(String(reason));
  }
}
async function fetchItem(id) {
  try {
    if (!await invoke('local_fetch', { id })) return;
    byId('fetch-title').textContent = '準備下載';
    byId('fetch-note').textContent = '';
    byId('fetch-level').style.width = '0';
    byId('fetch-cancel').disabled = false;
    say('');
    await readLocal();
  } catch (reason) {
    say(String(reason));
  }
}
byId('gpu-get').addEventListener('click', () => fetchItem('gpu'));
byId('update-get').addEventListener('click', () => local.updates[0] && fetchItem(local.updates[0].id));
byId('fetch-cancel').addEventListener('click', () => {
  byId('fetch-cancel').disabled = true;
  byId('fetch-note').textContent = '正在取消…';
  invoke('local_fetch_cancel');
});
const megabytes = bytes => Math.round(bytes / 1e6);
const fetchSteps = { verify: '核對檔案…', unpack: '解開檔案…', trial: '試跑中…' };
listen('local-fetch', async ({ payload }) => {
  const { id, stage, title, done, total, note } = payload;
  if (stage === 'download' || fetchSteps[stage]) {
    byId('fetch-row').hidden = false;
    byId('fetch-title').textContent = title;
    byId('fetch-level').style.width = (total ? Math.round(done / total * 100) : 0) + '%';
    if (!byId('fetch-cancel').disabled) byId('fetch-note').textContent = fetchSteps[stage] || `下載中 ${megabytes(done)} / ${megabytes(total)} MB`;
    return;
  }
  // Whether the program just finished was an update, known before the list is read again.
  const updated = local.updates.some(update => update.id === id);
  await readLocal();
  if (stage === 'failed') return say(note);
  if (stage === 'cancelled') return say('已取消。已下載的部分會保留，再按一次「下載」會接續。');
  // What was just downloaded is put to use at once.
  const model = local.models.find(model => model.id === id);
  if (updated) return say(`「${title}」已更新並通過試跑，現在開始使用。`, true);
  if (model) {
    byId('local-model').value = model.file;
    store(byId('local-model'));
  } else {
    byId('gpu').checked = true;
    store(byId('gpu'));
  }
  storing = storing.then(readLocal).then(() => say(`「${title}」已下載並通過試跑，現在開始使用。`, true));
});
byId('api-key-clear').addEventListener('click', async () => {
  await invoke('clear_api_key');
  panel.api_key_set = false;
  showKey();
  say('已刪除金鑰。', true);
});

// Every choice is stored as soon as it is made; there is no separate save step.
const engineIds = ['engine', 'api-url', 'api-model', 'api-key'];
const engineNames = { local: '本機 Whisper', api: '自選線上服務', codex: 'Codex 線上辨識' };
const valueOf = control => control.type === 'checkbox' ? control.checked : control.value;
// What each control showed when the app last accepted it, to go back to when a change is refused.
const accepted = new Map();
// Why the engine on the page is not in use yet; empty when the page and the app agree.
let engineWaiting = '';
// Changes are stored one after another, in the order they were made.
let storing = Promise.resolve();

function store(control) {
  storing = storing.then(async () => {
    const engineChange = engineIds.includes(control.id);
    const wanted = collect(engineChange || !engineWaiting);
    try {
      await invoke('save_settings', wanted);
      panel.prefs = wanted.prefs;
      accepted.set(control.id, valueOf(control));
      if (engineChange) engineWaiting = '';
      say(engineWaiting || '已套用。', !engineWaiting);
    } catch (reason) {
      if (engineChange) {
        // The fields to complete only show while this engine is chosen, so the page keeps it.
        engineWaiting = `${String(reason).replace(/。$/, '')}。補齊之前仍使用「${engineNames[savedEngine()]}」。`;
        say(engineWaiting);
      } else {
        if (control.type === 'checkbox') control.checked = accepted.get(control.id);
        else control.value = accepted.get(control.id);
        say(String(reason));
      }
    }
  });
}

byId('api-key').addEventListener('change', () => {
  const key = byId('api-key').value.trim();
  if (!key) return;
  storing = storing.then(async () => {
    try {
      await invoke('set_api_key', { key });
      panel.api_key_set = true;
      showKey();
    } catch (reason) {
      say(String(reason));
    }
  });
});
byId('refine-key').addEventListener('change', () => {
  const key = byId('refine-key').value.trim();
  if (!key) return;
  storing = storing.then(async () => {
    try {
      await invoke('set_refine_key', { key });
      panel.refine_key_set = true;
      showRefineKey();
      say('已存入潤稿金鑰。', true);
    } catch (reason) {
      say(String(reason));
    }
  });
});
byId('refine-key-clear').addEventListener('click', () => {
  storing = storing.then(async () => {
    try {
      // A service that needs a key cannot go on without one, so the app switches refinement off with it.
      const switchedOff = await invoke('clear_refine_key');
      panel.refine_key_set = false;
      showRefineKey();
      if (switchedOff) {
        byId('refine').checked = false;
        byId('rewrite-selection').checked = false;
        panel.prefs.Refine = false;
        panel.prefs.RewriteSelection = false;
        accepted.set('refine', false);
        accepted.set('rewrite-selection', false);
      }
      say(switchedOff ? '已刪除潤稿金鑰，並關閉 AI 潤稿與語音改寫。' : '已刪除潤稿金鑰。', true);
    } catch (reason) {
      say(String(reason));
    }
  });
});
// The test uses what the page shows now, after any change still being stored, such as a key just typed.
const refineTest = byId('refine-test'), refineResult = byId('refine-result');
refineTest.addEventListener('click', async () => {
  refineTest.disabled = true;
  refineResult.classList.remove('failed');
  refineResult.textContent = '測試中…';
  await storing;
  try {
    refineResult.textContent = await invoke('refine_test', {
      url: byId('refine-url').value.trim(),
      model: byId('refine-model').value.trim(),
      note: byId('refine-note').value.trim(),
      language: byId('refine-language').value,
    });
  } catch (reason) {
    refineResult.classList.add('failed');
    refineResult.textContent = String(reason);
  }
  refineTest.disabled = false;
  // Disabling the button drops keyboard focus; give it back.
  if (document.activeElement === document.body) refineTest.focus();
});
document.querySelector('.pages').addEventListener('change', event => {
  // Local learning is switched by its own tool, and the refinement key is stored by its own command.
  if (event.target.id !== 'learning' && event.target.id !== 'refine-key') store(event.target);
});
byId('close').addEventListener('click', () => invoke('close_settings'));

// Recording shortcut. Keys are stored as Windows virtual-key numbers and modifier bits, as the app reads them.
const HOTKEY_BITS = { ctrlKey: 2, altKey: 1, shiftKey: 4, metaKey: 8 };
const hotkeyField = byId('hotkey');
let listening = false;
function readHotkey() {
  const [modifiers, key] = hotkeyField.value.split(',').map(Number);
  return { Modifiers: modifiers || 0, Key: key || 0 };
}
function keyName(key) {
  if (key >= 65 && key <= 90) return String.fromCharCode(key);
  if (key >= 48 && key <= 57) return String(key - 48);
  if (key >= 112 && key <= 135) return 'F' + (key - 111);
  return '';
}
// Control, Alt, Shift and the Windows key, in that order, as this system names them.
function hotkeyLabel(modifiers, key) {
  const names = panel.modifier_keys;
  const held = [2, 1, 4, 8].map((bit, i) => modifiers & bit ? names[i] : '').filter(Boolean);
  return [...held, keyName(key)].filter(Boolean).join('+');
}
function showHotkey() {
  const { Modifiers, Key } = readHotkey();
  const label = byId('hotkey-label');
  label.classList.remove('listening');
  label.textContent = Key ? hotkeyLabel(Modifiers, Key) : '停用';
  byId('hotkey-change').textContent = '變更';
  byId('hotkey-off').disabled = !Key;
}
function keyFromCode(code) {
  let match = /^Key([A-Z])$/.exec(code);
  if (match) return match[1].charCodeAt(0);
  match = /^Digit([0-9])$/.exec(code);
  if (match) return 48 + Number(match[1]);
  match = /^F([0-9]{1,2})$/.exec(code);
  if (match && Number(match[1]) >= 1 && Number(match[1]) <= 24) return 111 + Number(match[1]);
  return 0;
}
async function stopListening() {
  if (!listening) return;
  listening = false;
  window.removeEventListener('keydown', onHotkeyKey, true);
  window.removeEventListener('blur', cancelListening);
  await storing;
  await invoke('capture_shortcut', { on: false });
  showHotkey();
}
function cancelListening() {
  say('');
  stopListening();
}
function saveHotkey(modifiers, key) {
  hotkeyField.value = modifiers + ',' + key;
  store(hotkeyField);
}
function onHotkeyKey(event) {
  event.preventDefault();
  event.stopImmediatePropagation();
  const modifiers = Object.entries(HOTKEY_BITS).reduce((sum, [flag, bit]) => event[flag] ? sum + bit : sum, 0);
  if (event.key === 'Escape' && !modifiers) return cancelListening();
  if (/^(Control|Alt|Shift|Meta|OS)/.test(event.code)) {
    byId('hotkey-label').textContent = hotkeyLabel(modifiers, 0) + '+…';
    return;
  }
  const key = keyFromCode(event.code);
  if (!key) return say('請用字母、數字或 F1 到 F24，可以搭配輔助鍵。Esc 取消。');
  saveHotkey(modifiers, key);
  stopListening();
}
byId('hotkey-change').addEventListener('click', async () => {
  if (listening) return cancelListening();
  listening = true;
  await invoke('capture_shortcut', { on: true });
  const label = byId('hotkey-label');
  label.classList.add('listening');
  label.textContent = '請按下組合…';
  byId('hotkey-change').textContent = '取消';
  say(`按下想用的組合，例如 ${hotkeyLabel(3, 82)} 或單獨的 F9；Esc 取消。`, true);
  window.addEventListener('keydown', onHotkeyKey, true);
  window.addEventListener('blur', cancelListening);
});
byId('hotkey-off').addEventListener('click', () => {
  saveHotkey(readHotkey().Modifiers, 0);
  storing.then(showHotkey);
});

// Microphone test
const micTest = byId('mic-test');
let testing = false;
function showTest(on) {
  testing = on;
  micTest.textContent = on ? '停止' : '測試';
  if (!on) byId('mic-level').style.width = '0';
}
async function stopTest() {
  if (!testing) return;
  showTest(false);
  await invoke('test_microphone', { name: null, on: false });
}
micTest.addEventListener('click', async () => {
  if (testing) return stopTest();
  const name = byId('microphone').value || null;
  if (name && name === missingMicrophone) return say('這支麥克風目前未連接。');
  try {
    await invoke('test_microphone', { name, on: true });
    showTest(true);
    say('');
  } catch (reason) {
    say(String(reason));
  }
});
byId('microphone').addEventListener('change', stopTest);
// Square root keeps quiet speech visible on the meter.
listen('mic-level', event => { if (testing) byId('mic-level').style.width = Math.round(Math.sqrt(event.payload) * 100) + '%'; });
listen('mic-test-ended', () => showTest(false));
listen('look', event => {
  if (panel) panel.look = event.payload;
  applyLook(event.payload);
});

// Local learning
const learning = byId('learning');
const learningNotes = {
  true: '已啟用：記住你改選過的字；連續選出的字出現兩次就記成新詞。',
  false: '已關閉：不記錄改字習慣。',
};
learning.addEventListener('change', async () => {
  const wanted = learning.checked;
  learning.disabled = true;
  const done = await invoke('set_learning', { on: wanted });
  if (!done) learning.checked = !wanted;
  byId('learning-note').textContent = done ? learningNotes[wanted] : '無法變更本機學習，請檢查工具與資料權限。';
  learning.disabled = false;
});
const dataNotes = {
  import: ['學習資料已匯入。', '無法匯入這個檔案。'],
  export: ['學習資料已匯出。', '沒有可匯出的學習資料，或無法寫入該位置。'],
  clear: ['學習資料已清除。', '沒有學習資料可清除，或無法寫入。'],
};
for (const action of Object.keys(dataNotes)) {
  byId('learning-' + action).addEventListener('click', async () => {
    const done = await invoke('learning_data', { action });
    if (done !== null) byId('learning-data-note').textContent = dataNotes[action][done ? 0 : 1];
  });
}

byId('history-clear').addEventListener('click', async () => {
  byId('history-note').textContent = await invoke('history_clear') ? '辨識紀錄已清除。' : '無法清除辨識紀錄。';
});

// Links and tools
for (const which of ['keyboard', 'language', 'home', 'issues', 'folder']) {
  byId('open-' + which).addEventListener('click', async () => {
    if (!await invoke('open_target', { which })) say('無法開啟，請確認完整套件已安裝。');
  });
}

// Custom words and templates share one editor; a template adds the text its trigger expands to.
const dialog = byId('words'), list = byId('words-list'), newWord = byId('new-word'), readings = byId('readings');
const expansion = byId('expansion'), wordsStatus = byId('words-status');
let templates = false, entries = [], selected = -1, lookup = 0;

function refill() {
  list.replaceChildren(...entries.map((entry, index) => {
    const item = document.createElement('li');
    const sounds = entry.reading.replaceAll('-', ' ');
    item.textContent = templates ? `${sounds}　→　${entry.word}` : `${entry.word}　${sounds}`;
    item.setAttribute('role', 'option');
    item.setAttribute('aria-selected', String(index === selected));
    item.addEventListener('click', () => { selected = index; refill(); });
    return item;
  }));
}

async function openWords(forTemplates) {
  templates = forTemplates;
  selected = -1;
  byId('words-title').textContent = templates ? '文字範本' : '自訂詞';
  byId('words-intro').textContent = templates
    ? '輸入一個觸發詞（例如「地址」）和要展開的文字。打出觸發詞的注音後按 ↓，就能在候選清單選到這段文字。破音字請選對讀音。新範本在重新開啟的應用程式生效。'
    : '加入的詞在打注音時優先出現，語音辨識的同音字也會換成這些詞。輸入詞語後，每個字會帶出讀音；破音字請從下拉選單選對的讀音。新詞在重新開啟的應用程式生效。';
  newWord.placeholder = templates ? '觸發詞，2 到 8 個字' : '輸入 2 到 8 個字的詞';
  newWord.setAttribute('aria-label', templates ? '觸發詞' : '新詞');
  expansion.hidden = !templates;
  newWord.value = expansion.value = wordsStatus.textContent = '';
  readings.replaceChildren();
  entries = await invoke('words_load', { templates });
  refill();
  dialog.showModal();
  newWord.focus();
}
byId('manage-words').addEventListener('click', () => openWords(false));
byId('manage-templates').addEventListener('click', () => openWords(true));
byId('words-close').addEventListener('click', () => dialog.close());

newWord.addEventListener('input', async () => {
  const word = newWord.value.trim();
  const mine = ++lookup;
  readings.replaceChildren();
  wordsStatus.textContent = '';
  if (!word) return;
  const found = await invoke('word_readings', { word });
  if (mine !== lookup) return; // the user kept typing
  if (!found) return void (wordsStatus.textContent = '無法讀取注音詞庫，請重新安裝完整套件。');
  const unknown = [...word].find((_, index) => found[index].length === 0);
  if (unknown) return void (wordsStatus.textContent = `詞庫裡沒有「${unknown}」的讀音，無法使用這個詞。`);
  readings.replaceChildren(...[...word].map((character, index) => {
    const choice = document.createElement('select');
    choice.setAttribute('aria-label', `「${character}」的讀音`);
    choice.append(...found[index].map(reading => option(reading, reading)));
    return choice;
  }));
});

async function persist(next) {
  if (!await invoke('words_save', { templates, entries: next })) {
    wordsStatus.textContent = '無法儲存。';
    return false;
  }
  entries = next;
  selected = -1;
  refill();
  return true;
}

byId('add-word').addEventListener('click', async () => {
  const trigger = newWord.value.trim();
  if ([...trigger].length < 2) return void (wordsStatus.textContent = '請輸入至少兩個字。');
  if (readings.children.length !== [...trigger].length) return;
  const word = templates ? expansion.value.trim() : trigger;
  if (!word) return void (wordsStatus.textContent = '請輸入展開後的文字。');
  const reading = [...readings.children].map(choice => choice.value).join('-');
  const kept = entries.filter(entry => !(entry.word === word && entry.reading === reading));
  if (!await persist([...kept, { reading, word }])) return;
  newWord.value = expansion.value = '';
  readings.replaceChildren();
  wordsStatus.textContent = `已加入「${trigger}」。`;
  newWord.focus();
});
newWord.addEventListener('keydown', event => { if (event.key === 'Enter') byId('add-word').click(); });

byId('words-remove').addEventListener('click', async () => {
  if (selected < 0) return void (wordsStatus.textContent = '請先在清單選一項。');
  if (await persist(entries.filter((_, index) => index !== selected))) wordsStatus.textContent = '已刪除。';
});
byId('words-export').addEventListener('click', async () => {
  const done = await invoke('words_export', { templates, entries });
  if (done !== null) wordsStatus.textContent = done ? `已匯出 ${entries.length} 筆。` : '無法寫入該位置。';
});
byId('words-import').addEventListener('click', async () => {
  const incoming = await invoke('words_import', { templates });
  if (incoming === null) return;
  const fresh = incoming.filter(entry => !entries.some(have => have.word === entry.word && have.reading === entry.reading));
  if (fresh.length && !await persist([...entries, ...fresh])) return;
  wordsStatus.textContent = incoming.length === 0
    ? `檔案裡沒有可用的${templates ? '範本' : '詞'}。`
    : `已匯入 ${fresh.length} 筆，略過 ${incoming.length - fresh.length} 筆重複。`;
});

// A list of choices the user can pick one of; clicking an item calls `chosen` with its index.
function fillList(element, labels, current, chosen) {
  element.replaceChildren(...labels.map((label, index) => {
    const item = document.createElement('li');
    item.textContent = label;
    item.setAttribute('role', 'option');
    item.setAttribute('aria-selected', String(index === current));
    item.addEventListener('click', () => chosen(index));
    return item;
  }));
}

// Replacement rules: what the recogniser keeps hearing, and what to write instead.
const replaceDialog = byId('replacements'), replaceList = byId('replacements-list');
const newHeard = byId('new-heard'), newWanted = byId('new-wanted'), replaceStatus = byId('replacements-status');
let replacements = [], replacePick = -1;
// English letters are compared without case, as the rules themselves are.
const sameHeard = (a, b) => a.trim().toLowerCase() === b.trim().toLowerCase();

function refillReplacements() {
  fillList(replaceList, replacements.map(rule => `${rule.heard}　→　${rule.wanted}`), replacePick, index => {
    replacePick = index;
    refillReplacements();
  });
}

async function keepReplacements(next) {
  if (!await invoke('replacements_save', { rules: next })) {
    replaceStatus.textContent = '無法儲存。';
    return false;
  }
  replacements = next;
  replacePick = -1;
  refillReplacements();
  return true;
}

byId('manage-replacements').addEventListener('click', async () => {
  replacePick = -1;
  newHeard.value = newWanted.value = replaceStatus.textContent = '';
  replacements = await invoke('replacements_load');
  refillReplacements();
  replaceDialog.showModal();
  newHeard.focus();
});
byId('replacements-close').addEventListener('click', () => replaceDialog.close());
byId('add-replacement').addEventListener('click', async () => {
  const heard = newHeard.value.trim(), wanted = newWanted.value.trim();
  if (!heard || !wanted) return void (replaceStatus.textContent = '請輸入聽到的字和要換成的字。');
  if (heard === wanted) return void (replaceStatus.textContent = '兩邊的字相同，不需要取代。');
  // A rule for the same heard text replaces the old one.
  if (!await keepReplacements([...replacements.filter(rule => !sameHeard(rule.heard, heard)), { heard, wanted }])) return;
  newHeard.value = newWanted.value = '';
  replaceStatus.textContent = `已加入「${heard}」→「${wanted}」。`;
  newHeard.focus();
});
for (const field of [newHeard, newWanted]) {
  field.addEventListener('keydown', event => { if (event.key === 'Enter') byId('add-replacement').click(); });
}
byId('replacements-remove').addEventListener('click', async () => {
  if (replacePick < 0) return void (replaceStatus.textContent = '請先在清單選一項。');
  if (await keepReplacements(replacements.filter((_, index) => index !== replacePick))) replaceStatus.textContent = '已刪除。';
});
byId('replacements-export').addEventListener('click', async () => {
  const done = await invoke('replacements_export', { rules: replacements });
  if (done !== null) replaceStatus.textContent = done ? `已匯出 ${replacements.length} 筆。` : '無法寫入該位置。';
});
byId('replacements-import').addEventListener('click', async () => {
  const incoming = await invoke('replacements_import');
  if (incoming === null) return;
  const fresh = incoming.filter(rule => !replacements.some(have => have.heard === rule.heard && have.wanted === rule.wanted));
  // An imported rule for text already listed takes over from the old one.
  const kept = replacements.filter(have => !fresh.some(rule => sameHeard(rule.heard, have.heard)));
  if (fresh.length && !await keepReplacements([...kept, ...fresh])) return;
  replaceStatus.textContent = incoming.length === 0
    ? '檔案裡沒有可用的規則。'
    : `已匯入 ${fresh.length} 筆，略過 ${incoming.length - fresh.length} 筆重複。`;
});

// Per-program rules. An empty choice means the program follows the main setting.
const rulesDialog = byId('app-rules'), rulesList = byId('app-rules-list'), ruleProgram = byId('rule-program');
const rulesStatus = byId('app-rules-status');
let appRules = [], rulePick = -1;
const tidyNames = { true: '整理口語', false: '不整理口語' };
const refineNames = { true: 'AI 潤稿', false: '不潤稿' };
const scriptNames = { true: '簡體', false: '繁體' };
const deliveryNames = ['自動插入', '只複製', '只顯示預覽'];
const languageFrom = value => value === '' ? null : value === 'same' ? '' : value;
const languageNames = { '': '潤稿照說的語言', en: '潤稿成英文', 'zh-TW': '潤稿成中文' };
const sameProgram = (a, b) => a.trim().toLowerCase() === b.trim().toLowerCase();

function ruleSummary(rule) {
  const parts = [];
  if (rule.Tidy !== null && rule.Tidy !== undefined) parts.push(tidyNames[rule.Tidy]);
  if (rule.Refine !== null && rule.Refine !== undefined) parts.push(refineNames[rule.Refine]);
  if (rule.Simplified !== null && rule.Simplified !== undefined) parts.push(scriptNames[rule.Simplified]);
  if (rule.DeliveryMode !== null && rule.DeliveryMode !== undefined) parts.push(deliveryNames[rule.DeliveryMode]);
  if (rule.DropFinalStop) parts.push('去掉句尾句號');
  if (rule.RefineLanguage !== null && rule.RefineLanguage !== undefined) parts.push(languageNames[rule.RefineLanguage] ?? languageNames['']);
  if (rule.RefineNote) parts.push(`語氣：${rule.RefineNote}`);
  return parts.length ? parts.join('・') : '全部跟隨主要設定';
}

// A yes-or-no choice as a select value, and back; '' is "follow the main setting".
const choiceOf = value => value === null || value === undefined ? '' : String(Number(value));
const flagFrom = id => byId(id).value === '' ? null : byId(id).value === '1';

function showRule(rule) {
  ruleProgram.value = rule.Program;
  byId('rule-tidy').value = choiceOf(rule.Tidy);
  byId('rule-refine').value = choiceOf(rule.Refine);
  byId('rule-script').value = choiceOf(rule.Simplified);
  byId('rule-delivery').value = choiceOf(rule.DeliveryMode);
  byId('rule-stop').value = rule.DropFinalStop ? '1' : '0';
  byId('rule-language').value = rule.RefineLanguage === null || rule.RefineLanguage === undefined ? '' : rule.RefineLanguage || 'same';
  byId('rule-note').value = rule.RefineNote ?? '';
}

function refillRules() {
  fillList(rulesList, appRules.map(rule => `${rule.Program}　${ruleSummary(rule)}`), rulePick, index => {
    rulePick = index;
    showRule(appRules[index]);
    refillRules();
  });
}

async function keepRules(next) {
  if (!await invoke('app_rules_save', { rules: next })) {
    rulesStatus.textContent = '無法儲存。';
    return false;
  }
  appRules = next;
  rulePick = -1;
  refillRules();
  return true;
}

byId('manage-app-rules').addEventListener('click', async () => {
  rulePick = -1;
  rulesStatus.textContent = '';
  showRule({ Program: '', Tidy: null, Refine: null, Simplified: null, DeliveryMode: null, DropFinalStop: false, RefineLanguage: null, RefineNote: '' });
  appRules = await invoke('app_rules_load');
  refillRules();
  rulesDialog.showModal();
  ruleProgram.focus();
  const running = await invoke('running_programs');
  byId('running-programs').replaceChildren(...running.map(name => option(name, name)));
});
byId('app-rules-close').addEventListener('click', () => rulesDialog.close());
byId('add-app-rule').addEventListener('click', async () => {
  const program = ruleProgram.value.trim();
  if (!program) return void (rulesStatus.textContent = onMac() ? '請輸入程式名稱，例如 Safari.app。' : '請輸入程式名稱，例如 Code.exe。');
  const delivery = byId('rule-delivery').value;
  const rule = {
    Program: program,
    Tidy: flagFrom('rule-tidy'),
    Refine: flagFrom('rule-refine'),
    Simplified: flagFrom('rule-script'),
    DeliveryMode: delivery === '' ? null : Number(delivery),
    DropFinalStop: byId('rule-stop').value === '1',
    // '' follows the main setting; 'same' keeps the language spoken, which the file stores as ''.
    RefineLanguage: languageFrom(byId('rule-language').value),
    RefineNote: byId('rule-note').value.trim(),
  };
  // A program has one rule; adding it again updates that rule.
  const existed = appRules.some(have => sameProgram(have.Program, program));
  if (!await keepRules([...appRules.filter(have => !sameProgram(have.Program, program)), rule])) return;
  rulesStatus.textContent = existed ? `已更新「${program}」。` : `已加入「${program}」。`;
});
ruleProgram.addEventListener('keydown', event => { if (event.key === 'Enter') byId('add-app-rule').click(); });
byId('app-rules-remove').addEventListener('click', async () => {
  if (rulePick < 0) return void (rulesStatus.textContent = '請先在清單選一項。');
  if (await keepRules(appRules.filter((_, index) => index !== rulePick))) rulesStatus.textContent = '已刪除。';
});

// Words local learning picked up from typing.
const learnedDialog = byId('learned'), learnedList = byId('learned-list'), learnedStatus = byId('learned-status');
let learned = [], learnedPick = -1;

function refillLearned() {
  const labels = learned.map(word => `${word.text}　${word.reading.replaceAll('-', ' ')}${word.hits === 1 ? '（暫記）' : ''}`);
  fillList(learnedList, labels, learnedPick, index => {
    learnedPick = index;
    refillLearned();
  });
}

async function readLearned() {
  const found = await invoke('learned_words');
  learned = found ?? [];
  learnedPick = -1;
  const empty = found === null ? '找不到本機學習工具' : '還沒有學會任何詞';
  learnedList.dataset.empty = empty;
  learnedStatus.textContent = learned.length ? `共 ${learned.length} 個詞。` : empty;
  byId('learned-remove').disabled = learned.length === 0;
  refillLearned();
}

byId('show-learned').addEventListener('click', async () => {
  learnedList.dataset.empty = learnedStatus.textContent = '讀取中…';
  learned = [];
  refillLearned();
  learnedDialog.showModal();
  await readLearned();
});
byId('learned-close').addEventListener('click', () => learnedDialog.close());
byId('learned-remove').addEventListener('click', async () => {
  if (learnedPick < 0) return void (learnedStatus.textContent = '請先在清單選一項。');
  const { reading, text } = learned[learnedPick];
  if (!await invoke('forget_learned_word', { reading, text })) return void (learnedStatus.textContent = `無法刪除「${text}」。`);
  await readLearned();
  learnedStatus.textContent = `已刪除「${text}」。`;
});

// Words dictation learned from the user's fixes.
const voiceLearnedDialog = byId('voice-learned'), voiceLearnedList = byId('voice-learned-list'), voiceLearnedStatus = byId('voice-learned-status');
let voiceLearned = [], voiceLearnedPick = -1;

function refillVoiceLearned() {
  const labels = voiceLearned.map(entry => `${entry.word}　${entry.reading.replaceAll('-', ' ')}`);
  fillList(voiceLearnedList, labels, voiceLearnedPick, index => {
    voiceLearnedPick = index;
    refillVoiceLearned();
  });
}

async function readVoiceLearned() {
  voiceLearned = await invoke('voice_learned');
  voiceLearnedPick = -1;
  const empty = '還沒有從改字學會任何詞';
  voiceLearnedList.dataset.empty = empty;
  voiceLearnedStatus.textContent = voiceLearned.length ? `共 ${voiceLearned.length} 個詞。` : empty;
  byId('voice-learned-remove').disabled = voiceLearned.length === 0;
  refillVoiceLearned();
}

byId('show-voice-learned').addEventListener('click', async () => {
  voiceLearnedDialog.showModal();
  await readVoiceLearned();
});
byId('voice-learned-close').addEventListener('click', () => voiceLearnedDialog.close());
byId('voice-learned-remove').addEventListener('click', async () => {
  if (voiceLearnedPick < 0) return void (voiceLearnedStatus.textContent = '請先在清單選一項。');
  const { reading, word } = voiceLearned[voiceLearnedPick];
  if (!await invoke('forget_voice_learned', { reading, word })) return void (voiceLearnedStatus.textContent = `無法刪除「${word}」。`);
  await readVoiceLearned();
  voiceLearnedStatus.textContent = `已刪除「${word}」，不會再學這個詞。`;
});

// First-run steps. They borrow the microphone and recognition cards from the 語音 page, so every choice is
// stored the same way as on that page, and put them back when the steps close.
const welcome = byId('welcome'), welcomeBody = byId('welcome-body');
const borrowed = [];
let welcomeAt = 0;

function borrow(card) {
  const marker = document.createComment('');
  card.before(marker);
  borrowed.push({ card, marker });
  welcomeBody.append(card);
}
function giveBack() {
  for (const { card, marker } of borrowed.splice(0)) marker.replaceWith(card);
}

const shortcutName = () => panel.look.shortcut || '錄音快捷鍵';
const welcomeSteps = [
  {
    title: '選擇麥克風',
    intro: () => '選你要用的麥克風，按「測試」後說幾個字，音量條有跟著動就可以了。',
    show: () => borrow(byId('mic-card')),
  },
  {
    title: '選擇辨識方式',
    intro: () => '「本機 Whisper」在這台電腦辨識，錄音不會離開電腦，不需要帳號；第一次要下載模型，不確定就選 Whisper small。'
      + '「Codex 線上辨識」把錄音經第三方元件送到 ChatGPT，需要這台電腦上有 Codex 登入。'
      + '「自選線上服務」把錄音送到你填的服務，需要該服務的金鑰。',
    show: () => borrow(byId('engine-card')),
  },
  {
    title: '練習一句',
    intro: () => onMac() ? '在文字編輯裡說一句話，看看文字怎麼插入。' : '在記事本裡說一句話，再用注音改一個字。',
    show: () => {
      byId('welcome-shortcut').textContent = shortcutName();
      byId('welcome-keyboard').hidden = onMac() || !panel.has_keyboard;
      byId('welcome-practice').hidden = false;
    },
  },
  {
    title: '可以開始用了',
    intro: () => `在任何程式裡按 ${shortcutName()} 開始錄音，再按一次停止，文字會插入游標所在的地方。`
      + (onMac() ? '選單列的聽字圖示也可以開始錄音和打開設定。' : '螢幕右側邊緣的聽字圖示也可以點一下錄音；系統匣圖示的選單可以找回浮窗和打開設定。')
      + '之後想重來，可以在設定的「關於」頁按「首次設定」。',
    show: () => {},
  },
];

function showWelcomeStep() {
  giveBack();
  byId('welcome-practice').hidden = true;
  byId('welcome-status').textContent = '';
  const step = welcomeSteps[welcomeAt];
  const last = welcomeAt === welcomeSteps.length - 1;
  byId('welcome-step').textContent = last ? '' : `第 ${welcomeAt + 1} 步，共 ${welcomeSteps.length - 1} 步・每一步都可以略過`;
  byId('welcome-title').textContent = step.title;
  byId('welcome-intro').textContent = step.intro();
  step.show();
  byId('welcome-back').hidden = welcomeAt === 0;
  byId('welcome-skip').hidden = last;
  byId('welcome-next').textContent = last ? '完成' : '下一步';
}
async function moveWelcome(by) {
  // A microphone test left running would keep the microphone open after its step.
  await stopTest();
  welcomeAt += by;
  if (welcomeAt >= welcomeSteps.length) return welcome.close();
  showWelcomeStep();
}
function startWelcome() {
  welcomeAt = 0;
  showWelcomeStep();
  welcome.showModal();
}
welcome.addEventListener('close', () => {
  stopTest();
  giveBack();
});
byId('welcome-next').addEventListener('click', () => moveWelcome(1));
byId('welcome-skip').addEventListener('click', () => moveWelcome(1));
byId('welcome-back').addEventListener('click', () => moveWelcome(-1));
byId('open-welcome').addEventListener('click', startWelcome);
for (const [id, which] of [['welcome-editor', 'practice'], ['welcome-keyboard', 'keyboard']]) {
  byId(id).addEventListener('click', async () => {
    if (!await invoke('open_target', { which })) say('無法開啟，請確認完整套件已安裝。');
  });
}

// Problem report: built on this computer, shown to the user to read and edit, and only sent by them.
const reportDialog = byId('report');
const reportText = byId('report-text');
const reportStatus = byId('report-status');
async function startReport() {
  reportStatus.textContent = '';
  reportText.value = '正在整理…';
  if (!reportDialog.open) reportDialog.showModal();
  reportText.value = await invoke('report_build');
}
byId('open-report').addEventListener('click', startReport);
byId('report-close').addEventListener('click', () => reportDialog.close());
byId('report-copy').addEventListener('click', async () => {
  reportStatus.textContent = await invoke('report_copy', { text: reportText.value }) ? '已複製。' : '剪貼簿忙碌，請再試一次。';
});
byId('report-save').addEventListener('click', async () => {
  const saved = await invoke('report_save', { text: reportText.value });
  if (saved !== null) reportStatus.textContent = saved ? '已存檔。' : '無法存檔，請換一個位置。';
});
byId('report-send').addEventListener('click', async () => {
  reportStatus.textContent = await invoke('report_send', { text: reportText.value })
    ? '已在瀏覽器開啟回報頁，內容也已複製。補上發生了什麼事，再按送出。'
    : '無法開啟瀏覽器。內容已複製，請到專案首頁的 Issues 回報。';
});
listen('report', startReport);

// Updates
let update = null;
function showUpdate(state) {
  update = state;
  const offered = state.offered;
  const checked = state.checked ? new Date(state.checked * 1000).toLocaleString('zh-TW', { dateStyle: 'medium', timeStyle: 'short' }) : '';
  byId('update-title').textContent = offered ? '有新版 ' + offered.version.replace(/^v/i, '') : '目前已是最新版';
  byId('update-note').textContent = offered
    ? (offered.installer ? '下載後會先核對檔案，再開啟安裝程式。安裝時語音浮窗會關閉，完成後自動開啟新版；設定、詞表、學習資料與模型都保留。' : '這個新版沒有附上這台電腦用的安裝程式，按下按鈕會開啟下載頁。')
    : '目前版本 ' + state.current + (checked ? '，上次檢查：' + checked : '。');
  const install = byId('update-install');
  install.hidden = !offered;
  install.textContent = offered?.installer ? '下載並安裝' : '開啟下載頁';
  install.disabled = state.installing;
  byId('update-notes-row').hidden = !offered?.notes;
  byId('update-notes').textContent = offered?.notes ?? '';
}
byId('update-check').addEventListener('click', async () => {
  const button = byId('update-check');
  button.disabled = true;
  try {
    showUpdate(await invoke('update_check'));
    say(update.offered ? '找到新版。' : '目前已是最新版。', true);
  } catch (reason) {
    say(String(reason));
  }
  button.disabled = false;
});
byId('update-install').addEventListener('click', async () => {
  const button = byId('update-install');
  button.disabled = true;
  try {
    say(await invoke('update_install'), true);
  } catch (reason) {
    say(String(reason));
    showUpdate(await invoke('update_state'));
  }
  button.disabled = false;
});
listen('update', event => showUpdate(event.payload));
listen('update-progress', event => {
  const [stage, have, size] = event.payload;
  byId('update-note').textContent = stage === 'verify' ? '正在核對下載的檔案…' : '正在下載新版… ' + Math.floor(have * 100 / Math.max(size, 1)) + '%';
});

document.addEventListener('keydown', event => {
  if (event.key === 'Escape' && !document.querySelector('dialog[open]')) invoke('close_settings');
});

invoke('load_settings').then(async loaded => {
  panel = loaded;
  local = await invoke('local_state');
  applyLook(panel.look);
  wordForSystem();
  fill();
  for (const control of document.querySelectorAll('.pages select, .pages input, .pages textarea')) accepted.set(control.id, valueOf(control));
  showUpdate(await invoke('update_state'));
  if (await invoke('take_welcome')) startWelcome();
  else if (await invoke('take_report')) startReport();
  if (!panel.has_learning) return void (byId('learning-note').textContent = '找不到本機學習工具，請安裝完整套件。');
  const on = await invoke('learning_state');
  learning.checked = on === true;
  learning.disabled = on === null;
  byId('learning-note').textContent = on === null ? '無法讀取目前的學習狀態。' : learningNotes[on];
});
