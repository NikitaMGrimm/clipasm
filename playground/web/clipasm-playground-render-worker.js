import { FFmpeg, FFFSType } from "./ffmpeg/wrapper/index.js";

const PLAN_VERSION = 1;
const RECIPE_CONTRACT = 1;
const WRAPPER_VERSION = "0.12.15";
const CORE_VERSION = "0.12.10";
const RUNTIME_POLICY = "ffv1-flac-matroska-v1";
const EXECUTION_TIMEOUT_MS = 5 * 60 * 1000;
const MAX_LOG_LINES = 24;
const MAX_PROBE_JSON_BYTES = 256 * 1024;
const MAX_TOTAL_PROBE_JSON_BYTES = 1024 * 1024;
const coreUrl = new URL("./ffmpeg/core/ffmpeg-core.js", import.meta.url).href;
const wasmUrl = new URL("./ffmpeg/core/ffmpeg-core.wasm", import.meta.url).href;

let ffmpeg;
let loaded = false;
let directoriesCreated = false;
let logTail = [];
let probeSequence = 0;

self.addEventListener("message", async ({ data }) => {
    const { id, operation = "render", files } = data;
    try {
        const fileMap = validateFiles(files);
        if (operation === "probe") {
            await ensureRuntime(id);
            const probes = await probeVideoAssets(data.requests, fileMap, id);
            self.postMessage({ id, status: "probed", probes });
            return;
        }
        if (operation !== "render") {
            throw new Error("The browser render worker received an unknown operation.");
        }
        const { plan } = data;
        validatePlan(plan);
        await ensureRuntime(id);
        const mounts = await mountAssets(plan.assets, fileMap);
        try {
            for (const [index, step] of plan.steps.entries()) {
                progress(id, `Rendering operation ${index + 1} of ${plan.steps.length}…`);
                await execute(step.arguments, `operation ${index + 1}`);
                await verifyArtifact(step.output, step.contract);
                await deleteFiles(step.delete_after);
            }

            progress(id, "Encoding and verifying MP4…");
            await execute(plan.export.arguments, "final MP4 export");
            await verifyArtifact(plan.export.output, plan.export.contract);
            await deleteFiles(plan.export.delete_after);
            const bytes = await ffmpeg.readFile(plan.export.output);
            const buffer =
                bytes.byteOffset === 0 && bytes.byteLength === bytes.buffer.byteLength
                    ? bytes.buffer
                    : bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
            await deleteFiles([plan.export.output]);
            self.postMessage({ id, status: "success", buffer }, [buffer]);
        } finally {
            await unmountAssets(mounts);
        }
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        const detail = logTail.length === 0 ? "" : `\n\nFFmpeg log tail:\n${logTail.join("\n")}`;
        self.postMessage({ id, status: "error", error: `${message}${detail}` });
    }
});

async function ensureRuntime(id) {
    ffmpeg ??= createRuntime();
    if (loaded) {
        return;
    }
    progress(id, "Loading the browser FFmpeg runtime (about 31 MiB)…");
    await ffmpeg.load({ coreURL: coreUrl, wasmURL: wasmUrl });
    loaded = true;
    if (!directoriesCreated) {
        await ffmpeg.createDir("/inputs");
        await ffmpeg.createDir("/work");
        await ffmpeg.createDir("/output");
        directoriesCreated = true;
    }
}

function createRuntime() {
    const runtime = new FFmpeg();
    runtime.on("log", ({ message }) => {
        if (typeof message !== "string" || message.length === 0) {
            return;
        }
        logTail.push(message);
        if (logTail.length > MAX_LOG_LINES) {
            logTail.splice(0, logTail.length - MAX_LOG_LINES);
        }
    });
    return runtime;
}

async function mountAssets(assets, files) {
    const mounts = [];
    for (const [index, asset] of assets.entries()) {
        const file = files.get(asset.path);
        if (!file) {
            throw new Error(`The required browser asset \`${asset.path}\` is missing.`);
        }
        mounts.push(await mountBlob(file, asset.virtual_path, `browser asset ${index + 1}`));
    }
    return mounts;
}

async function mountBlob(file, virtualPath, label) {
    const separator = virtualPath.lastIndexOf("/");
    const mountPoint = virtualPath.slice(0, separator);
    const name = virtualPath.slice(separator + 1);
    await ffmpeg.createDir(mountPoint);
    const mounted = await ffmpeg.mount(
        FFFSType.WORKERFS,
        { blobs: [{ name, data: file }] },
        mountPoint,
    );
    if (!mounted) {
        throw new Error(`Could not mount ${label}.`);
    }
    return mountPoint;
}

