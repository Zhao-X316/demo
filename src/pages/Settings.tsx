import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import {
  RecitationConfig,
  VolcanoCreds,
  MaskedVolcanoCreds,
  BackupCatalog,
  BackupInfo,
  backupCreate,
  backupRestore,
  backupsList,
  configGet,
  configSet,
  DiagnosticPreview,
  DiagnosticSensitiveOptions,
  diagnosticExport,
  diagnosticPreview,
  secretsGet,
  secretsSet,
} from "../api/settings";

export default function Settings() {
  const [cfg, setCfg] = useState<RecitationConfig | null>(null);
  const [creds, setCreds] = useState<VolcanoCreds | null>(null);
  const [credentialStatus, setCredentialStatus] = useState<MaskedVolcanoCreds | null>(null);
  const [toast, setToast] = useState("");
  const [err, setErr] = useState("");
  const [backups, setBackups] = useState<BackupCatalog | null>(null);
  const [backupBusy, setBackupBusy] = useState(false);
  const [diagnosticBusy, setDiagnosticBusy] = useState(false);
  const [diagnosticOptions, setDiagnosticOptions] = useState<DiagnosticSensitiveOptions>({
    include_failed_audio: false,
    include_failed_asr: false,
    include_standard_answers: false,
  });
  const [diagnosticPreviewResult, setDiagnosticPreviewResult] = useState<DiagnosticPreview | null>(null);

  useEffect(() => {
    configGet().then(setCfg).catch((e) => setErr(String(e)));
    secretsGet()
      .then((view) => {
        setCredentialStatus(view);
        setCreds({ app_id: view.app_id, access_token: "", secret: "", cluster: view.cluster });
      })
      .catch(() => setCreds({ app_id: "", access_token: "", secret: "", cluster: "" }));
    backupsList().then(setBackups).catch((e) => setErr(String(e)));
  }, []);

  const saveCfg = async () => {
    if (!cfg) return;
    try {
      await configSet(cfg);
      setToast("评分参数已保存");
    } catch (e) {
      setErr(String(e));
    }
  };
  const saveCreds = async () => {
    if (!creds) return;
    try {
      await secretsSet(creds);
      const view = await secretsGet();
      setCredentialStatus(view);
      setCreds({ app_id: view.app_id, access_token: "", secret: "", cluster: view.cluster });
      setToast("凭据已保存（仅本机；敏感值不会回传页面）");
    } catch (e) {
      setErr(String(e));
    }
  };
  const refreshBackups = async () => setBackups(await backupsList());
  const createBackup = async () => {
    setBackupBusy(true);
    setErr("");
    try {
      await backupCreate();
      await refreshBackups();
      setToast("数据库备份已创建并通过完整性检查");
    } catch (e) {
      setErr(String(e));
    }
    setBackupBusy(false);
  };
  const restoreBackup = async (item: BackupInfo) => {
    const confirmed = window.confirm(
      `确定恢复备份 ${item.file_name}？\n\n系统会先自动备份当前数据库；恢复失败会自动回滚。`,
    );
    if (!confirmed) return;
    setBackupBusy(true);
    setErr("");
    try {
      const protective = await backupRestore(item.file_name);
      setCfg(await configGet());
      await refreshBackups();
      setToast(`恢复完成；恢复前快照已保留为 ${protective.file_name}`);
    } catch (e) {
      setErr(String(e));
    }
    setBackupBusy(false);
  };

  const backupKind = (kind: BackupInfo["kind"]) =>
    ({
      daily: "每日自动",
      "pre-migration": "迁移前",
      manual: "手动",
      "before-restore": "恢复前保护",
    })[kind];
  const backupTime = (value: string) => value.replace("T", " ").replace("+08:00", " 中国时间");
  const backupSize = (value: number) => `${(value / 1024 / 1024).toFixed(2)} MB`;
  const diagnosticSize = (value: number) =>
    value < 1024 * 1024 ? `${Math.max(1, Math.ceil(value / 1024))} KB` : `${(value / 1024 / 1024).toFixed(2)} MB`;
  const diagnosticRequestKey = () =>
    `diagnostic-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  const diagnosticDefaultName = () => {
    const stamp = new Date().toISOString().replace(/[-:]/g, "").replace(/\..+/, "").replace("T", "-");
    return `教辅系统诊断包-${stamp}.zip`;
  };
  const chooseDiagnosticPath = async () =>
    save({
      defaultPath: diagnosticDefaultName(),
      filters: [{ name: "诊断包", extensions: ["zip"] }],
    });
  const exportDefaultDiagnostic = async () => {
    const outputPath = await chooseDiagnosticPath();
    if (!outputPath) return;
    setDiagnosticBusy(true);
    setErr("");
    try {
      const result = await diagnosticExport(
        outputPath,
        diagnosticRequestKey(),
        {
          include_failed_audio: false,
          include_failed_asr: false,
          include_standard_answers: false,
        },
        null,
        false,
      );
      setToast(`默认脱敏诊断包已导出：${result.file_name}`);
    } catch (e) {
      setErr(String(e));
    }
    setDiagnosticBusy(false);
  };
  const updateDiagnosticOption = (
    key: keyof DiagnosticSensitiveOptions,
    checked: boolean,
  ) => {
    setDiagnosticOptions((current) => ({ ...current, [key]: checked }));
    setDiagnosticPreviewResult(null);
  };
  const previewSensitiveDiagnostic = async () => {
    setDiagnosticBusy(true);
    setErr("");
    try {
      setDiagnosticPreviewResult(await diagnosticPreview(diagnosticOptions));
    } catch (e) {
      setErr(String(e));
    }
    setDiagnosticBusy(false);
  };
  const exportSensitiveDiagnostic = async () => {
    if (!diagnosticPreviewResult?.preview_token) return;
    const confirmed = window.confirm(
      `将导出 ${diagnosticPreviewResult.available_item_count} 项内容证据（约 ${diagnosticSize(diagnosticPreviewResult.total_available_bytes)}）。\n\n内容可能包含学生声音、背诵正文或标准答案。确认继续吗？`,
    );
    if (!confirmed) return;
    const outputPath = await chooseDiagnosticPath();
    if (!outputPath) return;
    setDiagnosticBusy(true);
    setErr("");
    try {
      const result = await diagnosticExport(
        outputPath,
        diagnosticRequestKey(),
        diagnosticOptions,
        diagnosticPreviewResult.preview_token,
        true,
      );
      setToast(`含 ${result.included_sensitive_items} 项内容证据的诊断包已导出：${result.file_name}`);
      setDiagnosticPreviewResult(null);
    } catch (e) {
      setErr(String(e));
    }
    setDiagnosticBusy(false);
  };

  if (!cfg) return <div className="page"><div className="loading">加载设置…</div></div>;

  return (
    <div className="page">
      <div className="page-head">
        <h1>设置</h1>
      </div>
      <div className="sub">评分参数 + 本地数据保护 + 云识别凭据。凭据只存本机，不入库、不打包。</div>
      {err && <div className="error">{err}</div>}
      {toast && <div className="ok-banner">{toast}</div>}

      <h2>评分参数</h2>
      <div className="form">
        <div className="field">
          <div className="fl">正确率门槛（%）—— 达到才算通过</div>
          <input
            type="number"
            value={cfg.accuracy_threshold}
            onChange={(e) => setCfg({ ...cfg, accuracy_threshold: Number(e.target.value) })}
            style={{ maxWidth: 140 }}
          />
        </div>
        <div className="field">
          <div className="fl">补背偏移（天）—— 1 = 次日补背</div>
          <input
            type="number"
            value={cfg.makeup_offset_days}
            onChange={(e) => setCfg({ ...cfg, makeup_offset_days: Number(e.target.value) })}
            style={{ maxWidth: 140 }}
          />
        </div>
        <label className="field check">
          <input
            type="checkbox"
            checked={cfg.use_pinyin}
            onChange={(e) => setCfg({ ...cfg, use_pinyin: e.target.checked })}
          />
          拼音兜底（同音也算对）
        </label>
        <label className="field check">
          <input
            type="checkbox"
            checked={cfg.ignore_tone}
            onChange={(e) => setCfg({ ...cfg, ignore_tone: e.target.checked })}
          />
          拼音忽略声调
        </label>
        <label className="field check">
          <input
            type="checkbox"
            checked={cfg.remove_fillers}
            onChange={(e) => setCfg({ ...cfg, remove_fillers: e.target.checked })}
          />
          剔除语气词（嗯、啊…）
        </label>
        <div>
          <button className="primary" onClick={saveCfg}>
            保存评分参数
          </button>
        </div>
      </div>

      <h2>本地数据保护</h2>
      <div className="form" style={{ maxWidth: 760 }}>
        <div className="sub">
          每天首次启动和数据库迁移前自动创建 SQLite 在线快照；备份只含 data.db，不含云凭据。
        </div>
        <div>
          <button className="primary" disabled={backupBusy} onClick={createBackup}>
            {backupBusy ? "处理中…" : "立即创建备份"}
          </button>
        </div>
        {backups?.older_retained ? (
          <div className="error">
            当前超过默认保留的 {backups.retention_limit} 份，另有 {backups.older_retained} 份旧备份仍保留；系统不会在未提示老师时自动删除。
          </div>
        ) : (
          <div className="sub">默认保留最近 {backups?.retention_limit ?? 14} 份；更早备份目前不会自动删除。</div>
        )}
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          {backups?.items.length ? backups.items.map((item) => (
            <div key={item.file_name} className="row" style={{ padding: "12px 14px" }}>
              <div>
                <div className="who">{backupKind(item.kind)}</div>
                <div className="meta">{backupTime(item.created_at)} · {backupSize(item.size_bytes)}</div>
              </div>
              <div className="spacer" />
              <button disabled={backupBusy} onClick={() => restoreBackup(item)}>恢复此备份</button>
            </div>
          )) : <div className="sub">暂无备份</div>}
        </div>
      </div>

      <h2>一键诊断</h2>
      <div className="form" style={{ maxWidth: 760 }}>
        <div className="sub">
          默认只导出版本、数据库完整性、迁移、备份、任务状态、内部 ID 和脱敏错误码；不包含姓名、绝对路径、录音、ASR、答案正文或云凭据。
        </div>
        <div>
          <button className="primary" disabled={diagnosticBusy} onClick={exportDefaultDiagnostic}>
            {diagnosticBusy ? "正在生成…" : "导出默认脱敏诊断包"}
          </button>
        </div>
        <details>
          <summary>高级：确需技术人员核对内容证据</summary>
          <div className="form" style={{ marginTop: 12 }}>
            <div className="error">
              只有排查内容识别问题时才选择。系统会先展示精确清单，必须再次确认后才会导出。
            </div>
            <label className="field check">
              <input
                type="checkbox"
                checked={diagnosticOptions.include_failed_audio}
                onChange={(event) => updateDiagnosticOption("include_failed_audio", event.target.checked)}
              />
              包含最近失败记录的原始录音
            </label>
            <label className="field check">
              <input
                type="checkbox"
                checked={diagnosticOptions.include_failed_asr}
                onChange={(event) => updateDiagnosticOption("include_failed_asr", event.target.checked)}
              />
              包含最近失败记录的完整 ASR
            </label>
            <label className="field check">
              <input
                type="checkbox"
                checked={diagnosticOptions.include_standard_answers}
                onChange={(event) => updateDiagnosticOption("include_standard_answers", event.target.checked)}
              />
              包含相关标准答案正文
            </label>
            <div>
              <button
                disabled={diagnosticBusy || !Object.values(diagnosticOptions).some(Boolean)}
                onClick={previewSensitiveDiagnostic}
              >
                先预览内容清单
              </button>
            </div>
            {diagnosticPreviewResult?.sensitive_content_selected && (
              <div className="panel" style={{ padding: 12 }}>
                <div className="who">
                  可导出 {diagnosticPreviewResult.available_item_count} 项 · 不可用 {diagnosticPreviewResult.unavailable_item_count} 项 · 约 {diagnosticSize(diagnosticPreviewResult.total_available_bytes)}
                </div>
                <div className="meta">{diagnosticPreviewResult.warning}</div>
                <div style={{ display: "flex", flexDirection: "column", gap: 6, marginTop: 10 }}>
                  {diagnosticPreviewResult.items.map((item) => (
                    <div className="row" style={{ padding: "8px 10px" }} key={`${item.category}-${item.internal_ref}-${item.archive_name}`}>
                      <div>
                        <div className="who">{item.category} · {item.internal_ref}</div>
                        <div className="meta">
                          {item.archive_name} · {item.available ? diagnosticSize(item.size_bytes) : item.reason_code}
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
                <div style={{ marginTop: 10 }}>
                  <button
                    className="primary"
                    disabled={diagnosticBusy || !diagnosticPreviewResult.available_item_count}
                    onClick={exportSensitiveDiagnostic}
                  >
                    二次确认并导出所列内容
                  </button>
                </div>
              </div>
            )}
          </div>
        </details>
      </div>

      <h2>火山 ASR 凭据（本机）</h2>
      {creds && (
        <div className="form">
          <div className="field">
            <div className="fl">App ID</div>
            <input type="text" value={creds.app_id} onChange={(e) => setCreds({ ...creds, app_id: e.target.value })} />
          </div>
          <div className="field">
            <div className="fl">Access Token</div>
            <input
              type="password"
              value={creds.access_token}
              placeholder={credentialStatus?.access_token_mask ?? "尚未配置"}
              onChange={(e) => setCreds({ ...creds, access_token: e.target.value })}
            />
            {credentialStatus?.access_token_mask && (
              <div className="meta">已配置 {credentialStatus.access_token_mask}；留空保存表示不修改</div>
            )}
          </div>
          <div className="field">
            <div className="fl">Cluster</div>
            <input type="text" value={creds.cluster} onChange={(e) => setCreds({ ...creds, cluster: e.target.value })} />
          </div>
          <div>
            <button className="primary" onClick={saveCreds}>
              保存凭据
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
