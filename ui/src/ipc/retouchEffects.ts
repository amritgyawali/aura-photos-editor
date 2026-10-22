/**
 * The selected effects (ADR-0131): liquify, optics, colour, light and portrait tools. Each is a
 * native retouch operation with its own selection, saved in the recipe and undoable. This file
 * says what each one is called, what its sliders mean and where it starts.
 */
export const EFFECT_TOOLS = [
  ['liquify_push', 'Liquify · push', 'Liquify', 'Moves what is inside the ellipse in the direction you set, tapering to nothing at the edge. Use it to tuck in a waistline, lift a jaw or smooth a hairline. It changes photographed shape and is never used by automatic retouch.'],
  ['liquify_bloat', 'Liquify · enlarge (eyes, hair volume)', 'Liquify', 'Enlarges what is at the centre of the ellipse - an eye, a curl, a flower - tapering smoothly to the edge.'],
  ['liquify_pucker', 'Liquify · shrink (nose, waist)', 'Liquify', 'Shrinks what is at the centre of the ellipse toward its middle, tapering smoothly to the edge.'],
  ['liquify_twirl', 'Liquify · twirl', 'Liquify', 'Rotates what is inside the ellipse around its centre; strongest in the middle.'],
  ['spin_blur', 'Spin blur', 'Blur & optics', 'Rotational blur around a point - wheels, a twirling dress, a spinning bouquet toss.'],
  ['pixelate', 'Pixelate (privacy)', 'Blur & optics', 'Square blocks over a face, a number plate or a screen that must not be shown.'],
  ['privacy_blur', 'Blur (privacy / soften)', 'Blur & optics', 'An even, strong blur inside the selection.'],
  ['soft_focus', 'Soft focus · diffusion filter', 'Blur & optics', 'Highlights bloom softly into their surroundings, like a mist or diffusion filter on the lens.'],
  ['orton_glow', 'Orton glow · dreamy', 'Blur & optics', 'A bright, blurred copy glowed over the photograph: dreamy landscapes and soft romantic portraits.'],
  ['local_sharpen', 'Sharpen (local)', 'Blur & optics', 'Brings out detail in eyes, jewellery, lace or hair without sharpening the skin around them.'],
  ['local_denoise', 'Noise reduction (local)', 'Blur & optics', 'Removes colour speckle and grain inside the selection while keeping edges sharp.'],
  ['hue_shift', 'Hue shift', 'Colour', 'Turns the colours inside the selection around the colour wheel while keeping their brightness - change a dress, a wall or flowers.'],
  ['invert_colors', 'Invert colours', 'Colour', 'Turns the selection into its negative.'],
  ['chroma_key', 'Green screen · replace colour', 'Colour', 'Pick the screen colour on the photo; it is replaced with the colour you choose, and its spill on the subject is removed. Without a colour, only the spill is removed.'],
  ['vein_reduce', 'Reduce visible veins', 'Skin', 'Evens bluish, darker veins on hands, feet, legs or temples toward the skin around them. Skin that is not bluer than its surroundings is left exactly as it was.'],
  ['color_unify', 'Unify colour (hair, clothes)', 'Colour', 'Pulls every colour inside the selection toward its own average: patchy hair dye, a faded shirt, uneven paint.'],
  ['local_white_balance', 'Local white balance', 'Colour', 'Pick something that should be neutral grey or white inside the area; the area is corrected to make it neutral. For mixed light - a window on one side, tungsten on the other.'],
  ['light_leak', 'Light leak', 'Light & effects', 'A soft coloured leak of light from a point, like old film cameras.'],
  ['glitch', 'Glitch', 'Light & effects', 'Splits the colour channels apart and offsets slices of the picture.'],
  ['haze', 'Haze · fog', 'Light & effects', 'Atmospheric haze; use with a gradient selection for fog that thickens with distance.'],
  ['starburst', 'Sun rays · starburst', 'Light & effects', 'Rays of light from a point: the sun, a lamp, a candle.'],
  ['halation', 'Film halation', 'Light & effects', 'The red-orange glow film shows around bright lights.'],
  ['catchlight', 'Add catchlight', 'Details', 'Places a soft light in the eye: draw a small ellipse on the iris where the reflection of the light belongs.'],
  ['eye_glow_fix', 'Pet eye / eye glow fix', 'Details', 'Darkens glowing pupils of any colour - green or gold flash glow in animals, and red-eye in people.'],
  ['lip_gloss', 'Lip gloss', 'Details', 'Lifts the highlights the lips already have into a glossy shine.'],
  ['freckles', 'Add freckles', 'Skin', 'Adds natural-looking freckles inside the selection. Density and size are adjustable; moving the selection makes a different pattern.'],
  ['floor_reflection', 'Floor reflection', 'Scene', 'Mirrors what stands above the selection into it, fading downward - for a glossy studio floor or still water. Select the floor area; its top edge is the mirror line.'],
  ['drop_shadow', 'Drop shadow', 'Scene', 'Casts a soft shadow of the selected subject onto what is around it, offset in the direction you set. Never darkens the subject itself.'],
  // Practical repairs and creative looks (ADR-0132).
  ['dust_scratches', 'Dust & scratches', 'Fix & clean up', 'Removes sensor dust, specks and fine scratches: anything that stands out from the median of its surroundings by more than the threshold. Edges wider than the radius stay sharp.'],
  ['jpeg_cleanup', 'JPEG artifact cleanup', 'Fix & clean up', 'Smooths the small 8 x 8 block steps of a heavily compressed JPEG - a downloaded or messaged photo - while real edges stay.'],
  ['deband', 'Remove banding', 'Fix & clean up', 'Smooths the visible steps in skies and smooth backdrops, with a fine fixed grain so they do not come back in the exported file. Textured areas are left alone.'],
  ['auto_levels', 'Auto levels (selection)', 'Fix & clean up', 'Stretches the darkest and brightest tones inside the selection to a full range - a hazy window, a flat background, a dull dress.'],
  ['auto_neutral', 'Remove colour cast (selection)', 'Fix & clean up', 'Removes the average colour cast inside the selection, for example a green tint from foliage on a white dress.'],
  ['flatten_light', 'Even out uneven lighting', 'Fix & clean up', 'Measures the gradient of light across the selection - a backdrop lit from one side, a vignetted wall - and evens it, keeping the detail.'],
  ['hdr_detail', 'HDR detail · structure', 'Fix & clean up', 'Brings out large-scale detail and depth - clouds, architecture, fabric folds - and can open the shadows.'],
  ['bokeh_highlights', 'Bokeh highlights', 'Creative looks', 'Turns lights and bright points in the selection into round out-of-focus discs, like fairy lights behind a couple.'],
  ['tilt_shift', 'Tilt-shift · miniature', 'Creative looks', 'Keeps a band through the centre of the ellipse sharp and blurs away from it, with a little extra colour: a venue from above starts to look like a model.'],
  ['duotone', 'Duotone', 'Creative looks', 'Maps the tones in the selection between a dark shadow colour and the colour you choose.'],
  ['posterize', 'Posterize', 'Creative looks', 'Reduces each colour to a few flat levels, like a screen print.'],
  ['halftone', 'Halftone print', 'Creative looks', 'Redraws the selection as printed dots on paper, in black or a colour you choose.'],
  ['pencil_sketch', 'Pencil sketch', 'Creative looks', 'Turns the selection into a pencil drawing; keep some of the colour for a tinted sketch.'],
  ['oil_paint', 'Oil paint', 'Creative looks', 'Paints the selection in flat brush strokes while keeping its edges.'],
  ['cross_process', 'Cross-process', 'Creative looks', 'The punchy colour of slide film developed in the wrong chemistry: contrasty, with yellow highlights.'],
  ['infrared', 'Infrared', 'Creative looks', 'An infrared look: foliage turns white. From black and white to false colour.'],
  ['mirror_symmetry', 'Mirror symmetry', 'Creative looks', 'Mirrors one half of the ellipse onto the other, side to side or top to bottom.'],
  ['lens_flare', 'Lens flare', 'Creative looks', 'A lens flare from a light you pick, with coloured ghosts along the line through the frame.'],
  ['god_rays', 'God rays', 'Creative looks', 'Beams of light streaming from a point you pick through the bright gaps in front of it - trees, windows, a doorway.'],
  ['weather', 'Snow & rain', 'Creative looks', 'Adds falling snow, or rain streaks at the angle you set.'],
] as const;

