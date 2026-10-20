import { invoke } from './client';
import type { ColorMatchControls, ColorReference } from './colorMatch';

export type LibraryAction = 'read' | 'create_group' | 'rename_group' | 'delete_group' | 'reorder_groups'
  | 'move' | 'remove' | 'reorder_references' | 'favorite' | 'defaults' | 'reset_defaults' | 'export' | 'import';
export type LibraryRequest = {
  action: LibraryAction; expectedRevision?: string; group?: string; name?: string; referenceIds?: string[];
  order?: string[]; favorite?: boolean; controls?: ColorMatchControls; document?: string; replaceExisting?: boolean;
};
export type LibrarySnapshot = {
  groups: string[]; references: ColorReference[]; revision: string; document: string | null; changed: boolean;
};
export const referenceLibraryWorkflow = (input: LibraryRequest): Promise<LibrarySnapshot> =>
  invoke<LibrarySnapshot>('color_reference_library_workflow', { input });
