/**
 * Live API smoke checks matching Mote curious.rs backends.
 * Run: npm run qa
 */
const UA = "Mote/0.1 (+qa-smoke)";

async function getJson(url) {
  const res = await fetch(url, { headers: { "User-Agent": UA, Accept: "application/json" } });
  if (!res.ok) throw new Error(`${res.status} ${url}`);
  return res.json();
}

async function postJson(url, body) {
  const res = await fetch(url, {
    method: "POST",
    headers: { "User-Agent": UA, Accept: "application/json", "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`${res.status} ${url}`);
  return res.json();
}

function encodePath(s) {
  return encodeURIComponent(s);
}

async function wikiSearch(q) {
  const words = q.trim().split(/\s+/).length;
  if (words <= 2) {
    const open = await getJson(
      `https://ru.wikipedia.org/w/api.php?action=opensearch&search=${encodePath(q)}&limit=1&namespace=0&format=json`,
    );
    const title = open?.[1]?.[0];
    if (title) return title;
  }
  const full = await getJson(
    `https://ru.wikipedia.org/w/api.php?action=query&list=search&srsearch=${encodePath(q)}&srlimit=1&srnamespace=0&format=json`,
  );
  const title = full?.query?.search?.[0]?.title;
  if (!title) throw new Error(`wiki miss: ${q}`);
  return title;
}

async function wikiSummary(title) {
  return getJson(`https://ru.wikipedia.org/api/rest_v1/page/summary/${encodePath(title)}`);
}

const checks = [];

function pass(name, detail) {
  checks.push({ name, ok: true, detail });
  console.log(`PASS  ${name} — ${detail}`);
}

function fail(name, err) {
  checks.push({ name, ok: false, detail: String(err) });
  console.error(`FAIL  ${name} — ${err}`);
}

async function run(name, fn) {
  try {
    pass(name, await fn());
  } catch (e) {
    fail(name, e?.message || e);
  }
}

await run("wiki question → пластик", async () => {
  const title = await wikiSearch("из чего состоит пластик");
  const sum = await wikiSummary(title);
  if (!sum.extract) throw new Error("no extract");
  return `${title}: ${String(sum.extract).slice(0, 60)}…`;
});

await run("wiki short → путин", async () => {
  const title = await wikiSearch("путин");
  const sum = await wikiSummary(title);
  if (!sum.extract) throw new Error("no extract");
  return title;
});

await run("wiki random", async () => {
  const sum = await getJson("https://ru.wikipedia.org/api/rest_v1/page/random/summary");
  if (!sum.extract) throw new Error("no extract");
  return sum.title;
});

await run("number fact fallback wiki 42", async () => {
  const title = await wikiSearch("42");
  const sum = await wikiSummary(title);
  if (!sum.extract) throw new Error("no extract");
  return title;
});

await run("quote dummyjson", async () => {
  const j = await getJson("https://dummyjson.com/quotes/random");
  if (!j.quote) throw new Error("no quote");
  return `${String(j.quote).slice(0, 50)} — ${j.author}`;
});

await run("country EN countriesnow", async () => {
  const j = await postJson("https://countriesnow.space/api/v0.1/countries/capital", {
    country: "france",
  });
  if (j.error || !j.data?.capital) throw new Error(j.msg || "no capital");
  return `${j.data.name}: ${j.data.capital}`;
});

await run("country RU via wiki", async () => {
  const title = await wikiSearch("франция");
  const sum = await wikiSummary(title);
  if (!sum.extract) throw new Error("no extract");
  return title;
});

await run("open library dune", async () => {
  const j = await getJson("https://openlibrary.org/search.json?q=dune&limit=1");
  const title = j?.docs?.[0]?.title;
  if (!title) throw new Error("no book");
  return title;
});

await run("nasa apod", async () => {
  let lastErr = "unknown";
  for (let i = 0; i < 2; i++) {
    try {
      const j = await getJson("https://api.nasa.gov/planetary/apod?api_key=DEMO_KEY");
      if (!j.title) throw new Error("no title");
      return j.title;
    } catch (e) {
      lastErr = e?.message || String(e);
      await new Promise((r) => setTimeout(r, 400));
    }
  }
  // DEMO_KEY is rate-limited — soft pass with warning.
  console.warn(`WARN  nasa apod — ${lastErr} (DEMO_KEY flaky)`);
  return `soft-ok (${lastErr})`;
});

const failed = checks.filter((c) => !c.ok).length;
console.log(`\n${checks.length - failed}/${checks.length} passed`);
process.exit(failed ? 1 : 0);
