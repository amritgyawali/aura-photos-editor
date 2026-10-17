# ADR-0126: Edge-aware Shadows, Highlights and Clarity, a physical Dehaze, and Auto enhance sized from regions

Status: accepted. Date: 2026-10-17.

Four of the sliders a photographer reaches for on every kind of photograph - Shadows, Highlights,
Clarity and Dehaze - were the weakest operations in the renderer, and they were weak for one reason:
each looked at a pixel, or at a blur, when the thing the slider is about is a *region*.

- **Shadows and Highlights** were a per-pixel curve (`tonemap::tone`, at most a 1.6x change in a
  band of the tone range). A curve cannot lift a dark wall without pressing its grain together, and
  it cannot pull down a sky without pulling its white clouds below the blue around them: on a real
  photograph at Highlights -80 the clouds came out as grey blobs darker than the sky.
- **Clarity** was a log-domain unsharp mask at a fixed 16-pixel box radius. Around every strong edge
  it drew a glow two stops deep - a white halo round tree branches against a sky, a dark rim along a
  building - and because the radius was in render pixels, a 2,048-pixel preview and a 6,000-pixel
  export of the same recipe had different clarity.
- **Dehaze** subtracted a fraction of the frame's fifth-percentile dark channel from every pixel: a
  black-point shift, the same everywhere, when haze is thickest in the distance and absent in the
  foreground.

## 1. One edge-aware base, measured once over the whole frame

`crates/aura-render/src/edge_aware.rs`. A **guided filter** (He, Sun and Tang) over log luminance
gives every pixel the brightness of the region it belongs to: it follows edges much stronger than its
threshold (a window and the wall around it) and averages away texture below it (the grain of the
wall). Three bases come from it:

| Base | Radius (share of the long edge) | Edge threshold | Reads |
|---|---|---|---|
| Tone | 3.5 % | 0.10 stop² | Shadows, Highlights |
| Clarity | 2 % | 0.40 stop² | Clarity |
| Haze density | 3 % (after a 1 % dark-channel patch) | 10⁻³ on a curve-domain guide | Dehaze |

The coefficients are measured on a **512-pixel area reduction of the source frame**, taken through the
same stages the full-resolution buffer goes through before each base is read (the input half,
calibration, vignette and distortion, exposure; then the creative half for clarity's base; then
clarity for the haze). A pixel's base is `a * guide + b` with `a` and `b` read bilinearly at its
position *in the whole frame* and `guide` its own full-resolution value - the fast guided filter. So:

- **A proxy and an export look the same.** Measured: a 2,048-pixel render scaled down differs from a
  1,024-pixel render of the same photograph by 0.4 code values on average, where the edit itself
  moves pixels by about nine.
- **A tiled render stays bit-identical to a whole one.** The bases are frame-wide measurements, like
  `spatial::Stats`, and `Stats::base` carries them. The tiler measures once before the first tile.
  Applying a base reads only the pixel and its absolute position, so Clarity and Dehaze now declare a
  halo of zero rather than 48 and 24. `tests/tiling.rs` covers Dehaze as well now.
- **Edges stay at full resolution.** Only the smooth coefficient fields are low resolution.

The grid is 512 pixels rather than 1,024 because of cost, measured on a 4-core machine at preview
size: 461 ms for the three bases at 1,024, 87 ms at 512. The two grids render within 0.35 code values
of each other (99th percentile 2). The filter's box sums are prefix sums accumulated in a fixed order,
so the same frame gives the same coefficients whatever the thread count.

## 2. Shadows and Highlights act on regions

The gain is decided by the region and applied to the pixel, so the pixel's departure from its region
- its texture - is carried along unchanged. In stops from middle grey:

