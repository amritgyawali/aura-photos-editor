export type InternalAiProfile = {
  id: string; endpoint: string; model: string; vision: boolean;
  inputUsdPerMtok: number | null; outputUsdPerMtok: number | null; status: string;
};
export type InternalAiStatus = { profiles: InternalAiProfile[]; checkedDay: string; sourceAvailable: boolean };