export type EffectTool = typeof EFFECT_TOOLS[number][0];
type Key = 'tone' | 'warmth' | 'tint' | 'texture' | 'radius';
/** `toggle` is a checkbox: `max` when on, `min` when off. */
export type EffectParam = { key: Key; label: string; min: number; max: number; step: number; show: (v: number) => string; toggle?: boolean };

const pct = (v: number) => `${Math.round(v * 100)}%`;
const signed = (v: number) => `${v > 0 ? '+' : ''}${Math.round(v * 100)}%`;
const deg = (scale: number) => (v: number) => `${Math.round(v * scale)}°`;
const size = (v: number) => `${(v * 100).toFixed(2)}% of short edge`;
const radius = (label: string): EffectParam => ({ key: 'radius', label, min: .0005, max: .05, step: .0005, show: size });
const unit = (key: Key, label: string): EffectParam => ({ key, label, min: 0, max: 1, step: .01, show: pct });
const both = (key: Key, label: string, show = signed): EffectParam => ({ key, label, min: -1, max: 1, step: .01, show });

export const EFFECT_PARAMS: Record<EffectTool, EffectParam[]> = {
  liquify_push: [both('warmth', 'Push right (+) or left (−)'), both('tint', 'Push down (+) or up (−)')],
  liquify_bloat: [],
  liquify_pucker: [],
  liquify_twirl: [both('warmth', 'Twirl clockwise (−) or anticlockwise (+)', deg(180))],
  spin_blur: [{ ...unit('tone', 'Spin angle'), show: deg(30) }],
  pixelate: [radius('Block size')],
  privacy_blur: [radius('Blur radius')],
  soft_focus: [radius('Glow spread'), unit('tone', 'Overall softness')],
  orton_glow: [radius('Glow spread'), unit('tone', 'Glow brightness')],
  local_sharpen: [radius('Detail size'), { key: 'texture', label: 'Sharpening', min: 0, max: 2, step: .01, show: pct }],
  local_denoise: [radius('Smoothing radius'), unit('tone', 'Luminance noise reduction')],
  hue_shift: [both('warmth', 'Hue', deg(180))],
  invert_colors: [],
  chroma_key: [unit('tone', 'Tolerance')],
  vein_reduce: [radius('Vein width'), unit('tone', 'Even the darkness too')],
  color_unify: [],
  local_white_balance: [],
  light_leak: [unit('tone', 'Brightness')],
  glitch: [radius('Channel offset'), unit('tone', 'Slice displacement')],
  haze: [unit('tone', 'Thickness')],
  starburst: [{ ...unit('tone', 'Number of rays'), show: v => `${4 + Math.round(v * 6) * 2}` }],
  halation: [radius('Glow spread'), unit('tone', 'Reaches dimmer lights')],
  catchlight: [],
  eye_glow_fix: [unit('tone', 'Pupil darkness')],
  lip_gloss: [radius('Highlight size'), unit('tone', 'Shine')],
  freckles: [radius('Freckle size'), unit('tone', 'Density')],
  floor_reflection: [unit('tone', 'Reflection length')],
  drop_shadow: [both('warmth', 'Shadow right (+) or left (−)'), both('tint', 'Shadow down (+) or up (−)'), radius('Softness'), unit('tone', 'Darkness')],
  dust_scratches: [radius('Largest speck'), unit('tone', 'Threshold')],
  jpeg_cleanup: [unit('tone', 'Largest step treated as an artifact')],
  deband: [radius('Smoothing radius')],
  auto_levels: [],
  auto_neutral: [],
  flatten_light: [],
  hdr_detail: [radius('Structure size'), { key: 'texture', label: 'Detail', min: 0, max: 2, step: .01, show: pct }, unit('tone', 'Open shadows')],
  bokeh_highlights: [radius('Disc size'), unit('tone', 'Reaches dimmer lights')],
  tilt_shift: [radius('Blur'), unit('tone', 'Sharp band width')],
  duotone: [both('warmth', 'Shadow hue', deg(180)), unit('tone', 'Contrast')],
  posterize: [{ ...unit('tone', 'Levels'), show: v => String(2 + Math.round(v * 14)) }],
  halftone: [radius('Dot size')],
  pencil_sketch: [radius('Line softness'), unit('tone', 'Keep colour')],
  oil_paint: [radius('Brush size')],
  cross_process: [],
  infrared: [unit('tone', 'Black and white to false colour')],
  mirror_symmetry: [{ key: 'warmth', label: 'Keep the right (or lower) half', min: -1, max: 1, step: 2, show: v => v > 0 ? 'on' : 'off', toggle: true }, { key: 'tint', label: 'Mirror top and bottom', min: -1, max: 1, step: 2, show: v => v > 0 ? 'on' : 'off', toggle: true }],
  lens_flare: [radius('Ghost size'), unit('tone', 'Brightness')],
  god_rays: [unit('tone', 'Length and reach')],
  weather: [radius('Flake or drop size'), unit('tone', 'Density'), { key: 'tint', label: 'Rain instead of snow', min: -1, max: 1, step: 2, show: v => v > 0 ? 'on' : 'off', toggle: true }, both('warmth', 'Rain angle', deg(45))],
};

