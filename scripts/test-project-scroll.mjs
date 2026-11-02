/** 在真实浏览器中验收项目切换后的阅读位置。先启动 npm run dev。 */
import { chromium } from 'playwright';
import assert from 'node:assert/strict';

const browser = await chromium.launch({ channel: process.env.TASKRAIL_BROWSER ?? 'msedge', headless: true });
try {
 for (const mobile of [false, true]) {
  const page = await browser.newPage({ viewport: mobile ? { width: 390, height: 844 } : { width: 1280, height: 900 }, isMobile: mobile, hasTouch: mobile });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(`${process.env.TASKRAIL_URL ?? 'http://127.0.0.1:1420'}/tests/browser-project-scroll.html${mobile ? '?mobile' : ''}`);
  await page.getByRole('button', { name: '验证项目切换' }).click();
  await page.waitForFunction(() => document.querySelector('#result')?.textContent?.includes('"pass"'));
  const result = JSON.parse(await page.locator('#result').textContent());
  if (!result.pass) console.log(JSON.stringify(result, null, 2));
  assert.deepEqual(errors, []);
  assert(result.pass, '切换项目后阅读位置发生偏移');
  console.log(`${mobile ? '手机' : '桌面'}项目切换阅读位置验收通过（${result.scenarios.length} 个场景）。`);
  await page.close();
 }
} finally { await browser.close(); }
