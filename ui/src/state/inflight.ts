/**
 * Every request the window has sent to the engine and is still waiting on, and the ones that
 * finished a moment ago. ADR-0188.
 *
 * The status bar reads this once a second. It is a plain module rather than a store on purpose:
 * a grid scrolling a wedding sends `get_preview` many times a second, and a React update per
 * request would make the indicator the busiest thing on the screen. Nothing subscribes; the
 * reader asks.
 */

export type InflightRequest = {
  id: number;
  command: string;
  /** `Date.now()` when it was sent. */
  startedAt: number;
  /** A high-frequency read (`get_preview`, ...), shown only when it is slow. */
  quiet: boolean;
};

export type FinishedRequest = {
  id: number;
  command: string;
  startedAt: number;
  ms: number;
  ok: boolean;
  /** The engine's message, when it refused. */
  message: string | null;
};

const MAX_RECENT = 60;

let nextId = 1;
const running = new Map<number, InflightRequest>();
let recent: FinishedRequest[] = [];
/** When any request other than a quiet read last started or ended. */
let lastActivityAt = 0;

/** Commands the indicator itself sends: listing them would show the watcher watching. */
const SELF = new Set(['activity_status', 'ui_audit']);

export function beginRequest(command: string, quiet: boolean, now = Date.now()): number {
  const id = nextId++;
  if (SELF.has(command)) return id;
  running.set(id, { id, command, startedAt: now, quiet });
  if (!quiet) lastActivityAt = now;
  return id;
}

export function endRequest(id: number, ok: boolean, message: string | null = null, now = Date.now()): void {
  const request = running.get(id);
  if (!request) return;
  running.delete(id);
  const ms = now - request.startedAt;
  // A quiet read is history only when it was slow or failed: those are the ones worth seeing.
  if (request.quiet && ok && ms < 3000) return;
  lastActivityAt = now;
  recent = [...recent, { id, command: request.command, startedAt: request.startedAt, ms, ok, message }].slice(-MAX_RECENT);
}

/** Requests still waiting for an answer, oldest first. */
export function inflightRequests(): InflightRequest[] {
  return [...running.values()];
}

/** The last requests that finished, oldest first. */
export function recentRequests(): FinishedRequest[] {
  return recent;
}

/** When a request other than a quiet read last started or finished; 0 when none has. */
export function lastRequestActivity(): number {
  return lastActivityAt;
}

/** For tests. */
export function resetInflight(): void {
  running.clear();
  recent = [];
  lastActivityAt = 0;
  nextId = 1;
}

const NAMES: Record<string, string> = {
  'plugin:dialog|open': 'Waiting for you to choose in the file dialog',
  full_edit_photo: 'Full automatic edit of a photo',
  full_edit_picture: 'Picture of the edited photo',
  auto_advanced_retouch: 'Advanced retouch',
  photo_auto_edit: 'AI auto edit',
  auto_retouch: 'Auto retouch',
  enhance_portrait: 'Portrait enhance',
  start_photo_cull: 'Starting the cull',
  photo_cull_status: 'Checking the cull',
  photo_cull_review: 'Reading the cull review',
  automatic_start: 'Starting the automatic workflow',
  one_click_finish: 'Starting Finish a folder',
  one_click_status: 'Checking the folder run',
  analyse_reference_style: 'Analysing the reference style',
  get_preview: 'Loading a preview',
  render_preview: 'Rendering a preview',
  render_image: 'Rendering the photo',
  photo_original: 'Reading the original photo',
  list_images: 'Loading the photo list',
  list_projects: 'Loading collections',
  import_start: 'Starting the import',
  export_run_watermarked: 'Exporting',
  sync_settings: 'Copying settings to other photos',
  find_glare: 'Looking for glare',
  studio_finish: 'Reading the finishing tools',
  save_studio_finish: 'Saving the finish',
  save_studio_color: 'Saving colour',
  save_color_lab: 'Saving colour lab',
  mask_workflow: 'Working on a mask',
  create_local_mask: 'Creating a mask',
  retouch_intensity: 'Changing retouch strength',
  evoto_faces: 'Finding faces',
  evoto_export: 'Exporting',
  blink_check: 'Checking for closed eyes',
};

/** `full_edit_photo` -> `Full automatic edit of a photo`; unknown names read as words. */
export function describeCommand(command: string): string {
  const known = NAMES[command];
  if (known) return known;
  const words = command.replace(/^plugin:/, '').replace(/[_|:]+/g, ' ').trim();
  return words ? words.charAt(0).toUpperCase() + words.slice(1) : command;
}
