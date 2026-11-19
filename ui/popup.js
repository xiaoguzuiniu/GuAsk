const { invoke, Channel } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const win = window.__TAURI__.window.getCurrentWindow();
const $ = (id) => document.getElementById(id);

let cfg = null;
let text = "";
let mode = "translate";
let history = [];
let pinned = false;
let busy = false;

// 中文（CJK）字符占比超过 30% 视为中文
function isChinese(s) {
  const letters = s.replace(/[\s\d\p{P}\p{S}]/gu, "");
  if (!letters) return false;
  const cjk = (letters.match(/[\u3400-\u9fff\uf900-\ufaff]/g) || []).length;
  return cjk / letters.length > 0.3;
}

// 单个英文单词（允许连字符/撇号，如 well-known、don't）
const isWord = (s) => /^[A-Za-z][A-Za-z'\-]{0,30}$/.test(s.trim());

// 粗略判断代码 / 报错 / 日志
function isCode(s) {
  const t = s.trim();
  if (t.length < 8) return false;
  const lines = t.split(/\r?\n/);
  let score = 0;
  if (/(Error|Exception|Traceback|panicked at|FATAL|ERROR|WARN|undefined is not|Segmentation fault|errno|E\d{4}|at [\w.$<>]+\(.*:\d+)/.test(t)) score += 3;
  if (/^\s*(at |File "|\s+at )/m.test(t)) score += 2;
  if (/^\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}/m.test(t)) score += 2; // 日志时间戳
  if (/[{};]\s*$/m.test(t)) score += 1;
  if (/\b(function|def|class|import|return|const|let|var|fn|pub|public|private|#include|SELECT|FROM)\b/.test(t)) score += 2;
  if (/(=>|->|::|==|!=|&&|\|\||\+\+|\w+\([^)]*\))/.test(t)) score += 1;
  if (lines.length > 1 && lines.filter((l) => /^\s{2,}|\t/.test(l)).length >= lines.length / 3) score += 1;
  if (/[\u3400-\u9fff]/.test(t) && score < 3) score -= 2;
  return score >= 3;
}

let tplIdx = -1; // mode === "tpl" 时使用的模板下标
function sysPrompt() {
  let p;
  if (mode === "translate") p = isWord(text) && cfg.word_prompt ? cfg.word_prompt : cfg.translate_prompt;
  else if (mode === "code") p = cfg.code_prompt;
  else if (mode === "tpl") p = cfg.templates[tplIdx].prompt;
  else p = cfg.ask_prompt;
  return p.replaceAll("{lang}", cfg.target_lang);
}

function addMsg(cls, s) {
  const d = document.createElement("div");
  d.className = "msg " + cls;
  d.textContent = s;
  $("out").appendChild(d);
  $("out").scrollTop = 1e9;
  return d;
}

function setBusy(b) {
  busy = b;
  $("stop").style.display = b ? "" : "none";
  $("send").disabled = b || !$("q").value.trim();
  document.querySelectorAll(".acts").forEach((a) => a.classList.toggle("dis", b));
}

const ICON_COPY = `<svg width="13" height="13" viewBox="0 0 16 16"><rect x="5" y="5" width="9" height="9" rx="2" fill="none" stroke="currentColor" stroke-width="1.4"/><path d="M11 5V3.5A1.5 1.5 0 0 0 9.5 2h-6A1.5 1.5 0 0 0 2 3.5v6A1.5 1.5 0 0 0 3.5 11H5" fill="none" stroke="currentColor" stroke-width="1.4"/></svg>`;
const ICON_REDO = `<svg width="13" height="13" viewBox="0 0 16 16"><path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9M13.5 2.5v3h-3" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>`;

function addActions(el, getText) {
  document.querySelectorAll(".acts .redo").forEach((b) => b.remove()); // 只有最后一条可重新生成
  const bar = document.createElement("div");
  bar.className = "acts";
  const copy = document.createElement("button");
  copy.innerHTML = ICON_COPY + "<span>复制</span>";
  copy.onclick = async () => {
    await navigator.clipboard.writeText(getText());
    copy.querySelector("span").textContent = "已复制";
    copy.classList.add("ok");
    setTimeout(() => { copy.querySelector("span").textContent = "复制"; copy.classList.remove("ok"); }, 1200);
  };
  const redo = document.createElement("button");
  redo.className = "redo";
  redo.innerHTML = ICON_REDO + "<span>重新生成</span>";
  redo.onclick = () => {
    if (busy) return;
    bar.remove();
    el.remove();
    history.pop(); // 去掉 assistant
    const u = history.pop(); // 取出 user 重新发
    run(u.content);
  };
  bar.append(copy, redo);
  el.after(bar);
  $("out").scrollTop = 1e9;
}

async function run(userContent) {
  if (busy) return;
  setBusy(true);
  history.push({ role: "user", content: userContent });
  const el = addMsg("loading", "思考中…");
  let acc = "";
  let stopped = false;
  stopFn = () => { stopped = true; invoke("stop_chat"); };
  const ch = new Channel();
  ch.onmessage = (s) => {
    acc += s;
    el.className = "msg md";
    el.innerHTML = renderMd(acc);
    $("out").scrollTop = 1e9;
  };
  try {
    await invoke("chat", {
      messages: [{ role: "system", content: sysPrompt() }, ...history],
      model: mode === "translate" ? null : curModel() || null,
      onEvent: ch,
    });
    if (stopped && !acc) { el.className = "msg hint"; el.textContent = "已停止"; }
    if (stopped && acc) el.insertAdjacentHTML("beforeend", `<div class="stopped">· 已停止生成</div>`);
    history.push({ role: "assistant", content: acc });
    const final = acc;
    addActions(el, () => final);
    if (acc) saveHistory();
  } catch (e) {
    el.className = "msg err";
    el.textContent = String(e);
    history.pop();
  }
  stopFn = null;
  setBusy(false);
}
let stopFn = null;

// 历史记录：同一次划词的追问合并为一条
let histId = 0;
function modeLabel() {
  if (mode === "translate") return isWord(text) ? "单词" : "翻译";
  if (mode === "code") return "代码";
  if (mode === "tpl") return cfg.templates[tplIdx].name;
  return "提问";
}
async function saveHistory() {
  const firstQ = (history.find((m) => m.role === "user") || {}).content || "";
  try {
    histId = await invoke("history_save", {
      item: {
        id: histId, mode: modeLabel(), source: text || firstQ.slice(0, 500),
        messages: JSON.stringify(history),
        model: mode === "translate" ? cfg.translate_model || cfg.model : curModel(),
      },
    });
  } catch (e) { console.error(e); }
}

function setMode(m, ti) {
  if (busy && stopFn) stopFn();
  mode = m;
  if (m === "tpl") tplIdx = ti;
  $("mTrans").classList.toggle("on", m === "translate");
  $("mAsk").classList.toggle("on", m === "ask");
  $("mCode").classList.toggle("on", m === "code");
  $("tBtn").classList.toggle("on", m === "tpl");
  $("tName").textContent = m === "tpl" ? cfg.templates[ti].name : "模板";
  $("askBox").style.display = m === "translate" ? "none" : "flex";
  const word = m === "translate" && isWord(text);
  $("word").style.display = word ? "" : "none";
  $("src").style.display = text && !word ? "" : "none";
  if (word) $("wText").textContent = text.trim();
  $("out").innerHTML = "";
  history = [];
  histId = 0;
  if (m === "translate" || m === "code" || m === "tpl") {
    if (text) run(text);
    else addMsg("hint", m === "translate" ? "没有选中的文本，切换到“提问”直接问吧" : "没有选中的内容，可以直接在下方输入");
    $("q").value = "";
    autoGrow();
    if (m !== "translate") setTimeout(() => $("q").focus(), 50);
  } else {
    addMsg("hint", text ? "针对选中的内容，你想问什么？" : "有什么想问的？");
    $("q").value = "";
    autoGrow();
    setTimeout(() => $("q").focus(), 50);
  }
}

function fillModels() {
  const list = cfg.ask_models && cfg.ask_models.length ? cfg.ask_models : [cfg.model];
  const cur = list.includes(cfg.ask_model) ? cfg.ask_model : list[0];
  $("mName").textContent = cur;
  $("mMenu").innerHTML = "";
  list.forEach((m) => {
    const d = document.createElement("div");
    d.textContent = m;
    if (m === cur) d.className = "cur";
    d.onclick = () => {
      cfg.ask_model = m;
      invoke("set_ask_model", { model: m });
      fillModels();
      $("mpick").classList.remove("open");
      $("q").focus();
    };
    $("mMenu").appendChild(d);
  });
  $("mBtn").disabled = list.length < 2;
}
const curModel = () => $("mName").textContent;
$("mBtn").onclick = (e) => { e.stopPropagation(); $("mpick").classList.toggle("open"); };
document.addEventListener("click", () => $("mpick").classList.remove("open"));

listen("selection", async (e) => {
  cfg = await invoke("get_config");
  fillModels();
  if (busy && stopFn) stopFn();
  text = e.payload;
  $("src").textContent = text;
  fillTemplates();
  if (!text) setMode("ask");
  else if (cfg.code_detect && isCode(text)) setMode("code");
  else setMode(isChinese(text) ? "ask" : "translate");
});

$("mTrans").onclick = () => setMode("translate");
$("mAsk").onclick = () => setMode("ask");
$("mCode").onclick = () => setMode("code");

function fillTemplates() {
  const list = cfg.templates || [];
  $("tpick").style.display = list.length ? "" : "none";
  $("tMenu").innerHTML = "";
  list.forEach((tp, i) => {
    const d = document.createElement("div");
    d.textContent = tp.name;
    if (mode === "tpl" && i === tplIdx) d.className = "cur";
    d.onclick = () => { $("tpick").classList.remove("open"); setMode("tpl", i); };
    $("tMenu").appendChild(d);
  });
}
$("tBtn").onclick = (e) => { e.stopPropagation(); fillTemplates(); $("tpick").classList.toggle("open"); };
document.addEventListener("click", () => $("tpick").classList.remove("open"));

// 朗读：Windows 自带语音（SAPI），离线可用
function speak() {
  const synth = window.speechSynthesis;
  synth.cancel();
  const u = new SpeechSynthesisUtterance(text.trim());
  const vs = synth.getVoices();
  const v = vs.find((x) => /en-US/i.test(x.lang)) || vs.find((x) => /^en/i.test(x.lang));
  if (v) u.voice = v;
  u.lang = v ? v.lang : "en-US";
  u.rate = 0.9;
  $("speak").classList.add("on");
  u.onend = u.onerror = () => $("speak").classList.remove("on");
  synth.speak(u);
}
$("speak").onclick = speak;

// 代码块复制
$("out").addEventListener("click", async (e) => {
  const b = e.target.closest(".cpy");
  if (!b) return;
  await navigator.clipboard.writeText(b.closest(".cb").querySelector("code").innerText);
  b.textContent = "已复制"; b.classList.add("ok");
  setTimeout(() => { b.textContent = "复制"; b.classList.remove("ok"); }, 1200);
});
$("close").onclick = () => win.hide();
$("pin").onclick = () => { pinned = !pinned; $("pin").classList.toggle("on", pinned); };

const send = () => {
  const q = $("q").value.trim();
  if (!q || busy) return;
  $("q").value = "";
  autoGrow();
  const hint = document.querySelector(".msg.hint");
  if (hint) hint.remove();
  addMsg("user", q);
  // 第一个问题带上选中的内容作为上下文
  const content = history.length === 0 && text ? `我选中了这段内容：\n"""\n${text}\n"""\n\n我的问题：${q}` : q;
  run(content);
};
$("send").onclick = send;
$("stop").onclick = () => stopFn && stopFn();
const autoGrow = () => { const q = $("q"); q.style.height = "auto"; q.style.height = q.scrollHeight + "px"; $("send").disabled = !q.value.trim(); };
$("q").addEventListener("input", autoGrow);
$("q").addEventListener("keydown", (e) => { if (e.key === "Enter" && !e.shiftKey && !e.isComposing) { e.preventDefault(); send(); } });
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") { if (busy && stopFn) stopFn(); else win.hide(); }
});

// 记住窗口大小
let sizeTimer;
win.onResized(() => {
  clearTimeout(sizeTimer);
  sizeTimer = setTimeout(async () => {
    const f = await win.scaleFactor();
    const s = await win.innerSize();
    invoke("save_popup_size", { w: s.width / f, h: s.height / f });
  }, 400);
});

// 失焦自动隐藏（未固定时）
win.onFocusChanged(({ payload: focused }) => {
  if (!focused && !pinned) { if (busy && stopFn) stopFn(); win.hide(); }
});
