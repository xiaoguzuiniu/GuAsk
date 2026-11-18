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

async function run(userContent) {
  if (busy) return;
  busy = true;
  history.push({ role: "user", content: userContent });
  const el = addMsg("loading", "思考中…");
  let acc = "";
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
    history.push({ role: "assistant", content: acc });
  } catch (e) {
    el.className = "msg err";
    el.textContent = String(e);
    history.pop();
  }
  busy = false;
}

function setMode(m) {
  mode = m;
  $("mTrans").classList.toggle("on", m === "translate");
  $("mAsk").classList.toggle("on", m === "ask");
  $("askBox").style.display = m === "ask" ? "flex" : "none";
  $("out").innerHTML = "";
  history = [];
  if (m === "translate") {
    run(text);
  } else {
    addMsg("hint", "针对选中的内容，你想问什么？");
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
  text = e.payload;
  $("src").textContent = text;
  setMode(isChinese(text) ? "ask" : "translate");
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
  const content = history.length === 0 ? `我选中了这段内容：\n"""\n${text}\n"""\n\n我的问题：${q}` : q;
  run(content);
};
$("send").onclick = send;
const autoGrow = () => { const q = $("q"); q.style.height = "auto"; q.style.height = q.scrollHeight + "px"; $("send").disabled = !q.value.trim(); };
$("q").addEventListener("input", autoGrow);
$("q").addEventListener("keydown", (e) => { if (e.key === "Enter" && !e.shiftKey && !e.isComposing) { e.preventDefault(); send(); } });
document.addEventListener("keydown", (e) => { if (e.key === "Escape") win.hide(); });

// 失焦自动隐藏（未固定时）
win.onFocusChanged(({ payload: focused }) => {
  if (!focused && !pinned) win.hide();
});
