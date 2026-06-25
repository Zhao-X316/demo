import { call } from "./client";

export interface RecitationConfig {
  accuracy_threshold: number;
  use_pinyin: boolean;
  ignore_tone: boolean;
  remove_fillers: boolean;
  ideal_cps: number;
  quality_a_min: number;
  quality_b_min: number;
  makeup_offset_days: number;
}

export interface VolcanoCreds {
  app_id: string;
  access_token: string;
  secret: string;
  cluster: string;
}

export const configGet = () => call<RecitationConfig>("config_get");
export const configSet = (cfg: RecitationConfig) => call<void>("config_set", { cfg });
export const secretsGet = () => call<VolcanoCreds>("secrets_get");
export const secretsSet = (creds: VolcanoCreds) => call<void>("secrets_set", { creds });
