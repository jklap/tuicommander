// Browser proof uses real VtLogBuffer snapshots exported by the Rust replay.
import { createRequire } from "node:module";
import { readFileSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
const [base, framesPath, out, pwDir] = process.argv.slice(2);
const { webkit, devices } = createRequire(`${pwDir}/`)("playwright-core");
const replay = JSON.parse(readFileSync(framesPath, "utf8"));
mkdirSync(out, { recursive: true });
rmSync(`${out}/report.json`, { force: true });
const browser = await webkit.launch({ headless: true, executablePath: process.env.TUIC_MOBILE_WEBKIT_LAUNCHER });
try {
    const page = await browser.newPage({ ...devices["iPhone 13"] });
    await page.goto(`${base}/tests/visual/mobile-reading/index.html`);
    await page.waitForFunction(() => window.mobileReplay);
    // Pick recently finalized, substantive producer text; it stays within the 500-row UI cap.
    const candidates = replay.expected.slice(-60).filter((text) => text.trim().length > 30);
    let selected;
    let before;
    let reading;
    for (let i = 0; i < replay.frames.length; i++) {
        const frame = replay.frames[i];
        await page.evaluate((f) => {
            if (f.lines.length) window.mobileReplay.onLogLines(f.lines);
            window.mobileReplay.onScreenRows(f.screen);
        }, frame);
        await page.evaluate(() => new Promise(requestAnimationFrame));
        const text = await page.locator("pre").innerText();
        if (!selected) {
            selected = candidates.find((candidate) => text.includes(candidate));
            if (selected) {
                before = i;
                await page.evaluate((target) => {
                    const row = [...document.querySelectorAll("pre > div")].find((el) => el.textContent.includes(target));
                    const box = document.querySelector("pre").parentElement;
                    box.scrollTop += row.getBoundingClientRect().top - box.getBoundingClientRect().top - 100;
                    box.scrollTop = Math.max(0, box.scrollTop - 250);
                    box.dispatchEvent(new Event("scroll"));
                }, selected);
                await page.evaluate(() => new Promise(requestAnimationFrame));
                reading = await page.evaluate((target) => {
                    const row = [...document.querySelectorAll("pre > div")].find((el) => el.textContent.includes(target));
                    const box = document.querySelector("pre").parentElement;
                    return { distanceFromBottom: box.scrollHeight - box.scrollTop - box.clientHeight,
                        scrollTop: box.scrollTop, visible: row.getBoundingClientRect().top >= box.getBoundingClientRect().top && row.getBoundingClientRect().bottom <= box.getBoundingClientRect().bottom };
                }, selected);
                if (reading.distanceFromBottom <= 200 || !reading.visible) throw new Error(`Replay did not reach a visible reading position above bottom: ${JSON.stringify(reading)}`);
                await page.screenshot({ path: `${out}/before.png` });
            }
        } else if (!text.includes(selected)) {
            await page.screenshot({ path: `${out}/after.png` });
            writeFileSync(`${out}/report.json`, JSON.stringify({ selected, before, reading, after: i, missing: true, ua: await page.evaluate(() => navigator.userAgent) }, null, 2));
            throw new Error(`Already displayed producer text disappeared at frame ${i}: ${selected}`);
        }
    }
    if (!selected) throw new Error("No substantive recorded producer text was displayed");
    await page.screenshot({ path: `${out}/after.png` });
    writeFileSync(`${out}/report.json`, JSON.stringify({ selected, before, reading, frames: replay.frames.length, missing: false, ua: await page.evaluate(() => navigator.userAgent) }, null, 2));
    console.log("Recorded mobile text retained through replay");
} finally {
    await browser.close();
}
