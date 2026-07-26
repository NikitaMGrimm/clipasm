(async () => {
    "use strict";

    const RESPONSE_VERSION = 1;
    const MAX_SOURCE_BYTES = 256 * 1024;
    const TIMEOUT_MS = 5_000;
    const workerUrl = new URL(
        `${path_to_root}playground/clipasm-playground-worker.js`,
        document.baseURI,
    );
    const wasmUrl = new URL(
        `${path_to_root}playground/clipasm_playground_bg.wasm`,
        document.baseURI,
    );
    const encoder = new TextEncoder();

    if (window.location.pathname.endsWith("/print.html")) {
        return;
    }

    const mounts = [...document.querySelectorAll("[data-clipasm-playground]")];
    if (mounts.length === 0) {
        return;
    }
    if (!(await assetsAvailable())) {
        for (const mount of mounts) {
            mount.textContent =
                "The interactive compiler is unavailable in this documentation build.";
        }
        return;
    }

    for (const mount of mounts) {
        const sourceBlock = mount.previousElementSibling;
        const sourceCode = sourceBlock?.querySelector("code.language-clipasm");
        if (!sourceCode) {
            console.error("A ClipAsm playground must follow a ClipAsm code block.");
            continue;
        }

        enhance(mount, sourceBlock, sourceCode.textContent);
    }

    function enhance(mount, sourceBlock, initialSource) {
        const playground = document.createElement("section");
        playground.className = "clipasm-playground";
        playground.setAttribute("aria-label", "ClipAsm playground");

        const editor = document.createElement("textarea");
        editor.className = "clipasm-playground__editor";
        editor.setAttribute("aria-label", "ClipAsm source");
        editor.setAttribute("spellcheck", "false");
        editor.value = initialSource;

        const actions = document.createElement("div");
        actions.className = "clipasm-playground__actions";
        const validateButton = button("Validate", "primary");
        const inspectButton = button("Inspect");
        const resetButton = button("Reset");
        actions.append(validateButton, inspectButton, resetButton);

        const status = document.createElement("p");
        status.className = "clipasm-playground__status";
        status.setAttribute("role", "status");
        status.textContent = "Ready. Validation runs locally in your browser.";

        const output = document.createElement("pre");
        output.className = "clipasm-playground__output";
        output.setAttribute("aria-live", "polite");
        output.hidden = true;

        playground.append(editor, actions, status, output);
        sourceBlock.replaceWith(playground);
        mount.remove();

        let worker;
        let activeRequest;
        let nextRequestId = 1;
        let cached;

        validateButton.addEventListener("click", () => run("validate"));
        inspectButton.addEventListener("click", () => run("inspect"));
        resetButton.addEventListener("click", () => {
            cancel();
            editor.value = initialSource;
            cached = undefined;
            output.hidden = true;
            status.className = "clipasm-playground__status";
            status.textContent = "Source reset.";
            editor.focus();
        });
        editor.addEventListener("input", () => {
            cancel();
            cached = undefined;
            status.className = "clipasm-playground__status";
            status.textContent = "Edited. Validate when ready.";
        });
        editor.addEventListener("keydown", (event) => {
            if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
                event.preventDefault();
                run("validate");
            }
        });

        async function run(mode) {
            if (activeRequest) {
                return;
            }

            const source = editor.value;
            const sourceBytes = encoder.encode(source).byteLength;
            if (sourceBytes > MAX_SOURCE_BYTES) {
                showFailure("Source is larger than the 256 KiB playground limit.");
                return;
            }

            setBusy(true);
            status.className = "clipasm-playground__status";
            status.textContent = mode === "inspect" ? "Compiling for inspection…" : "Validating…";

            try {
                const response =
                    cached?.source === source ? cached.response : await compile(source);
                if (response.version !== RESPONSE_VERSION) {
                    throw new Error("The playground returned an unsupported response.");
                }
                cached = { source, response };
                showResponse(response, mode);
            } catch (error) {
                if (error?.name === "AbortError") {
                    return;
                }
                showFailure(error instanceof Error ? error.message : String(error));
            } finally {
                setBusy(false);
            }
        }

        function compile(source) {
            cancel();
            worker ??= new Worker(workerUrl, { type: "module" });
            const id = nextRequestId++;

            return new Promise((resolve, reject) => {
                const timeout = window.setTimeout(() => {
                    if (!activeRequest || activeRequest.id !== id) {
                        return;
                    }
                    finishRequest();
                    reject(new Error("Validation took longer than 5 seconds and was stopped."));
                    stopWorker();
                }, TIMEOUT_MS);

                activeRequest = { id, resolve, reject, timeout };
                worker.addEventListener("message", receive);
                worker.addEventListener("error", fail);
                worker.postMessage({ id, source });
            });
        }

        function receive({ data }) {
            if (!activeRequest || data.id !== activeRequest.id) {
                return;
            }
            const { resolve, reject } = activeRequest;
            finishRequest();
            if (data.error) {
                reject(new Error(data.error));
            } else {
                resolve(data.response);
            }
        }

        function fail() {
            if (!activeRequest) {
                return;
            }
            const { reject } = activeRequest;
            finishRequest();
            reject(new Error("The browser compiler could not start."));
            stopWorker();
        }

        function cancel() {
            if (!activeRequest) {
                return;
            }
            const { reject } = activeRequest;
            finishRequest();
            reject(new DOMException("Validation was cancelled.", "AbortError"));
            stopWorker();
        }

        function finishRequest() {
            window.clearTimeout(activeRequest.timeout);
            worker.removeEventListener("message", receive);
            worker.removeEventListener("error", fail);
            activeRequest = undefined;
        }

        function stopWorker() {
            worker?.terminate();
            worker = undefined;
        }

        function showResponse(response, mode) {
            if (response.status === "error") {
                status.className = "clipasm-playground__status clipasm-playground__status--error";
                status.textContent = `${response.diagnostic.code} at ${response.diagnostic.line}:${response.diagnostic.column}`;
                output.textContent = response.diagnostic.rendered;
                output.hidden = false;
                return;
            }
            if (response.status !== "success") {
                throw new Error("The playground returned an unknown response.");
            }

            status.className = "clipasm-playground__status clipasm-playground__status--success";
            status.textContent = summary(response);
            if (mode === "inspect") {
                output.textContent = JSON.stringify(JSON.parse(response.compiled_json), null, 2);
                output.hidden = false;
            } else {
                output.hidden = true;
            }
        }

        function showFailure(message) {
            status.className = "clipasm-playground__status clipasm-playground__status--error";
            status.textContent = message;
            output.hidden = true;
        }

        function setBusy(busy) {
            validateButton.disabled = busy;
            inspectButton.disabled = busy;
        }
    }

    function button(label, kind) {
        const element = document.createElement("button");
        element.type = "button";
        element.className = `clipasm-playground__button${kind ? ` clipasm-playground__button--${kind}` : ""}`;
        element.textContent = label;
        return element;
    }

    function summary(response) {
        const values = `${response.value_count} semantic value${response.value_count === 1 ? "" : "s"}`;
        const outputs = response.outputs.map(titleCase).join(", ") || "no outputs";
        const frames = response.frames == null ? "" : ` · ${response.frames} frames`;
        return `Valid: ${values} · ${outputs}${frames}`;
    }

    function titleCase(value) {
        return value.charAt(0).toUpperCase() + value.slice(1);
    }

    async function assetsAvailable() {
        try {
            const responses = await Promise.all(
                [workerUrl, wasmUrl].map((url) =>
                    fetch(url, { method: "HEAD", cache: "no-store" }),
                ),
            );
            return responses.every((response) => response.ok);
        } catch {
            return false;
        }
    }
})();
