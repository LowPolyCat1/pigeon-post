# Voice spike

This document gives the results of the voice spike. The spike examined the effort of proximity voice for Pigeon Post.

## Scope

The spike compared two options:

- Option A: The game captures the microphone with `cpal`. The game encodes the audio with Opus (`opus` 0.4.0).
- Option B: The game uses the voice functions of the Steam API (`steamworks` 0.12.2).

The spike built one loopback example for option A. The spike read the source of `steamworks` 0.12.2 for option B.

## The requirements

| Requirement | The audio that the game needs |
| --- | --- |
| Spatial proximity voice | The decoded audio of each speaker, before the mix |
| Wind and storms muffle the voices and shrink the range | The decoded audio of each speaker, for a filter and a gain |
| Speaking tubes connect stations | The decoded audio of each speaker, for a filter and a new source position |
| The megaphone item | The decoded audio of each speaker, for a gain and a larger range |
| The sleeping passenger reacts to loudness | The loudness of each speaker on the host |
| Optional: a pigeon coos when it is out of breath | A sound effect at the position of the pigeon |

Each requirement needs the raw audio (PCM) of each speaker on the receiver. The Steam overlay voice does not give this audio. A game-side mixer is necessary for both options.

## The loopback example

The file `examples/voice_loopback.rs` contains the full voice pipeline without the network:

1. The example captures the default microphone and mixes the channels to mono.
2. The example resamples the audio to 48 kHz and cuts it into frames of 20 ms (960 samples).
3. The example encodes each frame with Opus in the VoIP mode. Then it decodes the frame.
4. The example applies a distance gain and a one-pole low-pass filter (the muffle).
5. The example resamples the audio to the output rate and plays it on the default output.

The virtual distance changes from 1 m to 25 m and back. The wind changes from calm to storm and back. Wind cuts the filter frequency from 8 kHz to 600 Hz. Wind also shrinks the range from 30 m to 10 m.

To run the example, use `cargo run --example voice_loopback -- 10`. The number is the run time in seconds. Use headphones, because speakers feed back into the microphone.

### The build

The `opus` crate uses `opusic-sys`. `opusic-sys` compiles libopus from C source with CMake and MSVC.

The first build failed on the development computer. The `cmake` crate selected a Visual Studio generator that CMake 4.1.0-rc2 does not know:

```text
CMake Error: Could not create named generator Visual Studio 18 2026
```

To prevent this error, set the generator before the build:

```sh
CMAKE_GENERATOR="Visual Studio 17 2022" cargo build --example voice_loopback
```

With this variable, the build passed. The CI computer and each developer computer need CMake and a C compiler. A pure-Rust Opus crate was not necessary.

`cpal` 0.17.3 is the version that Bevy audio uses. The example uses the same version, so the build contains one `cpal`.

### The run

The example ran for 10 seconds on the development computer. The log of the last second:

```text
input:  Mikrofon | 48000 Hz, 2 ch, F32
output: Kopfhörer | 48000 Hz, 2 ch, F32
t=10.0s captured=480480 frames=500 avg_packet=80B played=479520 underrun=1536 rms=-85.7dBFS dist=13.0m wind=0.00
```

| Measurement | Value |
| --- | --- |
| Frame length | 20 ms, 960 samples at 48 kHz |
| Frames in 10 s | 500 (50 frames each second) |
| Packet size | 70 B to 80 B, approximately 30 kbit/s |
| Output underrun | 1536 samples (32 ms), only at the start |
| Playback queue | Approximately one frame (20 ms) |

Both devices opened. Frames flowed for the full run without a stall. The loudness value (RMS in dBFS) followed the input: −30 dBFS for speech, −100 dBFS for silence.

The spike measured no latency with a microphone and a speaker. The estimate for the local part is 60 ms: 10 ms capture buffer, 20 ms frame, 20 ms queue, and 10 ms output buffer. The network adds its time to this value.

## The Steam voice API

`steamworks` 0.12.2 has no safe function for voice. The FFI functions are in `steamworks::sys`:

