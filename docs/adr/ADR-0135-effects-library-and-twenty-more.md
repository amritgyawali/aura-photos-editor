# ADR-0135: The Effects library, and twenty more tools

Status: accepted. Date: 2026-10-20.

Two things: a new **Effects** screen in the sidebar, and twenty more tools in the shape of
ADR-0131, ADR-0132 and ADR-0134. That makes 88 effects and 119 retouch tools in all.

## The Effects screen

With 119 tools, the Retouch **Tool** list had become hard to search. The Effects screen
(`ui/src/components/effects/EffectsLibrary.tsx`, mounted from `App.tsx` between Auto edit and
Match a look) lists every entry of `RETOUCH_TOOLS`:

- each tool appears once, under its own group;
- there is a search over name, group and description, and group chips;
- "Quick jobs" are the existing `EFFECT_SHORTCUTS`;
- favourites and recently used tools have their own views;
- each card says what the tool needs before it can run: a picked point, a skin sample, a colour,
  or that it moves pixels.

**Use on this photo** goes through one small store (`ui/src/state/effectRequest.ts`) and does not
apply anything:

- it records the tool, plus a quick job's starting point, and switches to Auto edit;
- `PhotoStudio` opens Retouch when a request is waiting;
- `NativeRetouchWorkspace` takes the request only once the saved stack has loaded, then chooses
  the tool exactly as the Tool list would and applies the patch;
- it consumes that request and only that one.

Nothing is saved until **Apply retouch**. Three cases are handled:

- **An unsaved draft is never replaced.** The request waits, and the photographer is told to apply
  or discard the draft first.
- **Without a collection or a photo in front of you, the screen is for browsing.** Every Use button
  is disabled and the screen says why.
- **Favourites and recent tools are per viewer.** They are kept in browser storage, which is a
  convenience: when storage is unavailable the screen still works, with empty lists.

## The twenty tools

| Grooming (`grooming.rs`) | Stylised colour (`stylise.rs`) | Optics and distortion (`distort.rs`) | Engraving (`engrave.rs`) | Sparkle (`sparkle.rs`) |
|---|---|---|---|---|
| Darken lashes & liner | Channel swap | Iris blur | Emboss | Sparkle |
| Fill thin hair or brows | Sunset glow | Double exposure echo | Crosshatch | Fairy lights |
| Match grain | Day to night | Ripple | | Rainbow prism |
| | Lomo | Frosted glass | | |
| | Instant film | Pixel sort | | |
| | Pop art | VHS tape | | |

All twenty are held to the shared effect tests over the 88-tool list:

- nothing outside the selection moves;
- nothing changes at zero strength or when the operation is off;
- the same pixels come out on every machine.

Each tool also has its own test of what it is for.

## Decisions

- **Match grain measures the photograph's own grain.** It takes the median absolute difference
  between a pixel and its four neighbours on a sparse grid over the whole frame, in the display
  encoding, and adds roughly Gaussian grain of that size. An over-smoothed cheek gets back the
  texture of the photograph it sits in, not a fixed amount. A test checks the added grain is within
  a factor of three of the frame's own.
- **Lash darken and hair fill read the selection, not a constant.** A lash is darker than its
  blurred surroundings, and only that difference is deepened, so the lid stays exactly as it was.
  Hair fill takes the hair colour as the mean of the darkest third of the selection and fills only
  what is lighter than it.
- **Day to night keeps its lights lit.** Pixels in the top tones are blended back toward their own
  colour, so lamps and windows stay bright while the rest turns to moonlight.
- **Pixel sort sorts only mid-tone runs.** Pixels darker than the threshold or brighter than 0.95
  break a run, which keeps the subject's darkest and brightest parts in place.
- **Ripple and frosted glass are warps.** They move pixels, so like fisheye and liquify they fall
  to nothing at the ellipse as `(1 - d^2)^2`, and `Tool::is_warp` makes them refuse an inverted
  selection.
- **Sparkle finds glints rather than placing stars.** Each cell of the selection may hold one star,
  on its brightest pixel, and only when that pixel is a highlight. There are at most 400 stars.
  Rays thicken with their length so a star looks the same at any resolution, and a pixel only
  looks at stars it could be on, so the cost stays bounded on a full-size photograph. A flat area
  gets no stars at all, and a test checks that.
- **Fairy lights and the scattered effects are seeded by the selection.** Moving the ellipse makes
  a different pattern, and the same recipe makes the same one.

## What this does not do

None of these tools finds its area on its own or creates picture content. Visual quality against
any other product has not been measured. The Effects screen opens a tool; it does not preview
each effect on your photograph.