/** Tools that read a point on the photo: required for the first two, optional for the rest. */
export const EFFECT_SOURCE: Partial<Record<EffectTool, { required: boolean; label: string }>> = {
  chroma_key: { required: true, label: 'Pick the screen colour on the photo' },
  local_white_balance: { required: true, label: 'Pick something neutral on the photo' },
  spin_blur: { required: false, label: 'Pick the spin centre (default: the ellipse centre)' },
  light_leak: { required: false, label: 'Pick where the light comes from (default: the ellipse centre)' },
  starburst: { required: false, label: 'Pick the light source (default: the ellipse centre)' },
  lens_flare: { required: false, label: 'Pick the light causing the flare (default: the ellipse centre)' },
  god_rays: { required: false, label: 'Pick where the light comes from (default: the ellipse centre)' },
};

/** Tools with a chosen colour, and the colour they start with. */
export const EFFECT_COLOR: Partial<Record<EffectTool, { label: string; initial: [number, number, number]; optional: boolean }>> = {
  chroma_key: { label: 'Replacement colour', initial: [.85, .85, .85], optional: true },
  light_leak: { label: 'Light colour', initial: [1, .55, .25], optional: true },
  haze: { label: 'Haze colour', initial: [.82, .84, .87], optional: true },
  starburst: { label: 'Light colour', initial: [1, .97, .9], optional: true },
  catchlight: { label: 'Light colour', initial: [1, 1, 1], optional: true },
  duotone: { label: 'Highlight colour', initial: [.95, .82, .6], optional: false },
  halftone: { label: 'Ink colour', initial: [.05, .05, .08], optional: true },
  god_rays: { label: 'Light colour', initial: [1, .88, .65], optional: true },
};

