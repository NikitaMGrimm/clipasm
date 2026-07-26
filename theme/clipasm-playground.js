(async () => {
    "use strict";

    const RESPONSE_VERSION = 2;
    const MAX_SOURCE_BYTES = 256 * 1024;
    const MAX_ASSET_BYTES = 128 * 1024 * 1024;
    const MAX_TOTAL_ASSET_BYTES = 256 * 1024 * 1024;
    const COMPILE_TIMEOUT_MS = 5_000;
    const RENDER_TIMEOUT_MS = 15 * 60 * 1000;
    const encoder = new TextEncoder();
    const workerUrl = new URL(
        `${path_to_root}playground/clipasm-playground-worker.js`,
        document.baseURI,
    );
    const wasmUrl = new URL(
        `${path_to_root}playground/clipasm_playground_bg.wasm`,
        document.baseURI,
    );
    const renderWorkerUrl = new URL(
        `${path_to_root}playground/clipasm-playground-render-worker.js`,
        document.baseURI,
    );
    const ffmpegWrapperUrl = new URL(
        `${path_to_root}playground/ffmpeg/wrapper/index.js`,
        document.baseURI,
    );
    const ffmpegCoreUrl = new URL(
        `${path_to_root}playground/ffmpeg/core/ffmpeg-core.js`,
        document.baseURI,
    );
    const ffmpegWasmUrl = new URL(
        `${path_to_root}playground/ffmpeg/core/ffmpeg-core.wasm`,
        document.baseURI,
    );

    if (window.location.pathname.endsWith("/print.html")) {
        return;
    }

    const mounts = [...document.querySelectorAll("[data-clipasm-playground]")];
    if (mounts.length === 0) {
        return;
    }
    if (!(await assetsAvailable([workerUrl, wasmUrl]))) {
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
        const bundledAssetBase = mount.dataset.clipasmAssetsBase
            ? new URL(mount.dataset.clipasmAssetsBase, document.baseURI)
            : undefined;
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
        const renderButton = button("Render video", "render");
        const cancelButton = button("Cancel");
        const resetButton = button("Reset");
        cancelButton.hidden = true;
        actions.append(
            validateButton,
            inspectButton,
            renderButton,
            cancelButton,
            resetButton,
        );

        const assets = document.createElement("details");
        assets.className = "clipasm-playground__assets";
        const assetsSummary = document.createElement("summary");
        assetsSummary.textContent = "Virtual project files";
        const assetHelp = document.createElement("p");
        assetHelp.textContent =
            "Add files by name, or add a folder to preserve paths such as assets/morning.png.";
        const fileInput = document.createElement("input");
        fileInput.type = "file";
        fileInput.multiple = true;
        fileInput.setAttribute("aria-label", "Add project files");
        const folderInput = document.createElement("input");
        folderInput.type = "file";
        folderInput.multiple = true;
        folderInput.setAttribute("webkitdirectory", "");
        folderInput.setAttribute("aria-label", "Add project folder");
        const selectedFiles = document.createElement("ul");
        selectedFiles.className = "clipasm-playground__file-list";
        const fileControls = document.createElement("div");
        fileControls.className = "clipasm-playground__file-controls";
        fileControls.append(labeledInput("Add files", fileInput), labeledInput("Add folder", folderInput));
        assets.append(assetsSummary, assetHelp, fileControls, selectedFiles);

        const status = document.createElement("p");
        status.className = "clipasm-playground__status";
        status.setAttribute("role", "status");
        status.textContent = "Ready. Compilation and rendering run locally in your browser.";

        const output = document.createElement("pre");
        output.className = "clipasm-playground__output";
        output.setAttribute("aria-live", "polite");
        output.hidden = true;

        const preview = document.createElement("section");
        preview.className = "clipasm-playground__preview";
        preview.hidden = true;
        const video = document.createElement("video");
        video.controls = true;
        video.preload = "metadata";
        const download = document.createElement("a");
        download.textContent = "Download MP4";
        download.download = "clipasm-preview.mp4";
        preview.append(video, download);

        playground.append(editor, actions, assets, status, output, preview);
        sourceBlock.replaceWith(playground);
        mount.remove();

        let compilerWorker;
        let activeCompile;
        let renderWorker;
        let activeRender;
        let nextRequestId = 1;
        let compileCache;
        let activeToken;
        let previewUrl;
        const uploadedFiles = new Map();

        validateButton.addEventListener("click", () => runCompile("validate"));
        inspectButton.addEventListener("click", () => runCompile("inspect"));
        renderButton.addEventListener("click", runRender);
        cancelButton.addEventListener("click", () => {
            cancelWork();
            setStatus("Rendering cancelled.");
        });
        resetButton.addEventListener("click", () => {
            cancelWork();
            editor.value = initialSource;
            compileCache = undefined;
            uploadedFiles.clear();
            renderFileList();
            clearOutput();
            clearPreview();
            setStatus("Source and virtual files reset.");
            editor.focus();
        });
        editor.addEventListener("input", () => {
            cancelWork();
            compileCache = undefined;
            clearPreview();
            setStatus("Edited. Validate or render when ready.");
        });
        editor.addEventListener("keydown", (event) => {
            if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
                event.preventDefault();
                runCompile("validate");
            }
        });
        fileInput.addEventListener("change", () => addUploads(fileInput.files));
        folderInput.addEventListener("change", () => addUploads(folderInput.files));
        window.addEventListener("pagehide", () => {
            cancelWork();
            clearPreview();
        });

        async function runCompile(mode) {
            if (activeToken) {
                return;
            }
            const token = beginWork();
            try {
                const response = await compile(editor.value);
                if (!isActive(token)) {
                    return;
                }
                showResponse(response, mode);
            } catch (error) {
                showUnhandled(error);
            } finally {
                finishWork(token);
            }
        }

        async function runRender() {
            if (activeToken) {
                return;
            }
            const token = beginWork();
            clearOutput();
            clearPreview();
            try {
                validateSourceSize(editor.value);
                setStatus("Preparing virtual project…");
                const compiled = await compile(editor.value);
                if (!isActive(token)) {
                    return;
                }
                requireResponseVersion(compiled);
                if (compiled.status === "error") {
                    showDiagnostic(compiled.diagnostic);
                    return;
                }
                if (compiled.render.status !== "ready") {
                    showDiagnostic(compiled.render.diagnostic);
                    return;
                }
                const resolvedFiles = await resolveAssets(compiled.render.assets);
                if (!isActive(token)) {
                    return;
                }
                const facts = await hashAssets(resolvedFiles);
                if (!isActive(token)) {
                    return;
                }
                setStatus("Building exact browser render recipes…");
                const prepared = await compilerRequest("prepare_render", editor.value, facts);
                if (!isActive(token)) {
                    return;
                }
                requireResponseVersion(prepared);
                if (prepared.status === "error") {
                    showDiagnostic(prepared.diagnostic);
                    return;
                }
                const runtimeAvailable = await assetsAvailable([
                    renderWorkerUrl,
                    ffmpegWrapperUrl,
                    ffmpegCoreUrl,
                    ffmpegWasmUrl,
                ]);
                if (!isActive(token)) {
                    return;
                }
                if (!runtimeAvailable) {
                    throw new Error(
                        "The browser FFmpeg runtime is unavailable in this documentation build.",
                    );
                }
                const plan = JSON.parse(prepared.plan_json);
                const buffer = await render(plan, resolvedFiles, token);
                if (!isActive(token)) {
                    return;
                }
                showPreview(buffer, plan);
            } catch (error) {
                showUnhandled(error);
            } finally {
                finishWork(token);
            }
        }

        async function compile(source) {
            validateSourceSize(source);
            if (compileCache?.source === source) {
                return compileCache.response;
            }
            setStatus("Compiling…");
            const response = await compilerRequest("compile", source);
            requireResponseVersion(response);
            compileCache = { source, response };
            return response;
        }

        function compilerRequest(operation, source, facts) {
            stopCompilerRequest();
            compilerWorker ??= new Worker(workerUrl, { type: "module" });
            const id = nextRequestId++;
            return new Promise((resolve, reject) => {
                const timeout = window.setTimeout(() => {
                    if (activeCompile?.id !== id) {
                        return;
                    }
                    finishCompilerRequest();
                    reject(new Error("Compilation took longer than 5 seconds and was stopped."));
                    stopCompilerWorker();
                }, COMPILE_TIMEOUT_MS);
                activeCompile = { id, resolve, reject, timeout };
                compilerWorker.addEventListener("message", receiveCompile);
                compilerWorker.addEventListener("error", failCompile);
                compilerWorker.postMessage({ id, operation, source, assets: facts });
            });
        }

        function receiveCompile({ data }) {
            if (activeCompile?.id !== data.id) {
                return;
            }
            const { resolve, reject } = activeCompile;
            finishCompilerRequest();
            data.error ? reject(new Error(data.error)) : resolve(data.response);
        }

        function failCompile() {
            if (!activeCompile) {
                return;
            }
            const { reject } = activeCompile;
            finishCompilerRequest();
            reject(new Error("The browser compiler could not start."));
            stopCompilerWorker();
        }

        function render(plan, files, token) {
            stopRenderRequest();
            renderWorker ??= new Worker(renderWorkerUrl, { type: "module" });
            const id = nextRequestId++;
            return new Promise((resolve, reject) => {
                const timeout = window.setTimeout(() => {
                    if (activeRender?.id !== id) {
                        return;
                    }
                    finishRenderRequest();
                    reject(new Error("Browser rendering exceeded the 15-minute safety limit."));
                    stopRenderWorker();
                }, RENDER_TIMEOUT_MS);
                activeRender = { id, token, resolve, reject, timeout };
                renderWorker.addEventListener("message", receiveRender);
                renderWorker.addEventListener("error", failRender);
                renderWorker.postMessage({
                    id,
                    plan,
                    files: files.map(({ path, file }) => ({ path, file })),
                });
            });
        }

        function receiveRender({ data }) {
            if (activeRender?.id !== data.id) {
                return;
            }
            if (data.status === "progress") {
                setStatus(data.message);
                return;
            }
            const { resolve, reject } = activeRender;
            finishRenderRequest();
            if (data.status === "success") {
                resolve(data.buffer);
            } else {
                reject(new Error(data.error || "Browser rendering failed."));
                stopRenderWorker();
            }
        }

        function failRender() {
            if (!activeRender) {
                return;
            }
            const { reject } = activeRender;
            finishRenderRequest();
            reject(new Error("The browser FFmpeg worker could not start."));
            stopRenderWorker();
        }

        async function resolveAssets(requests) {
            const files = [];
            const missing = [];
            for (const request of requests) {
                const uploaded = uploadedFiles.get(request.path);
                if (uploaded) {
                    files.push({ path: request.path, file: uploaded });
                    continue;
                }
                const bundled = await bundledAsset(request.path);
                if (bundled) {
                    files.push({ path: request.path, file: bundled });
                } else {
                    missing.push(request.path);
                }
            }
            if (missing.length > 0) {
                assets.open = true;
                throw new Error(
                    `Add the following virtual project file${missing.length === 1 ? "" : "s"}: ${missing.join(", ")}`,
                );
            }
            return files;
        }

        async function bundledAsset(path) {
            if (!bundledAssetBase) {
                return undefined;
            }
            const relative = path.split("/").map(encodeURIComponent).join("/");
            const response = await fetch(new URL(relative, bundledAssetBase), { cache: "force-cache" });
            return response.ok ? response.blob() : undefined;
        }

        async function hashAssets(files) {
            let total = 0;
            const facts = [];
            for (const { path, file } of files) {
                if (file.size > MAX_ASSET_BYTES) {
                    throw new Error(`Virtual file \`${path}\` exceeds the 128 MiB browser limit.`);
                }
                total += file.size;
                if (total > MAX_TOTAL_ASSET_BYTES) {
                    throw new Error("Virtual project files exceed the 256 MiB browser limit.");
                }
                const digest = await crypto.subtle.digest("SHA-256", await file.arrayBuffer());
                facts.push({ path, content_hash: hex(digest) });
            }
            return facts;
        }

        function addUploads(list) {
            let error;
            for (const file of list || []) {
                try {
                    uploadedFiles.set(uploadPath(file), file);
                } catch (caught) {
                    error = caught;
                }
            }
            fileInput.value = "";
            folderInput.value = "";
            cancelWork();
            clearPreview();
            renderFileList();
            if (error) {
                showUnhandled(error);
            } else {
                setStatus(`${uploadedFiles.size} virtual project file(s) ready.`);
            }
        }

        function uploadPath(file) {
            const candidate = (file.webkitRelativePath || file.name).replaceAll("\\", "/");
            const parts = candidate.split("/");
            if (
                candidate.startsWith("/") ||
                parts.length === 0 ||
                parts.some((part) => part === "" || part === "." || part === "..")
            ) {
                throw new Error(`Uploaded path \`${candidate}\` is not a safe virtual path.`);
            }
            return parts.join("/");
        }

        function renderFileList() {
            selectedFiles.replaceChildren(
                ...[...uploadedFiles.keys()].sort().map((path) => {
                    const item = document.createElement("li");
                    item.textContent = path;
                    return item;
                }),
            );
            selectedFiles.hidden = uploadedFiles.size === 0;
        }

        function showResponse(response, mode) {
            requireResponseVersion(response);
            if (response.status === "error") {
                showDiagnostic(response.diagnostic);
                return;
            }
            if (response.status !== "success") {
                throw new Error("The playground returned an unknown response.");
            }
            setStatus(summary(response), "success");
            if (mode === "inspect") {
                output.textContent = JSON.stringify(JSON.parse(response.compiled_json), null, 2);
                output.hidden = false;
            } else {
                clearOutput();
            }
        }

        function showDiagnostic(diagnostic) {
            setStatus(
                `${diagnostic.code} at ${diagnostic.line}:${diagnostic.column}: ${diagnostic.message}`,
                "error",
            );
            output.textContent = diagnostic.rendered;
            output.hidden = false;
        }

        function showUnhandled(error) {
            if (error?.name === "AbortError") {
                return;
            }
            setStatus(error instanceof Error ? error.message : String(error), "error");
        }

        function showPreview(buffer, plan) {
            clearPreview();
            previewUrl = URL.createObjectURL(new Blob([buffer], { type: "video/mp4" }));
            video.src = previewUrl;
            download.href = previewUrl;
            preview.hidden = false;
            const contract = plan.export.contract;
            setStatus(
                `Rendered ${contract.frames} frames at ${contract.width}×${contract.height}.`,
                "success",
            );
        }

        function clearPreview() {
            video.removeAttribute("src");
            video.load();
            download.removeAttribute("href");
            preview.hidden = true;
            if (previewUrl) {
                URL.revokeObjectURL(previewUrl);
                previewUrl = undefined;
            }
        }

        function clearOutput() {
            output.hidden = true;
            output.textContent = "";
        }

        function beginWork() {
            const token = Symbol("playground operation");
            activeToken = token;
            setBusy(true);
            return token;
        }

        function finishWork(token) {
            if (activeToken !== token) {
                return;
            }
            activeToken = undefined;
            setBusy(false);
        }

        function cancelWork() {
            activeToken = undefined;
            stopCompilerRequest();
            stopCompilerWorker();
            stopRenderRequest();
            stopRenderWorker();
            setBusy(false);
        }

        function stopCompilerRequest() {
            if (!activeCompile) {
                return;
            }
            const { reject } = activeCompile;
            finishCompilerRequest();
            reject(new DOMException("Compilation was cancelled.", "AbortError"));
        }

        function finishCompilerRequest() {
            window.clearTimeout(activeCompile.timeout);
            compilerWorker?.removeEventListener("message", receiveCompile);
            compilerWorker?.removeEventListener("error", failCompile);
            activeCompile = undefined;
        }

        function stopCompilerWorker() {
            compilerWorker?.terminate();
            compilerWorker = undefined;
        }

        function stopRenderRequest() {
            if (!activeRender) {
                return;
            }
            const { reject } = activeRender;
            finishRenderRequest();
            reject(new DOMException("Rendering was cancelled.", "AbortError"));
        }

        function finishRenderRequest() {
            window.clearTimeout(activeRender.timeout);
            renderWorker?.removeEventListener("message", receiveRender);
            renderWorker?.removeEventListener("error", failRender);
            activeRender = undefined;
        }

        function stopRenderWorker() {
            renderWorker?.terminate();
            renderWorker = undefined;
        }

        function setBusy(busy) {
            validateButton.disabled = busy;
            inspectButton.disabled = busy;
            renderButton.disabled = busy;
            fileInput.disabled = busy;
            folderInput.disabled = busy;
            cancelButton.hidden = !busy;
        }

        function setStatus(message, kind) {
            status.className = `clipasm-playground__status${kind ? ` clipasm-playground__status--${kind}` : ""}`;
            status.textContent = message;
        }

        function isActive(token) {
            return activeToken === token;
        }
    }

    function button(label, kind) {
        const element = document.createElement("button");
        element.type = "button";
        element.className = `clipasm-playground__button${kind ? ` clipasm-playground__button--${kind}` : ""}`;
        element.textContent = label;
        return element;
    }

    function labeledInput(label, input) {
        const wrapper = document.createElement("label");
        wrapper.className = "clipasm-playground__file-button";
        wrapper.append(label, input);
        return wrapper;
    }

    function validateSourceSize(source) {
        if (encoder.encode(source).byteLength > MAX_SOURCE_BYTES) {
            throw new Error("Source is larger than the 256 KiB playground limit.");
        }
    }

    function requireResponseVersion(response) {
        if (response.version !== RESPONSE_VERSION) {
            throw new Error("The playground returned an unsupported response.");
        }
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

    function hex(buffer) {
        return [...new Uint8Array(buffer)]
            .map((byte) => byte.toString(16).padStart(2, "0"))
            .join("");
    }

    async function assetsAvailable(urls) {
        try {
            const responses = await Promise.all(
                urls.map((url) => fetch(url, { method: "HEAD", cache: "no-store" })),
            );
            return responses.every((response) => response.ok);
        } catch {
            return false;
        }
    }
})();
