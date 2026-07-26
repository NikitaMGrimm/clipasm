import { createReadStream } from "node:fs";
import { realpath, stat } from "node:fs/promises";
import { createServer } from "node:http";
import { extname, resolve, sep } from "node:path";
import process from "node:process";

import { chromium } from "playwright-core";

const MIME_TYPES = new Map([
    [".css", "text/css; charset=utf-8"],
    [".html", "text/html; charset=utf-8"],
    [".js", "text/javascript; charset=utf-8"],
    [".png", "image/png"],
    [".wasm", "application/wasm"],
]);
const bookRoot = await realpath(resolve(process.argv[2] ?? "../../target/book"));
const server = createServer(serveBook);
await new Promise((resolveListen, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolveListen);
});

const address = server.address();
if (!address || typeof address === "string") {
    throw new Error("The browser smoke-test server did not expose a TCP port.");
}

let browser;
try {
    browser = process.env.PLAYWRIGHT_CDP_URL
        ? await chromium.connectOverCDP(process.env.PLAYWRIGHT_CDP_URL)
        : await chromium.launch({
              ...(process.env.CHROME_BIN
                  ? { executablePath: process.env.CHROME_BIN }
                  : { channel: "chrome" }),
              headless: true,
              args: ["--disable-dev-shm-usage", "--no-sandbox"],
          });
    const context = browser.contexts()[0] ?? (await browser.newContext());
    const page = await context.newPage();
    await page.addInitScript(() => {
        const digest = SubtleCrypto.prototype.digest;
        window.__clipasmDelayDigest = true;
        window.__clipasmDigestStarted = false;
        SubtleCrypto.prototype.digest = async function (...arguments_) {
            window.__clipasmDigestStarted = true;
            if (window.__clipasmDelayDigest) {
                await new Promise((resolveDelay) => window.setTimeout(resolveDelay, 750));
            }
            return digest.apply(this, arguments_);
        };
    });
    const pageErrors = [];
    page.on("console", (message) => {
        if (message.type() === "error") {
            pageErrors.push(message.text());
        }
    });
    page.on("pageerror", (error) => pageErrors.push(error.message));

    await page.goto(`http://127.0.0.1:${address.port}/try-clipasm.html`);
    const status = page.locator(".clipasm-playground__status");
    const render = page.getByRole("button", { name: "Render video" });
    await render.click();
    await page.waitForFunction(() => window.__clipasmDigestStarted === true);
    await page.getByRole("button", { name: "Cancel" }).click();
    await page.evaluate(() => {
        window.__clipasmDelayDigest = false;
    });
    await status.getByText("Rendering cancelled.", { exact: true }).waitFor();
    await page.waitForTimeout(1_000);
    if ((await status.textContent()) !== "Rendering cancelled.") {
        throw new Error("Cancelled asset hashing resumed the render.");
    }

    await render.click();
    await status.getByText("Rendered 108 frames at 320×180.", { exact: true }).waitFor({
        timeout: 5 * 60 * 1000,
    });

    const media = await page.locator(".clipasm-playground__preview video").evaluate(
        (video) =>
            new Promise((resolveMedia, reject) => {
                let timeout;
                const inspect = () => {
                    if (video.readyState >= HTMLMediaElement.HAVE_METADATA) {
                        window.clearTimeout(timeout);
                        resolveMedia({
                            duration: video.duration,
                            readyState: video.readyState,
                        });
                    }
                };
                timeout = window.setTimeout(
                    () => reject(new Error("The rendered MP4 did not load metadata.")),
                    30_000,
                );
                video.addEventListener(
                    "loadedmetadata",
                    () => {
                        window.clearTimeout(timeout);
                        inspect();
                    },
                    { once: true },
                );
                inspect();
            }),
    );
    if (Math.abs(media.duration - 4.5) > 0.01) {
        throw new Error(`Expected a 4.5-second MP4, found ${String(media.duration)} seconds.`);
    }

    await render.click();
    await status.getByText(/Rendering operation/).waitFor({ timeout: 30_000 });
    await page.getByRole("button", { name: "Cancel" }).click();
    await status.getByText("Rendering cancelled.", { exact: true }).waitFor();
    if (!(await render.isEnabled())) {
        throw new Error("Render did not become available after cancellation.");
    }
    if (await page.locator(".clipasm-playground__preview").isVisible()) {
        throw new Error("Cancellation retained a stale preview.");
    }
    if (pageErrors.length > 0) {
        throw new Error(`Browser console errors:\n${pageErrors.join("\n")}`);
    }
} finally {
    await browser?.close();
    await new Promise((resolveClose) => server.close(resolveClose));
}

async function serveBook(request, response) {
    try {
        const url = new URL(request.url ?? "/", "http://127.0.0.1");
        const relative = decodeURIComponent(url.pathname).replace(/^\/+/, "");
        let path = resolve(bookRoot, relative);
        if (path !== bookRoot && !path.startsWith(`${bookRoot}${sep}`)) {
            respond(response, 403, "Forbidden");
            return;
        }
        let metadata = await stat(path);
        if (metadata.isDirectory()) {
            path = resolve(path, "index.html");
            metadata = await stat(path);
        }
        if (!metadata.isFile()) {
            respond(response, 404, "Not found");
            return;
        }
        response.writeHead(200, {
            "Content-Length": metadata.size,
            "Content-Type": MIME_TYPES.get(extname(path)) ?? "application/octet-stream",
        });
        if (request.method === "HEAD") {
            response.end();
        } else {
            createReadStream(path).pipe(response);
        }
    } catch {
        respond(response, 404, "Not found");
    }
}

function respond(response, status, message) {
    response.writeHead(status, { "Content-Type": "text/plain; charset=utf-8" });
    response.end(message);
}
