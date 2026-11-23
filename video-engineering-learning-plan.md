# Video Engineering Learning Plan — Rust + TypeScript

A hands-on path from raw pixels to streaming playback. You build your own version of each layer, then check it against professional tools. **When your tool agrees with the pro tool, you understand that layer.**

- **Rust** for pixels, codecs, bitstreams and containers
- **TypeScript** for players and streaming
- **C** read-only: ffmpeg and x264 source as reference implementations

> **On ffmpeg:** don't treat it as a black box that converts files. Use it as an *instrument* and *answer key*: `ffprobe -show_frames`, `-bsf:v trace_headers`, `-vf showinfo`. Build things yourself, then use ffmpeg to check you got them right.

---

## Project structure: one Cargo workspace

```
vidlab/
  crates/
    yuv/           # pixel formats, colour conversion
    toycodec/      # your own DCT + motion codec
    bits/          # bit reader, Exp-Golomb
    h264/          # NAL + SPS/PPS/slice header parser
    mp4/           # ISO BMFF box parser
    ts/            # MPEG-TS demuxer
    inspect-wasm/  # parsers compiled to WebAssembly
  cli/             # `vidlab` command that ties it all together
  player/          # TypeScript MSE player (Milestone 5)
```

Splitting code into crates inside a workspace is a Rust skill in its own right, and the result is a portfolio project.

---

## Milestone 1: `yuv`, pixels and colour (1–2 weeks)

**Video concepts**
- RGB vs YUV (Y′CbCr), chroma subsampling (4:4:4 / 4:2:2 / 4:2:0)
- Planar vs semi-planar layouts (I420 vs NV12)
- Colour matrices: BT.601 vs BT.709 (and BT.2020 for HDR later)
- Limited (16–235) vs full (0–255) range: a top cause of "washed out" or "crushed blacks" bugs

**Build**
- Load a PNG with the `image` crate and convert it to YUV 4:2:0 using each matrix and range
- Write raw `.yuv` and `.y4m` files
- Open them in YUView. Then use the wrong width or pixel format on purpose and study the garbage you get

**Rust concepts**
- `struct` / `enum` (`PixelFormat`, `ColorMatrix`, `ColorRange`)
- Slices, iterators, `Vec<u8>` buffers, `std::io::Write`

**Done when**
- Your output matches `ffmpeg -i in.png -pix_fmt yuv420p -color_range tv out.yuv` within rounding error

---

## Milestone 2: `toycodec`, your own codec (2–4 weeks)

**Video concepts**
- 8×8 DCT, quantisation (what QP really is), zigzag scan, run-length and entropy coding
- Intra vs inter prediction, motion estimation, residuals
- I / P / B frames, GOP structure, why scene cuts are expensive
- Objective quality: PSNR, and why it disagrees with your eyes

**Build, in stages**
1. Intra-only: DCT → quantise → zigzag → RLE, then decode back
2. Add block-matching motion search between frames; code only the residual
3. Plot PSNR against QP and against bitrate for your codec

**Rust concepts**
- Traits (`trait Transform`, `trait EntropyCoder`) and generics
- `const` arrays (zigzag table, quant matrices)
- Benchmarking with `criterion`
- Parallelise motion search with `rayon`, and see why ownership makes that safe

**Done when**
- You can encode and decode a short clip and explain why every artefact you see is there

---

## Milestone 3: `bits` + `h264`, a real bitstream (2–3 weeks)

**Video concepts**
- Annex B start codes vs AVCC length-prefixed (the format inside MP4)
- NAL unit types: SPS, PPS, IDR, non-IDR, SEI, AUD
- Exp-Golomb coding (`ue(v)` / `se(v)`)
- Profiles and levels, slice types, POC (picture order count)

**Build**
- Your own `BitReader` (no crate). Write it yourself; this is where you learn
- NAL splitter, SPS/PPS parser (resolution, profile, level, VUI colour info), slice header parser
- Encode the same clip with different x264 presets, `keyint` and B-frame settings, and diff the structures

**Rust concepts**
- Lifetimes (`BitReader<'a>` borrowing `&'a [u8]`)
- Error handling with `Result`, `thiserror`, and the `?` operator
- Unit tests and integration tests against real sample files

**Check against**
- `ffmpeg -i in.h264 -c copy -bsf:v trace_headers -f null -`
- YUView (block partitions, motion vectors)
- ffmpeg source: `libavcodec/h264_ps.c` (practice reading C)

**Done when**
- Running `vidlab h264 dump file.h264` prints the same fields as `trace_headers`

---

## Milestone 4: `mp4` + `ts`, containers and timestamps (2–3 weeks)