/** Where each tool starts when chosen. */
export const EFFECT_DEFAULTS: Record<EffectTool, { amount: number; tone: number; warmth: number; tint: number; texture: number; radius: number; feather: number }> = {
  liquify_push: { amount: .5, tone: .5, warmth: .3, tint: 0, texture: 1, radius: .01, feather: .6 },
  liquify_bloat: { amount: .35, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .6 },
  liquify_pucker: { amount: .35, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .6 },
  liquify_twirl: { amount: .5, tone: .5, warmth: .3, tint: 0, texture: 1, radius: .01, feather: .6 },
  spin_blur: { amount: 1, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .4 },
  pixelate: { amount: 1, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .02, feather: 0 },
  privacy_blur: { amount: 1, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .02, feather: .2 },
  soft_focus: { amount: .5, tone: .3, warmth: 0, tint: 0, texture: 1, radius: .015, feather: .5 },
  orton_glow: { amount: .5, tone: .4, warmth: 0, tint: 0, texture: 1, radius: .02, feather: .5 },
  local_sharpen: { amount: .7, tone: .5, warmth: 0, tint: 0, texture: .8, radius: .002, feather: .5 },
  local_denoise: { amount: .8, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .004, feather: .5 },
  hue_shift: { amount: 1, tone: .5, warmth: .25, tint: 0, texture: 1, radius: .01, feather: .4 },
  invert_colors: { amount: 1, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .2 },
  chroma_key: { amount: 1, tone: .4, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .2 },
  vein_reduce: { amount: .7, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .008, feather: .5 },
  color_unify: { amount: .5, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .4 },
  local_white_balance: { amount: 1, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .6 },
  light_leak: { amount: .6, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .8 },
  glitch: { amount: 1, tone: .4, warmth: 0, tint: 0, texture: 1, radius: .006, feather: .1 },
  haze: { amount: .5, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .8 },
  starburst: { amount: .6, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .8 },
  halation: { amount: .6, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .5 },
  catchlight: { amount: .8, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .7 },
  eye_glow_fix: { amount: 1, tone: .6, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .3 },
  lip_gloss: { amount: .6, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .004, feather: .5 },
  freckles: { amount: .7, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .004, feather: .6 },
  floor_reflection: { amount: .5, tone: .4, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .1 },
  drop_shadow: { amount: .6, tone: .5, warmth: .4, tint: .4, texture: 1, radius: .015, feather: .2 },
  dust_scratches: { amount: 1, tone: .3, warmth: 0, tint: 0, texture: 1, radius: .002, feather: .2 },
  jpeg_cleanup: { amount: 1, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .2 },
  deband: { amount: 1, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .4 },
  auto_levels: { amount: .8, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .4 },
  auto_neutral: { amount: .8, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .4 },
  flatten_light: { amount: .8, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .3 },
  hdr_detail: { amount: .7, tone: .3, warmth: 0, tint: 0, texture: .6, radius: .01, feather: .4 },
  bokeh_highlights: { amount: .8, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .015, feather: .3 },
  tilt_shift: { amount: 1, tone: .25, warmth: 0, tint: 0, texture: 1, radius: .012, feather: .2 },
  duotone: { amount: 1, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .3 },
  posterize: { amount: 1, tone: .3, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .2 },
  halftone: { amount: 1, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .006, feather: .2 },
  pencil_sketch: { amount: 1, tone: .2, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .2 },
  oil_paint: { amount: 1, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .003, feather: .2 },
  cross_process: { amount: .8, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .3 },
  infrared: { amount: 1, tone: .3, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .3 },
  mirror_symmetry: { amount: 1, tone: .5, warmth: -1, tint: -1, texture: 1, radius: .01, feather: .1 },
  lens_flare: { amount: .7, tone: .5, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .6 },
  god_rays: { amount: .8, tone: .6, warmth: 0, tint: 0, texture: 1, radius: .01, feather: .6 },
  weather: { amount: 1, tone: .4, warmth: .2, tint: -1, texture: 1, radius: .004, feather: .5 },
};

