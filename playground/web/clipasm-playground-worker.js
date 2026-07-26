import init, { compileSource } from "./clipasm_playground.js";

const MAX_SOURCE_BYTES = 256 * 1024;
const encoder = new TextEncoder();
const initialized = init();

self.addEventListener("message", async ({ data }) => {
    const { id, source } = data;

    try {
        if (typeof source !== "string") {
            throw new TypeError("playground source must be text");
        }
        if (encoder.encode(source).byteLength > MAX_SOURCE_BYTES) {
            throw new Error("source is larger than the 256 KiB playground limit");
        }

        await initialized;
        const response = JSON.parse(compileSource(source));
        self.postMessage({ id, response });
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        self.postMessage({ id, error: message });
    }
});
