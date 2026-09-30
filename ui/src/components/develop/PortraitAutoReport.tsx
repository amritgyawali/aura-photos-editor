import type { RecipeDto } from '../../ipc/types';

type FaceDecision = {
  face: number; status: string; confidence: number; reason: string; strengths: number[];
  findings: string[]; spotsHealed: number; marksKept: number;
};
type StepSummary = { step: number; title: string; detail: string; operations: number };
type SceneSummary = { kind: string; decisions: string[] };
type Report = { message: string; faces: FaceDecision[]; steps: StepSummary[]; scene: SceneSummary | null };

const isRecord = (value: unknown): value is Record<string, unknown> => Boolean(value && typeof value === 'object');
const strings = (value: unknown): string[] => Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string') : [];
const count = (value: unknown): number => typeof value === 'number' && Number.isInteger(value) && value >= 0 ? value : 0;

function readReport(recipe: RecipeDto | null | undefined): Report | null {
  if (!recipe?.body) return null;
  try {
    const report: unknown = JSON.parse(recipe.body).studio_portrait_auto_v1;
    if (!isRecord(report) || typeof report.message !== 'string') return null;
    const rows: unknown[] = Array.isArray(report.assessments) ? report.assessments : [];
    const faces = rows.filter((row): row is Record<string, unknown> => isRecord(row)
      && typeof row.face === 'number' && Number.isInteger(row.face) && row.face > 0
      && (row.status === 'retouched' || row.status === 'skipped')
      && typeof row.confidence === 'number' && Number.isFinite(row.confidence) && row.confidence >= 0 && row.confidence <= 1
      && typeof row.reason === 'string'
      && Array.isArray(row.strengths) && row.strengths.length === 3
      && row.strengths.every((v: unknown) => typeof v === 'number' && Number.isFinite(v) && v >= 0 && v <= 1))
      .map(row => ({
        face: row.face as number, status: row.status as string, confidence: row.confidence as number,
        reason: row.reason as string, strengths: row.strengths as number[],
        findings: strings(row.findings), spotsHealed: count(row.spotsHealed), marksKept: count(row.marksKept),
      }));
    const steps = (Array.isArray(report.steps) ? report.steps : []).filter((s: unknown): s is StepSummary => isRecord(s)
      && typeof s.step === 'number' && typeof s.title === 'string' && typeof s.detail === 'string' && typeof s.operations === 'number');
    const scene = isRecord(report.scene) && typeof report.scene.kind === 'string'
      ? { kind: report.scene.kind, decisions: strings(report.scene.decisions) } : null;
    return { message: report.message, faces, steps, scene };
  } catch { /* A legacy recipe may have no automatic portrait report. */ }
  return null;
}

export function portraitMessage(recipe: RecipeDto | null | undefined): string | null {
  return readReport(recipe)?.message ?? null;
}

export function PortraitAutoReport({ recipe }: { recipe: RecipeDto | null }) {
  const report = readReport(recipe);
  if (!report?.message) return null;
  return <div className="lr-hint auto-edit-report">
    <p role="status">Last automatic pass: {report.message}</p>
    {report.steps.length > 0 && <details open><summary>Automatic steps ({report.steps.length})</summary>
      <p>Each step was saved separately. Undo walks back one step at a time, or use “Go back to here” in the edit history; then adjust anything by hand.</p>
      <ol className="auto-edit-steps">{report.steps.map(step => <li key={step.step}>
        <strong>{step.title}</strong>{step.operations > 0 && ` · ${step.operations} editable operation${step.operations === 1 ? '' : 's'}`}
        <p>{step.detail}</p>
      </li>)}</ol>
    </details>}
    {report.scene && <details><summary>Why these light and colour settings ({report.scene.kind})</summary>
      <ul>{report.scene.decisions.map(decision => <li key={decision}>{decision}</li>)}</ul>
    </details>}
    {report.faces.length > 0 && <details><summary>Automatic decisions by face ({report.faces.length})</summary>
      <p>Detection confidence describes finding a face, not the quality of the edit. Each saved operation can be adjusted in Retouch.</p>
      <ol>{report.faces.map((face, index) => <li key={`${face.face}-${index}`}>
        <strong>Face {face.face} · {face.status}</strong> · Detection confidence {Math.round(face.confidence * 100)}%
        <p>{face.reason}</p>
        {face.status === 'retouched' && <p>Texture {Math.round((face.strengths[0] ?? 0) * 100)}% · Tone {Math.round((face.strengths[1] ?? 0) * 100)}% · Light {Math.round((face.strengths[2] ?? 0) * 100)}%</p>}
        {face.findings.length > 0 && <ul>{face.findings.map(finding => <li key={finding}>{finding}</li>)}</ul>}
      </li>)}</ol>
    </details>}
  </div>;
}
