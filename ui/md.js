// 轻量 Markdown 渲染（标题/粗斜体/行内代码/代码块/列表/引用/段落），先转义防注入
function esc(s) {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
function inline(s) {
  // 行内代码先抽出来，避免内部的 * \ 被当作格式
  const codes = [];
  s = s.replace(/`([^`]+)`/g, (_, c) => { codes.push(c); return `\u0000${codes.length - 1}\u0000`; });
  // 反斜杠转义：\| \* \_ \` \# 等显示为字符本身
  const escs = [];
  s = s.replace(/\\([\\`*_{}\[\]()#+\-.!|<>~])/g, (_, c) => { escs.push(c); return `\u0001${escs.length - 1}\u0001`; });
  return esc(s)
    .replace(/\*\*\s*([^*]+?)\s*\*\*/g, "<b>$1</b>")
    .replace(/(^|[^*])\*([^*\s][^*]*?)\*/g, "$1<i>$2</i>")
    .replace(/~~([^~]+)~~/g, "<s>$1</s>")
    .replace(/\u0001(\d+)\u0001/g, (_, i) => esc(escs[i]))
    .replace(/\u0000(\d+)\u0000/g, (_, i) => `<code>${esc(codes[i])}</code>`);
}

// 表格行拆分：忽略首尾竖线，跳过 \| 和行内代码里的 |
function splitRow(line) {
  let s = line.trim().replace(/^\|/, "");
  if (/[^\\]\|$/.test(s) || s === "|") s = s.slice(0, -1);
  const cells = []; let cur = "", inCode = false;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (c === "\\" && s[i + 1] === "|") { cur += "\\|"; i++; continue; }
    if (c === "`") inCode = !inCode;
    if (c === "|" && !inCode) { cells.push(cur.trim()); cur = ""; continue; }
    cur += c;
  }
  cells.push(cur.trim());
  return cells;
}
const isSep = (l) => /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/.test(l);

function table(head, sep, rows) {
  const al = splitRow(sep).map((c) => (/^:-+:$/.test(c) ? "center" : /-:$/.test(c) ? "right" : ""));
  const td = (tag, c, i) => `<${tag}${al[i] ? ` style="text-align:${al[i]}"` : ""}>${inline(c)}</${tag}>`;
  const h = splitRow(head);
  let out = `<div class="tbl"><table><thead><tr>${h.map((c, i) => td("th", c, i)).join("")}</tr></thead><tbody>`;
  for (const r of rows) {
    const cs = splitRow(r);
    out += `<tr>${h.map((_, i) => td("td", cs[i] || "", i)).join("")}</tr>`;
  }
  return out + "</tbody></table></div>";
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
  for (let li = 0; li < lines.length; li++) {
    const line = lines[li];
    if (line.trim().startsWith("```")) {
      if (inCode) { html += codeBlock(code.join("\n"), lang); code = []; inCode = false; }
      else { flushPara(); closeList(); inCode = true; lang = line.trim().slice(3).trim(); }
      continue;
    }
    if (inCode) { code.push(line); continue; }
    let m;
    if (!line.trim()) { flushPara(); closeList(); continue; }
    // 表格：当前行含 |，下一行是 |---|---| 分隔线
    if (line.includes("|") && li + 1 < lines.length && isSep(lines[li + 1])) {
      flushPara(); closeList();
      const rows = [];
      let j = li + 2;
      while (j < lines.length && lines[j].includes("|") && lines[j].trim()) rows.push(lines[j++]);
      html += table(line, lines[li + 1], rows);
      li = j - 1;
      continue;
    }
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
