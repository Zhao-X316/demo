import { useEffect, useState } from "react";
import {
  RecitationConfig,
  VolcanoCreds,
  BackupCatalog,
  BackupInfo,
  backupCreate,
  backupRestore,
  backupsList,
  configGet,
  configSet,
  secretsGet,
  secretsSet,
} from "../api/settings";

export default function Settings() {
  const [cfg, setCfg] = useState<RecitationConfig | null>(null);
  const [creds, setCreds] = useState<VolcanoCreds | null>(null);
  const [toast, setToast] = useState("");
  const [err, setErr] = useState("");
  const [backups, setBackups] = useState<BackupCatalog | null>(null);
  const [backupBusy, setBackupBusy] = useState(false);

  useEffect(() => {
    configGet().then(setCfg).catch((e) => setErr(String(e)));
    secretsGet().then(setCreds).catch(() => setCreds({ app_id: "", access_token: "", secret: "", cluster: "" }));
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
      setToast("凭据已保存（仅本机）");
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
              onChange={(e) => setCreds({ ...creds, access_token: e.target.value })}
            />
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
