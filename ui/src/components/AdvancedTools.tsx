import { useState, type ReactNode } from 'react';
import { AiSetup } from './AiSetup';
import { LogPanel } from './LogPanel';
import { PhotoAnalysis } from './PhotoAnalysis';
import { CameraMatchPanel } from './camera/CameraMatchPanel';
import { CleanupPanel } from './cleanup/CleanupPanel';
import { CullView } from './cull/CullView';
import { DevelopWorkspace } from './develop/DevelopWorkspace';
import { ToneReviewQueue } from './develop/ToneReviewQueue';
import { PortraitRetouch } from './develop/PortraitRetouch';
import { NativeRetouchWorkspace } from './develop/NativeRetouchWorkspace';
import { Inspector } from './explain/Inspector';
import { FilterChips } from './explain/FilterChips';
import { MomentStack } from './grid/MomentStack';
import { PeoplePanel } from './people/PeoplePanel';
import { StoryPanel } from './story/StoryPanel';
import { StylePanel } from './style/StylePanel';
import { OneClickRunner } from './workflow/OneClickRunner';

/** Mount expensive tools only when opened, within the single studio shell. */
function Tool({ title, children }: { title: string; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  return <details className="advanced-tools" onToggle={event => setOpen(event.currentTarget.open)}>
    <summary>{title}</summary>{open && children}
  </details>;
}
export function AdvancedTools({ projectId, photoId, onOpen, onError, onRefresh, onBusyChange }: {
  onBusyChange: (busy: boolean) => void; projectId: string; photoId: string | null; onOpen: (id: string) => void;
  onError: (error: { code: string; message: string } | null) => void; onRefresh: () => void;
}) {
  const [setup, setSetup] = useState(false);
  return <>
    <Tool title="Photo measurements"><PhotoAnalysis projectId={projectId} photoId={photoId} /></Tool>
    <Tool title="People"><PeoplePanel projectId={projectId} onError={onError} /></Tool>
    <Tool title="Story and moments"><StoryPanel projectId={projectId} onError={onError} /><MomentStack projectId={projectId} /></Tool>
    <Tool title="Camera matching"><CameraMatchPanel projectId={projectId} onError={onError} /></Tool>
    <Tool title="Cull and selection"><CullView projectId={projectId} onOpenImage={onOpen} /></Tool>
    <Tool title="Photo evidence"><FilterChips projectId={projectId} onSelect={ids => { if (ids[0]) onOpen(ids[0]); }} /><Inspector projectId={projectId} photoId={photoId} onSelect={onOpen} onError={onError} /></Tool>
    <Tool title="Detailed development"><DevelopWorkspace projectId={projectId} photoId={photoId} onError={onError} /><ToneReviewQueue projectId={projectId} onOpen={onOpen} onError={onError} /></Tool>
    <Tool title="Portrait regions">{photoId ? <PortraitRetouch projectId={projectId} photoId={photoId} disabled={false} onBusyChange={onBusyChange} /> : <p>Select a photo first.</p>}</Tool>
    <Tool title="Precision retouch, hair color and proportions">{photoId ? <NativeRetouchWorkspace key={`${projectId}:${photoId}`} projectId={projectId} photoId={photoId} onClose={() => onOpen(photoId)} onBusyChange={onBusyChange}/> : <p>Select a photo first.</p>}</Tool>
    <Tool title="Object cleanup"><CleanupPanel projectId={projectId} photoId={photoId} onError={onError} /></Tool>
    <Tool title="Learn a style"><StylePanel projectId={projectId} onError={onError} /></Tool>
    <Tool title="Complete collection workflow"><OneClickRunner onFinished={onRefresh} /></Tool>
    <Tool title="Provider catalogue"><button type="button" onClick={() => setSetup(true)}>Configure AI provider</button>
      {setup && <AiSetup onDone={() => setSetup(false)} onDismiss={() => setSetup(false)} onError={onError} />}</Tool>
    <Tool title="Activity log"><LogPanel /></Tool>
  </>;
}
