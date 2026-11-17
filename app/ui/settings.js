// The settings window: shows the saved choices and sends them back when the user saves.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const byId = id => document.getElementById(id);
const status = byId('status');

let panel = null;
// The saved microphone when it is not plugged in right now; kept so saving does not lose the choice.
let missingMicrophone = null;

function say(text, fine = false) {
  status.textContent = text;
  status.classList.toggle('fine', fine);
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

function fill() {
  const { prefs, zhuyin } = panel;
  const microphone = byId('microphone');
  microphone.replaceChildren(option('', '跟隨 Windows 預設麥克風'), ...panel.microphones.map(name => option(name, name)));
  if (prefs.MicrophoneName && !panel.microphones.includes(prefs.MicrophoneName)) {
    missingMicrophone = prefs.MicrophoneName;
    microphone.append(option(missingMicrophone, missingMicrophone + '（未連接）'));
  }
  microphone.value = prefs.MicrophoneName || '';
  byId('engine').value = prefs.Online ? '1' : '0';
  byId('engine-note').textContent = (panel.local_ready ? '本機 Whisper 與模型已就緒。' : '本機 Whisper 或模型尚未就緒，請先完成本機語音模型安裝。')
    + '選用線上辨識時，錄音會經第三方的 codex-asr 傳到 ChatGPT。';
  byId('script').value = prefs.Simplified ? '1' : '0';
  byId('use-phrases').checked = prefs.UsePhrases;
  byId('use-replacements').checked = prefs.UseReplacements;
  byId('tidy').checked = prefs.Tidy;
  byId('hold').value = prefs.HoldToTalk ? '1' : '0';
  byId('keep-history').checked = prefs.KeepHistory;
  byId('use-phrases').disabled = !panel.has_word_list;
  byId('delivery').value = String(prefs.DeliveryMode ?? (prefs.AutoInsert ? 0 : 2));
  byId('modifiers').value = String(prefs.Modifiers);
  const key = byId('record-key');
  key.replaceChildren(option('0', '停用'), ...Array.from({ length: 26 }, (_, i) => option(String(65 + i), String.fromCharCode(65 + i))));
  key.value = String(prefs.RecordKey);
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
  byId('open-pad').disabled = !panel.has_pad;
  if (!panel.has_pad) byId('pad-note').textContent = '找不到打字板，請重新安裝完整套件。';
  byId('learning-data').hidden = !panel.has_learning;
  byId('learned-row').hidden = !panel.has_learning;
  if (!panel.shortcut_bound) say('目前錄音快捷鍵被占用，請選其他組合。');
}

function collect() {
  const number = id => Number(byId(id).value);
  const name = byId('microphone').value || null;
  return {
    prefs: {
      ...panel.prefs,
      Online: byId('engine').value === '1',
      Modifiers: number('modifiers'),
      RecordKey: number('record-key'),
      TextSize: number('text-size'),
      MicrophoneName: name,
      AutoInsert: number('delivery') === 0,
      Simplified: byId('script').value === '1',
      UsePhrases: byId('use-phrases').checked,
      UseReplacements: byId('use-replacements').checked,
      Tidy: byId('tidy').checked,
      HoldToTalk: byId('hold').value === '1',
      KeepHistory: byId('keep-history').checked,
      Appearance: number('appearance'),
      SystemAccent: byId('accent').value === '1',
      DeliveryMode: number('delivery'),
    },
    zhuyin: { layout: number('key-layout'), staged: byId('segment').value === '1', candidate_size: number('candidate-size'), line_size: number('line-size') },
    startup: byId('startup').checked,
  };
}

byId('save').addEventListener('click', async () => {
  try {
    await invoke('save_settings', collect());
    invoke('close_settings');
  } catch (reason) {
    say(String(reason));
    openPage(tabs[0]);
  }
});
byId('cancel').addEventListener('click', () => invoke('close_settings'));

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
listen('look', event => applyLook(event.payload));
// The voice window's quick strip opens settings straight on a page.
const pageFor = name => byId('tab-' + name);
listen('open-page', event => { if (pageFor(event.payload)) openPage(pageFor(event.payload)); });
if (pageFor(location.hash.slice(1))) openPage(pageFor(location.hash.slice(1)));

// Local learning
const learning = byId('learning');
const learningNotes = {
  true: '已啟用：記住你改選過的字；連續選出的字出現兩次就記成新詞。切換後立即套用。',
  false: '已關閉：不記錄改字習慣。切換後立即套用。',
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
for (const which of ['keyboard', 'pad', 'language', 'home', 'issues', 'folder']) {
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
const scriptNames = { true: '簡體', false: '繁體' };
const deliveryNames = ['自動插入', '只複製', '只顯示預覽'];
const sameProgram = (a, b) => a.trim().toLowerCase() === b.trim().toLowerCase();

function ruleSummary(rule) {
  const parts = [];
  if (rule.Tidy !== null && rule.Tidy !== undefined) parts.push(tidyNames[rule.Tidy]);
  if (rule.Simplified !== null && rule.Simplified !== undefined) parts.push(scriptNames[rule.Simplified]);
  if (rule.DeliveryMode !== null && rule.DeliveryMode !== undefined) parts.push(deliveryNames[rule.DeliveryMode]);
  if (rule.DropFinalStop) parts.push('去掉句尾句號');
  return parts.length ? parts.join('・') : '全部跟隨主要設定';
}

// A yes-or-no choice as a select value, and back; '' is "follow the main setting".
const choiceOf = value => value === null || value === undefined ? '' : String(Number(value));
const flagFrom = id => byId(id).value === '' ? null : byId(id).value === '1';

function showRule(rule) {
  ruleProgram.value = rule.Program;
  byId('rule-tidy').value = choiceOf(rule.Tidy);
  byId('rule-script').value = choiceOf(rule.Simplified);
  byId('rule-delivery').value = choiceOf(rule.DeliveryMode);
  byId('rule-stop').value = rule.DropFinalStop ? '1' : '0';
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
  showRule({ Program: '', Tidy: null, Simplified: null, DeliveryMode: null, DropFinalStop: false });
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
  if (!program) return void (rulesStatus.textContent = '請輸入程式名稱，例如 Code.exe。');
  const delivery = byId('rule-delivery').value;
  const rule = {
    Program: program,
    Tidy: flagFrom('rule-tidy'),
    Simplified: flagFrom('rule-script'),
    DeliveryMode: delivery === '' ? null : Number(delivery),
    DropFinalStop: byId('rule-stop').value === '1',
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

document.addEventListener('keydown', event => {
  if (event.key === 'Escape' && !document.querySelector('dialog[open]')) invoke('close_settings');
});

invoke('load_settings').then(async loaded => {
  panel = loaded;
  applyLook(panel.look);
  fill();
  if (!panel.has_learning) return void (byId('learning-note').textContent = '找不到本機學習工具，請安裝完整套件。');
  const on = await invoke('learning_state');
  learning.checked = on === true;
  learning.disabled = on === null;
  byId('learning-note').textContent = on === null ? '無法讀取目前的學習狀態。' : learningNotes[on];
});
