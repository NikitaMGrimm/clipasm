# Browser runtime notices

The interactive renderer loads these separately built browser dependencies only
when **Render video** is selected:

- `@ffmpeg/ffmpeg` 0.12.15 — MIT
- `@ffmpeg/core` 0.12.10 — GPL-2.0-or-later

The single-threaded `@ffmpeg/core` build contains FFmpeg 5.1.4 and GPL-enabled
codec libraries, including x264. It is distributed under the GNU General Public
License, version 2 or any later version. ClipAsm's MIT license does not replace
or narrow those terms.

License and corresponding source/build material:

- [`@ffmpeg/ffmpeg` MIT license](LICENSE.ffmpeg-wasm-MIT)
- [GNU GPL version 2](COPYING.GPLv2)
- <https://github.com/ffmpegwasm/ffmpeg.wasm/tree/v12.15>
- <https://github.com/ffmpegwasm/ffmpeg.wasm/tree/v0.12.10>
- <https://github.com/FFmpeg/FFmpeg/tree/n5.1.4>
- <https://github.com/ffmpegwasm/x264/tree/4-cores>
- <https://github.com/ffmpegwasm/x265/tree/3.4>

The deployed files are copied from the exact npm packages and integrity hashes
recorded in `playground/web/package-lock.json`. ClipAsm does not modify the
runtime binaries.
