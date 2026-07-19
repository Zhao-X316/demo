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

export interface MaskedVolcanoCreds {
  app_id: string;
  access_token_mask: string | null;
  secret_mask: string | null;
  cluster: string;
  ark_api_key_mask: string | null;
  ark_model: string;
}

export const configGet = () => call<RecitationConfig>("config_get");
export const configSet = (cfg: RecitationConfig) => call<void>("config_set", { cfg });
export const secretsGet = () => call<MaskedVolcanoCreds>("secrets_get");
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

export interface DiagnosticSensitiveOptions {
  include_failed_audio: boolean;
  include_failed_asr: boolean;
  include_standard_answers: boolean;
}

export interface DiagnosticPreviewItem {
  category: string;
  internal_ref: string;
  archive_name: string;
  size_bytes: number;
  available: boolean;
  reason_code: string | null;
}

export interface DiagnosticPreview {
  schema_version: number;
  sensitive_content_selected: boolean;
  selected_categories: string[];
  warning: string | null;
  preview_token: string | null;
  items: DiagnosticPreviewItem[];
  available_item_count: number;
  unavailable_item_count: number;
  total_available_bytes: number;
}

export interface DiagnosticExportResult {
  schema_version: number;
  bundle_id: string;
  file_name: string;
  sha256: string;
  size_bytes: number;
  generated_at: string;
  sensitive_content_included: boolean;
  included_sensitive_items: number;
  audit_event_public_id: string;
}

export const diagnosticPreview = (sensitive: DiagnosticSensitiveOptions) =>
  call<DiagnosticPreview>("diagnostic_preview", { request: { sensitive } });

export const diagnosticExport = (
  output_path: string,
  request_key: string,
  sensitive: DiagnosticSensitiveOptions,
  preview_token: string | null,
  confirm_sensitive_evidence: boolean,
) =>
  call<DiagnosticExportResult>("diagnostic_export", {
    request: {
      output_path,
      request_key,
      sensitive,
      preview_token,
      confirm_sensitive_evidence,
    },
  });
