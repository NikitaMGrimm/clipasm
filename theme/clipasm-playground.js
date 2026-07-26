(async () => {
    "use strict";

    const RESPONSE_VERSION = 4;
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
        try {
            await enhance(mount, sourceBlock, sourceCode.textContent);
        } catch (error) {
            console.error(error);
            mount.textContent = "The interactive project files could not be loaded.";
        }
    }

    async function enhance(mount, sourceBlock, initialSource) {
        const bundledAssetBase = mount.dataset.clipasmAssetsBase
            ? new URL(mount.dataset.clipasmAssetsBase, document.baseURI)
            : undefined;
        const initialProjectFiles = await loadBundledAssets(
            bundledAssetBase,
            mount.dataset.clipasmAssets,
        );
        const playground = document.createElement("section");
        playground.className = "clipasm-playground";
        playground.setAttribute("aria-label", "ClipAsm playground");

        const editorFrame = document.createElement("div");
        editorFrame.className = "clipasm-playground__editor-frame";
        const lineNumbers = document.createElement("pre");
        lineNumbers.className = "clipasm-playground__line-numbers";
        lineNumbers.setAttribute("aria-hidden", "true");
        const editor = document.createElement("textarea");
        editor.className = "clipasm-playground__editor";
        editor.setAttribute("aria-label", "ClipAsm source");
        editor.setAttribute("spellcheck", "false");
        editor.setAttribute("wrap", "off");
        editor.value = initialSource;
        editorFrame.append(lineNumbers, editor);

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
            "Add files by name, or add a folder to preserve paths such as uploaded_folder/folder_image.png.";
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
        selectedFiles.hidden = true;
        const fileControls = document.createElement("div");
        fileControls.className = "clipasm-playground__file-controls";
        fileControls.append(
            labeledInput("Add files", fileInput),
            labeledInput("Add folder", folderInput),
        );
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

        const renameDialog = document.createElement("dialog");
        renameDialog.className = "clipasm-playground__dialog";
        renameDialog.setAttribute("aria-label", "Rename virtual file");
        const renameForm = document.createElement("form");
        renameForm.method = "dialog";
        const renameTitle = document.createElement("h3");
        renameTitle.textContent = "Rename virtual file";
        const renameLabel = document.createElement("label");
        renameLabel.textContent = "Path";
        const renameInput = document.createElement("input");
        renameInput.type = "text";
        renameInput.required = true;
        renameInput.setAttribute("autocomplete", "off");
        renameLabel.append(renameInput);
        const renameError = document.createElement("p");
        renameError.className = "clipasm-playground__dialog-error";
        renameError.setAttribute("role", "alert");
        renameError.hidden = true;
        const renameActions = document.createElement("div");
        renameActions.className = "clipasm-playground__dialog-actions";
        const renameCancel = button("Cancel");
        const renameSubmit = button("Rename", "primary");
        renameSubmit.type = "submit";
        renameActions.append(renameCancel, renameSubmit);
        renameForm.append(renameTitle, renameLabel, renameError, renameActions);
        renameDialog.append(renameForm);

        const filePreviewDialog = document.createElement("dialog");
        filePreviewDialog.className =
            "clipasm-playground__dialog clipasm-playground__file-preview";
        filePreviewDialog.setAttribute("aria-label", "File preview");
        const filePreviewHeader = document.createElement("div");
        filePreviewHeader.className = "clipasm-playground__file-preview-header";
        const filePreviewTitle = document.createElement("h3");
        const filePreviewClose = button("Close");
        filePreviewHeader.append(filePreviewTitle, filePreviewClose);
        const filePreviewContent = document.createElement("div");
        filePreviewContent.className = "clipasm-playground__file-preview-content";
        filePreviewDialog.append(filePreviewHeader, filePreviewContent);

        playground.append(
            editorFrame,
            actions,
            assets,
            status,
            output,
            preview,
            renameDialog,
            filePreviewDialog,
        );
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
        let filePreviewUrl;
        let renamePath;
        let displayedLineCount;
        const projectFiles = new Map(initialProjectFiles);

        updateLineNumbers();
        renderFileList();
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
            editor.scrollTop = 0;
            updateLineNumbers();
            syncLineNumbers();
            compileCache = undefined;
            projectFiles.clear();
            for (const [path, file] of initialProjectFiles) {
                projectFiles.set(path, file);
            }
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
            updateLineNumbers();
            setStatus("Edited. Validate or render when ready.");
        });
        editor.addEventListener("scroll", syncLineNumbers);
        editor.addEventListener("keydown", (event) => {
            if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
                event.preventDefault();
                runCompile("validate");
            }
        });
        fileInput.addEventListener("change", () => addUploads(fileInput.files));
        folderInput.addEventListener("change", () => addUploads(folderInput.files));
        assets.addEventListener("toggle", () => {
            if (!assets.open) {
                closeFileMenus();
            }
        });
        renameCancel.addEventListener("click", () => renameDialog.close());
        renameForm.addEventListener("submit", renameFile);
        filePreviewClose.addEventListener("click", () => filePreviewDialog.close());
        filePreviewDialog.addEventListener("close", clearFilePreview);
        document.addEventListener("click", closeFileMenusOnOutsideClick);
        document.addEventListener("keydown", closeFileMenusOnEscape);
        window.addEventListener("pagehide", () => {
            document.removeEventListener("click", closeFileMenusOnOutsideClick);
            document.removeEventListener("keydown", closeFileMenusOnEscape);
            cancelWork();
            clearPreview();
            clearFilePreview();
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
                const videoRequests = compiled.render.assets.filter(
                    (request) => request.kind === "video",
                );
                let runtimeAvailable;
                if (videoRequests.length > 0) {
                    runtimeAvailable = await assetsAvailable([
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
                    const probes = await probeVideos(videoRequests, resolvedFiles, token);
                    if (!isActive(token)) {
                        return;
                    }
                    const probesByPath = new Map(
                        probes.map((probe) => [probe.path, probe.video_probe]),
                    );
                    for (const fact of facts) {
                        const videoProbe = probesByPath.get(fact.path);
                        if (videoProbe) {
                            fact.video_probe = videoProbe;
                        }
                    }
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
                runtimeAvailable ??= await assetsAvailable([
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
            return renderWorkerRequest(
                {
                    operation: "render",
                    plan,
                    files: files.map(({ path, file }) => ({ path, file })),
                },
                token,
                "Browser rendering exceeded the 15-minute safety limit.",
            ).then((response) => response.buffer);
        }

        function probeVideos(requests, files, token) {
            return renderWorkerRequest(
                {
                    operation: "probe",
                    requests,
                    files: files.map(({ path, file }) => ({ path, file })),
                },
                token,
                "Browser video inspection exceeded the 15-minute safety limit.",
            ).then((response) => response.probes);
        }

        function renderWorkerRequest(message, token, timeoutMessage) {
            stopRenderRequest();
            renderWorker ??= new Worker(renderWorkerUrl, { type: "module" });
            const id = nextRequestId++;
            return new Promise((resolve, reject) => {
                const timeout = window.setTimeout(() => {
                    if (activeRender?.id !== id) {
                        return;
                    }
                    finishRenderRequest();
                    reject(new Error(timeoutMessage));
                    stopRenderWorker();
                }, RENDER_TIMEOUT_MS);
                activeRender = { id, token, resolve, reject, timeout };
                renderWorker.addEventListener("message", receiveRender);
                renderWorker.addEventListener("error", failRender);
                renderWorker.postMessage({ id, ...message });
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
            if (data.status === "success" || data.status === "probed") {
                resolve(data);
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
                const file = projectFiles.get(request.path);
                if (file) {
                    files.push({ path: request.path, file });
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
                    projectFiles.set(uploadPath(file), file);
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
                setStatus(`${projectFiles.size} virtual project file(s) ready.`);
            }
        }

        function uploadPath(file) {
            return normalizeVirtualPath(file.webkitRelativePath || file.name, "Uploaded path");
        }

        function renderFileList() {
            selectedFiles.replaceChildren(
                ...[...projectFiles.entries()]
                    .sort(([left], [right]) => left.localeCompare(right))
                    .map(([path, file]) => {
                    const item = document.createElement("li");
                    item.className = "clipasm-playground__file-row";
                    const filePath = document.createElement("code");
                    filePath.className = "clipasm-playground__file-path";
                    filePath.textContent = path;
                    filePath.title = path;
                    const menu = document.createElement("details");
                    menu.className = "clipasm-playground__file-menu";
                    const menuSummary = document.createElement("summary");
                    menuSummary.textContent = "Actions";
                    menuSummary.setAttribute("aria-label", `File actions for ${path}`);
                    menuSummary.addEventListener("click", () => {
                        if (!menu.open) {
                            closeFileMenus(menu);
                        }
                    });
                    const menuActions = document.createElement("div");
                    menuActions.className = "clipasm-playground__file-menu-actions";
                    if (previewKind(file)) {
                        menuActions.append(
                            fileAction("Preview", () => {
                                menu.open = false;
                                showFilePreview(path, file);
                            }),
                        );
                    }
                    menuActions.append(
                        fileAction("Copy path", async () => {
                            menu.open = false;
                            try {
                                await copyText(path);
                                setStatus(`Copied \`${path}\` to the clipboard.`, "success");
                            } catch (error) {
                                showUnhandled(error);
                            }
                        }),
                        fileAction("Rename", () => {
                            menu.open = false;
                            openRenameDialog(path);
                        }),
                        fileAction("Delete", () => {
                            menu.open = false;
                            projectFiles.delete(path);
                            filesChanged(`Removed \`${path}\`.`);
                        }, "danger"),
                    );
                    menu.append(menuSummary, menuActions);
                    menu.addEventListener("toggle", () => {
                        if (!menu.open) {
                            menu.classList.remove("clipasm-playground__file-menu--above");
                            return;
                        }
                        positionFileMenu(menu, menuActions);
                    });
                    item.append(filePath, menu);
                    return item;
                }),
            );
            selectedFiles.hidden = projectFiles.size === 0;
            assetsSummary.textContent =
                projectFiles.size === 0
                    ? "Virtual project files"
                    : `Virtual project files (${projectFiles.size})`;
        }

        function closeFileMenus(except) {
            for (const menu of selectedFiles.querySelectorAll(
                ".clipasm-playground__file-menu[open]",
            )) {
                if (menu !== except) {
                    menu.open = false;
                }
            }
        }

        function closeFileMenusOnOutsideClick(event) {
            const menu =
                event.target instanceof Element
                    ? event.target.closest(".clipasm-playground__file-menu")
                    : undefined;
            if (!menu || !playground.contains(menu)) {
                closeFileMenus();
            }
        }

        function closeFileMenusOnEscape(event) {
            if (event.key === "Escape") {
                closeFileMenus();
            }
        }

        function positionFileMenu(menu, menuActions) {
            menu.classList.remove("clipasm-playground__file-menu--above");
            const playgroundBounds = playground.getBoundingClientRect();
            const menuBounds = menu.getBoundingClientRect();
            const spaceAbove = menuBounds.top - playgroundBounds.top;
            const spaceBelow = playgroundBounds.bottom - menuBounds.bottom;
            if (menuActions.offsetHeight > spaceBelow && spaceAbove > spaceBelow) {
                menu.classList.add("clipasm-playground__file-menu--above");
            }
        }

        function fileAction(label, action, kind) {
            const element = document.createElement("button");
            element.type = "button";
            element.className = `clipasm-playground__file-action${
                kind ? ` clipasm-playground__file-action--${kind}` : ""
            }`;
            element.textContent = label;
            element.addEventListener("click", action);
            return element;
        }

        function openRenameDialog(path) {
            renamePath = path;
            renameInput.value = path;
            renameError.hidden = true;
            renameDialog.showModal();
            renameInput.focus();
            renameInput.select();
        }

        function renameFile(event) {
            event.preventDefault();
            try {
                const path = normalizeVirtualPath(renameInput.value, "Virtual path");
                if (path !== renamePath && projectFiles.has(path)) {
                    throw new Error(`A virtual file already exists at \`${path}\`.`);
                }
                if (path === renamePath) {
                    renameDialog.close();
                    return;
                }
                const file = projectFiles.get(renamePath);
                if (!file) {
                    throw new Error("The virtual file is no longer available.");
                }
                projectFiles.delete(renamePath);
                projectFiles.set(path, file);
                const previousPath = renamePath;
                renameDialog.close();
                filesChanged(`Renamed \`${previousPath}\` to \`${path}\`.`);
            } catch (error) {
                renameError.textContent = error instanceof Error ? error.message : String(error);
                renameError.hidden = false;
            }
        }

        function filesChanged(message) {
            cancelWork();
            clearPreview();
            renderFileList();
            setStatus(message);
        }

        function showFilePreview(path, file) {
            clearFilePreview();
            const kind = previewKind(file);
            if (!kind) {
                return;
            }
            const media = document.createElement(kind);
            filePreviewUrl = URL.createObjectURL(file);
            media.src = filePreviewUrl;
            if (kind === "img") {
                media.alt = `Preview of ${path}`;
            } else {
                media.controls = true;
                media.preload = "metadata";
            }
            filePreviewTitle.textContent = path;
            filePreviewContent.replaceChildren(media);
            filePreviewDialog.showModal();
        }

        function clearFilePreview() {
            filePreviewContent.replaceChildren();
            if (filePreviewUrl) {
                URL.revokeObjectURL(filePreviewUrl);
                filePreviewUrl = undefined;
            }
        }

        function updateLineNumbers() {
            const lineCount = editor.value.split("\n").length;
            if (lineCount === displayedLineCount) {
                return;
            }
            editorFrame.style.setProperty(
                "--clipasm-line-number-width",
                `${Math.max(3, String(lineCount).length + 2)}ch`,
            );
            lineNumbers.textContent = Array.from(
                { length: lineCount },
                (_, index) => index + 1,
            ).join("\n");
            displayedLineCount = lineCount;
            syncLineNumbers();
        }

        function syncLineNumbers() {
            lineNumbers.scrollTop = editor.scrollTop;
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
            for (const action of selectedFiles.querySelectorAll("button")) {
                action.disabled = busy;
            }
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

    async function loadBundledAssets(base, encodedPaths) {
        if (!base && !encodedPaths) {
            return new Map();
        }
        if (!base || !encodedPaths) {
            throw new Error(
                "Bundled playground files require both an asset base and a JSON path list.",
            );
        }

        let paths;
        try {
            paths = JSON.parse(encodedPaths);
        } catch {
            throw new Error("The bundled playground file list is not valid JSON.");
        }
        if (
            !Array.isArray(paths) ||
            paths.some((path) => typeof path !== "string")
        ) {
            throw new Error("The bundled playground file list must be an array of paths.");
        }

        const normalizedPaths = paths.map((path) =>
            normalizeVirtualPath(path, "Bundled path"),
        );
        if (new Set(normalizedPaths).size !== normalizedPaths.length) {
            throw new Error("The bundled playground file list contains duplicate paths.");
        }

        let total = 0;
        const entries = await Promise.all(
            normalizedPaths.map(async (path) => {
                const relative = path.split("/").map(encodeURIComponent).join("/");
                const response = await fetch(new URL(relative, base), {
                    cache: "force-cache",
                });
                if (!response.ok) {
                    throw new Error(`Could not load bundled virtual file \`${path}\`.`);
                }
                const blob = await response.blob();
                if (blob.size > MAX_ASSET_BYTES) {
                    throw new Error(
                        `Bundled virtual file \`${path}\` exceeds the 128 MiB browser limit.`,
                    );
                }
                total += blob.size;
                if (total > MAX_TOTAL_ASSET_BYTES) {
                    throw new Error(
                        "Bundled virtual project files exceed the 256 MiB browser limit.",
                    );
                }
                const name = path.slice(path.lastIndexOf("/") + 1);
                return [path, new File([blob], name, { type: blob.type })];
            }),
        );
        return new Map(entries);
    }

    function normalizeVirtualPath(candidate, label) {
        const normalized = candidate.replaceAll("\\", "/");
        const parts = normalized.split("/");
        if (
            normalized.startsWith("/") ||
            parts.length === 0 ||
            parts.some((part) => part === "" || part === "." || part === "..")
        ) {
            throw new Error(`${label} \`${candidate}\` is not a safe relative path.`);
        }
        return parts.join("/");
    }

    function previewKind(file) {
        if (file.type.startsWith("image/")) {
            return "img";
        }
        if (file.type.startsWith("video/")) {
            return "video";
        }
        if (file.type.startsWith("audio/")) {
            return "audio";
        }
        const extension = file.name.split(".").pop()?.toLowerCase();
        if (["avif", "gif", "jpeg", "jpg", "png", "svg", "webp"].includes(extension)) {
            return "img";
        }
        if (["m4v", "mov", "mp4", "ogv", "webm"].includes(extension)) {
            return "video";
        }
        if (["aac", "flac", "m4a", "mp3", "oga", "ogg", "wav", "webm"].includes(extension)) {
            return "audio";
        }
        return undefined;
    }

    async function copyText(value) {
        if (!navigator.clipboard?.writeText) {
            throw new Error("Clipboard access is unavailable in this browser.");
        }
        await navigator.clipboard.writeText(value);
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
