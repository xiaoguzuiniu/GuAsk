// 轻量 Markdown 渲染（标题/粗斜体/行内代码/代码块/列表/引用/段落），先转义防注入
function esc(s) {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
function inline(s) {
  return esc(s)
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*\s*([^*]+?)\s*\*\*/g, "<b>$1</b>")
    .replace(/(^|[^*])\*([^*\s][^*]*?)\*/g, "$1<i>$2</i>");
}

// 轻量语法高亮：注释 / 字符串 / 数字 / 关键字 / 函数调用 / 类型
const KW = new Set((
  "if else elif for while do switch case break continue return function func fn def class struct enum interface impl trait type " +
  "import from export default as package use mod pub let const var mut static final public private protected new delete try catch finally throw throws raise except with " +
  "async await yield lambda in of is not and or null nil None true false True False this self super void int long float double bool boolean string char byte " +
  "extends implements where match loop select insert update into values create table alter drop join on group by order limit go defer chan map range"
).split(" "));
const HASH_LANG = /^(py|python|sh|bash|shell|zsh|yaml|yml|toml|rb|ruby|ps1|powershell|pwsh|r|perl|conf|ini|dockerfile|makefile|nginx)$/i;
const DASH_LANG = /^(sql|lua|haskell|hs)$/i;

function highlight(code, lang) {
  lang = lang || "";
  const cm = ["\\/\\*[\\s\\S]*?\\*\\/", "\\/\\/[^\\n]*"];
  if (HASH_LANG.test(lang)) cm.push("#[^\\n]*");
  if (DASH_LANG.test(lang)) cm.push("--[^\\n]*");
  const str = "\"(?:\\\\.|[^\"\\\\\\n])*\"|'(?:\\\\.|[^'\\\\\\n])*'|`(?:\\\\.|[^`\\\\])*`";
  const num = "\\b0x[\\da-fA-F]+\\b|\\b\\d+(?:\\.\\d+)?\\b";
  const id = "[A-Za-z_$][\\w$]*";
  const re = new RegExp(`(${cm.join("|")})|(${str})|(${num})|(${id})`, "g");
  let out = "", last = 0, m;
  while ((m = re.exec(code))) {
    out += esc(code.slice(last, m.index));
    const s = esc(m[0]);
    if (m[1]) out += `<span class="hl-c">${s}</span>`;
    else if (m[2]) out += `<span class="hl-s">${s}</span>`;
    else if (m[3]) out += `<span class="hl-n">${s}</span>`;
    else if (KW.has(m[4])) out += `<span class="hl-k">${s}</span>`;
    else if (/^\s*\(/.test(code.slice(re.lastIndex))) out += `<span class="hl-f">${s}</span>`;
    else if (/^[A-Z][a-z]\w*$/.test(m[4])) out += `<span class="hl-t">${s}</span>`;
    else out += s;
    last = re.lastIndex;
  }
  return out + esc(code.slice(last));
}

function codeBlock(code, lang) {
  return `<div class="cb"><div class="cb-h"><span>${esc(lang || "code")}</span><button class="cpy" type="button">复制</button></div><pre><code>${highlight(code, lang)}</code></pre></div>`;
}

function renderMd(src) {
  const lines = src.replace(/\r/g, "").split("\n");
  let html = "", list = null, inCode = false, code = [], para = [], lang = "";
  const flushPara = () => { if (para.length) { html += `<p>${para.map(inline).join("<br>")}</p>`; para = []; } };
  const closeList = () => { if (list) { html += `</${list}>`; list = null; } };
  for (const line of lines) {
    if (line.trim().startsWith("```")) {
      if (inCode) { html += codeBlock(code.join("\n"), lang); code = []; inCode = false; }
      else { flushPara(); closeList(); inCode = true; lang = line.trim().slice(3).trim(); }
      continue;
    }
    if (inCode) { code.push(line); continue; }
    let m;
    if (!line.trim()) { flushPara(); closeList(); continue; }
    if ((m = line.match(/^(#{1,6})\s+(.*)/))) {
      flushPara(); closeList();
      const lv = Math.min(m[1].length + 2, 6);
      html += `<h${lv}>${inline(m[2])}</h${lv}>`;
    } else if ((m = line.match(/^\s*[-*+]\s+(.*)/)) || (m = line.match(/^\s*(\d+)[.)]\s+(.*)/))) {
      flushPara();
      const t = m.length === 3 ? "ol" : "ul";
      if (list !== t) { closeList(); html += t === "ol" ? `<ol start="${m[1]}">` : "<ul>"; list = t; }
      html += `<li>${inline(m[m.length - 1])}</li>`;
    } else if (list && /^\s{2,}\S/.test(line)) {
      // 列表项的续行（如例句下的翻译）
      html = html.replace(/<\/li>$/, `<br><span class="sub">${inline(line.trim())}</span></li>`);
    } else if ((m = line.match(/^>\s?(.*)/))) {
      flushPara(); closeList();
      html += `<blockquote>${inline(m[1])}</blockquote>`;
    } else if (/^\s*(---|\*\*\*)\s*$/.test(line)) {
      flushPara(); closeList(); html += "<hr>";
    } else {
      closeList(); para.push(line);
    }
  }
  if (inCode) html += codeBlock(code.join("\n"), lang);
  flushPara(); closeList();
  return html;
}