| Function | Result |
| --- | --- |
| `SteamAPI_ISteamUser_StartVoiceRecording` | Steam starts the capture of the microphone. |
| `SteamAPI_ISteamUser_GetAvailableVoice` | Steam gives the size of the compressed voice data that is ready. |
| `SteamAPI_ISteamUser_GetVoice` | Steam gives the compressed voice data. The format is private to Steam. |
| `SteamAPI_ISteamUser_DecompressVoice` | Steam converts compressed data to 16-bit mono PCM at a sample rate that the caller selects. |
| `SteamAPI_ISteamUser_GetVoiceOptimalSampleRate` | Steam gives the best sample rate for `DecompressVoice`. |

To get the interface pointer, call `SteamAPI_SteamUser_v023`. Each call is `unsafe`. The game must write and test its own wrapper.

Steam does not send the voice data. The game sends the compressed data on its own channel. Each receiver calls `DecompressVoice`. Then the receiver has PCM for the mixer, the filters, and the loudness.

The parameters for uncompressed capture in `GetVoice` are deprecated. The sender can get its own PCM only through `DecompressVoice` of its own data.

Steam selects the microphone from the Steam settings. The game cannot select the device.

On one computer, one Steam account can record its voice and decompress it again. A test with two players needs two Steam accounts. One computer runs one Steam client, so the test needs two computers. CI cannot run a test of Steam voice.

## The comparison

| Topic | Option A: `cpal` and Opus | Option B: Steam voice |
| --- | --- | --- |
| Capture and encoding | Done in the spike. | An `unsafe` wrapper over `steamworks::sys`. |
| Playback, mixer, and spatial audio | Game code. | Game code. The work is the same as option A. |
| Transport | An unreliable channel of renet. The Steam relay carries it. | The same channel. |
| Loudness on the host | The sender adds one byte of loudness to each packet. Or the host decodes each packet. | The host calls `DecompressVoice` for each speaker. |
| Device selection | The game selects the device. | Steam selects the device. |
| Noise suppression | None. A crate is necessary for this. | Steam does some processing. The spike did not measure it. |
| Build | Needs CMake and a C compiler. | No new build tool. |
| Test without Steam | Yes. The DSP functions have unit tests. Two game instances run on one computer. | No. Each test needs the Steam client. |
| Test of two players on one computer | Yes. | No. The test needs two accounts and two computers. |
| Latency | Approximately 60 ms plus the network. | Not measured. Steam adds its own buffer. |
| Main risk | The CMake step on new computers. | `unsafe` FFI and a codec that the game cannot examine. |

### The effort

| Work | Option A (days) | Option B (days) |
| --- | --- | --- |
| Capture and encoding | 0.5 (spike done) | 1 to 1.5 |
| Network messages, host relay, and jitter buffer | 2 to 3 | 2 to 3 |
| Mixer, spatial audio, and playback in Bevy | 2 to 3 | 2 to 3 |
| Push-to-talk, settings, and device selection | 1 | 0.5 |
| Wind, speaking tubes, megaphone, and loudness | 2 to 3 | 2 to 3 |
| Test setup | 0.5 | 1.5 to 2 |
| Total | 8 to 11 | 9 to 13 |

The two options have almost the same effort. Option B saves the capture and the encoding. Option B adds the wrapper and a difficult test setup.

## The recommendation

Use option A: `cpal` for the capture and Opus for the encoding. Send each packet on an unreliable renet channel. The Steam relay carries this channel.

The reasons:

- Each requirement needs PCM on the receiver. Steam voice does not remove this work.
- Option A runs and tests without Steam. Two players can test on one computer.
- The game selects the microphone and controls the codec.

Add CMake to the CI computer before the voice feature starts.

## Open questions

1. Does a player use push-to-talk, voice activation, or both?
2. Does the host relay all voice packets? With four players, the host sends approximately 270 kbit/s.
3. Is noise suppression or echo cancellation necessary for the first version?
4. Does the sender report its loudness, or does the host decode each packet?
5. Where does the project set `CMAKE_GENERATOR`: in the CI file, or in a tracked Cargo configuration?