**Video concepts**
- ISO BMFF boxes: `ftyp`, `moov`, `trak`, `mdia`, `stbl` (`stsd`, `stts`, `ctts`, `stss`, `stsz`, `stco`)
- Fragmented MP4 / CMAF: `moof`, `traf`, `tfdt`, `trun`, `mdat`; init segments vs media segments
- Codec configuration: `avcC`, `hvcC`, and how the `CODECS` string (e.g. `avc1.64001f`) is derived from it
- MPEG-TS: 188-byte packets, PID, PAT/PMT, PES, PTS/DTS, PCR, continuity counters
- Timescales, edit lists, DTS vs PTS, A/V sync and discontinuities

**Build**
- MP4 box tree dumper (plain and fragmented)
- TS demuxer that reports PTS/DTS/PCR per stream and flags gaps and continuity errors
- A **stream linter** for HLS/DASH renditions: keyframe alignment across the ladder, segment durations, timestamp gaps, A/V offset, codec-string mismatches

**Rust concepts**
- Recursive data structures, zero-copy parsing with `&[u8]`
- Optional: rewrite one parser with `nom` after writing it by hand

**Check against**
- Bento4 `mp4dump`, GPAC `MP4Box -info`, TSDuck `tsanalyze`
- `ffprobe -show_packets -show_frames`
- Your own AWS Elemental MediaConvert / MediaPackage outputs

---

## Milestone 5: `inspect-wasm` + TypeScript player (3–4 weeks)

This is where your two languages meet, and where you stop depending on the player vendor.

**Video concepts**
- HLS (master/media playlists, `EXT-X-*` tags) and DASH (MPD, periods, adaptation sets, `SegmentTemplate`)
- Media Source Extensions: `MediaSource`, `SourceBuffer`, `appendBuffer`, buffered ranges, `timestampOffset`
- ABR: throughput vs buffer-based algorithms
- Playback failure modes: decode errors, stalls, gaps, `QuotaExceededError`

**Build**
1. Compile the `mp4`, `ts` and `h264` crates to WASM with `wasm-bindgen` / `wasm-pack`
2. A minimal MSE player in TypeScript: parse the manifest → append init + media segments → simple ABR
3. A debug panel that runs every segment through your Rust inspector: GOP structure, timestamps, codec strings, warnings
4. **Break things on purpose** and record what each failure looks like:
   - Misaligned keyframes across renditions
   - Wrong `CODECS` attribute
   - Missing init segment
   - Timestamp gaps / discontinuities
   - Mismatched audio sample rates between renditions

**Tools**
- `chrome://media-internals`
- Apple `mediastreamvalidator` / `hlsreport`
- DASH-IF conformance tool
- Then read the hls.js and Shaka Player source; it will make sense by now

**Done when**
- You can take a stream that fails in your vendor's player at work and say why using your own tool

---

## Milestone 6: Quality and encoding ladders (1–2 weeks)

- Run VMAF, PSNR and SSIM with libvmaf (and explore the Rust bindings)
- Encode a few clips at many resolution/bitrate points, plot quality against bitrate, and find the convex hull. You've just reinvented per-title encoding
- Compare x264 vs x265 vs SVT-AV1 at equal VMAF

---

## Specialisations, once the fundamentals are in place

| Area | Topics |
|---|---|
| DRM | CENC, `cenc` vs `cbcs`, Widevine / FairPlay / PlayReady, EME, PSSH boxes |
| Low latency | LL-HLS (partial segments, preload hints), chunked CMAF, LL-DASH |
| Live and ads | AWS MediaLive, SCTE-35, SSAI, ad markers in HLS/DASH |
| Codecs | HEVC, AV1, VVC; HDR (PQ, HLG, Dolby Vision, HDR10+) |
| Device players | ExoPlayer/Media3 (Kotlin), AVPlayer (Swift), smart TV platforms |
| Audio | AAC, Opus, loudness (EBU R128), channel layouts |

---

# Learning Materials

## Start here (free, hands-on)
- **Digital Video Introduction**, Leandro Moreira: hands-on intro to video compression concepts
  https://github.com/leandromoreira/digital_video_introduction
- **FFmpeg libav tutorial**, Leandro Moreira: using the libav C API directly instead of the CLI (good for practising C reading)
  https://github.com/leandromoreira/ffmpeg-libav-tutorial
- **How Video Works**, by Mux: friendly overview of the whole pipeline
  https://howvideo.works

## Books
- **The H.264 Advanced Video Compression Standard** (2nd ed.), Iain Richardson. The clearest explanation of how a modern codec works. Pairs with Milestones 2–3.
- **Digital Video and HD: Algorithms and Interfaces** (2nd ed.), Charles Poynton. The authority on colour, gamma, YUV and range. Dip into it for Milestone 1.
- **Video Demystified**, Keith Jack. Broad reference on video formats and signals.
- Iain Richardson's articles and tutorials at Vcodex: https://www.vcodex.com

