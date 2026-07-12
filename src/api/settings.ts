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

export interface BackupInfo {
  file_name: string;
  created_at: string;
  kind: "daily" | "pre-migration" | "manual" | "before-restore";
  size_bytes: number;
}

export interface BackupCatalog {
  items: BackupInfo[];
  retention_limit: number;
  older_retained: number;
}

export const backupsList = () => call<BackupCatalog>("backups_list");
export const backupCreate = () => call<BackupInfo>("backup_create");
export const backupRestore = (file_name: string) =>
  call<BackupInfo>("backup_restore", { fileName: file_name });
