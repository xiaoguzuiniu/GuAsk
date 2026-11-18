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

function sysPrompt() {
  const p = mode === "translate" ? cfg.translate_prompt : cfg.ask_prompt;
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
      model: mode === "ask" ? curModel() || null : null,
      onEvent: ch,
    });
    if (stopped && !acc) { el.className = "msg hint"; el.textContent = "已停止"; }
    if (stopped && acc) el.insertAdjacentHTML("beforeend", `<div class="stopped">· 已停止生成</div>`);
    history.push({ role: "assistant", content: acc });
    const final = acc;
    addActions(el, () => final);
  } catch (e) {
    el.className = "msg err";
    el.textContent = String(e);
    history.pop();
  }
  stopFn = null;
  setBusy(false);
}
let stopFn = null;

function setMode(m) {
  mode = m;
  $("mTrans").classList.toggle("on", m === "translate");
  $("mAsk").classList.toggle("on", m === "ask");
  $("askBox").style.display = m === "ask" ? "flex" : "none";
  $("out").innerHTML = "";
  history = [];
  if (m === "translate") {
    if (text) run(text); else addMsg("hint", "没有选中的文本，切换到“提问”直接问吧");
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
  $("src").style.display = text ? "" : "none";
  setMode(!text || isChinese(text) ? "ask" : "translate");
});

$("mTrans").onclick = () => setMode("translate");
$("mAsk").onclick = () => setMode("ask");
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