## Rust
- **The Rust Programming Language** (the free "Book"): read chapters as each milestone needs them
  https://doc.rust-lang.org/book/
- **Comprehensive Rust** (Google): fast, practical course
  https://google.github.io/comprehensive-rust/
- **Rustlings**: small exercises for the basics
  https://github.com/rust-lang/rustlings
- **Programming Rust** (2nd ed., O'Reilly): a deeper reference
- **Rust for Rustaceans**, Jon Gjengset: after Milestone 3, for lifetimes, traits and API design
- **wasm-bindgen** (for Milestone 5): https://crates.io/crates/wasm-bindgen

## Specifications (free; read *alongside* your parser, not before)
- **ITU-T H.264**: https://www.itu.int/rec/T-REC-H.264
- **ITU-T H.265 (HEVC)**: https://www.itu.int/rec/T-REC-H.265
- **ISO/IEC 14496-12 (ISO BMFF / MP4)** and **ISO/IEC 23009-1 (MPEG-DASH)**, both in ISO's Publicly Available Standards list:
  https://standards.iso.org/ittf/PubliclyAvailableStandards/
- **HLS, RFC 8216**: https://datatracker.ietf.org/doc/html/rfc8216
- **Apple HLS Authoring Specification**: https://developer.apple.com/documentation/http-live-streaming/hls-authoring-specification-for-apple-devices
- **W3C Media Source Extensions**: https://www.w3.org/TR/media-source-2/
- **W3C Encrypted Media Extensions**: https://www.w3.org/TR/encrypted-media/

## Tools
| Tool | Use | Link |
|---|---|---|
| YUView | YUV viewer, bitstream analyser, motion vectors | https://github.com/IENT/YUView |
| ffmpeg / ffprobe | Answer key and instrument | https://ffmpeg.org |
| Bento4 | `mp4dump`, `mp4info`, packaging | https://github.com/axiomatic-systems/Bento4 |
| GPAC / MP4Box | MP4 inspection, DASH packaging | https://github.com/gpac/gpac |
| TSDuck | MPEG-TS analysis | https://tsduck.io |
| MediaInfo | Quick metadata check | https://mediaarea.net/en/MediaInfo |
| VMAF | Perceptual quality metric | https://github.com/Netflix/vmaf |
| Apple HLS tools | `mediastreamvalidator`, `hlsreport` | https://developer.apple.com/streaming/ |
| DASH-IF conformance | DASH validation | https://conformance.dashif.org |
| Chrome | `chrome://media-internals` | built in |

## Code worth reading
**Rust**
- rav1e, an AV1 encoder: https://github.com/xiph/rav1e
- rav1d, the Rust port of the dav1d AV1 decoder: https://github.com/memorysafety/rav1d
- Symphonia, audio demuxing and decoding: https://github.com/pdeljanov/Symphonia
- mp4-rust, an MP4 reader/writer: https://github.com/alfg/mp4-rust
- GStreamer Rust plugins: https://gitlab.freedesktop.org/gstreamer/gst-plugins-rs

**TypeScript / JavaScript players**
- hls.js: https://github.com/video-dev/hls.js
- Shaka Player: https://github.com/shaka-project/shaka-player
- dash.js: https://github.com/Dash-Industry-Forum/dash.js

**C (reference implementations)**
- FFmpeg: https://github.com/FFmpeg/FFmpeg (start with `libavcodec/h264_ps.c`, `libavformat/mov.c`, `libavformat/mpegts.c`)
- x264: https://code.videolan.org/videolan/x264

## Test media
- Xiph.org test clips (raw `.y4m`, ideal for Milestones 1–2): https://media.xiph.org/video/derf/
- Blender open movies (Big Buck Bunny, Sintel, Tears of Steel): https://studio.blender.org/films/

## Community and talks
- **Demuxed**: the best video-engineering conference; talks on YouTube
  https://demuxed.com
- **video-dev Slack**: where the hls.js, Shaka and dash.js maintainers hang out
  https://video-dev.org
- **Streaming Media** for industry news: https://www.streamingmedia.com

---

## Rough timeline

| Milestone | Duration (part-time) |
|---|---|
| 1. Pixels and colour | 1–2 weeks |
| 2. Toy codec | 2–4 weeks |
| 3. H.264 bitstream | 2–3 weeks |
| 4. Containers and timestamps | 2–3 weeks |
| 5. WASM + TS player | 3–4 weeks |
| 6. Quality and ladders | 1–2 weeks |
| **Total** | **~3–5 months** |

**Habit to build from day one:** every time something breaks at work (a playback failure, a quality complaint, an A/V sync issue), grab the manifest and segments and run them through `vidlab` before raising a vendor ticket.
