import init, { compileSource, prepareRender } from "./clipasm_playground.js";

const MAX_SOURCE_BYTES = 256 * 1024;
const encoder = new TextEncoder();
const initialized = init();

self.addEventListener("message", async ({ data }) => {
    const { id, operation, source, assets } = data;

    try {
        if (typeof source !== "string") {
            throw new TypeError("playground source must be text");
        }
        if (encoder.encode(source).byteLength > MAX_SOURCE_BYTES) {
            throw new Error("source is larger than the 256 KiB playground limit");
        }

        await initialized;
        const response =
            operation === "prepare_render"
                ? JSON.parse(prepareRender(source, JSON.stringify(assets)))
                : JSON.parse(compileSource(source));
        self.postMessage({ id, response });
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        self.postMessage({ id, error: message });
    }
});