/** Named starting points for common jobs; each one is just one of the tools above, set up. */
export const EFFECT_SHORTCUTS: { label: string; tool: EffectTool; patch: Partial<typeof EFFECT_DEFAULTS[EffectTool]> }[] = [
  { label: 'Enlarge eyes', tool: 'liquify_bloat', patch: { amount: .25, feather: .7 } },
  { label: 'Add hair volume', tool: 'liquify_bloat', patch: { amount: .4, feather: .8 } },
  { label: 'Slim a nose', tool: 'liquify_pucker', patch: { amount: .25, feather: .7 } },
  { label: 'Lift a jawline', tool: 'liquify_push', patch: { warmth: 0, tint: -.25, amount: .5 } },
  { label: 'Hide a face or plate', tool: 'pixelate', patch: { radius: .03 } },
  { label: 'Dreamy glow', tool: 'orton_glow', patch: { amount: .6, tone: .5 } },
  { label: 'Sharpen the eyes', tool: 'local_sharpen', patch: { texture: 1.2, radius: .0015 } },
];

const EFFECT_IDS = new Set<string>(EFFECT_TOOLS.map(t => t[0]));
export const isEffectTool = (tool: string): tool is EffectTool => EFFECT_IDS.has(tool);
/** The warps move pixels, so an inverted selection would warp the whole photo. */
export const isWarpTool = (tool: string) => tool === 'reshape' || tool.startsWith('liquify_');
