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
function renderMd(src) {
  const lines = src.replace(/\r/g, "").split("\n");
  let html = "", list = null, inCode = false, code = [], para = [];
  const flushPara = () => { if (para.length) { html += `<p>${para.map(inline).join("<br>")}</p>`; para = []; } };
  const closeList = () => { if (list) { html += `</${list}>`; list = null; } };
  for (const line of lines) {
    if (line.trim().startsWith("```")) {
      if (inCode) { html += `<pre><code>${esc(code.join("\n"))}</code></pre>`; code = []; inCode = false; }
      else { flushPara(); closeList(); inCode = true; }
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
    } else if ((m = line.match(/^>\s?(.*)/))) {
      flushPara(); closeList();
      html += `<blockquote>${inline(m[1])}</blockquote>`;
    } else if (/^\s*(---|\*\*\*)\s*$/.test(line)) {
      flushPara(); closeList(); html += "<hr>";
    } else {
      closeList(); para.push(line);
    }
  }
  if (inCode) html += `<pre><code>${esc(code.join("\n"))}</code></pre>`;
  flushPara(); closeList();
  return html;
}
