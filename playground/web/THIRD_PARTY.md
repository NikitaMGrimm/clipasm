# Browser runtime notices

The interactive renderer loads these separate browser dependencies only after
you select **Render video**:

- `@ffmpeg/ffmpeg` 0.12.15 — MIT
- `@ffmpeg/core` 0.12.10 — GPL-2.0-or-later

The single-threaded `@ffmpeg/core` build contains FFmpeg 5.1.4. It also contains
GPL-enabled codec libraries, including x264. The GNU General Public License,
version 2 or later, covers this build. ClipAsm's MIT license does not replace
or restrict those terms.

License and corresponding source/build material:

- [`@ffmpeg/ffmpeg` MIT license](LICENSE.ffmpeg-wasm-MIT)
- [GNU GPL version 2](COPYING.GPLv2)
- <https://github.com/ffmpegwasm/ffmpeg.wasm/tree/v12.15>
- <https://github.com/ffmpegwasm/ffmpeg.wasm/tree/v0.12.10>
- <https://github.com/FFmpeg/FFmpeg/tree/n5.1.4>
- <https://github.com/ffmpegwasm/x264/tree/4-cores>
- <https://github.com/ffmpegwasm/x265/tree/3.4>

The deployment contains files from the exact npm packages in
`playground/web/package-lock.json`. This lockfile also records each integrity
hash. ClipAsm does not modify the runtime binaries.
