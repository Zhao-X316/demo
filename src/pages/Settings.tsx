import { useEffect, useState } from "react";
import {
  configGet,
  configSet,
  secretsGet,
  secretsSet,
  type RecitationConfig,
  type VolcanoCreds,
} from "../api/settings";

export default function Settings() {
  const [cfg, setCfg] = useState<RecitationConfig | null>(null);
  const [creds, setCreds] = useState<VolcanoCreds | null>(null);
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");

  useEffect(() => {
    void (async () => {
      try {
        setCfg(await configGet());
        setCreds(await secretsGet());
      } catch (e) {
        setErr(String(e));
      }
    })();
  }, []);

  const saveCfg = async () => {
    if (!cfg) return;
    try {
      await configSet(cfg);
      flash("评分参数已保存");
    } catch (e) {
      setErr(String(e));
    }
  };
  const saveCreds = async () => {
    if (!creds) return;
    try {
      await secretsSet(creds);
      flash("火山凭据已保存到本地（secrets.json）");
    } catch (e) {
      setErr(String(e));
    }
  };
  const flash = (m: string) => {
    setMsg(m);
    setErr("");
    window.setTimeout(() => setMsg(""), 3000);
  };

  return (
    <div className="page">
      <h1>设置</h1>
      {err && <div className="error">出错：{err}</div>}
      {msg && <div className="ok-banner">{msg}</div>}

      <section className="group">
        <h2>火山 ASR 凭据</h2>
        <p className="muted">仅保存在本机 secrets.json（不入库、不上传、不进仓库）。</p>
        {creds && (
          <div className="form">
            <Field label="App ID" value={creds.app_id} onChange={(v) => setCreds({ ...creds, app_id: v })} />
            <Field label="Access Token" value={creds.access_token} onChange={(v) => setCreds({ ...creds, access_token: v })} type="password" />
            <Field label="Secret（可选）" value={creds.secret} onChange={(v) => setCreds({ ...creds, secret: v })} type="password" />
            <Field label="Cluster / 资源标识（可选）" value={creds.cluster} onChange={(v) => setCreds({ ...creds, cluster: v })} />
            <button className="primary" onClick={saveCreds}>保存凭据</button>
          </div>
        )}
      </section>

      <section className="group">
        <h2>评分参数</h2>
        {cfg && (
          <div className="form">
            <NumField label="正确率通过门槛 (%)" value={cfg.accuracy_threshold} onChange={(v) => setCfg({ ...cfg, accuracy_threshold: v })} />
            <NumField label="理想语速 (字/秒)" value={cfg.ideal_cps} onChange={(v) => setCfg({ ...cfg, ideal_cps: v })} step={0.5} />
            <NumField label="熟练度 A 档下限" value={cfg.quality_a_min} onChange={(v) => setCfg({ ...cfg, quality_a_min: v })} />
            <NumField label="熟练度 B 档下限" value={cfg.quality_b_min} onChange={(v) => setCfg({ ...cfg, quality_b_min: v })} />
            <NumField label="补背天数偏移 (1=次日)" value={cfg.makeup_offset_days} onChange={(v) => setCfg({ ...cfg, makeup_offset_days: v })} />
            <CheckField label="正确率拼音兜底（同音算对）" checked={cfg.use_pinyin} onChange={(v) => setCfg({ ...cfg, use_pinyin: v })} />
            <CheckField label="拼音忽略声调" checked={cfg.ignore_tone} onChange={(v) => setCfg({ ...cfg, ignore_tone: v })} />
            <CheckField label="归一化剔除语气词" checked={cfg.remove_fillers} onChange={(v) => setCfg({ ...cfg, remove_fillers: v })} />
            <button className="primary" onClick={saveCfg}>保存参数</button>
          </div>
        )}
      </section>
    </div>
  );
}

function Field(props: { label: string; value: string; onChange: (v: string) => void; type?: string }) {
  return (
    <label className="field">
      <span>{props.label}</span>
      <input type={props.type ?? "text"} value={props.value} onChange={(ev) => props.onChange(ev.target.value)} />
    </label>
  );
}

function NumField(props: { label: string; value: number; onChange: (v: number) => void; step?: number }) {
  return (
    <label className="field">
      <span>{props.label}</span>
      <input
        type="number"
        step={props.step ?? 1}
        value={props.value}
        onChange={(ev) => props.onChange(Number(ev.target.value))}
      />
    </label>
  );
}

function CheckField(props: { label: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="field check">
      <input type="checkbox" checked={props.checked} onChange={(ev) => props.onChange(ev.target.checked)} />
      <span>{props.label}</span>
    </label>
  );
}
