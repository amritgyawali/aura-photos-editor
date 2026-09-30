import type { RecipeDto } from '../../ipc/types';

type FaceDecision = { face: number; status: string; confidence: number; reason: string; strengths: number[] };
function readReport(recipe: RecipeDto | null | undefined): { message: string; faces: FaceDecision[] } | null {
  if (!recipe?.body) return null;
  try {
    const report: unknown = JSON.parse(recipe.body).studio_portrait_auto_v1;
    if (report && typeof report === 'object' && 'message' in report && typeof report.message === 'string') {
      const rows: unknown[] = 'assessments' in report && Array.isArray(report.assessments) ? report.assessments : [];
      const faces = rows.filter((row): row is FaceDecision => Boolean(row && typeof row === 'object'
        && 'face' in row && typeof row.face === 'number' && Number.isInteger(row.face) && row.face > 0
        && 'status' in row && (row.status === 'retouched' || row.status === 'skipped')
        && 'confidence' in row && typeof row.confidence === 'number' && Number.isFinite(row.confidence) && row.confidence >= 0 && row.confidence <= 1
        && 'reason' in row && typeof row.reason === 'string'
        && 'strengths' in row && Array.isArray(row.strengths) && row.strengths.length === 3
        && row.strengths.every((v: unknown) => typeof v === 'number' && Number.isFinite(v) && v >= 0 && v <= 1)));
      return { message: report.message, faces };
    }
  } catch { /* A legacy recipe may have no automatic portrait report. */ }
  return null;
}

export function portraitMessage(recipe: RecipeDto | null | undefined): string | null {
  return readReport(recipe)?.message ?? null;
}

export function PortraitAutoReport({ recipe }: { recipe: RecipeDto | null }) {
  const report = readReport(recipe);
  if (!report?.message) return null;
  return <div className="lr-hint">
    <p role="status">Last automatic pass: {report.message}</p>
    {report.faces.length > 0 && <details><summary>Automatic decisions by face ({report.faces.length})</summary>
      <p>Detection confidence describes finding a face, not the quality of the edit. Each saved operation can be adjusted in Retouch.</p>
      <ol>{report.faces.map((face, index) => <li key={`${face.face}-${index}`}>
        <strong>Face {face.face} · {face.status}</strong> · Detection confidence {Math.round(face.confidence * 100)}%
        <p>{face.reason}</p>
        {face.status === 'retouched' && <p>Texture {Math.round((face.strengths[0] ?? 0) * 100)}% · Tone {Math.round((face.strengths[1] ?? 0) * 100)}% · Light {Math.round((face.strengths[2] ?? 0) * 100)}%</p>}
      </li>)}</ol>
    </details>}
  </div>;
}
