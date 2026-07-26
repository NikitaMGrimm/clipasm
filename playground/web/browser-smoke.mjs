import { createReadStream } from "node:fs";
import { readFile, realpath, stat } from "node:fs/promises";
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
    const origin = `http://127.0.0.1:${address.port}`;
    await context.grantPermissions(["clipboard-read", "clipboard-write"], { origin });
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

    await page.goto(`${origin}/try-clipasm.html`);
    const playground = page.locator(".clipasm-playground");
    const status = page.locator(".clipasm-playground__status");
    const editor = page.getByRole("textbox", { name: "ClipAsm source" });
    const lineNumbers = page.locator(".clipasm-playground__line-numbers");
    const source = await editor.inputValue();
    const expectedLineCount = source.split("\n").length;
    const displayedLines = (await lineNumbers.textContent())?.split("\n").length;
    if (displayedLines !== expectedLineCount) {
        throw new Error(
            `Expected ${String(expectedLineCount)} editor line numbers, found ${String(displayedLines)}.`,
        );
    }
    const editorHeight = await editor.evaluate((element) => element.getBoundingClientRect().height);
    if (editorHeight < 340) {
        throw new Error(`Expected the editor to be at least 340px tall, found ${editorHeight}px.`);
    }
    await editor.fill(Array.from({ length: 100 }, (_, index) => `# line ${index + 1}`).join("\n"));
    const scrollTop = await editor.evaluate((element) => {
        element.scrollTop = element.scrollHeight;
        element.dispatchEvent(new Event("scroll"));
        return element.scrollTop;
    });
    const gutterScrollTop = await lineNumbers.evaluate((element) => element.scrollTop);
    if (Math.abs(scrollTop - gutterScrollTop) > 1) {
        throw new Error(
            `Editor and line-number scroll positions differ: ${scrollTop} and ${gutterScrollTop}.`,
        );
    }
    await page.getByRole("button", { name: "Reset" }).click();

    await page.locator(".clipasm-playground__assets > summary").click();
    const fileInput = page.getByLabel("Add project files");
    const fileRows = page.locator(".clipasm-playground__file-row");
    if ((await fileRows.count()) !== 3) {
        throw new Error("The three bundled scenic assets did not begin as project files.");
    }
    for (const path of [
        "assets/evening.png",
        "assets/meadow.png",
        "assets/morning.png",
    ]) {
        await fileRows.getByText(path, { exact: true }).waitFor();
    }

    let fileRow = fileRows.filter({ hasText: "assets/morning.png" });
    const playgroundHeight = await playground.evaluate(
        (element) => element.getBoundingClientRect().height,
    );
    await fileRow.locator("summary").click();
    await fileRow.getByRole("button", { name: "Preview" }).click();
    const filePreview = page.getByRole("dialog", { name: "File preview" });
    await filePreview.locator("img").waitFor();
    const previewHeight = await playground.evaluate(
        (element) => element.getBoundingClientRect().height,
    );
    if (Math.abs(playgroundHeight - previewHeight) > 1) {
        throw new Error("Opening a file preview changed the playground layout.");
    }
    await filePreview.getByRole("button", { name: "Close" }).click();

    await fileRow.locator("summary").click();
    await fileRow.getByRole("button", { name: "Rename" }).click();
    const renameDialog = page.getByRole("dialog", { name: "Rename virtual file" });
    await renameDialog.getByLabel("Path").fill("../unsafe.png");
    await renameDialog.getByRole("button", { name: "Rename" }).click();
    await renameDialog.getByRole("alert").getByText(/not a safe relative path/).waitFor();
    await renameDialog.getByLabel("Path").fill("assets/renamed.png");
    await renameDialog.getByRole("button", { name: "Rename" }).click();
    fileRow = fileRows.filter({ hasText: "assets/renamed.png" });
    await fileRow.getByText("assets/renamed.png", { exact: true }).waitFor();
    const fileActions = fileRow.locator("summary");
    if ((await fileActions.textContent())?.trim() !== "Actions") {
        throw new Error("The virtual-file menu trigger does not have a visible label.");
    }
    await fileActions.click();
    const copyPath = fileRow.getByRole("button", { name: "Copy path" });
    const renameAction = fileRow.getByRole("button", { name: "Rename" });
    const [copyPathHeight, renameHeight] = await Promise.all([
        copyPath.evaluate((element) => element.getBoundingClientRect().height),
        renameAction.evaluate((element) => element.getBoundingClientRect().height),
    ]);
    if (Math.abs(copyPathHeight - renameHeight) > 1) {
        throw new Error("Virtual-file menu labels wrap onto multiple lines.");
    }
    await copyPath.click();
    await status
        .getByText("Copied `assets/renamed.png` to the clipboard.", { exact: true })
        .waitFor();
    const clipboardPath = await page.evaluate(() => navigator.clipboard.readText());
    if (clipboardPath !== "assets/renamed.png") {
        throw new Error(`Copy path wrote \`${clipboardPath}\` to the clipboard.`);
    }
    await fileRow.locator("summary").click();
    await fileRow.getByRole("button", { name: "Delete" }).click();
    if ((await fileRows.count()) !== 2) {
        throw new Error("Deleting a bundled project file did not remove it.");
    }
    while ((await fileRows.count()) > 0) {
        const row = fileRows.first();
        await row.locator("summary").click();
        await row.getByRole("button", { name: "Delete" }).click();
    }
    if (await page.locator(".clipasm-playground__file-list").isVisible()) {
        throw new Error("Deleting the final virtual file left the file list visible.");
    }
    await page.getByRole("button", { name: "Reset" }).click();
    if ((await fileRows.count()) !== 3) {
        throw new Error("Reset did not restore the bundled project files.");
    }

    await fileInput.setInputFiles(
        ["a.png", "b.png", "c.png", "d.png", "z.png"].map((name) => ({
            name,
            mimeType: "image/png",
            buffer: Buffer.from(
                "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
                "base64",
            ),
        })),
    );
    const fileMenus = page.locator(".clipasm-playground__file-menu");
    await fileMenus.first().locator("summary").click();
    await fileMenus.last().locator("summary").click();
    await page.waitForFunction(
        () => document.querySelectorAll(".clipasm-playground__file-menu[open]").length === 1,
    );
    await fileMenus.last().evaluate(
        () => new Promise((resolveFrame) => requestAnimationFrame(resolveFrame)),
    );
    if ((await fileMenus.first().getAttribute("open")) !== null) {
        throw new Error("Opening a virtual-file menu left the previous menu open.");
    }
    const [menuBox, playgroundBox] = await Promise.all([
        fileMenus
            .last()
            .locator(".clipasm-playground__file-menu-actions")
            .evaluate((element) => element.getBoundingClientRect().toJSON()),
        playground.evaluate((element) => element.getBoundingClientRect().toJSON()),
    ]);
    if (
        menuBox.top < playgroundBox.top - 1 ||
        menuBox.right > playgroundBox.right + 1 ||
        menuBox.bottom > playgroundBox.bottom + 1 ||
        menuBox.left < playgroundBox.left - 1
    ) {
        throw new Error("The final virtual-file menu is clipped by the playground.");
    }
    await status.click();
    if ((await page.locator(".clipasm-playground__file-menu[open]").count()) !== 0) {
        throw new Error("Clicking outside a virtual-file menu did not close it.");
    }
    await page.getByRole("button", { name: "Reset" }).click();

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

    const previewVideo = page.locator(".clipasm-playground__preview video");
    const media = await readVideoMetadata(previewVideo);
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

    await editor.fill(
        'clipasm 1\n\nconfig {\n    video {\n        width = 320\n        height = 180\n        fps = 24\n    }\n}\n\nvideo("gentle-motion.mkv")\n',
    );
    await fileInput.setInputFiles({
        name: "gentle-motion.mkv",
        mimeType: "video/x-matroska",
        buffer: await readFile(resolve(bookRoot, "../../examples/assets/gentle-motion.mkv")),
    });
    await render.click();
    await status.getByText("Rendered 48 frames at 320×180.", { exact: true }).waitFor({
        timeout: 5 * 60 * 1000,
    });
    const videoSourceMedia = await readVideoMetadata(previewVideo);
    if (Math.abs(videoSourceMedia.duration - 2) > 0.01) {
        throw new Error(
            `Expected a 2-second video-source render, found ${String(videoSourceMedia.duration)} seconds.`,
        );
    }

    if (pageErrors.length > 0) {
        throw new Error(`Browser console errors:\n${pageErrors.join("\n")}`);
    }
} finally {
    await browser?.close();
    await new Promise((resolveClose) => server.close(resolveClose));
}

function readVideoMetadata(video) {
    return video.evaluate(
        (element) =>
            new Promise((resolveMedia, reject) => {
                let timeout;
                const inspect = () => {
                    if (element.readyState >= HTMLMediaElement.HAVE_METADATA) {
                        window.clearTimeout(timeout);
                        resolveMedia({
                            duration: element.duration,
                            readyState: element.readyState,
                        });
                    }
                };
                timeout = window.setTimeout(
                    () => reject(new Error("The rendered MP4 did not load metadata.")),
                    30_000,
                );
                element.addEventListener(
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
