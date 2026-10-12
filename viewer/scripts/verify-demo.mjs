import { chromium } from 'playwright';
import fs from 'node:fs/promises';
import path from 'node:path';

const url = process.argv[2] ?? 'http://127.0.0.1:4173/';
const output = process.argv[3];
if (!output) throw new Error('Usage: node scripts/verify-demo.mjs URL OUTPUT_DIRECTORY');
await fs.mkdir(output, { recursive: true });
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
const errors = [];
page.on('pageerror', error => errors.push(error.message));
await page.goto(url);
await page.getByRole('region', { name: 'Game replay' }).waitFor();
if (await page.getByRole('alert').count()) throw new Error(await page.getByRole('alert').innerText());
const manifest = await (await page.request.get(new URL('demo/manifest.json', url).href)).json();
const recording = await (await page.request.get(new URL(`demo/${manifest.games[0].file}`, url).href)).json();
if (!recording.complete || recording.bookmarks.length !== 3) throw new Error('Demo is not complete/annotated');
const slider = page.getByRole('slider', { name: 'Decision timeline' });
const moments = [0, 1, 7, 15, 21, 22, 100, 101, 124, 125];
const seekTimings = [];
for (let index = 0; index < recording.frames.length; index++) {
  const timing = await page.evaluate(async index => {
    const input = document.querySelector('input[aria-label="Decision timeline"]');
    const started = performance.now();
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, String(index));
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
    for (let attempt = 0; attempt < 120; attempt++) {
      await new Promise(requestAnimationFrame);
      if (document.querySelector('.decision-count b')?.textContent === String(index + 1)) return performance.now() - started;
    }
    throw new Error(`Seek failed at ${index}`);
  }, index);
  seekTimings.push(timing);
  if (!await page.getByText('Recorded action', { exact: true }).isVisible()) throw new Error(`Action missing at ${index}`);
  if (moments.includes(index)) {
    await page.waitForFunction(() => [...document.images].every(image => image.complete));
    await page.screenshot({ path: path.join(output, `decision-${index}.png`), fullPage: true });
  }
}
await slider.fill('15');
await page.getByRole('tab', { name: 'Search' }).click();
await page.getByText('Share of search visits', { exact: true }).waitFor();
await page.screenshot({ path: path.join(output, 'search-comparison.png'), fullPage: true });
for (const viewport of [{ width: 1920, height: 1080 }, { width: 320, height: 900 }]) {
  await page.setViewportSize(viewport);
  if (!await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)) throw new Error(`Overflow at ${viewport.width}`);
  await page.screenshot({ path: path.join(output, `layout-${viewport.width}.png`), fullPage: true });
}
await page.setViewportSize({ width: 1440, height: 900 });
await page.emulateMedia({ reducedMotion: 'reduce' });
await page.getByRole('button', { name: 'About the agent' }).click();
await page.getByRole('dialog').waitFor();
await page.keyboard.press('Escape');
if (await page.getByRole('dialog').count()) throw new Error('Escape did not close modal');
await page.emulateMedia({ reducedMotion: 'no-preference' });
await slider.fill('0');
await page.getByRole('combobox', { name: 'Playback speed' }).selectOption('4');
await page.evaluate(() => {
  window.__frameDeltas = [];
  let previous = performance.now();
  const sample = time => {
    window.__frameDeltas.push(time - previous); previous = time;
    window.__frameRequest = requestAnimationFrame(sample);
  };
  window.__frameRequest = requestAnimationFrame(sample);
});
await page.getByRole('button', { name: 'Play recording' }).click();
console.log('Watching complete recorded playback at 4×');
await page.waitForFunction(() => Boolean(document.querySelector('.terminal-result')), undefined, { timeout: 180000 });
await page.getByRole('button', { name: 'Play recording' }).waitFor({ timeout: 10000 });
const deltas = await page.evaluate(() => { cancelAnimationFrame(window.__frameRequest); return window.__frameDeltas; });
await page.screenshot({ path: path.join(output, 'terminal.png'), fullPage: true });
const quantile = (values, q) => [...values].sort((a,b) => a-b)[Math.floor((values.length-1)*q)];
const report = {
  url, game: recording.id, observedFrames: recording.frames.length, bookmarks: recording.bookmarks.map(b => b.frame),
  searchDecisions: recording.frames.filter(f => f.search).length, consoleErrors: errors,
  seek: { medianMs: quantile(seekTimings,.5), p95Ms: quantile(seekTimings,.95), maxMs: Math.max(...seekTimings), targetMs: 100 },
  playback: { sampledFrames: deltas.length, averageFps: 1000 / (deltas.reduce((a,b) => a+b,0) / deltas.length), p95FrameMs: quantile(deltas,.95), framesOver50Ms: deltas.filter(n=>n>50).length },
  terminal: recording.frames.at(-1).terminal,
};
await fs.writeFile(path.join(output,'report.json'),JSON.stringify(report,null,2)+'\n');
await browser.close();
console.log(JSON.stringify(report));
if (errors.length || report.seek.maxMs >= 100) process.exitCode = 1;
