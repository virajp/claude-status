const { chromium } = require("playwright-core");
const exe = process.env.HOME
  + "/Library/Caches/ms-playwright/chromium_headless_shell-1243/chrome-headless-shell-mac-arm64/chrome-headless-shell";
(async () => {
  const b = await chromium.launch({ executablePath: exe });
  const p = await b.newPage({ viewport: { width: 1920, height: 1080 } });
  await p.goto("file://" + __dirname + "/index.html");
  await p.evaluate(() => document.fonts.ready);
  await p.evaluate(() =>
    Promise.all([...document.fonts].map(f => f.load().catch(() => 0)))
  );
  await p.waitForTimeout(300);
  const mode = process.argv[2];
  if (mode === "stills") {
    for (const t of process.argv.slice(3)) {
      await p.evaluate(t => render(t), +t);
      await p.screenshot({ path: `still-${t}.png` });
    }
  }
  else {
    require("fs").mkdirSync("frames", { recursive: true });
    const N = Math.round(21.5 * 30);
    for (let i = 0; i < N; i++) {
      await p.evaluate(t => render(t), i / 30);
      await p.screenshot({ path: `frames/f${String(i).padStart(4, "0")}.png` });
    }
  }
  await b.close();
})();
