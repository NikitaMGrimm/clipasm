import { FFmpeg, FFFSType } from "./ffmpeg/wrapper/index.js";

const PLAN_VERSION = 1;
const RECIPE_CONTRACT = 1;
const WRAPPER_VERSION = "0.12.15";
const CORE_VERSION = "0.12.10";
const RUNTIME_POLICY = "ffv1-flac-matroska-v1";
const EXECUTION_TIMEOUT_MS = 5 * 60 * 1000;
const MAX_LOG_LINES = 24;
const coreUrl = new URL("./ffmpeg/core/ffmpeg-core.js", import.meta.url).href;
const wasmUrl = new URL("./ffmpeg/core/ffmpeg-core.wasm", import.meta.url).href;

let ffmpeg;
let loaded = false;
let directoriesCreated = false;
let logTail = [];
let probeSequence = 0;

self.addEventListener("message", async ({ data }) => {
    const { id, plan, files } = data;
    try {
        validatePlan(plan);
        const fileMap = validateFiles(files);
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
        const separator = asset.virtual_path.lastIndexOf("/");
        const mountPoint = asset.virtual_path.slice(0, separator);
        const name = asset.virtual_path.slice(separator + 1);
        await ffmpeg.createDir(mountPoint);
        const mounted = await ffmpeg.mount(
            FFFSType.WORKERFS,
            { blobs: [{ name, data: file }] },
            mountPoint,
        );
        if (!mounted) {
            throw new Error(`Could not mount browser asset ${index + 1}.`);
        }
        mounts.push(mountPoint);
    }
    return mounts;
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
