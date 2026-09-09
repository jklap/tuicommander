# Voice Dictation

TUICommander includes local voice-to-text using Whisper AI. All processing happens on your machine — no cloud services.

## Setup

1. Open **Settings → Dictation**
2. Enable dictation
3. Download a Whisper model (recommended: `large-v3-turbo`, ~1.6 GB)
4. Wait for download to complete (progress shown in UI)
5. Optionally configure language and hotkey

## Usage

**Push-to-talk workflow:**

1. **Hold** the dictation hotkey (default: `F5`) or the mic button in the status bar
2. **Speak** your text
3. **Release** the key/button
4. Transcribed text is inserted into the focused input element (textarea, input, or contenteditable). If no text input has focus, the text falls back to the active terminal PTY. The focus target is captured at key-press time.

The hotkey works globally — even when TUICommander is not focused.

## Models

| Model | Size | Quality |
|-------|------|---------|
| small | ~488 MB | Good |
| small.en | ~488 MB | Good (English-only) |
| large-v2 | ~3.0 GB | Highest accuracy (slow) |
| **large-v3-turbo** | **~1.6 GB** | **Best (recommended, default)** |

Models are downloaded to `<config_dir>/models/` and cached between sessions.

## Languages

Auto-detect (default), or set explicitly:
English, Spanish, French, German, Italian, Portuguese, Dutch, Japanese, Chinese, Korean, Russian.

## Text Corrections

Configure word replacements applied after transcription:

| Spoken | Replaced with |
|--------|---------------|
| "new line" | `\n` |
| "tab" | `\t` |
| "period" | `.` |

Add custom corrections in Settings → Dictation → Corrections.

## Audio Device

Select which microphone to use from the dropdown in dictation settings. Lists all available input devices.

## Voice Tuning

Settings > Dictation > Voice tuning records a test phrase and shows the result in
the panel — nothing is sent to a terminal. Use it to set two gates that decide
whether captured audio counts as speech.

| Control | What it does |
|---------|--------------|
| Level gate | Audio quieter than this never reaches Whisper. The marker on the meter is the gate; the bar is your live level. |
| Speech confidence gate | Discards a transcript when Whisper itself reports it probably heard no speech. Lower is stricter; 100% turns it off. |

When a recording produces no text, the panel says why ("Rejected: ..."). Without
that line a gate that swallowed your speech and a microphone that captured
nothing look identical.

**Symptom → fix:**

- Random "Grazie" / "Thank you" appear when you are not speaking — usually a
  distant microphone that keeps picking up room noise. Raise the level gate until
  room noise sits below the marker, then lower the speech confidence gate.
- Quiet speech is dropped — lower the level gate, or raise the speech confidence
  gate toward 100%.

## Platform Notes

- **macOS:** GPU-accelerated transcription via Metal
- **Windows:** GPU-accelerated transcription via Vulkan
- **Linux:** CPU-only (optional CUDA/Vulkan build feature)
- Microphone permission is requested on first use (not at app startup)

## Status Indicators

| Indicator | Meaning |
|-----------|---------|
| Mic button (status bar) | Click/hold to start recording |
| Recording animation | Audio is being captured |
| Live level meter in the dictation preview | The selected microphone is receiving sound; it appears immediately while recording |
| Processing spinner | Whisper is transcribing |
| Model downloading | Progress bar with percentage |

## Hotkey Configuration

Change the push-to-talk hotkey in Settings → Dictation. The hotkey is registered globally via Tauri's global-shortcut plugin.

Default: `F5`

### Auto-Send

Enable 'Auto-send' in Settings > Dictation to automatically press Enter after the transcribed text is inserted into the terminal. Useful when dictating commands.
