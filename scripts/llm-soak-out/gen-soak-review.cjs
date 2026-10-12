const fs = require("fs");
const path = require("path");

const dir = __dirname;
const data = JSON.parse(
  fs.readFileSync(path.join(dir, "soak-review-data.json"), "utf8"),
);

function esc(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function lcsOps(a, b) {
  const m = a.length;
  const n = b.length;
  const dp = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));
  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      dp[i][j] =
        a[i - 1] === b[j - 1]
          ? dp[i - 1][j - 1] + 1
          : Math.max(dp[i - 1][j], dp[i][j - 1]);
    }
  }
  const ops = [];
  let i = m;
  let j = n;
  while (i > 0 || j > 0) {
    if (i > 0 && j > 0 && a[i - 1] === b[j - 1]) {
      ops.push({ t: "eq", c: a[i - 1] });
      i--;
      j--;
    } else if (j > 0 && (i === 0 || dp[i][j - 1] >= dp[i - 1][j])) {
      ops.push({ t: "ins", c: b[j - 1] });
      j--;
    } else {
      ops.push({ t: "del", c: a[i - 1] });
      i--;
    }
  }
  ops.reverse();
  return ops;
}

function renderDiff(ops) {
  return ops
    .map((o) => {
      const c = esc(o.c);
      if (o.t === "eq") return `<span class="eq">${c}</span>`;
      if (o.t === "del") return `<span class="del">${c}</span>`;
      return `<span class="ins">${c}</span>`;
    })
    .join("");
}

function letterChanges(ops) {
  const dels = ops.filter((o) => o.t === "del").map((o) => o.c);
  const inns = ops.filter((o) => o.t === "ins").map((o) => o.c);
  if (!dels.length && !inns.length) {
    return '<span class="muted">без изменений</span>';
  }
  const vis = (chars) =>
    chars.map((c) => (c === " " ? "␠" : c === "\n" ? "↵" : c)).join("");
  let html = "";
  if (dels.length) {
    html += `<span class="chip del-chip">− ${esc(vis(dels))}</span>`;
  }
  if (inns.length) {
    html += `<span class="chip ins-chip">+ ${esc(vis(inns))}</span>`;
  }
  return html;
}

const cards = data.cases
  .map((c) => {
    const ops = lcsOps(c.input, c.output);
    const badge = !c.ok
      ? "fail"
      : !c.expectHit
        ? "miss"
        : c.changed
          ? "fix"
          : "same";
    const badgeLabel = !c.ok
      ? "fail"
      : !c.expectHit
        ? "miss expect"
        : c.changed
          ? "исправлено"
          : "без изменений";
    return `
  <article class="card ${badge}">
    <header>
      <code class="id">${esc(c.id)}</code>
      <span class="badge">${badgeLabel}</span>
      <span class="meta">×${c.count} · avg ${c.msAvg}ms (${c.msMin}–${c.msMax})</span>
    </header>
    <div class="row">
      <div class="label">было</div>
      <div class="text before">${esc(c.input)}</div>
    </div>
    <div class="row">
      <div class="label">стало</div>
      <div class="text after">${esc(c.output)}</div>
    </div>
    <div class="row">
      <div class="label">буквы</div>
      <div class="text diff">${renderDiff(ops)}</div>
    </div>
    <div class="changes">${letterChanges(ops)}</div>
  </article>`;
  })
  .join("\n");