async function probeVideoAssets(requests, files, id) {
    const validated = validateProbeRequests(requests);
    const probes = [];
    const mounts = [];
    let probeBytes = 0;
    try {
        for (const [index, request] of validated.entries()) {
            const file = files.get(request.path);
            if (!file) {
                throw new Error(`The required browser video \`${request.path}\` is missing.`);
            }
            progress(id, `Inspecting browser video ${index + 1} of ${validated.length}…`);
            const virtualPath = `/inputs/probe-${index}/asset${safeExtension(request.path)}`;
            mounts.push(await mountBlob(file, virtualPath, `browser video ${index + 1}`));
            const videoProbe = await probeVideo(virtualPath);
            probeBytes += videoProbe.length;
            if (probeBytes > MAX_TOTAL_PROBE_JSON_BYTES) {
                throw new Error("FFprobe returned excessive source-video metadata.");
            }
            await execute(
                [
                    "-v",
                    "error",
                    "-xerror",
                    "-i",
                    virtualPath,
                    "-map",
                    "0:v:0",
                    "-frames:v",
                    "1",
                    "-an",
                    "-f",
                    "null",
                    "-",
                ],
                `source video ${index + 1}`,
            );
            probes.push({ path: request.path, video_probe: videoProbe });
        }
        return probes;
    } finally {
        await unmountAssets(mounts);
    }
}

async function probeVideo(path) {
    const output = `/work/source-probe-${probeSequence++}.json`;
    try {
        logTail = [];
        await ffmpeg.ffprobe(
            [
                "-v",
                "error",
                "-count_frames",
                "-show_entries",
                "stream=codec_type,nb_read_frames,duration_ts,time_base,avg_frame_rate,sample_rate",
                "-of",
                "json",
                "-o",
                output,
                path,
            ],
            EXECUTION_TIMEOUT_MS,
        );
        const document = await ffmpeg.readFile(output, "utf8");
        if (typeof document !== "string" || document.length > MAX_PROBE_JSON_BYTES) {
            throw new Error("FFprobe returned invalid or excessive source-video metadata.");
        }
        JSON.parse(document);
        return document;
    } finally {
        await deleteFiles([output]);
    }
}

function validateProbeRequests(requests) {
    if (!Array.isArray(requests)) {
        throw new Error("Browser video probe requests are malformed.");
    }
    const paths = new Set();
    for (const request of requests) {
        if (
            !request ||
            request.kind !== "video" ||
            typeof request.path !== "string" ||
            paths.has(request.path)
        ) {
            throw new Error("Browser video probe requests are malformed or duplicated.");
        }
        paths.add(request.path);
    }
    return requests;
}

function safeExtension(path) {
    const name = path.slice(path.lastIndexOf("/") + 1);
    const separator = name.lastIndexOf(".");
    const extension = separator < 0 ? "" : name.slice(separator + 1);
    return extension.length > 0 &&
        extension.length <= 12 &&
        [...extension].every((character) => /[a-z0-9]/i.test(character))
        ? `.${extension}`
        : "";
}

async function unmountAssets(mounts) {
    for (const mount of mounts.reverse()) {
        try {
            await ffmpeg.unmount(mount);
            await ffmpeg.deleteDir(mount);
        } catch {
            // A failed render worker is discarded by the page before reuse.
        }
    }
}

async function execute(arguments_, label) {
    logTail = [];
    const exitCode = await ffmpeg.exec(arguments_, EXECUTION_TIMEOUT_MS);
    if (exitCode !== 0) {
        throw new Error(`FFmpeg failed while running ${label} (exit code ${exitCode}).`);
    }
}

async function verifyArtifact(path, contract) {
    const probePath = `/work/probe-${probeSequence++}.json`;
    try {
        logTail = [];
        await ffmpeg.ffprobe(
            [
                "-v",
                "error",
                "-count_frames",
                "-show_streams",
                "-of",
                "json",
                "-o",
                probePath,
                path,
            ],
            EXECUTION_TIMEOUT_MS,
        );
        // @ffmpeg/core 0.12.10 leaves FFprobe's success return value at -1.
        // The required output file and its validated contents are authoritative.
        const document = JSON.parse(await ffmpeg.readFile(probePath, "utf8"));
        const videos = document.streams.filter((stream) => stream.codec_type === "video");
        const audios = document.streams.filter((stream) => stream.codec_type === "audio");
        if (contract.media === "video") {
            verifyVideo(path, contract, videos, audios);
        } else if (contract.media === "audio") {
            verifyAudio(path, contract, videos, audios);
        } else {
            throw new Error(`Unknown browser artifact contract for \`${path}\`.`);
        }
        if (
            (contract.media === "audio" || contract.exact_audio_samples) &&
            contract.samples != null
        ) {
            await verifyAudioSamples(path, contract.samples);
        }
    } finally {
        await deleteFiles([probePath]);
    }
}

