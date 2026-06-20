# azure-sapi

Rust source for the Azure neural TTS SAPI5 voice engine used by the Stellaris
natural-voice setup. See the top-level [`..\README.md`](../README.md) for
install and usage instructions.

- `src/lib.rs` - COM in-process server (`DllGetClassObject`, class factory).
- `src/engine.rs` - the `ISpTTSEngine`: sentence splitting, rate/volume,
  pacing, word-boundary events, PCM write path.
- `src/azure.rs` - Azure Speech REST call over WinHTTP, SSML build, PCM decode.
- `src/config.rs`, `src/logger.rs` - log dir resolution and file logger.
- `src/bin/setup/` - the `setup.exe` install/registry/test tool.

Build (needs Rust GNU toolchain + WinLibs MinGW on PATH; see `..\build.ps1`):

```powershell
..\build.ps1
```

Forked from the MIT-licensed
[kokoro-sapi](https://github.com/RvRooijen/kokoro-sapi); the local Kokoro ONNX
model was replaced with the Azure REST backend. Original license retained in
`LICENSE`.