const html = `<!DOCTYPE html>
<html lang="ru">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>Mote soak review — ${esc(data.source)}</title>
<style>
  :root {
    --bg: #12141a;
    --panel: #1c2030;
    --text: #e8eaf0;
    --muted: #8b93a7;
    --del: #ff6b7a;
    --del-bg: rgba(255,107,122,.18);
    --ins: #5ddea8;
    --ins-bg: rgba(93,222,168,.18);
    --fix: #5b9cff;
    --same: #8b93a7;
    --miss: #f0c674;
    --fail: #ff6b7a;
    --border: #2a3148;
  }
  * { box-sizing: border-box; }
  body {
    margin: 0; padding: 24px 20px 64px;
    font: 15px/1.45 "Segoe UI", system-ui, sans-serif;
    background: radial-gradient(1200px 600px at 10% -10%, #243056 0%, transparent 55%), var(--bg);
    color: var(--text);
  }
  h1 { font-size: 1.35rem; font-weight: 650; margin: 0 0 6px; }
  .sub { color: var(--muted); margin-bottom: 18px; font-size: .92rem; }
  .filters { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 18px; }
  .filters button {
    background: var(--panel); color: var(--text); border: 1px solid var(--border);
    border-radius: 8px; padding: 6px 12px; cursor: pointer; font: inherit;
  }
  .filters button.active { border-color: var(--fix); color: #cfe0ff; }
  .grid { display: grid; gap: 14px; grid-template-columns: repeat(auto-fill, minmax(340px, 1fr)); }
  .card {
    background: var(--panel); border: 1px solid var(--border); border-radius: 12px;
    padding: 14px 14px 12px; display: flex; flex-direction: column; gap: 8px;
  }
  .card.fix { border-left: 3px solid var(--fix); }
  .card.same { border-left: 3px solid var(--same); opacity: .92; }
  .card.miss { border-left: 3px solid var(--miss); }
  .card.fail { border-left: 3px solid var(--fail); }
  header { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin-bottom: 2px; }
  .id { background: #10131c; padding: 2px 7px; border-radius: 6px; color: #b8c4e0; }
  .badge {
    font-size: .78rem; text-transform: uppercase; letter-spacing: .04em;
    padding: 2px 8px; border-radius: 999px; background: #10131c; color: var(--muted);
  }
  .card.fix .badge { color: #cfe0ff; }
  .card.miss .badge { color: var(--miss); }
  .card.fail .badge { color: var(--fail); }
  .meta { color: var(--muted); font-size: .82rem; margin-left: auto; }
  .row { display: grid; grid-template-columns: 52px 1fr; gap: 8px; align-items: start; }
  .label { color: var(--muted); font-size: .78rem; padding-top: 3px; }
  .text { white-space: pre-wrap; word-break: break-word; background: #10131c; border-radius: 8px; padding: 8px 10px; }
  .diff { font-family: ui-monospace, "Cascadia Mono", Consolas, monospace; font-size: .95rem; letter-spacing: .01em; }
  .eq { color: var(--text); }
  .del {
    color: var(--del); background: var(--del-bg); text-decoration: line-through;
    border-radius: 3px; padding: 0 1px;
  }
  .ins {
    color: var(--ins); background: var(--ins-bg); border-radius: 3px; padding: 0 1px;
    font-weight: 600;
  }
  .changes { display: flex; flex-wrap: wrap; gap: 6px; min-height: 22px; }
  .chip {
    font-family: ui-monospace, Consolas, monospace; font-size: .82rem;
    padding: 2px 8px; border-radius: 6px;
  }
  .del-chip { background: var(--del-bg); color: var(--del); }
  .ins-chip { background: var(--ins-bg); color: var(--ins); }
  .muted { color: var(--muted); font-size: .85rem; }
  .hidden { display: none !important; }
</style>
</head>
<body>
  <h1>Soak: исправления слов и букв</h1>
  <div class="sub">${esc(data.source)} · запросов ${data.total} · уникальных кейсов ${data.unique} · красный = удалено, зелёный = добавлено</div>
  <div class="filters">
    <button type="button" data-filter="all" class="active">все</button>
    <button type="button" data-filter="fix">только исправления</button>
    <button type="button" data-filter="same">без изменений</button>
    <button type="button" data-filter="miss">miss expect</button>
  </div>
  <div class="grid" id="grid">
${cards}
  </div>
<script>
document.querySelector(".filters").addEventListener("click", (e) => {
  const btn = e.target.closest("button[data-filter]");
  if (!btn) return;
  document.querySelectorAll(".filters button").forEach((b) => b.classList.toggle("active", b === btn));
  const f = btn.dataset.filter;
  document.querySelectorAll(".card").forEach((card) => {
    const show = f === "all" || card.classList.contains(f);
    card.classList.toggle("hidden", !show);
  });
});
</script>
</body>
</html>
`;

const out = path.join(dir, "soak-review.html");
fs.writeFileSync(out, html, "utf8");
console.log("wrote", out);
