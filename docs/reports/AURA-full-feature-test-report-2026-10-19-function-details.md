# AURA registered function details — 19 October 2026

This appendix describes the actual registered handler and binding, its desktop call coverage and known documentation. Function registration is not proof that every workflow behind it is complete. Full automated case results are recorded in the main report and CSV ledgers.

## `ui_audit`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Every command in the product passes through here, and the audit log hangs off this one line: started, ok with latency, or refused with the error's code and message. That is why no call site logs anything - a per-site log is one forgotten site at a time, and this surface has 220 of them. The high-frequency reads are the exception and `isQuietIpc` names them: a grid scrolling a wedding fires `get_preview` faster than a log can hold it, and the lines would crowd out the clicks the log exists for. Their failures are still always recorded. `ui_audit` is exempt from logging too: it is the log's own pipe, and watching the watcher doubles the traffic it exists to reduce.

Native definition: `ui/src-tauri/src/main.rs:885`.

```rust
async fn ui_audit(batch: Vec<serde_json::Value>) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:22`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `automatic_start`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:3219`.

```rust
async fn automatic_start( state: State<'_, AppState>, input: AutomaticStartInput, ) -> IpcResult<AutomaticStartDto>
```

Primary UI binding: `ui/src/ipc/client.ts:105`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `photo_analysis`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Native definition: `ui/src-tauri/src/main.rs:3230`.

```rust
async fn photo_analysis( state: State<'_, AppState>, input: PhotoAutoEditInput, ) -> IpcResult<PhotoAnalysisDto>
```

Primary UI binding: `ui/src/ipc/client.ts:102`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `photo_auto_edit`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Push the queued lines into the shell's file log. Batched on an interval and flushed on errors and at page hide, because a command per line would make the relay itself one of the loudest things in the log. The call is deliberately not awaited and never logs or retries: if the shell cannot take the lines, they still live in memory and localStorage for the panel.

Native definition: `ui/src-tauri/src/main.rs:3241`.

```rust
async fn photo_auto_edit( state: State<'_, AppState>, input: PhotoAutoEditInput, ) -> IpcResult<PhotoAutoEditDto>
```

Primary UI binding: `ui/src/ipc/client.ts:99`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `one_click_finish`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: ADR-0068. Start the whole delivery pipeline. Returns immediately; poll `oneClickStatus`.

Native definition: `ui/src-tauri/src/main.rs:3255`.

```rust
async fn one_click_finish( state: State<'_, AppState>, input: OneClickFinishInput, ) -> IpcResult<OneClickFinishDto>
```

Primary UI binding: `ui/src/ipc/client.ts:109`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `one_click_status`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The live row behind the button. Rejects with `AURA-IO-1001` once the run leaves memory.

Native definition: `ui/src-tauri/src/main.rs:3266`.

```rust
async fn one_click_status(job_id: String) -> IpcResult<OneClickStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:113`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `one_click_cancel`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Stop between photographs. True when the id was known.

Native definition: `ui/src-tauri/src/main.rs:3282`.

```rust
async fn one_click_cancel(job_id: String) -> IpcResult<bool>
```

Primary UI binding: `ui/src/ipc/client.ts:117`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `activity_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 6; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Read it. Not written to the audit log: the status bar asks every second.

Native definition: `ui/src-tauri/src/main.rs:3274`.

```rust
async fn activity_status(state: State<'_, AppState>) -> IpcResult<aura_app::activity::ActivityDto>
```

Primary UI binding: `ui/src/ipc/activity.ts:25`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "activity_status",
  "args": null,
  "ok": true,
  "ms": 0.0,
  "result": {
    "items": [],
    "imageWorkersBusy": 0,
    "imageWorkers": 4
  }
}
```

## `analyse_portrait`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Native definition: `ui/src-tauri/src/main.rs:3376`.

```rust
async fn analyse_portrait( state: State<'_, AppState>, input: aura_app::portrait_commands::PortraitInput, ) -> IpcResult<aura_app::portrait_commands::PortraitAnalysisDto>
```

Primary UI binding: `ui/src/ipc/client.ts:260`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `portrait_retouch`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Native definition: `ui/src-tauri/src/main.rs:3389`.

```rust
async fn portrait_retouch( state: State<'_, AppState>, input: aura_app::portrait_commands::PortraitInput, ) -> IpcResult<aura_app::portrait_commands::PortraitRetouchDto>
```

Primary UI binding: `ui/src/ipc/client.ts:261`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_portrait_retouch`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:3402`.

```rust
async fn set_portrait_retouch( state: State<'_, AppState>, input: aura_app::portrait_commands::SetPortraitRetouchInput, ) -> IpcResult<aura_app::portrait_commands::PortraitRetouchDto>
```

Primary UI binding: `ui/src/ipc/client.ts:262`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `auto_portrait_retouch`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:3415`.

```rust
async fn auto_portrait_retouch( state: State<'_, AppState>, input: aura_app::portrait_commands::AutoPortraitRetouchInput, ) -> IpcResult<aura_app::portrait_commands::PortraitRetouchDto>
```

Primary UI binding: `ui/src/ipc/client.ts:264`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `sync_settings`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 9; refused harness calls: 1; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Lightroom's Sync Settings: copy one photo's develop settings onto others, as your own edit.

Native definition: `ui/src-tauri/src/main.rs:3474`.

```rust
async fn sync_settings( state: State<'_, AppState>, input: aura_app::develop_commands::SyncSettingsInput, ) -> IpcResult<aura_app::develop_commands::SyncSettingsReport>
```

Primary UI binding: `ui/src/ipc/client.ts:161`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "sync_settings",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83",
      "sourcePhotoId": "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7",
      "targetPhotoIds": [
        "pht_01a150aa-03f4-78e3-ac32-6fd99b2b80c3"
      ],
      "includeGeometry": false,
      "groups": [
        "geometry"
      ]
    }
  },
  "ok": true,
  "ms": 32.0,
  "result": {
    "synced": 1,
    "failed": []
  }
}
```

## `pick_white_balance`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Lightroom's Sync Settings: copy one photo's develop settings onto others, as your own edit.

Native definition: `ui/src-tauri/src/main.rs:3485`.

```rust
async fn pick_white_balance( state: State<'_, AppState>, input: aura_app::studio_tools::WhiteBalancePickInput, ) -> IpcResult<RecipeDto>
```

Primary UI binding: `ui/src/ipc/client.ts:164`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `native_retouch_edit`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 159; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:3614`.

```rust
async fn native_retouch_edit( state: State<'_, AppState>, input: aura_app::native_retouch::RetouchInput, ) -> IpcResult<Vec<aura_app::native_retouch::Edit>>
```

Primary UI binding: `ui/src/ipc/nativeRetouch.ts:177`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "native_retouch_edit",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83",
      "photoId": "pht_01a150aa-0637-7952-94a9-e26308c1ffee",
      "action": "list",
      "edits": [],
      "id": null
    }
  },
  "ok": true,
  "ms": 63.0,
  "result": [
    {
      "id": "auto-portrait-v1-0-clear",
      "tool": "frequency_heal",
      "enabled": true,
      "repairPasses": 4,
      "region": [
        0.589312,
        0.391602,
        0.335286,
        0.271484
      ],
      "source": null,
      "sourceScale": 1,
      "amount": 1,
      "feather": 0,
      "radius": 0.002214,
      "texture": 0.25,
      "sensitivity": 1,
      "moleEdge": 1,
      "tone": 1,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-blemish-surface-blemish-feature-safe"
    },
    {
      "id": "auto-portrait-v1-0-texture",
      "tool": "skin_smooth",
      "enabled": true,
      "region": [
        0.588794,
        0.372705,
        0.122489,
        0.104112
      ],
      "source": [
        0.513091,
        0.385198
      ],
      "sourceScale": 1,
      "amount": 0.693426,
      "feather": 0.6,
      "radius": 0.00166,
      "texture": 0.82,
      "tone": 0.9,
      "warmth": 0,
      "tint": 0,
      "mask": {
        "strokes": [
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.255859,
                1
              ],
              [
                0.756955,
                0.255859,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.375742,
                1
              ],
              [
                0.756955,
                0.375742,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.495625,
                1
              ],
              [
                0.756955,
                0.495625,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.527344,
                1
              ],
              [
                0.756955,
                0.527344,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.012177,
            "opacity": 1,
            "points": [
              [
                0.502981,
                0.348844,
                1
              ],
              [
                0.53616,
                0.347894,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.012177,
            "opacity": 1,
            "points": [
              [
                0.613577,
                0.345678,
                1
              ],
              [
                0.646756,
                0.344728,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.008856,
            "opacity": 1,
            "points": [
              [
                0.498237,
                0.326841,
                1
              ],
              [
                0.538052,
                0.325701,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.008856,
            "opacity": 1,
            "points": [
              [
                0.608833,
                0.323675,
                1
              ],
              [
                0.648648,
                0.322535,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.015498,
            "opacity": 1,
            "points": [
              [
                0.539356,
                0.423309,
                1
              ],
              [
                0.637365,
                0.420957,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.005535,
            "opacity": 1,
            "points": [
              [
                0.561944,
                0.395586,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.005535,
            "opacity": 1,
            "points": [
              [
                0.581852,
                0.395016,
                1
              ]
            ]
          }
        ]
      },
      "matte": "auto-portrait-v1-0-face-feature-safe"
    },
    {
      "id": "auto-portrait-v1-0-tone",
      "tool": "skin_uniformity",
      "enabled": true,
      "region": [
        0.588794,
        0.372705,
        0.122489,
        0.104112
      ],
      "source": [
        0.513091,
        0.385198
      ],
      "sourceScale": 1,
      "amount": 0.346897,
      "feather": 0.6,
      "radius": 0.004981,
      "texture": 1,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "mask": {
        "strokes": [
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.255859,
                1
              ],
              [
                0.756955,
                0.255859,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.375742,
                1
              ],
              [
                0.756955,
                0.375742,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.495625,
                1
              ],
              [
                0.756955,
                0.495625,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.527344,
                1
              ],
              [
                0.756955,
                0.527344,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.012177,
            "opacity": 1,
            "points": [
              [
                0.502981,
                0.348844,
                1
              ],
              [
                0.53616,
                0.347894,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.012177,
            "opacity": 1,
            "points": [
              [
                0.613577,
                0.345678,
                1
              ],
              [
                0.646756,
                0.344728,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.008856,
            "opacity": 1,
            "points": [
              [
                0.498237,
                0.326841,
                1
              ],
              [
                0.538052,
                0.325701,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.008856,
            "opacity": 1,
            "points": [
              [
                0.608833,
                0.323675,
                1
              ],
              [
                0.648648,
                0.322535,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.015498,
            "opacity": 1,
            "points": [
              [
                0.539356,
                0.423309,
                1
              ],
              [
                0.637365,
                0.420957,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.005535,
            "opacity": 1,
            "points": [
              [
                0.561944,
                0.395586,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.005535,
            "opacity": 1,
            "points": [
              [
                0.581852,
                0.395016,
                1
              ]
            ]
          }
        ]
      },
      "matte": "auto-portrait-v1-0-face-feature-safe"
    },
    {
      "id": "auto-portrait-v1-0-light",
      "tool": "portrait_dodge_burn",
      "enabled": true,
      "region": [
        0.588794,
        0.372705,
        0.122489,
        0.104112
      ],
      "source": [
        0.513091,
        0.385198
      ],
      "sourceScale": 1,
      "amount": 0.140676,
      "feather": 0.6,
      "radius": 0.004981,
      "texture": 1,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "mask": {
        "strokes": [
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.255859,
                1
              ],
              [
                0.756955,
                0.255859,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.375742,
                1
              ],
              [
                0.756955,
                0.375742,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.495625,
                1
              ],
              [
                0.756955,
                0.495625,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.527344,
                1
              ],
              [
                0.756955,
                0.527344,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.012177,
            "opacity": 1,
            "points": [
              [
                0.502981,
                0.348844,
                1
              ],
              [
                0.53616,
                0.347894,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.012177,
            "opacity": 1,
            "points": [
              [
                0.613577,
                0.345678,
                1
              ],
              [
                0.646756,
                0.344728,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.008856,
            "opacity": 1,
            "points": [
              [
                0.498237,
                0.326841,
                1
              ],
              [
                0.538052,
                0.325701,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.008856,
            "opacity": 1,
            "points": [
              [
                0.608833,
                0.323675,
                1
              ],
              [
                0.648648,
                0.322535,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.015498,
            "opacity": 1,
            "points": [
              [
                0.539356,
                0.423309,
                1
              ],
              [
                0.637365,
                0.420957,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.005535,
            "opacity": 1,
            "points": [
              [
                0.561944,
                0.395586,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.005535,
            "opacity": 1,
            "points": [
              [
                0.581852,
                0.395016,
                1
              ]
            ]
          }
        ]
      },
      "matte": "auto-portrait-v1-0-face-feature-safe"
    },
    {
      "id": "auto-portrait-v1-0-microdb",
      "tool": "micro_dodge_burn",
      "enabled": true,
      "region": [
        0.588794,
        0.372705,
        0.122489,
        0.104112
      ],
      "source": [
        0.513091,
        0.385198
      ],
      "sourceScale": 1,
      "amount": 0.131392,
      "feather": 0.6,
      "radius": 0.002491,
      "texture": 1,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "mask": {
        "strokes": [
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.255859,
                1
              ],
              [
                0.756955,
                0.255859,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.375742,
                1
              ],
              [
                0.756955,
                0.375742,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.495625,
                1
              ],
              [
                0.756955,
                0.495625,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.527344,
                1
              ],
              [
                0.756955,
                0.527344,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.012177,
            "opacity": 1,
            "points": [
              [
                0.502981,
                0.348844,
                1
              ],
              [
                0.53616,
                0.347894,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.012177,
            "opacity": 1,
            "points": [
              [
                0.613577,
                0.345678,
                1
              ],
              [
                0.646756,
                0.344728,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.008856,
            "opacity": 1,
            "points": [
              [
                0.498237,
                0.326841,
                1
              ],
              [
                0.538052,
                0.325701,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.008856,
            "opacity": 1,
            "points": [
              [
                0.608833,
                0.323675,
                1
              ],
              [
                0.648648,
                0.322535,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.015498,
            "opacity": 1,
            "points": [
              [
                0.539356,
                0.423309,
                1
              ],
              [
                0.637365,
                0.420957,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.005535,
            "opacity": 1,
            "points": [
              [
                0.561944,
                0.395586,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.005535,
            "opacity": 1,
            "points": [
              [
                0.581852,
                0.395016,
                1
              ]
            ]
          }
        ]
      },
      "matte": "auto-portrait-v1-0-face-feature-safe"
    },
    {
      "id": "auto-portrait-v1-0-pores",
      "tool": "frequency",
      "enabled": true,
      "region": [
        0.588794,
        0.372705,
        0.122489,
        0.104112
      ],
      "source": [
        0.513091,
        0.385198
      ],
      "sourceScale": 1,
      "amount": 0.7,
      "feather": 0.6,
      "radius": 0.000664,
      "texture": 0.9,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "mask": {
        "strokes": [
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.255859,
                1
              ],
              [
                0.756955,
                0.255859,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.375742,
                1
              ],
              [
                0.756955,
                0.375742,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.495625,
                1
              ],
              [
                0.756955,
                0.495625,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.527344,
                1
              ],
              [
                0.756955,
                0.527344,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.012177,
            "opacity": 1,
            "points": [
              [
                0.502981,
                0.348844,
                1
              ],
              [
                0.53616,
                0.347894,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.012177,
            "opacity": 1,
            "points": [
              [
                0.613577,
                0.345678,
                1
              ],
              [
                0.646756,
                0.344728,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.008856,
            "opacity": 1,
            "points": [
              [
                0.498237,
                0.326841,
                1
              ],
              [
                0.538052,
                0.325701,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.008856,
            "opacity": 1,
            "points": [
              [
                0.608833,
                0.323675,
                1
              ],
              [
                0.648648,
                0.322535,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.015498,
            "opacity": 1,
            "points": [
              [
                0.539356,
                0.423309,
                1
              ],
              [
                0.637365,
                0.420957,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.005535,
            "opacity": 1,
            "points": [
              [
                0.561944,
                0.395586,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.005535,
            "opacity": 1,
            "points": [
              [
                0.581852,
                0.395016,
                1
              ]
            ]
          }
        ]
      },
      "matte": "auto-portrait-v1-0-face-feature-safe"
    },
    {
      "id": "auto-portrait-v1-0-facelight",
      "tool": "dodge",
      "enabled": true,
      "region": [
        0.588794,
        0.372705,
        0.122489,
        0.104112
      ],
      "source": null,
      "sourceScale": 1,
      "amount": 0.13125,
      "feather": 0.6,
      "radius": 0.00166,
      "texture": 1,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "mask": {
        "strokes": [
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.255859,
                1
              ],
              [
                0.756955,
                0.255859,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.375742,
                1
              ],
              [
                0.756955,
                0.375742,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.495625,
                1
              ],
              [
                0.756955,
                0.495625,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.421669,
                0.527344,
                1
              ],
              [
                0.756955,
                0.527344,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.012177,
            "opacity": 1,
            "points": [
              [
                0.502981,
                0.348844,
                1
              ],
              [
                0.53616,
                0.347894,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.012177,
            "opacity": 1,
            "points": [
              [
                0.613577,
                0.345678,
                1
              ],
              [
                0.646756,
                0.344728,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.008856,
            "opacity": 1,
            "points": [
              [
                0.498237,
                0.326841,
                1
              ],
              [
                0.538052,
                0.325701,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.008856,
            "opacity": 1,
            "points": [
              [
                0.608833,
                0.323675,
                1
              ],
              [
                0.648648,
                0.322535,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.015498,
            "opacity": 1,
            "points": [
              [
                0.539356,
                0.423309,
                1
              ],
              [
                0.637365,
                0.420957,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.005535,
            "opacity": 1,
            "points": [
              [
                0.561944,
                0.395586,
                1
              ]
            ]
          },
          {
            "erase": true,
            "radius": 0.005535,
            "opacity": 1,
            "points": [
              [
                0.581852,
                0.395016,
                1
              ]
            ]
          }
        ]
      },
      "selection": {
        "inverted": false,
        "luminance": {
          "low": -16,
          "high": 1.5,
          "softness": 0.5
        }
      },
      "matte": "auto-portrait-v1-0-face-feature-safe"
    },
    {
      "id": "auto-portrait-v1-0-body-texture",
      "tool": "skin_smooth",
      "enabled": true,
      "region": [
        0.523426,
        0.756348,
        0.22328,
        0.224121
      ],
      "source": [
        0.523426,
        0.756348
      ],
      "sourceScale": 1,
      "amount": 0.473773,
      "feather": 0.6,
      "radius": 0.00294,
      "texture": 0.87,
      "tone": 0.9,
      "warmth": 0,
      "tint": 0,
      "mask": {
        "strokes": [
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.431641,
                1
              ],
              [
                0.70571,
                0.431641,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.551523,
                1
              ],
              [
                0.70571,
                0.551523,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.671406,
                1
              ],
              [
                0.70571,
                0.671406,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.791289,
                1
              ],
              [
                0.70571,
                0.791289,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.879883,
                1
              ],
              [
                0.70571,
                0.879883,
                1
              ]
            ]
          }
        ]
      },
      "matte": "auto-portrait-v1-0-body-outside-faces-anatomy-safe-finish"
    },
    {
      "id": "auto-portrait-v1-0-body-tone",
      "tool": "skin_uniformity",
      "enabled": true,
      "region": [
        0.523426,
        0.756348,
        0.22328,
        0.224121
      ],
      "source": [
        0.523426,
        0.756348
      ],
      "sourceScale": 1,
      "amount": 0.319161,
      "feather": 0.6,
      "radius": 0.007349,
      "texture": 1,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "mask": {
        "strokes": [
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.431641,
                1
              ],
              [
                0.70571,
                0.431641,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.551523,
                1
              ],
              [
                0.70571,
                0.551523,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.671406,
                1
              ],
              [
                0.70571,
                0.671406,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.791289,
                1
              ],
              [
                0.70571,
                0.791289,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.879883,
                1
              ],
              [
                0.70571,
                0.879883,
                1
              ]
            ]
          }
        ]
      },
      "matte": "auto-portrait-v1-0-body-outside-faces-anatomy-safe-finish"
    },
    {
      "id": "auto-portrait-v1-0-body-match",
      "tool": "skin_uniformity",
      "enabled": true,
      "region": [
        0.523426,
        0.756348,
        0.22328,
        0.224121
      ],
      "source": [
        0.513091,
        0.385198
      ],
      "sourceScale": 1,
      "amount": 0.075,
      "feather": 0.6,
      "radius": 0.007349,
      "texture": 1,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "mask": {
        "strokes": [
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.431641,
                1
              ],
              [
                0.70571,
                0.431641,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.551523,
                1
              ],
              [
                0.70571,
                0.551523,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.671406,
                1
              ],
              [
                0.70571,
                0.671406,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.791289,
                1
              ],
              [
                0.70571,
                0.791289,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.879883,
                1
              ],
              [
                0.70571,
                0.879883,
                1
              ]
            ]
          }
        ]
      },
      "matte": "auto-portrait-v1-0-body-outside-faces-anatomy-safe-finish"
    },
    {
      "id": "auto-portrait-v1-0-body-redness",
      "tool": "skin_color",
      "enabled": true,
      "region": [
        0.523426,
        0.756348,
        0.22328,
        0.224121
      ],
      "source": null,
      "sourceScale": 1,
      "amount": 0.2625,
      "feather": 0.6,
      "radius": 0.007349,
      "texture": 1,
      "tone": 0.5,
      "warmth": 0,
      "tint": -0.8,
      "mask": {
        "strokes": [
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.431641,
                1
              ],
              [
                0.70571,
                0.431641,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.551523,
                1
              ],
              [
                0.70571,
                0.551523,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.671406,
                1
              ],
              [
                0.70571,
                0.671406,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.791289,
                1
              ],
              [
                0.70571,
                0.791289,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.879883,
                1
              ],
              [
                0.70571,
                0.879883,
                1
              ]
            ]
          }
        ]
      },
      "matte": "auto-portrait-v1-0-body-outside-faces-anatomy-safe-finish"
    },
    {
      "id": "auto-portrait-v1-0-body-spots",
      "tool": "frequency_heal",
      "enabled": true,
      "region": [
        0.48243,
        0.655762,
        0.22328,
        0.224121
      ],
      "source": null,
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.6,
      "radius": 0.002205,
      "texture": 0.65,
      "sensitivity": 0.5,
      "keepDarkMarks": true,
      "moleEdge": 0,
      "tone": 1,
      "warmth": 0,
      "tint": 0,
      "mask": {
        "strokes": [
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.431641,
                1
              ],
              [
                0.70571,
                0.431641,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.551523,
                1
              ],
              [
                0.70571,
                0.551523,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.671406,
                1
              ],
              [
                0.70571,
                0.671406,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.791289,
                1
              ],
              [
                0.70571,
                0.791289,
                1
              ]
            ]
          },
          {
            "erase": false,
            "radius": 0.25,
            "opacity": 1,
            "points": [
              [
                0.259151,
                0.879883,
                1
              ],
              [
                0.70571,
                0.879883,
                1
              ]
            ]
          }
        ]
      },
      "matte": "auto-portrait-v1-0-body-outside-faces-anatomy-safe-heal"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-0",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.566848,
        0.327771,
        0.006565,
        0.003707
      ],
      "source": [
        0.685348,
        0.327881
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "cleanRingFit": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-1",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.668356,
        0.375336,
        0.009363,
        0.006241
      ],
      "source": [
        0.574725,
        0.281982
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-2",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.730403,
        0.387939,
        0.003228,
        0.003177
      ],
      "source": [
        0.678755,
        0.410889
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "cleanRingFit": true,
      "curvedHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-3",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.678746,
        0.367204,
        0.007674,
        0.005115
      ],
      "source": [
        0.626007,
        0.452393
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-4",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.692276,
        0.399622,
        0.007088,
        0.004724
      ],
      "source": [
        0.668498,
        0.423096
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-5",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.70028,
        0.382927,
        0.008394,
        0.005595
      ],
      "source": [
        0.695604,
        0.366455
      ],
      "sourceScale": 0.7,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-6",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.576557,
        0.451416,
        0.004799,
        0.003198
      ],
      "source": [
        0.562271,
        0.449463
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-7",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.682139,
        0.360049,
        0.005203,
        0.003468
      ],
      "source": [
        0.682418,
        0.328857
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-8",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.721312,
        0.39411,
        0.001783,
        0.001561
      ],
      "source": [
        0.703663,
        0.4021
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "cleanRingFit": true,
      "curvedHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-9",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.566575,
        0.303026,
        0.006588,
        0.004391
      ],
      "source": [
        0.584982,
        0.307861
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-10",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.682536,
        0.376544,
        0.009005,
        0.006002
      ],
      "source": [
        0.637729,
        0.448486
      ],
      "sourceScale": 0.7,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-11",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.604706,
        0.464111,
        0.007474,
        0.004981
      ],
      "source": [
        0.623077,
        0.455811
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-12",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.587349,
        0.470146,
        0.006287,
        0.00419
      ],
      "source": [
        0.634066,
        0.448975
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-13",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.651443,
        0.387978,
        0.004648,
        0.003098
      ],
      "source": [
        0.68315,
        0.33667
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-14",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.633117,
        0.295141,
        0.008552,
        0.0057
      ],
      "source": [
        0.582784,
        0.28833
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-15",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.495665,
        0.406474,
        0.006376,
        0.004249
      ],
      "source": [
        0.522711,
        0.388428
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-16",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.623759,
        0.272166,
        0.004661,
        0.003107
      ],
      "source": [
        0.59011,
        0.305908
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-17",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.51967,
        0.424146,
        0.003899,
        0.002598
      ],
      "source": [
        0.517582,
        0.41626
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-18",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.57771,
        0.350125,
        0.003952,
        0.002634
      ],
      "source": [
        0.57326,
        0.343018
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-19",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.60967,
        0.386914,
        0.00246,
        0.001919
      ],
      "source": [
        0.621612,
        0.390381
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "cleanRingFit": true,
      "curvedHeal": true,
      "healSamples": [
        [
          -0.208459,
          1.043207
        ],
        [
          -0.506252,
          1.043207
        ],
        [
          -0.804045,
          0.788766
        ],
        [
          -1.101837,
          0.279886
        ],
        [
          -1.101837,
          0.025446
        ],
        [
          -0.208459,
          -0.992315
        ],
        [
          0.387127,
          -0.992315
        ],
        [
          0.68492,
          -0.992315
        ],
        [
          0.68492,
          -0.737875
        ],
        [
          0.982713,
          -0.483435
        ],
        [
          1.280506,
          0.025446
        ],
        [
          1.280506,
          0.279886
        ],
        [
          0.089334,
          1.297647
        ],
        [
          -0.208459,
          1.297647
        ],
        [
          -0.804045,
          1.043207
        ],
        [
          -1.101837,
          0.788766
        ],
        [
          -1.101837,
          0.534326
        ],
        [
          -0.208459,
          -1.246756
        ],
        [
          0.089334,
          -1.246756
        ],
        [
          0.982713,
          -0.737875
        ],
        [
          1.280506,
          -0.228995
        ],
        [
          1.280506,
          0.534326
        ],
        [
          -0.506252,
          1.297647
        ],
        [
          -1.101837,
          1.043207
        ],
        [
          -1.39963,
          0.279886
        ],
        [
          -1.39963,
          0.025446
        ],
        [
          -1.39963,
          -0.228995
        ],
        [
          -0.506252,
          -1.246756
        ],
        [
          0.387127,
          -1.246756
        ],
        [
          0.68492,
          -1.246756
        ],
        [
          0.982713,
          -0.992315
        ],
        [
          1.280506,
          -0.483435
        ],
        [
          1.578299,
          0.025446
        ],
        [
          1.578299,
          0.279886
        ],
        [
          1.578299,
          0.534326
        ],
        [
          0.387127,
          1.552087
        ],
        [
          0.089334,
          1.552087
        ],
        [
          -0.208459,
          1.552087
        ],
        [
          -0.506252,
          1.552087
        ],
        [
          -0.804045,
          1.297647
        ],
        "11 further values in raw evidence"
      ],
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-20",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.549118,
        0.306855,
        0.006772,
        0.004513
      ],
      "source": [
        0.59011,
        0.306885
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-21",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.630166,
        0.284395,
        0.004885,
        0.003256
      ],
      "source": [
        0.577656,
        0.307861
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-22",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.6561,
        0.445707,
        0.004609,
        0.003072
      ],
      "source": [
        0.658974,
        0.436768
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-23",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.63542,
        0.280044,
        0.005098,
        0.003398
      ],
      "source": [
        0.652381,
        0.297119
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-24",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.570232,
        0.470231,
        0.003319,
        0.002212
      ],
      "source": [
        0.601099,
        0.449951
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-25",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.517044,
        0.397188,
        0.005081,
        0.003386
      ],
      "source": [
        0.501465,
        0.397217
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-26",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.622344,
        0.264224,
        0.003552,
        0.002367
      ],
      "source": [
        0.685348,
        0.327393
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    {
      "id": "auto-portrait-v1-0-spot-deep-27",
      "tool": "patch_heal",
      "enabled": true,
      "region": [
        0.535336,
        0.29181,
        0.006156,
        0.004103
      ],
      "source": [
        0.509524,
        0.299072
      ],
      "sourceScale": 1,
      "amount": 1,
      "feather": 0.25,
      "radius": 0.002,
      "texture": 1,
      "textureHeal": true,
      "tone": 0.5,
      "warmth": 0,
      "tint": 0,
      "matte": "auto-portrait-v1-0-feature-guard-spot-feature-guard"
    },
    "11 further values in raw evidence"
  ]
}
```

## `native_retouch_blemish_review`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: How sharp a preview is: the original's own resolution, or a screen-sized first look.

Native definition: `ui/src-tauri/src/main.rs:3685`.

```rust
async fn native_retouch_blemish_review( state: State<'_, AppState>, input: aura_app::blemish_review::Input, ) -> IpcResult<aura_app::blemish_review::Report>
```

Primary UI binding: `ui/src/ipc/nativeRetouch.ts:174`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `native_retouch_selection_preview`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The photograph as taken, with no edits: what Original and Compare show.

Native definition: `ui/src-tauri/src/main.rs:3709`.

```rust
async fn native_retouch_selection_preview( state: State<'_, AppState>, input: aura_app::native_retouch::DraftInput, ) -> IpcResult<aura_app::native_retouch::SelectionPreview>
```

Primary UI binding: `ui/src/ipc/nativeRetouch.ts:186`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `native_retouch_saved_selection`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The photograph as taken, with no edits: what Original and Compare show.

Native definition: `ui/src-tauri/src/main.rs:3722`.

```rust
async fn native_retouch_saved_selection( state: State<'_, AppState>, input: aura_app::native_retouch::CoverageInput, ) -> IpcResult<aura_app::native_retouch::SelectionPreview>
```

Primary UI binding: `ui/src/ipc/nativeRetouch.ts:187`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `local_masks`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:3736`.

```rust
async fn local_masks( state: State<'_, AppState>, input: aura_app::local_mask_commands::LocalMasksInput, ) -> IpcResult<aura_app::local_mask_commands::LocalMasksDto>
```

Primary UI binding: `ui/src/ipc/localMasks.ts:59`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `create_local_mask`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 18; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:3748`.

```rust
async fn create_local_mask( state: State<'_, AppState>, input: aura_app::local_mask_commands::CreateMaskInput, ) -> IpcResult<aura_app::local_mask_commands::LocalMasksDto>
```

Primary UI binding: `ui/src/ipc/localMasks.ts:61`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "create_local_mask",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83",
      "photoId": "pht_01a150aa-03f4-78e3-ac32-6fd99b2b80c3",
      "what": "facial_hair",
      "source": null,
      "into": null,
      "mode": null,
      "invert": false,
      "bounds": null
    }
  },
  "ok": true,
  "ms": 328.0,
  "result": {
    "masks": [
      {
        "id": "mask-22a47a5f1d654a209015eaf929cccdf9",
        "name": "Subject 1",
        "enabled": true,
        "amount": 1,
        "components": [
          {
            "mode": "add",
            "source": {
              "type": "matte",
              "matte": "subject-9999a7b0a0cc",
              "what": "subject"
            }
          }
        ]
      },
      {
        "id": "mask-a074a94387c945a7aa27cee2a0d0d2f4",
        "name": "Background 2",
        "enabled": true,
        "amount": 1,
        "components": [
          {
            "mode": "add",
            "source": {
              "type": "matte",
              "matte": "background-099d8dc5e67a",
              "what": "background"
            }
          }
        ]
      },
      {
        "id": "mask-d28e5edd753145ad8c654ba6114e44c5",
        "name": "Face skin 3",
        "enabled": true,
        "amount": 1,
        "components": [
          {
            "mode": "add",
            "source": {
              "type": "matte",
              "matte": "face_skin-894ad9fdc120",
              "what": "face_skin"
            }
          }
        ]
      },
      {
        "id": "mask-57b7116b44d14d13aec193c5eabffafb",
        "name": "Body skin 4",
        "enabled": true,
        "amount": 1,
        "components": [
          {
            "mode": "add",
            "source": {
              "type": "matte",
              "matte": "body_skin-9798623ec9a4",
              "what": "body_skin"
            }
          }
        ]
      },
      {
        "id": "mask-c3b4a29c58e249c3b5f0c1731fdf9c1f",
        "name": "Hair 5",
        "enabled": true,
        "amount": 1,
        "components": [
          {
            "mode": "add",
            "source": {
              "type": "matte",
              "matte": "hair-dcee12cb7a98",
              "what": "hair"
            }
          }
        ]
      },
      {
        "id": "mask-e124f1abef98431c804b78dab88a6c42",
        "name": "Clothes 6",
        "enabled": true,
        "amount": 1,
        "components": [
          {
            "mode": "add",
            "source": {
              "type": "matte",
              "matte": "clothes-8fcf5f1d9021",
              "what": "clothes"
            }
          }
        ]
      },
      {
        "id": "mask-32fc396d481646119176d3fff240f71f",
        "name": "Eyes 7",
        "enabled": true,
        "amount": 1,
        "components": [
          {
            "mode": "add",
            "source": {
              "type": "region",
              "region": "eyes"
            }
          }
        ]
      },
      {
        "id": "mask-741cb15070b64386a2d709d761c7648a",
        "name": "Irises 8",
        "enabled": true,
        "amount": 1,
        "components": [
          {
            "mode": "add",
            "source": {
              "type": "region",
              "region": "iris"
            }
          }
        ]
      },
      {
        "id": "mask-a17e7c5b6c934d478dac8cb5d3a15d0d",
        "name": "Whites of the eyes 9",
        "enabled": true,
        "amount": 1,
        "components": [
          {
            "mode": "add",
            "source": {
              "type": "region",
              "region": "sclera"
            }
          }
        ]
      },
      {
        "id": "mask-66122278890346bdad6d3d0c8c5ab08c",
        "name": "Eyebrows 10",
        "enabled": true,
        "amount": 1,
        "components": [
          {
            "mode": "add",
            "source": {
              "type": "region",
              "region": "eyebrows"
            }
          }
        ]
      },
      {
        "id": "mask-80707de1dd5f4e2086601a63046e68ab",
        "name": "Lips 11",
        "enabled": true,
        "amount": 1,
        "components": [
          {
            "mode": "add",
            "source": {
              "type": "region",
              "region": "lips"
            }
          }
        ]
      },
      {
        "id": "mask-2a3ea586e8174f18b8de7db96b03c8f1",
        "name": "Teeth 12",
        "enabled": true,
        "amount": 1,
        "components": [
          {
            "mode": "add",
            "source": {
              "type": "region",
              "region": "teeth"
            }
          }
        ]
      },
      {
        "id": "mask-cecef36624fb4ba7ab3306df124f8f49",
        "name": "Beard and moustache 13",
        "enabled": true,
        "amount": 1,
        "components": [
          {
            "mode": "add",
            "source": {
              "type": "region",
              "region": "facial_hair"
            }
          }
        ]
      }
    ],
    "recipe": {
      "photoId": "pht_01a150aa-03f4-78e3-ac32-6fd99b2b80c3",
      "recipeHash": "443d20e1c1343273fa5e6cbec40763949582fb9853b88391f307a93e1c87d4c4",
      "schema": 1,
      "engine": "aura-render/1.0.0",
      "source": "user",
      "confidence": 0.35,
      "decisionId": null,
      "userEditedFields": [
        "studio_masks_mattes_v1.background-099d8dc5e67a.bounds",
        "studio_masks_mattes_v1.background-099d8dc5e67a.data",
        "studio_masks_mattes_v1.background-099d8dc5e67a.height",
        "studio_masks_mattes_v1.background-099d8dc5e67a.refineEdges",
        "studio_masks_mattes_v1.background-099d8dc5e67a.width",
        "studio_masks_mattes_v1.body_skin-9798623ec9a4.bounds",
        "studio_masks_mattes_v1.body_skin-9798623ec9a4.data",
        "studio_masks_mattes_v1.body_skin-9798623ec9a4.height",
        "studio_masks_mattes_v1.body_skin-9798623ec9a4.refineEdges",
        "studio_masks_mattes_v1.body_skin-9798623ec9a4.width",
        "studio_masks_mattes_v1.clothes-8fcf5f1d9021.bounds",
        "studio_masks_mattes_v1.clothes-8fcf5f1d9021.data",
        "studio_masks_mattes_v1.clothes-8fcf5f1d9021.height",
        "studio_masks_mattes_v1.clothes-8fcf5f1d9021.refineEdges",
        "studio_masks_mattes_v1.clothes-8fcf5f1d9021.width",
        "studio_masks_mattes_v1.face_skin-894ad9fdc120.bounds",
        "studio_masks_mattes_v1.face_skin-894ad9fdc120.data",
        "studio_masks_mattes_v1.face_skin-894ad9fdc120.height",
        "studio_masks_mattes_v1.face_skin-894ad9fdc120.refineEdges",
        "studio_masks_mattes_v1.face_skin-894ad9fdc120.width",
        "studio_masks_mattes_v1.hair-dcee12cb7a98.bounds",
        "studio_masks_mattes_v1.hair-dcee12cb7a98.data",
        "studio_masks_mattes_v1.hair-dcee12cb7a98.height",
        "studio_masks_mattes_v1.hair-dcee12cb7a98.refineEdges",
        "studio_masks_mattes_v1.hair-dcee12cb7a98.width",
        "studio_masks_mattes_v1.subject-9999a7b0a0cc.bounds",
        "studio_masks_mattes_v1.subject-9999a7b0a0cc.data",
        "studio_masks_mattes_v1.subject-9999a7b0a0cc.height",
        "studio_masks_mattes_v1.subject-9999a7b0a0cc.refineEdges",
        "studio_masks_mattes_v1.subject-9999a7b0a0cc.width",
        "studio_masks_v1"
      ]
    },
    "maskId": "mask-cecef36624fb4ba7ab3306df124f8f49",
    "message": null
  }
}
```

## `save_local_masks`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 13; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:3762`.

```rust
async fn save_local_masks( state: State<'_, AppState>, input: aura_app::local_mask_commands::SaveMasksInput, ) -> IpcResult<aura_app::local_mask_commands::LocalMasksDto>
```

Primary UI binding: `ui/src/ipc/localMasks.ts:63`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "save_local_masks",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83",
      "photoId": "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7",
      "masks": [],
      "label": "Finish mask audit"
    }
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "masks": [],
    "recipe": {
      "photoId": "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7",
      "recipeHash": "ec072769d3423b81d78e1dca66a20c5dfef046dabc8657f818964912df7083ea",
      "schema": 1,
      "engine": "aura-render/1.0.0",
      "source": "user",
      "confidence": 1,
      "decisionId": null,
      "userEditedFields": [
        "studio_masks_v1"
      ]
    },
    "maskId": null,
    "message": null
  }
}
```

## `local_mask_coverage`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:3776`.

```rust
async fn local_mask_coverage( state: State<'_, AppState>, input: aura_app::local_mask_commands::MaskCoverageInput, ) -> IpcResult<aura_app::local_mask_commands::MaskCoverageDto>
```

Primary UI binding: `ui/src/ipc/localMasks.ts:65`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `native_retouch_preview`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The retouch view's photograph: `full` is the original's own resolution (ADR-0097), `fast` the first look.

Native definition: `ui/src-tauri/src/main.rs:3625`.

```rust
async fn native_retouch_preview( state: State<'_, AppState>, project_id: String, photo_id: String, before: bool, quality: Option<String>, ) -> IpcResult<RenderDto>
```

Primary UI binding: `ui/src/ipc/nativeRetouch.ts:179`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `photo_original`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The photograph as taken, with no edits: what Original and Compare show.

Native definition: `ui/src-tauri/src/main.rs:3665`.

```rust
async fn photo_original( state: State<'_, AppState>, project_id: String, photo_id: String, quality: Option<String>, ) -> IpcResult<RenderDto>
```

Primary UI binding: `ui/src/ipc/nativeRetouch.ts:184`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "photo_original",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83",
    "photoId": "pht_01a150aa-03f4-78e3-ac32-6fd99b2b80c3",
    "quality": "full"
  },
  "ok": true,
  "ms": 1109.0,
  "result": {
    "width": 1066,
    "height": 1600,
    "colourSpace": "srgb",
    "renderHash": "b30572c953cba2f182b48989c66040dc7441b19da612b043af125df87bc06e74",
    "backend": "wgpu",
    "stagesRun": [
      "output_transform"
    ],
    "notes": [
      {
        "stage": "calibration",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "camera_matrix",
        "reason": "upstream_already",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "clarity",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "cleanup",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "colour_grade",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "contrast",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "curve",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "dehaze",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "exposure",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "geometry",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "grain",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "highlight_recovery",
        "reason": "upstream_already",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "hsl",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "lens_ca",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "lens_distortion",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "lens_vignette",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "masks",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "monochrome",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "noise_reduction",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "post_crop_vignette",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "restoration",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "retouch",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "sharpen",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "texture",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "tone",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "vibrance",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "white_balance",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      }
    ],
    "ms": 60
  }
}
```

## `live_preview`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: A live look while a setting changes: the retouch carried over from the last render, or null until there is one (ADR-0099). Replaced by the exact quick look when it arrives.

Native definition: `ui/src-tauri/src/main.rs:3649`.

```rust
async fn live_preview( state: State<'_, AppState>, project_id: String, photo_id: String, view: String, ) -> IpcResult<Option<RenderDto>>
```

Primary UI binding: `ui/src/ipc/nativeRetouch.ts:182`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `native_retouch_draft_preview`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The photograph as taken, with no edits: what Original and Compare show.

Native definition: `ui/src-tauri/src/main.rs:3696`.

```rust
async fn native_retouch_draft_preview( state: State<'_, AppState>, input: aura_app::native_retouch::DraftInput, ) -> IpcResult<RenderDto>
```

Primary UI binding: `ui/src/ipc/nativeRetouch.ts:185`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `list_edit_profiles`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 2; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 10.

Binding documentation: Named, adaptive looks. Every application starts from the measured correction, never compounds.

Native definition: `ui/src-tauri/src/main.rs:3498`.

```rust
async fn list_edit_profiles( state: State<'_, AppState>, ) -> IpcResult<Vec<aura_app::edit_profiles::EditProfile>>
```

Primary UI binding: `ui/src/ipc/client.ts:168`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "list_edit_profiles",
  "args": null,
  "ok": true,
  "ms": 31.0,
  "result": [
    {
      "id": "true-natural",
      "name": "True Natural",
      "category": "Everyday",
      "tagline": "Clean, honest colour. The photo, only better.",
      "description": "Balances the light, opens the shadows a touch and adds a little crispness without changing what the scene looked like. The safest choice when you want an edit nobody notices.",
      "bestFor": [
        "Family",
        "Travel",
        "Events",
        "Anything"
      ],
      "technique": [
        "Measure exposure and set the midtones where the eye expects them",
        "Recover a little highlight detail and open the deepest shadows",
        "Vibrance rather than saturation, so muted colours come up and skin does not",
        "A light touch of capture sharpening with edge masking"
      ],
      "origin": "researched",
      "sources": [
        {
          "title": "Adobe Lightroom: apply clarity, dehaze and sharpening",
          "url": "https://helpx.adobe.com/lightroom-cc/how-to/apply-clarity-dehaze-sharpen-lightroom-cc.html"
        },
        {
          "title": "Amateur Photographer: Lightroom editing tips",
          "url": "https://amateurphotographer.com/latest/landscape-photography-editing-tips/"
        }
      ],
      "evidence": null,
      "swatch": [
        "#6f8fb8",
        "#e3c3a4",
        "#7d9a5c",
        "#f2eee6",
        "#3a3b40"
      ],
      "adjust": {
        "exposure": 0,
        "contrast": 0,
        "highlights": -10,
        "shadows": 8,
        "whites": 4,
        "blacks": -4,
        "temperature": 0,
        "tint": 0,
        "clarity": 5,
        "texture": 0,
        "dehaze": 0,
        "vibrance": 10,
        "saturation": 0,
        "curve": [],
        "hsl": {},
        "sharpen": 30,
        "noise": 0,
        "vignette": 0,
        "parametric": null,
        "channelCurves": null,
        "colourGrade": null,
        "calibration": null,
        "grain": null,
        "bw": null,
        "developedStrength": null
      }
    },
    {
      "id": "light-airy",
      "name": "Light & Airy",
      "category": "Bright",
      "tagline": "Bright, soft and luminous with pastel colour.",
      "description": "The classic bright-and-airy wedding and lifestyle look: lifted exposure, open shadows, soft contrast and a gently desaturated, pastel palette. Highlights are pulled back so white dresses keep their detail.",
      "bestFor": [
        "Weddings",
        "Newborn",
        "Lifestyle",
        "Outdoor portraits"
      ],
      "technique": [
        "Raise exposure, then pull highlights back at least -25 so whites do not blow out",
        "Open the shadows, but keep a real black so the image keeps depth",
        "Lift the darks in the tone curve and soften the top for a matte glow",
        "Mute and brighten greens and yellows in HSL; brighten skin slightly",
        "A little warmth, a little lower clarity for softness",
        "Parametric curve: shadows and darks lifted; a hint of warm highlight grading"
      ],
      "origin": "researched",
      "sources": [
        {
          "title": "Brendan Williams Creative: light and airy in Lightroom",
          "url": "https://www.bwillcreative.com/how-to-make-photos-look-light-and-airy-in-lightroom/"
        },
        {
          "title": "The Editing Studio: bright and airy presets guide",
          "url": "https://theeditingstudio.co/blog/bright-and-airy-lightroom-presets"
        },
        {
          "title": "Imagen: the professional's guide to light and airy editing",
          "url": "https://imagen-ai.com/valuable-tips/light-and-airy-lightroom-presets/"
        }
      ],
      "evidence": null,
      "swatch": [
        "#f4efe8",
        "#e9d8c9",
        "#c9d6c3",
        "#dfe6ee",
        "#b7a89c"
      ],
      "adjust": {
        "exposure": 0.35,
        "contrast": -15,
        "highlights": -30,
        "shadows": 30,
        "whites": 12,
        "blacks": -4,
        "temperature": 180,
        "tint": 5,
        "clarity": -6,
        "texture": 0,
        "dehaze": 0,
        "vibrance": 6,
        "saturation": -10,
        "curve": [
          [
            0,
            16
          ],
          [
            64,
            74
          ],
          [
            192,
            206
          ],
          [
            255,
            250
          ]
        ],
        "hsl": {
          "aqua": {
            "h": 0,
            "s": -15,
            "l": 6
          },
          "blue": {
            "h": 0,
            "s": -15,
            "l": 10
          },
          "green": {
            "h": 0,
            "s": -30,
            "l": 12
          },
          "orange": {
            "h": 0,
            "s": -6,
            "l": 8
          },
          "yellow": {
            "h": -6,
            "s": -18,
            "l": 15
          }
        },
        "sharpen": 0,
        "noise": 0,
        "vignette": 0,
        "parametric": {
          "highlights": 0,
          "lights": 0,
          "darks": 5,
          "shadows": 10,
          "shadow_split": 25,
          "midtone_split": 50,
          "highlight_split": 75
        },
        "channelCurves": null,
        "colourGrade": {
          "shadows": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "midtones": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "highlights": {
            "hue": 40,
            "saturation": 6,
            "luminance": 0
          },
          "global": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "blending": 50,
          "balance": 0
        },
        "calibration": null,
        "grain": null,
        "bw": null,
        "developedStrength": null
      }
    },
    {
      "id": "dark-moody",
      "name": "Dark & Moody",
      "category": "Moody",
      "tagline": "Deep shadows, muted colour, matte blacks.",
      "description": "A dramatic, low-key finish: exposure and whites come down, contrast goes up, greens and blues are desaturated and darkened, and the blacks are lifted into a soft matte. A gentle vignette keeps the eye on the subject.",
      "bestFor": [
        "Portraits",
        "Autumn",
        "Interiors",
        "Storytelling"
      ],
      "technique": [
        "Exposure -0.3 to -0.7, contrast +30, highlights down hard",
        "Open the shadows a little so faces stay readable",
        "Lift the black point in the tone curve for a matte fade",
        "Desaturate and darken greens, blues and yellows; keep skin warm",
        "Clarity up a touch, saturation down, then a vignette",
        "Colour grading: cool blue-teal shadows at low saturation; a touch of grain"
      ],
      "origin": "researched",
      "sources": [
        {
          "title": "The Editing Studio: how to get the dark moody look",
          "url": "https://theeditingstudio.co/blog/how-to-get-dark-moody-look-lightroom"
        },
        {
          "title": "Lightroom Tutorials: dark and moody",
          "url": "https://www.lightroomtutorials.com/dark-and-moody/"
        },
        {
          "title": "Editing With Jazzie: dark moody photos in Lightroom",
          "url": "https://www.editingwithjazzie.com/blog/how-to-edit-dark-moody-photos-in-lightroom"
        }
      ],
      "evidence": null,
      "swatch": [
        "#2c2a28",
        "#5a4a3e",
        "#3e4a42",
        "#8a6e58",
        "#1c1f24"
      ],
      "adjust": {
        "exposure": -0.4,
        "contrast": 30,
        "highlights": -55,
        "shadows": 25,
        "whites": -22,
        "blacks": -18,
        "temperature": 120,
        "tint": 0,
        "clarity": 16,
        "texture": 0,
        "dehaze": 0,
        "vibrance": -10,
        "saturation": -16,
        "curve": [
          [
            0,
            26
          ],
          [
            70,
            62
          ],
          [
            190,
            194
          ],
          [
            255,
            238
          ]
        ],
        "hsl": {
          "aqua": {
            "h": 0,
            "s": -25,
            "l": -15
          },
          "blue": {
            "h": 0,
            "s": -28,
            "l": -22
          },
          "green": {
            "h": 10,
            "s": -38,
            "l": -25
          },
          "orange": {
            "h": 0,
            "s": -4,
            "l": -4
          },
          "yellow": {
            "h": -8,
            "s": -22,
            "l": -12
          }
        },
        "sharpen": 0,
        "noise": 0,
        "vignette": 0.3,
        "parametric": null,
        "channelCurves": null,
        "colourGrade": {
          "shadows": {
            "hue": 210,
            "saturation": 15,
            "luminance": 0
          },
          "midtones": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "highlights": {
            "hue": 40,
            "saturation": 10,
            "luminance": 0
          },
          "global": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "blending": 50,
          "balance": 0
        },
        "calibration": null,
        "grain": {
          "amount": 12,
          "size": 25,
          "roughness": 50
        },
        "bw": null,
        "developedStrength": null
      }
    },
    {
      "id": "cinematic-teal-orange",
      "name": "Cinematic Teal & Orange",
      "category": "Cinematic",
      "tagline": "Blockbuster colour: warm people, teal world.",
      "description": "The film-grade complementary palette. Skin and warm light stay golden while greens, cyans and blues are pushed toward teal, then a gentle S-curve with lifted blacks gives it a projected-print feel.",
      "bestFor": [
        "Urban",
        "Travel",
        "Couples",
        "Sunset"
      ],
      "technique": [
        "S-curve with a lifted black point",
        "Rotate greens and blues toward teal in HSL and darken them",
        "Keep orange hue near zero, lift its luminance a little to protect skin",
        "Slight warmth overall so highlights read amber",
        "Keep the effect felt rather than seen - saturation stays moderate",
        "Colour grading: teal shadows (hue 200, 28) and amber highlights (hue 38, 22), balance -10"
      ],
      "origin": "researched",
      "sources": [
        {
          "title": "Fstoppers: quick and easy orange and teal in Lightroom",
          "url": "https://fstoppers.com/education/quick-and-easy-orange-and-teal-look-lightroom-204459"
        },
        {
          "title": "Shotkit: how to get the teal and orange look",
          "url": "https://shotkit.com/teal-orange-look/"
        },
        {
          "title": "iPhotography: cinematic look with colour grading",
          "url": "https://www.iphotography.com/blog/cinematic-look-in-lightroom-with-colour-grading/"
        }
      ],
      "evidence": null,
      "swatch": [
        "#1f4e5a",
        "#2f7f86",
        "#d9894a",
        "#f2c28f",
        "#15262c"
      ],
      "adjust": {
        "exposure": 0,
        "contrast": 18,
        "highlights": -35,
        "shadows": 20,
        "whites": 6,
        "blacks": -10,
        "temperature": 150,
        "tint": -3,
        "clarity": 8,
        "texture": 0,
        "dehaze": 5,
        "vibrance": 10,
        "saturation": -4,
        "curve": [
          [
            0,
            14
          ],
          [
            64,
            56
          ],
          [
            192,
            202
          ],
          [
            255,
            246
          ]
        ],
        "hsl": {
          "aqua": {
            "h": -8,
            "s": 18,
            "l": -8
          },
          "blue": {
            "h": -28,
            "s": 10,
            "l": -18
          },
          "green": {
            "h": 45,
            "s": -35,
            "l": -15
          },
          "orange": {
            "h": 3,
            "s": 12,
            "l": 6
          },
          "red": {
            "h": 0,
            "s": -6,
            "l": 0
          },
          "yellow": {
            "h": -18,
            "s": -20,
            "l": 0
          }
        },
        "sharpen": 0,
        "noise": 0,
        "vignette": 0.22,
        "parametric": null,
        "channelCurves": null,
        "colourGrade": {
          "shadows": {
            "hue": 200,
            "saturation": 28,
            "luminance": 0
          },
          "midtones": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "highlights": {
            "hue": 38,
            "saturation": 22,
            "luminance": 0
          },
          "global": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "blending": 50,
          "balance": -10
        },
        "calibration": null,
        "grain": null,
        "bw": null,
        "developedStrength": null
      }
    },
    {
      "id": "film-portra",
      "name": "Portra Film",
      "category": "Film",
      "tagline": "Warm skin, soft roll-off, gentle film fade.",
      "description": "An emulation of the look Kodak Portra 400 scans are loved for: peachy skin, warm olive greens, soft pastel skies, lifted shadows and a highlight shoulder that never clips harshly. Saturation is harmonious rather than punchy.",
      "bestFor": [
        "Portraits",
        "Weddings",
        "Golden hour",
        "Family"
      ],
      "technique": [
        "Pull highlights down and compress the top of the curve for film roll-off",
        "Lift the black point to a warm grey",
        "Warm the white balance slightly and add a hint of magenta",
        "Shift yellows and greens toward olive and mute them; soften blues",
        "Lower texture a little; never reach for the saturation slider",
        "Colour grading: warm shadows (hue 35, 12) and soft warm highlights; fine grain"
      ],
      "origin": "researched",
      "sources": [
        {
          "title": "Medialoot: emulate the Portra 400 film look",
          "url": "https://medialoot.com/blog/how-to-emulate-the-portra-400-film-look-in-lightroom/"
        },
        {
          "title": "Mastin Labs: Portra 400 in Lightroom",
          "url": "https://mastinlabs.com/blogs/photoism/blog-portra-400-lightroom-authentic-look"
        },
        {
          "title": "The Editing Studio: Kodak Portra 400 film emulation guide",
          "url": "https://theeditingstudio.co/blog/kodak-portra-400-lightroom-preset"
        }
      ],
      "evidence": null,
      "swatch": [
        "#e8c9a8",
        "#c9a27a",
        "#8c9466",
        "#b9c7cf",
        "#5b4d44"
      ],
      "adjust": {
        "exposure": 0.12,
        "contrast": -10,
        "highlights": -40,
        "shadows": 18,
        "whites": -10,
        "blacks": 0,
        "temperature": 260,
        "tint": 6,
        "clarity": 0,
        "texture": -6,
        "dehaze": 0,
        "vibrance": -5,
        "saturation": -8,
        "curve": [
          [
            0,
            24
          ],
          [
            60,
            66
          ],
          [
            128,
            132
          ],
          [
            196,
            202
          ],
          [
            255,
            244
          ]
        ],
        "hsl": {
          "aqua": {
            "h": 0,
            "s": -20,
            "l": 4
          },
          "blue": {
            "h": -8,
            "s": -20,
            "l": 8
          },
          "green": {
            "h": -15,
            "s": -25,
            "l": 5
          },
          "orange": {
            "h": 4,
            "s": -6,
            "l": 8
          },
          "red": {
            "h": 0,
            "s": -10,
            "l": 0
          },
          "yellow": {
            "h": -12,
            "s": -18,
            "l": 4
          }
        },
        "sharpen": 0,
        "noise": 0,
        "vignette": 0,
        "parametric": null,
        "channelCurves": null,
        "colourGrade": {
          "shadows": {
            "hue": 35,
            "saturation": 12,
            "luminance": 0
          },
          "midtones": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "highlights": {
            "hue": 45,
            "saturation": 8,
            "luminance": 0
          },
          "global": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "blending": 50,
          "balance": 0
        },
        "calibration": null,
        "grain": {
          "amount": 18,
          "size": 30,
          "roughness": 50
        },
        "bw": null,
        "developedStrength": null
      }
    },
    {
      "id": "vintage-fade",
      "name": "Vintage Fade",
      "category": "Film",
      "tagline": "Sun-faded print from a family album.",
      "description": "A nostalgic, low-contrast print: both ends of the curve are faded, colour is muted and warmed, blues drift toward cyan and a soft vignette darkens the corners like an old lens.",
      "bestFor": [
        "Street",
        "Travel",
        "Lifestyle",
        "Throwback"
      ],
      "technique": [
        "Fade the curve at both ends - lifted blacks, lowered whites",
        "Lower contrast and saturation; warm the white balance",
        "Push greens toward yellow and blues toward cyan, then mute both",
        "Soften clarity and add a vignette",
        "Colour grading: cool shadows, warm highlights; coarse grain and a faded vignette"
      ],
      "origin": "researched",
      "sources": [
        {
          "title": "presets.io: ultimate guide to film look emulation",
          "url": "https://presets.io/a/blog/ultimate-guide-film-look-emulation-lightroom"
        },
        {
          "title": "Imagen: recreating the classic film look",
          "url": "https://imagen-ai.com/valuable-tips/kodak-portra-400-lightroom-preset/"
        }
      ],
      "evidence": null,
      "swatch": [
        "#d8c7a6",
        "#b59a78",
        "#8f9a82",
        "#9fb2b4",
        "#6b5a4a"
      ],
      "adjust": {
        "exposure": 0,
        "contrast": -18,
        "highlights": -20,
        "shadows": 15,
        "whites": -8,
        "blacks": 0,
        "temperature": 380,
        "tint": 10,
        "clarity": -6,
        "texture": 0,
        "dehaze": 0,
        "vibrance": -10,
        "saturation": -24,
        "curve": [
          [
            0,
            40
          ],
          [
            64,
            78
          ],
          [
            192,
            188
          ],
          [
            255,
            226
          ]
        ],
        "hsl": {
          "aqua": {
            "h": 0,
            "s": -25,
            "l": 5
          },
          "blue": {
            "h": -15,
            "s": -30,
            "l": 10
          },
          "green": {
            "h": -20,
            "s": -30,
            "l": 0
          },
          "orange": {
            "h": 0,
            "s": -10,
            "l": 4
          },
          "yellow": {
            "h": -5,
            "s": -15,
            "l": 0
          }
        },
        "sharpen": 0,
        "noise": 0,
        "vignette": 0.3,
        "parametric": null,
        "channelCurves": null,
        "colourGrade": {
          "shadows": {
            "hue": 220,
            "saturation": 12,
            "luminance": 0
          },
          "midtones": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "highlights": {
            "hue": 45,
            "saturation": 18,
            "luminance": 0
          },
          "global": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "blending": 50,
          "balance": 0
        },
        "calibration": null,
        "grain": {
          "amount": 30,
          "size": 40,
          "roughness": 60
        },
        "bw": null,
        "developedStrength": null
      }
    },
    {
      "id": "golden-hour",
      "name": "Golden Hour Glow",
      "category": "Warm",
      "tagline": "That last hour of sun, every time.",
      "description": "Wraps the frame in warm, low sun: a strong but guarded warm white balance, glowing yellows and oranges, softened greens toward autumn and a slightly lower clarity for a hazy glow. The warmth backs off automatically on frames that are already warm.",
      "bestFor": [
        "Sunset",
        "Outdoor portraits",
        "Couples",
        "Beach"
      ],
      "technique": [
        "Warm white balance and a touch of magenta",
        "Recover highlights so the sun keeps its shape",
        "Brighten and enrich yellows and oranges; move greens toward yellow",
        "Lower clarity and dehaze slightly for glow; vibrance up",
        "Colour grading: golden highlights and warm midtones"
      ],
      "origin": "researched",
      "sources": [
        {
          "title": "presets.io: bright and warm colour toning explained",
          "url": "https://presets.io/a/blog/lightroom-color-toning-bright-and-airy-explained"
        },
        {
          "title": "Amateur Photographer: Lightroom editing tips",
          "url": "https://amateurphotographer.com/latest/landscape-photography-editing-tips/"
        }
      ],
      "evidence": null,
      "swatch": [
        "#f7c46c",
        "#ec9a4f",
        "#c9723f",
        "#9b8a4a",
        "#5a3a2a"
      ],
      "adjust": {
        "exposure": 0.1,
        "contrast": 8,
        "highlights": -28,
        "shadows": 15,
        "whites": 4,
        "blacks": 0,
        "temperature": 650,
        "tint": 8,
        "clarity": -8,
        "texture": -4,
        "dehaze": -5,
        "vibrance": 16,
        "saturation": 0,
        "curve": [
          [
            0,
            6
          ],
          [
            64,
            60
          ],
          [
            192,
            200
          ],
          [
            255,
            252
          ]
        ],
        "hsl": {
          "blue": {
            "h": 0,
            "s": -20,
            "l": 5
          },
          "green": {
            "h": -15,
            "s": -15,
            "l": 0
          },
          "orange": {
            "h": 0,
            "s": 8,
            "l": 6
          },
          "red": {
            "h": 0,
            "s": 6,
            "l": 0
          },
          "yellow": {
            "h": -10,
            "s": 12,
            "l": 8
          }
        },
        "sharpen": 0,
        "noise": 0,
        "vignette": 0,
        "parametric": null,
        "channelCurves": null,
        "colourGrade": {
          "shadows": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "midtones": {
            "hue": 35,
            "saturation": 10,
            "luminance": 0
          },
          "highlights": {
            "hue": 42,
            "saturation": 25,
            "luminance": 0
          },
          "global": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "blending": 50,
          "balance": 0
        },
        "calibration": null,
        "grain": null,
        "bw": null,
        "developedStrength": null
      }
    },
    {
      "id": "bw-classic",
      "name": "Classic Monochrome",
      "category": "Black & White",
      "tagline": "Rich greys, bright skin, dramatic sky.",
      "description": "A timeless black and white conversion: the mix brightens skin through red and orange and darkens blue skies like a yellow-orange filter on film, with a full tonal range from true black to clean white.",
      "bestFor": [
        "Portraits",
        "Weddings",
        "Street",
        "Architecture"
      ],
      "technique": [
        "Convert with a custom mix: reds and oranges up for skin, blues down for sky",
        "Highlights down, shadows up, then set true whites and blacks",
        "A gentle S-curve and clarity for tonal separation",
        "A light vignette and sharpening",
        "Split-tone the monochrome: cool shadows, warm highlights, very low saturation; fine grain"
      ],
      "origin": "researched",
      "sources": [
        {
          "title": "Digital Photography School: better black and white in Lightroom",
          "url": "https://digital-photography-school.com/3-tips-for-better-black-and-white-conversion-using-lightroom/"
        },
        {
          "title": "Outdoor Photographer: converting to black and white",
          "url": "https://www.outdoorphotographer.com/tips-techniques/photo-editing-tips/converting-to-black-white-in-lightroom-classic/"
        },
        {
          "title": "PHLEARN: black and white in 4 easy steps",
          "url": "https://phlearn.com/tutorial/30-days-lightroom-day-19/"
        }
      ],
      "evidence": null,
      "swatch": [
        "#f2f2f2",
        "#bdbdbd",
        "#8a8a8a",
        "#555555",
        "#1a1a1a"
      ],
      "adjust": {
        "exposure": 0,
        "contrast": 22,
        "highlights": -30,
        "shadows": 20,
        "whites": 18,
        "blacks": -22,
        "temperature": 0,
        "tint": 0,
        "clarity": 14,
        "texture": 6,
        "dehaze": 0,
        "vibrance": 0,
        "saturation": 0,
        "curve": [
          [
            0,
            4
          ],
          [
            64,
            56
          ],
          [
            192,
            204
          ],
          [
            255,
            252
          ]
        ],
        "hsl": {},
        "sharpen": 40,
        "noise": 0,
        "vignette": 0.18,
        "parametric": null,
        "channelCurves": null,
        "colourGrade": {
          "shadows": {
            "hue": 215,
            "saturation": 6,
            "luminance": 0
          },
          "midtones": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "highlights": {
            "hue": 45,
            "saturation": 8,
            "luminance": 0
          },
          "global": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "blending": 50,
          "balance": 0
        },
        "calibration": null,
        "grain": {
          "amount": 15,
          "size": 25,
          "roughness": 50
        },
        "bw": {
          "aqua": -15,
          "blue": -25,
          "green": -10,
          "magenta": 5,
          "orange": 20,
          "purple": -5,
          "red": 15,
          "yellow": 10
        },
        "developedStrength": null
      }
    },
    {
      "id": "bw-noir",
      "name": "Film Noir",
      "category": "Black & White",
      "tagline": "Hard light, inky blacks, red-filter skies.",
      "description": "A high-contrast, graphic monochrome. The mix acts like a red filter - skies go nearly black, skin glows - and the curve is steep with deep blacks and a heavy vignette.",
      "bestFor": [
        "Street",
        "Night",
        "Dramatic portraits",
        "Architecture"
      ],
      "technique": [
        "Red-filter mix: reds, oranges and yellows up; blues and aqua far down",
        "Strong contrast and a steep S-curve",
        "Whites up, blacks crushed, clarity high",
        "Heavy vignette to close the frame",
        "Heavy, rough grain like pushed film"
      ],
      "origin": "researched",
      "sources": [
        {
          "title": "iPhotography: edit black and white photos in Lightroom",
          "url": "https://www.iphotography.com/blog/edit-black-and-white-photos-lightroom/"
        },
        {
          "title": "Imagen: mastering monochrome",
          "url": "https://imagen-ai.com/valuable-tips/black-and-white-presets-lightroom/"
        }
      ],
      "evidence": null,
      "swatch": [
        "#ffffff",
        "#9e9e9e",
        "#4a4a4a",
        "#1e1e1e",
        "#050505"
      ],
      "adjust": {
        "exposure": 0,
        "contrast": 42,
        "highlights": -20,
        "shadows": -8,
        "whites": 32,
        "blacks": -38,
        "temperature": 0,
        "tint": 0,
        "clarity": 24,
        "texture": 8,
        "dehaze": 0,
        "vibrance": 0,
        "saturation": 0,
        "curve": [
          [
            0,
            0
          ],
          [
            60,
            40
          ],
          [
            196,
            220
          ],
          [
            255,
            255
          ]
        ],
        "hsl": {},
        "sharpen": 45,
        "noise": 0,
        "vignette": 0.42,
        "parametric": null,
        "channelCurves": null,
        "colourGrade": null,
        "calibration": null,
        "grain": {
          "amount": 35,
          "size": 35,
          "roughness": 65
        },
        "bw": {
          "aqua": -30,
          "blue": -45,
          "green": -20,
          "magenta": 0,
          "orange": 25,
          "purple": -10,
          "red": 30,
          "yellow": 20
        },
        "developedStrength": null
      }
    },
    {
      "id": "vivid-landscape",
      "name": "Vivid Landscape",
      "category": "Nature",
      "tagline": "Deep skies, living greens, crisp detail.",
      "description": "A punchy nature finish: highlights recovered for the sky, shadows opened for the foreground, dehaze and clarity for depth, and blues and greens enriched while the sky is darkened for drama.",
      "bestFor": [
        "Landscape",
        "Mountains",
        "Hiking",
        "Seascape"
      ],
      "technique": [
        "Highlights down, shadows up to fit a wide dynamic range",
        "Clarity +15 to +25, a little dehaze and texture",
        "Vibrance around +20",
        "Darken and saturate blues; enrich greens and yellows",
        "Output sharpening",
        "Calibration: blue primary saturation up - the landscape photographer's sky trick"
      ],
      "origin": "researched",
      "sources": [
        {
          "title": "Amateur Photographer: landscape photography editing tips",
          "url": "https://amateurphotographer.com/latest/landscape-photography-editing-tips/"
        },
        {
          "title": "Picfair Focus: 10 expert tips for landscapes in Lightroom",
          "url": "https://focus.picfair.com/articles/10-tips-for-boosting-landscapes-in-lightroom"
        },
        {
          "title": "PhotoshopCAFE: texture, clarity and dehaze explained",
          "url": "https://photoshopcafe.com/texture-clarity-and-dehaze-in-lightroom-acr-explained-the-ultimate-comparision/"
        }
      ],
      "evidence": null,
      "swatch": [
        "#1f5fa8",
        "#3f8f5a",
        "#d6b24a",
        "#e9eef2",
        "#28332a"
      ],
      "adjust": {
        "exposure": 0,
        "contrast": 14,
        "highlights": -45,
        "shadows": 30,
        "whites": 14,
        "blacks": -14,
        "temperature": 0,
        "tint": 0,
        "clarity": 20,
        "texture": 12,
        "dehaze": 14,
        "vibrance": 22,
        "saturation": 3,
        "curve": [],
        "hsl": {
          "aqua": {
            "h": 0,
            "s": 10,
            "l": -8
          },
          "blue": {
            "h": 0,
            "s": 15,
            "l": -15
          },
          "green": {
            "h": 5,
            "s": 12,
            "l": -5
          },
          "orange": {
            "h": 0,
            "s": 5,
            "l": 0
          },
          "yellow": {
            "h": -8,
            "s": 8,
            "l": 0
          }
        },
        "sharpen": 45,
        "noise": 0,
        "vignette": 0,
        "parametric": null,
        "channelCurves": null,
        "colourGrade": null,
        "calibration": {
          "shadows_tint": 0,
          "red_hue": 0,
          "red_saturation": 5,
          "green_hue": 0,
          "green_saturation": 0,
          "blue_hue": 0,
          "blue_saturation": 20
        },
        "grain": null,
        "bw": null,
        "developedStrength": null
      }
    },
    {
      "id": "soft-portrait",
      "name": "Soft Portrait",
      "category": "Portrait",
      "tagline": "Flattering skin, gentle light, true colour.",
      "description": "A portrait-first finish: texture and clarity are lowered to soften skin without smearing it, skin is lightened and calmed a touch, the light is warmed slightly and sharpening is masked to edges so eyes and lashes stay crisp.",
      "bestFor": [
        "Headshots",
        "Portraits",
        "Bridal",
        "Beauty"
      ],
      "technique": [
        "Negative texture for skin, a little negative clarity for glow",
        "Lighten orange luminance and lower its saturation slightly",
        "Recover highlights on the forehead and cheeks",
        "Sharpen with a high masking value so only edges sharpen"
      ],
      "origin": "researched",
      "sources": [
        {
          "title": "PhotoshopCAFE: texture, clarity and dehaze explained",
          "url": "https://photoshopcafe.com/texture-clarity-and-dehaze-in-lightroom-acr-explained-the-ultimate-comparision/"
        },
        {
          "title": "Julieanne Kost: using the colour mixer for HSL",
          "url": "https://jkost.com/blog/2024/08/using-color-mixer-to-adjust-hue-saturation-and-luminance-in-lightroom-classic.html"
        }
      ],
      "evidence": null,
      "swatch": [
        "#f0d4c0",
        "#dcb39a",
        "#b58a74",
        "#e8e2dc",
        "#6a5c56"
      ],
      "adjust": {
        "exposure": 0.1,
        "contrast": 4,
        "highlights": -25,
        "shadows": 15,
        "whites": 5,
        "blacks": 0,
        "temperature": 100,
        "tint": 3,
        "clarity": -8,
        "texture": -18,
        "dehaze": 0,
        "vibrance": 6,
        "saturation": -3,
        "curve": [],
        "hsl": {
          "orange": {
            "h": 0,
            "s": -8,
            "l": 10
          },
          "red": {
            "h": 0,
            "s": -6,
            "l": 4
          },
          "yellow": {
            "h": 0,
            "s": -8,
            "l": 0
          }
        },
        "sharpen": 25,
        "noise": 10,
        "vignette": 0,
        "parametric": null,
        "channelCurves": null,
        "colourGrade": null,
        "calibration": null,
        "grain": null,
        "bw": null,
        "developedStrength": null
      }
    },
    {
      "id": "urban-night",
      "name": "Neon Night",
      "category": "Cinematic",
      "tagline": "Cool city shadows, glowing lights.",
      "description": "Built for city lights after dark: a cooler white balance, deep recovered highlights around street lamps, lifted shadows, enriched blues, magentas and ambers, and noise reduction tuned for high ISO.",
      "bestFor": [
        "Night",
        "City",
        "Concerts",
        "Neon"
      ],
      "technique": [
        "Cool the white balance and nudge tint toward magenta",
        "Pull highlights hard so lamps keep their shape; open shadows",
        "Enrich blue, purple, magenta and orange lights",
        "Clarity and a little dehaze for crisp edges",
        "Luminance and colour noise reduction for high ISO",
        "Colour grading: blue shadows, warm highlights for street lamps"
      ],
      "origin": "researched",
      "sources": [
        {
          "title": "The Editing Studio: cinematic Lightroom preset guide",
          "url": "https://theeditingstudio.co/blog/cinematic-lightroom-preset"
        },
        {
          "title": "Impact Photography: colour grade photos to look cinematic",
          "url": "https://www.impact-photography.com/how-to-color-grade-photos-to-look-cinematic-in-lightroom-a-complete-walkthrough/"
        }
      ],
      "evidence": null,
      "swatch": [
        "#101b3a",
        "#3a3fa8",
        "#c23fa0",
        "#f0a040",
        "#0a0c14"
      ],
      "adjust": {
        "exposure": 0,
        "contrast": 18,
        "highlights": -50,
        "shadows": 25,
        "whites": 10,
        "blacks": -14,
        "temperature": -420,
        "tint": 8,
        "clarity": 14,
        "texture": 0,
        "dehaze": 10,
        "vibrance": 16,
        "saturation": 0,
        "curve": [],
        "hsl": {
          "aqua": {
            "h": 0,
            "s": 15,
            "l": 0
          },
          "blue": {
            "h": -10,
            "s": 15,
            "l": 0
          },
          "magenta": {
            "h": -5,
            "s": 12,
            "l": 0
          },
          "orange": {
            "h": 0,
            "s": 10,
            "l": 0
          },
          "purple": {
            "h": 0,
            "s": 15,
            "l": 0
          }
        },
        "sharpen": 20,
        "noise": 25,
        "vignette": 0,
        "parametric": null,
        "channelCurves": null,
        "colourGrade": {
          "shadows": {
            "hue": 225,
            "saturation": 20,
            "luminance": 0
          },
          "midtones": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "highlights": {
            "hue": 30,
            "saturation": 12,
            "luminance": 0
          },
          "global": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "blending": 50,
          "balance": 0
        },
        "calibration": null,
        "grain": null,
        "bw": null,
        "developedStrength": null
      }
    },
    {
      "id": "editorial-matte",
      "name": "Editorial Matte",
      "category": "Editorial",
      "tagline": "Magazine-clean with a quiet matte finish.",
      "description": "Restrained and modern: a matte curve, calm muted colour, clean neutrals and a little crispness. Lets composition and wardrobe carry the frame, the way a fashion editorial does.",
      "bestFor": [
        "Fashion",
        "Brand",
        "Architecture",
        "Product"
      ],
      "technique": [
        "Matte curve: black point lifted, highlights slightly rolled off",
        "Saturation down, vibrance up a touch so colour stays even",
        "Mute greens and blues; keep neutrals neutral",
        "Clarity up slightly for crisp edges",
        "Parametric curve: darks lifted, lights eased for the matte finish"
      ],
      "origin": "researched",
      "sources": [
        {
          "title": "Medialoot: create a modern airy look",
          "url": "https://medialoot.com/blog/how-to-create-a-modern-airy-look-in-lightroom/"
        },
        {
          "title": "Imagen: dark presets and adaptive editing",
          "url": "https://imagen-ai.com/valuable-tips/dark-lightroom-presets/"
        }
      ],
      "evidence": null,
      "swatch": [
        "#dcd6ce",
        "#a9a39b",
        "#7f8a86",
        "#c7c1b8",
        "#3b3a38"
      ],
      "adjust": {
        "exposure": 0,
        "contrast": -5,
        "highlights": -20,
        "shadows": 10,
        "whites": 0,
        "blacks": 0,
        "temperature": 0,
        "tint": 0,
        "clarity": 6,
        "texture": 0,
        "dehaze": 0,
        "vibrance": 4,
        "saturation": -14,
        "curve": [
          [
            0,
            30
          ],
          [
            50,
            58
          ],
          [
            200,
            204
          ],
          [
            255,
            242
          ]
        ],
        "hsl": {
          "blue": {
            "h": -5,
            "s": -16,
            "l": 0
          },
          "green": {
            "h": 0,
            "s": -22,
            "l": 0
          },
          "yellow": {
            "h": 0,
            "s": -12,
            "l": 0
          }
        },
        "sharpen": 0,
        "noise": 0,
        "vignette": 0,
        "parametric": {
          "highlights": 0,
          "lights": -4,
          "darks": 8,
          "shadows": 0,
          "shadow_split": 25,
          "midtone_split": 50,
          "highlight_split": 75
        },
        "channelCurves": null,
        "colourGrade": {
          "shadows": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "midtones": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "highlights": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "global": {
            "hue": 30,
            "saturation": 4,
            "luminance": 0
          },
          "blending": 50,
          "balance": 0
        },
        "calibration": null,
        "grain": null,
        "bw": null,
        "developedStrength": null
      }
    },
    {
      "id": "fresh-crisp",
      "name": "Fresh & Crisp",
      "category": "Bright",
      "tagline": "Appetising colour and texture for food and product.",
      "description": "Bright, clean and saturated in the warm band: whites set clean, texture and clarity bring out surface detail, reds, oranges and yellows are enriched and greens stay fresh. Strong output sharpening.",
      "bestFor": [
        "Food",
        "Product",
        "Flat lay",
        "Interiors"
      ],
      "technique": [
        "Exposure up a little with whites set clean",
        "Texture and clarity up for surface detail",
        "Vibrance up; enrich red, orange, yellow and green",
        "Slight warmth, strong sharpening",
        "Calibration: red primary saturation up for appetising colour"
      ],
      "origin": "researched",
      "sources": [
        {
          "title": "Adobe Lightroom: apply clarity, dehaze and sharpening",
          "url": "https://helpx.adobe.com/lightroom-cc/how-to/apply-clarity-dehaze-sharpen-lightroom-cc.html"
        },
        {
          "title": "Julieanne Kost: using the colour mixer for HSL",
          "url": "https://jkost.com/blog/2024/08/using-color-mixer-to-adjust-hue-saturation-and-luminance-in-lightroom-classic.html"
        }
      ],
      "evidence": null,
      "swatch": [
        "#f6f1e7",
        "#e0463a",
        "#f09a2a",
        "#6aa84f",
        "#2b2320"
      ],
      "adjust": {
        "exposure": 0.2,
        "contrast": 12,
        "highlights": -20,
        "shadows": 15,
        "whites": 20,
        "blacks": -10,
        "temperature": 120,
        "tint": 0,
        "clarity": 8,
        "texture": 12,
        "dehaze": 0,
        "vibrance": 18,
        "saturation": 4,
        "curve": [],
        "hsl": {
          "green": {
            "h": 0,
            "s": 12,
            "l": 0
          },
          "orange": {
            "h": 0,
            "s": 10,
            "l": 4
          },
          "red": {
            "h": 0,
            "s": 10,
            "l": 0
          },
          "yellow": {
            "h": 0,
            "s": 8,
            "l": 6
          }
        },
        "sharpen": 50,
        "noise": 0,
        "vignette": 0,
        "parametric": null,
        "channelCurves": null,
        "colourGrade": null,
        "calibration": {
          "shadows_tint": 0,
          "red_hue": 0,
          "red_saturation": 10,
          "green_hue": 0,
          "green_saturation": 0,
          "blue_hue": 0,
          "blue_saturation": 0
        },
        "grain": null,
        "bw": null,
        "developedStrength": null
      }
    },
    {
      "id": "romantic-wedding",
      "name": "Romantic Wedding",
      "category": "Wedding",
      "tagline": "Warm, soft and timeless for the big day.",
      "description": "A wedding finish between airy and film: bright but not washed out, warm skin, a soft glow from lower clarity and texture, calm sage greens and pale skies, and highlights held so the dress keeps every detail.",
      "bestFor": [
        "Weddings",
        "Engagements",
        "Bridal",
        "Ceremony"
      ],
      "technique": [
        "Exposure up moderately; highlights held for the dress",
        "Soft curve with lifted blacks",
        "Warm white balance with a touch of magenta",
        "Sage greens (muted, slightly lighter), pale blues, light skin",
        "Lower clarity and texture for a soft glow",
        "Colour grading: warm highlights, faint warm shadows"
      ],
      "origin": "researched",
      "sources": [
        {
          "title": "Flourish Presets: editing dark and moody vs bright",
          "url": "https://flourishpresets.com/blogs/flourish-presets-lightroom-presets-luts/how-to-edit-dark-and-moody-photos-moody-presets"
        },
        {
          "title": "Wedding Rebels: Kodak Portra 400 presets",
          "url": "https://weddingrebels.co/blogs/blog/kodak-portra-400-lightroom-preset"
        },
        {
          "title": "The Luxe Lens: light and airy presets",
          "url": "https://help.luxelens.com/hc/en-us/articles/360047321574-Light-Airy-How-to-Use-Your-Presets"
        }
      ],
      "evidence": null,
      "swatch": [
        "#f3e6dc",
        "#e5c4ae",
        "#a9b59a",
        "#d9e0e6",
        "#7a655a"
      ],
      "adjust": {
        "exposure": 0.22,
        "contrast": -8,
        "highlights": -35,
        "shadows": 22,
        "whites": 8,
        "blacks": 0,
        "temperature": 220,
        "tint": 8,
        "clarity": -10,
        "texture": -8,
        "dehaze": 0,
        "vibrance": 4,
        "saturation": -8,
        "curve": [
          [
            0,
            14
          ],
          [
            64,
            70
          ],
          [
            192,
            200
          ],
          [
            255,
            250
          ]
        ],
        "hsl": {
          "aqua": {
            "h": 0,
            "s": -20,
            "l": 5
          },
          "blue": {
            "h": 0,
            "s": -20,
            "l": 10
          },
          "green": {
            "h": -10,
            "s": -30,
            "l": 8
          },
          "orange": {
            "h": 0,
            "s": -5,
            "l": 10
          },
          "yellow": {
            "h": 0,
            "s": -15,
            "l": 5
          }
        },
        "sharpen": 0,
        "noise": 0,
        "vignette": 0,
        "parametric": null,
        "channelCurves": null,
        "colourGrade": {
          "shadows": {
            "hue": 20,
            "saturation": 6,
            "luminance": 0
          },
          "midtones": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "highlights": {
            "hue": 35,
            "saturation": 10,
            "luminance": 0
          },
          "global": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "blending": 50,
          "balance": 0
        },
        "calibration": null,
        "grain": null,
        "bw": null,
        "developedStrength": null
      }
    },
    {
      "id": "nordic-cool",
      "name": "Nordic Cool",
      "category": "Moody",
      "tagline": "Clean, cool and quiet Scandinavian light.",
      "description": "A cool, desaturated palette with crisp detail: white balance cooled, greens shifted toward teal and muted, yellows quieted and a slight fade in the blacks. Skin is kept out of the cooling by the skin guard.",
      "bestFor": [
        "Winter",
        "Coast",
        "Minimal",
        "Architecture"
      ],
      "technique": [
        "Cool the white balance",
        "Mute saturation overall, especially yellows and greens",
        "Push greens toward teal",
        "Slight black fade and crisp clarity",
        "Colour grading: cool shadows; calibration: bluer blues"
      ],
      "origin": "researched",
      "sources": [
        {
          "title": "presets.io: ultimate guide to film look emulation",
          "url": "https://presets.io/a/blog/ultimate-guide-film-look-emulation-lightroom"
        },
        {
          "title": "Imagen: mastering the moody aesthetic",
          "url": "https://imagen-ai.com/valuable-tips/dark-lightroom-presets/"
        }
      ],
      "evidence": null,
      "swatch": [
        "#dfe7ec",
        "#9fb5bf",
        "#5f7f86",
        "#c9c9c4",
        "#2c353a"
      ],
      "adjust": {
        "exposure": 0,
        "contrast": 8,
        "highlights": -30,
        "shadows": 20,
        "whites": 0,
        "blacks": 0,
        "temperature": -380,
        "tint": -3,
        "clarity": 8,
        "texture": 0,
        "dehaze": 0,
        "vibrance": 5,
        "saturation": -18,
        "curve": [
          [
            0,
            16
          ],
          [
            64,
            62
          ],
          [
            192,
            196
          ],
          [
            255,
            250
          ]
        ],
        "hsl": {
          "aqua": {
            "h": 0,
            "s": -10,
            "l": 0
          },
          "blue": {
            "h": 0,
            "s": -10,
            "l": 5
          },
          "green": {
            "h": 20,
            "s": -35,
            "l": 0
          },
          "orange": {
            "h": 0,
            "s": -12,
            "l": 4
          },
          "yellow": {
            "h": 0,
            "s": -30,
            "l": 0
          }
        },
        "sharpen": 0,
        "noise": 0,
        "vignette": 0,
        "parametric": null,
        "channelCurves": null,
        "colourGrade": {
          "shadows": {
            "hue": 200,
            "saturation": 12,
            "luminance": 0
          },
          "midtones": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "highlights": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "global": {
            "hue": 0,
            "saturation": 0,
            "luminance": 0
          },
          "blending": 50,
          "balance": 0
        },
        "calibration": {
          "shadows_tint": 0,
          "red_hue": 0,
          "red_saturation": 0,
          "green_hue": 0,
          "green_saturation": 0,
          "blue_hue": 0,
          "blue_saturation": 10
        },
        "grain": null,
        "bw": null,
        "developedStrength": null
      }
    }
  ]
}
```

## `learn_lightroom_style`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Learn the photographer's own look from the develop settings in a Lightroom Classic catalogue.

Native definition: `ui/src-tauri/src/main.rs:3553`.

```rust
async fn learn_lightroom_style( state: State<'_, AppState>, input: aura_app::personal_style::LearnInput, ) -> IpcResult<aura_app::personal_style::LearnedStyle>
```

Primary UI binding: `ui/src/ipc/client.ts:175`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `licence_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 4.

Binding documentation: A subscription: renews from the licence server rather than by pasting a key.

Native definition: `ui/src-tauri/src/main.rs:3510`.

```rust
async fn licence_status( state: State<'_, AppState>, ) -> IpcResult<aura_app::licensing::LicenceStatus>
```

Primary UI binding: `ui/src/ipc/client.ts:188`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "licence_status",
  "args": {},
  "ok": true,
  "ms": 0.0,
  "result": {
    "state": "trial",
    "mayExport": true,
    "name": null,
    "email": null,
    "edition": null,
    "expires": null,
    "daysLeft": 9,
    "trialEnds": "2026-10-26",
    "message": "Free trial: 9 days left. Everything works until 2026-10-26.",
    "renews": false,
    "renewalDue": false
  }
}
```

## `activate_licence`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: A subscription: renews from the licence server rather than by pasting a key.

Native definition: `ui/src-tauri/src/main.rs:3520`.

```rust
async fn activate_licence( state: State<'_, AppState>, key: String, ) -> IpcResult<aura_app::licensing::LicenceStatus>
```

Primary UI binding: `ui/src/ipc/client.ts:189`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `deactivate_licence`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: A subscription: renews from the licence server rather than by pasting a key.

Native definition: `ui/src-tauri/src/main.rs:3543`.

```rust
async fn deactivate_licence( state: State<'_, AppState>, ) -> IpcResult<aura_app::licensing::LicenceStatus>
```

Primary UI binding: `ui/src/ipc/client.ts:190`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `refresh_licence`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Fetch the current key for this machine's subscription.

Native definition: `ui/src-tauri/src/main.rs:3533`.

```rust
async fn refresh_licence( state: State<'_, AppState>, ) -> IpcResult<aura_app::licensing::LicenceStatus>
```

Primary UI binding: `ui/src/ipc/client.ts:192`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `delete_personal_style`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Learn the photographer's own look from the develop settings in a Lightroom Classic catalogue.

Native definition: `ui/src-tauri/src/main.rs:3566`.

```rust
async fn delete_personal_style(state: State<'_, AppState>, id: String) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:176`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `apply_edit_profile`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Named, adaptive looks. Every application starts from the measured correction, never compounds.

Native definition: `ui/src-tauri/src/main.rs:3588`.

```rust
async fn apply_edit_profile( state: State<'_, AppState>, input: aura_app::edit_profiles::ApplyProfileInput, ) -> IpcResult<aura_app::edit_profiles::ApplyProfileReport>
```

Primary UI binding: `ui/src/ipc/client.ts:170`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "apply_edit_profile",
  "args": {
    "input": {
      "photoId": "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7",
      "profileId": "true-natural",
      "strength": 0.5
    }
  },
  "ok": true,
  "ms": 391.0,
  "result": {
    "profileId": "true-natural",
    "changed": 33,
    "protectedFields": [
      "global.exposure"
    ],
    "adaptations": []
  }
}
```

## `preview_edit_profile`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 48.

Binding documentation: Named, adaptive looks. Every application starts from the measured correction, never compounds.

Native definition: `ui/src-tauri/src/main.rs:3601`.

```rust
async fn preview_edit_profile( state: State<'_, AppState>, input: aura_app::edit_profiles::PreviewProfileInput, ) -> IpcResult<aura_app::edit_profiles::ProfilePreview>
```

Primary UI binding: `ui/src/ipc/client.ts:172`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `analyse_reference_style`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Measure a folder of reference photos, or an Instagram data export when `fromExport`. Nothing is downloaded.

Native definition: `ui/src-tauri/src/main.rs:3306`.

```rust
async fn analyse_reference_style( state: State<'_, AppState>, input: aura_app::reference_style::AnalyseReferenceInput, ) -> IpcResult<aura_app::reference_style::ReferenceAnalysis>
```

Primary UI binding: `ui/src/ipc/client.ts:206`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `apply_reference_style`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Measure a folder of reference photos, or an Instagram data export when `fromExport`. Nothing is downloaded.

Native definition: `ui/src-tauri/src/main.rs:3319`.

```rust
async fn apply_reference_style( state: State<'_, AppState>, input: aura_app::reference_style::ApplyReferenceInput, ) -> IpcResult<aura_app::reference_style::ApplyReferenceReport>
```

Primary UI binding: `ui/src/ipc/client.ts:207`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `match_paired_reference`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: `keepAs` keeps the look the match finds as a personal look of that name.

Native definition: `ui/src-tauri/src/main.rs:3332`.

```rust
async fn match_paired_reference( state: State<'_, AppState>, input: aura_app::paired_reference::PairedReferenceInput, ) -> IpcResult<aura_app::paired_reference::PairedReferenceReport>
```

Primary UI binding: `ui/src/ipc/client.ts:223`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `import_color_reference`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:3345`.

```rust
async fn import_color_reference(state: State<'_, AppState>, input: aura_app::color_match::ImportInput) -> IpcResult<aura_app::color_match::ReferenceSummary>
```

Primary UI binding: `ui/src/ipc/colorMatch.ts:16`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "import_color_reference",
  "args": {
    "input": {
      "path": "H:\\editing to aura\\AURA_Fresh_Edit_2026-10-18\\prj_01a14fae-0e05-7950-8592-9968c88178db-67be76a9-483a-478c-ae30-b72a2b41ea2f\\Final_JPEG\\989A2699_AURA_FINAL.jpg",
      "cancelId": "ab3afaca-1bea-41c5-8018-f7c46052755e"
    }
  },
  "ok": true,
  "ms": 1578.0,
  "result": {
    "id": "7bb6ccf42f07cddfcbb865f3e82c10f8003446611f3ed44e9d741251e6ed4d90",
    "name": "989A2699_AURA_FINAL.jpg",
    "width": 4480,
    "height": 6720,
    "convertedIcc": true,
    "regions": [
      "whole image",
      "face skin",
      "body skin",
      "hair",
      "clothes",
      "background"
    ]
  }
}
```

## `list_color_references`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 2; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 12.

Native definition: `ui/src-tauri/src/main.rs:3352`.

```rust
async fn list_color_references(state: State<'_, AppState>) -> IpcResult<Vec<aura_app::color_match::ReferenceSummary>>
```

Primary UI binding: `ui/src/ipc/colorMatch.ts:14`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "list_color_references",
  "args": null,
  "ok": true,
  "ms": 0.0,
  "result": [
    {
      "id": "7bb6ccf42f07cddfcbb865f3e82c10f8003446611f3ed44e9d741251e6ed4d90",
      "name": "989A2699_AURA_FINAL.jpg",
      "width": 4480,
      "height": 6720,
      "convertedIcc": true,
      "regions": [
        "whole image",
        "face skin",
        "body skin",
        "hair",
        "clothes",
        "background"
      ]
    }
  ]
}
```

## `prepare_color_match`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 2; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:3357`.

```rust
async fn prepare_color_match(state: State<'_, AppState>, input: aura_app::color_match::PrepareInput) -> IpcResult<aura_app::color_match::MatchReport>
```

Primary UI binding: `ui/src/ipc/colorMatch.ts:18`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "prepare_color_match",
  "args": {
    "input": {
      "photoId": "pht_01a150aa-03f4-78e3-ac32-6fd99b2b80c3",
      "referenceId": "7bb6ccf42f07cddfcbb865f3e82c10f8003446611f3ed44e9d741251e6ed4d90",
      "cancelId": "ec2914dc-2f22-4c88-9b20-a9d470c18a3c"
    }
  },
  "ok": true,
  "ms": 12047.0,
  "result": {
    "sessionId": "dee6f887-fe7b-434a-a597-7f5eb05866be",
    "photoId": "pht_01a150aa-03f4-78e3-ac32-6fd99b2b80c3",
    "reference": {
      "id": "7bb6ccf42f07cddfcbb865f3e82c10f8003446611f3ed44e9d741251e6ed4d90",
      "name": "989A2699_AURA_FINAL.jpg",
      "width": 4480,
      "height": 6720,
      "convertedIcc": true,
      "regions": [
        "whole image",
        "face skin",
        "body skin",
        "hair",
        "clothes",
        "background"
      ]
    },
    "controls": {
      "amount": 1,
      "tone": 1,
      "color": 1,
      "whiteBalance": 1
    },
    "matchedRegions": [
      "whole image",
      "face skin",
      "clothes",
      "background"
    ],
    "beforeDistance": 18.40823,
    "afterDistance": 17.073597,
    "changed": 0,
    "protectedFields": 64,
    "fullResolutionChecked": false,
    "current": true
  }
}
```

## `apply_color_match`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 5; refused harness calls: 1; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:3364`.

```rust
async fn apply_color_match(state: State<'_, AppState>, input: aura_app::color_match::ApplyInput) -> IpcResult<aura_app::color_match::MatchReport>
```

Primary UI binding: `ui/src/ipc/colorMatch.ts:20`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "apply_color_match",
  "args": {
    "input": {
      "photoId": "pht_01a150aa-03f4-78e3-ac32-6fd99b2b80c3",
      "sessionId": "dee6f887-fe7b-434a-a597-7f5eb05866be",
      "controls": {
        "amount": 0.5,
        "tone": 1,
        "color": 1,
        "whiteBalance": 1
      },
      "cancelId": "90f5bebe-1dbf-49f6-b653-cbffc93334d4"
    }
  },
  "ok": false,
  "ms": 15.0,
  "result": {
    "code": "AURA-ML-5146",
    "message": "The photo changed after reference analysis. Your newer edit was kept. Analyse again before applying.",
    "retryable": false,
    "runbookUrl": "https://aura.app/e/AURA-ML-5146"
  }
}
```

## `color_match_status`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 12.

Native definition: `ui/src-tauri/src/main.rs:3371`.

```rust
async fn color_match_status(state: State<'_, AppState>, photo_id: String) -> IpcResult<Option<aura_app::color_match::MatchReport>>
```

Primary UI binding: `ui/src/ipc/colorMatch.ts:15`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `enhance_photo`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 2; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: PHASE-14. The develop surface. Nine calls. None of them names a destination and none can overwrite a parameter a person set: `setParam` sends a person's own change, and every automated pass goes through the same merge in Rust with an automated source and is refused there.

Native definition: `ui/src-tauri/src/main.rs:3293`.

```rust
async fn enhance_photo( state: State<'_, AppState>, input: aura_app::contract::ipc::DevelopImageInput, ) -> IpcResult<aura_app::contract::ipc::RecipeDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1205`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "enhance_photo",
  "args": {
    "input": {
      "photoId": "pht_01a150aa-0637-7952-94a9-e26308c1ffee"
    }
  },
  "ok": true,
  "ms": 11469.0,
  "result": {
    "photoId": "pht_01a150aa-0637-7952-94a9-e26308c1ffee",
    "recipeHash": "c587dba423f7a4be3ba78474df9b23c8cbd83c57eeeb7e6fd9e28d231000bb3f",
    "schema": 1,
    "engine": "aura-render/1.0.0",
    "source": "ai",
    "confidence": 0.35,
    "decisionId": null,
    "userEditedFields": []
  }
}
```

## `enhance_portrait`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: How sharp a preview is: the original's own resolution, or a screen-sized first look.

Native definition: `ui/src-tauri/src/main.rs:3428`.

```rust
async fn enhance_portrait( state: State<'_, AppState>, input: aura_app::contract::ipc::DevelopImageInput, ) -> IpcResult<aura_app::contract::ipc::RecipeDto>
```

Primary UI binding: `ui/src/ipc/nativeRetouch.ts:175`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `auto_retouch`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: How sharp a preview is: the original's own resolution, or a screen-sized first look.

Native definition: `ui/src-tauri/src/main.rs:3441`.

```rust
async fn auto_retouch( state: State<'_, AppState>, input: aura_app::smart_edit::AutoRetouchInput, ) -> IpcResult<aura_app::contract::ipc::RecipeDto>
```

Primary UI binding: `ui/src/ipc/nativeRetouch.ts:176`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `auto_advanced_retouch`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Runs every stage in order; each stage that changes something is its own history step.

Native definition: `ui/src-tauri/src/main.rs:3455`.

```rust
async fn auto_advanced_retouch( app: tauri::AppHandle, state: State<'_, AppState>, input: aura_app::advanced_retouch::AdvancedRetouchInput, ) -> IpcResult<aura_app::advanced_retouch::AdvancedRetouchDto>
```

Primary UI binding: `ui/src/ipc/advancedRetouch.ts:64`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `create_project`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 2; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Narrow an unknown rejection into the typed error the backend promises.

Native definition: `ui/src-tauri/src/main.rs:183`.

```rust
fn create_project( state: State<'_, AppState>, input: CreateProjectInput, ) -> IpcResult<ProjectHandle>
```

Primary UI binding: `ui/src/ipc/client.ts:634`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "create_project",
  "args": {
    "input": {
      "name": "Full feature audit 2026-10-19",
      "coupleNames": null,
      "eventDate": null
    }
  },
  "ok": true,
  "ms": 62.0,
  "result": {
    "id": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  }
}
```

## `list_projects`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 10.

Native definition: `ui/src-tauri/src/main.rs:191`.

```rust
fn list_projects(state: State<'_, AppState>) -> IpcResult<Vec<ProjectSummary>>
```

Primary UI binding: `ui/src/ipc/client.ts:636`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `start_ingest`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 7; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:196`.

```rust
fn start_ingest( app: tauri::AppHandle, state: State<'_, AppState>, input: StartIngestInput, ) -> IpcResult<JobHandle>
```

Primary UI binding: `ui/src/ipc/client.ts:639`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "start_ingest",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83",
      "roots": [
        "H:\\editing to aura\\989A2698.CR3"
      ]
    }
  },
  "ok": true,
  "ms": 16.0,
  "result": {
    "jobId": "imp_01a150aa-0592-7ca0-9641-35e1aef33397"
  }
}
```

## `cancel_job`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:214`.

```rust
fn cancel_job(state: State<'_, AppState>, job_id: String) -> IpcResult<bool>
```

Primary UI binding: `ui/src/ipc/client.ts:641`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `ingest_progress`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 14; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:219`.

```rust
fn ingest_progress(state: State<'_, AppState>, job_id: String) -> IpcResult<IngestProgressDto>
```

Primary UI binding: `ui/src/ipc/client.ts:644`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "ingest_progress",
  "args": {
    "jobId": "imp_01a150aa-0592-7ca0-9641-35e1aef33397"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "known": true,
    "done": 1,
    "total": 1,
    "running": false
  }
}
```

## `list_images`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 3; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:224`.

```rust
fn list_images(state: State<'_, AppState>, input: ListImagesInput) -> IpcResult<Vec<ImageRowLite>>
```

Primary UI binding: `ui/src/ipc/client.ts:647`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "list_images",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83",
      "offset": 0,
      "limit": 100,
      "orderBy": "timeline"
    }
  },
  "ok": true,
  "ms": 15.0,
  "result": [
    {
      "id": "pht_01a150aa-03f4-78e3-ac32-6fd99b2b80c3",
      "fileName": "164A2098-original-fast.jpg",
      "timelineTs": null,
      "cameraId": null,
      "width": 0,
      "height": 0,
      "status": "indexed"
    },
    {
      "id": "pht_01a150aa-0637-7952-94a9-e26308c1ffee",
      "fileName": "989A2698.CR3",
      "timelineTs": null,
      "cameraId": null,
      "width": 0,
      "height": 0,
      "status": "indexed"
    },
    {
      "id": "pht_01a150aa-0267-7f72-baef-6cff2b94d75e",
      "fileName": "broken.jpg",
      "timelineTs": null,
      "cameraId": null,
      "width": 0,
      "height": 0,
      "status": "indexed"
    },
    {
      "id": "pht_01a150aa-0015-7bf0-869e-f360f41a1cd3",
      "fileName": "skin_chart.jpg",
      "timelineTs": null,
      "cameraId": null,
      "width": 0,
      "height": 0,
      "status": "indexed"
    },
    {
      "id": "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7",
      "fileName": "skin_chart.png",
      "timelineTs": null,
      "cameraId": null,
      "width": 0,
      "height": 0,
      "status": "indexed"
    }
  ]
}
```

## `set_camera_label`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:229`.

```rust
fn set_camera_label(state: State<'_, AppState>, input: SetCameraLabelInput) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:650`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `list_problems`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 4; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 2.

Native definition: `ui/src-tauri/src/main.rs:234`.

```rust
fn list_problems(state: State<'_, AppState>, project_id: String) -> IpcResult<Vec<ProblemRow>>
```

Primary UI binding: `ui/src/ipc/client.ts:653`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "list_problems",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": [
    {
      "path": "C:\\Users\\Admin\\Documents\\project\\.work-checks\\full-feature-audit-2026-10-19\\inputs\\broken.jpg",
      "code": "AURA-RAW-2001",
      "message": "AURA does not recognise this file as a photo it can develop. It has been set aside; the rest of the import continues."
    }
  ]
}
```

## `composition_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Coverage, keypoint awareness, flags, tilt telemetry and stored versions.

Native definition: `ui/src-tauri/src/main.rs:242`.

```rust
async fn composition_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<CompositionStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1074`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "composition_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "photos": 5,
    "scored": 0,
    "coverage": 0,
    "keypointAware": 0,
    "flagCounts": [
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0
    ],
    "flagNames": [
      "horizon_tilted",
      "intentional_tilt",
      "verticals_converging",
      "headroom_tight",
      "headroom_excessive",
      "head_cropped",
      "joint_cut",
      "limb_cut",
      "edge_intrusion",
      "off_balance",
      "centred",
      "cluttered_background",
      "bright_blob",
      "head_merge",
      "colour_competition",
      "no_geometry"
    ],
    "meanScore": 0,
    "meanAbsTilt": 0,
    "intentionalRatio": 0,
    "hinted": 0,
    "reviewed": 0,
    "violatingAtMost": 0,
    "unruledScenes": [],
    "modelVer": 0,
    "analysisVer": 0,
    "rulesVer": 0
  }
}
```

## `image_composition`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: One frame's complete judgement. Null means nobody has analysed it.

Native definition: `ui/src-tauri/src/main.rs:253`.

```rust
async fn image_composition( state: State<'_, AppState>, photo_id: String, ) -> IpcResult<Option<CompositionDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1078`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `flagged_composition`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: A review queue ordered from the lowest composition score upward.

Native definition: `ui/src-tauri/src/main.rs:264`.

```rust
async fn flagged_composition( state: State<'_, AppState>, input: FlaggedCompositionInput, ) -> IpcResult<Vec<String>>
```

Primary UI binding: `ui/src/ipc/client.ts:1082`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `dismiss_composition_flag`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Clear one violation and remember the photographer's disagreement.

Native definition: `ui/src-tauri/src/main.rs:275`.

```rust
async fn dismiss_composition_flag( state: State<'_, AppState>, input: DismissCompositionFlagInput, ) -> IpcResult<CompositionDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1086`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `analyse_composition`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Judge pending frames. Completed rows survive cancellation and the next run resumes.

Native definition: `ui/src-tauri/src/main.rs:286`.

```rust
async fn analyse_composition( state: State<'_, AppState>, input: AnalyseCompositionInput, ) -> IpcResult<CompositionPassDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1090`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "analyse_composition",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
    }
  },
  "ok": true,
  "ms": 766.0,
  "result": {
    "scored": 4,
    "failed": 1,
    "keypointSubjects": 0,
    "cut": 0,
    "intentionalTilts": 0,
    "horizons": 4,
    "meanAbsTilt": 11.26374,
    "flagCounts": [
      1,
      0,
      4,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      4
    ],
    "flagNames": [
      "horizon_tilted",
      "intentional_tilt",
      "verticals_converging",
      "headroom_tight",
      "headroom_excessive",
      "head_cropped",
      "joint_cut",
      "limb_cut",
      "edge_intrusion",
      "off_balance",
      "centred",
      "cluttered_background",
      "bright_blob",
      "head_merge",
      "colour_competition",
      "no_geometry"
    ],
    "meanScore": 0.97632015,
    "hinted": 0,
    "unruledScenes": [
      "unknown"
    ],
    "elapsedMs": 762,
    "cancelled": false
  }
}
```

## `cull_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Coverage, guarantee counts, overrides, mode and stored versions.

Native definition: `ui/src-tauri/src/main.rs:302`.

```rust
async fn cull_status(state: State<'_, AppState>, project_id: String) -> IpcResult<CullStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1108`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "cull_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 16.0,
  "result": {
    "photos": 5,
    "eligible": 0,
    "selected": 0,
    "coverage": 0,
    "emotionAware": 0,
    "compositionAware": 0,
    "grouped": 0,
    "covered": 0,
    "coveredWeak": 0,
    "missing": 0,
    "userKept": 0,
    "userRejected": 0,
    "mode": "balanced",
    "deterministicHash": "0000000000000000",
    "modelVer": 0,
    "analysisVer": 0,
    "calibrationVer": 0
  }
}
```

## `gallery`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: The stored gallery. Null means nobody has culled this wedding yet.

Native definition: `ui/src-tauri/src/main.rs:310`.

```rust
async fn gallery( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Option<SelectionDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1112`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `image_decision`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: What was decided about one photograph, in either direction, with reasons.

Native definition: `ui/src-tauri/src/main.rs:321`.

```rust
async fn image_decision( state: State<'_, AppState>, photo_id: String, ) -> IpcResult<Option<DecisionDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1116`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `cull_project`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 2; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Run or re-run the cull. Reads stored analysis; opens no image file.

Native definition: `ui/src-tauri/src/main.rs:332`.

```rust
async fn cull_project( state: State<'_, AppState>, input: CullProjectInput, ) -> IpcResult<CullPassDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1120`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "cull_project",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83",
      "target": 3
    }
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "photos": 5,
    "eligible": 4,
    "selected": 3,
    "vetoCounts": [
      0,
      0,
      0
    ],
    "vetoNames": [
      "veto_out_of_focus",
      "veto_exposure_lost",
      "veto_eyes_closed"
    ],
    "swaps": 0,
    "coverageAdded": 0,
    "diversityDropped": 0,
    "sizeAdded": 0,
    "sizeTrimmed": 0,
    "peaksRejected": 0,
    "coverageWeak": 0,
    "coverageMissing": 11,
    "unweightedScenes": [],
    "elapsedMs": 7
  }
}
```

## `resize_gallery`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 1; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Move the size slider. The result may exceed the requested target, because the coverage guard runs last and a guarantee outranks a slider.

Native definition: `ui/src-tauri/src/main.rs:343`.

```rust
async fn resize_gallery( state: State<'_, AppState>, input: ResizeGalleryInput, ) -> IpcResult<SelectionDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1127`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "resize_gallery",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83",
      "target": 2
    }
  },
  "ok": true,
  "ms": 16.0,
  "result": {
    "selected": [
      {
        "photoId": "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7",
        "momentId": null,
        "keepScore": 0.6843628,
        "confidence": 0.13254267,
        "reasons": [
          {
            "code": "only_candidate",
            "text": "the only frame there was",
            "weight": 0.6,
            "keep": true,
            "veto": false
          },
          {
            "code": "no_scene",
            "text": "AURA does not know what kind of photograph this is, so it was judged cautiously",
            "weight": 0.05,
            "keep": false,
            "veto": false
          },
          {
            "code": "no_moment",
            "text": "this photograph was not grouped with any others, so it was judged on its own",
            "weight": 0.05,
            "keep": false,
            "veto": false
          }
        ],
        "runnerUp": null,
        "coverageRole": null,
        "protected": false
      },
      {
        "photoId": "pht_01a150aa-0015-7bf0-869e-f360f41a1cd3",
        "momentId": null,
        "keepScore": 0.7006911,
        "confidence": 0.13254032,
        "reasons": [
          {
            "code": "only_candidate",
            "text": "the only frame there was",
            "weight": 0.6,
            "keep": true,
            "veto": false
          },
          {
            "code": "no_scene",
            "text": "AURA does not know what kind of photograph this is, so it was judged cautiously",
            "weight": 0.05,
            "keep": false,
            "veto": false
          },
          {
            "code": "no_moment",
            "text": "this photograph was not grouped with any others, so it was judged on its own",
            "weight": 0.05,
            "keep": false,
            "veto": false
          }
        ],
        "runnerUp": null,
        "coverageRole": null,
        "protected": false
      }
    ],
    "rejected": [
      {
        "photoId": "pht_01a150aa-0267-7f72-baef-6cff2b94d75e",
        "momentId": null,
        "keepScore": 0,
        "reasons": [
          {
            "code": "not_analysed",
            "text": "AURA has not checked this photograph yet, so it has not been considered - this is not a judgement about it",
            "weight": -0.1,
            "keep": false,
            "veto": false
          }
        ],
        "keptInstead": null,
        "wasPeak": false,
        "vetoed": false
      },
      {
        "photoId": "pht_01a150aa-03f4-78e3-ac32-6fd99b2b80c3",
        "momentId": null,
        "keepScore": 0.6182558,
        "reasons": [
          {
            "code": "chapter_full",
            "text": "this part of the day was already well covered by stronger frames",
            "weight": -0.5,
            "keep": false,
            "veto": false
          }
        ],
        "keptInstead": null,
        "wasPeak": false,
        "vetoed": false
      },
      {
        "photoId": "pht_01a150aa-0637-7952-94a9-e26308c1ffee",
        "momentId": null,
        "keepScore": 0.6520762,
        "reasons": [
          {
            "code": "chapter_full",
            "text": "this part of the day was already well covered by stronger frames",
            "weight": -0.5,
            "keep": false,
            "veto": false
          }
        ],
        "keptInstead": null,
        "wasPeak": false,
        "vetoed": false
      }
    ],
    "coverage": {
      "mustHaves": [
        {
          "rule": "first_look",
          "title": "The first look",
          "state": "missing",
          "satisfied": false
        },
        {
          "rule": "ceremony_entrance",
          "title": "Walking in",
          "state": "missing",
          "satisfied": false
        },
        {
          "rule": "vows",
          "title": "The vows",
          "state": "missing",
          "satisfied": false
        },
        {
          "rule": "rings",
          "title": "The rings",
          "state": "covered",
          "satisfied": true
        },
        {
          "rule": "kiss",
          "title": "The kiss",
          "state": "missing",
          "satisfied": false
        },
        {
          "rule": "family_formals",
          "title": "Family groups",
          "state": "missing",
          "satisfied": false
        },
        {
          "rule": "reception_entrance",
          "title": "Entering the reception",
          "state": "missing",
          "satisfied": false
        },
        {
          "rule": "cake",
          "title": "The cake",
          "state": "missing",
          "satisfied": false
        },
        {
          "rule": "first_dance",
          "title": "The first dance",
          "state": "missing",
          "satisfied": false
        },
        {
          "rule": "venue_establishing",
          "title": "The venue",
          "state": "missing",
          "satisfied": false
        },
        {
          "rule": "exit",
          "title": "The exit",
          "state": "missing",
          "satisfied": false
        },
        {
          "rule": "close_family",
          "title": "Close family",
          "state": "missing",
          "satisfied": false
        }
      ],
      "identityCoverage": [],
      "chapters": [
        {
          "chapter": "getting_ready",
          "title": "Getting Ready",
          "delivered": 0,
          "target": 0
        },
        {
          "chapter": "details",
          "title": "Details",
          "delivered": 0,
          "target": 0
        },
        {
          "chapter": "ceremony",
          "title": "Ceremony",
          "delivered": 0,
          "target": 0
        },
        {
          "chapter": "rituals",
          "title": "Rituals",
          "delivered": 0,
          "target": 0
        },
        {
          "chapter": "portraits",
          "title": "Portraits",
          "delivered": 0,
          "target": 0
        },
        {
          "chapter": "reception",
          "title": "Reception",
          "delivered": 0,
          "target": 0
        },
        {
          "chapter": "dance",
          "title": "Dance",
          "delivered": 0,
          "target": 0
        },
        {
          "chapter": "exit",
          "title": "Exit",
          "delivered": 0,
          "target": 0
        },
        {
          "chapter": "other",
          "title": "Other",
          "delivered": 2,
          "target": 2
        }
      ],
      "warnings": [
        "The first look: no candidates found. AURA cannot add what was not photographed.",
        "Walking in: no candidates found. AURA cannot add what was not photographed.",
        "The vows: no candidates found. AURA cannot add what was not photographed.",
        "The kiss: no candidates found. AURA cannot add what was not photographed.",
        "Family groups: no candidates found. AURA cannot add what was not photographed.",
        "Entering the reception: no candidates found. AURA cannot add what was not photographed.",
        "The cake: no candidates found. AURA cannot add what was not photographed.",
        "The first dance: no candidates found. AURA cannot add what was not photographed.",
        "The venue: no candidates found. AURA cannot add what was not photographed.",
        "The exit: no candidates found. AURA cannot add what was not photographed.",
        "AURA has not identified any close family in this wedding, so it could not guarantee that each of them is in the gallery.",
        "AURA worked out who the couple are from the photographs and nobody has confirmed it. If that is wrong, the rules about who must appear in the gallery were applied to the wrong people."
      ]
    },
    "targetCount": 2,
    "actualCount": 2,
    "mode": "balanced",
    "deterministicHash": "02ac04960ea7c649",
    "modelVer": 100,
    "analysisVer": 1,
    "calibrationVer": 0
  }
}
```

## `set_cull_mode`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Switch autonomy mode. Cannot drop a must-have, whichever mode is chosen.

Native definition: `ui/src-tauri/src/main.rs:354`.

```rust
async fn set_cull_mode( state: State<'_, AppState>, input: SetCullModeInput, ) -> IpcResult<SelectionDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1131`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `override_decision`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 3; refused harness calls: 3; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Keep or remove one photograph by hand, or withdraw an earlier choice. Unbeatable and re-applied onto every fresh selection. A removal can leave a guarantee short: the coverage report then degrades that rule and says so.

Native definition: `ui/src-tauri/src/main.rs:365`.

```rust
async fn override_decision( state: State<'_, AppState>, input: OverrideDecisionInput, ) -> IpcResult<DecisionDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1140`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "override_decision",
  "args": {
    "input": {
      "photoId": "pht_01a150aa-03f4-78e3-ac32-6fd99b2b80c3",
      "action": "clear"
    }
  },
  "ok": true,
  "ms": 16.0,
  "result": {
    "kept": false,
    "selected": null,
    "rejected": {
      "photoId": "pht_01a150aa-03f4-78e3-ac32-6fd99b2b80c3",
      "momentId": null,
      "keepScore": 0.6182558,
      "reasons": [
        {
          "code": "chapter_full",
          "text": "this part of the day was already well covered by stronger frames",
          "weight": -0.5,
          "keep": false,
          "veto": false
        }
      ],
      "keptInstead": null,
      "wasPeak": false,
      "vetoed": false
    }
  }
}
```

## `explain_image`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Everything the Explain panel draws for one photograph.

Native definition: `ui/src-tauri/src/main.rs:382`.

```rust
async fn explain_image(state: State<'_, AppState>, photo_id: String) -> IpcResult<ExplainPanelDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1158`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `decision_history`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Every decision ever recorded about one photograph, newest first.

Native definition: `ui/src-tauri/src/main.rs:390`.

```rust
async fn decision_history( state: State<'_, AppState>, photo_id: String, ) -> IpcResult<Vec<LedgerDecisionDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1162`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `decision_by_id`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: One decision by its id - what a support case quotes down a telephone.

Native definition: `ui/src-tauri/src/main.rs:401`.

```rust
async fn decision_by_id( state: State<'_, AppState>, decision_id: String, ) -> IpcResult<Option<LedgerDecisionDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1166`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `ledger_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Counts, coverage, calibration version and size for one wedding's ledger.

Native definition: `ui/src-tauri/src/main.rs:412`.

```rust
async fn ledger_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<LedgerStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1170`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "ledger_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "decisions": 0,
    "explained": 0,
    "explanationCoverage": 0,
    "current": 0,
    "superseded": 0,
    "byKind": [
      0,
      0,
      0,
      0,
      0,
      0
    ],
    "kindNames": [
      "cull",
      "edit",
      "retouch",
      "qc",
      "curate",
      "export"
    ],
    "byAutonomy": [
      0,
      0,
      0,
      0
    ],
    "autonomyNames": [
      "auto",
      "auto_zero_touch",
      "suggest",
      "require_review"
    ],
    "bySource": [
      0,
      0,
      0
    ],
    "sourceNames": [
      "local",
      "cloud",
      "user"
    ],
    "calibrated": 0,
    "calibrationVer": 0,
    "evidenced": 0,
    "reasons": 0,
    "bytes": 0
  }
}
```

## `review_queue`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The decisions waiting for a person, newest first.

Native definition: `ui/src-tauri/src/main.rs:423`.

```rust
async fn review_queue( state: State<'_, AppState>, input: ReviewQueueInput, ) -> IpcResult<Vec<LedgerDecisionDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1174`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `record_decisions`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record the stored gallery's decisions. Append-only, so running it twice records two rounds of decisions rather than overwriting the first.

Native definition: `ui/src-tauri/src/main.rs:434`.

```rust
async fn record_decisions( state: State<'_, AppState>, input: RecordDecisionsInput, ) -> IpcResult<RecordDecisionsDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1181`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `export_support_bundle`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: An anonymised slice of the ledger. No pixels, no names, no keys - every identifier is replaced by a handle before the file exists.

Native definition: `ui/src-tauri/src/main.rs:445`.

```rust
async fn export_support_bundle( state: State<'_, AppState>, input: ExportBundleInput, ) -> IpcResult<SupportBundleDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1188`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `compact_ledger`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Apply the compaction policy. Keeps the newest decision per subject and every decision the photographer made themselves.

Native definition: `ui/src-tauri/src/main.rs:456`.

```rust
async fn compact_ledger(state: State<'_, AppState>, project_id: String) -> IpcResult<u32>
```

Primary UI binding: `ui/src/ipc/client.ts:1193`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `tone_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: How much of a wedding has an exposure decision, and how much of it came from a face.

Native definition: `ui/src-tauri/src/main.rs:468`.

```rust
async fn tone_status(state: State<'_, AppState>, project_id: String) -> IpcResult<ToneStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1248`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "tone_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "photos": 5,
    "estimated": 0,
    "coverage": 0,
    "faceAnchored": 0,
    "skinConstrained": 0,
    "mixedLight": 0,
    "colouredLight": 0,
    "needsReview": 0,
    "userEdited": 0,
    "meanEv": 0,
    "meanCct": 0,
    "illuminantCounts": [
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0
    ],
    "illuminantNames": [
      "unknown",
      "daylight",
      "shade",
      "cloudy",
      "tungsten",
      "fluorescent",
      "led",
      "flash",
      "candle",
      "stage"
    ],
    "segmentsAnchored": 0,
    "segments": 0,
    "loci": 0,
    "untargetedScenes": [],
    "modelVer": 0,
    "analysisVer": 0,
    "targetsVer": 0
  }
}
```

## `image_tone`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: One photograph's decision, or `null` when nobody has estimated it.

Native definition: `ui/src-tauri/src/main.rs:476`.

```rust
async fn image_tone(state: State<'_, AppState>, photo_id: String) -> IpcResult<Option<ToneDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1252`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `tone_review_queue`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: The frames whose white balance is worth a look, weakest first. A queue, not a cull.

Native definition: `ui/src-tauri/src/main.rs:484`.

```rust
async fn tone_review_queue( state: State<'_, AppState>, input: ToneReviewInput, ) -> IpcResult<Vec<String>>
```

Primary UI binding: `ui/src/ipc/client.ts:1256`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `reference_frames`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: One chapter's anchors, best first. What phase 25 will normalise toward.

Native definition: `ui/src-tauri/src/main.rs:495`.

```rust
async fn reference_frames( state: State<'_, AppState>, input: ReferenceFramesInput, ) -> IpcResult<Vec<ReferenceFrameDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1260`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `accept_tone`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record that the photographer looked and agrees. Does not set `userEdited`.

Native definition: `ui/src-tauri/src/main.rs:506`.

```rust
async fn accept_tone(state: State<'_, AppState>, input: AcceptToneInput) -> IpcResult<ToneDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1264`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_tone_override`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record what they set instead, and write it into the recipe as a person.

Native definition: `ui/src-tauri/src/main.rs:514`.

```rust
async fn set_tone_override( state: State<'_, AppState>, input: SetToneOverrideInput, ) -> IpcResult<SetToneOverrideDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1268`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `estimate_tone`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Run the resumable pass, then carry what it decided into the recipes.

Native definition: `ui/src-tauri/src/main.rs:525`.

```rust
async fn estimate_tone( state: State<'_, AppState>, input: EstimateToneInput, ) -> IpcResult<TonePassDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1272`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `colour_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: How much of a wedding has a grade, and how much of the skin guarantee was checked.

Native definition: `ui/src-tauri/src/main.rs:776`.

```rust
async fn colour_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<ColourStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1286`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "colour_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 16.0,
  "result": {
    "photos": 5,
    "decided": 0,
    "coverage": 0,
    "skinMeasured": 0,
    "skinGuardTriggered": 0,
    "clipGuardResolved": 0,
    "subtletyCapped": 0,
    "needsReview": 0,
    "userEdited": 0,
    "meanContrast": 0,
    "meanShadowLift": 0,
    "meanSubtlety": 0,
    "worstSkinHueShift": 0,
    "guaranteeHeld": true,
    "untargetedScenes": [],
    "modelVer": 0,
    "analysisVer": 0,
    "intentVer": 0
  }
}
```

## `image_colour`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: One photograph's grade, or `null` when nobody has graded it.

Native definition: `ui/src-tauri/src/main.rs:787`.

```rust
async fn image_colour( state: State<'_, AppState>, photo_id: String, ) -> IpcResult<Option<ColourDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1290`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `colour_review_queue`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The frames whose grade is worth a look, weakest first. A queue, not a cull.

Native definition: `ui/src-tauri/src/main.rs:798`.

```rust
async fn colour_review_queue( state: State<'_, AppState>, input: ColourReviewInput, ) -> IpcResult<Vec<String>>
```

Primary UI binding: `ui/src/ipc/client.ts:1294`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `accept_colour`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record that the photographer looked and agrees. Does not set `userEdited`.

Native definition: `ui/src-tauri/src/main.rs:809`.

```rust
async fn accept_colour( state: State<'_, AppState>, input: AcceptColourInput, ) -> IpcResult<ColourDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1298`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_colour_override`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record what they set instead, and write it into the recipe as a person.

Native definition: `ui/src-tauri/src/main.rs:820`.

```rust
async fn set_colour_override( state: State<'_, AppState>, input: SetColourOverrideInput, ) -> IpcResult<SetColourOverrideDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1302`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `select_colour_variant`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Promote one stored alternative to the primary grade. Safe because every variant has already been through the clipping guard and the skin guard, so the promoted set is one somebody checked.

Native definition: `ui/src-tauri/src/main.rs:831`.

```rust
async fn select_colour_variant( state: State<'_, AppState>, input: SelectVariantInput, ) -> IpcResult<SetColourOverrideDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1311`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `estimate_colour`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Run the resumable pass, then carry what it decided into the recipes.

Native definition: `ui/src-tauri/src/main.rs:842`.

```rust
async fn estimate_colour( state: State<'_, AppState>, input: EstimateColourInput, ) -> IpcResult<ColourPassDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1315`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `look_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: What the match-a-look card shows: is a look selected, and over how much of the wedding.

Native definition: `ui/src-tauri/src/main.rs:557`.

```rust
async fn look_status(state: State<'_, AppState>, project_id: String) -> IpcResult<LookStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2263`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "look_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "profiles": 0,
    "selected": null,
    "selectedName": "",
    "selectedOrigin": "",
    "strength": 1,
    "appliable": 0,
    "photographs": 5,
    "baselineCoverage": 0,
    "networkTransportAvailable": false
  }
}
```

## `list_looks`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Every stored look, newest first.

Native definition: `ui/src-tauri/src/main.rs:565`.

```rust
async fn list_looks(state: State<'_, AppState>) -> IpcResult<Vec<LookProfileDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2266`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `parse_reference`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Read back what was typed into the address box. Called on every keystroke. It parses and **does not resolve**: there is no round trip to Instagram here and no tick to render, only the handle the text was understood as.

Native definition: `ui/src-tauri/src/main.rs:573`.

```rust
fn parse_reference(address: String) -> IpcResult<ReferenceOriginDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2275`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `look_buckets`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: One look's lighting buckets, as the matrix renders them. Pass the project to fill each row's measured `afterDe00`; without it a row says what the look asks for and stays `null` about what it did.

Native definition: `ui/src-tauri/src/main.rs:578`.

```rust
async fn look_buckets( state: State<'_, AppState>, input: LookBucketsInput, ) -> IpcResult<Vec<LookBucketDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2284`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `look_match_report`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: The last measured match on one project, or `null` when nothing has been measured.

Native definition: `ui/src-tauri/src/main.rs:589`.

```rust
async fn look_match_report( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Option<LookMatchDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2288`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `measure_look`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Measure a reference and store the look. Minutes rather than seconds on a real reference: it decodes every reference photograph and renders a sample of the wedding twice. The panel shows what it is doing.

Native definition: `ui/src-tauri/src/main.rs:604`.

```rust
async fn measure_look( state: State<'_, AppState>, input: MeasureLookInput, ) -> IpcResult<MeasureLookDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2297`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `select_look`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Point this project at a look, or pass `null` to go back to the baseline.

Native definition: `ui/src-tauri/src/main.rs:615`.

```rust
async fn select_look(state: State<'_, AppState>, input: SelectLookInput) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:2300`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_look_strength`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Apply a look at less than its measured strength. Never more.

Native definition: `ui/src-tauri/src/main.rs:623`.

```rust
async fn set_look_strength( state: State<'_, AppState>, input: SetLookStrengthInput, ) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:2304`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `rename_look`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Rename a look.

Native definition: `ui/src-tauri/src/main.rs:634`.

```rust
async fn rename_look(state: State<'_, AppState>, input: LookProfileInput) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:2307`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `forget_look`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Delete a look. A project that had it selected falls back to the baseline.

Native definition: `ui/src-tauri/src/main.rs:642`.

```rust
async fn forget_look(state: State<'_, AppState>, input: LookProfileInput) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:2310`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `style_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: What this project knows about style.

Native definition: `ui/src-tauri/src/main.rs:650`.

```rust
async fn style_status(state: State<'_, AppState>, project_id: String) -> IpcResult<StyleStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1331`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "style_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "profiles": 0,
    "active": null,
    "activeName": "",
    "activeVersion": 0,
    "trainedPairs": 0,
    "strength": 0,
    "overallDe00": 0,
    "chapterOverrides": [],
    "levelCounts": [],
    "bucketRatio": 0,
    "analysisVer": 0
  }
}
```

## `list_profiles`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Every profile, newest first.

Native definition: `ui/src-tauri/src/main.rs:658`.

```rust
async fn list_profiles(state: State<'_, AppState>) -> IpcResult<Vec<StyleProfileDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1335`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `profile_report`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: One profile's honest report, or `null` when nobody has trained it.

Native definition: `ui/src-tauri/src/main.rs:666`.

```rust
async fn profile_report( state: State<'_, AppState>, profile_id: String, ) -> IpcResult<Option<ProfileReportDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1339`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `profile_pairs`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The pairs behind one profile - accepted **and** rejected.

Native definition: `ui/src-tauri/src/main.rs:677`.

```rust
async fn profile_pairs( state: State<'_, AppState>, name: String, limit: Option<u32>, ) -> IpcResult<Vec<StylePairDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1343`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `scan_archive`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Look at what is in a folder, before anything is fitted. Opens nothing.

Native definition: `ui/src-tauri/src/main.rs:689`.

```rust
async fn scan_archive( state: State<'_, AppState>, input: ScanArchiveInput, ) -> IpcResult<ScanArchiveDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1347`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `train_profile`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Train a profile. The result is a **candidate**; adoption is a separate act.

Native definition: `ui/src-tauri/src/main.rs:700`.

```rust
async fn train_profile( state: State<'_, AppState>, input: TrainProfileInput, ) -> IpcResult<TrainProfileDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1351`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `adopt_profile`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Adopt one profile: it becomes what the product edits with.

Native definition: `ui/src-tauri/src/main.rs:718`.

```rust
async fn adopt_profile( state: State<'_, AppState>, input: AdoptProfileInput, ) -> IpcResult<StyleProfileDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1355`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `compare_profiles`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The side-by-side of the baseline, the adopted profile and a candidate. No pixels.

Native definition: `ui/src-tauri/src/main.rs:729`.

```rust
async fn compare_profiles( state: State<'_, AppState>, input: CompareProfilesInput, ) -> IpcResult<Vec<StyleComparisonDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1359`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `export_profile`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Write a signed, portable profile.

Native definition: `ui/src-tauri/src/main.rs:740`.

```rust
async fn export_profile( state: State<'_, AppState>, input: ExportProfileInput, ) -> IpcResult<ExportProfileDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1363`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `import_profile`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Read a signed profile bundle. A tampered one is refused with `AURA-ML-5076`.

Native definition: `ui/src-tauri/src/main.rs:751`.

```rust
async fn import_profile( state: State<'_, AppState>, input: ImportProfileInput, ) -> IpcResult<ImportProfileDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1367`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_project_profile`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Choose which profile a project, or one chapter of it, uses.

Native definition: `ui/src-tauri/src/main.rs:762`.

```rust
async fn set_project_profile( state: State<'_, AppState>, input: SetProjectProfileInput, ) -> IpcResult<StyleStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1371`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `local_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: How much of a wedding has a local light plan, and how much of it actually did anything.

Native definition: `ui/src-tauri/src/main.rs:957`.

```rust
async fn local_status(state: State<'_, AppState>, project_id: String) -> IpcResult<LocalStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1445`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "local_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "photos": 5,
    "planned": 0,
    "coverage": 0,
    "actedOn": 0,
    "maskCovered": 0,
    "opCounts": [
      0,
      0,
      0,
      0,
      0,
      0
    ],
    "opNames": [
      "face_light",
      "subject_enhance",
      "background_balance",
      "shine_control",
      "dodge_burn_low",
      "dodge_burn_mid"
    ],
    "gatedCounts": [
      0,
      0,
      0,
      0,
      0,
      0
    ],
    "gatedNames": [
      "face",
      "subject",
      "background",
      "skin",
      "hair",
      "sky"
    ],
    "meanBudgetUsed": 0,
    "shineReduced": 0,
    "meanShineEv": 0,
    "groupSolved": 0,
    "needsReview": 0,
    "userEdited": 0,
    "unpoliciedScenes": [],
    "modelVer": 0,
    "analysisVer": 0,
    "policyVer": 0,
    "shapingVer": 0
  }
}
```

## `image_local`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: One photograph's plan, or `null` when nobody has planned it.

Native definition: `ui/src-tauri/src/main.rs:965`.

```rust
async fn image_local( state: State<'_, AppState>, photo_id: String, ) -> IpcResult<Option<LocalPlanDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1449`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `local_review_queue`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The frames whose local work is worth a look, weakest first. A queue, not a cull.

Native definition: `ui/src-tauri/src/main.rs:976`.

```rust
async fn local_review_queue( state: State<'_, AppState>, input: LocalReviewInput, ) -> IpcResult<Vec<String>>
```

Primary UI binding: `ui/src/ipc/client.ts:1453`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `accept_local`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record that the photographer looked and agrees. Does not set `userEdited`.

Native definition: `ui/src-tauri/src/main.rs:987`.

```rust
async fn accept_local( state: State<'_, AppState>, input: AcceptLocalInput, ) -> IpcResult<LocalPlanDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1457`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_local_strength`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record one operation's own strength, and write it into the recipe as a person.

Native definition: `ui/src-tauri/src/main.rs:998`.

```rust
async fn set_local_strength( state: State<'_, AppState>, input: SetLocalStrengthInput, ) -> IpcResult<SetLocalStrengthDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1461`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `sculpt_local`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Run the resumable pass over the selected photographs, then carry what it decided into the recipes. `photoIds` is the normal path: invariant 3, and section 11's own budget is written about a thousand selected images rather than about a wedding.

Native definition: `ui/src-tauri/src/main.rs:1103`.

```rust
async fn sculpt_local( state: State<'_, AppState>, input: SculptLocalInput, ) -> IpcResult<LocalPassDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1471`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `retouch_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: How much of a wedding has a retouch plan, and how much of it actually did anything.

Native definition: `ui/src-tauri/src/main.rs:1012`.

```rust
async fn retouch_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<RetouchStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1488`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "retouch_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "photos": 5,
    "planned": 0,
    "coverage": 0,
    "actedOn": 0,
    "maskCovered": 0,
    "blemishesRemoved": 0,
    "anomaliesLeft": 0,
    "protectedCounts": [
      0,
      0,
      0,
      0,
      0,
      0
    ],
    "protectedKinds": [
      "mole",
      "freckle",
      "birthmark",
      "scar",
      "tattoo",
      "dimple"
    ],
    "textureResolved": 0,
    "textureWithdrawn": 0,
    "meanBandRatio": 0,
    "meanStrength": 0,
    "maxIdentitySpread": 0,
    "presetCounts": [
      0,
      0,
      0,
      0
    ],
    "presetNames": [
      "off",
      "light",
      "natural",
      "polished"
    ],
    "needsReview": 0,
    "userEdited": 0,
    "unpresetScenes": [],
    "modelVer": 0,
    "analysisVer": 0,
    "presetVer": 0
  }
}
```

## `image_retouch`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: One photograph's plan, or `null` when nobody has planned it.

Native definition: `ui/src-tauri/src/main.rs:1023`.

```rust
async fn image_retouch( state: State<'_, AppState>, photo_id: String, ) -> IpcResult<Option<RetouchPlanDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1492`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `protected_features`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: What this product will never remove from one person's face. The rectangles are **face-normalised** - origin between the eyes, x along the eye-to-eye line, unit the inter-ocular distance - which is what lets one row protect the same mole in four hundred photographs. Drawing one on a frame projects it through that frame's landmarks.

Native definition: `ui/src-tauri/src/main.rs:1034`.

```rust
async fn protected_features( state: State<'_, AppState>, project_id: String, identity_id: String, ) -> IpcResult<Vec<ProtectedFeatureDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1502`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `retouch_review_queue`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The frames whose retouch is worth a look, weakest first. A queue, not a cull.

Native definition: `ui/src-tauri/src/main.rs:1048`.

```rust
async fn retouch_review_queue( state: State<'_, AppState>, input: RetouchReviewInput, ) -> IpcResult<Vec<String>>
```

Primary UI binding: `ui/src/ipc/client.ts:1506`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `accept_retouch`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record that the photographer looked and agrees. Does not set `userEdited`.

Native definition: `ui/src-tauri/src/main.rs:1059`.

```rust
async fn accept_retouch( state: State<'_, AppState>, input: AcceptRetouchInput, ) -> IpcResult<RetouchPlanDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1510`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_retouch`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record the preset the photographer chose, and write it into the recipe as a person.

Native definition: `ui/src-tauri/src/main.rs:1070`.

```rust
async fn set_retouch( state: State<'_, AppState>, input: SetRetouchInput, ) -> IpcResult<SetRetouchDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1514`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_protection`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Protect or unprotect one feature. A tattoo cannot be unprotected. The refusal is `AURA-ML-5097` and it lives in the type, in the service and in a database trigger, because a promise enforced in one layer lasts until somebody writes a second caller.

Native definition: `ui/src-tauri/src/main.rs:1081`.

```rust
async fn set_protection( state: State<'_, AppState>, input: SetProtectionInput, ) -> IpcResult<Vec<ProtectedFeatureDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1524`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `retouch_pass`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Run the resumable retouch pass, then carry what it decided into the recipes.

Native definition: `ui/src-tauri/src/main.rs:1092`.

```rust
async fn retouch_pass( state: State<'_, AppState>, input: RetouchPassInput, ) -> IpcResult<RetouchPassDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1528`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `micro_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: How much of a wedding has a micro-retouch plan, and how much of it did anything.

Native definition: `ui/src-tauri/src/main.rs:1120`.

```rust
async fn micro_status(state: State<'_, AppState>, project_id: String) -> IpcResult<MicroStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1545`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "micro_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "photos": 5,
    "planned": 0,
    "coverage": 0,
    "actedOn": 0,
    "regionCovered": 0,
    "opCounts": [
      0,
      0,
      0,
      0,
      0
    ],
    "operators": [
      "flyaway",
      "teeth",
      "eyes",
      "clothing",
      "glare"
    ],
    "borrows": 0,
    "withdrawnCounts": [
      0,
      0,
      0
    ],
    "families": [
      "hair",
      "teeth",
      "eyes"
    ],
    "resolved": 0,
    "meanCatchlightRatio": 0,
    "meanHairEnergyRatio": 0,
    "needsReview": 0,
    "userEdited": 0,
    "unlistedScenes": [],
    "modelVer": 0,
    "analysisVer": 0,
    "matrixVer": 0
  }
}
```

## `image_micro`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: One photograph's plan, or `null` when nobody has planned it.

Native definition: `ui/src-tauri/src/main.rs:1128`.

```rust
async fn image_micro( state: State<'_, AppState>, photo_id: String, ) -> IpcResult<Option<MicroPlanDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1549`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `micro_composites`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Every frame in the project that carries pixels borrowed from another frame.

Native definition: `ui/src-tauri/src/main.rs:1139`.

```rust
async fn micro_composites( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Vec<MicroCompositeDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1553`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `micro_review_queue`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The frames whose small fixes are worth a look, weakest first. A queue, not a cull.

Native definition: `ui/src-tauri/src/main.rs:1150`.

```rust
async fn micro_review_queue( state: State<'_, AppState>, input: MicroReviewInput, ) -> IpcResult<Vec<String>>
```

Primary UI binding: `ui/src/ipc/client.ts:1557`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `micro_matrix`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Which of the five operations this studio has opted into, and their ceilings.

Native definition: `ui/src-tauri/src/main.rs:1161`.

```rust
async fn micro_matrix(state: State<'_, AppState>, project_id: String) -> IpcResult<MicroMatrixDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1561`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_micro_matrix`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Change the opt-in matrix. A studio may switch an operation off and may lower a ceiling. Nothing here can raise one: the contract owns every bound and the config file may only tighten it, which is what makes `docs/retouch-ethics.md` a promise about the product rather than about its defaults.

Native definition: `ui/src-tauri/src/main.rs:1169`.

```rust
async fn set_micro_matrix( state: State<'_, AppState>, input: SetMicroMatrixInput, ) -> IpcResult<MicroMatrixDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1571`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `accept_micro`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record that the photographer looked and agrees. Does not set `userEdited`.

Native definition: `ui/src-tauri/src/main.rs:1180`.

```rust
async fn accept_micro(state: State<'_, AppState>, input: AcceptMicroInput) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:1575`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `micro_pass`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Run the resumable pass, then carry what it decided into the recipes.

Native definition: `ui/src-tauri/src/main.rs:1188`.

```rust
async fn micro_pass(state: State<'_, AppState>, input: MicroPassInput) -> IpcResult<MicroPassDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1579`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `micro_reason_codes`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The panel's legend, assembled from the frozen enum rather than from a stored table.

Native definition: `ui/src-tauri/src/main.rs:1196`.

```rust
fn micro_reason_codes() -> IpcResult<Vec<MicroReasonDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1583`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `restore_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: How much of a wedding has a restoration plan, and how much of it did anything.

Native definition: `ui/src-tauri/src/main.rs:1204`.

```rust
async fn restore_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<RestoreStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1600`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "restore_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "photos": 5,
    "planned": 0,
    "coverage": 0,
    "actedOn": 0,
    "regionCovered": 0,
    "tiers": [
      0,
      0,
      0,
      0
    ],
    "tierNames": [
      "off",
      "light",
      "standard",
      "strong"
    ],
    "sharpened": 0,
    "sharpenRefusals": [],
    "facesRecovered": 0,
    "facesSkippedIdentity": 0,
    "worstIdentityDrift": 0,
    "meanTextureRetention": 1,
    "meanRinging": 0,
    "reduced": 0,
    "needsReview": 0,
    "userEdited": 0,
    "unmeasuredCameras": [],
    "unlistedScenes": [],
    "versions": [
      0,
      1,
      1
    ]
  }
}
```

## `image_restore`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: One photograph's plan, or `null` when nobody has planned it.

Native definition: `ui/src-tauri/src/main.rs:1215`.

```rust
async fn image_restore( state: State<'_, AppState>, photo_id: String, ) -> IpcResult<Option<RestorePlanDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1604`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `restore_identity_refusals`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Every face this product declined to recover, and how far it would have moved.

Native definition: `ui/src-tauri/src/main.rs:1226`.

```rust
async fn restore_identity_refusals( state: State<'_, AppState>, input: RestoreReviewInput, ) -> IpcResult<Vec<RestoreIdentityRefusalDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1608`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `restore_review_queue`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The frames whose repairs are worth a look, weakest first. A queue, not a cull.

Native definition: `ui/src-tauri/src/main.rs:1237`.

```rust
async fn restore_review_queue( state: State<'_, AppState>, input: RestoreReviewInput, ) -> IpcResult<Vec<String>>
```

Primary UI binding: `ui/src/ipc/client.ts:1612`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `accept_restore`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record that the photographer looked and agrees. Does not set `userEdited`.

Native definition: `ui/src-tauri/src/main.rs:1248`.

```rust
async fn accept_restore(state: State<'_, AppState>, input: AcceptRestoreInput) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:1616`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_restore_override`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record the photographer's own tier or amount, and write it into the recipe as a person.

Native definition: `ui/src-tauri/src/main.rs:1256`.

```rust
async fn set_restore_override( state: State<'_, AppState>, input: SetRestoreOverrideInput, ) -> IpcResult<RestorePlanDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1620`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `restore_pass`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Run the resumable pass, then carry what it decided into the recipes.

Native definition: `ui/src-tauri/src/main.rs:1267`.

```rust
async fn restore_pass( state: State<'_, AppState>, input: RestorePassInput, ) -> IpcResult<RestorePassDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1624`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `restore_reason_codes`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The panel's legend, assembled from the frozen enum rather than from a stored table.

Native definition: `ui/src-tauri/src/main.rs:1278`.

```rust
fn restore_reason_codes() -> IpcResult<Vec<RestoreReasonDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1628`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `geometry_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: How much of a wedding is planned, and what share of it was delivered exactly as shot.

Native definition: `ui/src-tauri/src/main.rs:1285`.

```rust
async fn geometry_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<GeometryStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1645`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "geometry_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 234.0,
  "result": {
    "photos": 5,
    "planned": 0,
    "coverage": 0,
    "keptOriginal": 0,
    "profileCovered": 0,
    "levelled": 0,
    "meanRotateDeg": 0,
    "keystoned": 0,
    "variantCounts": [
      0,
      0,
      0,
      0,
      0
    ],
    "variantNames": [
      "As shot",
      "Delivered",
      "Album",
      "Social",
      "Wide"
    ],
    "refusedCounts": [
      0,
      0,
      0,
      0
    ],
    "refusedNames": [
      "crop_cuts_face",
      "crop_cuts_hands",
      "crop_too_small",
      "crop_loses_content"
    ],
    "missingProfiles": [],
    "unpoliciedScenes": [],
    "needsReview": 0,
    "userEdited": 0,
    "profilesSynthetic": true,
    "profilesKnown": 8,
    "profileVer": 1,
    "analysisVer": 1,
    "rulesVer": 1
  }
}
```

## `image_geometry`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: One photograph's plan, or `null` when nobody has planned it. The aspect variants an album or a feed needs are `crops`, on this plan. They are rectangles in one recipe rather than files on a disk, which is what "without duplicating files" means.

Native definition: `ui/src-tauri/src/main.rs:1296`.

```rust
async fn image_geometry( state: State<'_, AppState>, photo_id: String, ) -> IpcResult<Option<GeometryPlanDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1654`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `geometry_review_queue`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The frames whose framing is worth a look, weakest first. A queue, not a cull.

Native definition: `ui/src-tauri/src/main.rs:1307`.

```rust
async fn geometry_review_queue( state: State<'_, AppState>, input: GeometryReviewInput, ) -> IpcResult<Vec<String>>
```

Primary UI binding: `ui/src/ipc/client.ts:1658`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `accept_geometry`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record that the photographer looked and agrees. Does not set `userEdited`.

Native definition: `ui/src-tauri/src/main.rs:1318`.

```rust
async fn accept_geometry( state: State<'_, AppState>, input: AcceptGeometryInput, ) -> IpcResult<GeometryPlanDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1662`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `cleanup_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: How much of a wedding has been examined, and what came of it.

Native definition: `ui/src-tauri/src/main.rs:2363`.

```rust
async fn cleanup_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<CleanupStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1700`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "cleanup_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "photos": 5,
    "examined": 0,
    "coverage": 0,
    "withProposals": 0,
    "applied": 0,
    "blocked": [
      0,
      0,
      0,
      0,
      0
    ],
    "checkNames": [
      "size_cap",
      "denylist",
      "identity_protect",
      "structure_span",
      "confidence"
    ],
    "borrowed": 0,
    "filled": 0,
    "inpainted": 0,
    "reverted": 0,
    "maskCovered": 0,
    "detectorTrained": false,
    "inpaintAvailable": false
  }
}
```

## `image_cleanup`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: One photograph's proposals, strongest first.

Native definition: `ui/src-tauri/src/main.rs:2374`.

```rust
async fn image_cleanup( state: State<'_, AppState>, photo_id: String, ) -> IpcResult<Vec<CleanupProposalDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1704`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `cleanup_blocked`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Every candidate the safety engine refused on one photograph. A separate call because the refused set is usually larger than the proposed one. It is on the surface at all because teaching a photographer what AURA will never do is most of the trust this feature needs.

Native definition: `ui/src-tauri/src/main.rs:2385`.

```rust
async fn cleanup_blocked( state: State<'_, AppState>, photo_id: String, ) -> IpcResult<Vec<CleanupBlockedDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1714`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `cleanup_disclosures`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Everything removed from this project, for the delivery report.

Native definition: `ui/src-tauri/src/main.rs:2396`.

```rust
async fn cleanup_disclosures( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Vec<CleanupDisclosureDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1718`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `cleanup_pass`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Run the resumable pass. Killing it costs nothing; the work remaining is a query.

Native definition: `ui/src-tauri/src/main.rs:2407`.

```rust
async fn cleanup_pass( state: State<'_, AppState>, input: CleanupPassInput, ) -> IpcResult<CleanupPassDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1722`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `decide_cleanup`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Accept or reject one proposal. Accepting does not apply it.

Native definition: `ui/src-tauri/src/main.rs:2418`.

```rust
async fn decide_cleanup(state: State<'_, AppState>, input: DecideCleanupInput) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:1726`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `disable_cleanup`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Leave one photograph alone entirely. Excluded from every later pass.

Native definition: `ui/src-tauri/src/main.rs:2426`.

```rust
async fn disable_cleanup(state: State<'_, AppState>, input: DisableCleanupInput) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:1730`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `manual_remove`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Remove one region the photographer drew. Still runs all five safety checks.

Native definition: `ui/src-tauri/src/main.rs:2434`.

```rust
async fn manual_remove( state: State<'_, AppState>, input: ManualRemoveInput, ) -> IpcResult<ManualRemoveDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1734`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `cleanup_reason_codes`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The panel's legend, assembled from the frozen enum rather than from a stored table.

Native definition: `ui/src-tauri/src/main.rs:2445`.

```rust
async fn cleanup_reason_codes(state: State<'_, AppState>) -> IpcResult<Vec<CleanupReasonDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1738`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `gallery_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: What the Consistency panel's project header shows. Two denominators; read both.

Native definition: `ui/src-tauri/src/main.rs:2693`.

```rust
async fn gallery_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<GalleryStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1756`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "gallery_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 15.0,
  "result": {
    "photos": 5,
    "normalised": 0,
    "coverage": 0,
    "nodes": 0,
    "anchoredNodes": 0,
    "splitNodes": 0,
    "pinnedAnchors": 0,
    "bounded": 0,
    "moodPreserved": 0,
    "userEdited": 0,
    "outliers": 0,
    "skinTargeted": 0,
    "identities": 0,
    "spreadBeforeCct": 0,
    "spreadAfterCct": 0,
    "spreadBeforeEv": 0,
    "spreadAfterEv": 0,
    "worstSkinSpread": 0,
    "untargetedScenes": [],
    "skinFieldAvailable": false,
    "policyVer": 0
  }
}
```

## `gallery_nodes`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: The node tree, in capture order of each node's first frame.

Native definition: `ui/src-tauri/src/main.rs:2704`.

```rust
async fn gallery_nodes( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Vec<SceneNodeDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1760`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `gallery_node_strip`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: One node's deltas, in capture order. What a timeline strip draws. A separate call from `galleryNodes` because a wedding has forty nodes and four thousand frames, and a header that pulled every delta would pull the whole gallery to draw a summary.

Native definition: `ui/src-tauri/src/main.rs:2715`.

```rust
async fn gallery_node_strip( state: State<'_, AppState>, node_id: String, ) -> IpcResult<Vec<GalleryDeltaDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1769`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `image_gallery`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: One photograph's delta. `null` is a gap in coverage, never a zero delta.

Native definition: `ui/src-tauri/src/main.rs:2726`.

```rust
async fn image_gallery( state: State<'_, AppState>, photo_id: String, ) -> IpcResult<Option<GalleryDeltaDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1773`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `gallery_outliers`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Every frame still out of line, worst first. What phase 27's QC queue reads.

Native definition: `ui/src-tauri/src/main.rs:2737`.

```rust
async fn gallery_outliers( state: State<'_, AppState>, project_id: String, limit: usize, ) -> IpcResult<Vec<GalleryOutlierDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1777`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `gallery_pass`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Run the consistency pass. Runs to completion and returns what it did rather than a job id: a node half-solved against one target and half against another has a target that describes neither, so there is no partial state to poll that would not be a lie about what the catalog holds.

Native definition: `ui/src-tauri/src/main.rs:2751`.

```rust
async fn gallery_pass( state: State<'_, AppState>, input: GalleryPassInput, ) -> IpcResult<GalleryPassDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1787`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "gallery_pass",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
    }
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "nodes": 0,
    "anchored": 0,
    "split": 0,
    "normalised": 0,
    "outliers": 0,
    "skinTargets": 0,
    "spreadBeforeCct": 0,
    "spreadAfterCct": 0,
    "spreadBeforeEv": 0,
    "spreadAfterEv": 0,
    "decisionsKept": 0,
    "cancelled": false,
    "elapsedMs": 0
  }
}
```

## `pin_gallery_anchor`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Pin or reject one anchor. Both survive every later pass.

Native definition: `ui/src-tauri/src/main.rs:2762`.

```rust
async fn pin_gallery_anchor(state: State<'_, AppState>, input: PinAnchorInput) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:1791`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_gallery_override`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record what the photographer set instead, on one frame. Records the disagreement; it does not move a pixel. A value outside its bound is refused rather than clamped.

Native definition: `ui/src-tauri/src/main.rs:2770`.

```rust
async fn set_gallery_override( state: State<'_, AppState>, input: GalleryOverrideInput, ) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:1800`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `disable_gallery`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Leave one photograph out of the gallery match entirely.

Native definition: `ui/src-tauri/src/main.rs:2781`.

```rust
async fn disable_gallery(state: State<'_, AppState>, input: DisableGalleryInput) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:1804`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `gallery_reason_codes`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The panel's legend, assembled from the frozen enum rather than from a stored table.

Native definition: `ui/src-tauri/src/main.rs:2789`.

```rust
async fn gallery_reason_codes(state: State<'_, AppState>) -> IpcResult<Vec<GalleryReasonDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1808`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `camera_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: The project header: how many cameras, on what evidence, and what is left over.

Native definition: `ui/src-tauri/src/main.rs:2458`.

```rust
async fn camera_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<CameraStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1836`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "camera_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "photos": 5,
    "matched": 0,
    "coverage": 0,
    "cameras": 0,
    "fingerprinted": 0,
    "solvedFromPairs": 0,
    "blended": 0,
    "baselineOnly": 0,
    "pairs": 0,
    "pairsRejected": 0,
    "heldoutPairs": 0,
    "flashSeparated": 0,
    "shootersMeasured": 0,
    "shootersCapped": 0,
    "disabled": 0,
    "userEdited": 0,
    "skinDe00Before": 0,
    "skinDe00After": 0,
    "worstSkinDe00": 0,
    "referenceId": null,
    "referenceSource": "primary_shooter",
    "unknownBrands": [],
    "baselinesMeasured": false,
    "skinFieldAvailable": false,
    "policyVer": 1
  }
}
```

## `camera_transforms`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Every body's correction, by body and then by flash state.

Native definition: `ui/src-tauri/src/main.rs:2469`.

```rust
async fn camera_transforms( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Vec<CameraTransformDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1840`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `camera_fingerprints`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Every body's measured colour response, by body and then by flash state.

Native definition: `ui/src-tauri/src/main.rs:2480`.

```rust
async fn camera_fingerprints( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Vec<CameraFingerprintDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1844`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `camera_reports`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: The per-camera report, worst evidence first. What section 13's third acceptance criterion asks for: what was corrected, and on what evidence, in sentences a photographer reads rather than numbers they interpret.

Native definition: `ui/src-tauri/src/main.rs:2491`.

```rust
async fn camera_reports( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Vec<CameraReportDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1853`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `camera_pairs`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The matched pairs behind one body's correction, best first. **Rejected pairs come back too.** "Both cameras shot the whole ceremony and AURA still used a brand baseline" is answered by a list of candidates whose backgrounds disagreed, and by nothing else.

Native definition: `ui/src-tauri/src/main.rs:2502`.

```rust
async fn camera_pairs( state: State<'_, AppState>, project_id: String, camera_id: String, limit: usize, ) -> IpcResult<Vec<MatchedPairDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1863`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `camera_shooter_bias`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Every measured exposure habit, per photographer and per kind of photograph.

Native definition: `ui/src-tauri/src/main.rs:2517`.

```rust
async fn camera_shooter_bias( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Vec<ShooterBiasDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1867`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `camera_pass`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Match a project's cameras to each other. Runs to completion rather than returning a job id: a project half solved against one reference and half against another has been matched to nothing.

Native definition: `ui/src-tauri/src/main.rs:2528`.

```rust
async fn camera_pass( state: State<'_, AppState>, input: CameraPassInput, ) -> IpcResult<CameraPassDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1876`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "camera_pass",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
    }
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "cameras": 1,
    "referenceId": "",
    "referenceSource": "primary_shooter",
    "pairs": 0,
    "pairsRejected": 0,
    "heldoutPairs": 0,
    "solved": 0,
    "blended": 0,
    "baselineOnly": 0,
    "heldoutFailures": 0,
    "distanceBefore": 0,
    "distanceAfter": 0,
    "signatureBefore": 0,
    "signatureAfter": 0,
    "worstSkinDe00": 0,
    "shootersMeasured": 0,
    "shootersCapped": 0,
    "summary": "One camera shot this wedding, so there is nothing to match it to."
  }
}
```

## `set_camera_reference`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Choose the body everything else is matched to, and re-solve against it. Durable.

Native definition: `ui/src-tauri/src/main.rs:2539`.

```rust
async fn set_camera_reference( state: State<'_, AppState>, input: SetCameraReferenceInput, ) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:1880`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `disable_camera`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Leave one camera out of matching entirely. Both flash states move together.

Native definition: `ui/src-tauri/src/main.rs:2550`.

```rust
async fn disable_camera(state: State<'_, AppState>, input: DisableCameraInput) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:1884`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_camera_override`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record what the photographer set instead, for one body in one flash state. Records the disagreement; it does not move a pixel. A value outside its bound is refused rather than clamped.

Native definition: `ui/src-tauri/src/main.rs:2558`.

```rust
async fn set_camera_override( state: State<'_, AppState>, input: CameraOverrideInput, ) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:1893`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `camera_reason_codes`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The panel's legend, assembled from the frozen enum rather than from a stored table.

Native definition: `ui/src-tauri/src/main.rs:2569`.

```rust
async fn camera_reason_codes() -> IpcResult<Vec<CameraReasonDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1897`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `qc_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: The project header: what was checked, what could not be, and what is outstanding.

Native definition: `ui/src-tauri/src/main.rs:2582`.

```rust
async fn qc_status(state: State<'_, AppState>, project_id: String) -> IpcResult<QcStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1924`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "qc_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "selected": 0,
    "checked": 0,
    "coverage": 0,
    "inspections": 0,
    "inspectionsSkipped": 0,
    "completeness": 0,
    "open": 0,
    "accepted": 0,
    "dismissed": 0,
    "falseTicketRate": 0,
    "replaced": 0,
    "rounds": 0,
    "plannerCalls": 0,
    "byCategory": [
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0
    ],
    "byStatus": [
      0,
      0,
      0,
      0,
      0,
      0
    ],
    "bytes": 0,
    "thresholdsVer": 0,
    "analysisVer": 0,
    "detectorTrained": false
  }
}
```

## `qc_report`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: What the most recent pass did, or `null` when none has run.

Native definition: `ui/src-tauri/src/main.rs:2590`.

```rust
async fn qc_report( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Option<QcReportDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1928`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `qc_report_markdown`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The same report as Markdown, for a studio's records. Rendered in Rust rather than here, so the archived report and the queue a photographer reads say the same thing.

Native definition: `ui/src-tauri/src/main.rs:2601`.

```rust
async fn qc_report_markdown( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Option<String>>
```

Primary UI binding: `ui/src/ipc/client.ts:1937`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "qc_report_markdown",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": "# Quality control report\n\n## What was checked\n\n- Photographs in the delivered gallery inspected: **0**\n- Inspections that ran: **1**\n\n## What was found\n\n| Check | Found | Fixed | Needs you | Not checked |\n|---|---:|---:|---:|---:|\n\n**0 found, 0 corrected and verified, 0 put back, 0 for you to look at.**\n\n## About this run\n\n- Took 0 ms\n- A second opinion was not used\n- Thresholds version 1, analysis version 1\n\nAURA checks finished photographs against the rest of the wedding they belong to. It measures rather than guesses: every line above is a number against a threshold, and `docs/how-qc-works.md` says what each check measures and why.\n"
}
```

## `qc_queue`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The escalation queue, worst first. Ordered by severity as a **ratio** rather than by raw deviation: 0.4 dE00 over a 0.2 ceiling and 400 K over a 200 K one are the same amount of wrong, and sorting on the raw number would put every colour-temperature finding above every skin finding for ever.

Native definition: `ui/src-tauri/src/main.rs:2612`.

```rust
async fn qc_queue( state: State<'_, AppState>, project_id: String, category: Option<String>, limit: usize, ) -> IpcResult<Vec<QcTicketDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1947`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `qc_queue_grouped`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: The same queue, grouped by category, with the groups ordered by their worst member. Which is how a photographer works: eleven soft frames are one decision, not eleven.

Native definition: `ui/src-tauri/src/main.rs:2627`.

```rust
async fn qc_queue_grouped( state: State<'_, AppState>, project_id: String, limit: usize, ) -> IpcResult<Vec<QcGroupDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1955`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `qc_tickets`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Every finding on one photograph, worst first.

Native definition: `ui/src-tauri/src/main.rs:2641`.

```rust
async fn qc_tickets(state: State<'_, AppState>, image_id: String) -> IpcResult<Vec<QcTicketDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1959`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `qc_rounds`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: What was tried on one finding, and whether it worked. `realisedShare` is the number the loop decided on: a remedy that delivered less than half of what it promised was put back, and the row says so rather than leaving a reader to infer it from two deviations.

Native definition: `ui/src-tauri/src/main.rs:2649`.

```rust
async fn qc_rounds( state: State<'_, AppState>, project_id: String, ticket_id: String, ) -> IpcResult<Vec<QcRoundDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1969`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `qc_run`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Inspect the delivered gallery, and remediate when asked to. `remediate: false` changes nothing, which is what makes it the thing to run before a delivery. Runs to completion rather than returning a job id: a gallery half inspected under one thresholds table and half under another has been checked against nothing.

Native definition: `ui/src-tauri/src/main.rs:2661`.

```rust
async fn qc_run(state: State<'_, AppState>, input: QcPassInput) -> IpcResult<QcReportDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1979`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "qc_run",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83",
      "remediate": false
    }
  },
  "ok": true,
  "ms": 15.0,
  "result": {
    "images": 0,
    "imagesUnreached": 0,
    "complete": true,
    "checksRun": 1,
    "skipped": 0,
    "byCategory": [
      {
        "category": "consistency",
        "found": 0,
        "fixed": 0,
        "escalated": 0,
        "skipped": 0
      },
      {
        "category": "skin",
        "found": 0,
        "fixed": 0,
        "escalated": 0,
        "skipped": 0
      },
      {
        "category": "exposure",
        "found": 0,
        "fixed": 0,
        "escalated": 0,
        "skipped": 0
      },
      {
        "category": "sharpness",
        "found": 0,
        "fixed": 0,
        "escalated": 0,
        "skipped": 0
      },
      {
        "category": "retouch",
        "found": 0,
        "fixed": 0,
        "escalated": 0,
        "skipped": 0
      },
      {
        "category": "mask",
        "found": 0,
        "fixed": 0,
        "escalated": 0,
        "skipped": 0
      },
      {
        "category": "crop",
        "found": 0,
        "fixed": 0,
        "escalated": 0,
        "skipped": 0
      },
      {
        "category": "cleanup",
        "found": 0,
        "fixed": 0,
        "escalated": 0,
        "skipped": 0
      },
      {
        "category": "duplicate",
        "found": 0,
        "fixed": 0,
        "escalated": 0,
        "skipped": 0
      },
      {
        "category": "coverage",
        "found": 0,
        "fixed": 0,
        "escalated": 0,
        "skipped": 0
      }
    ],
    "found": 0,
    "fixed": 0,
    "reverted": 0,
    "escalated": 0,
    "replacements": [],
    "plannerCalls": 0,
    "cloudUsed": false,
    "durationMs": 0,
    "thresholdsVer": 1,
    "analysisVer": 1
  }
}
```

## `qc_decide`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record what a photographer decided about one finding. `applyRemedy` overrules a review requirement upward - they have looked. Nothing here overrules it downward.

Native definition: `ui/src-tauri/src/main.rs:2669`.

```rust
async fn qc_decide( state: State<'_, AppState>, project_id: String, input: QcDecideInput, ) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:1988`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `qc_decide_bulk`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record the same verdict on many findings, and authorise nothing. Returns how many rows moved. See this module's header for why there is no bulk `applyRemedy`.

Native definition: `ui/src-tauri/src/main.rs:2681`.

```rust
async fn qc_decide_bulk(state: State<'_, AppState>, input: QcDecideBulkInput) -> IpcResult<u32>
```

Primary UI binding: `ui/src/ipc/client.ts:1996`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_framing`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record the framing the photographer chose, and write it into the recipe as a person. A person may crop one photograph of their own through a face, because it is their photograph and they are looking at it. There is no field anywhere on this surface that says cutting faces is acceptable *in general* - that is the setting which would crop the next four hundred frames through people.

Native definition: `ui/src-tauri/src/main.rs:1329`.

```rust
async fn set_framing( state: State<'_, AppState>, input: SetFramingInput, ) -> IpcResult<SetFramingDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1673`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `plan_geometry`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Run the resumable pass, then carry what it decided into the recipes.

Native definition: `ui/src-tauri/src/main.rs:1340`.

```rust
async fn plan_geometry( state: State<'_, AppState>, input: PlanGeometryInput, ) -> IpcResult<GeometryPassDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1677`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `analyse_integrity`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Check every frame that has no current verdict. Resumable and cancellable.

Native definition: `ui/src-tauri/src/main.rs:1799`.

```rust
async fn analyse_integrity( state: State<'_, AppState>, input: AnalyseIntegrityInput, ) -> IpcResult<IntegrityPassDto>
```

Primary UI binding: `ui/src/ipc/client.ts:985`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "analyse_integrity",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
    }
  },
  "ok": true,
  "ms": 2297.0,
  "result": {
    "scored": 4,
    "failed": 1,
    "faces": 0,
    "closed": 0,
    "closedOk": 0,
    "meanScore": 0.7537012,
    "uncalibrated": [],
    "elapsedMs": 2291,
    "cancelled": false
  }
}
```

## `build_index`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:1694`.

```rust
async fn build_index(state: State<'_, AppState>, project_id: String) -> IpcResult<IndexStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:771`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "build_index",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "vectors": 4,
    "photos": 5,
    "coverage": 0.8,
    "filterable": 0,
    "modelVer": 100,
    "staleModelVersions": [],
    "buildMs": 0,
    "fromSnapshot": false
  }
}
```

## `cancel_previews`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:2183`.

```rust
async fn cancel_previews( state: State<'_, AppState>, project_id: String, photo_ids: Vec<String>, ) -> IpcResult<i64>
```

Primary UI binding: `ui/src/ipc/client.ts:669`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `ai_setup_status`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: What the first-run screen and the AI panel both need to know.

Native definition: `ui/src-tauri/src/main.rs:1378`.

```rust
async fn ai_setup_status(state: State<'_, AppState>) -> IpcResult<AiSetupStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:729`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `check_ai_key`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The one command that carries the key, and it carries it one way only. There is deliberately no `getAiKey`.

Native definition: `ui/src-tauri/src/main.rs:1406`.

```rust
async fn check_ai_key(state: State<'_, AppState>) -> IpcResult<KeyCheckDto>
```

Primary UI binding: `ui/src/ipc/client.ts:719`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `classify_scenes`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Every scene's tolerances, with the sentence explaining them. The rationale is on the wire because "why is my dance floor being judged this way" is a question a photographer asks, and invariant 2 says it has to have an answer.

Native definition: `ui/src-tauri/src/main.rs:2255`.

```rust
async fn classify_scenes( state: State<'_, AppState>, input: ClassifyScenesInput, ) -> IpcResult<StoryStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:864`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "classify_scenes",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
    }
  },
  "ok": true,
  "ms": 47.0,
  "result": {
    "photos": 5,
    "classified": 4,
    "coverage": 0.8,
    "chapters": 0,
    "needsReview": 0,
    "locked": 0,
    "penalty": 0,
    "gapsOnly": false,
    "sceneVer": 100,
    "taxonomyVer": 1,
    "ritualsKnown": 48
  }
}
```

## `clear_ai_key`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The one command that carries the key, and it carries it one way only. There is deliberately no `getAiKey`.

Native definition: `ui/src-tauri/src/main.rs:1414`.

```rust
async fn clear_ai_key(state: State<'_, AppState>, provider: String) -> IpcResult<CloudStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:717`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `cloud_cache_stats`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Answer the first-run screen by declining it. Records no provider.

Native definition: `ui/src-tauri/src/main.rs:1422`.

```rust
async fn cloud_cache_stats(state: State<'_, AppState>) -> IpcResult<CloudCacheStatsDto>
```

Primary UI binding: `ui/src/ipc/client.ts:752`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `cloud_calls`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Answer the first-run screen by declining it. Records no provider.

Native definition: `ui/src-tauri/src/main.rs:1430`.

```rust
async fn cloud_calls( state: State<'_, AppState>, project_id: String, limit: u32, ) -> IpcResult<Vec<CloudCallDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:749`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `cloud_spend`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Answer the first-run screen by declining it. Records no provider.

Native definition: `ui/src-tauri/src/main.rs:1442`.

```rust
async fn cloud_spend(state: State<'_, AppState>, project_id: String) -> IpcResult<CloudSpendDto>
```

Primary UI binding: `ui/src/ipc/client.ts:746`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `cloud_status`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Native definition: `ui/src-tauri/src/main.rs:1478`.

```rust
async fn cloud_status(state: State<'_, AppState>) -> IpcResult<CloudStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:707`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `internal_ai_status`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Narrow an unknown rejection into the typed error the backend promises.

Native definition: `ui/src-tauri/src/main.rs:1450`.

```rust
async fn internal_ai_status( state: State<'_, AppState>, refresh: Option<bool>, ) -> IpcResult<aura_app::internal_ai::Status>
```

Primary UI binding: `ui/src/ipc/client.ts:631`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `internal_ai_feedback`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Narrow an unknown rejection into the typed error the backend promises.

Native definition: `ui/src-tauri/src/main.rs:1463`.

```rust
async fn internal_ai_feedback( state: State<'_, AppState>, project_id: String, call_id: String, accepted: bool, ) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:632`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `develop_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: How much of a wedding has an edit. The denominator is every photograph.

Native definition: `ui/src-tauri/src/main.rs:1531`.

```rust
async fn develop_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<DevelopStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1235`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "develop_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "images": 5,
    "withRecipe": 2,
    "fromAi": 0,
    "fromUser": 2,
    "touchedByHand": 2,
    "sidecarBehind": 2
  }
}
```

## `dismiss_flag`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: "This mark is wrong." Clears one flag and records that the photographer said so; a re-analysis re-applies the dismissal rather than reverting it.

Native definition: `ui/src-tauri/src/main.rs:1810`.

```rust
async fn dismiss_flag( state: State<'_, AppState>, input: DismissFlagInput, ) -> IpcResult<IntegrityDto>
```

Primary UI binding: `ui/src/ipc/client.ts:981`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `edit_mask`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Apply a composition and keep the result as the photographer's own. Sets `userEdited`.

Native definition: `ui/src-tauri/src/main.rs:1863`.

```rust
async fn edit_mask(state: State<'_, AppState>, input: EditMaskInput) -> IpcResult<MaskDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1411`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `embed_project`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:1702`.

```rust
async fn embed_project( state: State<'_, AppState>, input: EmbedProjectInput, ) -> IpcResult<EmbedProgressDto>
```

Primary UI binding: `ui/src/ipc/client.ts:774`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "embed_project",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
    }
  },
  "ok": true,
  "ms": 156.0,
  "result": {
    "embedded": 4,
    "failed": 1,
    "remaining": 1,
    "elapsedMs": 139,
    "batches": 1,
    "cancelled": false
  }
}
```

## `emotion_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The panel header: coverage, face-awareness, peaks, links and versions.

Native definition: `ui/src-tauri/src/main.rs:1608`.

```rust
async fn emotion_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<EmotionStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1005`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "emotion_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 16.0,
  "result": {
    "photos": 5,
    "scored": 0,
    "coverage": 0,
    "faceAware": 0,
    "moments": 0,
    "peaked": 0,
    "peakRate": 0,
    "links": 0,
    "interactionCounts": [
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0
    ],
    "interactionNames": [
      "hug",
      "kiss",
      "hand_hold",
      "dance_hold",
      "ring_exchange",
      "blessing",
      "toast",
      "tears_wiped",
      "group_cheer"
    ],
    "meanScore": 0,
    "meanMargin": 0,
    "preferences": 0,
    "modelVer": 0,
    "analysisVer": 0,
    "weightsVer": 0
  }
}
```

## `ensure_masks`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Produce the named regions if they are not already stored. Idempotent.

Native definition: `ui/src-tauri/src/main.rs:1871`.

```rust
async fn ensure_masks( state: State<'_, AppState>, input: EnsureMasksInput, ) -> IpcResult<Vec<MaskDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1399`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `erase_biometrics`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Irreversible. `confirm` must equal `projectId`, and the backend refuses otherwise - the check is there rather than only in the dialog because this is the one operation in the product with no undo.

Native definition: `ui/src-tauri/src/main.rs:2053`.

```rust
async fn erase_biometrics( state: State<'_, AppState>, input: EraseBiometricsInput, ) -> IpcResult<EraseBiometricsDto>
```

Primary UI binding: `ui/src/ipc/client.ts:833`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `find_similar`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:1713`.

```rust
async fn find_similar( state: State<'_, AppState>, input: FindSimilarInput, ) -> IpcResult<SimilarResultDto>
```

Primary UI binding: `ui/src/ipc/client.ts:765`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `flagged_images`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The frames carrying any of these marks, worst technical score first.

Native definition: `ui/src-tauri/src/main.rs:1821`.

```rust
async fn flagged_images(state: State<'_, AppState>, input: FlaggedInput) -> IpcResult<Vec<String>>
```

Primary UI binding: `ui/src/ipc/client.ts:965`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `get_preview`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:2197`.

```rust
async fn get_preview( state: State<'_, AppState>, input: GetPreviewInput, ) -> IpcResult<PreviewPayload>
```

Primary UI binding: `ui/src/ipc/client.ts:663`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `group_moments`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Regroup a wedding. Preserves every moment the photographer has locked.

Native definition: `ui/src-tauri/src/main.rs:1934`.

```rust
async fn group_moments( state: State<'_, AppState>, input: GroupMomentsInput, ) -> IpcResult<MomentStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:916`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "group_moments",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
    }
  },
  "ok": true,
  "ms": 16.0,
  "result": {
    "photos": 5,
    "groupable": 4,
    "grouped": 0,
    "coverage": 0,
    "moments": 0,
    "locked": 0,
    "meanSize": 0,
    "bursts": 0,
    "duplicates": [
      0,
      0,
      0
    ],
    "medianIntervalMs": 0,
    "implausible": false,
    "embedVer": 0,
    "groupVer": 1,
    "profileVer": 1
  }
}
```

## `group_people`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:2064`.

```rust
async fn group_people( state: State<'_, AppState>, input: GroupPeopleInput, ) -> IpcResult<GroupPeopleDto>
```

Primary UI binding: `ui/src/ipc/client.ts:799`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "group_people",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
    }
  },
  "ok": true,
  "ms": 16.0,
  "result": {
    "identities": 0,
    "assigned": 0,
    "unassigned": 0,
    "threshold": 0.55,
    "refusedMerges": 0,
    "subClustered": 0,
    "couple": [],
    "coupleConfidence": 0,
    "coupleAmbiguous": false,
    "sceneStarved": true,
    "decisionsReplayed": 0,
    "decisionsOrphaned": 0,
    "coverage": 0,
    "elapsedMs": 2
  }
}
```

## `hardware_plan`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Native definition: `ui/src-tauri/src/main.rs:1747`.

```rust
async fn hardware_plan(state: State<'_, AppState>) -> IpcResult<HardwarePlanDto>
```

Primary UI binding: `ui/src/ipc/client.ts:687`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "hardware_plan",
  "args": null,
  "ok": true,
  "ms": 0.0,
  "result": {
    "gpu": null,
    "epOrder": [
      "cpu"
    ],
    "selectedEp": "cpu",
    "overrideEp": null,
    "unavailable": [
      {
        "ep": "tensorrt",
        "reason": "not compiled into this build"
      },
      {
        "ep": "cuda",
        "reason": "not compiled into this build"
      },
      {
        "ep": "directml",
        "reason": "not compiled into this build"
      },
      {
        "ep": "coreml",
        "reason": "not compiled into this build"
      }
    ],
    "setAside": [],
    "vramBudgetMb": 0,
    "cpuThreads": 11,
    "probeScoresMs": [
      {
        "ep": "cpu",
        "medianMs": 0.273
      }
    ],
    "probedAt": "2026-10-18T20:26:10Z",
    "probed": true
  }
}
```

## `history_step`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 160; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 2.

Binding documentation: Undo, redo, or one of the two resets.

Native definition: `ui/src-tauri/src/main.rs:1542`.

```rust
async fn history_step( state: State<'_, AppState>, input: HistoryStepInput, ) -> IpcResult<SetParamDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1216`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "history_step",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83",
      "photoId": "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7",
      "action": "undo"
    }
  },
  "ok": true,
  "ms": 219.0,
  "result": {
    "recipe": {
      "photoId": "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7",
      "recipeHash": "f5fa8e3d92b96f2c1ec6e66915316399ff164759468f6a6055faa3fc107110e0",
      "schema": 1,
      "engine": "aura-render/1.0.0",
      "source": "user",
      "confidence": 1,
      "decisionId": null,
      "userEditedFields": [
        "bw.grade"
      ]
    },
    "changed": [
      "bw.mix.magenta"
    ],
    "invalidatedFrom": "monochrome"
  }
}
```

## `identity_cover`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The only route from the sealed biometric store to a screen. One crop per call, decoded in Rust; there is deliberately no command that returns a template.

Native definition: `ui/src-tauri/src/main.rs:2075`.

```rust
async fn identity_cover( state: State<'_, AppState>, project_id: String, face_id: String, ) -> IpcResult<FaceCropDto>
```

Primary UI binding: `ui/src/ipc/client.ts:825`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `image_subjects`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:2042`.

```rust
async fn image_subjects( state: State<'_, AppState>, photo_id: String, ) -> IpcResult<ImageSubjectsDto>
```

Primary UI binding: `ui/src/ipc/client.ts:793`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `identity_timelines`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The "this is the bride" button. Sets `userLocked`; automation may not undo it.

Native definition: `ui/src-tauri/src/main.rs:2089`.

```rust
async fn identity_timelines( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Vec<IdentityTimelineDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:818`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `image_descriptors`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:1724`.

```rust
async fn image_descriptors( state: State<'_, AppState>, project_id: String, photo_id: String, ) -> IpcResult<DescriptorsDto>
```

Primary UI binding: `ui/src/ipc/client.ts:777`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `image_emotion`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: One photograph's reading. `null` means **nobody has looked** - which is not the same as "nothing happened here", and the card must not draw it that way.

Native definition: `ui/src-tauri/src/main.rs:1619`.

```rust
async fn image_emotion( state: State<'_, AppState>, photo_id: String, ) -> IpcResult<Option<EmotionDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1014`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `image_history`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 12.

Binding documentation: One photograph's history, its snapshots, and what is available.

Native definition: `ui/src-tauri/src/main.rs:1553`.

```rust
async fn image_history( state: State<'_, AppState>, input: DevelopImageInput, ) -> IpcResult<HistoryDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1220`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `image_integrity`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: One photograph's verdict. `null` means **nobody has looked** - which is not the same as "nothing is wrong", and the card must not draw it that way.

Native definition: `ui/src-tauri/src/main.rs:1829`.

```rust
async fn image_integrity( state: State<'_, AppState>, photo_id: String, ) -> IpcResult<Option<IntegrityDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:961`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `image_masks`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Every region stored for one photograph, in the frozen class order. An empty list means nobody has masked this frame yet. It is not the same as a frame with no regions in it, and the panel renders the two differently.

Native definition: `ui/src-tauri/src/main.rs:1882`.

```rust
async fn image_masks(state: State<'_, AppState>, image_id: String) -> IpcResult<Vec<MaskDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1395`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `image_recipe`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 4; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 12.

Binding documentation: One photograph's edit, or the camera's own starting point when it has none.

Native definition: `ui/src-tauri/src/main.rs:1564`.

```rust
async fn image_recipe( state: State<'_, AppState>, input: DevelopImageInput, ) -> IpcResult<RecipeDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1208`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "image_recipe",
  "args": {
    "input": {
      "photoId": "pht_01a150aa-0637-7952-94a9-e26308c1ffee"
    }
  },
  "ok": true,
  "ms": 187.0,
  "result": {
    "photoId": "pht_01a150aa-0637-7952-94a9-e26308c1ffee",
    "recipeHash": "c587dba423f7a4be3ba78474df9b23c8cbd83c57eeeb7e6fd9e28d231000bb3f",
    "schema": 1,
    "engine": "aura-render/1.0.0",
    "source": "ai",
    "confidence": 0.35,
    "decisionId": null,
    "userEditedFields": []
  }
}
```

## `image_scene`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: What one photograph is of, for the Explain panel. Null when unclassified.

Native definition: `ui/src-tauri/src/main.rs:2266`.

```rust
async fn image_scene(state: State<'_, AppState>, photo_id: String) -> IpcResult<Option<SceneDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:853`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `index_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:1738`.

```rust
async fn index_status(state: State<'_, AppState>, project_id: String) -> IpcResult<IndexStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:768`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "index_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "vectors": 0,
    "photos": 5,
    "coverage": 0,
    "filterable": 0,
    "modelVer": 0,
    "staleModelVersions": [],
    "buildMs": 0,
    "fromSnapshot": false
  }
}
```

## `infer_stats`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Native definition: `ui/src-tauri/src/main.rs:1755`.

```rust
async fn infer_stats(state: State<'_, AppState>) -> IpcResult<InferStatsDto>
```

Primary UI binding: `ui/src/ipc/client.ts:698`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "infer_stats",
  "args": null,
  "ok": true,
  "ms": 0.0,
  "result": {
    "residentSessions": 0,
    "poolHits": 0,
    "poolLoads": 0,
    "requests": 0,
    "downshifts": 0,
    "meanOverheadMs": 0,
    "peakMemoryMb": 0
  }
}
```

## `integrity_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: What the Integrity panel's header shows, including what was not checked.

Native definition: `ui/src-tauri/src/main.rs:1840`.

```rust
async fn integrity_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<IntegrityStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:952`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "integrity_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "photos": 5,
    "scored": 0,
    "coverage": 0,
    "subjectAware": 0,
    "reviewed": 0,
    "flagCounts": [
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0
    ],
    "flagNames": [
      "subject_soft",
      "camera_shake",
      "subject_motion",
      "intentional_motion",
      "back_focus",
      "front_focus",
      "highlight_lost",
      "shadow_lost",
      "heavy_noise",
      "eyes_closed",
      "eyes_closed_ok",
      "squint",
      "no_subject_detected",
      "mixed_light_risk"
    ],
    "meanScore": 0,
    "defectiveAtMost": 0,
    "uncalibrated": [],
    "modelVer": 0,
    "analysisVer": 0,
    "calibVer": 0
  }
}
```

## `list_identities`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Native definition: `ui/src-tauri/src/main.rs:2100`.

```rust
async fn list_identities( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Vec<IdentityCardDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:790`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `list_models`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Native definition: `ui/src-tauri/src/main.rs:1763`.

```rust
async fn list_models(state: State<'_, AppState>) -> IpcResult<Vec<ModelStatusDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:694`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "list_models",
  "args": null,
  "ok": true,
  "ms": 0.0,
  "result": [
    {
      "name": "aura_tiny_embedding",
      "version": "1.0.0",
      "task": "embedding",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/aura_tiny_embedding.md",
      "workingSetMb": 8,
      "fileCount": 3,
      "int8Forbidden": false
    },
    {
      "name": "aura_tiny_scene",
      "version": "1.0.0",
      "task": "multiclass",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/aura_tiny_scene.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "wedding_embedding",
      "version": "1.0.0",
      "task": "embedding",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/wedding_embedding.md",
      "workingSetMb": 8,
      "fileCount": 3,
      "int8Forbidden": false
    },
    {
      "name": "face_detect",
      "version": "1.0.0",
      "task": "detection",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/face_detect.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "face_embed",
      "version": "1.0.0",
      "task": "embedding",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/face_embed.md",
      "workingSetMb": 8,
      "fileCount": 3,
      "int8Forbidden": false
    },
    {
      "name": "face_quality",
      "version": "1.0.0",
      "task": "multiclass",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/face_quality.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "scene_classifier",
      "version": "1.0.0",
      "task": "multiclass",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/scene_classifier.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "ritual_classifier",
      "version": "1.0.0",
      "task": "multiclass",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/ritual_classifier.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "focus_head",
      "version": "1.0.0",
      "task": "multiclass",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/focus_head.md",
      "workingSetMb": 8,
      "fileCount": 3,
      "int8Forbidden": false
    },
    {
      "name": "eye_state",
      "version": "1.0.0",
      "task": "multiclass",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/eye_state.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "expression_head",
      "version": "1.0.0",
      "task": "multilabel",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/expression_head.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "interaction_head",
      "version": "1.0.0",
      "task": "multilabel",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/interaction_head.md",
      "workingSetMb": 8,
      "fileCount": 3,
      "int8Forbidden": false
    },
    {
      "name": "pose_keypoints",
      "version": "1.0.0",
      "task": "keypoints",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/pose_keypoints.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "aesthetic_head",
      "version": "1.0.0",
      "task": "ranker",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/aesthetic_head.md",
      "workingSetMb": 8,
      "fileCount": 3,
      "int8Forbidden": false
    },
    {
      "name": "white_balance",
      "version": "1.0.0",
      "task": "regression",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/white_balance.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "exposure_scene",
      "version": "1.0.0",
      "task": "regression",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/exposure_scene.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "tone_model",
      "version": "1.0.0",
      "task": "regression",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/tone_model.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "semantic_segment",
      "version": "1.0.0",
      "task": "segmentation",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/semantic_segment.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "alpha_matting",
      "version": "1.0.0",
      "task": "matting",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/alpha_matting.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "blemish_detector",
      "version": "1.0.0",
      "task": "detection",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/blemish_detector.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "permanent_features",
      "version": "1.0.0",
      "task": "classification",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/permanent_features.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "flyaway_detector",
      "version": "1.0.0",
      "task": "detection",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/flyaway_detector.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "glare_detector",
      "version": "1.0.0",
      "task": "detection",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/glare_detector.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "lint_detector",
      "version": "1.0.0",
      "task": "classification",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/lint_detector.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "denoise",
      "version": "1.0.0",
      "task": "restoration",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/denoise.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    },
    {
      "name": "face_recovery",
      "version": "1.0.0",
      "task": "restoration",
      "activeVersion": null,
      "pendingVersion": null,
      "rejectedVersions": [],
      "modelCard": "docs/model-cards/face_recovery.md",
      "workingSetMb": 8,
      "fileCount": 2,
      "int8Forbidden": true
    }
  ]
}
```

## `list_moments`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Every moment in a wedding, or one chapter's. The grid virtualises this list exactly as it virtualises frames.

Native definition: `ui/src-tauri/src/main.rs:1945`.

```rust
async fn list_moments(state: State<'_, AppState>, input: MomentsInput) -> IpcResult<MomentListDto>
```

Primary UI binding: `ui/src/ipc/client.ts:897`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `lock_moment`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Pin a grouping against re-analysis, or release it.

Native definition: `ui/src-tauri/src/main.rs:1953`.

```rust
async fn lock_moment( state: State<'_, AppState>, input: LockMomentInput, ) -> IpcResult<MomentHandleDto>
```

Primary UI binding: `ui/src/ipc/client.ts:927`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `mask_allowance`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: What one operation may do through one region, and why it may not do more.

Native definition: `ui/src-tauri/src/main.rs:1890`.

```rust
async fn mask_allowance( state: State<'_, AppState>, mask_id: String, operation: String, ) -> IpcResult<MaskAllowanceDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1407`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `mask_kinds`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The twenty class slugs, in the frozen iteration order.

Native definition: `ui/src-tauri/src/main.rs:1904`.

```rust
fn mask_kinds() -> IpcResult<Vec<String>>
```

Primary UI binding: `ui/src/ipc/client.ts:1418`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "mask_kinds",
  "args": null,
  "ok": true,
  "ms": 0.0,
  "result": [
    "skin",
    "face",
    "eyes",
    "sclera",
    "iris",
    "teeth",
    "lips",
    "eyebrows",
    "hair",
    "facial_hair",
    "clothing",
    "dress",
    "background",
    "sky",
    "subject",
    "greenery",
    "water",
    "floor",
    "window",
    "skin_safe"
  ]
}
```

## `mask_overlay`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: One region as a plane to draw over a preview. Quarter resolution, capped.

Native definition: `ui/src-tauri/src/main.rs:1909`.

```rust
async fn mask_overlay(state: State<'_, AppState>, mask_id: String) -> IpcResult<MaskOverlayDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1403`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `mask_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: What the mask panel's project header shows. Two numbers, never a ratio.

Native definition: `ui/src-tauri/src/main.rs:1917`.

```rust
async fn mask_status(state: State<'_, AppState>, project_id: String) -> IpcResult<MaskStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1386`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "mask_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "selected": 0,
    "masked": 0,
    "masks": 0,
    "userEdited": 0,
    "lowQuality": 0,
    "meanConfidence": 0,
    "meanEdgeQuality": 0,
    "payloadBytes": 0,
    "bytesPerImage": 0,
    "modelVer": 1,
    "analysisVer": 1,
    "headTrained": false
  }
}
```

## `merge_chapters`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Move a boundary. Locks BOTH chapters either side of it - a boundary is shared.

Native definition: `ui/src-tauri/src/main.rs:2274`.

```rust
async fn merge_chapters( state: State<'_, AppState>, input: MergeChaptersInput, ) -> IpcResult<ChapterHandleDto>
```

Primary UI binding: `ui/src/ipc/client.ts:881`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `merge_identities`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:2111`.

```rust
async fn merge_identities( state: State<'_, AppState>, input: MergeIdentitiesInput, ) -> IpcResult<IdentityHandleDto>
```

Primary UI binding: `ui/src/ipc/client.ts:802`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `merge_moments`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Break a moment in two. Locks BOTH halves - a split is one statement about two.

Native definition: `ui/src-tauri/src/main.rs:1964`.

```rust
async fn merge_moments( state: State<'_, AppState>, input: MergeMomentsInput, ) -> IpcResult<MomentHandleDto>
```

Primary UI binding: `ui/src/ipc/client.ts:923`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `moment_duplicates`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The duplicate sets inside one moment - only the ones that cap the gallery. The alternatives phase 12 chooses between are the moment's frames grouped by `burstIx`; a variant set is not stored because it constrains nothing.

Native definition: `ui/src-tauri/src/main.rs:1975`.

```rust
async fn moment_duplicates( state: State<'_, AppState>, moment_id: String, ) -> IpcResult<Vec<DuplicateSetDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:912`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `moment_of_image`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The moment one photograph is in. Null when it has not been grouped.

Native definition: `ui/src-tauri/src/main.rs:1986`.

```rust
async fn moment_of_image( state: State<'_, AppState>, photo_id: String, ) -> IpcResult<Option<MomentDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:904`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `moment_peak`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: One moment's peak. `null` means the moment has not been scored. A moment that *was* scored and had no apex comes back with `resolved: false`, which is a different and common answer.

Native definition: `ui/src-tauri/src/main.rs:1630`.

```rust
async fn moment_peak( state: State<'_, AppState>, moment_id: String, ) -> IpcResult<Option<MomentPeakDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1024`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `moment_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Every moment in a wedding, or one chapter's. The grid virtualises this list exactly as it virtualises frames.

Native definition: `ui/src-tauri/src/main.rs:1997`.

```rust
async fn moment_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<MomentStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:900`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "moment_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "photos": 5,
    "groupable": 0,
    "grouped": 0,
    "coverage": 0,
    "moments": 0,
    "locked": 0,
    "meanSize": 0,
    "bursts": 0,
    "duplicates": [
      0,
      0,
      0
    ],
    "medianIntervalMs": 0,
    "implausible": false,
    "embedVer": 0,
    "groupVer": 1,
    "profileVer": 1
  }
}
```

## `within_moment`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: One moment's frames ranked by subject sharpness. Evidence phase 12 asked for by name. It says which of six frames is sharpest and nothing about which of them a client sees.

Native definition: `ui/src-tauri/src/main.rs:1851`.

```rust
async fn within_moment( state: State<'_, AppState>, input: WithinMomentInput, ) -> IpcResult<Vec<RankedFrameDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:974`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `move_chapter_boundary`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Move a boundary. Locks BOTH chapters either side of it - a boundary is shared.

Native definition: `ui/src-tauri/src/main.rs:2285`.

```rust
async fn move_chapter_boundary( state: State<'_, AppState>, input: MoveBoundaryInput, ) -> IpcResult<ChapterHandleDto>
```

Primary UI binding: `ui/src/ipc/client.ts:875`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `people_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Native definition: `ui/src-tauri/src/main.rs:2122`.

```rust
async fn people_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<PeopleStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:787`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "people_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 1016.0,
  "result": {
    "photos": 5,
    "scanned": 0,
    "coverage": 0,
    "faces": 0,
    "votingFaces": 0,
    "identities": 0,
    "tiledFrames": 0,
    "coupleUnconfirmed": true,
    "staleVersions": [],
    "erased": false,
    "keyStore": "dpapi",
    "weightsVer": 1
  }
}
```

## `prefer_frame`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: "I would deliver this one." Recorded for phase 30's learning loop and applied to nothing today: a ranker that refitted itself mid-cull would reorder the grid under the photographer's hands.

Native definition: `ui/src-tauri/src/main.rs:1641`.

```rust
async fn prefer_frame(state: State<'_, AppState>, input: PreferInput) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:1043`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `prefetch_previews`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:2208`.

```rust
async fn prefetch_previews(state: State<'_, AppState>, input: PrefetchInput) -> IpcResult<i64>
```

Primary UI binding: `ui/src/ipc/client.ts:666`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `preview_problems`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The previews a project could not build, with the reason each one was quarantined. Phase 02's surface, and the last command in the product to get a typed wrapper: the shell has answered to it since phase 02 and nothing had ever called it. A preview that failed is not an image that is missing - it is an image the grid must render as a problem rather than as an empty cell, which is invariant 9 at the level a photographer sees.

Native definition: `ui/src-tauri/src/main.rs:2216`.

```rust
async fn preview_problems( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Vec<(String, String)>>
```

Primary UI binding: `ui/src/ipc/client.ts:1431`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `preview_stats`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Native definition: `ui/src-tauri/src/main.rs:2227`.

```rust
async fn preview_stats(state: State<'_, AppState>, project_id: String) -> IpcResult<CacheStatsDto>
```

Primary UI binding: `ui/src/ipc/client.ts:672`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "preview_stats",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "bytesUsed": 28258568,
    "budgetBytes": 42949672960,
    "entries": 16,
    "hits": 19,
    "misses": 29,
    "evictions": 0,
    "hitRate": 0.3958333333333333
  }
}
```

## `purge_cache`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:2235`.

```rust
async fn purge_cache(state: State<'_, AppState>, project_id: String) -> IpcResult<CacheStatsDto>
```

Primary UI binding: `ui/src/ipc/client.ts:678`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "purge_cache",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 94.0,
  "result": {
    "bytesUsed": 0,
    "budgetBytes": 268435456,
    "entries": 0,
    "hits": 19,
    "misses": 29,
    "evictions": 0,
    "hitRate": 0.3958333333333333
  }
}
```

## `purge_cloud_cache`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Answer the first-run screen by declining it. Records no provider.

Native definition: `ui/src-tauri/src/main.rs:1486`.

```rust
async fn purge_cloud_cache( state: State<'_, AppState>, task: String, task_version: u32, ) -> IpcResult<u64>
```

Primary UI binding: `ui/src/ipc/client.ts:755`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `ranked_by_emotion`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: A project's frames ordered by emotion score, strongest first. **An ordering, not a selection.**

Native definition: `ui/src-tauri/src/main.rs:1649`.

```rust
async fn ranked_by_emotion( state: State<'_, AppState>, input: RankedInput, ) -> IpcResult<Vec<RankedByEmotionDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1036`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `reactions_of`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Every frame that reacts to this one, earliest first.

Native definition: `ui/src-tauri/src/main.rs:1660`.

```rust
async fn reactions_of( state: State<'_, AppState>, photo_id: String, ) -> IpcResult<Vec<ReactionLinkDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:1028`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `recheck_hardware`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:1771`.

```rust
async fn recheck_hardware(state: State<'_, AppState>) -> IpcResult<HardwarePlanDto>
```

Primary UI binding: `ui/src/ipc/client.ts:689`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `regenerate_mask`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Drop a photographer's edit so the next pass regenerates the region.

Native definition: `ui/src-tauri/src/main.rs:1925`.

```rust
async fn regenerate_mask(state: State<'_, AppState>, mask_id: String) -> IpcResult<bool>
```

Primary UI binding: `ui/src/ipc/client.ts:1415`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `rename_identity`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The "this is the bride" button. Sets `userLocked`; automation may not undo it.

Native definition: `ui/src-tauri/src/main.rs:2133`.

```rust
async fn rename_identity(state: State<'_, AppState>, input: RenameIdentityInput) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:812`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `render_caps`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 2; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: What this machine's renderer can do, and what it is running without.

Native definition: `ui/src-tauri/src/main.rs:1575`.

```rust
async fn render_caps(state: State<'_, AppState>) -> IpcResult<RenderCapsDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1231`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "render_caps",
  "args": null,
  "ok": true,
  "ms": 0.0,
  "result": {
    "backend": "wgpu",
    "maxTexture": 9459,
    "precisionBits": 24,
    "maxWorkingBytes": 1073741824,
    "engine": "aura-render/1.0.0+profiles.1+portrait.1+display.1+raw.2+clear.enclosed-light-v2+matte.bounded-contour-v2-zero-cell-support+undereye.socket-preserving-v2-reference-consensus+boundary.inward-distance-v1+skin.robust-patch-v3-pixel-connectivity+wgpu.output.1.Vulkan.10de.1c82.560.94.NVIDIA GeForce GTX 1050 Ti.min262144",
    "degradation": null,
    "degradationMessage": null
  }
}
```

## `render_image`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 447; refused harness calls: 1; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Render a proxy. The pixels come back inline; there is no file.

Native definition: `ui/src-tauri/src/main.rs:1583`.

```rust
async fn render_image(state: State<'_, AppState>, input: RenderImageInput) -> IpcResult<RenderDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1228`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "render_image",
  "args": {
    "input": {
      "photoId": "pht_01a150aa-0637-7952-94a9-e26308c1ffee",
      "level": "full",
      "purpose": "analysis"
    }
  },
  "ok": true,
  "ms": 42109.0,
  "result": {
    "width": 4480,
    "height": 6720,
    "bytes": 90316800,
    "pixelSha256": "6f4edaa14666270ffa430a9bcfbd1922401ea504cb04e0d071f2c2efac878636",
    "renderHash": "d3208704e96c4a535ebbbe38c3b4f3cb66cdadce5917d348981fdbf3aa5bb9b5",
    "backend": "wgpu",
    "ms": 22448,
    "notes": [
      {
        "stage": "calibration",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "camera_matrix",
        "reason": "upstream_already",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "clarity",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "cleanup",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "dehaze",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "exposure",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "geometry",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "grain",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "highlight_recovery",
        "reason": "upstream_already",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "lens_ca",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "lens_distortion",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "lens_vignette",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "masks",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "monochrome",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "noise_reduction",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "post_crop_vignette",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "restoration",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "retouch",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "texture",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "vibrance",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      },
      {
        "stage": "white_balance",
        "reason": "not_requested",
        "detail": null,
        "isCaveat": false
      }
    ],
    "finiteByteBuffer": true
  }
}
```

## `scan_faces`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:2141`.

```rust
async fn scan_faces(state: State<'_, AppState>, input: ScanFacesInput) -> IpcResult<ScanFacesDto>
```

Primary UI binding: `ui/src/ipc/client.ts:796`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `scene_profiles`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Every scene's tolerances, with the sentence explaining them. The rationale is on the wire because "why is my dance floor being judged this way" is a question a photographer asks, and invariant 2 says it has to have an answer.

Native definition: `ui/src-tauri/src/main.rs:2296`.

```rust
async fn scene_profiles( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Vec<SceneProfileDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:861`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `score_emotion`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Score every frame that has no current reading. Resumable and cancellable.

Native definition: `ui/src-tauri/src/main.rs:1671`.

```rust
async fn score_emotion( state: State<'_, AppState>, input: ScoreEmotionInput, ) -> IpcResult<EmotionPassDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1054`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "score_emotion",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
    }
  },
  "ok": true,
  "ms": 63.0,
  "result": {
    "scored": 4,
    "failed": 1,
    "faces": 0,
    "moments": 0,
    "peaked": 0,
    "links": 0,
    "meanScore": 0.47592992,
    "elapsedMs": 50,
    "cancelled": false
  }
}
```

## `segment_story`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Every scene's tolerances, with the sentence explaining them. The rationale is on the wire because "why is my dance floor being judged this way" is a question a photographer asks, and invariant 2 says it has to have an answer.

Native definition: `ui/src-tauri/src/main.rs:2307`.

```rust
async fn segment_story( state: State<'_, AppState>, project_id: String, ) -> IpcResult<StoryOutlineDto>
```

Primary UI binding: `ui/src/ipc/client.ts:867`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `list_ai_providers`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Every provider AURA knows how to reach. Static, and cheap to ask for.

Native definition: `ui/src-tauri/src/main.rs:1370`.

```rust
async fn list_ai_providers(state: State<'_, AppState>) -> IpcResult<Vec<AiProviderDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:725`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `open_ai_provider_page`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The one command that carries the key, and it carries it one way only. There is deliberately no `getAiKey`.

Native definition: `ui/src-tauri/src/main.rs:1363`.

```rust
async fn open_ai_provider_page(provider: String) -> IpcResult<String>
```

Primary UI binding: `ui/src/ipc/client.ts:721`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `save_ai_setup`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record the provider choice. The key is not part of this; `setAiKey` is.

Native definition: `ui/src-tauri/src/main.rs:1386`.

```rust
async fn save_ai_setup( state: State<'_, AppState>, input: SaveAiSetupInput, ) -> IpcResult<AiSetupStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:733`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_ai_key`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The one command that carries the key, and it carries it one way only. There is deliberately no `getAiKey`.

Native definition: `ui/src-tauri/src/main.rs:1500`.

```rust
async fn set_ai_key(state: State<'_, AppState>, input: SetAiKeyInput) -> IpcResult<CloudStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:714`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `skip_ai_setup`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Answer the first-run screen by declining it. Records no provider.

Native definition: `ui/src-tauri/src/main.rs:1397`.

```rust
async fn skip_ai_setup(state: State<'_, AppState>) -> IpcResult<AiSetupStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:737`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_cache_budget`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 2; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:2243`.

```rust
async fn set_cache_budget( state: State<'_, AppState>, input: SetCacheBudgetInput, ) -> IpcResult<CacheStatsDto>
```

Primary UI binding: `ui/src/ipc/client.ts:675`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "set_cache_budget",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83",
      "budgetBytes": 42949672960
    }
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "bytesUsed": 0,
    "budgetBytes": 42949672960,
    "entries": 0,
    "hits": 19,
    "misses": 29,
    "evictions": 0,
    "hitRate": 0.3958333333333333
  }
}
```

## `set_chapter`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Rename a chapter. Sets `userLocked`; re-analysis may not undo it.

Native definition: `ui/src-tauri/src/main.rs:2318`.

```rust
async fn set_chapter( state: State<'_, AppState>, input: SetChapterInput, ) -> IpcResult<ChapterHandleDto>
```

Primary UI binding: `ui/src/ipc/client.ts:871`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_cloud_budget`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Answer the first-run screen by declining it. Records no provider.

Native definition: `ui/src-tauri/src/main.rs:1508`.

```rust
async fn set_cloud_budget( state: State<'_, AppState>, input: SetCloudBudgetInput, ) -> IpcResult<CloudSpendDto>
```

Primary UI binding: `ui/src/ipc/client.ts:740`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_cloud_privacy`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Answer the first-run screen by declining it. Records no provider.

Native definition: `ui/src-tauri/src/main.rs:1519`.

```rust
async fn set_cloud_privacy( state: State<'_, AppState>, input: SetCloudPrivacyInput, ) -> IpcResult<CloudStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:743`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_execution_provider`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:1779`.

```rust
async fn set_execution_provider( state: State<'_, AppState>, input: SetExecutionProviderInput, ) -> IpcResult<HardwarePlanDto>
```

Primary UI binding: `ui/src/ipc/client.ts:692`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_identity_importance`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The "this is the bride" button. Sets `userLocked`; automation may not undo it.

Native definition: `ui/src-tauri/src/main.rs:2149`.

```rust
async fn set_identity_importance( state: State<'_, AppState>, input: SetIdentityImportanceInput, ) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:815`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_identity_role`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The "this is the bride" button. Sets `userLocked`; automation may not undo it.

Native definition: `ui/src-tauri/src/main.rs:2160`.

```rust
async fn set_identity_role( state: State<'_, AppState>, input: SetIdentityRoleInput, ) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:809`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_keep_hint`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: "Keep this one." Moves where phase 12 starts from; culls nothing, and cannot - every other frame in the set stays exactly as eligible as it was.

Native definition: `ui/src-tauri/src/main.rs:2008`.

```rust
async fn set_keep_hint( state: State<'_, AppState>, input: SetKeepHintInput, ) -> IpcResult<MomentHandleDto>
```

Primary UI binding: `ui/src/ipc/client.ts:934`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_moment_peak`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: "This frame is the one." Unbeatable: a re-analysis re-applies the choice rather than reverting it.

Native definition: `ui/src-tauri/src/main.rs:1682`.

```rust
async fn set_moment_peak( state: State<'_, AppState>, input: SetPeakInput, ) -> IpcResult<MomentPeakDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1050`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `set_param`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 102; refused harness calls: 5; successful responses observed in the isolated UI audit ring: 2.

Binding documentation: Change one parameter, as a person. Marks the path protected from then on.

Native definition: `ui/src-tauri/src/main.rs:1591`.

```rust
async fn set_param(state: State<'_, AppState>, input: SetParamInput) -> IpcResult<SetParamDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1212`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "set_param",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83",
      "photoId": "pht_01a150aa-03f4-78e3-ac32-6fd99b2b80c3",
      "path": "global.exposure",
      "value": 0.23,
      "label": "Audit protected manual correction"
    }
  },
  "ok": true,
  "ms": 156.0,
  "result": {
    "recipe": {
      "photoId": "pht_01a150aa-03f4-78e3-ac32-6fd99b2b80c3",
      "recipeHash": "289572350385846c9cf1e05f49d404bee20f425cd5ca1873e6aa7e6b1bdf640d",
      "schema": 1,
      "engine": "aura-render/1.0.0",
      "source": "user",
      "confidence": 0.35,
      "decisionId": null,
      "userEditedFields": [
        "global.blacks",
        "global.clarity",
        "global.contrast",
        "global.exposure",
        "global.highlights",
        "global.hsl.aqua.h",
        "global.hsl.aqua.l",
        "global.hsl.aqua.s",
        "global.hsl.blue.h",
        "global.hsl.blue.l",
        "global.hsl.blue.s",
        "global.hsl.green.h",
        "global.hsl.green.l",
        "global.hsl.green.s",
        "global.hsl.magenta.h",
        "global.hsl.magenta.l",
        "global.hsl.magenta.s",
        "global.hsl.orange.h",
        "global.hsl.orange.l",
        "global.hsl.orange.s",
        "global.hsl.purple.h",
        "global.hsl.purple.l",
        "global.hsl.purple.s",
        "global.hsl.red.h",
        "global.hsl.red.l",
        "global.hsl.red.s",
        "global.hsl.yellow.h",
        "global.hsl.yellow.l",
        "global.hsl.yellow.s",
        "global.sharpen.amount",
        "global.sharpen.masking",
        "global.vibrance",
        "global.whites",
        "studio_masks_mattes_v1.background-099d8dc5e67a.bounds",
        "studio_masks_mattes_v1.background-099d8dc5e67a.data",
        "studio_masks_mattes_v1.background-099d8dc5e67a.height",
        "studio_masks_mattes_v1.background-099d8dc5e67a.refineEdges",
        "studio_masks_mattes_v1.background-099d8dc5e67a.width",
        "studio_masks_mattes_v1.body_skin-9798623ec9a4.bounds",
        "studio_masks_mattes_v1.body_skin-9798623ec9a4.data",
        "11 further values in raw evidence"
      ]
    },
    "changed": [
      "global.exposure"
    ],
    "invalidatedFrom": "exposure"
  }
}
```

## `snapshot`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 97; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Take or restore a named snapshot.

Native definition: `ui/src-tauri/src/main.rs:1599`.

```rust
async fn snapshot(state: State<'_, AppState>, input: SnapshotInput) -> IpcResult<HistoryDto>
```

Primary UI binding: `ui/src/ipc/client.ts:1224`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "snapshot",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83",
      "photoId": "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7",
      "name": "Full audit baseline",
      "action": "restore"
    }
  },
  "ok": true,
  "ms": 125.0,
  "result": {
    "photoId": "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7",
    "entries": [
      {
        "seq": 390,
        "atMs": 1792354870599,
        "source": "user",
        "changed": [
          "global.exposure"
        ],
        "label": "Audit global.exposure"
      },
      {
        "seq": 391,
        "atMs": 1792354870613,
        "source": "user",
        "changed": [
          "global.exposure"
        ],
        "label": "Restore snapshot: Full audit baseline"
      },
      {
        "seq": 392,
        "atMs": 1792354870853,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "Clear audit masks"
      },
      {
        "seq": 393,
        "atMs": 1792354870888,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "New mask: Linear gradient"
      },
      {
        "seq": 394,
        "atMs": 1792354870897,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "Audit local exposure"
      },
      {
        "seq": 395,
        "atMs": 1792354870928,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "Audit zero mask strength"
      },
      {
        "seq": 396,
        "atMs": 1792354871004,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "Clear audit masks"
      },
      {
        "seq": 397,
        "atMs": 1792354871033,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "New mask: Radial gradient"
      },
      {
        "seq": 398,
        "atMs": 1792354871047,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "Audit local exposure"
      },
      {
        "seq": 399,
        "atMs": 1792354871082,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "Audit zero mask strength"
      },
      {
        "seq": 400,
        "atMs": 1792354871144,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "Clear audit masks"
      },
      {
        "seq": 401,
        "atMs": 1792354871177,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "New mask: Brush"
      },
      {
        "seq": 402,
        "atMs": 1792354871186,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "Audit local exposure"
      },
      {
        "seq": 403,
        "atMs": 1792354871217,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "Audit zero mask strength"
      },
      {
        "seq": 404,
        "atMs": 1792354871284,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "Clear audit masks"
      },
      {
        "seq": 405,
        "atMs": 1792354871314,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "New mask: Brightness range"
      },
      {
        "seq": 406,
        "atMs": 1792354871323,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "Audit local exposure"
      },
      {
        "seq": 407,
        "atMs": 1792354871354,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "Audit zero mask strength"
      },
      {
        "seq": 408,
        "atMs": 1792354871422,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "Finish mask audit"
      },
      {
        "seq": 409,
        "atMs": 1792355018156,
        "source": "user",
        "changed": [
          "studio_masks_v1"
        ],
        "label": "Restore snapshot: Full audit baseline"
      },
      {
        "seq": 420,
        "atMs": 1792355021542,
        "source": "user",
        "changed": [
          "bw.grade"
        ],
        "label": "Audit monochrome"
      },
      {
        "seq": 437,
        "atMs": 1792355025425,
        "source": "user",
        "changed": [
          "bw",
          "bw.grade"
        ],
        "label": "Restore snapshot: Full audit baseline"
      }
    ],
    "snapshots": [
      "Full audit baseline"
    ],
    "canUndo": true,
    "canRedo": false,
    "hasAiSuggestion": false
  }
}
```

## `split_chapter`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Move a boundary. Locks BOTH chapters either side of it - a boundary is shared.

Native definition: `ui/src-tauri/src/main.rs:2329`.

```rust
async fn split_chapter( state: State<'_, AppState>, input: SplitChapterInput, ) -> IpcResult<ChapterHandleDto>
```

Primary UI binding: `ui/src/ipc/client.ts:878`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `split_identity`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:2171`.

```rust
async fn split_identity( state: State<'_, AppState>, input: SplitIdentityInput, ) -> IpcResult<IdentityHandleDto>
```

Primary UI binding: `ui/src/ipc/client.ts:805`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `split_moment`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Break a moment in two. Locks BOTH halves - a split is one statement about two.

Native definition: `ui/src-tauri/src/main.rs:2019`.

```rust
async fn split_moment( state: State<'_, AppState>, input: SplitMomentInput, ) -> IpcResult<MomentHandleDto>
```

Primary UI binding: `ui/src/ipc/client.ts:920`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `story_outline`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 2.

Binding documentation: What the timeline opens with. One read; section 11 budgets 200 ms.

Native definition: `ui/src-tauri/src/main.rs:2340`.

```rust
async fn story_outline( state: State<'_, AppState>, project_id: String, ) -> IpcResult<StoryOutlineDto>
```

Primary UI binding: `ui/src/ipc/client.ts:846`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `story_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: What the timeline opens with. One read; section 11 budgets 200 ms.

Native definition: `ui/src-tauri/src/main.rs:2351`.

```rust
async fn story_status(state: State<'_, AppState>, project_id: String) -> IpcResult<StoryStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:849`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "story_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "photos": 5,
    "classified": 0,
    "coverage": 0,
    "chapters": 0,
    "needsReview": 0,
    "locked": 0,
    "penalty": 0,
    "gapsOnly": false,
    "sceneVer": 100,
    "taxonomyVer": 1,
    "ritualsKnown": 48
  }
}
```

## `undo_moment_edit`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: "Keep this one." Moves where phase 12 starts from; culls nothing, and cannot - every other frame in the set stays exactly as eligible as it was.

Native definition: `ui/src-tauri/src/main.rs:2030`.

```rust
async fn undo_moment_edit( state: State<'_, AppState>, project_id: String, ) -> IpcResult<MomentEditDto>
```

Primary UI binding: `ui/src/ipc/client.ts:937`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `warmup_models`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Native definition: `ui/src-tauri/src/main.rs:1790`.

```rust
async fn warmup_models(state: State<'_, AppState>) -> IpcResult<WarmupReportDto>
```

Primary UI binding: `ui/src/ipc/client.ts:696`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `autopilot_status`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 2.

Binding documentation: What the Autopilot panel's header shows.

Native definition: `ui/src-tauri/src/main.rs:2808`.

```rust
async fn autopilot_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<AutopilotStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2015`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `autopilot_preflight`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: What would happen if the run started now. Eight checks. Four can block - a wedding that will not open, a wedding with no photographs, a disk that cannot hold the output, and a missing model a mandatory stage needs - and every row carries a sentence saying what to do about it.

Native definition: `ui/src-tauri/src/main.rs:2819`.

```rust
async fn autopilot_preflight( state: State<'_, AppState>, project_id: String, ) -> IpcResult<AutopilotPreflightDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2025`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `autopilot_start`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Start or continue this wedding's run. Pressing this on a wedding that was stopped continues that run rather than starting a new one: a checkpoint is keyed on the run, so a fresh id would repeat every finished stage.

Native definition: `ui/src-tauri/src/main.rs:2830`.

```rust
async fn autopilot_start( state: State<'_, AppState>, input: AutopilotStartInput, ) -> IpcResult<AutopilotProgressDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2034`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `autopilot_progress`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: What the run in flight is doing right now, or null when nothing is running.

Native definition: `ui/src-tauri/src/main.rs:2841`.

```rust
async fn autopilot_progress( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Option<AutopilotProgressDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2038`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `autopilot_cancel`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Stop this wedding's run. The token is polled between units, never inside a write, so a stopped run leaves the catalog exactly as consistent as a finished one and picks up where it left off.

Native definition: `ui/src-tauri/src/main.rs:2852`.

```rust
async fn autopilot_cancel(state: State<'_, AppState>, project_id: String) -> IpcResult<bool>
```

Primary UI binding: `ui/src/ipc/client.ts:2047`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `autopilot_stages`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 2.

Binding documentation: Every stage of the newest run, with what happened to it.

Native definition: `ui/src-tauri/src/main.rs:2860`.

```rust
async fn autopilot_stages( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Vec<AutopilotStageDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2051`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `autopilot_summary`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 2.

Binding documentation: What the newest finished run did.

Native definition: `ui/src-tauri/src/main.rs:2871`.

```rust
async fn autopilot_summary( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Option<AutopilotSummaryDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2055`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `autopilot_events`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 2.

Binding documentation: Everything the governor did during the newest run.

Native definition: `ui/src-tauri/src/main.rs:2882`.

```rust
async fn autopilot_events( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Vec<AutopilotEventDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2059`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `autopilot_set_settings`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record what the photographer chose in the checklist.

Native definition: `ui/src-tauri/src/main.rs:2893`.

```rust
async fn autopilot_set_settings( state: State<'_, AppState>, input: AutopilotSettingsInput, ) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:2063`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `curate_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: The Curate panel's header: what has been curated, from how many keepers, at what versions.

Native definition: `ui/src-tauri/src/main.rs:2911`.

```rust
async fn curate_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<CurateStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2077`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "curate_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 15.0,
  "result": {
    "photos": 0,
    "selected": 0,
    "curated": 0,
    "coverage": 0,
    "bwOffered": 0,
    "bwAccepted": 0,
    "bwRejected": 0,
    "heroes": 0,
    "chaptersCovered": 0,
    "spreads": 0,
    "albumSize": 0,
    "rhythmScore": 0,
    "rhythmMeasurable": 0,
    "pairingScore": 0,
    "albumCovered": 0,
    "albumMissing": 0,
    "reorders": 0,
    "slotsUnfilled": 0,
    "cloudUsed": false,
    "cloudMovesApplied": 0,
    "cloudMovesRefused": 0,
    "bytes": 0,
    "policyVer": 0,
    "analysisVer": 0,
    "embedVer": 0,
    "headsTrained": false
  }
}
```

## `curate_project`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Run the whole curation pass for a wedding. Returns the header rather than the whole result: a 120-spread album with its coverage report, twenty heroes, two hundred monochrome candidates and three social sets is a large payload to send when the panel that asked is about to render a header.

Native definition: `ui/src-tauri/src/main.rs:2922`.

```rust
async fn curate_project( state: State<'_, AppState>, input: CurateProjectInput, ) -> IpcResult<CurateStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2087`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `curate_bw`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: The monochrome candidates, best first, each with its own eight-band mix.

Native definition: `ui/src-tauri/src/main.rs:2933`.

```rust
async fn curate_bw(state: State<'_, AppState>, project_id: String) -> IpcResult<Vec<CurateBwDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2091`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `curate_heroes`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: The portfolio, in rank order.

Native definition: `ui/src-tauri/src/main.rs:2941`.

```rust
async fn curate_heroes( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Vec<CurateHeroDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2095`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `curate_album`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: The album draft, or null when this wedding has not been curated.

Native definition: `ui/src-tauri/src/main.rs:2952`.

```rust
async fn curate_album( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Option<CurateAlbumDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2099`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `curate_spread`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: One spread in detail. A spread rather than the album, because the spread view is the screen a photographer spends the most time on and fetching 120 spreads to draw two frames is the shape ADR-0060 rejects.

Native definition: `ui/src-tauri/src/main.rs:2963`.

```rust
async fn curate_spread( state: State<'_, AppState>, spread_id: String, ) -> IpcResult<Option<CurateSpreadDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2108`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `curate_social`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: The grid set, the story set, the hero and their captions.

Native definition: `ui/src-tauri/src/main.rs:2974`.

```rust
async fn curate_social( state: State<'_, AppState>, project_id: String, ) -> IpcResult<CurateSocialDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2112`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `curate_teaser`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: The wedding-night teaser.

Native definition: `ui/src-tauri/src/main.rs:2985`.

```rust
async fn curate_teaser( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Vec<CuratePickDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2116`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `curate_set_order`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record a photographer's album order, and re-compose the album around it. The whole order rather than a move: the panel already holds the sequence, and a dropped message would leave the two disagreeing about an album silently. Refused when it reorders chapters.

Native definition: `ui/src-tauri/src/main.rs:2996`.

```rust
async fn curate_set_order( state: State<'_, AppState>, input: CurateOrderInput, ) -> IpcResult<CurateAlbumDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2125`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `curate_decide`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record accept or reject on one pick. One pick per call; there is no bulk decide.

Native definition: `ui/src-tauri/src/main.rs:3007`.

```rust
async fn curate_decide(state: State<'_, AppState>, input: CurateDecideInput) -> IpcResult<()>
```

Primary UI binding: `ui/src/ipc/client.ts:2129`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `curate_export`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: One set as a specification another tool can read. Text, never a file.

Native definition: `ui/src-tauri/src/main.rs:3015`.

```rust
async fn curate_export( state: State<'_, AppState>, input: CurateExportInput, ) -> IpcResult<CurateExportDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2133`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `export_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Three denominators, the verified share, and whether a manifest was sealed.

Native definition: `ui/src-tauri/src/main.rs:3035`.

```rust
async fn export_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<ExportStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2151`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "export_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "photos": 5,
    "selected": 0,
    "requested": 0,
    "written": 0,
    "verified": 0,
    "unverified": 0,
    "corrupt": 0,
    "renderFailed": 0,
    "renamed": 0,
    "sidecars": 0,
    "bytes": 0,
    "manifestSealed": false,
    "ms": 0
  }
}
```

## `export_presets`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: The six presets, each with the argument for it.

Native definition: `ui/src-tauri/src/main.rs:3046`.

```rust
async fn export_presets() -> IpcResult<Vec<ExportPresetDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2155`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `export_preview_names`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Every name a job would produce. **Writes nothing.**

Native definition: `ui/src-tauri/src/main.rs:3053`.

```rust
async fn export_preview_names( state: State<'_, AppState>, input: ExportJobInput, ) -> IpcResult<Vec<ExportNameDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2159`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "export_preview_names",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83",
      "sets": [
        {
          "name": "jpeg",
          "imageIds": [
            "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7"
          ],
          "format": "jpeg",
          "quality": 92,
          "colour": "srgb",
          "bitDepth": 8,
          "resize": "full",
          "sharpen": "none",
          "naming": "audit_{original}_{seq}",
          "sidecar": false
        },
        {
          "name": "png",
          "imageIds": [
            "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7"
          ],
          "format": "png",
          "quality": 92,
          "colour": "srgb",
          "bitDepth": 8,
          "resize": "full",
          "sharpen": "none",
          "naming": "audit_{original}_{seq}",
          "sidecar": false
        },
        {
          "name": "tiff",
          "imageIds": [
            "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7"
          ],
          "format": "tiff",
          "quality": 92,
          "colour": "srgb",
          "bitDepth": 16,
          "resize": "full",
          "sharpen": "none",
          "naming": "audit_{original}_{seq}",
          "sidecar": false
        }
      ],
      "destination": "C:\\Users\\Admin\\Documents\\project\\.work-checks\\full-feature-audit-2026-10-19\\test-exports",
      "destinationKind": "folder",
      "copyright": null,
      "contact": null,
      "creator": "AURA local feature audit",
      "keywords": [
        "local audit"
      ],
      "stripGps": true,
      "stripCameraSerial": true,
      "verify": true
    }
  },
  "ok": true,
  "ms": 0.0,
  "result": [
    {
      "imageId": "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7",
      "set": "jpeg",
      "path": "jpeg/audit_skin_chart_0001.jpg",
      "renamed": false,
      "reasons": []
    },
    {
      "imageId": "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7",
      "set": "png",
      "path": "png/audit_skin_chart_0001.png",
      "renamed": false,
      "reasons": []
    },
    {
      "imageId": "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7",
      "set": "tiff",
      "path": "tiff/audit_skin_chart_0001.tif",
      "renamed": false,
      "reasons": []
    }
  ]
}
```

## `export_run`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Render, write, read back, hash, seal. The one call that changes somebody's disk.

Native definition: `ui/src-tauri/src/main.rs:3064`.

```rust
async fn export_run( state: State<'_, AppState>, input: ExportJobInput, ) -> IpcResult<ExportStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2163`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "export_run",
  "args": {
    "input": {
      "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83",
      "sets": [
        {
          "name": "jpeg",
          "imageIds": [
            "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7"
          ],
          "format": "jpeg",
          "quality": 92,
          "colour": "srgb",
          "bitDepth": 8,
          "resize": "full",
          "sharpen": "none",
          "naming": "audit_{original}_{seq}",
          "sidecar": false
        },
        {
          "name": "png",
          "imageIds": [
            "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7"
          ],
          "format": "png",
          "quality": 92,
          "colour": "srgb",
          "bitDepth": 8,
          "resize": "full",
          "sharpen": "none",
          "naming": "audit_{original}_{seq}",
          "sidecar": false
        },
        {
          "name": "tiff",
          "imageIds": [
            "pht_01a150a9-fa0c-71d1-8948-b2a8e1b563d7"
          ],
          "format": "tiff",
          "quality": 92,
          "colour": "srgb",
          "bitDepth": 16,
          "resize": "full",
          "sharpen": "none",
          "naming": "audit_{original}_{seq}",
          "sidecar": false
        }
      ],
      "destination": "C:\\Users\\Admin\\Documents\\project\\.work-checks\\full-feature-audit-2026-10-19\\test-exports",
      "destinationKind": "folder",
      "copyright": null,
      "contact": null,
      "creator": "AURA local feature audit",
      "keywords": [
        "local audit"
      ],
      "stripGps": true,
      "stripCameraSerial": true,
      "verify": true
    }
  },
  "ok": true,
  "ms": 2031.0,
  "result": {
    "photos": 5,
    "selected": 0,
    "requested": 3,
    "written": 3,
    "verified": 3,
    "unverified": 0,
    "corrupt": 0,
    "renderFailed": 0,
    "renamed": 0,
    "sidecars": 0,
    "bytes": 157589,
    "manifestSealed": true,
    "ms": 1447
  }
}
```

## `export_run_watermarked`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: A personal style learned from a photographer's own Lightroom catalogue.

Native definition: `ui/src-tauri/src/main.rs:3574`.

```rust
async fn export_run_watermarked( state: State<'_, AppState>, input: ExportJobInput, watermark: aura_app::delivery_commands::Watermark, ) -> IpcResult<ExportStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:158`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `export_files`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Every file the last job wrote, with its reasons.

Native definition: `ui/src-tauri/src/main.rs:3075`.

```rust
async fn export_files( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Vec<ExportFileDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2167`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `export_manifest`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: The last sealed manifest, or null. Null is not an empty manifest: a wedding nobody exported and one whose export wrote nothing are different answers.

Native definition: `ui/src-tauri/src/main.rs:3086`.

```rust
async fn export_manifest( state: State<'_, AppState>, project_id: String, ) -> IpcResult<Option<DeliveryManifestDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2176`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "export_manifest",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 16.0,
  "result": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83",
    "createdAt": 0,
    "files": 3,
    "bytes": 157589,
    "sets": [
      [
        "jpeg",
        1
      ],
      [
        "png",
        1
      ],
      [
        "tiff",
        1
      ]
    ],
    "qcReportPath": null,
    "cleanupDisclosures": [],
    "engineVersions": [
      [
        "app",
        "1.0.0"
      ],
      [
        "export",
        "aura-export/1"
      ],
      [
        "recipe_schema",
        "1"
      ]
    ],
    "fullyHashed": true
  }
}
```

## `delivery_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Backups and uploads, plus whether this build can reach a network at all.

Native definition: `ui/src-tauri/src/main.rs:3097`.

```rust
async fn delivery_status( state: State<'_, AppState>, project_id: String, ) -> IpcResult<DeliveryStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2180`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "delivery_status",
  "args": {
    "projectId": "prj_01a150a9-f93f-75d0-bb2a-690fb0664e83"
  },
  "ok": true,
  "ms": 0.0,
  "result": {
    "files": 0,
    "backups": 0,
    "backedUp": 0,
    "diverged": 0,
    "providers": 0,
    "uploaded": 0,
    "outstanding": 0,
    "refused": 0,
    "resumes": 0,
    "unmappedSets": 0,
    "bytesSent": 0,
    "networkAvailable": false
  }
}
```

## `delivery_providers`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Which providers exist and which have a credential. **Never the credential.**

Native definition: `ui/src-tauri/src/main.rs:3108`.

```rust
async fn delivery_providers(state: State<'_, AppState>) -> IpcResult<Vec<ProviderDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2184`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `delivery_backup`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Copy a sealed delivery somewhere else, verifying every file.

Native definition: `ui/src-tauri/src/main.rs:3116`.

```rust
async fn delivery_backup( state: State<'_, AppState>, input: DeliveryInput, ) -> IpcResult<DeliveryStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2188`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `delivery_upload`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Start or resume an upload. Resuming is not a separate call: a photographer pressing this after their wifi came back is doing the same thing they did the first time.

Native definition: `ui/src-tauri/src/main.rs:3127`.

```rust
async fn delivery_upload( state: State<'_, AppState>, input: DeliveryInput, ) -> IpcResult<DeliveryStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2197`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `delivery_items`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Every file's state at a provider.

Native definition: `ui/src-tauri/src/main.rs:3138`.

```rust
async fn delivery_items( state: State<'_, AppState>, project_id: String, provider: String, ) -> IpcResult<Vec<UploadItemDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2201`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `learn_status`

Coverage: Harness-invoked: inspect case outcome and scope. Successful harness calls: 1; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: What the loop has seen, and whether it is receiving anything.

Native definition: `ui/src-tauri/src/main.rs:3152`.

```rust
async fn learn_status(state: State<'_, AppState>) -> IpcResult<LearnStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2214`.

Last recorded invocation (large pixel/recipe payloads summarized):

```json
{
  "command": "learn_status",
  "args": {},
  "ok": true,
  "ms": 0.0,
  "result": {
    "corrections": 0,
    "projects": 0,
    "buckets": 0,
    "actionableBuckets": 0,
    "unattributed": 0,
    "attributionRate": 0,
    "updates": 0,
    "adopted": 0,
    "consentedProjects": 0,
    "contributingProjects": 0,
    "fittedOnRealCorrections": false
  }
}
```

## `learn_buckets`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: Every bucket, with what was dropped as an outlier.

Native definition: `ui/src-tauri/src/main.rs:3160`.

```rust
async fn learn_buckets(state: State<'_, AppState>) -> IpcResult<Vec<LearnBucketDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2217`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `learn_compare`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The A/B, both sides on the same held-out corrections. Null when there is no candidate.

Native definition: `ui/src-tauri/src/main.rs:3168`.

```rust
async fn learn_compare( state: State<'_, AppState>, profile_id: String, ) -> IpcResult<Option<LearnComparisonDto>>
```

Primary UI binding: `ui/src/ipc/client.ts:2221`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `learn_adopt`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: *The only way a profile moves forward.**

Native definition: `ui/src-tauri/src/main.rs:3179`.

```rust
async fn learn_adopt(state: State<'_, AppState>, profile_id: String) -> IpcResult<LearnStatusDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2225`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `learn_roll_back`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: The previous version, byte for byte.

Native definition: `ui/src-tauri/src/main.rs:3187`.

```rust
async fn learn_roll_back(state: State<'_, AppState>, profile_id: String) -> IpcResult<u16>
```

Primary UI binding: `ui/src/ipc/client.ts:2229`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `learn_consent`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: What a project has consented to. Everything off unless somebody said otherwise.

Native definition: `ui/src-tauri/src/main.rs:3195`.

```rust
async fn learn_consent(state: State<'_, AppState>, project_id: String) -> IpcResult<ConsentDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2233`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `learn_set_consent`

Coverage: Registered; no invocation captured by the harness or UI ring log. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 0.

Binding documentation: Record consent. The app version stored is the build that asked, not one the caller sets.

Native definition: `ui/src-tauri/src/main.rs:3203`.

```rust
async fn learn_set_consent(state: State<'_, AppState>, input: ConsentDto) -> IpcResult<ConsentDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2237`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.

## `diagnostics_report`

Coverage: GUI-wrapper invocation observed; inspect panel scope. Successful harness calls: 0; refused harness calls: 0; successful responses observed in the isolated UI audit ring: 1.

Binding documentation: What this machine can and cannot do. Leads with the second.

Native definition: `ui/src-tauri/src/main.rs:3211`.

```rust
async fn diagnostics_report(state: State<'_, AppState>) -> IpcResult<DiagnosticsDto>
```

Primary UI binding: `ui/src/ipc/client.ts:2240`.

Remaining direct scenario: use an isolated catalogue and the typed input contract above, provide the required project/photo/dataset or external service, exercise its UI action and verify the specified saved state, pixels or external outcome. No separate harness invocation was made here; any GUI audit count above establishes only the observed UI request/response. Source reachability and automated tests are different evidence.
