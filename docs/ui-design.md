# Desktop design

The preview is a native OS window rendered by Rust egui/wgpu. The inspector is another native window; neither is a browser or webview. Widgets are Rust-drawn rather than AppKit/WinUI system widgets. Small previews retain reset and recording; detail appears on request.

The hierarchy follows [Apple toolbar guidance](https://developer.apple.com/design/human-interface-guidelines/toolbars) and [Microsoft command-bar guidance](https://learn.microsoft.com/en-ca/windows/apps/design/controls/command-bar): prioritize frequent commands, group related actions, move less frequent controls into secondary surfaces, and explain compact controls with tooltips. Icons are original native vector drawings; the application icon has an editable SVG source in assets/.

```mermaid
flowchart LR
  P[Compact ball preview + measured trajectory] --> R[Reset / instant record]
  P --> S[Source menu: cameras / video / playback]
  P --> D[Optional six-panel diagnostics]
  P --> I[Separate native config inspector]
  I --> C[Graphical calibration]
  I --> G[Geometry / tracking / quality]
  I --> O[Outputs / recording / templates]
  I --> T[Exact config text + history]
```

| Surface | Default | Equivalent command |
|---|---|---|
| Reset tracking | Toolbar | `reset` |
| Start/stop recording | Toolbar + mode chevron | `record_start` with optional `record_options`; `record_stop` |
| Camera/video selection | Source menu with cached device list | `source`, `camera_status`, `camera_format`, `camera_set` |
| Preview/diagnostics/guides | Preview first | Native `view` flags |
| Rim/exclusion/square | Inspector; source paused | `calibrate` kinds rim/exclude/square/square_cycle |
| Config fields/raw text | Inspector | `set`, `replace_config`, `config`, `undo`, `redo`, `save_config` |
| Instance/window placement | Overflow or startup args | `new_instance`; `--windows`, `--grid`, `--window-size`, `--window-position` |

Each camera format change starts a new tracking epoch and requires calibration appropriate to its geometry. Camera controls are enabled only when advertised by the native backend. A rejected frame is shown as Lost; its interval is unmeasured. The status tooltip separates delivered FPS, fit cost, complete worker processing, source budget and capture gaps. Running gaps exclude intentional pause/step boundaries; the full capture-gap counter remains inspectable.

The ball crop is projected with the selected camera model. Letterboxing fits the cropped pixel rectangle with one scale factor. Diagnostic source and map panels use the same rule; panel dimensions never distort pixel aspect. Calibration hit-testing maps back to source pixels, independent of crop and window size.

The native square editor keeps the legacy corner slots. Right-click can remove a corner, and the next click fills the missing slot without cycling the other corner labels. F changes the pose branch through the shared reflected-seed refit; it does not permute corners or silently choose a new coordinate convention.