function verifyVideo(path, contract, videos, audios) {
    const expectedAudio = contract.audio ? 1 : 0;
    if (videos.length !== 1 || audios.length !== expectedAudio) {
        contractFailure(
            path,
            `expected one video stream and ${expectedAudio} audio stream(s), found ${videos.length} video and ${audios.length} audio streams`,
        );
    }
    const video = videos[0];
    if (video.width !== contract.width || video.height !== contract.height) {
        contractFailure(
            path,
            `expected ${contract.width}x${contract.height}, found ${video.width}x${video.height}`,
        );
    }
    if (video.pix_fmt !== contract.pixel_format) {
        contractFailure(
            path,
            `expected pixel format ${contract.pixel_format}, found ${String(video.pix_fmt)}`,
        );
    }
    const expectedRate = `${contract.fps_numerator}/${contract.fps_denominator}`;
    if (contract.frames > 1 && video.r_frame_rate !== expectedRate) {
        contractFailure(
            path,
            `expected frame rate ${expectedRate}, found ${String(video.r_frame_rate)}`,
        );
    }
    if (Number.parseInt(video.nb_read_frames, 10) !== contract.frames) {
        contractFailure(
            path,
            `expected ${contract.frames} frames, found ${String(video.nb_read_frames)}`,
        );
    }
    if (video.sample_aspect_ratio !== "1:1") {
        contractFailure(
            path,
            `expected square pixels, found ${String(video.sample_aspect_ratio)}`,
        );
    }
    verifyZeroStart(path, video);
    if (contract.audio) {
        verifyAudioStream(path, audios[0], contract);
    }
}

function verifyAudio(path, contract, videos, audios) {
    if (videos.length !== 0 || audios.length !== 1) {
        contractFailure(
            path,
            `expected one audio stream and no video, found ${videos.length} video and ${audios.length} audio streams`,
        );
    }
    verifyAudioStream(path, audios[0], contract);
}

function verifyAudioStream(path, stream, contract) {
    if (Number.parseInt(stream.sample_rate, 10) !== contract.sample_rate) {
        contractFailure(
            path,
            `expected ${contract.sample_rate} Hz Audio, found ${String(stream.sample_rate)}`,
        );
    }
    if (stream.channels !== contract.channels) {
        contractFailure(
            path,
            `expected ${contract.channels} Audio channels, found ${String(stream.channels)}`,
        );
    }
    verifyZeroStart(path, stream);
}

function verifyZeroStart(path, stream) {
    const start = Number.parseFloat(stream.start_time);
    if (!Number.isFinite(start) || Math.abs(start) > 0.000001) {
        contractFailure(path, `timestamps must begin at zero, found ${String(stream.start_time)}`);
    }
}

async function verifyAudioSamples(path, expected) {
    const samplesPath = `/work/probe-${probeSequence++}.samples`;
    try {
        await ffmpeg.ffprobe(
            [
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_frames",
                "-show_entries",
                "frame=nb_samples",
                "-of",
                "json",
                "-o",
                samplesPath,
                path,
            ],
            EXECUTION_TIMEOUT_MS,
        );
        const document = JSON.parse(await ffmpeg.readFile(samplesPath, "utf8"));
        if (!Array.isArray(document.frames)) {
            contractFailure(path, "FFprobe returned no Audio frame list");
        }
        let actual = 0;
        for (const frame of document.frames) {
            const samples = Number.parseInt(frame.nb_samples, 10);
            if (!Number.isSafeInteger(samples) || samples < 0) {
                contractFailure(
                    path,
                    `FFprobe returned an invalid Audio sample count: ${String(frame.nb_samples)}`,
                );
            }
            actual += samples;
        }
        if (actual !== expected) {
            contractFailure(path, `expected ${expected} Audio samples, decoded ${actual}`);
        }
    } finally {
        await deleteFiles([samplesPath]);
    }
}

async function deleteFiles(paths) {
    for (const path of paths) {
        try {
            await ffmpeg.deleteFile(path);
        } catch {
            // Cleanup is best effort; contract and execution errors stay primary.
        }
    }
}

function validatePlan(plan) {
    if (
        !plan ||
        plan.version !== PLAN_VERSION ||
        plan.recipe_contract !== RECIPE_CONTRACT ||
        plan.runtime?.wrapper !== WRAPPER_VERSION ||
        plan.runtime?.core !== CORE_VERSION ||
        plan.runtime?.policy !== RUNTIME_POLICY ||
        !Array.isArray(plan.assets) ||
        !Array.isArray(plan.steps) ||
        !plan.export
    ) {
        throw new Error("The browser render plan is incompatible with this runtime.");
    }
}

function validateFiles(files) {
    if (!Array.isArray(files)) {
        throw new Error("Browser render assets are malformed.");
    }
    const result = new Map();
    for (const entry of files) {
        if (
            !entry ||
            typeof entry.path !== "string" ||
            !(entry.file instanceof Blob) ||
            result.has(entry.path)
        ) {
            throw new Error("Browser render assets are malformed or duplicated.");
        }
        result.set(entry.path, entry.file);
    }
    return result;
}

function contractFailure(path, message) {
    throw new Error(`Browser artifact \`${path}\` violates its contract: ${message}.`);
}

function progress(id, message) {
    self.postMessage({ id, status: "progress", message });
}
