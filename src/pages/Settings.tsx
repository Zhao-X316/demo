import { useEffect, useState } from "react";
import {
  RecitationConfig,
  VolcanoCreds,
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

  useEffect(() => {
    configGet().then(setCfg).catch((e) => setErr(String(e)));
    secretsGet().then(setCreds).catch(() => setCreds({ app_id: "", access_token: "", secret: "", cluster: "" }));
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

  if (!cfg) return <div className="page"><div className="loading">加载设置…</div></div>;

  return (
    <div className="page">
      <div className="page-head">
        <h1>设置</h1>
      </div>
      <div className="sub">评分参数 + 云识别凭据。凭据只存本机，不入库、不打包。</div>
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