- **Shadows** reach full weight from three stops down, fall to nothing half a stop above middle grey,
  and ease to half in the deepest blacks (the Blacks slider's, and mostly noise). +100 lifts a shadow
  region by 1.5 stops; -100 deepens it by 1.1.
- **Highlights** ramp in from half a stop below middle grey to full at two above. -100 brings a bright
  region down 1.1 stops; +100 raises it 0.7.
- Both falling edges are wide enough that the mapping from base to new base is **monotone at full
  travel**: a region darker than its neighbour stays darker, so no region can overtake another and
  draw a halo. `the_tone_mapping_is_monotone_at_full_travel` checks it.
- **Whites and Blacks stay per-pixel.** They set where the range ends, which is a property of a value.

**Clipped regions are protected.** A display-referred file's sRGB codes 250 to 255 unroll (ADR-0117)
across more than a stop of working space, so a few codes of JPEG noise near white read as a stop of
"detail", and pulling them down drew the JPEG's 8x8 blocks into a blown sky. A frame counts as clipped
when at least 0.2 % of it sits at its own maximum; each pixel's clip proximity is measured *through
the display shoulder* (its brightest channel's display value as a share of the ceiling's, from 0.86
to 0.985) and smoothed at region scale against the same guide, so the protection has no contour of
its own. A fully clipped region receives 5 % of Highlights. Measured in display terms because in
stops the same rule would either miss the JPEG noise or protect the top half-stop of every raw frame.

## 3. Clarity without halos

Clarity raises each region's detail against the clarity base - up to about 2.5 times at +100, with
each stop of detail beyond the first boosted progressively less - and lowers it for negative values,
which is a soft glow without bloom. It is weighted toward the mid-tones so it neither clips a
highlight nor crushes a shadow.

The guided filter's base is still a little soft right beside an edge much stronger than texture, and
boosting that difference drew a smooth 0.27-stop ramp seven pixels wide at +100 - small on a test
frame, but it scales with the image. That is exactly where the filter's own `a` coefficient approaches
one, so clarity steps aside there (`a` from 0.35 to 0.85). On a clean four-stop step at +100 the
largest departure is now 0.09 stops; the blur it replaces moved the same pixels by 1.97.

## 4. Dehaze is the dark channel prior

In a clear photograph almost every patch has some channel near black; haze adds airlight to every
channel, so a patch's darkest channel measures how much airlight it carries. The airlight's colour is
the brightest half of the 0.1 % of the frame with the densest dark channel (a histogram, so ties
cannot reorder anything). Density is the dark channel normalised by the airlight, refined by the
guided filter so its edges follow the photograph's. Positive Dehaze inverts `I = J t + A (1 - t)` with
`t = 1 - 0.95 * amount * density`, floored at 0.12; negative Dehaze runs the model forward and adds
haze where there already is some.

**The model runs in display-linear terms** - each channel through the output's shoulder, so white is 1.
The first version ran in the working space and did almost nothing on a hazy aerial photograph: its
airlight was read from clipped white water, which unrolls to four times diffuse white, and every real
veil then looked like a fortieth of one.

On a synthetic landscape with a sky, a far band under thick haze and a near band under thin haze, the
measured airlight is within 0.05 of the painted one and the far band is corrected more than the near.
On real photographs the effect at the same slider value is comparable in strength to the old shift,
with the distance cleared more than the foreground and a clear blue sky deepened while its clouds
stay white.

## 5. Auto enhance sizes Shadows and Highlights from regions

`crates/aura-app/src/local_range.rs`. With the sliders acting on regions, Auto enhance asks the same
regions what they need. `edge_aware::regions` reports the share of the frame in deep-shadow regions
(more than three stops below middle grey) and their median depth, the share in bright regions that
still carry detail (above 1.3 stops and not clipped), the share that is clipped, and the 3rd and
97th percentiles of region brightness.

- **Shadows** have two tiers. A minority of shadow regions - a dark background, black fur, dark
  corners - may ask for up to +35, enough to open them without losing the background; a frame that
  is *mostly* shadow (an interior exposed for its windows) may ask for up to 35 more. Both scale with
  depth. Regions darker than 8.5 stops below middle grey are black by design and ask for nothing.
- **Highlights** grow with how close the brightest regions come to white and how much of the frame
  they cover, up to -40, and are recovered less when the bright regions are the subject itself (over
  30 % of the frame, down to 40 % at 60 %): pulling a white building down only turns it grey. Clipped
  regions are not counted.
- **The scene has the last word**: a night scene is given no lift, a high-key frame keeps its white.
- Each slider takes the **stronger** of the histogram's answer and the regions', never their sum - they
  are two readings of the same light. `GlobalPlan::highlights_shadows` is the one place the two meet,
  and both the saved recipe and the step's description read it.

On the real photographs this was tuned against: a church interior with 80 % of the frame in shadow
regions went from Shadows +18 to +52 and opened up with its windows intact; a white hotel facade went
from Highlights -20 to -24 rather than -39; a meadow under a blown sky went from -25 to -37 with the
clipped sky left white; an ordinary blue-sky photograph and a black studio backdrop were left as the
histogram had them.

## 6. The engine string is bumped

`+local.1`. These four sliders render differently from the same recipe, so every cached preview and
every render hash that used them is stale. A recipe saved before this change opens with the same
slider values and the new rendering: a photograph edited with Highlights -60 under the old curve will
look different - its clouds will stay white - and that is the point of the change.

## What is not claimed

- No study of photographers' preferences. The tuning was done by eye on 23 public test photographs
  (the OpenCV and imageio samples, a public HDR bracket set and a 360° meadow), small JPEGs, none of
  them a raw file and none of them a wedding.
- No raw file was rendered. The clipped-region rule is argued for raw input (ADR-0117's unroll is
  where it was needed) and is not measured on one.
- The Dehaze airlight assumes the haze is the brightest dense-dark-channel thing in the frame. A frame
  with a large white object and no sky can mislead it; the floor on transmission and the slider being
  a person's choice (or, automatically, only on frames measured as hazy) are what bound the damage.
- Speed is measured on this 4-core sandbox only: about 90 ms of base measurement per preview and
  about 0.8 s more per 24-megapixel export with all four sliders set.
